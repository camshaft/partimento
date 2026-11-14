//! The changes between two trees of a set: each requirement added, changed or retired.
//!
//! A requirement is the same in both trees where both hold its identifier, whichever file holds
//! it. It is changed where its statement, level, polarity, conditions, composition, parts,
//! relations, logic or parameters differ, with each of them that a block leaves out read at its
//! default and each list read as its items in any order, so that a move, a reorder of a list or a
//! default written out isn't a change, and neither is a change of `label`, `rationale` or
//! `binds_to` alone.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use toml::Value;

use crate::config::{self, Config};
use crate::model::{self, Block, Set};
use crate::{rules, Finding, Severity, Tree};

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
];

/// The fields whose value is a list of items in any order, empty where a block leaves it out.
const LISTS: &[&str] = &["conditions", "parts", "relations", "logic", "params"];

/// The composition of a block that leaves `compose` out.
const ATOMIC: &str = "atomic";

/// How a requirement differs between the base tree and the head tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// The head holds the requirement's identifier, and the base doesn't.
    Added,
    /// Both trees hold it, and its statement, level, polarity, conditions, composition, parts,
    /// relations, logic or parameters differ.
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
    let mut failing = Vec::new();
    let base = load("base", base, &mut failing);
    let head = load("head", head, &mut failing);
    let (Some(base), Some(head)) = (base, head) else {
        return Err(failing);
    };
    let before = held(&base.set);
    let after = held(&head.set);
    let mut changes = BTreeMap::new();
    for (id, block) in &before {
        match after.get(id) {
            None => insert(&mut changes, &base, ChangeKind::Retired, id, block),
            Some(other) if !same_requirement(block, other) => {
                insert(&mut changes, &head, ChangeKind::Changed, id, other);
            }
            Some(_) => {}
        }
    }
    for (id, block) in &after {
        if !before.contains_key(id) {
            insert(&mut changes, &head, ChangeKind::Added, id, block);
        }
    }
    Ok(changes.into_values().collect())
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
    let order = (rules::parse_identifier(&tree.config.prefix, id), id.to_string());
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

/// The fields of a block that say what it requires, with each list it leaves out empty and
/// `compose` atomic where it leaves that out.
fn meaning(block: &Block) -> BTreeMap<&'static str, Value> {
    let mut fields = BTreeMap::new();
    for key in COMPARED {
        let value = match block.field(key) {
            Some(field) => field.value.clone(),
            None if LISTS.contains(key) => Value::Array(Vec::new()),
            None if *key == "compose" => Value::String(ATOMIC.to_string()),
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
    use toml::Value;

    use super::{same, same_items};

    fn list(items: &[&str]) -> Value {
        let item = |text: &&str| Value::String((*text).to_string());
        Value::Array(items.iter().map(item).collect())
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
}
