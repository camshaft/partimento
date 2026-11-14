//! The files of the set's RFCs: the index, each RFC's rfc.toml and its section files.
//!
//! An RFC's directory is `<rfcs>/<nnnn>-<slug>`, and each of its sections is a file
//! `<nn>-<slug>.toml` beside its rfc.toml.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Deserialize;
use toml::Spanned;

use crate::config::Config;
use crate::{artifact, source, ByteVec, Cache, Finding, NoCache, Tree};

/// The name of the RFC index in the RFCs' directory.
pub const INDEX: &str = "index.toml";

/// The name of an RFC's metadata in its directory.
pub const RFC: &str = "rfc.toml";

/// The rule of a file that is missing or named outside the set's layout.
pub const LAYOUT: &str = "layout";

/// The RFCs of a set, as its files hold them.
#[derive(Clone, Debug, Default)]
pub struct Set {
    /// The RFC index, where it parses.
    pub index: Option<Index>,
    /// Each RFC directory, in the order of its path.
    pub rfcs: Vec<Rfc>,
}

/// The RFC index: every RFC of the set by number, written or planned.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    #[serde(default)]
    pub rfc: Vec<IndexEntry>,
}

/// One RFC of the index.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub number: u32,
    pub title: String,
    pub area: String,
    pub state: State,
}

/// Whether an RFC of the index is written yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Planned,
    Written,
}

/// One RFC's directory.
#[derive(Clone, Debug)]
pub struct Rfc {
    /// The directory's path in the tree.
    pub dir: String,
    /// The path of its rfc.toml in the tree.
    pub meta_file: String,
    /// The RFC's number, from the directory's name.
    pub number: u32,
    /// Its rfc.toml, where it is there and parses.
    pub meta: Option<Meta>,
    /// Its section files that parse, in the order of their names.
    pub sections: Vec<Section>,
}

/// A value and the line of the file that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located<T> {
    pub line: u32,
    pub value: T,
}

/// An RFC's rfc.toml.
#[derive(Clone, Debug)]
pub struct Meta {
    pub number: u32,
    pub title: String,
    pub area: String,
    /// The RFC's abstract.
    pub summary: String,
    pub sources: Vec<Source>,
    /// The identifiers the RFC no longer holds, which no requirement uses again.
    pub retired: Vec<Located<String>>,
}

/// A source of an RFC: a design by reference and version, or an earlier file by path and commit.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub design: Option<String>,
    pub version: Option<u32>,
    pub path: Option<String>,
    pub commit: Option<String>,
}

/// One section file.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// The file's path in the tree.
    pub file: String,
    /// The line of its `[section]` table, or 1 where the file doesn't hold one.
    pub line: u32,
    /// The fields of its `[section]` table, in the order of their lines.
    pub fields: Vec<Field>,
    /// Its `[[requirement]]` blocks, in the order of the file.
    pub requirements: Vec<Block>,
}

/// One `[[requirement]]` block of a section file.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The line of the block's `[[requirement]]` header.
    pub line: u32,
    /// The block's fields, in the order of their lines.
    pub fields: Vec<Field>,
}

/// One field of a table, with the line of its key.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub key: String,
    pub line: u32,
    pub value: toml::Value,
}

impl Section {
    /// The section's field of that key.
    pub fn field(&self, key: &str) -> Option<&Field> {
        find(&self.fields, key)
    }
}

impl Block {
    /// The block's field of that key.
    pub fn field(&self, key: &str) -> Option<&Field> {
        find(&self.fields, key)
    }

    /// The block's field of that key, where it is a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.field(key)?.value.as_str()
    }
}

fn find<'a>(fields: &'a [Field], key: &str) -> Option<&'a Field> {
    fields.iter().find(|field| field.key == key)
}

type RawTable = BTreeMap<Spanned<String>, Spanned<toml::Value>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSection {
    #[serde(default)]
    section: Option<Spanned<RawTable>>,
    #[serde(default)]
    requirement: Vec<Spanned<RawTable>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMeta {
    number: Spanned<u32>,
    title: String,
    area: String,
    #[serde(rename = "abstract")]
    summary: String,
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    retired: Vec<Spanned<String>>,
}

/// Reads the RFCs from the tree, adding a finding for each file that is missing, misnamed or
/// doesn't parse.
pub fn load(tree: &Tree, config: &Config, findings: &mut Vec<Finding>) -> Set {
    load_cached(tree, config, &NoCache, findings)
}

/// Reads the RFCs from the tree as `load` does, and each section file's load from `cache` where
/// the cache keeps it, keeping there the load of each section file it reads from the tree.
pub(crate) fn load_cached<C: Cache>(
    tree: &Tree,
    config: &Config,
    cache: &C,
    findings: &mut Vec<Finding>,
) -> Set {
    let root = format!("{}/", config.paths.rfcs);
    let index = format!("{root}{INDEX}");
    let mut set = Set::default();

    match tree.files.get(&index) {
        Some(bytes) => {
            let text = source::text(&index, bytes, findings);
            set.index = text.and_then(|text| source::parse(&index, &text, findings));
        }
        None => {
            let message = "the RFC index is missing";
            findings.push(Finding::error(LAYOUT, &index, 1, None, message));
        }
    }

    let mut dirs: BTreeMap<&str, Vec<(&str, &ByteVec)>> = BTreeMap::new();
    for (path, bytes) in &tree.files {
        let Some(rest) = path.strip_prefix(&root) else {
            continue;
        };
        if rest == INDEX {
            continue;
        }
        match rest.split_once('/') {
            Some((dir, name)) if !name.contains('/') => {
                dirs.entry(dir).or_default().push((name, bytes));
            }
            _ => {
                let message = "the file isn't the RFC index or a file of an RFC's directory";
                findings.push(Finding::error(LAYOUT, path, 1, None, message));
            }
        }
    }

    for (dir, files) in dirs {
        let path = format!("{root}{dir}");
        let Some(number) = rfc_number(dir) else {
            let file = format!("{path}/{}", files[0].0);
            let message = format!("the RFC directory `{dir}` isn't named <nnnn>-<slug>");
            findings.push(Finding::error(LAYOUT, &file, 1, None, message));
            continue;
        };
        let meta_file = format!("{path}/{RFC}");
        let mut rfc = Rfc {
            dir: path,
            meta_file,
            number,
            meta: None,
            sections: Vec::new(),
        };
        let mut has_meta = false;
        for (name, bytes) in files {
            let file = format!("{}/{name}", rfc.dir);
            if name == RFC {
                has_meta = true;
                rfc.meta = load_meta(&file, bytes, number, findings);
            } else if is_section_name(name) {
                let section = cached_section(&file, bytes, cache, findings);
                rfc.sections.extend(section);
            } else {
                let message = "the file isn't rfc.toml or a section file named <nn>-<slug>.toml";
                findings.push(Finding::error(LAYOUT, &file, 1, None, message));
            }
        }
        if !has_meta {
            let message = "the RFC's directory doesn't hold its rfc.toml";
            findings.push(Finding::error(LAYOUT, &rfc.meta_file, 1, None, message));
        }
        set.rfcs.push(rfc);
    }
    set
}

/// The RFC number of a directory named `<nnnn>-<slug>`.
fn rfc_number(dir: &str) -> Option<u32> {
    let (digits, slug) = dir.split_once('-')?;
    if digits.len() != 4 || !all_digits(digits) || !is_slug(slug) {
        return None;
    }
    digits.parse().ok()
}

/// Whether a file's name is `<nn>-<slug>.toml`.
fn is_section_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".toml") else {
        return false;
    };
    let Some((digits, slug)) = stem.split_once('-') else {
        return false;
    };
    digits.len() == 2 && all_digits(digits) && is_slug(slug)
}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_slug(text: &str) -> bool {
    let allowed = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
    !text.is_empty() && text.bytes().all(allowed)
}

fn load_meta(
    file: &str,
    bytes: &ByteVec,
    number: u32,
    findings: &mut Vec<Finding>,
) -> Option<Meta> {
    let text = source::text(file, bytes, findings)?;
    let raw: RawMeta = source::parse(file, &text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let line = |span: Range<usize>| lines.line(span.start);
    let held = *raw.number.get_ref();
    if held != number {
        let at = line(raw.number.span());
        let message = format!("rfc.toml holds number {held}, and its directory is RFC {number}");
        findings.push(Finding::error(LAYOUT, file, at, None, message));
    }
    let retired = raw
        .retired
        .into_iter()
        .map(|id| Located {
            line: line(id.span()),
            value: id.into_inner(),
        })
        .collect();
    Some(Meta {
        number: held,
        title: raw.title,
        area: raw.area,
        summary: raw.summary,
        sources: raw.sources,
        retired,
    })
}

/// A section file's load: its section where the file parses, and the findings its read reports,
/// which it adds to `findings`. Where `cache` keeps artifacts, it reads the load from `cache`
/// where the cache holds one for the file's path and bytes, and otherwise reads the file and
/// keeps its load in `cache`. Where `cache` doesn't keep artifacts, such as `NoCache`, it reads
/// the file without building a key or an artifact.
fn cached_section<C: Cache>(
    file: &str,
    bytes: &ByteVec,
    cache: &C,
    findings: &mut Vec<Finding>,
) -> Option<Section> {
    if !cache.keeps() {
        return load_section(file, bytes, findings);
    }
    let key = artifact::section_key(file, bytes);
    let kept = cache.get(&key);
    if let Some((found, section)) = kept.as_ref().and_then(artifact::decode_section) {
        findings.extend(found);
        return section;
    }
    let mut found = Vec::new();
    let section = load_section(file, bytes, &mut found);
    cache.put(key, artifact::encode_section(&found, section.as_ref()));
    findings.extend(found);
    section
}

fn load_section(file: &str, bytes: &ByteVec, findings: &mut Vec<Finding>) -> Option<Section> {
    let text = source::text(file, bytes, findings)?;
    let raw: RawSection = source::parse(file, &text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let line = |span: Range<usize>| lines.line(span.start);
    let (header, fields) = match raw.section {
        Some(table) => (line(table.span()), read_fields(table.into_inner(), &lines)),
        None => (1, Vec::new()),
    };
    let requirements = raw
        .requirement
        .into_iter()
        .map(|block| Block {
            line: line(block.span()),
            fields: read_fields(block.into_inner(), &lines),
        })
        .collect();
    Some(Section {
        file: file.to_string(),
        line: header,
        fields,
        requirements,
    })
}

fn read_fields(table: RawTable, lines: &source::Lines) -> Vec<Field> {
    let mut fields: Vec<Field> = table
        .into_iter()
        .map(|(key, value)| Field {
            line: lines.line(key.span().start),
            key: key.into_inner(),
            value: value.into_inner(),
        })
        .collect();
    fields.sort_by(|a, b| (a.line, &a.key).cmp(&(b.line, &b.key)));
    fields
}

#[cfg(test)]
mod tests {
    use super::{load, Field, Set, LAYOUT};
    use crate::{config, ByteVec, Finding, Tree};

    /// The configuration of the tests' set.
    const SPEC: &str = r#"prefix = "EX"
levels = ["MUST"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = "requirements"
"#;

    /// An rfc.toml whose number, on line 3, isn't its directory's, and whose retired identifiers
    /// are on lines 6 and 7.
    const META: &str = r#"title = "Greetings"
area = "core"
number = 2
abstract = "How to greet."
retired = [
    "EX-0001-003",
    "EX-0001-004",
]
"#;

    /// A section file whose `[section]` header is on line 1 and whose `[[requirement]]` headers
    /// are on lines 5 and 9, with each key on a line of its own.
    const SECTION: &str = r#"[section]
anchor = "greeting"
title = "Greeting"

[[requirement]]
id = "EX-0001-001"
level = "MUST"

[[requirement]]
statement = "The greeter MUST greet."
id = "EX-0001-002"
"#;

    /// The set that the tests' files hold, with the findings of their reads.
    fn load_set(findings: &mut Vec<Finding>) -> Set {
        let files = [
            ("spec.toml", SPEC),
            ("rfcs/index.toml", ""),
            ("rfcs/0001-greetings/rfc.toml", META),
            ("rfcs/0001-greetings/01-greeting.toml", SECTION),
        ];
        let mut tree = Tree::default();
        for (path, text) in files {
            let bytes = ByteVec::from(text.to_string());
            tree.files.insert(path.to_string(), bytes);
        }
        let loaded = config::load(&tree, findings);
        let config = loaded.expect("spec.toml holds a valid configuration");
        load(&tree, &config, findings)
    }

    /// The key and the line of each field.
    fn keys(fields: &[Field]) -> Vec<(&str, u32)> {
        fields
            .iter()
            .map(|field| (field.key.as_str(), field.line))
            .collect()
    }

    #[test]
    fn a_section_holds_the_line_of_each_header_and_key() {
        let set = load_set(&mut Vec::new());
        let section = &set.rfcs[0].sections[0];
        assert_eq!(section.line, 1);
        assert_eq!(keys(&section.fields), [("anchor", 2), ("title", 3)]);
        let blocks = &section.requirements;
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].line, 5);
        assert_eq!(keys(&blocks[0].fields), [("id", 6), ("level", 7)]);
        assert_eq!(blocks[1].line, 9);
        assert_eq!(keys(&blocks[1].fields), [("statement", 10), ("id", 11)]);
    }

    #[test]
    fn an_rfc_toml_holds_the_line_of_its_number_and_each_retired_identifier() {
        let mut findings = Vec::new();
        let set = load_set(&mut findings);
        let file = "rfcs/0001-greetings/rfc.toml";
        let message = "rfc.toml holds number 2, and its directory is RFC 1";
        assert_eq!(findings, [Finding::error(LAYOUT, file, 3, None, message)]);
        let meta = set.rfcs[0].meta.as_ref().expect("rfc.toml parses");
        let lines: Vec<u32> = meta.retired.iter().map(|id| id.line).collect();
        assert_eq!(lines, [6, 7]);
    }
}
