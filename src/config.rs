//! The set's configuration, read from spec.toml at the root of the tree.
//!
//! The code holds none of these values: the identifier prefix, the paths, the levels, the RFC
//! numbers of each system and of the contracts, and the implementation terms all come from the
//! file.

use std::ops::Range;

use serde::Deserialize;
use toml::Spanned;

use crate::{source, Finding, Tree};

/// The path of the configuration in the tree.
pub const FILE: &str = "spec.toml";

/// The rule of a configuration that is missing or holds a value the checks can't use.
pub const RULE: &str = "config";

/// The set's configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The prefix of each requirement identifier, such as `EX` in `EX-0001-001`.
    pub prefix: String,
    pub paths: Paths,
    /// The key words of the statements, such as `MUST`, `SHOULD` and `MAY`.
    pub levels: Vec<String>,
    pub systems: Vec<System>,
    /// The RFC numbers of the contracts between the systems.
    pub contracts: Option<NumberRange>,
    /// The terms that name how a system is built, which a statement names by their category
    /// instead.
    pub implementation_terms: Vec<String>,
}

/// The directories of the set, relative to the root of the tree, each with its parts joined by
/// `/` and none of them empty, `.` or `..`, and none of the three the same as another or under
/// another, as `load` checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// The RFC sources: the index and one directory for each RFC.
    pub rfcs: String,
    /// The rendered text of each section.
    pub rendered: String,
    /// The requirement file of each section that the tracing tool reads.
    pub requirements: String,
}

/// A system and the RFC numbers it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct System {
    pub name: String,
    pub rfcs: NumberRange,
}

/// The RFC numbers from `first` to `last`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberRange {
    pub first: u32,
    pub last: u32,
}

impl NumberRange {
    /// Whether the range holds the RFC number.
    pub fn contains(&self, number: u32) -> bool {
        self.first <= number && number <= self.last
    }

    fn overlaps(&self, other: &NumberRange) -> bool {
        self.first <= other.last && other.first <= self.last
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    prefix: Spanned<String>,
    paths: RawPaths,
    levels: Spanned<Vec<String>>,
    #[serde(default, rename = "system")]
    systems: Vec<RawSystem>,
    #[serde(default)]
    contracts: Option<Spanned<NumberRange>>,
    #[serde(default)]
    implementation_terms: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSystem {
    name: Spanned<String>,
    first: u32,
    last: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPaths {
    rfcs: Spanned<String>,
    rendered: Spanned<String>,
    requirements: Spanned<String>,
}

impl RawSystem {
    fn range(&self) -> NumberRange {
        NumberRange {
            first: self.first,
            last: self.last,
        }
    }
}

fn system(raw: RawSystem) -> System {
    System {
        rfcs: raw.range(),
        name: raw.name.into_inner(),
    }
}

/// Whether `path` names a directory of the tree as the library and the command both key it:
/// relative to the root, its parts joined by `/`, none of them empty, `.` or `..`. A path of any
/// other form would let the command read outside its root.
fn in_tree(path: &str) -> bool {
    let plain = |part: &str| !part.is_empty() && part != "." && part != "..";
    !path.contains(['\\', ':']) && path.split('/').all(plain)
}

/// Whether two directories of the tree are the same, or one is under the other.
fn nested(a: &str, b: &str) -> bool {
    a == b || under(a, b) || under(b, a)
}

/// Whether the directory `inner` is under the directory `outer`.
fn under(inner: &str, outer: &str) -> bool {
    let rest = inner.strip_prefix(outer);
    rest.is_some_and(|rest| rest.starts_with('/'))
}

/// Reads the configuration from the tree, or adds the findings that say why it can't.
pub fn load(tree: &Tree, findings: &mut Vec<Finding>) -> Option<Config> {
    let Some(bytes) = tree.files.get(FILE) else {
        let message = "spec.toml, the set's configuration, is missing from the tree's root";
        findings.push(Finding::error(RULE, FILE, 1, None, message));
        return None;
    };
    let text = source::text(FILE, bytes, findings)?;
    let raw: Raw = source::parse(FILE, &text, findings)?;
    let lines = source::Lines::new(text.as_bytes());
    let line = |span: Range<usize>| lines.line(span.start);
    let before = findings.len();

    let prefix = raw.prefix.get_ref();
    if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        let at = line(raw.prefix.span());
        let message = format!("the prefix `{prefix}` isn't ASCII letters and digits");
        findings.push(Finding::error(RULE, FILE, at, None, message));
    }

    let levels = raw.levels.get_ref();
    let at = line(raw.levels.span());
    if levels.is_empty() {
        let message = "`levels` doesn't list a key word";
        findings.push(Finding::error(RULE, FILE, at, None, message));
    }
    for (index, level) in levels.iter().enumerate() {
        let uppercase = level.bytes().all(|byte| byte.is_ascii_uppercase());
        if level.is_empty() || !uppercase || level == "NOT" {
            let message = format!("the level `{level}` isn't an uppercase word other than NOT");
            findings.push(Finding::error(RULE, FILE, at, None, message));
        } else if levels[..index].contains(level) {
            let message = format!("`levels` lists `{level}` twice");
            findings.push(Finding::error(RULE, FILE, at, None, message));
        }
    }

    let paths = &raw.paths;
    let keys = [
        ("rfcs", &paths.rfcs),
        ("rendered", &paths.rendered),
        ("requirements", &paths.requirements),
    ];
    for (index, (key, path)) in keys.iter().enumerate() {
        let value = path.get_ref();
        if !in_tree(value) {
            let message =
                format!("`paths.{key}` is `{value}`, which isn't a directory of the tree");
            findings.push(Finding::error(RULE, FILE, line(path.span()), None, message));
            continue;
        }
        // A file under two of the directories would be read as a source and as a generated file.
        for (other, other_path) in &keys[..index] {
            let other_value = other_path.get_ref();
            if in_tree(other_value) && nested(value, other_value) {
                let message = format!(
                    "`paths.{key}` is `{value}`, which overlaps `paths.{other}`, `{other_value}`"
                );
                findings.push(Finding::error(RULE, FILE, line(path.span()), None, message));
            }
        }
    }

    let mut ranges = Vec::new();
    for system in &raw.systems {
        let name = format!("system `{}`", system.name.get_ref());
        ranges.push((name, system.range(), line(system.name.span())));
    }
    if let Some(contracts) = &raw.contracts {
        let name = "the contracts".to_string();
        ranges.push((name, *contracts.get_ref(), line(contracts.span())));
    }
    for (index, (name, range, at)) in ranges.iter().enumerate() {
        if range.first > range.last {
            let (first, last) = (range.first, range.last);
            let message = format!("the RFC numbers of {name} run from {first} down to {last}");
            findings.push(Finding::error(RULE, FILE, *at, None, message));
        }
        for (other, other_range, _) in &ranges[..index] {
            if range.overlaps(other_range) {
                let message = format!("the RFC numbers of {name} overlap those of {other}");
                findings.push(Finding::error(RULE, FILE, *at, None, message));
            }
        }
    }
    if findings.len() > before {
        return None;
    }

    let systems = raw.systems.into_iter().map(system).collect();
    Some(Config {
        prefix: raw.prefix.into_inner(),
        paths: Paths {
            rfcs: raw.paths.rfcs.into_inner(),
            rendered: raw.paths.rendered.into_inner(),
            requirements: raw.paths.requirements.into_inner(),
        },
        levels: raw.levels.into_inner(),
        systems,
        contracts: raw.contracts.map(Spanned::into_inner),
        implementation_terms: raw.implementation_terms,
    })
}

#[cfg(test)]
mod tests {
    use super::{load, FILE};
    use crate::{ByteVec, Tree};

    /// A configuration whose levels, on line 1, hold a lowercase word, whose prefix, on line 3,
    /// isn't letters and digits, and whose system named on line 12 has RFC numbers that run down.
    const SPEC: &str = r#"levels = ["MUST", "must"]

prefix = "E-X"

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = "requirements"

[[system]]
first = 9
name = "core"
last = 1
"#;

    #[test]
    fn each_finding_is_at_the_line_of_the_value_it_is_about() {
        let mut tree = Tree::default();
        let bytes = ByteVec::from(SPEC.to_string());
        tree.files.insert(FILE.to_string(), bytes);
        let mut findings = Vec::new();
        assert!(load(&tree, &mut findings).is_none());
        let found: Vec<(u32, &str)> = findings
            .iter()
            .map(|finding| (finding.line, finding.message.as_str()))
            .collect();
        let expected = [
            (3, "the prefix `E-X` isn't ASCII letters and digits"),
            (1, "the level `must` isn't an uppercase word other than NOT"),
            (12, "the RFC numbers of system `core` run from 9 down to 1"),
        ];
        assert_eq!(found, expected);
    }
}
