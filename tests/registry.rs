//! The registry of identifier kinds, through fixture 46-registry, whose spec.toml sets
//! `paths.registry`: the check of the fixture with its registry, with a registry of comment lines
//! alone, without the file, with a registry whose entries each break one rule, with a registry
//! that doesn't parse, and without `paths.registry`; the checks of `paths.registry` in spec.toml;
//! and the diff of a change that removes or alters an entry, and of one that only adds.
//!
//! The library's tests build each tree in memory from files embedded at compile time, so they
//! read no file and run on wasm32 too.

use partimento::{config, registry, ByteVec, Options, Report, Tree};

/// One file of the fixture, by its path in the fixture's tree and its text.
macro_rules! embed {
    ($path:literal) => {
        ($path, include_str!(concat!("fixtures/46-registry/", $path)))
    };
}

/// The path of the registry that the fixture's spec.toml sets, at its line 9.
const REGISTRY: &str = "registry/identifiers.toml";

/// The fixture's spec.toml.
const SPEC: &str = include_str!("fixtures/46-registry/spec.toml");

/// The fixture's registry: entry `g` at line 3 and entry `h` at line 10.
const ENTRIES: &str = include_str!("fixtures/46-registry/registry/identifiers.toml");

/// The fixture's sources. Its RFC 1 holds EX-0001-001 and EX-0001-002, and retires EX-0001-003.
const FILES: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    embed!("rfcs/index.toml"),
    embed!("rfcs/0001-greetings/rfc.toml"),
    embed!("rfcs/0001-greetings/01-greeting.toml"),
    (REGISTRY, ENTRIES),
];

/// A registry of comment lines alone.
const COMMENTS: &str = include_str!("fixtures/46-registry/cases/comments.toml");

/// A registry whose first entry passes and each entry after it breaks one rule.
const BROKEN: &str = include_str!("fixtures/46-registry/cases/broken.toml");

/// The findings the check prints for the fixture with [`BROKEN`] as its registry.
const BROKEN_EXPECTED: &str = include_str!("fixtures/46-registry/cases/expected.txt");

/// The registry of a change from the fixture that changes entry `g`'s body_length, at its line
/// 7, removes entry `h` and adds entry `r`.
const HEAD: &str = include_str!("fixtures/46-registry/cases/head.toml");

/// The findings of the diff from the fixture to the fixture with [`HEAD`] as its registry.
const HEAD_EXPECTED: &str = "\
error[registry-kept] head:registry/identifiers.toml:1: the change removes the entry of code `h`, \
which the base holds at line 10
error[registry-kept] head:registry/identifiers.toml:7: the entry of code `g` changes its \
body_length, which a change of the set doesn't change
";

/// The fixture's sources, with the files that `partimento render` generates from them.
fn fixture() -> Tree {
    let mut tree = Tree::default();
    for (path, text) in FILES {
        let bytes = ByteVec::from(text.to_string());
        tree.files.insert(path.to_string(), bytes);
    }
    let generated = partimento::render(&tree).expect("the fixture renders");
    tree.files.extend(generated.files);
    tree
}

/// The fixture with `text` as its registry.
fn with_registry(text: &str) -> Tree {
    let mut tree = fixture();
    let bytes = ByteVec::from(text.to_string());
    tree.files.insert(REGISTRY.to_string(), bytes);
    tree
}

/// The fixture with its spec.toml's line 9, which sets `paths.registry`, replaced by `line`.
fn with_registry_line(line: &str) -> Tree {
    let set = format!("registry = \"{REGISTRY}\"");
    assert!(SPEC.contains(&set), "spec.toml sets the registry");
    let spec = SPEC.replace(&set, line);
    let mut tree = fixture();
    tree.files.insert("spec.toml".to_string(), ByteVec::from(spec));
    tree
}

fn check(tree: &Tree) -> Report {
    partimento::check_default(tree, &Options::default())
}

/// The findings as the command prints them, one a line.
fn lines(findings: &[partimento::Finding]) -> String {
    let mut text = String::new();
    for finding in findings {
        text.push_str(&finding.to_string());
        text.push('\n');
    }
    text
}

#[test]
fn the_fixture_reads_its_registry_from_spec_toml() {
    let config = config::load(&fixture(), &mut Vec::new()).expect("spec.toml loads");
    assert_eq!(config.paths.registry.as_deref(), Some(REGISTRY));
    let report = check(&fixture());
    assert!(report.findings.is_empty(), "{}", lines(&report.findings));
    assert!(!report.failed());
}

#[test]
fn a_registry_of_comment_lines_alone_or_empty_passes() {
    for text in [COMMENTS, ""] {
        let report = check(&with_registry(text));
        assert!(report.findings.is_empty(), "{text:?}: {}", lines(&report.findings));
    }
}

#[test]
fn each_broken_entry_fails_at_its_line() {
    let report = check(&with_registry(BROKEN));
    assert_eq!(lines(&report.findings), BROKEN_EXPECTED);
    assert!(report.failed());
    for finding in &report.findings {
        assert_eq!(finding.rule, registry::RULE, "{finding}");
        assert!(finding.requirement.is_none(), "{finding}");
    }
}

#[test]
fn a_missing_registry_fails_at_its_path() {
    let mut tree = fixture();
    tree.files.remove(REGISTRY);
    let report = check(&tree);
    let expected = "error[registry] registry/identifiers.toml:1: the registry file that \
                    `paths.registry` states is missing\n";
    assert_eq!(lines(&report.findings), expected);
    assert!(report.failed());
}

#[test]
fn a_registry_that_doesnt_parse_fails_at_its_line() {
    // An `entry` that isn't an array of tables, a key other than `entry`, and a file that isn't
    // TOML, each on line 2.
    let cases = [
        "# A registry.\nentry = 5\n",
        "# A registry.\n[kind]\ncode = \"g\"\n",
        "# A registry.\n[[entry]\n",
    ];
    for text in cases {
        let report = check(&with_registry(text));
        assert_eq!(report.findings.len(), 1, "{text:?}: {}", lines(&report.findings));
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "parse", "{text:?}");
        assert_eq!(finding.file, REGISTRY, "{text:?}");
        assert_eq!(finding.line, 2, "{text:?}");
        assert!(report.failed(), "{text:?}");
    }
}

#[test]
fn a_set_without_paths_registry_doesnt_read_a_registry() {
    let mut tree = with_registry_line("");
    let config = config::load(&tree, &mut Vec::new()).expect("spec.toml loads");
    assert_eq!(config.paths.registry, None);
    for text in [None, Some(BROKEN)] {
        match text {
            None => tree.files.remove(REGISTRY),
            Some(text) => tree.files.insert(REGISTRY.to_string(), ByteVec::from(text)),
        };
        let report = check(&tree);
        assert!(report.findings.is_empty(), "{}", lines(&report.findings));
    }
}

#[test]
fn paths_registry_outside_the_tree_or_in_a_directory_of_the_set_fails() {
    let cases = [
        (
            "registry = \"../identifiers.toml\"",
            "`paths.registry` is `../identifiers.toml`, which isn't a file of the tree",
        ),
        ("registry = \"\"", "`paths.registry` is ``, which isn't a file of the tree"),
        (
            "registry = \"rfcs/identifiers.toml\"",
            "`paths.registry` is `rfcs/identifiers.toml`, which overlaps `paths.rfcs`, `rfcs`",
        ),
        (
            "registry = \"rendered\"",
            "`paths.registry` is `rendered`, which overlaps `paths.rendered`, `rendered`",
        ),
    ];
    for (line, message) in cases {
        let report = check(&with_registry_line(line));
        let expected = format!("error[config] spec.toml:9: {message}\n");
        assert_eq!(lines(&report.findings), expected, "{line}");
        assert!(report.failed(), "{line}");
    }
}

#[test]
fn a_change_that_removes_or_alters_an_entry_fails_the_diff() {
    let (base, head) = (fixture(), with_registry(HEAD));
    let findings = partimento::diff(&base, &head).expect_err("the change alters the registry");
    assert_eq!(lines(&findings), HEAD_EXPECTED);
    for finding in &findings {
        assert_eq!(finding.rule, registry::KEPT, "{finding}");
    }
    let versions = partimento::contract_versions(&base, &head);
    assert_eq!(lines(&versions.expect_err("the change alters it")), HEAD_EXPECTED);
    let both = partimento::diff_with_versions(&base, &head);
    assert_eq!(lines(&both.expect_err("the change alters it")), HEAD_EXPECTED);
}

#[test]
fn a_change_of_each_kept_field_fails_the_diff() {
    let cases = [
        ("identifies = \"a greeting\"", "identifies = \"a hello\"", 5, "identifies"),
        ("format = 1", "format = 3", 6, "format"),
        ("min = 8, max = 8", "min = 8, max = 10", 7, "body_length"),
    ];
    for (old, new, line, field) in cases {
        let head = with_registry(&ENTRIES.replacen(old, new, 1));
        let findings = partimento::diff(&fixture(), &head).expect_err(field);
        let expected = format!(
            "error[registry-kept] head:registry/identifiers.toml:{line}: the entry of code `g` \
             changes its {field}, which a change of the set doesn't change\n"
        );
        assert_eq!(lines(&findings), expected, "{field}");
    }
}

#[test]
fn a_change_that_adds_an_entry_or_changes_defined_by_passes_the_diff() {
    let added = "\n[[entry]]\ncode = \"r\"\nidentifies = \"a reply\"\nformat = 1\n\
                 body_length = { min = 4, max = 4 }\ndefined_by = \"EX-0001-002\"\n";
    let defined_by = ENTRIES.replacen("\"EX-0001-001\"", "\"EX-0001-002\"", 1);
    let head = with_registry(&format!("{defined_by}{added}"));
    let changes = partimento::diff(&fixture(), &head).expect("the change keeps each entry");
    assert!(changes.is_empty(), "{changes:?}");
    assert!(check(&head).findings.is_empty(), "{}", lines(&check(&head).findings));
}

#[test]
fn a_head_without_a_registry_removes_each_entry() {
    let mut head = with_registry_line("");
    head.files.remove(REGISTRY);
    let findings = partimento::diff(&fixture(), &head).expect_err("the change removes them");
    let expected = "\
error[registry-kept] head:registry/identifiers.toml:1: the change removes the entry of code `g`, \
which the base holds at line 3
error[registry-kept] head:registry/identifiers.toml:1: the change removes the entry of code `h`, \
which the base holds at line 10
";
    assert_eq!(lines(&findings), expected);
}

#[test]
fn a_head_whose_registry_doesnt_parse_isnt_compared() {
    let head = with_registry("entry = 5\n");
    let changes = partimento::diff(&fixture(), &head).expect("the check reports the parse");
    assert!(changes.is_empty(), "{changes:?}");
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use partimento::Tree;

    use super::{fixture, with_registry, BROKEN, BROKEN_EXPECTED, HEAD, HEAD_EXPECTED, REGISTRY};

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

    /// The command reads the registry from the path spec.toml sets: where it didn't read it, the
    /// check of the fixture would report the registry as missing.
    #[test]
    fn the_command_reads_the_registry() {
        let dir = scratch("registry-read");
        write(&dir, &fixture());
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(0));
    }

    #[test]
    fn the_command_prints_the_findings_of_a_broken_registry() {
        let dir = scratch("registry-broken");
        write(&dir, &with_registry(BROKEN));
        let dir = dir.to_str().expect("the path is UTF-8");
        let output = run(&["check", "--root", dir]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), BROKEN_EXPECTED);
        assert_eq!(output.status.code(), Some(1));
    }

    #[test]
    fn the_command_prints_the_diff_of_a_change_that_alters_the_registry() {
        let root = scratch("registry-diff");
        let (base, head) = (root.join("base"), root.join("head"));
        write(&base, &fixture());
        write(&head, &with_registry(HEAD));
        let base = base.to_str().expect("the path is UTF-8");
        let head = head.to_str().expect("the path is UTF-8");
        let output = run(&["diff", base, head]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), HEAD_EXPECTED);
        assert_eq!(output.status.code(), Some(1));
    }

    #[cfg(unix)]
    #[test]
    fn a_registry_that_is_a_symbolic_link_exits_with_2() {
        let dir = scratch("registry-link");
        let mut tree = fixture();
        tree.files.remove(REGISTRY);
        write(&dir, &tree);
        let target = dir.join("elsewhere.toml");
        std::fs::write(&target, "").expect("the file is written");
        std::fs::create_dir_all(dir.join("registry")).expect("the directory is made");
        let link = dir.join(REGISTRY);
        std::os::unix::fs::symlink(&target, &link).expect("the link is made");
        let output = run(&["check", "--root", dir.to_str().expect("the path is UTF-8")]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(stderr.contains("symbolic link"), "{stderr}");
    }
}
