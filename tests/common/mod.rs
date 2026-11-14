//! The fixture loader the test files share: the `fixture!` macro, which embeds a fixture's files
//! at compile time, and the tree and the printed findings of a check.

// Each test file uses a part of the module alone.
#![allow(dead_code, unused_macros)]

use std::fmt::Display;

use partimento::{ByteVec, Report, Tree};

/// A fixture: the files of its tree, by their paths, and the findings the check prints for it.
pub struct Fixture {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str)],
    pub expected: &'static str,
}

/// One file of a fixture's tree, by its path in the tree and its text.
macro_rules! embed {
    ($name:literal, $path:literal) => {
        ($path, include_str!(concat!("fixtures/", $name, "/", $path)))
    };
}

/// A fixture by its directory under tests/fixtures, with the paths of its files beside its
/// spec.toml, its RFC index, and the rfc.toml and 01-greeting.toml of its RFC 0001, which come
/// with each fixture. Each path is read from the test file that invokes the macro, in tests/.
macro_rules! fixture {
    ($name:literal $(, $path:literal)*) => {
        $crate::common::Fixture {
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

pub fn tree(fixture: &Fixture) -> Tree {
    tree_of(fixture.files)
}

/// A tree of the files, by their paths.
pub fn tree_of(files: &[(&str, &str)]) -> Tree {
    let mut tree = Tree::default();
    for (path, text) in files {
        let bytes = ByteVec::from(text.to_string());
        tree.files.insert(path.to_string(), bytes);
    }
    tree
}

/// The findings as the command prints them, one a line.
pub fn lines(report: &Report) -> String {
    each_line(&report.findings)
}

/// The items, findings or changes, as the command prints them, one a line.
pub fn each_line(items: &[impl Display]) -> String {
    let mut text = String::new();
    for item in items {
        text.push_str(&item.to_string());
        text.push('\n');
    }
    text
}

/// Whether the fixture's expected findings fail the check.
pub fn fails(fixture: &Fixture) -> bool {
    let error = |line: &str| line.starts_with("error");
    fixture.expected.lines().any(error)
}
