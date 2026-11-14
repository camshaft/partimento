//! A check with a cache returns the report of a check without one, before and after a change to
//! one section file, and reads each section file that didn't change from the cache.
//!
//! The fixture's files are embedded at compile time, so the library's tests read no file and run
//! on wasm32 too.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use partimento::{ByteVec, Cache, Finding, Key, Options, Report, Tree};

/// The backend each check of these tests checks with.
type Backend = partimento::solver::FormatOnly;

/// One file of fixture 02-duplicate-identifier, by its path in the tree and its text.
macro_rules! fixture_file {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!("fixtures/02-duplicate-identifier/", $path)),
        )
    };
}

/// The files of fixture 02-duplicate-identifier, whose two section files hold the same
/// identifier.
const FILES: &[(&str, &str)] = &[
    fixture_file!("spec.toml"),
    fixture_file!("rfcs/index.toml"),
    fixture_file!("rfcs/0001-greetings/rfc.toml"),
    fixture_file!("rfcs/0001-greetings/01-greeting.toml"),
    fixture_file!("rfcs/0001-greetings/02-farewell.toml"),
];

/// The fixture's second section file.
const FAREWELL: &str = "rfcs/0001-greetings/02-farewell.toml";

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

/// A cache that returns each artifact it keeps with `damage` done to its bytes.
struct Damaged {
    kept: Memory,
    damage: fn(&mut Vec<u8>),
}

impl Cache for Damaged {
    fn get(&self, key: &Key) -> Option<ByteVec> {
        let artifact = self.kept.get(key)?;
        let mut bytes = artifact.copy_to_bytes().to_vec();
        (self.damage)(&mut bytes);
        Some(ByteVec::from(bytes))
    }

    fn put(&self, key: Key, artifact: ByteVec) {
        self.kept.put(key, artifact);
    }
}

fn emptied(bytes: &mut Vec<u8>) {
    bytes.clear();
}

fn cut_short(bytes: &mut Vec<u8>) {
    bytes.pop();
}

fn lengthened(bytes: &mut Vec<u8>) {
    bytes.push(0);
}

fn fixture() -> Tree {
    let mut tree = Tree::default();
    for (path, text) in FILES {
        tree.files.insert(path.to_string(), ByteVec::from(*text));
    }
    tree
}

/// The fixture's tree with `text` at `path`.
fn with_file(path: &str, text: String) -> Tree {
    let mut tree = fixture();
    tree.files.insert(path.to_string(), ByteVec::from(text));
    tree
}

/// The text of the fixture's file at `path`.
fn text(path: &str) -> &'static str {
    let file = FILES.iter().find(|(file, _)| *file == path);
    file.expect("the fixture holds the file").1
}

fn uncached(tree: &Tree) -> Report {
    partimento::check::<Backend>(tree, &Options::default())
}

fn cached(tree: &Tree, cache: &impl Cache) -> Report {
    partimento::check_cached::<Backend, _>(tree, &Options::default(), cache)
}

#[test]
fn a_second_check_reads_each_section_from_the_cache() {
    let (tree, cache) = (fixture(), Memory::default());
    let expected = uncached(&tree);
    assert!(expected.failed());
    assert_eq!(cached(&tree, &cache), expected);
    assert_eq!(cache.kept.borrow().len(), 2);
    assert_eq!(cache.returned.get(), 0);
    assert_eq!(cached(&tree, &cache), expected);
    assert_eq!(cache.returned.get(), 2);
}

#[test]
fn a_check_after_a_change_to_one_section_returns_the_report_without_the_cache() {
    let (tree, cache) = (fixture(), Memory::default());
    let before = cached(&tree, &cache);
    let farewell = text(FAREWELL).replace("EX-0001-001", "EX-0001-002");
    let changed = with_file(FAREWELL, farewell);
    let after = cached(&changed, &cache);
    assert_eq!(after, uncached(&changed));
    assert_ne!(after, before);
    assert!(!after.failed());
    // The check read 01-greeting.toml from the cache, and 02-farewell.toml from the tree.
    assert_eq!(cache.returned.get(), 1);
    assert_eq!(cache.kept.borrow().len(), 3);
}

#[test]
fn a_file_that_doesnt_parse_reports_the_same_findings_from_the_cache() {
    let broken = "rfcs/0001-greetings/03-broken.toml";
    let tree = with_file(broken, "[[requirement]\n".to_string());
    let cache = Memory::default();
    let expected = uncached(&tree);
    let in_broken = |finding: &Finding| finding.file == broken;
    assert!(expected.findings.iter().any(in_broken));
    assert_eq!(cached(&tree, &cache), expected);
    assert_eq!(cached(&tree, &cache), expected);
    assert_eq!(cache.returned.get(), 3);
}

#[test]
fn a_damaged_artifact_is_read_from_the_file_again() {
    let tree = fixture();
    let expected = uncached(&tree);
    for damage in [emptied, cut_short, lengthened] {
        let cache = Damaged {
            kept: Memory::default(),
            damage,
        };
        assert_eq!(cached(&tree, &cache), expected);
        assert_eq!(cached(&tree, &cache), expected);
        assert_eq!(cache.kept.returned.get(), 2);
    }
}

#[cfg(feature = "cli")]
mod binary {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    fn root() -> PathBuf {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests/fixtures/02-duplicate-identifier")
    }

    fn run(args: &[&str]) -> Output {
        let program = env!("CARGO_BIN_EXE_partimento");
        let output = Command::new(program).args(args).output();
        output.expect("the command runs")
    }

    /// Runs `check` on the fixture with its cache in `cache`.
    fn check(cache: &Path) -> Output {
        let root = root();
        let root = root.to_str().expect("the path is UTF-8");
        let cache = cache.to_str().expect("the path is UTF-8");
        run(&["check", "--root", root, "--cache", cache])
    }

    /// An empty directory under the build's temporary directory, for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    /// Each file in `dir` with its bytes, in the order of its path.
    fn kept(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let entries = std::fs::read_dir(dir).expect("the cache's directory reads");
        let mut files: Vec<(PathBuf, Vec<u8>)> = entries
            .map(|entry| {
                let path = entry.expect("the entry reads").path();
                let bytes = std::fs::read(&path).expect("the file reads");
                (path, bytes)
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn a_second_run_with_the_cache_prints_the_same_findings() {
        let cache = scratch("cache-runs").join("cache");
        let first = check(&cache);
        let expected = include_str!("fixtures/02-duplicate-identifier/expected.txt");
        assert_eq!(String::from_utf8_lossy(&first.stdout), expected);
        assert_eq!(first.status.code(), Some(1));
        assert_eq!(kept(&cache).len(), 2);
        let second = check(&cache);
        assert_eq!(second.stdout, first.stdout);
        assert_eq!(second.status.code(), Some(1));
    }

    #[test]
    fn a_damaged_entry_is_read_from_the_file_again() {
        let cache = scratch("cache-damaged").join("cache");
        let first = check(&cache);
        let entries = kept(&cache);
        for (path, _) in &entries {
            std::fs::write(path, b"damaged").expect("the entry is written");
        }
        let second = check(&cache);
        assert_eq!(second.stdout, first.stdout);
        assert_eq!(second.status.code(), Some(1));
        assert_eq!(kept(&cache), entries);
    }

    #[test]
    fn a_cache_that_isnt_a_directory_exits_with_2() {
        let file = scratch("cache-file").join("file");
        std::fs::write(&file, b"").expect("the file is written");
        let output = check(&file);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let output = run(&["check", "--cache"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}
