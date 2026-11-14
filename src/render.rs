//! The generated files of a set: the text of each section, and one requirement file for each of
//! its requirements.
//!
//! A section `<rfc>/<section>` renders to `<rendered>/<rfc>/<section>.md`: its title under a
//! heading whose explicit anchor is the section's anchor, its prose, and each requirement under a
//! heading of its own whose explicit anchor is the requirement's identifier, followed by its
//! statement. Each requirement also renders to
//! `<requirements>/<rfc>/<section>/<identifier>.toml`, whose `target` is the section's text at the
//! requirement's anchor and whose one `[[spec]]` holds the requirement's level and, as its quote,
//! the statement alone. `<rendered>` and `<requirements>` are the paths the set's configuration
//! gives, and nothing is generated outside them.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::{ByteVec, Finding, Severity, Tree};

/// The rule of the findings [`drift`] reports.
pub const DRIFT: &str = "drift";

const MISSING: &str = "the generated file is missing: `partimento render` writes it";
const CHANGED: &str = "the file differs from what `partimento render` writes, from this line on";
const STALE: &str = "no section generates this file: `partimento render` removes it";

/// The two paths the generated files go under, from the set's configuration.
#[derive(Clone, Copy, Debug)]
pub struct Paths<'a> {
    pub rendered: &'a str,
    pub requirements: &'a str,
}

/// A section to render.
#[derive(Clone, Debug)]
pub struct Section<'a> {
    /// The RFC's directory name.
    pub rfc: &'a str,
    /// The section file's name without its extension.
    pub name: &'a str,
    pub anchor: &'a str,
    pub title: &'a str,
    pub prose: &'a str,
    pub requirements: Vec<Requirement<'a>>,
}

/// A requirement of a section to render.
#[derive(Clone, Debug)]
pub struct Requirement<'a> {
    pub id: &'a str,
    pub level: &'a str,
    pub statement: &'a str,
}

/// The generated files, and the identifier of each requirement file among them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Generated {
    pub tree: Tree,
    /// The identifier of each requirement file, by its path.
    pub requirements: BTreeMap<String, String>,
}

/// Renders every section, in the order given.
pub fn generate(paths: Paths<'_>, sections: &[Section<'_>]) -> Generated {
    let mut generated = Generated::default();
    for section in sections {
        let name = format!("{}.md", section.name);
        let text_path = join(paths.rendered, &[section.rfc, name.as_str()]);
        for requirement in &section.requirements {
            let file = format!("{}.toml", requirement.id);
            let parts = [section.rfc, section.name, file.as_str()];
            let path = join(paths.requirements, &parts);
            let bytes = ByteVec::from(requirement_file(&text_path, requirement));
            generated.tree.files.insert(path.clone(), bytes);
            let id = requirement.id.to_owned();
            generated.requirements.insert(path, id);
        }
        let bytes = ByteVec::from(text(section));
        generated.tree.files.insert(text_path, bytes);
    }
    generated
}

/// The text of a section.
pub fn text(section: &Section<'_>) -> String {
    let mut text = String::new();
    heading(&mut text, "##", section.title.trim(), section.anchor);
    paragraph(&mut text, section.prose);
    for requirement in &section.requirements {
        heading(&mut text, "###", requirement.id, requirement.id);
        paragraph(&mut text, requirement.statement);
    }
    text
}

/// The requirement file of a requirement whose section's text is at `text_path`.
pub fn requirement_file(text_path: &str, requirement: &Requirement<'_>) -> String {
    let mut file = String::from("target = ");
    basic_string(&mut file, &format!("{text_path}#{}", requirement.id));
    file.push_str("\n\n[[spec]]\nlevel = ");
    basic_string(&mut file, requirement.level);
    file.push_str("\nquote = ");
    basic_string(&mut file, requirement.statement.trim());
    file.push('\n');
    file
}

/// The drift of a tree from its generated files, in the order of their paths: each generated file
/// the tree doesn't hold byte for byte, and each file under the generated paths that no section
/// generates.
pub fn drift(paths: Paths<'_>, tree: &Tree, generated: &Generated) -> Vec<Finding> {
    let mut findings = BTreeMap::new();
    for (path, expected) in &generated.tree.files {
        let requirement = generated.requirements.get(path).cloned();
        let found = match tree.files.get(path) {
            None => finding(path, 1, requirement, MISSING),
            Some(actual) if actual != expected => {
                let line = first_difference(&actual.copy_to_bytes(), &expected.copy_to_bytes());
                finding(path, line, requirement, CHANGED)
            }
            Some(_) => continue,
        };
        findings.insert(path.as_str(), found);
    }
    for path in stale(paths, tree, &generated.tree) {
        findings.insert(path, finding(path, 1, None, STALE));
    }
    findings.into_values().collect()
}

/// The files of a tree under the generated paths that the generated files don't hold, which a
/// render removes.
pub fn stale<'t>(paths: Paths<'_>, tree: &'t Tree, generated: &Tree) -> Vec<&'t str> {
    tree.files
        .keys()
        .filter(|path| is_generated(paths, path) && !generated.files.contains_key(*path))
        .map(String::as_str)
        .collect()
}

/// Whether a path is under one of the generated paths.
pub fn is_generated(paths: Paths<'_>, path: &str) -> bool {
    under(paths.rendered, path) || under(paths.requirements, path)
}

/// Joins a configured path and the parts under it with `/`.
fn join(base: &str, parts: &[&str]) -> String {
    let mut path = base.trim_end_matches('/').to_owned();
    for part in parts {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(part);
    }
    path
}

/// Whether a path is under a configured path.
fn under(base: &str, path: &str) -> bool {
    let base = base.trim_end_matches('/');
    let rest = path.strip_prefix(base);
    base.is_empty() || rest.is_some_and(|rest| rest.starts_with('/'))
}

/// Adds a heading with an explicit anchor, after a blank line where text comes before it.
fn heading(text: &mut String, level: &str, title: &str, anchor: &str) {
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(level);
    text.push(' ');
    text.push_str(title);
    text.push_str(" {#");
    text.push_str(anchor);
    text.push_str("}\n");
}

/// Adds a paragraph after a blank line, where the body holds any text.
fn paragraph(text: &mut String, body: &str) {
    let body = body.trim();
    if !body.is_empty() {
        text.push('\n');
        text.push_str(body);
        text.push('\n');
    }
}

/// Adds a value as a TOML basic string.
fn basic_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The line, from 1, at which two files first differ.
fn first_difference(actual: &[u8], expected: &[u8]) -> u32 {
    let at = actual
        .iter()
        .zip(expected)
        .position(|(a, e)| a != e)
        .unwrap_or(actual.len().min(expected.len()));
    let lines = expected[..at].iter().filter(|&&b| b == b'\n').count();
    u32::try_from(lines + 1).unwrap_or(u32::MAX)
}

fn finding(file: &str, line: u32, requirement: Option<String>, message: &str) -> Finding {
    Finding {
        severity: Severity::Error,
        rule: DRIFT.to_owned(),
        file: file.to_owned(),
        line,
        requirement,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATHS: Paths<'static> = Paths {
        rendered: "rendered",
        requirements: ".duvet/requirements",
    };

    const TEXT_PATH: &str = "rendered/0001-greeting/01-hello.md";
    const FIRST_PATH: &str = ".duvet/requirements/0001-greeting/01-hello/EX-0001-001.toml";
    const SECOND_PATH: &str = ".duvet/requirements/0001-greeting/01-hello/EX-0001-002.toml";

    const TEXT: &str = concat!(
        "## Saying hello {#hello}\n",
        "\n",
        "A greeting opens each exchange.\n",
        "\n",
        "### EX-0001-001 {#EX-0001-001}\n",
        "\n",
        "A greeter MUST answer each greeting.\n",
        "\n",
        "### EX-0001-002 {#EX-0001-002}\n",
        "\n",
        "A greeter SHOULD answer within a \"short\" time.\n",
    );

    const FIRST: &str = concat!(
        "target = \"rendered/0001-greeting/01-hello.md#EX-0001-001\"\n",
        "\n",
        "[[spec]]\n",
        "level = \"MUST\"\n",
        "quote = \"A greeter MUST answer each greeting.\"\n",
    );

    const SECOND: &str = concat!(
        "target = \"rendered/0001-greeting/01-hello.md#EX-0001-002\"\n",
        "\n",
        "[[spec]]\n",
        "level = \"SHOULD\"\n",
        "quote = \"A greeter SHOULD answer within a \\\"short\\\" time.\"\n",
    );

    fn section() -> Section<'static> {
        Section {
            rfc: "0001-greeting",
            name: "01-hello",
            anchor: "hello",
            title: "Saying hello",
            prose: "\nA greeting opens each exchange.\n",
            requirements: vec![
                Requirement {
                    id: "EX-0001-001",
                    level: "MUST",
                    statement: "A greeter MUST answer each greeting.",
                },
                Requirement {
                    id: "EX-0001-002",
                    level: "SHOULD",
                    statement: "A greeter SHOULD answer within a \"short\" time.\n",
                },
            ],
        }
    }

    fn file(tree: &Tree, path: &str) -> String {
        let bytes = tree.files[path].copy_to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn a_section_renders_its_title_prose_and_one_heading_for_each_requirement() {
        assert_eq!(text(&section()), TEXT);
    }

    #[test]
    fn a_section_without_prose_renders_its_requirements_after_its_title() {
        let mut section = section();
        section.prose = " \n";
        section.requirements.truncate(1);
        let expected = concat!(
            "## Saying hello {#hello}\n",
            "\n",
            "### EX-0001-001 {#EX-0001-001}\n",
            "\n",
            "A greeter MUST answer each greeting.\n",
        );
        assert_eq!(text(&section), expected);
    }

    #[test]
    fn each_requirement_file_targets_its_own_anchor_and_quotes_its_statement_alone() {
        let section = section();
        let first = requirement_file(TEXT_PATH, &section.requirements[0]);
        let second = requirement_file(TEXT_PATH, &section.requirements[1]);
        assert_eq!(first, FIRST);
        assert_eq!(second, SECOND);
    }

    #[test]
    fn a_requirement_file_reads_back_as_the_statement() {
        let source = requirement_file(TEXT_PATH, &section().requirements[1]);
        let file: toml::Table = toml::from_str(&source).unwrap();
        let target = "rendered/0001-greeting/01-hello.md#EX-0001-002";
        assert_eq!(file["target"].as_str(), Some(target));
        assert_eq!(file["spec"][0]["level"].as_str(), Some("SHOULD"));
        let quote = "A greeter SHOULD answer within a \"short\" time.";
        assert_eq!(file["spec"][0]["quote"].as_str(), Some(quote));
    }

    #[test]
    fn control_characters_are_escaped() {
        let mut out = String::new();
        basic_string(&mut out, "a\\b\tc\u{1}");
        assert_eq!(out, "\"a\\\\b\\tc\\u0001\"");
    }

    #[test]
    fn the_generated_files_are_the_text_and_one_file_for_each_requirement() {
        let generated = generate(PATHS, &[section()]);
        let paths: Vec<&str> = generated.tree.files.keys().map(String::as_str).collect();
        assert_eq!(paths, [FIRST_PATH, SECOND_PATH, TEXT_PATH]);
        assert_eq!(file(&generated.tree, TEXT_PATH), TEXT);
        assert_eq!(file(&generated.tree, FIRST_PATH), FIRST);
        assert_eq!(file(&generated.tree, SECOND_PATH), SECOND);
        assert_eq!(generated.requirements[FIRST_PATH], "EX-0001-001");
        assert_eq!(generated.requirements[SECOND_PATH], "EX-0001-002");
        assert!(!generated.requirements.contains_key(TEXT_PATH));
    }

    #[test]
    fn a_configured_path_with_a_trailing_slash_generates_the_same_paths() {
        let paths = Paths {
            rendered: "rendered/",
            requirements: ".duvet/requirements/",
        };
        let slashed = generate(paths, &[section()]);
        assert_eq!(slashed, generate(PATHS, &[section()]));
    }

    #[test]
    fn generating_twice_gives_the_same_bytes() {
        let first = generate(PATHS, &[section()]);
        assert_eq!(first, generate(PATHS, &[section()]));
    }

    fn put(tree: &mut Tree, path: &str, text: impl Into<ByteVec>) {
        tree.files.insert(path.to_owned(), text.into());
    }

    fn tree(generated: &Generated) -> Tree {
        let mut tree = generated.tree.clone();
        put(&mut tree, "rfcs/0001-greeting/01-hello.toml", "\n");
        put(&mut tree, ".duvet/config.toml", "\n");
        tree
    }

    #[test]
    fn a_tree_that_holds_its_generated_files_has_no_drift() {
        let generated = generate(PATHS, &[section()]);
        let tree = tree(&generated);
        assert!(drift(PATHS, &tree, &generated).is_empty());
        assert!(stale(PATHS, &tree, &generated.tree).is_empty());
    }

    #[test]
    fn a_hand_edit_of_a_rendered_file_is_drift_at_its_line() {
        let generated = generate(PATHS, &[section()]);
        let mut tree = tree(&generated);
        let edited = TEXT.replace("each greeting", "every greeting");
        put(&mut tree, TEXT_PATH, edited);
        let findings = drift(PATHS, &tree, &generated);
        assert_eq!(findings, [finding(TEXT_PATH, 7, None, CHANGED)]);
    }

    #[test]
    fn a_hand_edit_of_a_requirement_file_is_drift_of_its_requirement() {
        let generated = generate(PATHS, &[section()]);
        let mut tree = tree(&generated);
        let edited = FIRST.replace("MUST\"", "MAY\"");
        put(&mut tree, FIRST_PATH, edited);
        let findings = drift(PATHS, &tree, &generated);
        let requirement = Some("EX-0001-001".to_owned());
        assert_eq!(findings, [finding(FIRST_PATH, 4, requirement, CHANGED)]);
    }

    #[test]
    fn a_file_that_ends_early_is_drift_at_the_line_after_its_end() {
        let generated = generate(PATHS, &[section()]);
        let mut tree = tree(&generated);
        put(&mut tree, TEXT_PATH, &TEXT[..TEXT.len() - 1]);
        assert_eq!(first_difference(b"a\nb", b"a\nb\n"), 2);
        assert_eq!(first_difference(b"a\nb\n", b"a\nb\n\n"), 3);
        let findings = drift(PATHS, &tree, &generated);
        assert_eq!(findings, [finding(TEXT_PATH, 11, None, CHANGED)]);
    }

    #[test]
    fn a_missing_generated_file_is_drift() {
        let generated = generate(PATHS, &[section()]);
        let mut tree = tree(&generated);
        tree.files.remove(SECOND_PATH);
        let findings = drift(PATHS, &tree, &generated);
        let requirement = Some("EX-0001-002".to_owned());
        assert_eq!(findings, [finding(SECOND_PATH, 1, requirement, MISSING)]);
    }

    #[test]
    fn a_generated_file_whose_section_is_gone_is_drift_and_stale() {
        let generated = generate(PATHS, &[section()]);
        let mut tree = tree(&generated);
        let gone = "rendered/0002-farewell/01-goodbye.md";
        let gone_file = ".duvet/requirements/0002-farewell/01-goodbye/EX-0002-001.toml";
        put(&mut tree, gone, "\n");
        put(&mut tree, gone_file, "\n");
        assert_eq!(stale(PATHS, &tree, &generated.tree), [gone_file, gone]);
        let findings = drift(PATHS, &tree, &generated);
        let expected = [
            finding(gone_file, 1, None, STALE),
            finding(gone, 1, None, STALE),
        ];
        assert_eq!(findings, expected);
    }

    #[test]
    fn only_the_generated_paths_are_generated() {
        assert!(is_generated(PATHS, TEXT_PATH));
        assert!(is_generated(PATHS, FIRST_PATH));
        assert!(!is_generated(PATHS, ".duvet/config.toml"));
        assert!(!is_generated(PATHS, "rendered.md"));
        assert!(!is_generated(PATHS, "rfcs/0001-greeting/01-hello.toml"));
    }
}
