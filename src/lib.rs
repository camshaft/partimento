//! Checks and renders structured RFC specifications.
//!
//! The library reads only the [`Tree`] it is given: it doesn't read files, start processes, read
//! the environment or the clock, or print anything, and two checks of the same tree return the
//! same report.

use std::collections::BTreeMap;
use std::fmt;

pub use etude_bytevec::ByteVec;
use serde::Serialize;

pub mod config;
pub mod model;
pub mod rules;
pub mod solver;
mod source;

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

impl Severity {
    /// The severity as the text and the JSON output write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
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

impl Finding {
    /// A finding that fails the check.
    pub fn error(
        rule: &str,
        file: &str,
        line: u32,
        requirement: Option<&str>,
        message: impl Into<String>,
    ) -> Finding {
        let message = message.into();
        Finding::new(Severity::Error, rule, file, line, requirement, message)
    }

    /// A finding that the check reports and that doesn't fail it.
    pub fn warning(
        rule: &str,
        file: &str,
        line: u32,
        requirement: Option<&str>,
        message: impl Into<String>,
    ) -> Finding {
        let message = message.into();
        Finding::new(Severity::Warning, rule, file, line, requirement, message)
    }

    fn new(
        severity: Severity,
        rule: &str,
        file: &str,
        line: u32,
        requirement: Option<&str>,
        message: String,
    ) -> Finding {
        Finding {
            severity,
            rule: rule.to_string(),
            file: file.to_string(),
            line,
            requirement: requirement.map(str::to_string),
            message,
        }
    }
}

/// The text form of a finding: `<severity>[<rule>] <file>:<line>: <requirement>: <message>`,
/// without the requirement's part where the finding isn't about one requirement.
impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = self.severity.as_str();
        let (rule, file, line) = (&self.rule, &self.file, self.line);
        write!(f, "{severity}[{rule}] {file}:{line}: ")?;
        if let Some(requirement) = &self.requirement {
            write!(f, "{requirement}: ")?;
        }
        f.write_str(&self.message)
    }
}

/// The result of a check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Each finding, in the order of its file, its line, its rule, its requirement and its
    /// message.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether a finding fails the check.
    pub fn failed(&self) -> bool {
        let failing = |finding: &Finding| finding.severity == Severity::Error;
        self.findings.iter().any(failing)
    }
}

/// The digest of an artifact's inputs, which a [`Cache`] keeps the artifact under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub [u8; 32]);

/// A store of the artifacts a check builds, such as a parsed section, by the digest of their
/// inputs. A cache returns only bytes it was given for the same key, so a check returns the same
/// report with any cache.
pub trait Cache {
    /// The artifact kept under the key, where the cache holds one.
    fn get(&self, key: &Key) -> Option<ByteVec>;

    /// Keeps the artifact under the key.
    fn put(&self, key: Key, artifact: ByteVec);
}

/// The cache that keeps nothing: every artifact is built each time.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCache;

impl Cache for NoCache {
    fn get(&self, _key: &Key) -> Option<ByteVec> {
        None
    }

    fn put(&self, _key: Key, _artifact: ByteVec) {}
}

/// Checks the tree with the solver backend `B`.
pub fn check<B: solver::Backend + Default>(tree: &Tree, options: &Options) -> Report {
    check_cached::<B, NoCache>(tree, options, &NoCache)
}

/// Checks the tree with the default backend.
pub fn check_default(tree: &Tree, options: &Options) -> Report {
    check::<solver::FormatOnly>(tree, options)
}

/// Checks the tree with the solver backend `B`, keeping the artifacts it builds in `cache`.
pub fn check_cached<B: solver::Backend + Default, C: Cache>(
    tree: &Tree,
    options: &Options,
    cache: &C,
) -> Report {
    // The artifacts that a cache keeps come with the solver backends.
    let _ = cache;
    let mut findings = Vec::new();
    if let Some(config) = config::load(tree, &mut findings) {
        let set = model::load(tree, &config, &mut findings);
        rules::check(&config, &set, &mut findings);
    }
    if options.final_pass {
        for finding in &mut findings {
            if finding.rule == rules::RELATION_UNRESOLVED {
                finding.severity = Severity::Error;
            }
        }
    }
    findings.sort_by(|a, b| order(a).cmp(&order(b)));
    Report { findings }
}

fn order(finding: &Finding) -> (&str, u32, &str, Option<&str>, &str) {
    let requirement = finding.requirement.as_deref();
    let (file, rule) = (finding.file.as_str(), finding.rule.as_str());
    let message = finding.message.as_str();
    (file, finding.line, rule, requirement, message)
}
