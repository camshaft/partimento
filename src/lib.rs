//! Checks and renders structured RFC specifications.
//!
//! The library reads only the [`Tree`] it is given: it doesn't read files, start processes, read
//! the environment or the clock, or print anything, and two checks of the same tree return the
//! same report.

use std::collections::BTreeMap;
use std::fmt;

pub use etude_bytevec::ByteVec;
use serde::Serialize;

mod artifact;
pub mod checks;
pub mod config;
mod diff;
pub mod logic;
pub mod model;
mod render;
pub mod rules;
pub mod solver;
mod source;

#[cfg(not(any(feature = "batsat", feature = "varisat")))]
compile_error!("enable a solver backend: the `batsat` feature or the `varisat` feature");

pub use diff::{Binding, Change, ChangeKind};

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
        fails(&self.findings)
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

/// Checks the tree with the default backend: batsat where its feature is on, else varisat.
pub fn check_default(tree: &Tree, options: &Options) -> Report {
    check::<solver::DefaultBackend>(tree, options)
}

/// Checks the tree with the solver backend `B`, keeping the artifacts it builds in `cache`.
pub fn check_cached<B: solver::Backend + Default, C: Cache>(
    tree: &Tree,
    options: &Options,
    cache: &C,
) -> Report {
    let mut findings = Vec::new();
    // A set that breaks a format rule doesn't render, so the drift of its generated files is
    // checked only once its format rules pass.
    if let Some((config, set)) = load(tree, cache, &mut findings) {
        let renders = !fails(&findings);
        checks::check::<B>(&config, &set, &mut findings);
        if renders {
            let paths = render::Paths::of(&config.paths);
            let generated = render::generate(paths, &render::sections(&set));
            findings.extend(render::drift(paths, tree, &generated));
        }
    }
    if options.final_pass {
        for finding in &mut findings {
            if finding.rule == rules::RELATION_UNRESOLVED {
                finding.severity = Severity::Error;
            }
        }
    }
    sort(&mut findings);
    Report { findings }
}

/// Each requirement added, changed and retired from `base` to `head`, two trees of one set, in
/// the order of its identifier; or, where either tree fails a format rule, each finding that
/// fails, the base's first, with its file's path after the tree's side, `base:` or `head:`.
pub fn diff(base: &Tree, head: &Tree) -> Result<Vec<Change>, Vec<Finding>> {
    diff::changes(base, head)
}

/// Renders the tree's generated files: the text of each section and one requirement file for each
/// of its requirements, under the paths spec.toml sets. The tree holds the generated files alone.
/// A set that breaks a format rule doesn't render: the error holds the findings of the format
/// rules, in the order of a report.
pub fn render(tree: &Tree) -> Result<Tree, Vec<Finding>> {
    let mut findings = Vec::new();
    match load(tree, &NoCache, &mut findings) {
        Some((config, set)) if !fails(&findings) => {
            let paths = render::Paths::of(&config.paths);
            Ok(render::generate(paths, &render::sections(&set)).tree)
        }
        _ => {
            sort(&mut findings);
            Err(findings)
        }
    }
}

/// Reads the configuration and the RFCs, each section file through `cache`, and checks the
/// format rules.
fn load<C: Cache>(
    tree: &Tree,
    cache: &C,
    findings: &mut Vec<Finding>,
) -> Option<(config::Config, model::Set)> {
    let config = config::load(tree, findings)?;
    let set = model::load_cached(tree, &config, cache, findings);
    rules::check(&config, &set, findings);
    Some((config, set))
}

/// Whether a finding fails the check.
fn fails(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity == Severity::Error)
}

fn sort(findings: &mut [Finding]) {
    findings.sort_by(|a, b| order(a).cmp(&order(b)));
}

fn order(finding: &Finding) -> (&str, u32, &str, Option<&str>, &str) {
    let requirement = finding.requirement.as_deref();
    let (file, rule) = (finding.file.as_str(), finding.rule.as_str());
    let message = finding.message.as_str();
    (file, finding.line, rule, requirement, message)
}
