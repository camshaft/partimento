//! The artifacts a check keeps in a [`Cache`](crate::Cache): the key each one is kept under and
//! its encoding.
//!
//! The artifact of a section file is its load: its section, where the file parses, and the
//! findings its read reports. Its key is the SHA-256 of the crate's version, the encoding's
//! version, the file's path and its bytes, each after its length, so that a change to any of them
//! makes another key. Its encoding ends with the SHA-256 of the bytes before it, and the check
//! reads the file again where a cache returns bytes that don't decode to a whole load, so a
//! damaged entry doesn't change a report.

mod sha256;

use toml::Value;

use crate::model::{Block, Field, Section};
use crate::{ByteVec, Finding, Key, Severity};

/// The crate's version, which each key holds, so that a new version builds each artifact again.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version of the encoding, which each key holds.
const ENCODING: u32 = 1;

/// The first part of a section file's key, which no other kind of artifact's key starts with.
const SECTION: &[u8] = b"partimento section load";

/// The bytes of the digest that ends an encoding.
const DIGEST: usize = 32;

/// The key of a section file's load.
pub(crate) fn section_key(file: &str, bytes: &ByteVec) -> Key {
    let bytes = bytes.copy_to_bytes();
    load_key(VERSION, ENCODING, file, &bytes)
}

fn load_key(version: &str, encoding: u32, file: &str, bytes: &[u8]) -> Key {
    let encoding = encoding.to_le_bytes();
    let parts: [&[u8]; 5] = [
        SECTION,
        version.as_bytes(),
        &encoding,
        file.as_bytes(),
        bytes,
    ];
    key(&parts)
}

/// The SHA-256 of the parts, each after its length in 8 bytes, so that bytes moved from one part
/// to the next make another key.
fn key(parts: &[&[u8]]) -> Key {
    let mut hasher = sha256::Sha256::default();
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    Key(hasher.finish())
}

/// The encoding of a section file's load: the findings its read reports, and its section where
/// the file parses.
pub(crate) fn encode_section(findings: &[Finding], section: Option<&Section>) -> ByteVec {
    let mut writer = Writer::default();
    writer.count(findings.len());
    for finding in findings {
        writer.finding(finding);
    }
    writer.boolean(section.is_some());
    if let Some(section) = section {
        writer.section(section);
    }
    ByteVec::from(seal(writer.bytes))
}

/// The findings and the section that `bytes` encode, where they are a whole encoding whose
/// digest holds.
pub(crate) fn decode_section(bytes: &ByteVec) -> Option<(Vec<Finding>, Option<Section>)> {
    let bytes = bytes.copy_to_bytes();
    let body = unseal(&bytes)?;
    let mut reader = Reader { bytes: body };
    let findings = reader.list(Reader::finding)?;
    let section = if reader.boolean()? {
        Some(reader.section()?)
    } else {
        None
    };
    reader.bytes.is_empty().then_some((findings, section))
}

/// The body followed by its digest.
fn seal(mut body: Vec<u8>) -> Vec<u8> {
    let digest = sha256::digest(&body);
    body.extend_from_slice(&digest);
    body
}

/// The body of `bytes`, where they end with its digest.
fn unseal(bytes: &[u8]) -> Option<&[u8]> {
    let end = bytes.len().checked_sub(DIGEST)?;
    let (body, digest) = bytes.split_at(end);
    (sha256::digest(body).as_slice() == digest).then_some(body)
}

/// An encoding as it is written: each number in little-endian order, each list and text after
/// its length in 8 bytes, and each choice as a byte.
#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn byte(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    fn boolean(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn count(&mut self, count: usize) {
        self.u64(count as u64);
    }

    fn text(&mut self, text: &str) {
        self.count(text.len());
        self.bytes.extend_from_slice(text.as_bytes());
    }

    fn finding(&mut self, finding: &Finding) {
        let severity = match finding.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
        };
        self.byte(severity);
        self.text(&finding.rule);
        self.text(&finding.file);
        self.u32(finding.line);
        self.boolean(finding.requirement.is_some());
        if let Some(requirement) = &finding.requirement {
            self.text(requirement);
        }
        self.text(&finding.message);
    }

    fn section(&mut self, section: &Section) {
        self.text(&section.file);
        self.u32(section.line);
        self.fields(&section.fields);
        self.count(section.requirements.len());
        for block in &section.requirements {
            self.u32(block.line);
            self.fields(&block.fields);
        }
    }

    fn fields(&mut self, fields: &[Field]) {
        self.count(fields.len());
        for field in fields {
            self.text(&field.key);
            self.u32(field.line);
            self.value(&field.value);
        }
    }

    /// A value after a byte of its kind; a float as its bits, so that it decodes to the same bits,
    /// and a date-time as its text.
    fn value(&mut self, value: &Value) {
        match value {
            Value::String(text) => {
                self.byte(0);
                self.text(text);
            }
            Value::Integer(integer) => {
                self.byte(1);
                self.bytes.extend_from_slice(&integer.to_le_bytes());
            }
            Value::Float(float) => {
                self.byte(2);
                self.u64(float.to_bits());
            }
            Value::Boolean(boolean) => {
                self.byte(3);
                self.boolean(*boolean);
            }
            Value::Datetime(datetime) => {
                self.byte(4);
                self.text(&datetime.to_string());
            }
            Value::Array(items) => {
                self.byte(5);
                self.count(items.len());
                for item in items {
                    self.value(item);
                }
            }
            Value::Table(table) => {
                self.byte(6);
                self.count(table.len());
                for (key, value) in table {
                    self.text(key);
                    self.value(value);
                }
            }
        }
    }
}

/// An encoding as it is read, from its first byte: each read returns `None` where the bytes
/// left don't hold what it reads.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if count > self.bytes.len() {
            return None;
        }
        let (taken, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Some(taken)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn boolean(&mut self) -> Option<bool> {
        match self.byte()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_le_bytes)
    }

    fn count(&mut self) -> Option<usize> {
        usize::try_from(self.u64()?).ok()
    }

    fn text(&mut self) -> Option<String> {
        let count = self.count()?;
        let bytes = self.take(count)?;
        String::from_utf8(bytes.to_vec()).ok()
    }

    /// A count, and as many items as it says, each read by `read`.
    fn list<T>(&mut self, mut read: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let count = self.count()?;
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(read(self)?);
        }
        Some(items)
    }

    fn finding(&mut self) -> Option<Finding> {
        let severity = match self.byte()? {
            0 => Severity::Error,
            1 => Severity::Warning,
            _ => return None,
        };
        let rule = self.text()?;
        let file = self.text()?;
        let line = self.u32()?;
        let requirement = if self.boolean()? {
            Some(self.text()?)
        } else {
            None
        };
        let message = self.text()?;
        Some(Finding {
            severity,
            rule,
            file,
            line,
            requirement,
            message,
        })
    }

    fn section(&mut self) -> Option<Section> {
        let file = self.text()?;
        let line = self.u32()?;
        let fields = self.fields()?;
        let requirements = self.list(Self::block)?;
        Some(Section {
            file,
            line,
            fields,
            requirements,
        })
    }

    fn block(&mut self) -> Option<Block> {
        let line = self.u32()?;
        let fields = self.fields()?;
        Some(Block { line, fields })
    }

    fn fields(&mut self) -> Option<Vec<Field>> {
        self.list(Self::field)
    }

    fn field(&mut self) -> Option<Field> {
        let key = self.text()?;
        let line = self.u32()?;
        let value = self.value()?;
        Some(Field { key, line, value })
    }

    fn value(&mut self) -> Option<Value> {
        let value = match self.byte()? {
            0 => Value::String(self.text()?),
            1 => Value::Integer(i64::from_le_bytes(self.array()?)),
            2 => Value::Float(f64::from_bits(self.u64()?)),
            3 => Value::Boolean(self.boolean()?),
            4 => Value::Datetime(self.text()?.parse().ok()?),
            5 => Value::Array(self.list(Self::value)?),
            6 => Value::Table(self.list(Self::entry)?.into_iter().collect()),
            _ => return None,
        };
        Some(value)
    }

    fn entry(&mut self) -> Option<(String, Value)> {
        let key = self.text()?;
        let value = self.value()?;
        Some((key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value of each kind a TOML file holds.
    const VALUES: &str = r#"
string = "caf\u00e9 \"quoted\""
integer = -42
float = 2.5
infinity = -inf
boolean = true
offset_datetime = 1979-05-27T07:32:00.999999-07:00
local_datetime = 1979-05-27T07:32:00
local_date = 1979-05-27
local_time = 00:32:00.5
array = [1, "two", [3.0, false]]
table = { key = "value", nested = { list = [] } }
"#;

    /// A section whose `[section]` table and first block hold the fields of `values`, each at a
    /// line of its own, and whose second block holds no field.
    fn section(values: &str) -> Section {
        let table: toml::Table = toml::from_str(values).expect("the test's values parse");
        let fields = |first: u32| -> Vec<Field> {
            (first..)
                .zip(&table)
                .map(|(line, (key, value))| Field {
                    key: key.clone(),
                    line,
                    value: value.clone(),
                })
                .collect()
        };
        Section {
            file: "rfcs/0001-greetings/01-greeting.toml".to_string(),
            line: 1,
            fields: fields(2),
            requirements: vec![
                Block {
                    line: 20,
                    fields: fields(21),
                },
                Block {
                    line: 40,
                    fields: Vec::new(),
                },
            ],
        }
    }

    fn findings() -> Vec<Finding> {
        vec![
            Finding::error("parse", "rfcs/a.toml", 3, None, "an error"),
            Finding::warning("rule", "rfcs/b.toml", 4, Some("EX-0001-001"), "warned"),
        ]
    }

    fn decode(bytes: Vec<u8>) -> Option<(Vec<Finding>, Option<Section>)> {
        decode_section(&ByteVec::from(bytes))
    }

    #[test]
    fn a_load_decodes_to_the_load_it_encodes() {
        let expected = section(VALUES);
        let artifact = encode_section(&findings(), Some(&expected));
        let decoded = decode_section(&artifact);
        assert_eq!(decoded, Some((findings(), Some(expected))));
    }

    #[test]
    fn a_load_without_a_section_decodes() {
        let artifact = encode_section(&findings(), None);
        assert_eq!(decode_section(&artifact), Some((findings(), None)));
    }

    #[test]
    fn a_float_decodes_to_the_same_bits() {
        let expected = section("nan = nan\nzero = -0.0\n");
        let artifact = encode_section(&[], Some(&expected));
        let (_, decoded) = decode_section(&artifact).expect("the artifact decodes");
        let decoded = decoded.expect("the artifact holds a section");
        let bits = |section: &Section| -> Vec<u64> {
            let values = section.fields.iter().map(|field| &field.value);
            let floats = values.filter_map(Value::as_float);
            floats.map(f64::to_bits).collect()
        };
        assert_eq!(bits(&expected).len(), 2);
        assert_eq!(bits(&decoded), bits(&expected));
    }

    #[test]
    fn a_damaged_artifact_doesnt_decode() {
        let artifact = encode_section(&findings(), Some(&section(VALUES)));
        let bytes = artifact.copy_to_bytes().to_vec();
        for end in 0..bytes.len() {
            assert!(decode(bytes[..end].to_vec()).is_none(), "{end}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(decode(longer).is_none());
        for bit in 0..bytes.len() * 8 {
            let mut flipped = bytes.clone();
            flipped[bit / 8] ^= 1 << (bit % 8);
            assert!(decode(flipped).is_none(), "{bit}");
        }
    }

    #[test]
    fn a_sealed_body_that_isnt_a_whole_load_doesnt_decode() {
        let artifact = encode_section(&findings(), None);
        let bytes = artifact.copy_to_bytes();
        let mut body = bytes[..bytes.len() - DIGEST].to_vec();
        assert!(decode(seal(body.clone())).is_some());
        // The first finding's severity, after the count of the findings, is neither 0 nor 1.
        body[8] = 2;
        assert!(decode(seal(body.clone())).is_none());
        body[8] = 0;
        body.push(0);
        assert!(decode(seal(body)).is_none());
    }

    #[test]
    fn each_input_of_a_key_changes_it() {
        let (file, bytes) = ("rfcs/a.toml", b"id = 1\n".as_slice());
        let key = load_key(VERSION, ENCODING, file, bytes);
        assert_ne!(load_key("0.0.0-other", ENCODING, file, bytes), key);
        assert_ne!(load_key(VERSION, ENCODING + 1, file, bytes), key);
        assert_ne!(load_key(VERSION, ENCODING, "rfcs/b.toml", bytes), key);
        assert_ne!(load_key(VERSION, ENCODING, file, b"id = 2\n"), key);
        let text = ByteVec::from(bytes.to_vec());
        assert_eq!(section_key(file, &text), key);
    }

    #[test]
    fn a_part_keeps_its_bytes_apart_from_the_next() {
        let (ab, c) = (b"ab".as_slice(), b"c".as_slice());
        let (a, bc) = (b"a".as_slice(), b"bc".as_slice());
        assert_ne!(key(&[ab, c]), key(&[a, bc]));
    }
}
