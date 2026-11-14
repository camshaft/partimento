//! Checks and renders structured RFC specifications.
//!
//! The library reads only the [`Tree`] it is given: it doesn't read files, start processes, read
//! the environment or the clock, or print anything, and the same tree always gives the same
//! report.

use std::collections::BTreeMap;

pub use etude_bytevec::ByteVec;
use serde::Serialize;

// The library's `render` and the drift rule of `check` call it once the model reads sections.
#[cfg_attr(not(test), allow(dead_code))]
mod render;

/// The files of a specification tree, by their paths relative to its root, with `/` between the
/// parts of a path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub files: BTreeMap<String, ByteVec>,
}

/// What a check does beyond its rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Turns the warnings of unresolved relations into failures.
    pub final_pass: bool,
}

/// Whether a finding fails the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One finding of a rule, at a line of a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub rule: String,
    pub file: String,
    pub line: u32,
    pub requirement: Option<String>,
    pub message: String,
}
