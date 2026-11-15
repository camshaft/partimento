//! The fixtures of the format rules and of the generated files, each checked and rendered through
//! the library and, with the `cli` feature, through the command.
//!
//! The files of each fixture are embedded at compile time, so the library's tests read no file
//! and run on wasm32 too.

#[macro_use]
mod common;

use common::{each_line, fails, lines, tree, tree_of, Fixture};
use partimento::{config, Binding, ByteVec, ChangeKind, Options, Report, Tree, VersionChange};

const FIXTURES: &[Fixture] = &[
    fixture!(
        "01-valid-set",
        "rfcs/0001-greetings/02-reply.toml",
        "rfcs/0002-farewells/rfc.toml",
        "rfcs/0002-farewells/01-farewell.toml",
        "rendered/0001-greetings/01-greeting.md",
        "rendered/0001-greetings/02-reply.md",
        "rendered/0002-farewells/01-farewell.md",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-001.toml",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-002.toml",
        ".duvet/requirements/0001-greetings/02-reply/EX-0001-003.toml",
        ".duvet/requirements/0002-farewells/01-farewell/EX-0002-001.toml"
    ),
    fixture!(
        "02-duplicate-identifier",
        "rfcs/0001-greetings/02-farewell.toml"
    ),
    fixture!("05-key-word"),
    fixture!(
        "11-hand-edit",
        "rendered/0001-greetings/01-greeting.md",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-001.toml",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-002.toml"
    ),
    fixture!("12-unknown-field"),
    fixture!("13-section-field", "rfcs/0001-greetings/02-farewell.toml"),
    fixture!(
        "14-implementation-term",
        "rendered/0001-greetings/01-greeting.md",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-001.toml",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-002.toml",
        ".duvet/requirements/0001-greetings/01-greeting/EX-0001-003.toml"
    ),
    fixture!(
        "section-heading",
        "rfcs/0001-greetings/02-spaced.toml",
        "rfcs/0001-greetings/03-brace.toml"
    ),
];

/// The message of the drift finding of a generated file that no section generates.
const STALE: &str = "no section generates this file: `partimento render` removes it";

/// The fixture of the directory `name`.
fn named(name: &str) -> &'static Fixture {
    let fixture = FIXTURES.iter().find(|fixture| fixture.name == name);
    fixture.expect("the fixture is listed")
}

/// Whether the fixture's expected findings fail a format rule: an error of a rule other than the
/// drift rule, which stops the render.
fn format_fails(fixture: &Fixture) -> bool {
    let format = |line: &str| line.starts_with("error") && !line.starts_with("error[drift]");
    fixture.expected.lines().any(format)
}

/// A fixture's tree split into its sources and its generated files: the files under the
/// directories that spec.toml sets for the rendered texts and the requirement files.
fn split(full: &Tree) -> (Tree, Tree) {
    let loaded = config::load(full, &mut Vec::new());
    let paths = loaded.expect("spec.toml loads").paths;
    let dirs = [paths.rendered, paths.requirements];
    let (mut sources, mut generated) = (Tree::default(), Tree::default());
    for (path, bytes) in &full.files {
        let under = |dir: &String| path.starts_with(&format!("{dir}/"));
        if dirs.iter().any(under) {
            generated.files.insert(path.clone(), bytes.clone());
        } else {
            sources.files.insert(path.clone(), bytes.clone());
        }
    }
    (sources, generated)
}

/// The drift finding of a generated file that no section generates, as the command prints it.
fn stale(path: &str) -> String {
    format!("error[drift] {path}:1: {STALE}\n")
}

/// The files of one side of a diff fixture, `base` or `head`, by their paths in its tree.
macro_rules! diff_tree {
    ($fixture:literal, $side:literal $(, $path:literal)* $(,)?) => {
        &[$((
            $path,
            include_str!(concat!("fixtures/", $fixture, "/", $side, "/", $path)),
        ),)*]
    };
}

/// The base tree of fixture 15-diff.
const DIFF_BASE: &[(&str, &str)] = diff_tree!(
    "15-diff",
    "base",
    "spec.toml",
    "rfcs/index.toml",
    "rfcs/0001-greetings/rfc.toml",
    "rfcs/0001-greetings/01-greeting.toml",
);

/// The head tree of fixture 15-diff, which moves a requirement to 02-farewell.toml.
const DIFF_HEAD: &[(&str, &str)] = diff_tree!(
    "15-diff",
    "head",
    "spec.toml",
    "rfcs/index.toml",
    "rfcs/0001-greetings/rfc.toml",
    "rfcs/0001-greetings/01-greeting.toml",
    "rfcs/0001-greetings/02-farewell.toml",
);

/// The changes the diff of fixture 15-diff's base and head prints, one a line.
const DIFF_EXPECTED: &str = include_str!("fixtures/15-diff/expected.txt");

/// The base tree of fixture 40-contract-version, whose contract RFC 100 is at version 1.
const VERSION_BASE: &[(&str, &str)] = diff_tree!(
    "40-contract-version",
    "base",
    "spec.toml",
    "rfcs/index.toml",
    "rfcs/0100-answers/rfc.toml",
    "rfcs/0100-answers/01-answer.toml",
);

/// The head tree of fixture 40-contract-version, which moves RFC 100 to version 2.
const VERSION_HEAD: &[(&str, &str)] = diff_tree!(
    "40-contract-version",
    "head",
    "spec.toml",
    "rfcs/index.toml",
    "rfcs/0100-answers/rfc.toml",
    "rfcs/0100-answers/01-answer.toml",
);

/// The version change the diff of fixture 40-contract-version's base and head prints.
const VERSION_EXPECTED: &str = include_str!("fixtures/40-contract-version/expected.txt");

/// The failing findings of a fixture's expected findings as a diff prints them with the
/// fixture's tree on `side`, `base` or `head`: each file's path after `<side>:`.
fn with_side(side: &str, expected: &str) -> String {
    let mut text = String::new();
    for line in expected.lines().filter(|line| line.starts_with("error")) {
        let line = line.replacen("] ", &format!("] {side}:"), 1);
        text.push_str(&line);
        text.push('\n');
    }
    text
}

#[test]
fn diff_lists_each_change_of_fixture_15() {
    let (base, head) = (tree_of(DIFF_BASE), tree_of(DIFF_HEAD));
    // The diff reads the format rules alone, so neither tree holds the generated files that the
    // drift rule reads: a tree that renders passes the format rules.
    for tree in [&base, &head] {
        if let Err(findings) = partimento::render(tree) {
            panic!("{}", each_line(&findings));
        }
    }
    let changes = partimento::diff(&base, &head);
    let changes = changes.expect("both trees pass the format rules");
    assert_eq!(each_line(&changes), DIFF_EXPECTED);
}

#[test]
fn diff_lists_the_contract_version_change_of_fixture_40() {
    let (base, head) = (tree_of(VERSION_BASE), tree_of(VERSION_HEAD));
    // Both trees hold the same requirements, so the version change is the diff's only change.
    let changes = partimento::diff(&base, &head);
    let changes = changes.expect("both trees pass the format rules");
    assert!(changes.is_empty(), "{}", each_line(&changes));
    let versions = partimento::contract_versions(&base, &head);
    let versions = versions.expect("both trees pass the format rules");
    let expected = VersionChange {
        rfc: 100,
        base: 1,
        head: 2,
    };
    assert_eq!(versions, [expected]);
    assert_eq!(each_line(&versions), VERSION_EXPECTED);
}

#[test]
fn each_change_of_fixture_15_holds_its_binds_to() {
    let changes = partimento::diff(&tree_of(DIFF_BASE), &tree_of(DIFF_HEAD));
    let changes = changes.expect("both trees pass the format rules");
    let found: Vec<_> = changes
        .iter()
        .map(|c| (c.kind, c.requirement.as_str(), c.binds_to))
        .collect();
    let expected = [
        (ChangeKind::Changed, "EX-0001-003", Binding::System),
        (ChangeKind::Retired, "EX-0001-004", Binding::Process),
        (ChangeKind::Added, "EX-0001-007", Binding::Process),
        (ChangeKind::Added, "EX-0001-008", Binding::System),
        (ChangeKind::Changed, "EX-0001-009", Binding::Process),
        (ChangeKind::Changed, "EX-0001-010", Binding::System),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_tree_against_itself_has_no_change() {
    for files in [DIFF_BASE, DIFF_HEAD] {
        let tree = tree_of(files);
        let changes = partimento::diff(&tree, &tree);
        let changes = changes.expect("the tree passes the format rules");
        assert!(changes.is_empty(), "{}", each_line(&changes));
    }
}

#[test]
fn diff_of_a_failing_tree_returns_its_findings_after_its_side() {
    let fixture = named("02-duplicate-identifier");
    let (failing, passing) = (tree(fixture), tree_of(DIFF_BASE));
    let expected = fixture.expected;
    let base = with_side("base", expected);
    let head = with_side("head", expected);
    assert_eq!(diff_findings(&failing, &passing), base);
    assert_eq!(diff_findings(&passing, &failing), head);
    assert_eq!(diff_findings(&failing, &failing), base + &head);
    let findings = partimento::diff(&passing, &Tree::default());
    let findings = findings.expect_err("a tree without spec.toml fails");
    assert!(!findings.is_empty());
    for finding in &findings {
        assert_eq!(finding.file, "head:spec.toml", "{finding}");
    }
}

/// The findings that the diff of `base` and `head` returns where a tree fails, as the command
/// prints them, one a line.
fn diff_findings(base: &Tree, head: &Tree) -> String {
    let changes = partimento::diff(base, head);
    each_line(&changes.expect_err("a tree fails the format rules"))
}

#[test]
fn diff_with_versions_lists_each_change_of_fixture_15() {
    let (base, head) = (tree_of(DIFF_BASE), tree_of(DIFF_HEAD));
    let found = partimento::diff_with_versions(&base, &head);
    let (changes, versions) = found.expect("both trees pass the format rules");
    assert_eq!(each_line(&changes), DIFF_EXPECTED);
    assert!(versions.is_empty(), "{}", each_line(&versions));
}

#[test]
fn diff_with_versions_of_a_failing_tree_returns_the_findings_of_diff() {
    let fixture = named("02-duplicate-identifier");
    let (failing, passing) = (tree(fixture), tree_of(DIFF_BASE));
    for (base, head) in [(&failing, &passing), (&passing, &failing)] {
        let found = partimento::diff_with_versions(base, head);
        let found = found.expect_err("a tree fails the format rules");
        assert_eq!(each_line(&found), diff_findings(base, head));
    }
}

#[test]
fn each_fixture_holds_its_expected_findings() {
    let mut failures = Vec::new();
    for fixture in FIXTURES {
        let report = partimento::check_default(&tree(fixture), &Options::default());
        let found = lines(&report);
        let (name, expected) = (fixture.name, fixture.expected);
        if found != expected {
            failures.push(format!("{name}:\nexpected:\n{expected}found:\n{found}"));
        }
        if report.failed() != fails(fixture) {
            let message = format!("{name}: the report's failure isn't the expected one");
            failures.push(message);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_final_pass_doesnt_change_the_format_findings() {
    let options = Options { final_pass: true };
    for fixture in FIXTURES {
        let report = partimento::check_default(&tree(fixture), &options);
        assert_eq!(lines(&report), fixture.expected, "{}", fixture.name);
    }
}

#[test]
fn a_tree_without_spec_toml_fails_at_spec_toml() {
    let report = partimento::check_default(&Tree::default(), &Options::default());
    assert!(report.failed());
    let finding = &report.findings[0];
    assert_eq!(finding.rule, "config");
    assert_eq!(finding.file, "spec.toml");
}

#[test]
fn a_valid_set_renders_its_texts_and_requirement_files_alone() {
    let full = tree(named("01-valid-set"));
    let (sources, generated) = split(&full);
    assert_eq!(partimento::render(&sources), Ok(generated.clone()));
    // A render of a tree that holds its generated files already returns the same files.
    assert_eq!(partimento::render(&full), Ok(generated));
}

/// Fixture 01-valid-set with spec.toml's `paths.rendered` set to `out/text` and
/// `paths.requirements` to `out/req`, and its generated files moved there: each requirement
/// file's `target` holds its text's path under `out/text`.
fn valid_set_under_other_paths() -> Tree {
    let mut moved = Tree::default();
    for &(path, text) in named("01-valid-set").files {
        let mut path = path.to_string();
        let mut text = text.to_string();
        if path == config::FILE {
            text = text.replace("\"rendered\"", "\"out/text\"");
            text = text.replace("\".duvet/requirements\"", "\"out/req\"");
        } else if let Some(rest) = path.strip_prefix("rendered/") {
            path = format!("out/text/{rest}");
        } else if let Some(rest) = path.strip_prefix(".duvet/requirements/") {
            path = format!("out/req/{rest}");
            text = text.replace("target = \"rendered/", "target = \"out/text/");
        }
        moved.files.insert(path, ByteVec::from(text));
    }
    moved
}

#[test]
fn a_set_renders_and_checks_under_the_generated_paths_its_spec_toml_sets() {
    let full = valid_set_under_other_paths();
    let loaded = config::load(&full, &mut Vec::new());
    let paths = loaded.expect("spec.toml loads").paths;
    assert_eq!(paths.rendered, "out/text");
    assert_eq!(paths.requirements, "out/req");
    let (sources, generated) = split(&full);
    // The three texts and the four requirement files, each under a path spec.toml sets.
    assert_eq!(generated.files.len(), 7);
    let moved = |path: &String| path.starts_with("out/text/") || path.starts_with("out/req/");
    assert!(generated.files.keys().all(moved));
    // Each `target` the render writes holds the path of its text under `out/text`, as the moved
    // files do, so a render that reads a generated path from anywhere but spec.toml fails here.
    assert_eq!(partimento::render(&sources), Ok(generated.clone()));
    assert_eq!(partimento::render(&full), Ok(generated));
    let report = partimento::check_default(&full, &Options::default());
    assert!(report.findings.is_empty(), "{}", lines(&report));
    assert!(!report.failed());
}

#[test]
fn a_render_ends_the_drift_of_a_hand_edit() {
    let fixture = named("11-hand-edit");
    let mut full = tree(fixture);
    let report = partimento::check_default(&full, &Options::default());
    assert_eq!(lines(&report), fixture.expected);
    assert!(report.failed());
    let generated = partimento::render(&full).expect("the set renders");
    full.files.extend(generated.files);
    let report = partimento::check_default(&full, &Options::default());
    assert!(report.findings.is_empty(), "{}", lines(&report));
}

#[test]
fn a_generated_file_whose_section_is_gone_is_drift() {
    let mut full = tree(named("01-valid-set"));
    full.files.remove("rfcs/0001-greetings/02-reply.toml");
    let text = "rendered/0001-greetings/02-reply.md";
    let requirement = ".duvet/requirements/0001-greetings/02-reply/EX-0001-003.toml";
    let report = partimento::check_default(&full, &Options::default());
    assert_eq!(lines(&report), [stale(requirement), stale(text)].concat());
    assert!(report.failed());
    let (sources, mut generated) = split(&full);
    generated.files.remove(text);
    generated.files.remove(requirement);
    assert_eq!(partimento::render(&sources), Ok(generated));
}

#[test]
fn a_set_whose_format_rules_fail_doesnt_render() {
    let mut names = Vec::new();
    for fixture in FIXTURES {
        match partimento::render(&tree(fixture)) {
            Ok(_) => assert!(!format_fails(fixture), "{}", fixture.name),
            Err(findings) => {
                assert!(format_fails(fixture), "{}", fixture.name);
                let report = Report {
                    findings,
                    ..Report::default()
                };
                assert_eq!(lines(&report), fixture.expected, "{}", fixture.name);
                names.push(fixture.name);
            }
        }
    }
    let failing = [
        "02-duplicate-identifier",
        "05-key-word",
        "12-unknown-field",
        "13-section-field",
        "section-heading",
    ];
    assert_eq!(names, failing);
}

#[test]
fn check_and_render_twice_return_the_same_bytes() {
    let options = Options::default();
    for fixture in FIXTURES {
        let set = tree(fixture);
        let first = partimento::check_default(&set, &options);
        let second = partimento::check_default(&set, &options);
        assert_eq!(first, second, "{}", fixture.name);
        let rendered = partimento::render(&set);
        assert_eq!(rendered, partimento::render(&set), "{}", fixture.name);
    }
}

/// Fixture 05-key-word's tree with `paths.rfcs`, at line 6 of its spec.toml, set to `value`.
fn with_rfcs(value: &str) -> Tree {
    let mut tree = tree(named("05-key-word"));
    let spec = include_str!("fixtures/05-key-word/spec.toml");
    let spec = spec.replace("rfcs = \"rfcs\"", &format!("rfcs = {value:?}"));
    tree.files
        .insert("spec.toml".to_string(), ByteVec::from(spec));
    tree
}

#[test]
fn a_path_that_leaves_the_tree_fails_at_spec_toml() {
    let outside = [
        "", "/", "/home", "..", "../..", "rfcs/..", "./rfcs", "rfcs/", "rfcs//x",
    ];
    let other_forms = ["rfcs\\x", "c:rfcs"];
    for value in outside.iter().chain(other_forms.iter()) {
        let report = partimento::check_default(&with_rfcs(value), &Options::default());
        assert_eq!(report.findings.len(), 1, "{value:?}: {}", lines(&report));
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "config", "{value:?}");
        assert_eq!(finding.file, "spec.toml", "{value:?}");
        assert_eq!(finding.line, 6, "{value:?}");
        assert!(report.failed(), "{value:?}");
    }
    let report = partimento::check_default(&with_rfcs("rfcs"), &Options::default());
    assert_eq!(lines(&report), named("05-key-word").expected);
}

/// Fixture 05-key-word's tree with the `[paths]` key `key` of its spec.toml set to `value`.
fn with_path(key: &str, value: &str) -> Tree {
    let mut tree = tree(named("05-key-word"));
    let spec = include_str!("fixtures/05-key-word/spec.toml");
    let prefix = format!("{key} = ");
    let line = spec.lines().find(|text| text.starts_with(&prefix));
    let line = line.expect("spec.toml sets the key");
    let spec = spec.replace(line, &format!("{key} = {value:?}"));
    tree.files
        .insert("spec.toml".to_string(), ByteVec::from(spec));
    tree
}

#[test]
fn each_path_that_leaves_the_tree_fails_at_its_key() {
    // The lines of fixture 05-key-word's spec.toml that set the three keys.
    for (key, line) in [("rfcs", 6), ("rendered", 7), ("requirements", 8)] {
        let report = partimento::check_default(&with_path(key, "../.."), &Options::default());
        assert_eq!(report.findings.len(), 1, "{key}: {}", lines(&report));
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "config", "{key}");
        assert_eq!(finding.file, "spec.toml", "{key}");
        assert_eq!(finding.line, line, "{key}");
        let message = format!("`paths.{key}` is `../..`");
        assert!(finding.to_string().contains(&message), "{key}: {finding}");
        assert!(report.failed(), "{key}");
    }
}

/// The `[paths]` key of fixture 05-key-word's spec.toml that each case sets, its value, and the
/// line of the case's finding: the line of the later of the two keys whose paths overlap.
const OVERLAP_CASES: &[(&str, &str, u32)] = &[
    ("rendered", "rfcs", 7),
    ("rendered", "rfcs/rendered", 7),
    ("requirements", "rendered/req", 8),
    ("rfcs", "rendered/rfcs", 7),
];

/// The message of each case of `OVERLAP_CASES`.
const OVERLAP_MESSAGES: &[&str] = &[
    "`paths.rendered` is `rfcs`, which overlaps `paths.rfcs`, `rfcs`",
    "`paths.rendered` is `rfcs/rendered`, which overlaps `paths.rfcs`, `rfcs`",
    "`paths.requirements` is `rendered/req`, which overlaps `paths.rendered`, `rendered`",
    "`paths.rendered` is `rendered`, which overlaps `paths.rfcs`, `rendered/rfcs`",
];

#[test]
fn paths_that_overlap_fail_at_the_later_key() {
    let cases = OVERLAP_CASES.iter().zip(OVERLAP_MESSAGES);
    for ((key, value, line), message) in cases {
        let report = partimento::check_default(&with_path(key, value), &Options::default());
        assert_eq!(report.findings.len(), 1, "{key}: {}", lines(&report));
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "config", "{key}");
        assert_eq!(finding.file, "spec.toml", "{key}");
        assert_eq!(finding.line, *line, "{key}");
        assert_eq!(finding.message, *message, "{key}");
        assert!(report.failed(), "{key}");
    }
    // A path whose name begins with another path's name, without a `/` after it, is beside it.
    let beside = with_path("rendered", "rfcs-rendered");
    let report = partimento::check_default(&beside, &Options::default());
    assert_eq!(lines(&report), named("05-key-word").expected);
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use partimento::{ByteVec, Tree};

    use super::{
        fails, named, split, stale, tree, with_side, DIFF_EXPECTED, FIXTURES, VERSION_EXPECTED,
    };

    fn root(name: &str) -> PathBuf {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests/fixtures").join(name)
    }

    fn run(args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).args(args).output();
        output.expect("the command runs")
    }

    #[test]
    fn each_fixture_prints_its_expected_findings() {
        for fixture in FIXTURES {
            let dir = root(fixture.name);
            let dir = dir.to_str().expect("the path is UTF-8");
            let output = run(&["check", "--root", dir]);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert_eq!(stdout, fixture.expected, "{}", fixture.name);
            let code = i32::from(fails(fixture));
            assert_eq!(output.status.code(), Some(code), "{}", fixture.name);
        }
    }

    #[test]
    fn json_lists_the_findings() {
        let dir = root("02-duplicate-identifier");
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir, "--format", "json"]);
        let expected = include_str!("fixtures/02-duplicate-identifier/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn wrong_arguments_exit_with_2() {
        assert_eq!(run(&[]).status.code(), Some(2));
        assert_eq!(run(&["render", "--final"]).status.code(), Some(2));
        assert_eq!(run(&["check", "--format", "yaml"]).status.code(), Some(2));
        assert_eq!(run(&["check", "--root"]).status.code(), Some(2));
        assert_eq!(run(&["check", "--strict"]).status.code(), Some(2));
    }

    #[test]
    fn a_missing_root_exits_with_2() {
        let dir = root("no-such-fixture");
        let dir = dir.to_str().expect("the path is UTF-8");
        assert_eq!(run(&["check", "--root", dir]).status.code(), Some(2));
    }

    /// The directory of one side of fixture 15-diff, `base` or `head`.
    fn diff_root(side: &str) -> String {
        let dir = root("15-diff").join(side);
        dir.to_str().expect("the path is UTF-8").to_string()
    }

    #[test]
    fn diff_prints_the_changes_of_fixture_15() {
        let (base, head) = (diff_root("base"), diff_root("head"));
        let (base, head) = (base.as_str(), head.as_str());
        let output = run(&["diff", base, head]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), DIFF_EXPECTED);
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["diff", base, head, "--format", "json"]);
        let expected = include_str!("fixtures/15-diff/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["diff", "--format", "json", base, base]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "[]\n");
        assert_eq!(output.status.code(), Some(0));
    }

    #[test]
    fn diff_prints_the_contract_version_change_of_fixture_40() {
        let fixture = root("40-contract-version");
        let (base, head) = (fixture.join("base"), fixture.join("head"));
        let base = base.to_str().expect("the path is UTF-8");
        let head = head.to_str().expect("the path is UTF-8");
        let output = run(&["diff", base, head]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), VERSION_EXPECTED);
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["diff", base, head, "--format", "json"]);
        let expected = include_str!("fixtures/40-contract-version/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(0));
    }

    #[test]
    fn diff_of_a_failing_tree_prints_its_findings_and_exits_with_1() {
        let fixture = named("02-duplicate-identifier");
        let failing = root(fixture.name);
        let failing = failing.to_str().expect("the path is UTF-8");
        let head = diff_root("head");
        let output = run(&["diff", failing, head.as_str()]);
        let expected = with_side("base", fixture.expected);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn wrong_diff_arguments_exit_with_2() {
        let (base, head) = (diff_root("base"), diff_root("head"));
        let (base, head) = (base.as_str(), head.as_str());
        let code = |args: &[&str]| run(args).status.code();
        assert_eq!(code(&["diff"]), Some(2));
        assert_eq!(code(&["diff", base]), Some(2));
        assert_eq!(code(&["diff", base, head, head]), Some(2));
        assert_eq!(code(&["diff", base, head, "--format", "yaml"]), Some(2));
        assert_eq!(code(&["diff", base, head, "--final"]), Some(2));
        let missing = root("no-such-fixture");
        let missing = missing.to_str().expect("the path is UTF-8");
        assert_eq!(code(&["diff", base, missing]), Some(2));
    }

    /// An empty directory under the build's temporary directory, for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    /// Writes the tree's files into `dir`, each at its path in the tree.
    fn write_tree(dir: &Path, tree: &Tree) {
        for (path, bytes) in &tree.files {
            let file = dir.join(path);
            let parent = file.parent().expect("the file is in a directory");
            std::fs::create_dir_all(parent).expect("the directory is made");
            let data = bytes.copy_to_bytes().to_vec();
            std::fs::write(file, data).expect("the file is written");
        }
    }

    /// Writes fixture 05-key-word's files into `dir`.
    fn write_fixture(dir: &Path) {
        write_tree(dir, &tree(named("05-key-word")));
    }

    /// Each file under `dir`, by its path relative to `dir`, with `/` between the parts of a path.
    fn files_under(dir: &Path) -> Tree {
        let mut found = Tree::default();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in std::fs::read_dir(&next).expect("the directory is read") {
                let path = entry.expect("the entry is read").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let relative = path.strip_prefix(dir).expect("the file is in dir");
                let name = relative.to_str().expect("the path is UTF-8");
                let name = name.replace('\\', "/");
                let bytes = std::fs::read(&path).expect("the file is read");
                found.files.insert(name, ByteVec::from(bytes));
            }
        }
        found
    }

    #[test]
    fn render_writes_the_generated_files_and_removes_the_stale_ones() {
        let full = tree(named("01-valid-set"));
        let (mut written, _) = split(&full);
        let gone = "rendered/0003-gone/01-gone.md";
        let text = ByteVec::from("## Gone {#gone}\n".to_string());
        written.files.insert(gone.to_string(), text);
        // A file under `.duvet` that isn't under the generated directories stays as it is.
        let kept = ".duvet/config.toml";
        let config = ByteVec::from("[report]\n".to_string());
        written.files.insert(kept.to_string(), config.clone());
        let dir = scratch("render-valid-set");
        write_tree(&dir, &written);
        let root = dir.to_str().expect("the path is UTF-8");
        let mut expected = full;
        expected.files.insert(kept.to_string(), config);
        // The second render finds each file as the first one wrote it, and changes nothing.
        for _ in 0..2 {
            let output = run(&["render", "--root", root]);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(0), "{stderr}");
            assert!(output.stdout.is_empty(), "{stderr}");
            assert!(!dir.join("rendered/0003-gone").exists());
            assert_eq!(files_under(&dir), expected);
        }
        let output = run(&["check", "--root", root]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
    }

    #[test]
    fn render_of_a_set_that_fails_prints_its_findings_and_writes_nothing() {
        let fixture = named("02-duplicate-identifier");
        let dir = scratch("render-failing-set");
        write_tree(&dir, &tree(fixture));
        let root = dir.to_str().expect("the path is UTF-8");
        let output = run(&["render", "--root", root]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), fixture.expected);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(files_under(&dir), tree(fixture));
        let output = run(&["render", "--root", root, "--format", "json"]);
        let expected = include_str!("fixtures/02-duplicate-identifier/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(files_under(&dir), tree(fixture));
    }

    #[test]
    fn render_rewrites_a_hand_edit() {
        let fixture = named("11-hand-edit");
        let dir = scratch("render-hand-edit");
        write_tree(&dir, &tree(fixture));
        let root = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", root]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), fixture.expected);
        assert_eq!(output.status.code(), Some(1));
        let output = run(&["render", "--root", root]);
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["check", "--root", root]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
    }

    /// `render` exits with 2, and prints the file's path on stderr, where it can't write the
    /// hand-edited text of fixture 11-hand-edit: a file of mode 444, and the same file, removed,
    /// in a directory of mode 555, where the render has to create it again.
    #[cfg(unix)]
    #[test]
    fn render_exits_with_2_where_it_cant_write_a_file() {
        use std::fs::{OpenOptions, Permissions};
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("render-unwritable");
        write_tree(&dir, &tree(named("11-hand-edit")));
        let root = dir.to_str().expect("the path is UTF-8");
        let file = dir.join("rendered/0001-greetings/01-greeting.md");
        let file_path = file.to_str().expect("the path is UTF-8");
        let edited = std::fs::read(&file).expect("the file is read");
        let set_mode = |target: &Path, bits: u32| {
            let permissions = Permissions::from_mode(bits);
            std::fs::set_permissions(target, permissions).expect("the mode is set");
        };
        let refused = |output: &Output| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(2), "{stderr}");
            assert!(output.stdout.is_empty(), "{stderr}");
            let holds_the_path = stderr.lines().any(|line| line.contains(file_path));
            assert!(holds_the_path, "{stderr}");
        };

        set_mode(&file, 0o444);
        // A process that writes every file, such as one run as root, can't see the refusal.
        let writable = OpenOptions::new().write(true).open(&file).is_ok();
        let output = run(&["render", "--root", root]);
        set_mode(&file, 0o644);
        if writable {
            // In CI, where the environment sets `CI`, the test has to reach the assertions below.
            let message = "the test process writes a file of mode 444";
            assert!(std::env::var_os("CI").is_none(), "{message}");
            return;
        }
        refused(&output);
        assert_eq!(std::fs::read(&file).expect("the file is read"), edited);

        // A directory of mode 555 doesn't stop a write over a file in it, which only the file's
        // mode stops, but it stops the render from creating the file there.
        std::fs::remove_file(&file).expect("the file is removed");
        let parent = file.parent().expect("the file is in a directory");
        set_mode(parent, 0o555);
        let output = run(&["render", "--root", root]);
        set_mode(parent, 0o755);
        refused(&output);
        assert!(!file.exists());
    }

    #[test]
    fn render_removes_exactly_the_files_the_check_reports_as_stale() {
        let mut written = tree(named("01-valid-set"));
        written.files.remove("rfcs/0001-greetings/02-reply.toml");
        // A file under `.duvet` that isn't under the generated directories isn't stale.
        let config = ByteVec::from("[report]\n".to_string());
        let kept = ".duvet/config.toml".to_string();
        written.files.insert(kept, config);
        let dir = scratch("render-removes-the-stale-files");
        write_tree(&dir, &written);
        let root = dir.to_str().expect("the path is UTF-8");
        let check = run(&["check", "--root", root]);
        assert_eq!(check.status.code(), Some(1));
        let output = run(&["render", "--root", root]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{stderr}");
        // The check reports as stale each file the render removes, and no other file.
        let found = files_under(&dir);
        let mut removed = Vec::new();
        let mut reported = String::new();
        for path in written.files.keys() {
            if !found.files.contains_key(path) {
                removed.push(path.as_str());
                reported.push_str(&stale(path));
            }
        }
        assert_eq!(String::from_utf8_lossy(&check.stdout), reported);
        let expected = [
            ".duvet/requirements/0001-greetings/02-reply/EX-0001-003.toml",
            "rendered/0001-greetings/02-reply.md",
        ];
        assert_eq!(removed, expected);
        let output = run(&["check", "--root", root]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
    }

    /// Runs `check` on `dir`, and asserts that it exits with 2, prints nothing on stdout, and
    /// writes `error` on stderr.
    fn exits_with_2(dir: &Path, error: &str) {
        let output = run(&["check", "--root", dir.to_str().expect("the path is UTF-8")]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(output.stdout.is_empty(), "{stderr}");
        assert!(stderr.contains(error), "{stderr}");
    }

    #[test]
    fn a_path_that_leaves_the_tree_stops_the_read() {
        let dir = scratch("rfcs-outside");
        write_fixture(&dir);
        let spec = std::fs::read_to_string(dir.join("spec.toml")).expect("the file is read");
        let spec = spec.replace("rfcs = \"rfcs\"", "rfcs = \"../..\"");
        std::fs::write(dir.join("spec.toml"), spec).expect("the file is written");
        let output = run(&["check", "--root", dir.to_str().expect("the path is UTF-8")]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("error[config] spec.toml:6: `paths.rfcs`"),
            "{stdout}"
        );
        assert_eq!(stdout.lines().count(), 1, "{stdout}");
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn a_directory_that_is_a_file_exits_with_2() {
        let dir = scratch("rfcs-is-a-file");
        write_fixture(&dir);
        std::fs::remove_dir_all(dir.join("rfcs")).expect("the directory is removed");
        std::fs::write(dir.join("rfcs"), "").expect("the file is written");
        exits_with_2(&dir, "rfcs: ");
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_exits_with_2() {
        use std::os::unix::fs::symlink;

        let dir = scratch("linked-section");
        write_fixture(&dir);
        let section = dir.join("rfcs/0001-greetings/02-link.toml");
        symlink(dir.join("spec.toml"), section).expect("the link is made");
        exits_with_2(&dir, "02-link.toml: a symbolic link");

        let dir = scratch("linked-directory");
        write_fixture(&dir);
        std::fs::rename(dir.join("rfcs"), dir.join("sources")).expect("the directory is moved");
        symlink(dir.join("sources"), dir.join("rfcs")).expect("the link is made");
        exits_with_2(&dir, "rfcs: a symbolic link");

        let dir = scratch("linked-spec");
        write_fixture(&dir);
        std::fs::rename(dir.join("spec.toml"), dir.join("real.toml")).expect("the file is moved");
        symlink(dir.join("real.toml"), dir.join("spec.toml")).expect("the link is made");
        exits_with_2(&dir, "spec.toml: a symbolic link");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_an_earlier_part_of_a_path_exits_with_2() {
        use std::os::unix::fs::symlink;

        let target = scratch("linked-part-target");
        let dir = scratch("linked-part");
        write_fixture(&dir);
        let spec = std::fs::read_to_string(dir.join("spec.toml")).expect("the file is read");
        let spec = spec.replace("rfcs = \"rfcs\"", "rfcs = \"docs/rfcs\"");
        std::fs::write(dir.join("spec.toml"), spec).expect("the file is written");
        std::fs::rename(dir.join("rfcs"), target.join("rfcs")).expect("the directory is moved");
        symlink(&target, dir.join("docs")).expect("the link is made");
        exits_with_2(&dir, "docs: a symbolic link");
    }

    #[cfg(unix)]
    #[test]
    fn an_entry_that_is_neither_a_file_nor_a_directory_exits_with_2() {
        use std::os::unix::net::UnixListener;

        /// Removes the test's tree when the test ends, a failed run's too.
        struct RemovedAtEnd(PathBuf);

        impl Drop for RemovedAtEnd {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        // A socket's path holds about 100 bytes at most, and a deep build directory is longer, so
        // this test's tree is under the system's temporary directory, named for this process.
        // `create_dir` fails where the name is taken already, a link among them, so the test
        // doesn't write into a directory another user chose.
        let name = format!("partimento-socket-entry-{}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).expect("the directory is made");
        let _removed = RemovedAtEnd(dir.clone());
        write_fixture(&dir);
        let _socket = UnixListener::bind(dir.join("rfcs/socket")).expect("the socket is made");
        exits_with_2(&dir, "socket: neither a file nor a directory");
    }
}
