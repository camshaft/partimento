//! The fixture `configuration`, whose spec.toml sets each value to another than the one every
//! other fixture's spec.toml sets: the identifier prefix, the three paths, the levels, the RFC
//! numbers of each system and of the contracts, and the implementation terms. The check passes on
//! it, so the code holds none of those values.
//!
//! The library's tests build the tree in memory from files embedded at compile time, so they
//! don't read a file and run on wasm32 too.

use partimento::{model, rules, ByteVec, Options, Report, Tree};

/// One file of the fixture, by its path in the fixture's tree and its text.
macro_rules! embed {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!("fixtures/configuration/", $path)),
        )
    };
}

/// The fixture's spec.toml.
const SPEC: &str = include_str!("fixtures/configuration/spec.toml");

/// The spec.toml that every other fixture holds.
const SHARED_SPEC: &str = include_str!("fixtures/14-implementation-term/spec.toml");

/// The fixture's files. Its RFCs are under `docs/sources`, the directory its spec.toml sets as
/// `paths.rfcs`.
const FILES: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    embed!("docs/sources/index.toml"),
    embed!("docs/sources/0204-greetings/rfc.toml"),
    embed!("docs/sources/0204-greetings/01-greeting.toml"),
];

fn tree(files: &[(&str, &str)]) -> Tree {
    let mut tree = Tree::default();
    for (path, text) in files {
        let bytes = ByteVec::from(text.to_string());
        tree.files.insert(path.to_string(), bytes);
    }
    tree
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

/// The fixture's tree with the line of its spec.toml that sets `key` replaced by the line of the
/// shared spec.toml that sets it.
fn with_shared(key: &str) -> Tree {
    let prefix = format!("{key} = ");
    let line = |spec: &'static str| -> &'static str {
        let found = spec.lines().find(|text| text.starts_with(&prefix));
        found.expect("spec.toml sets the key")
    };
    let spec = SPEC.replace(line(SPEC), line(SHARED_SPEC));
    let mut tree = tree(FILES);
    tree.files
        .insert("spec.toml".to_string(), ByteVec::from(spec));
    tree
}

#[test]
fn the_fixture_with_each_value_changed_passes() {
    for final_pass in [false, true] {
        let report = partimento::check_default(&tree(FILES), &Options { final_pass });
        assert!(report.findings.is_empty(), "{}", lines(&report));
        assert!(!report.failed());
    }
}

/// The library's check reads four of the values past spec.toml's own checks: the prefix, the
/// levels, the implementation terms and `paths.rfcs`. Each case takes one of them from the shared
/// spec.toml, and the fixture's files then break the rule that reads it.
#[test]
fn each_value_the_check_reads_comes_from_spec_toml() {
    // Each key, the rule of the findings that the shared value makes, and their count: the four
    // identifiers of prefix `ZQ`, the three levels, the statement that names `sqlite`, and the RFC
    // index that `rfcs/` doesn't hold.
    let cases = [
        ("prefix", rules::IDENTIFIER_FORM, 4),
        ("levels", rules::FIELD_VALUE, 3),
        ("implementation_terms", rules::IMPLEMENTATION_TERM, 1),
        ("rfcs", model::LAYOUT, 1),
    ];
    for (key, rule, count) in cases {
        let report = partimento::check_default(&with_shared(key), &Options::default());
        assert_eq!(report.findings.len(), count, "{key}:\n{}", lines(&report));
        for finding in &report.findings {
            assert_eq!(finding.rule, rule, "{key}: {finding}");
        }
    }
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::Path;
    use std::process::{Command, Output};

    fn run(args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).args(args).output();
        output.expect("the command runs")
    }

    /// The command reads the RFCs from `docs/sources`: where it read them from `rfcs/`, the RFC
    /// index would be missing and the check would fail.
    #[test]
    fn the_command_passes_on_the_fixture() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = manifest.join("tests/fixtures/configuration");
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
        let output = run(&["check", "--root", dir, "--format", "json"]);
        let expected = include_str!("fixtures/configuration/expected.json");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert_eq!(output.status.code(), Some(0));
    }
}
