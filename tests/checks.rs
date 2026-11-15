//! The fixtures of checks 1 to 5, each checked through the library under each enabled solver
//! backend and, with the `cli` feature, through the command.
//!
//! The files of each fixture are embedded at compile time, so the library's tests read no file
//! and run on wasm32 too. A fixture holds its sources alone: each test adds the files that
//! `partimento render` generates from them, which the drift rule asks for, so that a fixture's
//! findings are those of the checks.

#[macro_use]
mod common;

use common::{fails, lines, tree, Fixture};
use partimento::solver::{Answer, Backend, Lit};
use partimento::{ByteVec, Options, Report, Tree};

const FIXTURES: &[Fixture] = &[
    fixture!("03-unresolved-relation"),
    fixture!("04-relation-cycle"),
    fixture!("06-minimal-conflict"),
    fixture!("07-logic-conflict"),
    fixture!(
        "08-system-boundary",
        "rfcs/0200-scheduling/rfc.toml",
        "rfcs/0200-scheduling/01-schedule.toml"
    ),
    fixture!("09-vacuous-condition"),
    fixture!("10-parameter-bounds"),
    fixture!("16-exactly-one"),
    fixture!("17-unless"),
    fixture!("20-minimal-core"),
    fixture!("21-polarity-conflict"),
    fixture!("22-malformed-formulas"),
    fixture!("23-supersedes"),
    fixture!("25-supersedes-bound"),
    fixture!("26-supersedes-part"),
    fixture!(
        "27-supersedes-findings",
        "rfcs/0200-scheduling/rfc.toml",
        "rfcs/0200-scheduling/01-schedule.toml"
    ),
    fixture!("28-supersedes-cycle"),
    fixture!("29-supersedes-self"),
    fixture!(
        "24-contract-logic",
        "rfcs/0100-transfer/rfc.toml",
        "rfcs/0100-transfer/01-transfer.toml",
        "rfcs/0200-scheduling/rfc.toml",
        "rfcs/0200-scheduling/01-schedule.toml"
    ),
    fixture!("29-unresolved-parts"),
    fixture!(
        "30-boundary-parts",
        "rfcs/0200-scheduling/rfc.toml",
        "rfcs/0200-scheduling/01-schedule.toml"
    ),
    fixture!("31-first-level"),
    fixture!(
        "32-bounds-across-rfcs",
        "rfcs/0002-farewells/rfc.toml",
        "rfcs/0002-farewells/01-farewell.toml"
    ),
    fixture!("33-exactly-one-logic"),
    fixture!(
        "34-planned-from-directory",
        "rfcs/0002-farewells/rfc.toml",
        "rfcs/0002-farewells/01-farewell.toml"
    ),
    fixture!(
        "35-stored-state",
        "rfcs/0002-farewells/rfc.toml",
        "rfcs/0002-farewells/01-farewell.toml"
    ),
    fixture!("36-parameter-types"),
    fixture!(
        "37-groups-and-versions",
        "rfcs/0002-farewells/rfc.toml",
        "rfcs/0003-replies/rfc.toml",
        "rfcs/0100-transfer/rfc.toml",
        "rfcs/0100-transfer/01-transfer.toml",
        "rfcs/0101-handoff/rfc.toml",
        "rfcs/0102-relay/rfc.toml",
        "rfcs/0900-base/rfc.toml"
    ),
    fixture!("38-permissive-level"),
    fixture!("39-binding-and-rationale"),
];

/// The listed fixture of that name.
fn named(name: &str) -> &'static Fixture {
    let found = FIXTURES.iter().find(|fixture| fixture.name == name);
    found.expect("the fixture is listed")
}

/// The tree with the files that `partimento render` generates from it. A tree whose format rules
/// fail doesn't render, and stays as it is.
fn with_render(mut tree: Tree) -> Tree {
    if let Ok(generated) = partimento::render(&tree) {
        tree.files.extend(generated.files);
    }
    tree
}

/// The fixture's tree with the files that `partimento render` generates from it.
fn rendered(fixture: &Fixture) -> Tree {
    with_render(tree(fixture))
}

/// Each finding's rule, file and line.
fn places(report: &Report) -> Vec<(&str, &str, u32)> {
    let mut places = Vec::new();
    for finding in &report.findings {
        let (rule, file) = (finding.rule.as_str(), finding.file.as_str());
        places.push((rule, file, finding.line));
    }
    places
}

/// A backend that answers each `Unsat` with every assumption as its core, the widest core a
/// backend may answer, so that the checks narrow it to a minimal one themselves.
#[derive(Default)]
struct Wide<B>(B);

impl<B: Backend> Backend for Wide<B> {
    fn new_var(&mut self) -> Lit {
        self.0.new_var()
    }

    fn add_clause(&mut self, clause: &[Lit]) {
        self.0.add_clause(clause);
    }

    fn solve(&mut self, assumptions: &[Lit]) -> Answer {
        match self.0.solve(assumptions) {
            Answer::Sat => Answer::Sat,
            Answer::Unsat { .. } => Answer::Unsat {
                core: assumptions.to_vec(),
            },
        }
    }
}

/// A check of a tree under one backend.
type Check = fn(&Tree, &Options) -> Report;

/// Each enabled backend by its name, and each again behind [`Wide`].
fn backends() -> Vec<(&'static str, Check)> {
    let mut backends: Vec<(&'static str, Check)> = Vec::new();
    #[cfg(feature = "batsat")]
    {
        use partimento::solver::Batsat;
        backends.push(("batsat", partimento::check::<Batsat>));
        backends.push(("wide batsat", partimento::check::<Wide<Batsat>>));
    }
    #[cfg(feature = "varisat")]
    {
        use partimento::solver::Varisat;
        backends.push(("varisat", partimento::check::<Varisat>));
        backends.push(("wide varisat", partimento::check::<Wide<Varisat>>));
    }
    backends
}

#[test]
fn each_fixture_holds_its_expected_findings() {
    let mut failures = Vec::new();
    for (backend, check) in backends() {
        for fixture in FIXTURES {
            let report = check(&rendered(fixture), &Options::default());
            let found = lines(&report);
            let (name, expected) = (fixture.name, fixture.expected);
            let place = format!("{name} under {backend}");
            if found != expected {
                failures.push(format!("{place}:\nexpected:\n{expected}found:\n{found}"));
            }
            if report.failed() != fails(fixture) {
                failures.push(format!("{place}: the failure isn't the expected one"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_final_pass_turns_the_unresolved_relations_into_failures() {
    let options = Options { final_pass: true };
    let warning = "warning[relation-unresolved]";
    let error = "error[relation-unresolved]";
    for fixture in FIXTURES {
        let report = partimento::check_default(&rendered(fixture), &options);
        let expected = fixture.expected.replace(warning, error);
        assert_eq!(lines(&report), expected, "{}", fixture.name);
        let fails = expected.lines().any(|line| line.starts_with("error"));
        assert_eq!(report.failed(), fails, "{}", fixture.name);
    }
}

/// The path of RFC 0001's first section file.
const GREETING: &str = "rfcs/0001-greetings/01-greeting.toml";

/// The path of the section file [`with_formula`] adds.
const EXTRA_PATH: &str = "rfcs/0001-greetings/02-extra.toml";

/// A section file whose requirement EX-0001-009, at line 8, holds `FORMULA` as its `logic` at
/// line 12.
const EXTRA: &str = r#"[section]
anchor = "extra"
title = "Extra"
problem = "A test needs one more formula."
motivation = "The formula constrains the other requirements."

[[requirement]]
id = "EX-0001-009"
level = "SHOULD"
polarity = "required"
statement = "The program SHOULD print the time."
logic = ["FORMULA"]
binds_to = "system"
rationale = "The tests need this requirement."
"#;

/// The fixture's tree with a second section file, at [`EXTRA_PATH`], whose requirement holds
/// `formula`, and the files that `partimento render` generates from both.
fn with_formula(fixture: &Fixture, formula: &str) -> Tree {
    let mut tree = tree(fixture);
    let text = EXTRA.replace("FORMULA", formula);
    let path = EXTRA_PATH.to_string();
    tree.files.insert(path, ByteVec::from(text));
    with_render(tree)
}

#[test]
fn exactly_one_fails_where_other_formulas_force_both_parts() {
    let fixture = named("16-exactly-one");
    let tree = with_formula(fixture, "EX-0001-002 & EX-0001-003");
    // The pairwise exclusion at the `compose` line and the extra formula.
    let expected = [
        ("unsatisfiable", GREETING, 14),
        ("unsatisfiable", EXTRA_PATH, 12),
    ];
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(places(&report), expected, "{backend}: {}", lines(&report));
        assert!(report.failed(), "{backend}");
    }
}

#[test]
fn an_unless_forces_its_requirement_only_where_its_exception_is_false() {
    let fixture = named("17-unless");
    // Where the exception is false, the requirement holds: denying both fails.
    let tree = with_formula(fixture, "!EX-0001-001 & !EX-0001-002");
    let expected = [
        ("unsatisfiable", GREETING, 14),
        ("unsatisfiable", EXTRA_PATH, 12),
    ];
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(places(&report), expected, "{backend}: {}", lines(&report));
    }
    // Where the exception is true, the requirement may not hold.
    let tree = with_formula(fixture, "!EX-0001-001 & EX-0001-002");
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), "", "{backend}");
    }
}

/// The text that `partimento render` writes for the section at [`GREETING`].
const GREETING_TEXT: &str = "rendered/0001-greetings/01-greeting.md";

/// The requirement file that `partimento render` writes for EX-0001-001 of [`GREETING`].
const FIRST_REQUIREMENT: &str = ".duvet/requirements/0001-greetings/01-greeting/EX-0001-001.toml";

/// Drift is checked wherever the format rules pass, though a check fails. Every other test here
/// renders its tree first, so none of them finds drift.
#[test]
fn drift_is_checked_where_the_format_rules_pass_and_a_check_fails() {
    // The fixture renders, and its formulas can't hold together.
    let fixture = named("07-logic-conflict");
    let generated = partimento::render(&tree(fixture)).expect("the format rules pass");
    let mut sources = tree(fixture);
    sources.files.extend(generated.files);
    let conflict = [
        ("unsatisfiable", GREETING, 10),
        ("unsatisfiable", GREETING, 14),
        ("unsatisfiable", GREETING, 19),
    ];
    // The section's text left out, and a requirement file edited by hand.
    let mut missing = sources.clone();
    assert!(missing.files.remove(GREETING_TEXT).is_some());
    let mut edited = sources.clone();
    let text = ByteVec::from("edited by hand\n".to_string());
    let path = FIRST_REQUIREMENT.to_string();
    assert!(edited.files.insert(path, text).is_some());
    for (backend, check) in backends() {
        let report = check(&sources, &Options::default());
        assert_eq!(places(&report), conflict, "{backend}: {}", lines(&report));
        for (changed, drift) in [(&missing, GREETING_TEXT), (&edited, FIRST_REQUIREMENT)] {
            let report = check(changed, &Options::default());
            let mut expected = vec![("drift", drift, 1)];
            expected.extend(conflict);
            assert_eq!(places(&report), expected, "{backend}: {}", lines(&report));
            assert!(report.failed(), "{backend}");
        }
    }
}

/// The path of the section file that [`a_polarity_conflict_is_found_once_for_each_pair`] adds
/// beside the polarity fixture's.
const STREAMS_PATH: &str = "rfcs/0001-greetings/02-streams.toml";

/// A section file whose requirement EX-0001-004, at line 8, is forbidden at MUST and leads to
/// EX-0001-001; whose EX-0001-005 and EX-0001-006, at lines 17 and 26, are required at MUST and
/// lead to each other, and through EX-0001-006 to EX-0001-004 and EX-0001-008; whose
/// EX-0001-007 is required at SHOULD and leads to EX-0001-004; and whose EX-0001-008 is
/// forbidden at SHOULD.
const STREAMS: &str = r#"[section]
anchor = "streams"
title = "Streams"
problem = "A person who reads the program's output can't tell its streams apart."
motivation = "Each stream carries one kind of line."

[[requirement]]
id = "EX-0001-004"
level = "MUST"
polarity = "forbidden"
statement = "The program MUST NOT print a log line to the output stream."
relations = [{ kind = "depends_on", target = "EX-0001-001" }]
binds_to = "system"
rationale = "The tests need this requirement."

[[requirement]]
id = "EX-0001-005"
level = "MUST"
polarity = "required"
statement = "The program MUST print each log line to the log file."
relations = [{ kind = "depends_on", target = "EX-0001-006" }]
binds_to = "system"
rationale = "The tests need this requirement."

[[requirement]]
id = "EX-0001-006"
level = "MUST"
polarity = "required"
statement = "The program MUST open the log file when it starts."
relations = [
  { kind = "depends_on", target = "EX-0001-005" },
  { kind = "depends_on", target = "EX-0001-004" },
  { kind = "depends_on", target = "EX-0001-008" },
]
binds_to = "system"
rationale = "The tests need this requirement."

[[requirement]]
id = "EX-0001-007"
level = "SHOULD"
polarity = "required"
statement = "The program SHOULD print the time on each log line."
relations = [{ kind = "depends_on", target = "EX-0001-004" }]
binds_to = "system"
rationale = "The tests need this requirement."

[[requirement]]
id = "EX-0001-008"
level = "SHOULD"
polarity = "forbidden"
statement = "The program SHOULD NOT print a blank log line."
binds_to = "system"
rationale = "The tests need this requirement."
"#;

#[test]
fn a_polarity_conflict_is_found_once_for_each_pair() {
    let mut tree = tree(named("21-polarity-conflict"));
    let path = STREAMS_PATH.to_string();
    tree.files.insert(path, ByteVec::from(STREAMS.to_string()));
    let tree = with_render(tree);
    // EX-0001-005 and EX-0001-006 lead to EX-0001-004, and through it, EX-0001-001 and
    // EX-0001-002 to the fixture's EX-0001-003, though their own relations form a cycle. The
    // relation from EX-0001-004 starts at a forbidden requirement, EX-0001-007 is required at
    // SHOULD and EX-0001-008 is forbidden at SHOULD, so none of them adds a finding.
    let rule = "error[polarity-conflict]";
    let lead = |file: &str, line: u32, id: &str, to: &str| {
        format!(
            "{rule} {file}:{line}: {id}: the requirement is required at MUST, and its relations \
             lead to `{to}`, which is forbidden at MUST\n"
        )
    };
    let expected = [
        lead(GREETING, 10, "EX-0001-001", "EX-0001-003"),
        lead(GREETING, 19, "EX-0001-002", "EX-0001-003"),
        lead(STREAMS_PATH, 17, "EX-0001-005", "EX-0001-003"),
        lead(STREAMS_PATH, 17, "EX-0001-005", "EX-0001-004"),
        lead(STREAMS_PATH, 26, "EX-0001-006", "EX-0001-003"),
        lead(STREAMS_PATH, 26, "EX-0001-006", "EX-0001-004"),
    ]
    .concat();
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), expected, "{backend}");
        assert!(report.failed(), "{backend}");
    }
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use partimento::Tree;

    use super::{fails, rendered, FIXTURES};

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
    fn write_tree(dir: &Path, tree: &Tree) {
        for (path, bytes) in &tree.files {
            let file = dir.join(path);
            let parent = file.parent().expect("the file is in a directory");
            std::fs::create_dir_all(parent).expect("the directory is made");
            let data = bytes.copy_to_bytes().to_vec();
            std::fs::write(file, data).expect("the file is written");
        }
    }

    #[test]
    fn each_fixture_prints_its_expected_findings() {
        for fixture in FIXTURES {
            let dir = scratch(&format!("checks-{}", fixture.name));
            write_tree(&dir, &rendered(fixture));
            let output = run(&["check", "--root", dir.to_str().expect("the path is UTF-8")]);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert_eq!(stdout, fixture.expected, "{}", fixture.name);
            let code = i32::from(fails(fixture));
            assert_eq!(output.status.code(), Some(code), "{}", fixture.name);
        }
    }
}
