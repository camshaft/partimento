//! Tests of the format rules and of the command's output that the fixtures of tests/fixtures.rs
//! don't reach: the form and the retirement of identifiers, the type and the presence of fields,
//! the checks of spec.toml, a section file's own sources and retired identifiers, the JSON
//! output's `null` and empty array, and a file the command can't read.
//!
//! The library's tests build each tree in memory from files embedded at compile time, so they
//! read no file and run on wasm32 too.

use partimento::{ByteVec, Finding, Options, Report, Tree};

const SPEC: &str = include_str!("fixtures/format-rules/spec.toml");
const INDEX: &str = include_str!("fixtures/format-rules/rfcs/index.toml");
/// An rfc.toml that retires `EX-0001-003` at line 6, `EX-0002-001`, an identifier of RFC 2 that
/// an RFC may retire, at line 7, and `EX-9`, which isn't of the identifier's form, at line 8.
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

/// The sources of a tree that doesn't break a rule: the fixture's files without 02-broken.toml,
/// and an rfc.toml that retires nothing. `clean` adds the files that `partimento render` generates.
const CLEAN: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    ("rfcs/index.toml", INDEX),
    ("rfcs/0001-greetings/rfc.toml", CLEAN_META),
    ("rfcs/0001-greetings/01-greeting.toml", GREETING),
];

/// A section file that retires `EX-0001-008` and `EX-0002-001`, an identifier of RFC 2, on line
/// 1, and lists its own source, before its `[section]` table.
const RETIRING: &str = r#"retired = ["EX-0001-008", "EX-0002-001"]

[[sources]]
design = "designs/greetings"
version = 2

[section]
anchor = "retiring"
title = "Retiring"
problem = "A greeting the program no longer prints is still written down."
motivation = "A retired identifier keeps a reader from using it again."
"#;

/// A section file whose requirement holds `EX-0001-008`, which `RETIRING` retires, at line 10.
const REUSING: &str = r#"[section]
anchor = "reusing"
title = "Reusing"
problem = "A person who stops the program doesn't know that it stopped."
motivation = "A farewell tells the person that the program stopped."

[[requirement]]
binds_to = "system"
rationale = "The tests need this requirement."
id = "EX-0001-008"
level = "MUST"
polarity = "required"
statement = "The program MUST print a farewell when it stops."
"#;

/// A section file that retires `EX-0001-008` and lists its own source before its `[section]`
/// table, and whose requirement supersedes `EX-0001-008`.
const SUPERSEDING: &str = r#"retired = ["EX-0001-008"]

[[sources]]
design = "designs/greetings"
version = 2

[section]
anchor = "farewell"
title = "Farewell"
problem = "A person who stops the program doesn't know that it stopped."
motivation = "A farewell tells the person that the program stopped."

[[requirement]]
binds_to = "system"
rationale = "The farewell takes the place of the retired one."
id = "EX-0001-002"
level = "MUST"
polarity = "required"
statement = "The program MUST print a farewell when it stops."
relations = [{ kind = "supersedes", target = "EX-0001-008" }]
"#;

/// A section file whose requirement implies `EX-0001-008`, which `RETIRING` retires, in its
/// relations at line 14.
const IMPLYING: &str = r#"[section]
anchor = "implying"
title = "Implying"
problem = "A person who stops the program doesn't know that it stopped."
motivation = "A farewell tells the person that the program stopped."

[[requirement]]
binds_to = "system"
rationale = "The tests need this requirement."
id = "EX-0001-002"
level = "MUST"
polarity = "required"
statement = "The program MUST print a farewell when it stops."
relations = [{ kind = "implies", target = "EX-0001-008" }]
"#;

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
    "system = [{ name = 'a', first = 1, last = 9 }]\n\
     contracts = { name = 'c', first = 5, last = 19 }",
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

/// `CLEAN` with the files that `partimento render` generates for it, which the drift rule asks for.
fn clean() -> Tree {
    let mut clean = tree(CLEAN);
    let generated = partimento::render(&clean).expect("CLEAN renders");
    clean.files.extend(generated.files);
    clean
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
    let expected: [Place; 7] = [
        ("field-value", broken, 6, None),
        ("identifier-form", broken, 11, Some("EX-1-1")),
        ("identifier-retired", broken, 19, Some("EX-0001-003")),
        ("field-missing", broken, 24, Some("EX-0001-004")),
        ("field-value", broken, 35, Some("EX-0001-005")),
        ("field-value", broken, 38, Some("EX-0001-005")),
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
    let report = partimento::check_default(&clean(), &Options::default());
    assert!(report.findings.is_empty(), "{}", lines(&report));
    assert!(!report.failed());
}

#[test]
fn an_identifier_a_section_file_retires_is_refused_in_another_section_file() {
    let (retiring, reusing) = (
        "rfcs/0001-greetings/02-retiring.toml",
        "rfcs/0001-greetings/03-reusing.toml",
    );
    let mut files = CLEAN.to_vec();
    files.extend([(retiring, RETIRING), (reusing, REUSING)]);
    let report = check(&files);
    assert!(report.failed());
    let retired = partimento::rules::IDENTIFIER_RETIRED;
    let found: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|finding| finding.rule == retired)
        .collect();
    let reused = format!("the identifier is retired at {retiring}:1");
    let expected = Finding::error(retired, reusing, 10, Some("EX-0001-008"), reused);
    assert_eq!(found, [&expected]);
}

/// The findings of the rule identifier-retired in the check of `files`, in the report's order.
fn retired_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    let retired = partimento::rules::IDENTIFIER_RETIRED;
    check(files)
        .findings
        .into_iter()
        .filter(|finding| finding.rule == retired)
        .collect()
}

/// RFC 2's row of the index, which follows RFC 1's.
const FAREWELLS_ROW: &str = r#"[[rfc]]
number = 2
title = "Farewells"
area = "core"
"#;

/// RFC 2's rfc.toml, which retires nothing.
const FAREWELLS_META: &str = r#"number = 2
title = "Farewells"
area = "core"
abstract = "How the program says goodbye when it stops."
retired = []
"#;

#[test]
fn a_section_file_may_retire_an_identifier_of_another_rfcs_number() {
    // A requirement that moved into RFC 1 from RFC 2 keeps its identifier, and a section file
    // of RFC 1 retires it: SUPERSEDING with RFC 2's `EX-0002-001` for `EX-0001-008`.
    let moved = SUPERSEDING.replace("EX-0001-008", "EX-0002-001");
    assert_ne!(moved, SUPERSEDING, "the identifier is RFC 2's");
    let mut files = CLEAN.to_vec();
    files.push(("rfcs/0001-greetings/02-farewell.toml", moved.as_str()));
    let mut tree = tree(&files);
    let generated = partimento::render(&tree).expect("the tree renders");
    tree.files.extend(generated.files);
    let report = partimento::check_default(&tree, &Options::default());
    assert!(report.findings.is_empty(), "{}", lines(&report));
    assert!(!report.failed());
}

#[test]
fn a_requirement_that_holds_a_retired_identifier_of_another_rfcs_number_fails() {
    let retired = partimento::rules::IDENTIFIER_RETIRED;
    let retiring = "rfcs/0001-greetings/02-retiring.toml";
    let message = format!("the identifier is retired at {retiring}:1");
    // REUSING with `EX-0002-001`, which RETIRING, a section file of RFC 1, retires.
    let holding = REUSING.replace("EX-0001-008", "EX-0002-001");
    assert_ne!(holding, REUSING, "the identifier is RFC 2's");
    // The requirement is in a section file of RFC 1, the RFC that retires the identifier.
    let reusing = "rfcs/0001-greetings/03-reusing.toml";
    let mut files = CLEAN.to_vec();
    files.extend([(retiring, RETIRING), (reusing, holding.as_str())]);
    let expected = Finding::error(retired, reusing, 10, Some("EX-0002-001"), message.clone());
    assert_eq!(retired_findings(&files), [expected]);
    // The requirement is in a section file of RFC 2, the RFC whose number the identifier holds.
    let index = format!("{INDEX}\n{FAREWELLS_ROW}");
    let farewell = "rfcs/0002-farewells/01-reusing.toml";
    let mut files = CLEAN.to_vec();
    files.extend([
        ("rfcs/index.toml", index.as_str()),
        ("rfcs/0002-farewells/rfc.toml", FAREWELLS_META),
        (retiring, RETIRING),
        (farewell, holding.as_str()),
    ]);
    let expected = Finding::error(retired, farewell, 10, Some("EX-0002-001"), message);
    assert_eq!(retired_findings(&files), [expected]);
}

/// An rfc.toml that retires `EX-0001-008` at line 5.
const RETIRING_META: &str = r#"number = 1
title = "Greetings"
area = "core"
abstract = "How the program greets a person when it starts and when it stops."
retired = ["EX-0001-008"]
"#;

/// A section file that retires `EX-0001-008` on line 1, as `RETIRING` does.
const RETIRING_AGAIN: &str = r#"retired = ["EX-0001-008"]

[section]
anchor = "retiring-again"
title = "Retiring again"
problem = "A greeting the program no longer prints is written down in two files."
motivation = "A retired identifier keeps a reader from using it again."
"#;

/// The findings of the rule identifier-retired about `EX-0001-008` in `CLEAN` with `files`, in
/// which a later text of a path takes the place of an earlier one.
fn retired_008(files: &[(&'static str, &'static str)]) -> Vec<Finding> {
    let mut all = CLEAN.to_vec();
    all.extend_from_slice(files);
    let retired = partimento::rules::IDENTIFIER_RETIRED;
    check(&all)
        .findings
        .into_iter()
        .filter(|finding| finding.rule == retired)
        .filter(|finding| finding.requirement.as_deref() == Some("EX-0001-008"))
        .collect()
}

#[test]
fn an_identifier_retired_in_two_files_is_kept_at_its_first_listing() {
    let retired = partimento::rules::IDENTIFIER_RETIRED;
    let reuse = |file: &str, at: &str, line: u32| {
        let message = format!("the identifier is retired at {at}:{line}");
        Finding::error(retired, file, 10, Some("EX-0001-008"), message)
    };
    let (meta, first) = (CLEAN[2].0, "rfcs/0001-greetings/02-retiring.toml");
    // rfc.toml retires the identifier at line 5 and a section file retires it at line 1, and a
    // third file reuses it at line 10: the finding names rfc.toml's line.
    let reusing = "rfcs/0001-greetings/03-reusing.toml";
    let files = [(meta, RETIRING_META), (first, RETIRING), (reusing, REUSING)];
    assert_eq!(retired_008(&files), [reuse(reusing, meta, 5)]);
    // Two section files retire the identifier, each at line 1, and a third reuses it: the
    // finding names the first file's line.
    let (again, reusing) = (
        "rfcs/0001-greetings/03-retiring-again.toml",
        "rfcs/0001-greetings/04-reusing.toml",
    );
    let files = [
        (first, RETIRING),
        (again, RETIRING_AGAIN),
        (reusing, REUSING),
    ];
    assert_eq!(retired_008(&files), [reuse(reusing, first, 1)]);
}

#[test]
fn a_supersedes_relation_targets_an_identifier_a_section_file_retires() {
    let mut files = CLEAN.to_vec();
    files.push(("rfcs/0001-greetings/02-farewell.toml", SUPERSEDING));
    let mut tree = tree(&files);
    let generated = partimento::render(&tree).expect("the tree renders");
    tree.files.extend(generated.files);
    let report = partimento::check_default(&tree, &Options::default());
    assert!(report.findings.is_empty(), "{}", lines(&report));
}

#[test]
fn an_implies_relation_to_an_identifier_a_section_file_retires_says_where_it_is_retired() {
    let (retiring, implying) = (
        "rfcs/0001-greetings/02-retiring.toml",
        "rfcs/0001-greetings/03-implying.toml",
    );
    let mut files = CLEAN.to_vec();
    files.extend([(retiring, RETIRING), (implying, IMPLYING)]);
    let report = check(&files);
    assert!(report.failed());
    let target = partimento::checks::RELATION_TARGET;
    let found: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|finding| finding.rule == target)
        .collect();
    let message = format!("the relation's target `EX-0001-008` is retired at {retiring}:1");
    let expected = Finding::error(target, implying, 14, Some("EX-0001-002"), message);
    assert_eq!(found, [&expected]);
}

#[test]
fn an_implies_relation_to_an_identifier_retired_in_two_files_says_its_first_listing() {
    // rfc.toml retires EX-0001-008 at line 5, 02-retiring.toml retires it again at line 1, and
    // 03-implying.toml implies it at line 14. The later rfc.toml of `files` takes the place of
    // CLEAN's, and the finding cites its line, the first listing.
    let meta = CLEAN_META.replace("retired = []", "retired = [\"EX-0001-008\"]");
    assert_ne!(meta, CLEAN_META, "CLEAN_META's line 5 retires nothing");
    let (rfc, retiring, implying) = (
        "rfcs/0001-greetings/rfc.toml",
        "rfcs/0001-greetings/02-retiring.toml",
        "rfcs/0001-greetings/03-implying.toml",
    );
    let mut files = CLEAN.to_vec();
    files.extend([
        (rfc, meta.as_str()),
        (retiring, RETIRING),
        (implying, IMPLYING),
    ]);
    let report = check(&files);
    assert!(report.failed());
    let target = partimento::checks::RELATION_TARGET;
    let found: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|finding| finding.rule == target)
        .collect();
    let message = format!("the relation's target `EX-0001-008` is retired at {rfc}:5");
    let expected = Finding::error(target, implying, 14, Some("EX-0001-002"), message);
    assert_eq!(found, [&expected]);
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use partimento::Tree;

    use super::{clean, EXPECTED};

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

    /// Writes the tree's files into `dir`, each at its path in the tree.
    fn write(dir: &Path, tree: &Tree) {
        for (path, bytes) in &tree.files {
            let file = dir.join(path);
            let parent = file.parent().expect("the file is in a directory");
            std::fs::create_dir_all(parent).expect("the directory is made");
            let data = bytes.copy_to_bytes().to_vec();
            std::fs::write(file, data).expect("the file is written");
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
        write(&dir, &clean());
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
        write(&dir, &clean());
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
