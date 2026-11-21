//! The format rules: the form, uniqueness and retirement of identifiers, the key word of each
//! statement, the fields of sections and requirement blocks, the form of a section's anchor and
//! title, which its rendered heading holds, and the implementation terms a statement names.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use toml::Value;

use crate::config::Config;
use crate::model::{Block, Field, Rfc, Section, Set, INDEX};
use crate::Finding;

/// An identifier that isn't of the form `<prefix>-<rfc>-<n>`.
pub const IDENTIFIER_FORM: &str = "identifier-form";
/// An identifier that two requirements hold.
pub const IDENTIFIER_UNIQUE: &str = "identifier-unique";
/// A retired identifier that a requirement holds. An RFC may retire an identifier of any RFC's
/// number, such as that of a requirement that moved into it from another RFC.
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
/// A requirement's `same_contract_version` that isn't an array of two or more distinct whole
/// numbers, that lists a number that isn't a contract RFC of the set, or that leaves out the RFC
/// that holds the requirement; or an RFC of such a list whose `contract_version` is below the
/// highest of the list's RFCs.
pub const CONTRACT_VERSION_SHARED: &str = "contract-version-shared";

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
    SAME_VERSION,
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
/// The requirement field that lists the RFCs that hold one contract version, the requirement's
/// own RFC among them.
const SAME_VERSION: &str = "same_contract_version";
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
    check_groups(config, set, findings);
    let mut cx = Context {
        config,
        levels: config.levels.iter().map(String::as_str).collect(),
        retired: Places::new(),
        held: Places::new(),
    };
    for rfc in &set.rfcs {
        for (file, id) in rfc.retired() {
            let mut place = Place {
                findings,
                file,
                requirement: Some(id.value.as_str()),
            };
            // An RFC may retire an identifier of any RFC's number, such as that of a requirement
            // that moved into it; a requirement that holds a retired identifier still fails.
            if parse_identifier(&config.prefix, &id.value).is_none() {
                place.error(IDENTIFIER_FORM, id.line, form(&config.prefix));
            }
            cx.retired
                .entry(id.value.as_str())
                .or_insert((file, id.line));
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

/// Adds a finding for each requirement whose `same_contract_version` lists a number that isn't
/// a contract RFC of the set or leaves out the RFC whose file holds the requirement, and, where
/// the set holds each RFC the list holds, one for each of those RFCs whose `contract_version` is
/// below the highest among them. It doesn't read a requirement whose identifier the set retires,
/// or a list whose form `check_block` refuses.
fn check_groups(config: &Config, set: &Set, findings: &mut Vec<Finding>) {
    let mut held = BTreeMap::new();
    let mut retired = BTreeSet::new();
    for rfc in &set.rfcs {
        held.entry(rfc.number).or_insert(rfc);
        retired.extend(rfc.retired().map(|(_, id)| id.value.as_str()));
    }
    for rfc in &set.rfcs {
        for section in &rfc.sections {
            for block in &section.requirements {
                let id = block.text("id");
                if id.is_some_and(|id| retired.contains(id)) {
                    continue;
                }
                let Some(field) = block.field(SAME_VERSION) else {
                    continue;
                };
                let Ok(numbers) = group(&field.value) else {
                    continue;
                };
                let mut place = Place {
                    findings,
                    file: section.file.as_str(),
                    requirement: id,
                };
                check_group(config, &held, rfc.number, &numbers, field.line, &mut place);
            }
        }
    }
}

/// The findings of one requirement's `same_contract_version`, the list `numbers` at line `line`
/// of `place`'s file, where RFC `own` holds the requirement and `held` holds each RFC of the set
/// by its number.
fn check_group(
    config: &Config,
    held: &BTreeMap<u32, &Rfc>,
    own: u32,
    numbers: &[i64],
    line: u32,
    place: &mut Place,
) {
    let mut listed = Vec::new();
    for &number in numbers {
        let contract = u32::try_from(number)
            .ok()
            .filter(|rfc| config.is_contract(*rfc));
        match contract.map(|rfc| held.get(&rfc)) {
            Some(Some(rfc)) => listed.push(*rfc),
            Some(None) => {
                let message = format!("the set doesn't hold RFC {number}");
                place.error(CONTRACT_VERSION_SHARED, line, message);
            }
            None => {
                let message = format!("RFC {number} isn't a contract: {}", contracts(config));
                place.error(CONTRACT_VERSION_SHARED, line, message);
            }
        }
    }
    if !numbers.contains(&i64::from(own)) {
        let message = format!("the list leaves out RFC {own}, which holds the requirement");
        place.error(CONTRACT_VERSION_SHARED, line, message);
    }
    if listed.len() < numbers.len() {
        return;
    }
    let versions: Vec<_> = listed
        .iter()
        .filter_map(|rfc| Some((*rfc, rfc.meta.as_ref()?.contract_version.as_ref()?)))
        .collect();
    let Some(highest) = versions.iter().map(|(_, version)| version.value).max() else {
        return;
    };
    let named = place.requirement.unwrap_or("the requirement");
    for (rfc, version) in versions {
        if version.value >= highest {
            continue;
        }
        let (number, value) = (rfc.number, version.value);
        let message = format!(
            "RFC {number}'s contract_version is {value}, below {highest}, the highest of the RFCs \
             that {named} lists: raise it to {highest}"
        );
        let (file, at) = (rfc.meta_file.as_str(), version.line);
        let requirement = place.requirement;
        let finding = Finding::error(CONTRACT_VERSION_SHARED, file, at, requirement, message);
        place.findings.push(finding);
    }
}

/// The RFC numbers of the contracts, as a finding names them.
fn contracts(config: &Config) -> String {
    match &config.contracts {
        Some(group) => {
            let (first, last) = (group.rfcs.first, group.rfcs.last);
            format!("spec.toml's contracts hold RFCs {first} to {last}")
        }
        None => "spec.toml doesn't set `[contracts]`".to_string(),
    }
}

/// The RFC numbers of a `same_contract_version` value, where it is an array of two or more
/// distinct whole numbers; what is wrong with it where it isn't.
fn group(value: &Value) -> Result<Vec<i64>, String> {
    let numbers: Option<Vec<i64>> = value
        .as_array()
        .and_then(|items| items.iter().map(Value::as_integer).collect());
    let Some(numbers) = numbers else {
        return Err(format!(
            "`{SAME_VERSION}` is an array of RFC numbers, each an integer"
        ));
    };
    if numbers.len() < 2 {
        let count = numbers.len();
        return Err(format!(
            "a list holds two RFC numbers or more, and this one holds {count}"
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some(number) = numbers.iter().find(|number| !seen.insert(**number)) {
        return Err(format!("the list holds RFC {number} more than once"));
    }
    Ok(numbers)
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
        } else if key == SAME_VERSION {
            if let Err(message) = group(&field.value) {
                place.error(CONTRACT_VERSION_SHARED, field.line, message);
            }
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

    /// An array of the integers.
    fn integers(items: &[i64]) -> Value {
        Value::Array(items.iter().copied().map(Value::Integer).collect())
    }

    #[test]
    fn a_group_is_an_array_of_two_or_more_distinct_integers() {
        assert_eq!(group(&integers(&[100, 101])), Ok(vec![100, 101]));
        assert_eq!(group(&integers(&[102, 100, 101])), Ok(vec![102, 100, 101]));
        let mixed = Value::Array(vec![Value::Integer(100), Value::String("101".to_owned())]);
        let refused = [
            Value::Integer(100),
            Value::String("100, 101".to_owned()),
            integers(&[]),
            integers(&[100]),
            integers(&[100, 100]),
            integers(&[100, 101, 100]),
            mixed,
        ];
        for value in refused {
            assert!(group(&value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn a_value_that_isnt_a_string_breaks_neither_form() {
        let field = Field::new("anchor".to_string(), 2, Value::Integer(5));
        assert_eq!(string(&field), "");
        assert!(is_anchor(string(&field)));
        assert!(!holds_line_break(string(&field)));
    }
}
