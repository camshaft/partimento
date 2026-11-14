//! Tests of the format rules and of the command's output that the fixtures of tests/fixtures.rs
//! don't reach: the form and the retirement of identifiers, the type and the presence of fields,
//! the checks of spec.toml, the JSON output's `null` and empty array, and a file the command
//! can't read.
//!
//! The library's tests build each tree in memory from files embedded at compile time, so they
//! read no file and run on wasm32 too.

use partimento::{ByteVec, Options, Report, Tree};

const SPEC: &str = include_str!("fixtures/format-rules/spec.toml");
const INDEX: &str = include_str!("fixtures/format-rules/rfcs/index.toml");
/// An rfc.toml that retires `EX-0001-003` at line 6, an identifier of RFC 2 at line 7, and
/// `EX-9`, which isn't of the identifier's form, at line 8.
const META: &str = include_str!("fixtures/format-rules/rfcs/0001-greetings/rfc.toml");
const GREETING: &str = include_str!("fixtures/format-rules/rfcs/0001-greetings/01-greeting.toml");
/// A section file whose `prose` and four requirement blocks each break one format rule.
const BROKEN: &str = include_str!("fixtures/format-rules/rfcs/0001-greetings/02-broken.toml");
/// The findings the check prints for the fixture format-rules.
const EXPECTED: &str = include_str!("fixtures/format-rules/expected.txt");
/// An rfc.toml that retires nothing.
const CLEAN_META: &str = include_str!("fixtures/05-key-word/rfcs/0001-greetings/rfc.toml");

/// The files of the fixture format-rules, by their paths in its tree.
const FILES: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    ("rfcs/index.toml", INDEX),
    ("rfcs/0001-greetings/rfc.toml", META),
    ("rfcs/0001-greetings/01-greeting.toml", GREETING),
    ("rfcs/0001-greetings/02-broken.toml", BROKEN),
];

/// A tree that breaks no rule: the fixture's files without 02-broken.toml, and an rfc.toml that
/// retires nothing.
const CLEAN: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    ("rfcs/index.toml", INDEX),
    ("rfcs/0001-greetings/rfc.toml", CLEAN_META),
    ("rfcs/0001-greetings/01-greeting.toml", GREETING),
];

/// spec.toml's first two lines, which each case of `RANGE_CASES` follows.
const TOP: &str = "prefix = 'EX'\nlevels = ['MUST']";

/// spec.toml's lines before its `[paths]` table, in each case that breaks a check of the prefix
/// or the levels, at line 1 or 2.
const KEY_CASES: &[&str] = &[
    "prefix = 'EX-A'\nlevels = ['MUST']",
    "prefix = ''\nlevels = ['MUST']",
    "prefix = 'EX'\nlevels = []",
    "prefix = 'EX'\nlevels = ['MUST', 'must']",
    "prefix = 'EX'\nlevels = ['MUST', 'NOT']",
    "prefix = 'EX'\nlevels = ['MUST', 'MUST']",
];

/// The lines after `TOP` in each case that breaks a check of the RFC numbers of a system or of
/// the contracts, at line 3 or 4.
const RANGE_CASES: &[&str] = &[
    "system = [{ name = 'a', first = 9, last = 1 }]",
    "system = [{ name = 'a', first = 1, last = 9 }, { name = 'b', first = 5, last = 15 }]",
    "system = [{ name = 'a', first = 1, last = 9 }]\ncontracts = { first = 5, last = 19 }",
];

/// The finding of each case of `KEY_CASES`, and then of each case of `RANGE_CASES`.
const CONFIG_FINDINGS: &[&str] = &[
    "error[config] spec.toml:1: the prefix `EX-A` isn't ASCII letters and digits",
    "error[config] spec.toml:1: the prefix `` isn't ASCII letters and digits",
    "error[config] spec.toml:2: `levels` doesn't list a key word",
    "error[config] spec.toml:2: the level `must` isn't an uppercase word other than NOT",
    "error[config] spec.toml:2: the level `NOT` isn't an uppercase word other than NOT",
    "error[config] spec.toml:2: `levels` lists `MUST` twice",
    "error[config] spec.toml:3: the RFC numbers of system `a` run from 9 down to 1",
    "error[config] spec.toml:3: the RFC numbers of system `b` overlap those of system `a`",
    "error[config] spec.toml:4: the RFC numbers of the contracts overlap those of system `a`",
];

/// spec.toml's `[paths]` table, which each configuration case ends with.
const PATHS: &str = "
[paths]
rfcs = 'rfcs'
rendered = 'rendered'
requirements = 'requirements'
";

/// A finding's rule, file, line and requirement.
type Place<'a> = (&'a str, &'a str, u32, Option<&'a str>);

fn tree(files: &[(&str, &str)]) -> Tree {
    let mut tree = Tree::default();
    for (path, text) in files {
        let bytes = ByteVec::from(text.to_string());
        tree.files.insert(path.to_string(), bytes);
    }
    tree
}

fn check(files: &[(&str, &str)]) -> Report {
    partimento::check_default(&tree(files), &Options::default())
}

/// The findings as the command prints them, one a line.
fn lines(report: &Report) -> String {
    let mut text = String::new();
    for finding in &report.findings {
        text.push_str(&finding.to_string());
        text.push('\n');
    }
    text
}

/// Each finding's rule, file, line and requirement, in the report's order.
fn places(report: &Report) -> Vec<Place<'_>> {
    let mut places = Vec::new();
    for finding in &report.findings {
        let (rule, file) = (finding.rule.as_str(), finding.file.as_str());
        let requirement = finding.requirement.as_deref();
        places.push((rule, file, finding.line, requirement));
    }
    places
}

#[test]
fn each_identifier_and_field_finding_is_at_its_place() {
    let report = check(FILES);
    assert_eq!(lines(&report), EXPECTED);
    assert!(report.failed());
    let (meta, broken) = (FILES[2].0, FILES[4].0);
    let expected: [Place; 8] = [
        ("field-value", broken, 6, None),
        ("identifier-form", broken, 9, Some("EX-1-1")),
        ("identifier-retired", broken, 15, Some("EX-0001-003")),
        ("field-missing", broken, 20, Some("EX-0001-004")),
        ("field-value", broken, 27, Some("EX-0001-005")),
        ("field-value", broken, 30, Some("EX-0001-005")),
        ("identifier-retired", meta, 7, Some("EX-0002-001")),
        ("identifier-form", meta, 8, Some("EX-9")),
    ];
    assert_eq!(places(&report), expected);
}

#[test]
fn each_configuration_case_fails_at_its_line() {
    let key_cases = KEY_CASES.iter().map(|case| case.to_string());
    let range_cases = RANGE_CASES.iter().map(|case| format!("{TOP}\n{case}"));
    let mut found = Vec::new();
    for keys in key_cases.chain(range_cases) {
        let spec = format!("{keys}\n{PATHS}");
        let report = check(&[("spec.toml", spec.as_str())]);
        assert_eq!(report.findings.len(), 1, "{keys}\n{}", lines(&report));
        let finding = &report.findings[0];
        assert!(finding.requirement.is_none(), "{keys}");
        assert!(report.failed(), "{keys}");
        found.push(finding.to_string());
    }
    assert_eq!(found, CONFIG_FINDINGS);
}

#[test]
fn a_tree_that_breaks_no_rule_has_no_finding() {
    let report = check(CLEAN);
    assert!(report.findings.is_empty(), "{}", lines(&report));
    assert!(!report.failed());
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use super::{CLEAN, EXPECTED};

    fn root(name: &str) -> PathBuf {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests/fixtures").join(name)
    }

    fn run(args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).args(args).output();
        output.expect("the command runs")
    }

    /// An empty directory under the build's temporary directory, for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    /// Writes the files into `dir`, each at its path in the tree.
    fn write(dir: &Path, files: &[(&str, &str)]) {
        for (path, text) in files {
            let file = dir.join(path);
            let parent = file.parent().expect("the file is in a directory");
            std::fs::create_dir_all(parent).expect("the directory is made");
            std::fs::write(file, text).expect("the file is written");
        }
    }

    #[test]
    fn the_command_prints_the_identifier_and_field_findings() {
        let dir = root("format-rules");
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn json_writes_null_where_a_finding_is_about_no_requirement() {
        let dir = root("13-section-field");
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir, "--format", "json"]);
        let expected = include_str!("fixtures/13-section-field/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn a_tree_that_breaks_no_rule_prints_nothing_and_an_empty_array() {
        let dir = scratch("format-rules-clean");
        write(&dir, CLEAN);
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["check", "--root", dir, "--format", "json"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "[]\n");
        assert_eq!(output.status.code(), Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_section_file_exits_with_2() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("format-rules-unreadable");
        write(&dir, CLEAN);
        let file = dir.join("rfcs/0001-greetings/01-greeting.toml");
        std::fs::set_permissions(&file, Permissions::from_mode(0o000)).expect("the mode is set");
        // A process that reads every file, such as one run as root, can't see the refusal.
        let readable = std::fs::read(&file).is_ok();
        let output = run(&["check", "--root", dir.to_str().expect("the path is UTF-8")]);
        std::fs::set_permissions(&file, Permissions::from_mode(0o644)).expect("the mode is set");
        if readable {
            // In CI, where the environment sets `CI`, the test has to reach the assertions below.
            let message = "the test process reads a file of mode 000";
            assert!(std::env::var_os("CI").is_none(), "{message}");
            return;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(output.stdout.is_empty(), "{stderr}");
        assert!(stderr.contains("01-greeting.toml: "), "{stderr}");
    }
}
