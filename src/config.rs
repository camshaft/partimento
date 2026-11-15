//! The set's configuration, read from spec.toml at the root of the tree.
//!
//! The code holds none of these values: the identifier prefix, the paths, the levels, the name
//! and the RFC numbers of each system, of the contracts and of the foundation, and the
//! implementation terms all come from the file.

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
    /// The group of the contracts between the systems.
    pub contracts: Option<Group>,
    /// The group of the RFCs that every system reads, such as the format of the set itself.
    pub foundation: Option<Group>,
    /// The terms that name how a system is built, which a statement names by their category
    /// instead.
    pub implementation_terms: Vec<String>,
}

impl Config {
    /// Each group the configuration declares: the systems, then the contracts, then the
    /// foundation.
    pub fn groups(&self) -> impl Iterator<Item = &Group> {
        self.systems
            .iter()
            .chain(&self.contracts)
            .chain(&self.foundation)
    }

    /// The group whose name is `name`, where the configuration declares one.
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups().find(|group| group.name == name)
    }

    /// Whether the RFC number is in the contracts' group.
    pub fn is_contract(&self, number: u32) -> bool {
        let contracts = self.contracts.as_ref();
        contracts.is_some_and(|group| group.rfcs.contains(number))
    }
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
    /// The registry of identifier kinds, a file of the tree outside the three directories, where
    /// the configuration states one.
    pub registry: Option<String>,
}

/// A group of RFCs: its name, which an RFC's area holds, and the RFC numbers it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub rfcs: NumberRange,
}

/// A system: a group of the RFCs of one part of the product.
pub type System = Group;

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
    systems: Vec<RawGroup>,
    #[serde(default)]
    contracts: Option<Spanned<RawGroup>>,
    #[serde(default)]
    foundation: Option<Spanned<RawGroup>>,
    #[serde(default)]
    implementation_terms: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGroup {
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
    #[serde(default)]
    registry: Option<Spanned<String>>,
}

impl RawGroup {
    fn range(&self) -> NumberRange {
        NumberRange {
            first: self.first,
            last: self.last,
        }
    }
}

fn group(raw: RawGroup) -> Group {
    Group {
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

/// Whether `inner`, a file or a directory of the tree, is under the directory `outer`, which may
/// end in `/`. Every path is under the empty directory, the root of the tree.
fn under(inner: &str, outer: &str) -> bool {
    let outer = outer.trim_end_matches('/');
    let rest = inner.strip_prefix(outer);
    outer.is_empty() || rest.is_some_and(|rest| rest.starts_with('/'))
}

/// Whether `path`, a file or a directory of the tree, is under the generated directory
/// `rendered` or `requirements`, each of which may end in `/`. The drift rule reports each file
/// under them that the render doesn't generate as stale, and the `render` command removes those
/// same files; both call this, so they can't disagree on which files are generated.
pub fn is_generated(rendered: &str, requirements: &str, path: &str) -> bool {
    under(path, rendered) || under(path, requirements)
}

/// One group as the file declares it, for the checks of its name and its range.
struct Declared<'a> {
    /// How a finding names the group, such as "system `core`" or "the contracts".
    label: String,
    name: &'a str,
    /// The line of its name.
    name_line: u32,
    range: NumberRange,
    /// The line a finding about its range is at.
    range_line: u32,
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
    // A registry under one of the directories would be read as a source or a generated file too.
    if let Some(registry) = &paths.registry {
        let (value, at) = (registry.get_ref(), line(registry.span()));
        if !in_tree(value) {
            let message = format!("`paths.registry` is `{value}`, which isn't a file of the tree");
            findings.push(Finding::error(RULE, FILE, at, None, message));
        } else {
            for (key, path) in &keys {
                let dir = path.get_ref();
                if in_tree(dir) && nested(value, dir) {
                    let message = format!(
                        "`paths.registry` is `{value}`, which overlaps `paths.{key}`, `{dir}`"
                    );
                    findings.push(Finding::error(RULE, FILE, at, None, message));
                }
            }
        }
    }

    let mut groups = Vec::new();
    for system in &raw.systems {
        let at = line(system.name.span());
        groups.push(Declared {
            label: format!("system `{}`", system.name.get_ref()),
            name: system.name.get_ref(),
            name_line: at,
            range: system.range(),
            range_line: at,
        });
    }
    let tables = [
        ("the contracts", &raw.contracts),
        ("the foundation", &raw.foundation),
    ];
    for (label, table) in tables {
        let Some(table) = table else {
            continue;
        };
        let declared = table.get_ref();
        groups.push(Declared {
            label: label.to_string(),
            name: declared.name.get_ref(),
            name_line: line(declared.name.span()),
            range: declared.range(),
            range_line: line(table.span()),
        });
    }
    for (index, group) in groups.iter().enumerate() {
        let (label, range) = (&group.label, group.range);
        if group.name.is_empty() {
            let message = format!("the name of {label} is empty");
            findings.push(Finding::error(RULE, FILE, group.name_line, None, message));
        }
        if range.first > range.last {
            let (first, last) = (range.first, range.last);
            let message = format!("the RFC numbers of {label} run from {first} down to {last}");
            findings.push(Finding::error(RULE, FILE, group.range_line, None, message));
        }
        for other in &groups[..index] {
            let other_label = &other.label;
            if range.overlaps(&other.range) {
                let message = format!("the RFC numbers of {label} overlap those of {other_label}");
                findings.push(Finding::error(RULE, FILE, group.range_line, None, message));
            }
            if !group.name.is_empty() && group.name == other.name {
                let name = group.name;
                let message = format!("{label} has the name `{name}`, which {other_label} has too");
                findings.push(Finding::error(RULE, FILE, group.name_line, None, message));
            }
        }
    }
    if findings.len() > before {
        return None;
    }

    let systems = raw.systems.into_iter().map(group).collect();
    Some(Config {
        prefix: raw.prefix.into_inner(),
        paths: Paths {
            rfcs: raw.paths.rfcs.into_inner(),
            rendered: raw.paths.rendered.into_inner(),
            requirements: raw.paths.requirements.into_inner(),
            registry: raw.paths.registry.map(Spanned::into_inner),
        },
        levels: raw.levels.into_inner(),
        systems,
        contracts: raw.contracts.map(|table| group(table.into_inner())),
        foundation: raw.foundation.map(|table| group(table.into_inner())),
        implementation_terms: raw.implementation_terms,
    })
}

#[cfg(test)]
mod tests {
    use super::{is_generated, load, FILE};
    use crate::{ByteVec, Finding, Tree};

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

    /// A configuration whose groups each hold a name and a range: two systems, the contracts and
    /// the foundation.
    const GROUPS: &str = r#"prefix = "EX"
levels = ["MUST"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = "requirements"

[[system]]
name = "core"
first = 1
last = 99

[[system]]
name = "edge"
first = 200
last = 299

[contracts]
name = "contracts"
first = 100
last = 199

[foundation]
name = "format"
first = 0
last = 0
"#;

    /// The tree that holds `text` as its configuration.
    fn tree(text: &str) -> Tree {
        let mut tree = Tree::default();
        tree.files
            .insert(FILE.to_string(), ByteVec::from(text.to_string()));
        tree
    }

    #[test]
    fn each_group_holds_its_name_and_its_range() {
        let mut findings = Vec::new();
        let config = load(&tree(GROUPS), &mut findings).expect("the configuration loads");
        assert_eq!(findings, []);
        let names: Vec<&str> = config.groups().map(|group| group.name.as_str()).collect();
        assert_eq!(names, ["core", "edge", "contracts", "format"]);
        let format = config.group("format").expect("the foundation's group");
        assert!(format.rfcs.contains(0));
        assert!(config.is_contract(150));
        assert!(!config.is_contract(250));
        assert!(config.group("missing").is_none());
    }

    #[test]
    fn a_group_name_that_another_group_holds_fails_at_its_line() {
        let text = GROUPS.replace("name = \"format\"", "name = \"edge\"");
        let mut findings = Vec::new();
        assert!(load(&tree(&text), &mut findings).is_none());
        let message = "the foundation has the name `edge`, which system `edge` has too";
        assert_eq!(
            findings,
            [Finding::error("config", FILE, 25, None, message)]
        );
    }

    #[test]
    fn a_foundation_range_that_overlaps_a_system_fails_at_its_table() {
        let text = GROUPS.replace("first = 0\nlast = 0", "first = 0\nlast = 1");
        let mut findings = Vec::new();
        assert!(load(&tree(&text), &mut findings).is_none());
        let message = "the RFC numbers of the foundation overlap those of system `core`";
        assert_eq!(
            findings,
            [Finding::error("config", FILE, 24, None, message)]
        );
    }

    #[test]
    fn a_group_without_a_name_doesnt_parse() {
        let text = GROUPS.replace("name = \"contracts\"\n", "");
        let mut findings = Vec::new();
        assert!(load(&tree(&text), &mut findings).is_none());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("name"), "{findings:?}");
    }

    #[test]
    fn a_generated_file_is_one_under_the_rendered_or_the_requirements_directory() {
        let generated = |path: &str| is_generated("rendered", ".duvet/requirements", path);
        assert!(generated("rendered/0001-greeting/01-hello.md"));
        assert!(generated(
            ".duvet/requirements/0001-greeting/01-hello/EX-0001-001.toml"
        ));
        assert!(!generated("rendered"));
        assert!(!generated("rendered.md"));
        assert!(!generated(".duvet/config.toml"));
        assert!(!generated("rfcs/0001-greeting/01-hello.toml"));
        assert!(is_generated("rendered/", "requirements/", "rendered/a.md"));
        assert!(!is_generated("rendered/", "requirements/", "rendered.md"));
        assert!(is_generated("", "requirements", "rfcs/index.toml"));
    }
}
