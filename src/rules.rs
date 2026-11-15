//! The format rules: the form, uniqueness and retirement of identifiers, the key word of each
//! statement, the fields of sections and requirement blocks, the form of a section's anchor and
//! title, which its rendered heading holds, and the implementation terms a statement names.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use toml::Value;

use crate::config::Config;
use crate::model::{Block, Field, Section, Set, INDEX};
use crate::Finding;

/// An identifier that isn't of the form `<prefix>-<rfc>-<n>`.
pub const IDENTIFIER_FORM: &str = "identifier-form";
/// An identifier that two requirements hold.
pub const IDENTIFIER_UNIQUE: &str = "identifier-unique";
/// A retired identifier that a requirement holds, or that another RFC than its own retires.
pub const IDENTIFIER_RETIRED: &str = "identifier-retired";
/// A statement whose key words don't match its `level` and `polarity`.
pub const KEY_WORD: &str = "key-word";
/// A field outside the fields of a section or a requirement block.
pub const FIELD_UNKNOWN: &str = "field-unknown";
/// A requirement block without one of its required fields.
pub const FIELD_MISSING: &str = "field-missing";
/// A field whose value has another type or isn't one of its values.
pub const FIELD_VALUE: &str = "field-value";
/// A section without its anchor, title, problem or motivation.
pub const SECTION_FIELD: &str = "section-field";
/// A section anchor that holds a character other than ASCII letters, digits, `-` and `_`: the
/// rendered heading writes the anchor as written in its explicit anchor `{#<anchor>}`, which
/// white space, a brace or a line break would break.
pub const ANCHOR_FORM: &str = "anchor-form";
/// A section title that holds a line break, which the rendered heading, one line, can't hold.
pub const TITLE_FORM: &str = "title-form";
/// A statement that names a term of spec.toml's `implementation_terms`.
pub const IMPLEMENTATION_TERM: &str = "implementation-term";
/// A relation whose target no requirement holds yet; `Options::final_pass` makes it fail.
pub const RELATION_UNRESOLVED: &str = "relation-unresolved";
/// An RFC's area, in the index or in its rfc.toml, that isn't the name of a group of
/// spec.toml, or whose group doesn't hold the RFC's number.
pub const AREA: &str = "area";
/// An rfc.toml whose title or area isn't the one its row of the index lists.
pub const INDEX_AGREEMENT: &str = "index-agreement";
/// A contract's RFC without `contract_version`, a `contract_version` below 1, or one that an
/// RFC outside the contracts holds.
pub const CONTRACT_VERSION: &str = "contract-version";

const REQUIREMENT_FIELDS: &[&str] = &[
    "id",
    "label",
    "level",
    "polarity",
    "statement",
    "conditions",
    "compose",
    "parts",
    "relations",
    "logic",
    "params",
    "binds_to",
    "rationale",
];
const REQUIRED_REQUIREMENT_FIELDS: &[&str] = &[
    "id",
    "level",
    "polarity",
    "statement",
    "binds_to",
    "rationale",
];
const SECTION_FIELDS: &[&str] = &["anchor", "title", "problem", "motivation", "prose"];
const REQUIRED_SECTION_FIELDS: &[&str] = &["anchor", "title", "problem", "motivation"];
const POLARITIES: &[&str] = &["required", "forbidden"];
const COMPOSITIONS: &[&str] = &["atomic", "all_of", "any_of", "exactly_one", "unless"];
const BINDINGS: &[&str] = &["system", "process"];
/// The message of an `anchor-form` finding; it doesn't quote the anchor, which may hold a line
/// break.
const ANCHOR_MESSAGE: &str =
    "the anchor holds a character other than ASCII letters, digits, `-` and `_`";
const TITLE_MESSAGE: &str = "the title holds a line break, and a heading is one line";

/// Where each identifier is held or retired: a file and a line.
type Places<'a> = BTreeMap<&'a str, (&'a str, u32)>;

struct Context<'a> {
    config: &'a Config,
    levels: Vec<&'a str>,
    retired: Places<'a>,
    held: Places<'a>,
}

/// The file and the requirement that the findings of a block are about.
struct Place<'a, 'f> {
    findings: &'f mut Vec<Finding>,
    file: &'a str,
    requirement: Option<&'a str>,
}

impl Place<'_, '_> {
    fn error(&mut self, rule: &str, line: u32, message: String) {
        let finding = Finding::error(rule, self.file, line, self.requirement, message);
        self.findings.push(finding);
    }

    fn warning(&mut self, rule: &str, line: u32, message: String) {
        let finding = Finding::warning(rule, self.file, line, self.requirement, message);
        self.findings.push(finding);
    }
}

/// Adds a finding for each format rule that the set breaks.
pub fn check(config: &Config, set: &Set, findings: &mut Vec<Finding>) {
    check_rfcs(config, set, findings);
    let mut cx = Context {
        config,
        levels: config.levels.iter().map(String::as_str).collect(),
        retired: Places::new(),
        held: Places::new(),
    };
    for rfc in &set.rfcs {
        let Some(meta) = &rfc.meta else {
            continue;
        };
        for id in &meta.retired {
            let mut place = Place {
                findings,
                file: rfc.meta_file.as_str(),
                requirement: Some(id.value.as_str()),
            };
            match parse_identifier(&config.prefix, &id.value) {
                None => place.error(IDENTIFIER_FORM, id.line, form(&config.prefix)),
                Some((number, _)) if number != rfc.number => {
                    let held = rfc.number;
                    let message = format!("RFC {held} retires an identifier of RFC {number}");
                    place.error(IDENTIFIER_RETIRED, id.line, message);
                }
                Some(_) => {}
            }
            let at = (rfc.meta_file.as_str(), id.line);
            cx.retired.entry(id.value.as_str()).or_insert(at);
        }
    }
    for rfc in &set.rfcs {
        for section in &rfc.sections {
            check_section(section, findings);
            for block in &section.requirements {
                check_block(&mut cx, section, block, findings);
            }
        }
    }
}

fn check_section(section: &Section, findings: &mut Vec<Finding>) {
    let mut place = Place {
        findings,
        file: section.file.as_str(),
        requirement: None,
    };
    for field in &section.fields {
        let key = field.key.as_str();
        if !SECTION_FIELDS.contains(&key) {
            let fields = list(SECTION_FIELDS);
            let message = format!("`{key}` isn't a field of a section, which holds only {fields}");
            place.error(FIELD_UNKNOWN, field.line, message);
        } else if !field.value.is_str() {
            place.error(FIELD_VALUE, field.line, format!("`{key}` is a string"));
        }
    }
    for key in REQUIRED_SECTION_FIELDS {
        let Some(field) = section.field(key) else {
            let fields = list(REQUIRED_SECTION_FIELDS);
            let message = format!("the section doesn't hold `{key}`, one of {fields}");
            place.error(SECTION_FIELD, section.line, message);
            continue;
        };
        if is_blank(&field.value) {
            let message = format!("the section's `{key}` is empty");
            place.error(SECTION_FIELD, field.line, message);
        }
    }
    let anchor = section.field("anchor");
    if let Some(field) = anchor.filter(|field| !is_anchor(string(field))) {
        place.error(ANCHOR_FORM, field.line, ANCHOR_MESSAGE.to_string());
    }
    let title = section.field("title");
    if let Some(field) = title.filter(|field| holds_line_break(string(field))) {
        place.error(TITLE_FORM, field.line, TITLE_MESSAGE.to_string());
    }
}

/// Adds a finding for each RFC whose area isn't the name of a group that holds its number, whose
/// rfc.toml disagrees with its row of the index, or whose `contract_version` breaks its rule.
fn check_rfcs(config: &Config, set: &Set, findings: &mut Vec<Finding>) {
    let index_file = format!("{}/{INDEX}", config.paths.rfcs);
    let mut rows = BTreeMap::new();
    if let Some(index) = &set.index {
        for entry in &index.rfc {
            if let Some(message) = area_problem(config, &entry.area, entry.number) {
                findings.push(Finding::error(
                    AREA,
                    &index_file,
                    entry.area_line,
                    None,
                    message,
                ));
            }
            rows.entry(entry.number).or_insert(entry);
        }
    }
    for rfc in &set.rfcs {
        let Some(meta) = &rfc.meta else {
            continue;
        };
        let number = rfc.number;
        let mut place = Place {
            findings,
            file: rfc.meta_file.as_str(),
            requirement: None,
        };
        if let Some(message) = area_problem(config, &meta.area, number) {
            place.error(AREA, meta.area_line, message);
        }
        if let Some(row) = rows.get(&number) {
            let (title, listed) = (&meta.title, &row.title);
            if title != listed {
                let message = format!("the title is `{title}`, and the index lists `{listed}`");
                place.error(INDEX_AGREEMENT, meta.title_line, message);
            }
            let (area, listed) = (&meta.area, &row.area);
            if area != listed {
                let message = format!("the area is `{area}`, and the index lists `{listed}`");
                place.error(INDEX_AGREEMENT, meta.area_line, message);
            }
        }
        let contract = config.is_contract(number);
        match &meta.contract_version {
            None if contract => {
                let message = format!("RFC {number}, a contract, doesn't hold `contract_version`");
                place.error(CONTRACT_VERSION, meta.number_line, message);
            }
            Some(version) if !contract => {
                let message = "only a contract's RFC holds `contract_version`".to_string();
                place.error(CONTRACT_VERSION, version.line, message);
            }
            Some(version) if version.value < 1 => {
                let value = version.value;
                let message = format!("`contract_version` is 1 or more, and {value} isn't");
                place.error(CONTRACT_VERSION, version.line, message);
            }
            _ => {}
        }
    }
}

/// What is wrong with an RFC's area: it isn't the name of a group, or its group doesn't hold
/// the RFC's number.
fn area_problem(config: &Config, area: &str, number: u32) -> Option<String> {
    let Some(group) = config.group(area) else {
        return Some(format!(
            "the area `{area}` isn't the name of a group of spec.toml"
        ));
    };
    let range = group.rfcs;
    if range.contains(number) {
        return None;
    }
    let (first, last) = (range.first, range.last);
    Some(format!(
        "RFC {number} is outside group `{area}`, which holds RFCs {first} to {last}"
    ))
}

fn check_block<'a>(
    cx: &mut Context<'a>,
    section: &'a Section,
    block: &'a Block,
    findings: &mut Vec<Finding>,
) {
    let file = section.file.as_str();
    let mut place = Place {
        findings,
        file,
        requirement: block.text("id"),
    };

    for field in &block.fields {
        let key = field.key.as_str();
        if !REQUIREMENT_FIELDS.contains(&key) {
            let fields = list(REQUIREMENT_FIELDS);
            let message = format!("`{key}` isn't a requirement field; a block holds only {fields}");
            place.error(FIELD_UNKNOWN, field.line, message);
        } else if let Some(kind) = expected_kind(key, &field.value) {
            place.error(FIELD_VALUE, field.line, format!("`{key}` is {kind}"));
        }
    }
    for key in REQUIRED_REQUIREMENT_FIELDS {
        if block.field(key).is_none() {
            let message = format!("the requirement block doesn't hold `{key}`");
            place.error(FIELD_MISSING, block.line, message);
        }
    }
    let rationale = block.field("rationale");
    if let Some(field) = rationale.filter(|field| is_blank(&field.value)) {
        place.error(FIELD_VALUE, field.line, "`rationale` is empty".to_string());
    }

    if let Some(field) = block.field("id") {
        if let Some(id) = field.value.as_str() {
            if parse_identifier(&cx.config.prefix, id).is_none() {
                place.error(IDENTIFIER_FORM, field.line, form(&cx.config.prefix));
            }
            if let Some((file, line)) = cx.retired.get(id) {
                let message = format!("the identifier is retired at {file}:{line}");
                place.error(IDENTIFIER_RETIRED, field.line, message);
            }
            match cx.held.entry(id) {
                Entry::Occupied(entry) => {
                    let (file, line) = *entry.get();
                    let message = format!("{file}:{line} holds the identifier too");
                    place.error(IDENTIFIER_UNIQUE, field.line, message);
                }
                Entry::Vacant(entry) => {
                    entry.insert((file, field.line));
                }
            }
        }
    }

    let level = choice(&mut place, block, "level", &cx.levels);
    let polarity = choice(&mut place, block, "polarity", POLARITIES);
    choice(&mut place, block, "compose", COMPOSITIONS);
    choice(&mut place, block, "binds_to", BINDINGS);

    let Some(field) = block.field("statement") else {
        return;
    };
    let Some(statement) = field.value.as_str() else {
        return;
    };
    if let (Some(level), Some(polarity)) = (level, polarity) {
        let expected = match polarity {
            "forbidden" => format!("{level} NOT"),
            _ => level.to_string(),
        };
        let found = key_words(statement, &cx.levels);
        if let Some(message) = key_word_problem(&found, &expected) {
            place.error(KEY_WORD, field.line, message);
        }
    }
    for term in &cx.config.implementation_terms {
        if !term.is_empty() && names(statement, term) {
            let message = format!("the statement names `{term}`, a term of `implementation_terms`");
            place.warning(IMPLEMENTATION_TERM, field.line, message);
        }
    }
}

/// The block's value of `key` where it is one of `choices`; a finding where it is another.
fn choice<'b>(out: &mut Place, block: &'b Block, key: &str, choices: &[&str]) -> Option<&'b str> {
    let field = block.field(key)?;
    let value = field.value.as_str()?;
    if choices.contains(&value) {
        return Some(value);
    }
    let choices = list(choices);
    let message = format!("`{key}` is one of {choices}, and `{value}` isn't");
    out.error(FIELD_VALUE, field.line, message);
    None
}

/// What is wrong with the key words a statement holds, where they aren't the one it needs.
fn key_word_problem(found: &[String], expected: &str) -> Option<String> {
    let message = match found {
        [] => format!("the statement doesn't hold a key word; it needs {expected}"),
        [one] if one == expected => return None,
        [one] => format!("the statement says {one}, and `level` and `polarity` say {expected}"),
        many => {
            let count = many.len();
            format!("the statement holds {count} key words, and it needs exactly one")
        }
    };
    Some(message)
}

/// The type a requirement field's value has, where `value` has another.
fn expected_kind(key: &str, value: &Value) -> Option<&'static str> {
    let (kind, matches) = match key {
        "conditions" | "parts" | "logic" => ("an array of strings", all(value, Value::is_str)),
        "relations" | "params" => ("an array of tables", all(value, Value::is_table)),
        _ => ("a string", value.is_str()),
    };
    (!matches).then_some(kind)
}

/// Whether `value` is an array whose every item is of the kind.
fn all(value: &Value, kind: fn(&Value) -> bool) -> bool {
    value.as_array().is_some_and(|items| items.iter().all(kind))
}

fn is_blank(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.trim().is_empty())
}

/// The field's value where it is a string, and the empty string where it isn't.
fn string(field: &Field) -> &str {
    field.value.as_str().unwrap_or_default()
}

/// Whether the anchor holds ASCII letters, digits, `-` and `_` alone: the rendered heading writes
/// it as written in its explicit anchor `{#<anchor>}`.
fn is_anchor(anchor: &str) -> bool {
    let allowed = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
    anchor.bytes().all(allowed)
}

/// Whether the title, trimmed as the rendered heading trims it, holds a line break.
fn holds_line_break(title: &str) -> bool {
    title.trim().contains(['\n', '\r'])
}

/// The RFC number and the requirement number of an identifier `<prefix>-<rfc>-<n>`, with `<rfc>`
/// four digits and `<n>` three digits or more.
pub fn parse_identifier(prefix: &str, id: &str) -> Option<(u32, u32)> {
    let rest = id.strip_prefix(prefix)?.strip_prefix('-')?;
    let (rfc, number) = rest.split_once('-')?;
    let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    if rfc.len() != 4 || number.len() < 3 || !digits(rfc) || !digits(number) {
        return None;
    }
    Some((rfc.parse().ok()?, number.parse().ok()?))
}

fn form(prefix: &str) -> String {
    format!("the identifier isn't {prefix}-<rfc>-<n>, <rfc> four digits and <n> three or more")
}

/// Each key word of the statement: a level, followed by NOT where the statement forbids.
fn key_words(statement: &str, levels: &[&str]) -> Vec<String> {
    let words: Vec<&str> = statement
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let mut found = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if !levels.contains(word) {
            continue;
        }
        match words.get(index + 1) {
            Some(&"NOT") => found.push(format!("{word} NOT")),
            _ => found.push((*word).to_string()),
        }
    }
    found
}

/// Whether the statement holds the term as whole words, in any case.
fn names(statement: &str, term: &str) -> bool {
    let text = statement.to_lowercase();
    let term = term.to_lowercase();
    text.match_indices(term.as_str()).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + term.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn list(items: &[&str]) -> String {
    let items: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    items.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_is_ascii_letters_digits_hyphens_and_underscores() {
        for anchor in ["waking", "HT-1002-012", "a_b", "2-ways"] {
            assert!(is_anchor(anchor), "{anchor:?}");
        }
        let refused = [
            "two words",
            "tab\there",
            "reply}",
            "{reply",
            "a\\b",
            "a<b",
            "a#b",
            "a.b",
            "caf\u{e9}",
            "line\nbreak",
        ];
        for anchor in refused {
            assert!(!is_anchor(anchor), "{anchor:?}");
        }
    }

    #[test]
    fn a_title_holds_a_line_break_only_inside_its_trimmed_text() {
        assert!(holds_line_break("Saying\nhello"));
        assert!(holds_line_break("Saying\rhello"));
        for title in ["Saying hello\n", "\nSaying hello", "Saying hello"] {
            assert!(!holds_line_break(title), "{title:?}");
        }
    }

    #[test]
    fn a_value_that_isnt_a_string_breaks_neither_form() {
        let field = Field {
            key: "anchor".to_string(),
            line: 2,
            value: Value::Integer(5),
        };
        assert_eq!(string(&field), "");
        assert!(is_anchor(string(&field)));
        assert!(!holds_line_break(string(&field)));
    }
}
