//! The kind tables of section files, through fixture 47-kinds, whose first section file names two
//! kinds of self-describing identifier in `[[kind]]` tables, whose second section file holds a
//! requirement and no kind table, and whose registry holds an entry of each format of each kind:
//! the check of the fixture, of a registry that doesn't hold an entry of a format of a kind table,
//! of a set whose spec.toml doesn't set `paths.registry`, of a set without its registry file, of
//! entries that don't match the kind tables, of kind tables that each break one rule of their
//! fields, and of a kind table whose name an earlier one holds; and a check through a cache, which
//! reads the kind tables from the section file's cached load.
//!
//! The library's tests build each tree in memory from files embedded at compile time, so they
//! read no file and run on wasm32 too.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use partimento::{registry, ByteVec, Cache, Key, Options, Report, Tree};

/// The backend the check through a cache checks with.
type Backend = partimento::solver::DefaultBackend;

/// One file of the fixture, by its path in the fixture's tree and its text.
macro_rules! embed {
    ($path:literal) => {
        ($path, include_str!(concat!("fixtures/47-kinds/", $path)))
    };
}

/// The fixture's section file: its kind table `a greeting` holds formats 1 and 2 at line 10, and
/// its kind table `a farewell` holds format 1 at line 14.
const SECTION: &str = "rfcs/0001-greetings/01-greeting.toml";

/// The path of the registry that the fixture's spec.toml sets, at its line 9.
const REGISTRY: &str = "registry/identifiers.toml";

/// The fixture's spec.toml.
const SPEC: &str = include_str!("fixtures/47-kinds/spec.toml");

/// The fixture's sources. Its first section file holds EX-0001-001 and EX-0001-002, and its second
/// EX-0001-003. Its registry holds entry `g`, of `a greeting` and format 1, entry `h`, of
/// `a greeting` and format 2, and entry `f`, of `a farewell` and format 1.
const FILES: &[(&str, &str)] = &[
    ("spec.toml", SPEC),
    embed!("rfcs/index.toml"),
    embed!("rfcs/0001-greetings/rfc.toml"),
    embed!("rfcs/0001-greetings/01-greeting.toml"),
    embed!("rfcs/0001-greetings/02-parting.toml"),
    embed!("registry/identifiers.toml"),
];

/// The text of the fixture's first section file.
const GREETING: &str = include_str!("fixtures/47-kinds/rfcs/0001-greetings/01-greeting.toml");

/// A registry whose entries `g` and `h` pass, its entry `f` names in its defined_by, at line 24,
/// the requirement of the second section file, its entry `w` names `a wave` in its identifies, at
/// line 29, and its entry `t` holds format 3 of `a greeting`, at line 38.
const ENTRIES: &str = include_str!("fixtures/47-kinds/cases/entries.toml");

/// The findings of the fixture with [`ENTRIES`] as its registry: each field of an entry that
/// doesn't match the kind tables.
const ENTRIES_EXPECTED: &str = "\
error[registry] registry/identifiers.toml:24: defined_by is `EX-0001-003`, which isn't a \
requirement of `rfcs/0001-greetings/01-greeting.toml`, the section file of the kind `a farewell`
error[registry] registry/identifiers.toml:29: identifies is `a wave`, which isn't a kind \
table's name
error[registry] registry/identifiers.toml:38: format is 3, which isn't one of the formats of the \
kind `a greeting`
";

/// The findings of the fixture without its registry file: the missing file, and each format of
/// each kind table.
const MISSING_FILE_EXPECTED: &str = "\
error[registry] registry/identifiers.toml:1: the registry file that `paths.registry` states is \
missing
error[registry] rfcs/0001-greetings/01-greeting.toml:10: the kind `a greeting` has format 1, and \
the registry file `registry/identifiers.toml` that `paths.registry` states is missing, so the set \
doesn't hold a registry entry of it
error[registry] rfcs/0001-greetings/01-greeting.toml:10: the kind `a greeting` has format 2, and \
the registry file `registry/identifiers.toml` that `paths.registry` states is missing, so the set \
doesn't hold a registry entry of it
error[registry] rfcs/0001-greetings/01-greeting.toml:14: the kind `a farewell` has format 1, and \
the registry file `registry/identifiers.toml` that `paths.registry` states is missing, so the set \
doesn't hold a registry entry of it
";

/// The findings of the fixture whose first section file holds a third kind table, at line 16,
/// whose name `a greeting` the first one holds, with formats 1 and 3 at line 18: the repeated
/// name, and format 3, which no entry holds, though entry `g` holds format 1.
const REPEATED_EXPECTED: &str = "\
error[kind] rfcs/0001-greetings/01-greeting.toml:17: the kind table at \
rfcs/0001-greetings/01-greeting.toml:8 holds the name `a greeting` too
error[registry] rfcs/0001-greetings/01-greeting.toml:18: the kind `a greeting` has format 3, and \
the registry `registry/identifiers.toml` doesn't hold an entry of identifies `a greeting` and \
format 3
";

/// A registry that holds entry `g` alone.
const MISSING: &str = include_str!("fixtures/47-kinds/cases/missing.toml");

/// The findings of the fixture with [`MISSING`] as its registry: each format of a kind table that
/// no entry holds.
const MISSING_EXPECTED: &str = "\
error[registry] rfcs/0001-greetings/01-greeting.toml:10: the kind `a greeting` has format 2, and \
the registry `registry/identifiers.toml` doesn't hold an entry of identifies `a greeting` and \
format 2
error[registry] rfcs/0001-greetings/01-greeting.toml:14: the kind `a farewell` has format 1, and \
the registry `registry/identifiers.toml` doesn't hold an entry of identifies `a farewell` and \
format 1
";

/// The findings of the fixture without `paths.registry`: each format of each kind table.
const UNREGISTERED_EXPECTED: &str = "\
error[registry] rfcs/0001-greetings/01-greeting.toml:10: the kind `a greeting` has format 1, and \
spec.toml doesn't set `paths.registry`, so the set doesn't hold a registry entry of it
error[registry] rfcs/0001-greetings/01-greeting.toml:10: the kind `a greeting` has format 2, and \
spec.toml doesn't set `paths.registry`, so the set doesn't hold a registry entry of it
error[registry] rfcs/0001-greetings/01-greeting.toml:14: the kind `a farewell` has format 1, and \
spec.toml doesn't set `paths.registry`, so the set doesn't hold a registry entry of it
";

/// A section file whose first two kind tables are the fixture's, and each kind table after them
/// breaks one rule of its fields.
const BROKEN: &str = include_str!("fixtures/47-kinds/cases/broken.toml");

/// The findings the check prints for the fixture with [`BROKEN`] as its section file.
const BROKEN_EXPECTED: &str = include_str!("fixtures/47-kinds/cases/expected.txt");

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

/// The fixture with `text` at `path`.
fn with_file(path: &str, text: &str) -> Tree {
    let mut tree = fixture();
    let bytes = ByteVec::from(text.to_string());
    tree.files.insert(path.to_string(), bytes);
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

/// A cache that keeps each artifact in memory, and counts the artifacts it returns.
#[derive(Default)]
struct Memory {
    kept: RefCell<BTreeMap<Key, ByteVec>>,
    returned: Cell<usize>,
}

impl Cache for Memory {
    fn get(&self, key: &Key) -> Option<ByteVec> {
        let artifact = self.kept.borrow().get(key).cloned();
        if artifact.is_some() {
            self.returned.set(self.returned.get() + 1);
        }
        artifact
    }

    fn put(&self, key: Key, artifact: ByteVec) {
        self.kept.borrow_mut().insert(key, artifact);
    }
}

#[test]
fn a_registry_with_an_entry_of_each_format_of_each_kind_table_passes() {
    let report = check(&fixture());
    assert!(report.findings.is_empty(), "{}", lines(&report.findings));
    assert!(!report.failed());
}

#[test]
fn each_format_without_an_entry_fails_at_its_kind_table() {
    let report = check(&with_file(REGISTRY, MISSING));
    assert_eq!(lines(&report.findings), MISSING_EXPECTED);
    assert!(report.failed());
    for finding in &report.findings {
        assert_eq!(finding.rule, registry::RULE, "{finding}");
        assert_eq!(finding.file, SECTION, "{finding}");
        assert!(finding.requirement.is_none(), "{finding}");
    }
}

#[test]
fn a_set_without_paths_registry_fails_at_each_format_of_each_kind_table() {
    let set = format!("registry = \"{REGISTRY}\"");
    assert!(SPEC.contains(&set), "spec.toml sets the registry");
    let tree = with_file("spec.toml", &SPEC.replace(&set, ""));
    let report = check(&tree);
    assert_eq!(lines(&report.findings), UNREGISTERED_EXPECTED);
    assert!(report.failed());
}

#[test]
fn each_broken_kind_table_fails_at_its_line() {
    let report = check(&with_file(SECTION, BROKEN));
    assert_eq!(lines(&report.findings), BROKEN_EXPECTED);
    assert!(report.failed());
    for finding in &report.findings {
        assert!(
            [registry::KIND, registry::RULE].contains(&finding.rule.as_str()),
            "{finding}"
        );
    }
}

#[test]
fn a_set_without_its_registry_file_fails_at_each_format_of_each_kind_table() {
    let mut tree = fixture();
    tree.files.remove(REGISTRY);
    let report = check(&tree);
    assert_eq!(lines(&report.findings), MISSING_FILE_EXPECTED);
    assert!(report.failed());
}

#[test]
fn each_entry_that_doesnt_match_the_kind_tables_fails_at_its_field() {
    let report = check(&with_file(REGISTRY, ENTRIES));
    assert_eq!(lines(&report.findings), ENTRIES_EXPECTED);
    assert!(report.failed());
    for finding in &report.findings {
        assert_eq!(finding.rule, registry::RULE, "{finding}");
        assert_eq!(finding.file, REGISTRY, "{finding}");
    }
}

#[test]
fn a_kind_table_whose_name_an_earlier_one_holds_fails_at_each_format_without_an_entry() {
    let repeated = "\n[[kind]]\nname = \"a greeting\"\nformats = [1, 3]\n\n[[requirement]]\n";
    let section = GREETING.replacen("\n[[requirement]]\n", repeated, 1);
    assert_ne!(section, GREETING, "the section file holds a requirement");
    let report = check(&with_file(SECTION, &section));
    assert_eq!(lines(&report.findings), REPEATED_EXPECTED);
    assert!(report.failed());
}

#[test]
fn a_check_through_a_cache_reads_the_kind_tables_from_the_cached_load() {
    let (tree, options) = (with_file(REGISTRY, MISSING), Options::default());
    let cache = Memory::default();
    let check = || partimento::check_cached::<Backend, _>(&tree, &options, &cache);
    let first = check();
    assert_eq!(cache.returned.get(), 0);
    let second = check();
    assert!(cache.returned.get() > 0, "the second check reads it");
    assert_eq!(lines(&second.findings), MISSING_EXPECTED);
    assert_eq!(second, first);
}
