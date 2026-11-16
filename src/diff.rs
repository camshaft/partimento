//! The changes between two trees of a set: each requirement added, changed or retired, and each
//! contract RFC whose `contract_version` differs.
//!
//! A requirement is the same in both trees where both hold its identifier, whichever file holds
//! it. It is changed where its statement, level, polarity, conditions, composition, parts,
//! relations, logic, parameters or `binds_to` differ, with each of them that a block leaves out
//! read at its default and each list read as its items in any order, so that a move, a reorder of
//! a list or a default written out isn't a change, and neither is a change of `label` or
//! `rationale` alone.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use toml::Value;

use crate::config::{self, Config};
use crate::model::{self, Block, Set};
use crate::{registry, rules, Finding, Severity, Tree};

/// The fields whose difference changes a requirement.
const COMPARED: &[&str] = &[
    "statement",
    "level",
    "polarity",
    "conditions",
    "compose",
    "parts",
    "relations",
    "logic",
    "params",
    "binds_to",
];

/// The fields whose value is a list of items in any order, empty where a block leaves it out.
const LISTS: &[&str] = &["conditions", "parts", "relations", "logic", "params"];

/// The composition of a block that leaves `compose` out.
const ATOMIC: &str = "atomic";

/// The binding of a block that leaves `binds_to` out.
const SYSTEM: &str = "system";

/// How a requirement differs between the base tree and the head tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// The head holds the requirement's identifier, and the base doesn't.
    Added,
    /// Both trees hold it, and its statement, level, polarity, conditions, composition, parts,
    /// relations, logic, parameters or `binds_to` differ.
    Changed,
    /// The base holds it, and the head doesn't.
    Retired,
}

impl ChangeKind {
    /// The kind as the text and the JSON output write it.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Changed => "changed",
            ChangeKind::Retired => "retired",
        }
    }
}

/// What a requirement binds, as its `binds_to` says: the system, or how the specification itself
/// changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Binding {
    /// The requirement binds the system.
    System,
    /// The requirement binds how the specification itself changes.
    Process,
}

impl Binding {
    /// The binding as `binds_to`, the text and the JSON output write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Binding::System => "system",
            Binding::Process => "process",
        }
    }
}

/// One requirement that differs between the base tree and the head tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    /// The requirement's identifier.
    pub requirement: String,
    /// The requirement's `binds_to` in the head, or in the base where the head doesn't hold it.
    pub binds_to: Binding,
}

/// The text form of a change: `<kind> <requirement> binds_to=<binds_to>`.
impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, binds_to) = (self.kind.as_str(), self.binds_to.as_str());
        write!(f, "{kind} {} binds_to={binds_to}", self.requirement)
    }
}

/// A contract RFC that both trees hold, whose `contract_version` differs between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct VersionChange {
    /// The contract RFC's number.
    pub rfc: u32,
    /// Its `contract_version` in the base.
    pub base: i64,
    /// Its `contract_version` in the head.
    pub head: i64,
}

/// The text form of a version change: `version <rfc> <base> <head>`.
impl fmt::Display for VersionChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "version {} {} {}", self.rfc, self.base, self.head)
    }
}

/// A tree's configuration and RFCs, where it passes the format rules.
struct Loaded {
    config: Config,
    set: Set,
}

/// The order of a requirement: its RFC number and its number, then its identifier's text.
type Order = (Option<(u32, u32)>, String);

/// Each requirement added, changed and retired from `base` to `head`, in the order of its
/// identifier; or, where a tree fails a format rule, each finding that fails, the base's first,
/// with its file's path after the tree's side, `base:` or `head:`.
pub(crate) fn changes(base: &Tree, head: &Tree) -> Result<Vec<Change>, Vec<Finding>> {
    let (base, head) = load_both(base, head)?;
    Ok(requirement_changes(&base, &head))
}

/// Each contract RFC that both trees hold whose `contract_version` differs from `base` to `head`,
/// in the order of its number; or, where a tree fails a format rule, each finding that fails, as
/// [`changes`] returns them.
pub(crate) fn contract_versions(
    base: &Tree,
    head: &Tree,
) -> Result<Vec<VersionChange>, Vec<Finding>> {
    let (base, head) = load_both(base, head)?;
    Ok(version_changes(&base, &head))
}

/// The changes [`changes`] returns and the version changes [`contract_versions`] returns, from
/// one load of each tree; or, where a tree fails a format rule, each finding that fails, as
/// [`changes`] returns them.
pub(crate) fn changes_and_versions(
    base: &Tree,
    head: &Tree,
) -> Result<(Vec<Change>, Vec<VersionChange>), Vec<Finding>> {
    let (base, head) = load_both(base, head)?;
    Ok((
        requirement_changes(&base, &head),
        version_changes(&base, &head),
    ))
}

/// Both trees loaded, where both pass the format rules and the head keeps each entry of the
/// base's registry; otherwise each finding that fails, the base's first, with its file's path
/// after the tree's side, `base:` or `head:`.
fn load_both(base_tree: &Tree, head_tree: &Tree) -> Result<(Loaded, Loaded), Vec<Finding>> {
    let mut failing = Vec::new();
    let base = load("base", base_tree, &mut failing);
    let head = load("head", head_tree, &mut failing);
    let (Some(base), Some(head)) = (base, head) else {
        return Err(failing);
    };
    let kept = registry::kept((base_tree, &base.config), (head_tree, &head.config));
    if kept.is_empty() {
        Ok((base, head))
    } else {
        Err(kept)
    }
}

/// Each requirement added, changed and retired from `base` to `head`, in the order of its
/// identifier.
fn requirement_changes(base: &Loaded, head: &Loaded) -> Vec<Change> {
    let before = held(&base.set);
    let after = held(&head.set);
    let mut changes = BTreeMap::new();
    for (id, block) in &before {
        match after.get(id) {
            None => insert(&mut changes, base, ChangeKind::Retired, id, block),
            Some(other) if !same_requirement(block, other) => {
                insert(&mut changes, head, ChangeKind::Changed, id, other);
            }
            Some(_) => {}
        }
    }
    for (id, block) in &after {
        if !before.contains_key(id) {
            insert(&mut changes, head, ChangeKind::Added, id, block);
        }
    }
    changes.into_values().collect()
}

/// Each contract RFC that both trees hold whose `contract_version` differs from `base` to `head`,
/// in the order of its number.
fn version_changes(base: &Loaded, head: &Loaded) -> Vec<VersionChange> {
    let after = versions(head);
    let changed = versions(base).into_iter().filter_map(|(rfc, base)| {
        let head = *after.get(&rfc)?;
        (head != base).then_some(VersionChange { rfc, base, head })
    });
    changed.collect()
}

/// The `contract_version` of each contract RFC of the tree, by its number.
fn versions(tree: &Loaded) -> BTreeMap<u32, i64> {
    let mut versions = BTreeMap::new();
    for rfc in &tree.set.rfcs {
        let meta = rfc.meta.as_ref();
        let version = meta.and_then(|meta| meta.contract_version.as_ref());
        if let Some(version) = version.filter(|_| tree.config.is_contract(rfc.number)) {
            versions.insert(rfc.number, version.value);
        }
    }
    versions
}

/// The tree's configuration and RFCs, where it passes the format rules; otherwise adds to
/// `failing` each finding that fails, with its file's path after `<side>:`.
fn load(side: &str, tree: &Tree, failing: &mut Vec<Finding>) -> Option<Loaded> {
    let mut findings = Vec::new();
    let loaded = config::load(tree, &mut findings).map(|config| {
        let set = model::load(tree, &config, &mut findings);
        rules::check(&config, &set, &mut findings);
        Loaded { config, set }
    });
    findings.retain(|finding| finding.severity == Severity::Error);
    if findings.is_empty() {
        return loaded;
    }
    findings.sort_by(|a, b| crate::order(a).cmp(&crate::order(b)));
    for mut finding in findings {
        finding.file = format!("{side}:{}", finding.file);
        failing.push(finding);
    }
    None
}

/// Each requirement block of the set by its identifier, which the format rules hold that each
/// block has and that no two blocks share.
fn held(set: &Set) -> BTreeMap<&str, &Block> {
    let sections = set.rfcs.iter().flat_map(|rfc| &rfc.sections);
    let blocks = sections.flat_map(|section| &section.requirements);
    blocks
        .filter_map(|block| Some((block.text("id")?, block)))
        .collect()
}

/// Adds the change of the requirement `id`, with its `binds_to` from `block` of `tree`, at the
/// place of its identifier.
fn insert(
    changes: &mut BTreeMap<Order, Change>,
    tree: &Loaded,
    kind: ChangeKind,
    id: &str,
    block: &Block,
) {
    let order = (
        rules::parse_identifier(&tree.config.prefix, id),
        id.to_string(),
    );
    let binds_to = match block.text("binds_to") {
        Some("process") => Binding::Process,
        _ => Binding::System,
    };
    let requirement = id.to_string();
    let change = Change {
        kind,
        requirement,
        binds_to,
    };
    changes.insert(order, change);
}

/// Whether two blocks of one identifier require the same thing.
fn same_requirement(base: &Block, head: &Block) -> bool {
    let (base, head) = (meaning(base), meaning(head));
    base.len() == head.len()
        && base.iter().all(|(key, value)| {
            let compare: fn(&Value, &Value) -> bool = if LISTS.contains(key) {
                same_items
            } else {
                same
            };
            head.get(key).is_some_and(|other| compare(value, other))
        })
}

/// The fields of a block that say what it requires, with each list it leaves out empty, `compose`
/// atomic where it leaves that out and `binds_to` system where it leaves that out.
fn meaning(block: &Block) -> BTreeMap<&'static str, Value> {
    let mut fields = BTreeMap::new();
    for key in COMPARED {
        let value = match block.field(key) {
            Some(field) => field.value.clone(),
            None if LISTS.contains(key) => Value::Array(Vec::new()),
            None if *key == "compose" => Value::String(ATOMIC.to_string()),
            None if *key == "binds_to" => Value::String(SYSTEM.to_string()),
            None => continue,
        };
        fields.insert(*key, value);
    }
    fields
}

/// Whether two lists hold the same items, each as often, in any order.
fn same_items(a: &Value, b: &Value) -> bool {
    let (Some(a), Some(b)) = (a.as_array(), b.as_array()) else {
        return same(a, b);
    };
    if a.len() != b.len() {
        return false;
    }
    let mut matched = vec![false; b.len()];
    for item in a {
        let free = (0..b.len()).find(|&index| !matched[index] && same(item, &b[index]));
        match free {
            Some(index) => matched[index] = true,
            None => return false,
        }
    }
    true
}

/// Whether two values are equal, with a NaN equal to a NaN, so that a float the trees both hold
/// isn't a change.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(a), Value::Float(b)) => a == b || (a.is_nan() && b.is_nan()),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Table(a), Value::Table(b)) => {
            let held = |(key, value): (&String, &Value)| b.get(key).is_some_and(|b| same(value, b));
            a.len() == b.len() && a.iter().all(held)
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use toml::{Table, Value};

    use super::{
        changes, changes_and_versions, contract_versions, same, same_items, same_requirement,
        VersionChange,
    };
    use crate::model::{Block, Field};
    use crate::{ByteVec, Tree};

    /// The configuration of a set of one system and one contract group.
    const SPEC: &str = r#"prefix = "EX"
levels = ["MUST"]
implementation_terms = ["sqlite"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = ".duvet/requirements"

[[system]]
name = "core"
first = 1
last = 99

[contracts]
name = "contracts"
first = 100
last = 199
"#;

    /// An index of RFC 100, a contract.
    const INDEX: &str = r#"[[rfc]]
number = 100
title = "Answers"
area = "contracts"
"#;

    /// RFC 100's metadata, without its `contract_version`.
    const META: &str = r#"number = 100
title = "Answers"
area = "contracts"
abstract = "How the program answers requests."
retired = []
"#;

    /// RFC 100's one section, of one requirement.
    const SECTION: &str = r#"[section]
anchor = "answers"
title = "Answers"
problem = "The program drops requests."
motivation = "An answer to each request keeps it."

[[requirement]]
id = "EX-0100-001"
level = "MUST"
polarity = "required"
statement = "The program MUST answer each request."
binds_to = "system"
rationale = "A caller waits for each answer."
"#;

    /// A tree of RFC 100 at the contract version, with the files the render generates from it.
    fn contract_tree(version: u32) -> Tree {
        let meta = format!("{META}contract_version = {version}\n");
        let files = [
            ("spec.toml", SPEC.to_string()),
            ("rfcs/index.toml", INDEX.to_string()),
            ("rfcs/0100-answers/rfc.toml", meta),
            ("rfcs/0100-answers/01-answer.toml", SECTION.to_string()),
        ];
        let mut tree = Tree::default();
        for (path, text) in files {
            tree.files.insert(path.to_string(), ByteVec::from(text));
        }
        let generated = crate::render(&tree).expect("the tree renders");
        tree.files.extend(generated.files);
        tree
    }

    #[test]
    fn a_changed_contract_version_is_reported() {
        let found = contract_versions(&contract_tree(1), &contract_tree(2));
        let expected = VersionChange {
            rfc: 100,
            base: 1,
            head: 2,
        };
        let found = found.map_err(|findings| format!("{findings:?}"));
        assert_eq!(found, Ok(vec![expected]));
        assert_eq!(expected.to_string(), "version 100 1 2");
    }

    #[test]
    fn an_unchanged_contract_version_isnt_reported() {
        let found = contract_versions(&contract_tree(1), &contract_tree(1));
        let found = found.map_err(|findings| format!("{findings:?}"));
        assert_eq!(found, Ok(Vec::new()));
    }

    #[test]
    fn changes_and_versions_returns_the_version_changes() {
        let (base, head) = (contract_tree(1), contract_tree(2));
        let found = changes_and_versions(&base, &head);
        let found = found.map_err(|findings| format!("{findings:?}"));
        let version = VersionChange {
            rfc: 100,
            base: 1,
            head: 2,
        };
        assert_eq!(found, Ok((Vec::new(), vec![version])));
    }

    #[test]
    fn changes_and_versions_of_a_failing_tree_returns_the_findings_of_changes() {
        let (failing, passing) = (Tree::default(), contract_tree(1));
        for (base, head) in [(&failing, &passing), (&passing, &failing)] {
            let found = changes_and_versions(base, head).expect_err("a tree fails");
            let expected = changes(base, head).expect_err("a tree fails");
            assert_eq!(found, expected);
        }
    }

    /// A block of the required fields alone, so that each compared field reads at its default.
    const PLAIN: &str = r#"id = "EX-0001-001"
level = "MUST"
polarity = "required"
statement = "The program MUST print a greeting."
"#;

    /// A block that writes each compared field, with `compose` other than its default and an item
    /// in each list.
    const FULL: &str = r#"id = "EX-0001-001"
level = "MUST"
polarity = "required"
statement = "The program MUST print a greeting."
conditions = ["EX-0001-002"]
compose = "all_of"
parts = ["EX-0001-003"]
relations = [{ kind = "depends_on", target = "EX-0001-004" }]
logic = ["EX-0001-002"]
params = [{ name = "delay", value = 5 }]
binds_to = "system"
"#;

    /// A change of each compared field of `FULL` that fixture 15 doesn't change alone. A fixture
    /// can't change `level` or `polarity` alone, since its statement would then fail the key-word
    /// rule, so these tests compare the blocks with `same_requirement`.
    const CHANGES: &[&str] = &[
        r#"level = "SHOULD""#,
        r#"polarity = "forbidden""#,
        r#"conditions = ["EX-0001-005"]"#,
        r#"compose = "any_of""#,
        r#"parts = ["EX-0001-006"]"#,
        r#"relations = [{ kind = "depends_on", target = "EX-0001-007" }]"#,
        r#"logic = ["EX-0001-005"]"#,
        r#"params = [{ name = "delay", value = 10 }]"#,
    ];

    fn list(items: &[&str]) -> Value {
        let item = |text: &&str| Value::String((*text).to_string());
        Value::Array(items.iter().map(item).collect())
    }

    /// The block that `text`, a TOML table, holds, with each field of `changes`, a TOML table too,
    /// set to its value there.
    fn block(text: &str, changes: &str) -> Block {
        let mut table: Table = toml::from_str(text).expect("the test's block parses");
        let changes: Table = toml::from_str(changes).expect("the test's change parses");
        for (key, value) in changes {
            table.insert(key, value);
        }
        let fields = table
            .into_iter()
            .map(|(key, value)| Field::new(key, 1, value))
            .collect();
        Block { line: 1, fields }
    }

    #[test]
    fn a_list_holds_the_same_items_in_any_order() {
        assert!(same_items(&list(&["a", "b"]), &list(&["b", "a"])));
        assert!(!same_items(&list(&["a", "a"]), &list(&["a", "b"])));
        assert!(!same_items(&list(&["a"]), &list(&["a", "a"])));
        assert!(!same_items(&list(&["a"]), &list(&["b"])));
    }

    #[test]
    fn a_nan_is_the_same_as_a_nan_and_no_other_float() {
        let nan = Value::Float(f64::NAN);
        assert!(same(&nan, &Value::Float(f64::NAN)));
        assert!(!same(&nan, &Value::Float(1.0)));
        assert!(same(&Value::Float(1.0), &Value::Float(1.0)));
    }

    #[test]
    fn a_nested_list_keeps_its_order() {
        let a = Value::Array(vec![list(&["a", "b"])]);
        let b = Value::Array(vec![list(&["b", "a"])]);
        assert!(!same_items(&a, &b));
    }

    #[test]
    fn a_change_of_each_compared_field_alone_changes_the_requirement() {
        let base = block(FULL, "");
        assert!(same_requirement(&base, &block(FULL, "")));
        for change in CHANGES {
            let head = block(FULL, change);
            assert_eq!(head.fields.len(), base.fields.len(), "{change}");
            assert!(!same_requirement(&base, &head), "{change}");
            assert!(!same_requirement(&head, &base), "{change}");
        }
    }

    #[test]
    fn conditions_written_out_empty_are_the_conditions_left_out() {
        let left_out = block(PLAIN, "");
        let written = block(PLAIN, "conditions = []");
        assert!(left_out.field("conditions").is_none());
        assert!(written.field("conditions").is_some());
        assert!(same_requirement(&left_out, &written));
        assert!(same_requirement(&written, &left_out));
    }
}
