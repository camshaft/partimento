//! The formula set a check returns beside its findings, read through the library under each
//! enabled solver backend and, with the `cli` feature, through `check --formulas`, which writes
//! it as JSON.
//!
//! The files of each fixture are embedded at compile time, so the library's tests read no file
//! and run on wasm32 too.

#[macro_use]
mod common;

use common::{lines, tree, Fixture};
use partimento::logic::{Atom, FormulaSet};
use partimento::{Options, Report, Tree};

const RELATION: Fixture = fixture!("03-unresolved-relation");
const CONDITION: Fixture = fixture!("09-vacuous-condition");
const CONFLICT: Fixture = fixture!("06-minimal-conflict");
const CYCLE: Fixture = fixture!("04-relation-cycle");

/// The section file of RFC 0001 that each fixture holds.
const GREETING: &str = "rfcs/0001-greetings/01-greeting.toml";

/// The fixture's tree with the files that `partimento render` generates from it.
fn rendered(fixture: &Fixture) -> Tree {
    let mut tree = tree(fixture);
    if let Ok(generated) = partimento::render(&tree) {
        tree.files.extend(generated.files);
    }
    tree
}

/// A check of a tree under one backend.
type Check = fn(&Tree, &Options) -> Report;

/// Each enabled backend by its name.
fn backends() -> Vec<(&'static str, Check)> {
    let mut backends: Vec<(&'static str, Check)> = Vec::new();
    #[cfg(feature = "batsat")]
    {
        use partimento::solver::Batsat;
        backends.push(("batsat", partimento::check::<Batsat>));
    }
    #[cfg(feature = "varisat")]
    {
        use partimento::solver::Varisat;
        backends.push(("varisat", partimento::check::<Varisat>));
    }
    backends
}

/// A required requirement of RFC 0001's greeting section, at that level.
fn held(id: &str, level: &str) -> Atom {
    Atom {
        id: id.to_string(),
        rfc: Some(1),
        section: Some(GREETING.to_string()),
        level: Some(level.to_string()),
        polarity: Some("required".to_string()),
    }
}

/// An identifier that no requirement of the set holds, of the RFC its form names.
fn unheld(id: &str, rfc: u32) -> Atom {
    Atom {
        id: id.to_string(),
        rfc: Some(rfc),
        section: None,
        level: None,
        polarity: None,
    }
}

/// Each formula of the set as "<line> <requirement>: <formula>", followed by the kind and the
/// target of the relation that states it, where one does.
fn formula_lines(set: &FormulaSet) -> Vec<String> {
    let mut found = Vec::new();
    for entry in &set.formulas {
        let origin = &entry.origin;
        assert_eq!(origin.file, GREETING, "{}", entry.formula);
        let mut line = format!("{} {}: {}", origin.line, origin.requirement, entry.formula);
        if let Some(relation) = &entry.relation {
            line.push_str(&format!(" ({} {})", relation.kind, relation.target));
        }
        found.push(line);
    }
    found
}

/// Each condition of the set as "<line> <requirement>: <atom>".
fn condition_lines(set: &FormulaSet) -> Vec<String> {
    let mut found = Vec::new();
    for condition in &set.conditions {
        let origin = &condition.origin;
        assert_eq!(origin.file, GREETING, "{}", condition.atom);
        found.push(format!(
            "{} {}: {}",
            origin.line, origin.requirement, condition.atom
        ));
    }
    found
}

#[test]
fn each_relation_states_a_formula_with_its_kind_and_its_target() {
    let tree = rendered(&RELATION);
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), RELATION.expected, "{backend}");
        let atoms = [
            held("EX-0001-001", "MUST"),
            held("EX-0001-002", "SHOULD"),
            unheld("EX-0001-009", 1),
            unheld("EX-0002-001", 2),
        ];
        assert_eq!(report.formulas.atoms, atoms, "{backend}");
        let formulas = [
            "8 EX-0001-001: EX-0001-001",
            "12 EX-0001-001: (EX-0001-001 -> EX-0001-009) (depends_on EX-0001-009)",
            "15 EX-0001-002: EX-0001-002",
            "19 EX-0001-002: (EX-0001-002 -> EX-0002-001) (depends_on EX-0002-001)",
        ];
        assert_eq!(formula_lines(&report.formulas), formulas, "{backend}");
        assert!(report.formulas.conditions.is_empty(), "{backend}");
    }
}

#[test]
fn a_composition_logic_and_a_condition_state_their_formulas() {
    let tree = rendered(&CONDITION);
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), CONDITION.expected, "{backend}");
        let atoms = [
            held("EX-0001-001", "MUST"),
            held("EX-0001-002", "SHOULD"),
            held("EX-0001-003", "MAY"),
            held("EX-0001-004", "SHOULD"),
        ];
        assert_eq!(report.formulas.atoms, atoms, "{backend}");
        let formulas = [
            "8 EX-0001-001: EX-0001-001",
            "12 EX-0001-001: (EX-0001-001 <-> (EX-0001-002 | EX-0001-003))",
            "14 EX-0001-001: !EX-0001-003",
            "33 EX-0001-004: (EX-0001-003 -> EX-0001-004)",
        ];
        assert_eq!(formula_lines(&report.formulas), formulas, "{backend}");
        let conditions = ["33 EX-0001-004: EX-0001-003"];
        assert_eq!(condition_lines(&report.formulas), conditions, "{backend}");
    }
}

#[test]
fn a_conflicts_with_relation_states_the_negation_of_both() {
    let tree = rendered(&CONFLICT);
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), CONFLICT.expected, "{backend}");
        let atoms = [
            held("EX-0001-001", "MUST"),
            held("EX-0001-002", "MUST"),
            held("EX-0001-003", "SHOULD"),
        ];
        assert_eq!(report.formulas.atoms, atoms, "{backend}");
        let formulas = [
            "8 EX-0001-001: EX-0001-001",
            "14 EX-0001-002: EX-0001-002",
            "18 EX-0001-002: !(EX-0001-002 & EX-0001-001) (conflicts_with EX-0001-001)",
            "21 EX-0001-003: EX-0001-003",
        ];
        assert_eq!(formula_lines(&report.formulas), formulas, "{backend}");
        assert!(report.formulas.conditions.is_empty(), "{backend}");
    }
}

#[test]
fn implies_and_refines_relations_state_an_implication() {
    let tree = rendered(&CYCLE);
    for (backend, check) in backends() {
        let report = check(&tree, &Options::default());
        assert_eq!(lines(&report), CYCLE.expected, "{backend}");
        let atoms = [held("EX-0001-001", "MUST"), held("EX-0001-002", "SHOULD")];
        assert_eq!(report.formulas.atoms, atoms, "{backend}");
        let formulas = [
            "8 EX-0001-001: EX-0001-001",
            "12 EX-0001-001: (EX-0001-001 -> EX-0001-002) (implies EX-0001-002)",
            "15 EX-0001-002: EX-0001-002",
            "19 EX-0001-002: (EX-0001-002 -> EX-0001-001) (refines EX-0001-001)",
        ];
        assert_eq!(formula_lines(&report.formulas), formulas, "{backend}");
    }
}

#[test]
fn a_tree_without_a_configuration_returns_an_empty_formula_set() {
    for (backend, check) in backends() {
        let report = check(&Tree::default(), &Options::default());
        assert_eq!(report.formulas, FormulaSet::default(), "{backend}");
    }
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use partimento::Tree;

    use super::common::fails;
    use super::{rendered, Fixture, CONDITION, CONFLICT, RELATION};

    fn run(dir: &Path, args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).current_dir(dir).args(args).output();
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

    /// Checks the fixture's rendered tree with `--formulas`, asserts the findings the command
    /// prints and its exit code, and returns the file it writes.
    fn formulas(name: &str, fixture: &Fixture) -> String {
        let dir = scratch(name);
        write_tree(&dir.join("tree"), &rendered(fixture));
        let args = ["check", "--root", "tree", "--formulas", "formulas.json"];
        let output = run(&dir, &args);
        assert_eq!(String::from_utf8_lossy(&output.stdout), fixture.expected);
        let code = i32::from(fails(fixture));
        assert_eq!(output.status.code(), Some(code));
        let file = dir.join("formulas.json");
        std::fs::read_to_string(file).expect("the command writes the formula set")
    }

    #[test]
    fn the_command_writes_the_formula_set_of_relations() {
        let json = formulas("formulas-relation", &RELATION);
        let expected = include_str!("formula-sets/03-unresolved-relation.json");
        assert_eq!(json, expected);
    }

    #[test]
    fn the_command_writes_the_formula_set_of_a_condition() {
        let json = formulas("formulas-condition", &CONDITION);
        let expected = include_str!("formula-sets/09-vacuous-condition.json");
        assert_eq!(json, expected);
    }

    #[test]
    fn the_command_writes_the_formula_set_of_a_conflict() {
        let json = formulas("formulas-conflict", &CONFLICT);
        let expected = include_str!("formula-sets/06-minimal-conflict.json");
        assert_eq!(json, expected);
    }

    #[test]
    fn formulas_without_a_file_exits_with_2() {
        let dir = scratch("formulas-missing");
        for args in [&["check", "--formulas"][..], &["check", "--formulas", "--root", "tree"]] {
            let output = run(&dir, args);
            assert_eq!(output.status.code(), Some(2), "{args:?}");
            assert!(output.stdout.is_empty(), "{args:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = "partimento: --formulas needs a file\n";
            assert!(stderr.starts_with(message), "{args:?}: {stderr}");
        }
        assert!(!dir.join("--root").exists());
    }

    #[test]
    fn a_formulas_file_the_command_cant_write_exits_with_2() {
        let dir = scratch("formulas-directory");
        write_tree(&dir.join("tree"), &rendered(&RELATION));
        std::fs::create_dir(dir.join("formulas.json")).expect("the directory is made");
        let args = ["check", "--root", "tree", "--formulas", "formulas.json"];
        let output = run(&dir, &args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        // The check ran and its write failed: stderr holds the write's error alone, without the
        // usage text an argument error prints.
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.starts_with("partimento: formulas.json: "), "{stderr}");
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
    }
}
