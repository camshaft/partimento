//! The fixtures of the format rules, each checked through the library and, with the `cli`
//! feature, through the command.
//!
//! The files of each fixture are embedded at compile time, so the library's tests read no file
//! and run on wasm32 too.

use partimento::{ByteVec, Options, Report, Tree};

/// A fixture: the files of its tree, by their paths, and the findings the check prints for it.
struct Fixture {
    name: &'static str,
    files: &'static [(&'static str, &'static str)],
    expected: &'static str,
}

/// One file of a fixture's tree, by its path in the tree and its text.
macro_rules! embed {
    ($name:literal, $path:literal) => {
        ($path, include_str!(concat!("fixtures/", $name, "/", $path)))
    };
}

/// A fixture by its directory under tests/fixtures, with the names of the section files of its
/// RFC 0001 after 01-greeting.toml: its spec.toml, its RFC index, the RFC's rfc.toml and
/// 01-greeting.toml come with each fixture.
macro_rules! fixture {
    ($name:literal $(, $section:literal)*) => {
        Fixture {
            name: $name,
            files: &[
                embed!($name, "spec.toml"),
                embed!($name, "rfcs/index.toml"),
                embed!($name, "rfcs/0001-greetings/rfc.toml"),
                embed!($name, "rfcs/0001-greetings/01-greeting.toml"),
                $((
                    concat!("rfcs/0001-greetings/", $section),
                    include_str!(concat!("fixtures/", $name, "/rfcs/0001-greetings/", $section)),
                ),)*
            ],
            expected: include_str!(concat!("fixtures/", $name, "/expected.txt")),
        }
    };
}

const FIXTURES: &[Fixture] = &[
    fixture!("02-duplicate-identifier", "02-farewell.toml"),
    fixture!("05-key-word"),
    fixture!("12-unknown-field"),
    fixture!("13-section-field", "02-farewell.toml"),
    fixture!("14-implementation-term"),
];

fn tree(fixture: &Fixture) -> Tree {
    let mut tree = Tree::default();
    for (path, text) in fixture.files {
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

/// Whether the fixture's expected findings fail the check.
fn fails(fixture: &Fixture) -> bool {
    let error = |line: &str| line.starts_with("error");
    fixture.expected.lines().any(error)
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

/// Fixture 05-key-word's tree with `paths.rfcs`, at line 6 of its spec.toml, set to `value`.
fn with_rfcs(value: &str) -> Tree {
    let mut tree = tree(&FIXTURES[1]);
    let spec = include_str!("fixtures/05-key-word/spec.toml");
    let spec = spec.replace("rfcs = \"rfcs\"", &format!("rfcs = {value:?}"));
    tree.files.insert("spec.toml".to_string(), ByteVec::from(spec));
    tree
}

#[test]
fn a_path_that_leaves_the_tree_fails_at_spec_toml() {
    let outside = ["", "/", "/home", "..", "../..", "rfcs/..", "./rfcs", "rfcs/", "rfcs//x"];
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
    assert_eq!(lines(&report), FIXTURES[1].expected);
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use super::{fails, FIXTURES};

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
        assert_eq!(run(&["render"]).status.code(), Some(2));
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

    /// An empty directory under the build's temporary directory, for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    /// Writes fixture 05-key-word's files into `dir`.
    fn write_fixture(dir: &Path) {
        for (path, text) in FIXTURES[1].files {
            let file = dir.join(path);
            let parent = file.parent().expect("the file is in a directory");
            std::fs::create_dir_all(parent).expect("the directory is made");
            std::fs::write(file, text).expect("the file is written");
        }
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
        assert!(stdout.starts_with("error[config] spec.toml:6: `paths.rfcs`"), "{stdout}");
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
}
