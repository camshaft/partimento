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

/// The rule of an index row that still stores its RFC's state.
pub const INDEX_STATE: &str = "index-state";

/// The RFCs of a set, as its files hold them.
#[derive(Clone, Debug, Default)]
pub struct Set {
    /// The RFC index, where it parses.
    pub index: Option<Index>,
    /// Each RFC directory, in the order of its path.
    pub rfcs: Vec<Rfc>,
}

/// The RFC index: every RFC of the set by number. An RFC is written where the set holds its
/// directory, and planned where the index lists it and the set doesn't hold its directory.
#[derive(Clone, Debug, Default)]
pub struct Index {
    pub rfc: Vec<IndexEntry>,
}

/// One RFC of the index.
///
/// Other crates read its fields, and only this module builds it, as it loads the index, so a
/// field added to it doesn't break them:
///
/// ```compile_fail,E0639
/// let entry = partimento::model::IndexEntry {
///     number: 1,
///     line: 1,
///     title: String::new(),
///     title_line: 2,
///     area: String::new(),
///     area_line: 3,
///     state: None,
/// };
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct IndexEntry {
    pub number: u32,
    /// The line of the row's `number`.
    pub line: u32,
    pub title: String,
    /// The line of the row's `title`.
    pub title_line: u32,
    pub area: String,
    /// The line of the row's `area`.
    pub area_line: u32,
    /// The state the row still stores, where it stores one. The check doesn't read it, since
    /// the RFC's directory decides whether the RFC is written.
    pub state: Option<Located<State>>,
}

/// The state an index row may still store: whether its RFC is written yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Planned,
    Written,
}

/// One RFC's directory.
///
/// Other crates read its fields and build it only through [`Rfc::new`], so a field added to it
/// doesn't break them:
///
/// ```compile_fail,E0639
/// let rfc = partimento::model::Rfc {
///     dir: "rfcs/0001-greetings".to_owned(),
///     meta_file: "rfcs/0001-greetings/rfc.toml".to_owned(),
///     number: 1,
///     meta: None,
///     sections: Vec::new(),
/// };
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
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

impl Rfc {
    /// The RFC of number `number` whose directory is `dir`, before the directory's files are
    /// read: its rfc.toml's path is `<dir>/rfc.toml`, and it doesn't hold an rfc.toml or a
    /// section yet.
    pub fn new(dir: String, number: u32) -> Self {
        let meta_file = format!("{dir}/{RFC}");
        Self {
            dir,
            meta_file,
            number,
            meta: None,
            sections: Vec::new(),
        }
    }

    /// The RFC's sources: those its rfc.toml lists, where it parses, and then those each section
    /// file lists, in the order of the sections.
    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        let meta = self.meta.iter().flat_map(|meta| &meta.sources);
        meta.chain(self.sections.iter().flat_map(|section| &section.sources))
    }

    /// The identifiers the RFC retires, each with the path of the file that lists it: those its
    /// rfc.toml lists, where it parses, and then those each section file lists, in the order of
    /// the sections. An identifier that two of these files list comes once for each.
    pub fn retired(&self) -> impl Iterator<Item = (&str, &Located<String>)> {
        let file = self.meta_file.as_str();
        let meta = self.meta.iter().flat_map(|meta| &meta.retired);
        let meta = meta.map(move |id| (file, id));
        let sections = self.sections.iter().flat_map(|section| {
            let file = section.file.as_str();
            section.retired.iter().map(move |id| (file, id))
        });
        meta.chain(sections)
    }
}

/// A value and the line of the file that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located<T> {
    pub line: u32,
    pub value: T,
}

/// An RFC's rfc.toml.
///
/// Other crates read its fields, and only this module builds it, as it loads the RFC's
/// rfc.toml, so a field added to it doesn't break them:
///
/// ```compile_fail,E0639
/// let meta = partimento::model::Meta {
///     number: 1,
///     number_line: 1,
///     title: String::new(),
///     title_line: 2,
///     area: String::new(),
///     area_line: 3,
///     summary: String::new(),
///     sources: Vec::new(),
///     retired: Vec::new(),
///     contract_version: None,
/// };
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Meta {
    pub number: u32,
    /// The line of its `number`.
    pub number_line: u32,
    pub title: String,
    /// The line of its `title`.
    pub title_line: u32,
    pub area: String,
    /// The line of its `area`.
    pub area_line: u32,
    /// The RFC's abstract.
    pub summary: String,
    /// The sources rfc.toml lists; [`Rfc::sources`] reads them with each section file's.
    pub sources: Vec<Source>,
    /// The identifiers rfc.toml lists as ones the RFC no longer holds, which no requirement uses
    /// again; [`Rfc::retired`] reads them with each section file's.
    pub retired: Vec<Located<String>>,
    /// The version of the contract that a contract's RFC states, where its rfc.toml holds
    /// `contract_version`.
    pub contract_version: Option<Located<i64>>,
}

/// A source of an RFC: a design by reference and version, or an earlier file by path and commit.
///
/// Other crates read its fields, and only this module builds it, as it loads the RFC's
/// rfc.toml, so a field added to it doesn't break them:
///
/// ```compile_fail,E0639
/// let source = partimento::model::Source {
///     design: Some("designs/greetings".to_owned()),
///     version: Some(1),
///     path: None,
///     commit: None,
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Source {
    pub design: Option<String>,
    pub version: Option<u32>,
    pub path: Option<String>,
    pub commit: Option<String>,
}

/// One section file.
///
/// Other crates read its fields and build it only through [`Section::new`], so a field added
/// to it doesn't break them:
///
/// ```compile_fail,E0639
/// let section = partimento::model::Section {
///     file: "rfcs/0001-greetings/01-greeting.toml".to_owned(),
///     line: 1,
///     fields: Vec::new(),
///     requirements: Vec::new(),
///     kinds: Vec::new(),
///     sources: Vec::new(),
///     retired: Vec::new(),
/// };
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section {
    /// The file's path in the tree.
    pub file: String,
    /// The line of its `[section]` table, or 1 where the file doesn't hold one.
    pub line: u32,
    /// The fields of its `[section]` table, in the order of their lines.
    pub fields: Vec<Field>,
    /// Its `[[requirement]]` blocks, in the order of the file.
    pub requirements: Vec<Block>,
    /// Its `[[kind]]` tables, in the order of the file: each names a kind of self-describing
    /// identifier that the section specifies, by its `name`, and the number of each form of that
    /// kind's identifier, in its `formats`. `registry::check` checks their fields.
    pub kinds: Vec<Block>,
    /// The sources the file lists in its own `[[sources]]` tables, in the order of the file, each
    /// of the form of rfc.toml's; [`Rfc::sources`] reads them after rfc.toml's.
    pub sources: Vec<Source>,
    /// The identifiers the file's own `retired` list holds, each with its line, in the order of
    /// the file; [`Rfc::retired`] reads them after rfc.toml's.
    pub retired: Vec<Located<String>>,
}

/// One `[[requirement]]` block of a section file, or one of its `[[kind]]` tables.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The line of the block's `[[requirement]]` or `[[kind]]` header.
    pub line: u32,
    /// The block's fields, in the order of their lines.
    pub fields: Vec<Field>,
}

/// One field of a table, with the line of its key.
///
/// Other crates read its fields and build it only through [`Field::new`], so a field added to
/// it doesn't break them:
///
/// ```compile_fail,E0639
/// let field = partimento::model::Field {
///     key: "title".to_owned(),
///     line: 1,
///     value: toml::Value::Boolean(true),
/// };
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Field {
    pub key: String,
    pub line: u32,
    pub value: toml::Value,
}

impl Field {
    /// The field of key `key`, at line `line` of its table's file, that holds `value`.
    pub fn new(key: String, line: u32, value: toml::Value) -> Self {
        Self { key, line, value }
    }
}

impl Section {
    /// The section file at `file` whose `[section]` table is at line `line`, 1 where the file
    /// doesn't hold one, before its tables are read: it doesn't hold a field, a requirement, a kind
    /// table, a source or a retired identifier yet.
    pub fn new(file: String, line: u32) -> Self {
        Self {
            file,
            line,
            fields: Vec::new(),
            requirements: Vec::new(),
            kinds: Vec::new(),
            sources: Vec::new(),
            retired: Vec::new(),
        }
    }

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
    #[serde(default)]
    kind: Vec<Spanned<RawTable>>,
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    retired: Vec<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMeta {
    number: Spanned<u32>,
    title: Spanned<String>,
    area: Spanned<String>,
    #[serde(rename = "abstract")]
    summary: String,
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    retired: Vec<Spanned<String>>,
    #[serde(default)]
    contract_version: Option<Spanned<i64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIndex {
    #[serde(default)]
    rfc: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    number: Spanned<u32>,
    title: Spanned<String>,
    area: Spanned<String>,
    #[serde(default)]
    state: Option<Spanned<State>>,
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
            set.index = text.and_then(|text| load_index(&index, &text, findings));
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
        let mut rfc = Rfc::new(path, number);
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

/// The RFC index that `file` holds as `text`, with a warning at each row that still stores its
/// RFC's state.
fn load_index(file: &str, text: &str, findings: &mut Vec<Finding>) -> Option<Index> {
    let raw: RawIndex = source::parse(file, text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let mut index = Index::default();
    for entry in raw.rfc {
        let state = entry.state.map(|state| Located {
            line: lines.line(state.span().start),
            value: state.into_inner(),
        });
        let number = *entry.number.get_ref();
        if let Some(state) = &state {
            let message = format!(
                "RFC {number}'s state follows from its directory, so the row can leave out `state`"
            );
            findings.push(Finding::warning(
                INDEX_STATE,
                file,
                state.line,
                None,
                message,
            ));
        }
        index.rfc.push(IndexEntry {
            number,
            line: lines.line(entry.number.span().start),
            title_line: lines.line(entry.title.span().start),
            title: entry.title.into_inner(),
            area_line: lines.line(entry.area.span().start),
            area: entry.area.into_inner(),
            state,
        });
    }
    Some(index)
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
    let retired = located(raw.retired, &lines);
    let contract_version = raw.contract_version.map(|version| Located {
        line: line(version.span()),
        value: version.into_inner(),
    });
    Some(Meta {
        number: held,
        number_line: line(raw.number.span()),
        title_line: line(raw.title.span()),
        title: raw.title.into_inner(),
        area_line: line(raw.area.span()),
        area: raw.area.into_inner(),
        summary: raw.summary,
        sources: raw.sources,
        retired,
        contract_version,
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

/// A section file's section, where the file parses, and the findings its read reports, which it
/// adds to `findings`. A cache keeps what it returns under a key that holds `artifact::BUILD`, the
/// identity of this crate's code and of its Cargo.lock, so a change to this code, to what
/// `source::text`, `source::parse` and `read_fields` return, or to the `toml` crate's version in
/// that lock makes another key. The key holds the digest of this function's loads of
/// `artifact::PROBES` too, so where a build doesn't read that lock, a `toml` version that changes
/// one of those loads makes another key.
pub(crate) fn load_section(
    file: &str,
    bytes: &ByteVec,
    findings: &mut Vec<Finding>,
) -> Option<Section> {
    let text = source::text(file, bytes, findings)?;
    let raw: RawSection = source::parse(file, &text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let line = |span: Range<usize>| lines.line(span.start);
    let (header, fields) = match raw.section {
        Some(table) => (line(table.span()), read_fields(table.into_inner(), &lines)),
        None => (1, Vec::new()),
    };
    let blocks = |tables: Vec<Spanned<RawTable>>| -> Vec<Block> {
        let block = |table: Spanned<RawTable>| Block {
            line: line(table.span()),
            fields: read_fields(table.into_inner(), &lines),
        };
        tables.into_iter().map(block).collect()
    };
    Some(Section {
        file: file.to_string(),
        line: header,
        fields,
        requirements: blocks(raw.requirement),
        kinds: blocks(raw.kind),
        sources: raw.sources,
        retired: located(raw.retired, &lines),
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

/// Each identifier of a `retired` list, with the line that holds it.
fn located(ids: Vec<Spanned<String>>, lines: &source::Lines) -> Vec<Located<String>> {
    let one = |id: Spanned<String>| Located {
        line: lines.line(id.span().start),
        value: id.into_inner(),
    };
    ids.into_iter().map(one).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        load, load_index, load_section, Field, Located, Rfc, Section, Set, Source, State,
        INDEX_STATE, LAYOUT,
    };
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
        load_files(&files, findings)
    }

    /// The set that `files`, each a path and its text, hold, with the findings of their reads.
    fn load_files(files: &[(&str, &str)], findings: &mut Vec<Finding>) -> Set {
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

    /// An index whose first row doesn't store a state, and whose second stores one on line 10.
    const ROWS: &str = r#"[[rfc]]
number = 1
title = "Greetings"
area = "core"

[[rfc]]
number = 2
title = "Farewells"
area = "core"
state = "planned"
"#;

    #[test]
    fn an_index_row_parses_with_and_without_a_stored_state() {
        let mut findings = Vec::new();
        let loaded = load_index("rfcs/index.toml", ROWS, &mut findings);
        let index = loaded.expect("the index parses");
        let states: Vec<_> = index.rfc.iter().map(|entry| entry.state.clone()).collect();
        let stored = Located {
            line: 10,
            value: State::Planned,
        };
        assert_eq!(states, [None, Some(stored)]);
        let message = "RFC 2's state follows from its directory, so the row can leave out `state`";
        let warning = Finding::warning(INDEX_STATE, "rfcs/index.toml", 10, None, message);
        assert_eq!(findings, [warning]);
    }

    #[test]
    fn a_new_rfc_holds_its_rfc_toml_path_and_no_file_read() {
        let rfc = Rfc::new("rfcs/0001-greetings".to_owned(), 1);
        assert_eq!(rfc.dir, "rfcs/0001-greetings");
        assert_eq!(rfc.meta_file, "rfcs/0001-greetings/rfc.toml");
        assert_eq!(rfc.number, 1);
        assert!(rfc.meta.is_none());
        assert!(rfc.sections.is_empty());
    }

    #[test]
    fn a_new_section_holds_its_file_and_line_and_no_table() {
        let file = "rfcs/0001-greetings/01-greeting.toml";
        let section = Section::new(file.to_owned(), 3);
        assert_eq!(section.file, file);
        assert_eq!(section.line, 3);
        assert!(section.fields.is_empty());
        assert!(section.requirements.is_empty());
        assert!(section.kinds.is_empty());
        assert!(section.sources.is_empty());
        assert!(section.retired.is_empty());
    }

    #[test]
    fn a_new_field_holds_its_key_line_and_value() {
        let field = Field::new("title".to_owned(), 2, toml::Value::Boolean(true));
        assert_eq!(field.key, "title");
        assert_eq!(field.line, 2);
        assert_eq!(field.value, toml::Value::Boolean(true));
    }

    /// The commit of the earlier file that `FIRST_LISTING` lists as its source.
    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// An rfc.toml of RFC 1 that retires `EX-0001-005` on line 4 and lists a design as its source.
    const LISTING_META: &str = r#"number = 1
title = "Greetings"
area = "core"
retired = ["EX-0001-005"]
abstract = "How to greet."

[[sources]]
design = "designs/greetings"
version = 3
"#;

    /// A section file whose `retired` list, on line 1, holds `EX-0001-006`, and which lists an
    /// earlier file as its source on lines 3 to 5, before its `[section]` table on line 7.
    const FIRST_LISTING: &str = r#"retired = ["EX-0001-006"]

[[sources]]
path = "drafts/greetings.md"
commit = "0123456789abcdef0123456789abcdef01234567"

[section]
anchor = "greeting"
title = "Greeting"
"#;

    /// A section file whose `retired` list holds `EX-0001-007` on line 2, and which lists a
    /// design as its source after its `[section]` table.
    const SECOND_LISTING: &str = r#"retired = [
    "EX-0001-007",
]

[section]
anchor = "farewell"
title = "Farewell"

[[sources]]
design = "designs/farewells"
version = 1
"#;

    /// The files of a set whose rfc.toml and two section files each list one source and retire
    /// one identifier.
    fn listing_files() -> [(&'static str, &'static str); 5] {
        [
            ("spec.toml", SPEC),
            ("rfcs/index.toml", ""),
            ("rfcs/0001-greetings/rfc.toml", LISTING_META),
            ("rfcs/0001-greetings/01-greeting.toml", FIRST_LISTING),
            ("rfcs/0001-greetings/02-farewell.toml", SECOND_LISTING),
        ]
    }

    /// The source of a design at a version.
    fn design(design: &str, version: u32) -> Source {
        Source {
            design: Some(design.to_owned()),
            version: Some(version),
            path: None,
            commit: None,
        }
    }

    /// The source that `FIRST_LISTING` lists: an earlier file at a commit.
    fn draft() -> Source {
        Source {
            design: None,
            version: None,
            path: Some("drafts/greetings.md".to_owned()),
            commit: Some(COMMIT.to_owned()),
        }
    }

    /// The identifier `value` that a `retired` list holds on line `line`.
    fn retired(line: u32, value: &str) -> Located<String> {
        Located {
            line,
            value: value.to_owned(),
        }
    }

    #[test]
    fn a_section_file_holds_its_own_sources_and_retired_identifiers_with_their_lines() {
        let mut findings = Vec::new();
        let set = load_files(&listing_files(), &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
        let sections = &set.rfcs[0].sections;
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].line, 7);
        assert_eq!(keys(&sections[0].fields), [("anchor", 8), ("title", 9)]);
        assert_eq!(sections[0].sources, [draft()]);
        assert_eq!(sections[0].retired, [retired(1, "EX-0001-006")]);
        assert_eq!(sections[1].sources, [design("designs/farewells", 1)]);
        assert_eq!(sections[1].retired, [retired(2, "EX-0001-007")]);
    }

    #[test]
    fn an_rfc_reads_its_rfc_toml_and_then_each_section_file_in_order() {
        let set = load_files(&listing_files(), &mut Vec::new());
        let rfc = &set.rfcs[0];
        let sources: Vec<&Source> = rfc.sources().collect();
        let expected = [
            design("designs/greetings", 3),
            draft(),
            design("designs/farewells", 1),
        ];
        assert_eq!(sources, expected.iter().collect::<Vec<_>>());
        let retired: Vec<(&str, u32, &str)> = rfc
            .retired()
            .map(|(file, id)| (file, id.line, id.value.as_str()))
            .collect();
        let expected = [
            ("rfcs/0001-greetings/rfc.toml", 4, "EX-0001-005"),
            ("rfcs/0001-greetings/01-greeting.toml", 1, "EX-0001-006"),
            ("rfcs/0001-greetings/02-farewell.toml", 2, "EX-0001-007"),
        ];
        assert_eq!(retired, expected);
    }

    #[test]
    fn a_section_file_with_another_top_level_key_doesnt_load() {
        let text = "summary = \"Greeting\"\nretired = []\n\n[section]\nanchor = \"greeting\"\n";
        let file = "rfcs/0001-greetings/01-greeting.toml";
        let mut findings = Vec::new();
        let section = load_section(file, &ByteVec::from(text.to_owned()), &mut findings);
        assert_eq!(section, None);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].rule, crate::source::RULE);
        assert_eq!((findings[0].file.as_str(), findings[0].line), (file, 1));
        let message = &findings[0].message;
        assert!(message.contains("unknown field `summary`"), "{message}");
    }
}
