//! The artifacts a check keeps in a [`Cache`](crate::Cache): the key each one is kept under and
//! its encoding.
//!
//! The artifact of a section file is its load: its section, where the file parses, and the
//! findings its read reports. Its key is the SHA-256 of the crate's version, the identity of the
//! build's code, the encoding's version, the file's path and its bytes, each after its length, so
//! that a change to any of them makes another key. Its encoding ends with the SHA-256 of the bytes
//! before it, and the check reads the file again where a cache returns bytes that don't decode to
//! a whole load, so a damaged entry doesn't change a report.
//!
//! Two artifacts follow from the set's loads. The result of checks 1 and 2 for the set's formulas
//! is kept under a key of the formulas and of the conditions' atoms, and only where the formulas
//! hold together: a conflict's minimal subset starts from the backend's core, and the conditions
//! that can't hold don't depend on the backend. The files a section generates are kept under a
//! key of each part of the section that a render reads and of the two paths the files go under.
//! Their keys hold the crate's version and the build's identity too, and each of their encodings
//! ends with its digest, as a section's load does.

#[cfg(test)]
mod build_identity;
mod sha256;

use toml::Value;

use crate::logic::{Formula, FormulaSet};
use crate::model::{Block, Field, Section};
use crate::render::{Generated, Paths, Section as Rendered};
use crate::{ByteVec, Finding, Key, Severity};

/// The crate's version, which each key holds, so that a new version builds each artifact again.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The identity of the code this build compiles, which each key holds: the SHA-256, in hex, that
/// build.rs computes over each file under src/ and the Cargo.lock beside Cargo.toml. A change to
/// the code that loads a section file, such as `model::load_section`, `source::text`,
/// `source::parse` or this encoding, or to the `toml` version in that lock, makes another
/// identity, so a build with such a change doesn't read an earlier build's entries from a cache.
/// A build of this crate as a dependency of another workspace resolves its dependencies with that
/// workspace's lock, which build.rs doesn't read.
const BUILD: &str = env!("PARTIMENTO_BUILD");

/// The version of the encoding, which each key holds. A change to this encoding, or to what a
/// section's load holds, makes another `BUILD` and so another key, and leaves this version as it
/// is.
const ENCODING: u32 = 1;

/// The first part of a section file's key, which no other kind of artifact's key starts with.
const SECTION: &[u8] = b"partimento section load";

/// The bytes of the digest that ends an encoding.
const DIGEST: usize = 32;

/// The first part of the key of checks 1 and 2's result for a formula set.
const CONSISTENCY: &[u8] = b"partimento consistency";

/// The version of the encoding of checks 1 and 2's result and of the formulas its key holds,
/// which the key holds too. A change to that encoding, or to what `checks::consistency` returns,
/// makes another `BUILD` and so another key, and leaves this version as it is.
const CONSISTENCY_ENCODING: u32 = 1;

/// The first part of the key of the files a section generates.
const RENDER: &[u8] = b"partimento section render";

/// The version of the encoding of a section's generated files, which their key holds. A change to
/// that encoding, or to the files `render::generate` writes, makes another `BUILD` and so another
/// key, and leaves this version as it is.
const RENDER_ENCODING: u32 = 1;

/// The key of a section file's load.
pub(crate) fn section_key(file: &str, bytes: &ByteVec) -> Key {
    let bytes = bytes.copy_to_bytes();
    load_key(VERSION, BUILD, ENCODING, file, &bytes)
}

fn load_key(version: &str, build: &str, encoding: u32, file: &str, bytes: &[u8]) -> Key {
    let encoding = encoding.to_le_bytes();
    let parts: [&[u8]; 6] = [
        SECTION,
        version.as_bytes(),
        build.as_bytes(),
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

/// The key of checks 1 and 2's result for `set`: its formulas in order and its conditions' atoms
/// in order. It leaves out where each formula and condition comes from, which doesn't change the
/// result, and the backend, since only a result without a conflict is kept.
pub(crate) fn consistency_key(set: &FormulaSet) -> Key {
    consistency_key_of(VERSION, BUILD, CONSISTENCY_ENCODING, &formulas(set))
}

fn consistency_key_of(version: &str, build: &str, encoding: u32, formulas: &[u8]) -> Key {
    let encoding = encoding.to_le_bytes();
    let parts: [&[u8]; 5] = [
        CONSISTENCY,
        version.as_bytes(),
        build.as_bytes(),
        &encoding,
        formulas,
    ];
    key(&parts)
}

/// The formulas of `set`, each in the prefix encoding that `Writer::formula` writes, and the
/// atoms of its conditions, each list after its count.
fn formulas(set: &FormulaSet) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.count(set.formulas.len());
    for entry in &set.formulas {
        writer.formula(&entry.formula);
    }
    writer.count(set.conditions.len());
    for condition in &set.conditions {
        writer.text(&condition.atom);
    }
    writer.bytes
}

/// The encoding of checks 1 and 2's result for a formula set whose formulas hold together: the
/// indexes of the conditions that can't hold.
pub(crate) fn encode_consistency(vacuous: &[usize]) -> ByteVec {
    let mut writer = Writer::default();
    writer.count(vacuous.len());
    for &index in vacuous {
        writer.count(index);
    }
    ByteVec::from(seal(writer.bytes))
}

/// The indexes of the conditions that can't hold that `bytes` encode, where they are a whole
/// encoding whose digest holds, each index is below `conditions` and each is above the one before
/// it.
pub(crate) fn decode_consistency(bytes: &ByteVec, conditions: usize) -> Option<Vec<usize>> {
    let bytes = bytes.copy_to_bytes();
    let body = unseal(&bytes)?;
    let mut reader = Reader { bytes: body };
    let vacuous = reader.list(Reader::count)?;
    let rising = vacuous.windows(2).all(|pair| pair[0] < pair[1]);
    let within = vacuous.iter().all(|&index| index < conditions);
    (reader.bytes.is_empty() && rising && within).then_some(vacuous)
}

/// The key of the files `section` generates: the two paths they go under, and each part of the
/// section that a render reads.
pub(crate) fn render_key(paths: Paths<'_>, section: &Rendered<'_>) -> Key {
    render_key_of(VERSION, BUILD, RENDER_ENCODING, paths, section)
}

fn render_key_of(
    version: &str,
    build: &str,
    encoding: u32,
    paths: Paths<'_>,
    section: &Rendered<'_>,
) -> Key {
    let encoding = encoding.to_le_bytes();
    let count = (section.requirements.len() as u64).to_le_bytes();
    let head: [&[u8]; 12] = [
        RENDER,
        version.as_bytes(),
        build.as_bytes(),
        &encoding,
        paths.rendered.as_bytes(),
        paths.requirements.as_bytes(),
        section.rfc.as_bytes(),
        section.name.as_bytes(),
        section.anchor.as_bytes(),
        section.title.as_bytes(),
        section.prose.as_bytes(),
        &count,
    ];
    let mut parts = head.to_vec();
    for requirement in &section.requirements {
        parts.push(requirement.id.as_bytes());
        parts.push(requirement.level.as_bytes());
        parts.push(requirement.statement.as_bytes());
    }
    key(&parts)
}

/// The encoding of the files a section generates: each file's path and bytes, in the order of
/// its path, and the identifier of each requirement file among them.
pub(crate) fn encode_render(generated: &Generated) -> ByteVec {
    let mut writer = Writer::default();
    writer.count(generated.tree.files.len());
    for (path, bytes) in &generated.tree.files {
        writer.text(path);
        writer.blob(&bytes.copy_to_bytes());
        let requirement = generated.requirements.get(path);
        writer.boolean(requirement.is_some());
        if let Some(requirement) = requirement {
            writer.text(requirement);
        }
    }
    ByteVec::from(seal(writer.bytes))
}

/// The files that `bytes` encode, where they are a whole encoding whose digest holds.
pub(crate) fn decode_render(bytes: &ByteVec) -> Option<Generated> {
    let bytes = bytes.copy_to_bytes();
    let body = unseal(&bytes)?;
    let mut reader = Reader { bytes: body };
    let mut generated = Generated::default();
    for _ in 0..reader.count()? {
        let path = reader.text()?;
        let file = ByteVec::from(reader.blob()?);
        if reader.boolean()? {
            let requirement = reader.text()?;
            generated.requirements.insert(path.clone(), requirement);
        }
        generated.tree.files.insert(path, file);
    }
    reader.bytes.is_empty().then_some(generated)
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

    /// Bytes after their length in 8 bytes.
    fn blob(&mut self, bytes: &[u8]) {
        self.count(bytes.len());
        self.bytes.extend_from_slice(bytes);
    }

    /// A formula in prefix order: each node a byte of its kind, then an atom's name or a
    /// conjunction's or a disjunction's count of parts, then its parts. The walk keeps the nodes
    /// left to write in a list, so a deeply nested formula doesn't grow the thread's stack.
    fn formula(&mut self, formula: &Formula) {
        let mut pending = vec![formula];
        while let Some(formula) = pending.pop() {
            match formula {
                Formula::Atom(name) => {
                    self.byte(0);
                    self.text(name);
                }
                Formula::Not(inner) => {
                    self.byte(1);
                    pending.push(inner);
                }
                Formula::And(parts) => {
                    self.byte(2);
                    self.count(parts.len());
                    pending.extend(parts.iter().rev());
                }
                Formula::Or(parts) => {
                    self.byte(3);
                    self.count(parts.len());
                    pending.extend(parts.iter().rev());
                }
                Formula::Implies(a, b) => {
                    self.byte(4);
                    pending.push(b);
                    pending.push(a);
                }
                Formula::Iff(a, b) => {
                    self.byte(5);
                    pending.push(b);
                    pending.push(a);
                }
            }
        }
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

    fn blob(&mut self) -> Option<Vec<u8>> {
        let count = self.count()?;
        Some(self.take(count)?.to_vec())
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
    use std::collections::BTreeSet;

    use super::*;
    use crate::logic::{parse, Condition, Entry, Origin};
    use crate::render::{generate, Requirement};

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
        let key = load_key(VERSION, BUILD, ENCODING, file, bytes);
        assert_ne!(load_key("0.0.0-other", BUILD, ENCODING, file, bytes), key);
        assert_ne!(load_key(VERSION, "other build", ENCODING, file, bytes), key);
        assert_ne!(load_key(VERSION, BUILD, ENCODING + 1, file, bytes), key);
        assert_ne!(
            load_key(VERSION, BUILD, ENCODING, "rfcs/b.toml", bytes),
            key
        );
        assert_ne!(load_key(VERSION, BUILD, ENCODING, file, b"id = 2\n"), key);
        let text = ByteVec::from(bytes.to_vec());
        assert_eq!(section_key(file, &text), key);
    }

    #[test]
    fn the_build_identity_is_a_sha256_in_hex() {
        assert_eq!(BUILD.len(), 64);
        assert!(BUILD.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    fn build_file(path: &str, bytes: &[u8]) -> (String, Vec<u8>) {
        (path.to_string(), bytes.to_vec())
    }

    #[test]
    fn each_file_of_a_build_changes_its_identity() {
        let lib = build_file("src/lib.rs", b"mod a;\n");
        let lock = build_file("Cargo.lock", b"toml 0.8.0\n");
        let identity = build_identity::identity(&[lib.clone(), lock.clone()]);
        let changed = [
            vec![build_file("src/lib.rs", b"mod b;\n"), lock.clone()],
            vec![build_file("src/main.rs", b"mod a;\n"), lock.clone()],
            vec![lib.clone(), build_file("Cargo.lock", b"toml 0.9.0\n")],
            vec![build_file("src/lib.r", b"smod a;\n"), lock],
            vec![lib],
        ];
        for files in &changed {
            assert_ne!(build_identity::identity(files), identity, "{files:?}");
        }
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn the_build_identity_is_the_identity_of_the_package_files() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let files = build_identity::package_files(root).expect("the package's files read");
        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert!(paths.contains(&"src/artifact/sha256.rs"), "{paths:?}");
        assert_eq!(paths.last(), Some(&"Cargo.lock"));
        assert!(paths[..paths.len() - 1].is_sorted(), "{paths:?}");
        assert_eq!(build_identity::identity(&files), BUILD);
    }

    #[test]
    fn a_part_keeps_its_bytes_apart_from_the_next() {
        let (ab, c) = (b"ab".as_slice(), b"c".as_slice());
        let (a, bc) = (b"a".as_slice(), b"bc".as_slice());
        assert_ne!(key(&[ab, c]), key(&[a, bc]));
    }

    /// A formula set of the formulas `texts` and of a condition for each of `atoms`.
    fn formula_set(texts: &[&str], atoms: &[&str]) -> FormulaSet {
        let origin = |line: u32| Origin {
            requirement: "EX-0001-001".to_owned(),
            file: "rfcs/a.toml".to_owned(),
            line,
        };
        let formulas = texts
            .iter()
            .map(|text| Entry {
                formula: parse(text).expect("the test's formula parses"),
                origin: origin(1),
            })
            .collect();
        let conditions = atoms
            .iter()
            .map(|atom| Condition {
                atom: (*atom).to_owned(),
                origin: origin(2),
            })
            .collect();
        FormulaSet {
            formulas,
            conditions,
        }
    }

    #[test]
    fn a_formula_encodes_its_shape() {
        fn atom(name: &str) -> Formula {
            Formula::atom(name)
        }
        let shapes = [
            Formula::And(vec![atom("a"), Formula::And(vec![atom("b"), atom("c")])]),
            Formula::And(vec![Formula::And(vec![atom("a"), atom("b")]), atom("c")]),
            Formula::Or(vec![atom("a"), Formula::And(vec![atom("b"), atom("c")])]),
            atom("a").implies(atom("b")),
            atom("b").implies(atom("a")),
            atom("a").iff(atom("b")),
            atom("a").not(),
            atom("ab"),
            Formula::And(Vec::new()),
            Formula::Or(Vec::new()),
        ];
        let encode = |formula: &Formula| {
            let mut writer = Writer::default();
            writer.formula(formula);
            writer.bytes
        };
        let encodings: BTreeSet<Vec<u8>> = shapes.iter().map(encode).collect();
        assert_eq!(encodings.len(), shapes.len());
    }

    #[test]
    fn each_input_of_a_consistency_key_changes_it() {
        let set = formula_set(&["a -> b", "!(a & c)"], &["a"]);
        let key = consistency_key(&set);
        let body = formulas(&set);
        assert_eq!(
            consistency_key_of(VERSION, BUILD, CONSISTENCY_ENCODING, &body),
            key
        );
        assert_ne!(
            consistency_key_of("0.0.0-other", BUILD, CONSISTENCY_ENCODING, &body),
            key
        );
        assert_ne!(
            consistency_key_of(VERSION, "other build", CONSISTENCY_ENCODING, &body),
            key
        );
        assert_ne!(
            consistency_key_of(VERSION, BUILD, CONSISTENCY_ENCODING + 1, &body),
            key
        );
        let others = [
            formula_set(&["a -> b"], &["a"]),
            formula_set(&["a -> b", "!(a & c)", "c"], &["a"]),
            formula_set(&["!(a & c)", "a -> b"], &["a"]),
            formula_set(&["a -> d", "!(a & c)"], &["a"]),
            formula_set(&["a <-> b", "!(a & c)"], &["a"]),
            formula_set(&["a -> b", "!(a | c)"], &["a"]),
            formula_set(&["a -> b", "!(a & c)"], &["b"]),
            formula_set(&["a -> b", "!(a & c)"], &["a", "a"]),
            formula_set(&["a -> b", "!(a & c)"], &[]),
        ];
        for other in &others {
            assert_ne!(consistency_key(other), key, "{other:?}");
        }
        // Where each formula and condition comes from doesn't change the result or the key.
        let mut moved = set.clone();
        moved.formulas[0].origin.line = 9;
        moved.conditions[0].origin.file = "rfcs/b.toml".to_owned();
        assert_eq!(consistency_key(&moved), key);
    }

    #[test]
    fn a_consistency_result_decodes_where_its_indexes_rise_and_fit() {
        let artifact = encode_consistency(&[0, 2]);
        assert_eq!(decode_consistency(&artifact, 3), Some(vec![0, 2]));
        assert_eq!(decode_consistency(&artifact, 2), None);
        assert_eq!(
            decode_consistency(&encode_consistency(&[]), 0),
            Some(Vec::new())
        );
        assert_eq!(decode_consistency(&encode_consistency(&[2, 0]), 3), None);
        assert_eq!(decode_consistency(&encode_consistency(&[1, 1]), 3), None);
        let bytes = artifact.copy_to_bytes().to_vec();
        for end in 0..bytes.len() {
            let cut = ByteVec::from(bytes[..end].to_vec());
            assert!(decode_consistency(&cut, 3).is_none(), "{end}");
        }
        let mut body = bytes[..bytes.len() - DIGEST].to_vec();
        body.push(0);
        assert!(decode_consistency(&ByteVec::from(seal(body)), 3).is_none());
    }

    const PATHS: Paths<'static> = Paths {
        rendered: "rendered",
        requirements: ".duvet/requirements",
    };

    fn rendered() -> Rendered<'static> {
        Rendered {
            rfc: "0001-greetings",
            name: "01-greeting",
            anchor: "greeting",
            title: "Greeting",
            prose: "A greeting.",
            requirements: vec![
                Requirement {
                    id: "EX-0001-001",
                    level: "MUST",
                    statement: "The program MUST greet.",
                },
                Requirement {
                    id: "EX-0001-002",
                    level: "SHOULD",
                    statement: "The program SHOULD wave.",
                },
            ],
        }
    }

    #[test]
    fn a_render_decodes_to_the_files_it_encodes() {
        let generated = generate(PATHS, &[rendered()]);
        assert_eq!(generated.tree.files.len(), 3);
        assert_eq!(generated.requirements.len(), 2);
        let artifact = encode_render(&generated);
        assert_eq!(decode_render(&artifact), Some(generated));
        let bytes = artifact.copy_to_bytes().to_vec();
        for end in 0..bytes.len() {
            let cut = ByteVec::from(bytes[..end].to_vec());
            assert!(decode_render(&cut).is_none(), "{end}");
        }
        let mut body = bytes[..bytes.len() - DIGEST].to_vec();
        body.push(0);
        assert!(decode_render(&ByteVec::from(seal(body))).is_none());
    }

    #[test]
    fn each_input_of_a_render_key_changes_it() {
        let section = rendered();
        let key = render_key(PATHS, &section);
        assert_eq!(
            render_key_of(VERSION, BUILD, RENDER_ENCODING, PATHS, &section),
            key
        );
        assert_ne!(
            render_key_of("0.0.0-other", BUILD, RENDER_ENCODING, PATHS, &section),
            key
        );
        assert_ne!(
            render_key_of(VERSION, "other build", RENDER_ENCODING, PATHS, &section),
            key
        );
        assert_ne!(
            render_key_of(VERSION, BUILD, RENDER_ENCODING + 1, PATHS, &section),
            key
        );
        let docs = Paths {
            rendered: "docs",
            ..PATHS
        };
        assert_ne!(render_key(docs, &section), key);
        let requirements = Paths {
            requirements: "requirements",
            ..PATHS
        };
        assert_ne!(render_key(requirements, &section), key);
        let changes: [fn(&mut Rendered<'static>); 9] = [
            |section| section.rfc = "0002-farewells",
            |section| section.name = "02-greeting",
            |section| section.anchor = "hello",
            |section| section.title = "Hello",
            |section| section.prose = "A hello.",
            |section| section.requirements[0].id = "EX-0001-003",
            |section| section.requirements[0].level = "SHOULD",
            |section| section.requirements[1].statement = "The program SHOULD bow.",
            |section| {
                section.requirements.pop();
            },
        ];
        for (index, change) in changes.iter().enumerate() {
            let mut other = rendered();
            change(&mut other);
            assert_ne!(render_key(PATHS, &other), key, "{index}");
        }
    }
}
