//! The rule that a change of a contract RFC's requirements raises its `contract_version`,
//! through fixture 48-contract-version-raised: its base holds contract RFC 100 at version 1, and
//! each of its heads changes the statement of the RFC's requirement EX-0100-001, `raised` with
//! the version raised to 2 and `unraised` with the version left at 1.
//!
//! The library's tests build each tree in memory from files embedded at compile time, so they
//! don't read a file and run on wasm32 too.

use std::fmt::Display;

use partimento::{Binding, ByteVec, ChangeKind, Severity, Tree, VersionChange, VERSION_RAISED};

/// One file of a side of the fixture, by its path in the side's tree and its text.
macro_rules! embed {
    ($side:literal, $path:literal) => {
        (
            $path,
            include_str!(concat!(
                "fixtures/48-contract-version-raised/",
                $side,
                "/",
                $path
            )),
        )
    };
}

/// The four files of one side of the fixture.
macro_rules! side {
    ($side:literal) => {
        [
            embed!($side, "spec.toml"),
            embed!($side, "rfcs/index.toml"),
            embed!($side, "rfcs/0100-answers/rfc.toml"),
            embed!($side, "rfcs/0100-answers/01-answer.toml"),
        ]
    };
}

/// The fixture's base, whose RFC 100 is at contract version 1.
const BASE: [(&str, &str); 4] = side!("base");

/// The head that changes EX-0100-001 and raises RFC 100 to version 2.
const RAISED: [(&str, &str); 4] = side!("raised");

/// The head that changes EX-0100-001 and leaves RFC 100 at version 1.
const UNRAISED: [(&str, &str); 4] = side!("unraised");

/// The changes the diff of the base and the raised head prints, one a line.
const RAISED_EXPECTED: &str = include_str!("fixtures/48-contract-version-raised/raised.txt");

/// The finding the diff of the base and the unraised head prints.
const UNRAISED_EXPECTED: &str = include_str!("fixtures/48-contract-version-raised/expected.txt");

/// The tree of the files, by their paths.
fn tree(files: &[(&str, &str)]) -> Tree {
    let mut tree = Tree::default();
    for (path, text) in files {
        let bytes = ByteVec::from(text.to_string());
        tree.files.insert(path.to_string(), bytes);
    }
    tree
}

/// The items, findings or changes, as the command prints them, one a line.
fn lines(items: &[impl Display]) -> String {
    let mut text = String::new();
    for item in items {
        text.push_str(&item.to_string());
        text.push('\n');
    }
    text
}

#[test]
fn a_change_with_the_version_raised_passes_the_diff() {
    let found = partimento::diff_with_versions(&tree(&BASE), &tree(&RAISED));
    let (changes, versions) = found.unwrap_or_else(|findings| panic!("{}", lines(&findings)));
    let found: Vec<_> = changes
        .iter()
        .map(|change| (change.kind, change.requirement.as_str(), change.binds_to))
        .collect();
    assert_eq!(found, [(ChangeKind::Changed, "EX-0100-001", Binding::System)]);
    let expected = VersionChange {
        rfc: 100,
        base: 1,
        head: 2,
    };
    assert_eq!(versions, [expected]);
    assert_eq!(lines(&changes) + &lines(&versions), RAISED_EXPECTED);
}

#[test]
fn the_same_change_without_the_raise_fails_the_diff() {
    let (base, head) = (tree(&BASE), tree(&UNRAISED));
    let findings = partimento::diff(&base, &head);
    let findings = findings.expect_err("the change doesn't raise the version");
    assert_eq!(lines(&findings), UNRAISED_EXPECTED);
    for finding in &findings {
        assert_eq!(finding.rule, VERSION_RAISED, "{finding}");
        assert_eq!(finding.severity, Severity::Error, "{finding}");
    }
    let versions = partimento::contract_versions(&base, &head);
    let versions = versions.expect_err("the change doesn't raise the version");
    assert_eq!(lines(&versions), UNRAISED_EXPECTED);
    let both = partimento::diff_with_versions(&base, &head);
    let both = both.expect_err("the change doesn't raise the version");
    assert_eq!(lines(&both), UNRAISED_EXPECTED);
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::Path;
    use std::process::{Command, Output};

    use super::{RAISED_EXPECTED, UNRAISED_EXPECTED};

    fn run(args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).args(args).output();
        output.expect("the command runs")
    }

    /// The directory of one side of the fixture.
    fn side(name: &str) -> String {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = manifest.join("tests/fixtures/48-contract-version-raised");
        let dir = dir.join(name);
        dir.to_str().expect("the path is UTF-8").to_string()
    }

    #[test]
    fn the_command_passes_a_change_with_the_version_raised() {
        let (base, head) = (side("base"), side("raised"));
        let output = run(&["diff", base.as_str(), head.as_str()]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), RAISED_EXPECTED);
        assert_eq!(output.status.code(), Some(0));
    }

    #[test]
    fn the_command_fails_the_same_change_without_the_raise() {
        let (base, head) = (side("base"), side("unraised"));
        let output = run(&["diff", base.as_str(), head.as_str()]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), UNRAISED_EXPECTED);
        assert_eq!(output.status.code(), Some(1));
    }
}
