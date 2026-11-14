//! The fixtures of the format rules and of the generated files, each checked and rendered through
//! the library and, with the `cli` feature, through the command.
//!
//! The files of each fixture are embedded at compile time, so the library's tests read no file
//! and run on wasm32 too.

use partimento::{config, ByteVec, Options, Report, Tree};

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

/// A fixture by its directory under tests/fixtures, with the paths of its files beside its
/// spec.toml, its RFC index, and the rfc.toml and 01-greeting.toml of its RFC 0001, which come
/// with each fixture.
macro_rules! fixture {
    ($name:literal $(, $path:literal)*) => {
        Fixture {
            name: $name,
            files: &[
                embed!($name, "spec.toml"),
                embed!($name, "rfcs/index.toml"),
                embed!($name, "rfcs/0001-greetings/rfc.toml"),
                embed!($name, "rfcs/0001-greetings/01-greeting.toml"),
                $(embed!($name, $path),)*
            ],
            expected: include_str!(concat!("fixtures/", $name, "/expected.txt")),
        }
    };
}

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
];

/// The message of the drift finding of a generated file that no section generates.
const STALE: &str = "no section generates this file: `partimento render` removes it";

/// The fixture of the directory `name`.
fn named(name: &str) -> &'static Fixture {
    let fixture = FIXTURES.iter().find(|fixture| fixture.name == name);
    fixture.expect("the fixture is listed")
}

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
                let report = Report { findings };
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

    use super::{fails, named, split, tree, FIXTURES};

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

        let dir = scratch("socket-entry");
        write_fixture(&dir);
        let _socket = UnixListener::bind(dir.join("rfcs/socket")).expect("the socket is made");
        exits_with_2(&dir, "socket: neither a file nor a directory");
    }
}
