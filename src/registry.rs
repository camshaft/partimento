//! The set's registry of identifier kinds: the file whose path spec.toml sets as
//! `paths.registry`, which holds one `[[entry]]` table for each format of each kind of
//! self-describing identifier that an RFC of the set specifies. A registry that doesn't hold an
//! entry yet is a file of comment lines alone, and a set whose spec.toml doesn't set
//! `paths.registry` doesn't hold a registry.
//!
//! A section that specifies a kind of self-describing identifier names it in a `[[kind]]` table
//! of its section file, whose `name` is the identifies of the kind's entries and whose `formats`
//! lists the number of each form of the kind's identifier, so the registry holds an entry of each
//! format of each kind table.
//!
//! [`check`] reads one tree's registry against the requirements and the kind tables of its RFCs,
//! and [`kept`] reads a change of the set, from a base tree to a head tree, for an entry it
//! removes or alters.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::Deserialize;
use toml::{Spanned, Value};

use crate::config::Config;
use crate::model::Set;
use crate::{source, ByteVec, Finding, Tree};

/// The rule of a registry that is missing or whose entry breaks a rule of its fields, or that
/// doesn't hold an entry of a format of a kind table.
pub const RULE: &str = "registry";

/// The rule of a change of the set that removes an entry of the registry, or changes its code,
/// identifies, format or body_length.
pub const KEPT: &str = "registry-kept";

/// The rule of a kind table that breaks a rule of its fields, or whose name an earlier kind table
/// holds.
pub const KIND: &str = "kind";

/// The fields of a kind table: it holds both, and no other.
const KIND_FIELDS: &[&str] = &["name", "formats"];

const KIND_UNKNOWN_MESSAGE: &str = "a kind table holds only name and formats";

/// The fields of an entry: each entry holds each of them, and no other.
const FIELDS: &[&str] = &["code", "identifies", "format", "body_length", "defined_by"];

/// The fields of an entry that a change of the set doesn't alter, beside its code, which is the
/// entry's identity across the change.
const KEPT_FIELDS: &[&str] = &["identifies", "format", "body_length"];

/// The fields of an entry's body_length: it holds both, and no other.
const LENGTH_FIELDS: &[&str] = &["min", "max"];

const UNKNOWN_MESSAGE: &str =
    "an entry holds only code, identifies, format, body_length and defined_by";

type RawTable = BTreeMap<Spanned<String>, Spanned<Value>>;

/// The registry as its file holds it. A key other than `entry`, or an `entry` that isn't an
/// array of tables, fails the parse.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegistry {
    #[serde(default)]
    entry: Vec<Spanned<RawTable>>,
}

/// One entry of the registry.
struct Entry {
    /// The line of its `[[entry]]` header.
    line: u32,
    /// Each field, by its key, with the line of its key.
    fields: BTreeMap<String, (u32, Value)>,
}

impl Entry {
    fn value(&self, key: &str) -> Option<&Value> {
        self.fields.get(key).map(|(_, value)| value)
    }

    /// The line of the field `key`, or of the entry's header where it doesn't hold the field.
    fn line_of(&self, key: &str) -> u32 {
        self.fields.get(key).map_or(self.line, |(line, _)| *line)
    }

    /// The entry's code, where it is a text.
    fn code(&self) -> Option<&str> {
        self.value("code")?.as_str()
    }
}

/// Checks the registry that spec.toml locates, where it locates one, against the requirements of
/// the set, adding a finding for each rule the file breaks; and checks the fields of each kind
/// table of the set's section files, and that the registry holds an entry of each format of each
/// kind table, with the table's name as its identifies. Where spec.toml doesn't locate a
/// registry, the set doesn't hold an entry, so each format of each kind table fails.
pub fn check(tree: &Tree, config: &Config, set: &Set, findings: &mut Vec<Finding>) {
    let tables = kinds(set, findings);
    let Some(file) = config.paths.registry.as_deref() else {
        for table in &tables {
            let name = table.name;
            for format in &table.formats {
                let message = format!(
                    "the kind `{name}` has format {format}, and spec.toml doesn't set \
                     `paths.registry`, so the set doesn't hold a registry entry of it"
                );
                findings.push(Finding::error(RULE, table.file, table.line, None, message));
            }
        }
        return;
    };
    let Some(bytes) = tree.files.get(file) else {
        let message = "the registry file that `paths.registry` states is missing";
        findings.push(Finding::error(RULE, file, 1, None, message));
        return;
    };
    let Some(entries) = read(file, bytes, findings) else {
        return;
    };
    let identifiers = Identifiers::of(set);
    let mut codes: BTreeMap<&str, u32> = BTreeMap::new();
    let mut kinds: BTreeMap<(&str, i64), u32> = BTreeMap::new();
    for entry in &entries {
        check_entry(file, entry, &identifiers, findings);
        if let Some(code) = entry.code() {
            match codes.get(code) {
                Some(first) => {
                    let at = entry.line_of("code");
                    let message =
                        format!("the code `{code}` is the code of the entry at line {first} too");
                    findings.push(Finding::error(RULE, file, at, None, message));
                }
                None => {
                    codes.insert(code, entry.line);
                }
            }
        }
        // A format that isn't a whole number gets a finding of its own, so the check compares
        // whole numbers alone.
        let identifies = entry.value("identifies").and_then(Value::as_str);
        let format = entry.value("format").and_then(Value::as_integer);
        if let (Some(identifies), Some(format)) = (identifies, format) {
            let kind = (identifies, format);
            match kinds.get(&kind) {
                Some(first) => {
                    let message =
                        format!("the entry at line {first} holds the same identifies and format");
                    findings.push(Finding::error(RULE, file, entry.line, None, message));
                }
                None => {
                    kinds.insert(kind, entry.line);
                }
            }
        }
    }
    for table in &tables {
        let name = table.name;
        for &format in &table.formats {
            if kinds.contains_key(&(name, format)) {
                continue;
            }
            let message = format!(
                "the kind `{name}` has format {format}, and the registry `{file}` doesn't hold \
                 an entry of identifies `{name}` and format {format}"
            );
            findings.push(Finding::error(RULE, table.file, table.line, None, message));
        }
    }
}

/// Each entry of the registry of `base` that the registry of `head` removes, or whose identifies,
/// format or body_length it changes, as a finding of the head's side, with its file's path after
/// `head:`. An entry is the same in both trees where both hold its code, so a change of its code
/// removes it. Where the head's registry isn't UTF-8 or doesn't parse, the answer is the
/// findings of that read, with the file's path after `head:`, since the head doesn't show that it
/// keeps any entry. A base registry that isn't UTF-8 or doesn't parse isn't compared: the check
/// of the base tree reports it.
pub(crate) fn kept(base: (&Tree, &Config), head: (&Tree, &Config)) -> Vec<Finding> {
    let Some(base_file) = base.1.paths.registry.as_deref() else {
        return Vec::new();
    };
    let Some(bytes) = base.0.files.get(base_file) else {
        return Vec::new();
    };
    let Some(before) = read(base_file, bytes, &mut Vec::new()) else {
        return Vec::new();
    };
    let head_file = head.1.paths.registry.as_deref();
    let after = match head_file.and_then(|file| Some((file, head.0.files.get(file)?))) {
        Some((file, bytes)) => {
            let mut unread = Vec::new();
            match read(file, bytes, &mut unread) {
                Some(after) => after,
                None => {
                    for finding in &mut unread {
                        finding.file = format!("head:{}", finding.file);
                    }
                    return unread;
                }
            }
        }
        None => Vec::new(),
    };
    let by_code: BTreeMap<&str, &Entry> = after
        .iter()
        .filter_map(|entry| Some((entry.code()?, entry)))
        .collect();
    let file = format!("head:{}", head_file.unwrap_or(base_file));
    let mut findings = Vec::new();
    for entry in &before {
        let Some(code) = entry.code() else {
            continue;
        };
        let Some(other) = by_code.get(code) else {
            let line = entry.line;
            let message = format!(
                "the change removes the entry of code `{code}`, which the base holds at line {line}"
            );
            findings.push(Finding::error(KEPT, &file, 1, None, message));
            continue;
        };
        for key in KEPT_FIELDS {
            if entry.value(key) != other.value(key) {
                let message = format!(
                    "the entry of code `{code}` changes its {key}, which a change of the set \
                     doesn't change"
                );
                let at = other.line_of(key);
                findings.push(Finding::error(KEPT, &file, at, None, message));
            }
        }
    }
    findings.sort_by(|a, b| crate::order(a).cmp(&crate::order(b)));
    findings
}

/// The registry's entries, in the order of the file, or `None` where the file isn't UTF-8 or
/// doesn't parse, with the finding that says why.
fn read(file: &str, bytes: &ByteVec, findings: &mut Vec<Finding>) -> Option<Vec<Entry>> {
    let text = source::text(file, bytes, findings)?;
    let raw: RawRegistry = source::parse(file, &text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let line = |span: Range<usize>| lines.line(span.start);
    let entries = raw.entry.into_iter().map(|table| {
        let header = line(table.span());
        let fields = table.into_inner().into_iter().map(|(key, value)| {
            let at = line(key.span());
            (key.into_inner(), (at, value.into_inner()))
        });
        Entry {
            line: header,
            fields: fields.collect(),
        }
    });
    Some(entries.collect())
}

/// The identifiers of the set that a `defined_by` reads: each requirement's of a written RFC,
/// and each that an RFC retires, which a `defined_by` doesn't name.
struct Identifiers<'a> {
    held: BTreeSet<&'a str>,
    retired: BTreeSet<&'a str>,
}

impl<'a> Identifiers<'a> {
    fn of(set: &'a Set) -> Identifiers<'a> {
        let sections = set.rfcs.iter().flat_map(|rfc| &rfc.sections);
        let blocks = sections.flat_map(|section| &section.requirements);
        let held = blocks.filter_map(|block| block.text("id")).collect();
        let metas = set.rfcs.iter().filter_map(|rfc| rfc.meta.as_ref());
        let retired = metas.flat_map(|meta| &meta.retired);
        let retired = retired.map(|id| id.value.as_str()).collect();
        Identifiers { held, retired }
    }
}

/// One kind table of a section file whose name is a text: a kind of self-describing identifier
/// that the section specifies, which a registry entry's identifies names.
struct Kind<'a> {
    /// The section file that holds the table.
    file: &'a str,
    /// The line of its `formats`, or of its `[[kind]]` header where it doesn't hold `formats`.
    line: u32,
    name: &'a str,
    /// Each number of its formats that is a whole number of at least 1, once, in the order of
    /// the list.
    formats: Vec<i64>,
}

/// The kind tables of the set's section files whose name is a text that no earlier kind table
/// holds, in the order of the files, adding a finding with the rule `kind` for each rule a kind
/// table breaks: a field other than name and formats, or a missing one; a name that isn't a text,
/// or that an earlier kind table holds; and formats that aren't a list of one or more whole
/// numbers of at least 1, or that hold one number twice.
fn kinds<'a>(set: &'a Set, findings: &mut Vec<Finding>) -> Vec<Kind<'a>> {
    let mut tables: Vec<Kind<'a>> = Vec::new();
    let mut names: BTreeMap<&str, (&str, u32)> = BTreeMap::new();
    for section in set.rfcs.iter().flat_map(|rfc| &rfc.sections) {
        let file = section.file.as_str();
        for table in &section.kinds {
            let mut error = |line: u32, message: String| {
                findings.push(Finding::error(KIND, file, line, None, message));
            };
            for field in &table.fields {
                if !KIND_FIELDS.contains(&field.key.as_str()) {
                    let key = &field.key;
                    error(
                        field.line,
                        format!("the kind table holds `{key}`, and {KIND_UNKNOWN_MESSAGE}"),
                    );
                }
            }
            for key in KIND_FIELDS {
                if table.field(key).is_none() {
                    error(table.line, format!("the kind table doesn't hold `{key}`"));
                }
            }
            let formats = table.field("formats").map(|field| {
                let (formats, problems) = formats_of(&field.value);
                for message in problems {
                    error(field.line, message);
                }
                (field.line, formats)
            });
            let Some(field) = table.field("name") else {
                continue;
            };
            let Some(name) = field.value.as_str() else {
                let kind = kind(&field.value);
                error(field.line, format!("name is {kind}, which isn't a text"));
                continue;
            };
            if let Some((first, line)) = names.get(name) {
                let message =
                    format!("the kind table at {first}:{line} holds the name `{name}` too");
                error(field.line, message);
                continue;
            }
            names.insert(name, (file, table.line));
            let (line, formats) = formats.unwrap_or((table.line, Vec::new()));
            tables.push(Kind {
                file,
                line,
                name,
                formats,
            });
        }
    }
    tables
}

/// The formats of a kind table that are whole numbers of at least 1, once each, and what its
/// `formats` breaks: it is a list of one or more whole numbers of at least 1, none of them twice.
fn formats_of(value: &Value) -> (Vec<i64>, Vec<String>) {
    let Some(items) = value.as_array() else {
        let kind = kind(value);
        let message = format!("formats is {kind}, which isn't a list of whole numbers");
        return (Vec::new(), vec![message]);
    };
    if items.is_empty() {
        let message = "formats is an empty list, and a kind has one or more formats".to_string();
        return (Vec::new(), vec![message]);
    }
    let mut formats = Vec::new();
    let mut problems = Vec::new();
    for item in items {
        let problem = match item.as_integer() {
            Some(format) if formats.contains(&format) => format!("formats holds {format} twice"),
            Some(format) if format >= 1 => {
                formats.push(format);
                continue;
            }
            Some(format) => format!("formats holds {format}, which is less than 1"),
            None => format!("formats holds {}, which isn't a whole number", kind(item)),
        };
        problems.push(problem);
    }
    (formats, problems)
}

/// Checks one entry's fields: each of the five and no other, and the form of each.
fn check_entry(file: &str, entry: &Entry, ids: &Identifiers, findings: &mut Vec<Finding>) {
    let mut error = |line: u32, message: String| {
        findings.push(Finding::error(RULE, file, line, None, message));
    };
    for (key, (line, _)) in &entry.fields {
        if !FIELDS.contains(&key.as_str()) {
            error(
                *line,
                format!("the entry holds `{key}`, and {UNKNOWN_MESSAGE}"),
            );
        }
    }
    for key in FIELDS {
        let Some(value) = entry.value(key) else {
            error(entry.line, format!("the entry doesn't hold `{key}`"));
            continue;
        };
        let problems = match *key {
            "code" => code_problem(value).into_iter().collect(),
            "identifies" => identifies_problem(value).into_iter().collect(),
            "format" => format_problem(value).into_iter().collect(),
            "defined_by" => defined_by_problem(value, ids).into_iter().collect(),
            _ => body_length_problems(value),
        };
        for message in problems {
            error(entry.line_of(key), message);
        }
    }
}

fn code_problem(value: &Value) -> Option<String> {
    let kind = kind(value);
    (!value.is_str()).then(|| format!("code is {kind}, which isn't a text"))
}

fn identifies_problem(value: &Value) -> Option<String> {
    match value.as_str() {
        Some(text) if text.trim().is_empty() => {
            Some("identifies is blank, which doesn't state a kind of record".to_string())
        }
        Some(_) => None,
        None => {
            let kind = kind(value);
            Some(format!(
                "identifies is {kind}, which isn't a text that states a kind of record"
            ))
        }
    }
}

fn format_problem(value: &Value) -> Option<String> {
    let found = match value.as_integer() {
        Some(format) if format >= 1 => return None,
        Some(format) => format.to_string(),
        None => kind(value).to_string(),
    };
    Some(format!(
        "format is {found}, which isn't a whole number of at least 1"
    ))
}

fn defined_by_problem(value: &Value, ids: &Identifiers) -> Option<String> {
    let Some(id) = value.as_str() else {
        let kind = kind(value);
        return Some(format!(
            "defined_by is {kind}, which isn't the identifier of a requirement"
        ));
    };
    if ids.retired.contains(id) {
        Some(format!("defined_by is `{id}`, which an RFC retires"))
    } else if !ids.held.contains(id) {
        Some(format!(
            "defined_by is `{id}`, which no requirement of a written RFC holds"
        ))
    } else {
        None
    }
}

/// What body_length breaks: it is a table that holds min and max alone, each a whole number,
/// min at least 1 and not greater than max.
fn body_length_problems(value: &Value) -> Vec<String> {
    let Some(table) = value.as_table() else {
        let kind = kind(value);
        return vec![format!(
            "body_length is {kind}, which isn't a table of min and max"
        )];
    };
    let mut problems = Vec::new();
    for key in table.keys() {
        if !LENGTH_FIELDS.contains(&key.as_str()) {
            problems.push(format!(
                "body_length holds `{key}`, and it holds only min and max"
            ));
        }
    }
    let mut ends = Vec::new();
    for key in LENGTH_FIELDS {
        match table.get(*key).map(|end| (end, end.as_integer())) {
            None => problems.push(format!("body_length doesn't hold `{key}`")),
            Some((_, Some(end))) => ends.push(end),
            Some((end, None)) => {
                let kind = kind(end);
                problems.push(format!(
                    "body_length's {key} is {kind}, which isn't a whole number"
                ));
            }
        }
    }
    if let Some(min) = table.get("min").and_then(Value::as_integer) {
        if min < 1 {
            problems.push(format!("body_length's min is {min}, which is less than 1"));
        }
    }
    if let &[min, max] = ends.as_slice() {
        if min > max {
            problems.push(format!(
                "body_length's min, {min}, is greater than its max, {max}"
            ));
        }
    }
    problems
}

/// The kind of a value, as a message names it.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "a text",
        Value::Integer(_) => "an integer",
        Value::Float(_) => "a float",
        Value::Boolean(_) => "a boolean",
        Value::Datetime(_) => "a date-time",
        Value::Array(_) => "an array",
        Value::Table(_) => "a table",
    }
}
