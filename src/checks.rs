//! Checks 1 to 5 of a specification's requirements.
//!
//! Check 1, that the formulas of the set hold together, and check 2, that each condition of a
//! requirement can hold while they do, read the formula set through [`Encoding`] and a
//! [`Backend`] alone, so they run the same under each backend. The other checks read the
//! requirements alone: each relation's target, the boundary between systems, cycles of
//! `implies` and `refines`, cycles of `supersedes`, a required requirement whose relations lead
//! to a forbidden one, and the bounds of each parameter. The composition, `logic` and `params`
//! fields are checked as the formula set is built.

use std::collections::{BTreeMap, BTreeSet};

use toml::Value;

use crate::config::Config;
use crate::logic::{parse, Condition, Encoding, Entry, Formula, FormulaSet, Origin};
use crate::model::{Block, Field, Set, State};
use crate::rules::{parse_identifier, RELATION_UNRESOLVED};
use crate::solver::{Answer, Backend, Lit};
use crate::{artifact, Cache, Finding, NoCache};

/// A minimal set of formulas that can't hold together (check 1).
pub const UNSATISFIABLE: &str = "unsatisfiable";
/// A condition that can't be true while the rest of the set holds (check 2).
pub const CONDITION_VACUOUS: &str = "condition-vacuous";
/// A relation, a part, a condition or a formula's atom that crosses from one system to another.
pub const SYSTEM_BOUNDARY: &str = "system-boundary";
/// A relation that doesn't hold a `kind` and a `target`, or whose kind isn't one, and each
/// requirement of a cycle of `supersedes` relations.
pub const RELATION: &str = "relation";
/// A relation's target, a part, a condition or a formula's atom that doesn't resolve.
pub const RELATION_TARGET: &str = "relation-target";
/// `implies` and `refines` relations that form a cycle.
pub const RELATION_CYCLE: &str = "relation-cycle";
/// A requirement required at the first level whose relations lead to one forbidden at it.
pub const POLARITY_CONFLICT: &str = "polarity-conflict";
/// A `compose` that doesn't match its `parts`.
pub const COMPOSITION: &str = "composition";
/// A `logic` formula that doesn't follow the grammar.
pub const LOGIC: &str = "logic";
/// A parameter that doesn't hold its fields, or whose bounds contradict each other.
pub const PARAMETER: &str = "parameter";

const RELATION_KINDS: &[&str] = &[
    "depends_on",
    "implies",
    "refines",
    "conflicts_with",
    "supersedes",
];
const BOUNDS: &[&str] = &["min", "max", "exact"];

/// Adds a finding for each of checks 1 to 5 that the set breaks, solving with a backend of type
/// `B`.
pub fn check<B: Backend + Default>(config: &Config, set: &Set, findings: &mut Vec<Finding>) {
    check_cached::<B, _>(config, set, &NoCache, findings);
}

/// Adds a finding for each of checks 1 to 5 that the set breaks, as [`check`] does, reading the
/// result of checks 1 and 2 from `cache` where it keeps one for the set's formulas.
pub(crate) fn check_cached<B: Backend + Default, C: Cache>(
    config: &Config,
    set: &Set,
    cache: &C,
    findings: &mut Vec<Finding>,
) {
    let mut cx = Context::new(config, set);
    cx.supersede();
    cx.compositions();
    for index in 0..cx.requirements.len() {
        let requirement = cx.requirements[index];
        let held = cx.held.get(requirement.id) == Some(&index);
        // A requirement that a `supersedes` relation retires isn't built, so none of its
        // fields reaches a check.
        if held && !cx.superseded.contains(requirement.id) {
            cx.build(requirement);
        }
    }
    cx.cycles();
    cx.polarity();
    cx.solve::<B, _>(cache);
    findings.append(&mut cx.found);
}

/// One requirement of the set, where its block holds an identifier.
#[derive(Clone, Copy)]
struct Requirement<'a> {
    id: &'a str,
    /// The number of the RFC whose directory holds it.
    rfc: u32,
    file: &'a str,
    /// The line of its `id`.
    line: u32,
    block: &'a Block,
}

/// A parameter's bound, and where it is stated.
#[derive(Clone, Copy)]
struct Bound<'a> {
    value: f64,
    file: &'a str,
    line: u32,
}

/// The bounds of one parameter across the set: the tightest minimum and maximum, and the first
/// exact value.
#[derive(Default)]
struct Interval<'a> {
    min: Option<Bound<'a>>,
    max: Option<Bound<'a>>,
    exact: Option<Bound<'a>>,
}

struct Context<'a> {
    config: &'a Config,
    requirements: Vec<Requirement<'a>>,
    /// The first requirement that holds each identifier, by its index.
    held: BTreeMap<&'a str, usize>,
    /// Where each identifier is retired.
    retired: BTreeMap<&'a str, (&'a str, u32)>,
    /// The RFCs that the index plans.
    planned: BTreeSet<u32>,
    /// The requirements that a composition holds as its parts, or as its exception.
    parts: BTreeSet<&'a str>,
    /// The requirements of the set that a `supersedes` relation retires.
    superseded: BTreeSet<&'a str>,
    /// The `implies` and `refines` relations, from requirement to target.
    cycle: BTreeMap<&'a str, BTreeSet<&'a str>>,
    /// The `depends_on`, `implies` and `refines` relations, from requirement to target.
    leads: BTreeMap<&'a str, BTreeSet<&'a str>>,
    bounds: BTreeMap<&'a str, Interval<'a>>,
    set: FormulaSet,
    found: Vec<Finding>,
}

impl<'a> Context<'a> {
    fn new(config: &'a Config, set: &'a Set) -> Self {
        let mut cx = Context {
            config,
            requirements: Vec::new(),
            held: BTreeMap::new(),
            retired: BTreeMap::new(),
            planned: BTreeSet::new(),
            parts: BTreeSet::new(),
            superseded: BTreeSet::new(),
            cycle: BTreeMap::new(),
            leads: BTreeMap::new(),
            bounds: BTreeMap::new(),
            set: FormulaSet::default(),
            found: Vec::new(),
        };
        if let Some(index) = &set.index {
            for entry in &index.rfc {
                if entry.state == State::Planned {
                    cx.planned.insert(entry.number);
                }
            }
        }
        for rfc in &set.rfcs {
            if let Some(meta) = &rfc.meta {
                for id in &meta.retired {
                    let at = (rfc.meta_file.as_str(), id.line);
                    cx.retired.entry(id.value.as_str()).or_insert(at);
                }
            }
            for section in &rfc.sections {
                for block in &section.requirements {
                    let Some(field) = block.field("id") else {
                        continue;
                    };
                    let Some(id) = field.value.as_str() else {
                        continue;
                    };
                    let index = cx.requirements.len();
                    cx.requirements.push(Requirement {
                        id,
                        rfc: rfc.number,
                        file: section.file.as_str(),
                        line: field.line,
                        block,
                    });
                    cx.held.entry(id).or_insert(index);
                }
            }
        }
        cx
    }

    /// Adds the requirement's formulas and conditions to the set, and checks its fields.
    fn build(&mut self, r: Requirement<'a>) {
        let block = r.block;
        let compose = block.text("compose");
        let compose_line = line_of(block, "compose");
        let parts = strings(block.field("parts"));
        let parts_line = block
            .field("parts")
            .map_or(compose_line, |field| field.line);
        let conditions = strings(block.field("conditions"));

        // A requirement holds by itself unless it holds under conditions, a composition holds
        // it as a part, or it holds where its exception is false.
        if conditions.is_empty() && !self.parts.contains(r.id) && compose != Some("unless") {
            self.push(Formula::atom(r.id), r, r.line);
        }
        if !conditions.is_empty() {
            let line = line_of(block, "conditions");
            for &condition in &conditions {
                self.resolve(r, condition, "the condition", line, false);
                self.boundary(r, condition, "the condition", line);
                let origin = origin(r, line);
                let atom = condition.to_owned();
                self.set.conditions.push(Condition { atom, origin });
            }
            let formula = all(atoms_of(&conditions)).implies(Formula::atom(r.id));
            self.push(formula, r, line);
        }

        match compose {
            Some(kind @ ("all_of" | "any_of" | "exactly_one")) => {
                if parts.is_empty() {
                    let message = format!(
                        "`{kind}` holds no part: `parts` lists the requirements it composes"
                    );
                    self.error(COMPOSITION, r, compose_line, message);
                } else {
                    for &part in &parts {
                        self.resolve(r, part, "the part", parts_line, false);
                        self.boundary(r, part, "the part", parts_line);
                    }
                    let body = match kind {
                        "all_of" => all(atoms_of(&parts)),
                        _ => any(atoms_of(&parts)),
                    };
                    self.push(Formula::atom(r.id).iff(body), r, compose_line);
                    if kind == "exactly_one" {
                        for (i, &first) in parts.iter().enumerate() {
                            for &second in &parts[i + 1..] {
                                let pair = vec![Formula::atom(first), Formula::atom(second)];
                                self.push(Formula::And(pair).not(), r, compose_line);
                            }
                        }
                    }
                }
            }
            Some("unless") => {
                if let [exception] = parts[..] {
                    self.resolve(r, exception, "the exception", parts_line, false);
                    self.boundary(r, exception, "the exception", parts_line);
                    let origin = origin(r, parts_line);
                    let atom = exception.to_owned();
                    self.set.conditions.push(Condition { atom, origin });
                    let formula = Formula::atom(r.id).iff(Formula::atom(exception).not());
                    self.push(formula, r, compose_line);
                } else {
                    let count = parts.len();
                    let message = format!(
                        "`unless` holds one part, its exception, and `parts` lists {count}"
                    );
                    self.error(COMPOSITION, r, parts_line, message);
                }
            }
            None | Some("atomic") if !parts.is_empty() => {
                let message = "an `atomic` requirement holds no parts: `compose` sets `all_of`, \
                               `any_of`, `exactly_one` or `unless` for them";
                self.error(COMPOSITION, r, parts_line, message.to_string());
            }
            _ => {}
        }

        self.relations(r);
        self.logic(r);
        self.params(r);
    }

    fn relations(&mut self, r: Requirement<'a>) {
        let Some(field) = r.block.field("relations") else {
            return;
        };
        let line = field.line;
        for item in field.value.as_array().into_iter().flatten() {
            let (Some(kind), Some(target)) = (text(item, "kind"), text(item, "target")) else {
                let message = "a relation holds `kind` and `target`, each a string";
                self.error(RELATION, r, line, message.to_string());
                continue;
            };
            if !RELATION_KINDS.contains(&kind) {
                let message = format!(
                    "`{kind}` isn't a relation kind: `depends_on`, `implies`, `refines`, \
                     `conflicts_with` or `supersedes`"
                );
                self.error(RELATION, r, line, message);
                continue;
            }
            let noun = "the relation's target";
            self.resolve(r, target, noun, line, kind == "supersedes");
            if kind == "supersedes" {
                continue;
            }
            self.boundary(r, target, noun, line);
            let (own, other) = (Formula::atom(r.id), Formula::atom(target));
            if kind == "conflicts_with" {
                self.push(Formula::And(vec![own, other]).not(), r, line);
                continue;
            }
            self.push(own.implies(other), r, line);
            self.leads.entry(r.id).or_default().insert(target);
            if kind != "depends_on" {
                self.cycle.entry(r.id).or_default().insert(target);
            }
        }
    }

    fn logic(&mut self, r: Requirement<'a>) {
        let Some(field) = r.block.field("logic") else {
            return;
        };
        let line = field.line;
        for source in strings(Some(field)) {
            match parse(source) {
                Err(error) => {
                    let message = format!("`{source}` isn't a formula: {error}");
                    self.error(LOGIC, r, line, message);
                }
                Ok(formula) => {
                    let mut names = BTreeSet::new();
                    names_of(&formula, &mut names);
                    for name in &names {
                        self.resolve(r, name, "the formula's atom", line, false);
                        self.boundary(r, name, "the formula's atom", line);
                    }
                    self.joined(r, &names, line);
                    self.push(formula, r, line);
                }
            }
        }
    }

    /// Checks the requirement's parameters against the bounds the set states before them.
    fn params(&mut self, r: Requirement<'a>) {
        let Some(field) = r.block.field("params") else {
            return;
        };
        let line = field.line;
        for item in field.value.as_array().into_iter().flatten() {
            let name = text(item, "name");
            let bound = text(item, "bound").filter(|bound| BOUNDS.contains(bound));
            let value = item.get("value").and_then(number);
            let (Some(name), Some(bound), Some(value)) = (name, bound, value) else {
                let message = "a parameter holds `name`, `bound` (`min`, `max` or \
                               `exact`) and a number as `value`";
                self.error(PARAMETER, r, line, message.to_string());
                continue;
            };
            let interval = self.bounds.entry(name).or_default();
            let found = problem(interval, name, bound, value);
            let file = r.file;
            tighten(interval, bound, Bound { value, file, line });
            if let Some(message) = found {
                self.error(PARAMETER, r, line, message);
            }
        }
    }

    /// Whether `id` names a requirement of the set, adding a finding where it doesn't.
    fn resolve(&mut self, r: Requirement<'a>, id: &str, noun: &str, line: u32, retired: bool) {
        if self.held.contains_key(id) {
            return;
        }
        if let Some(&(file, at)) = self.retired.get(id) {
            if !retired {
                let message = format!("{noun} `{id}` is retired at {file}:{at}");
                self.error(RELATION_TARGET, r, line, message);
            }
            return;
        }
        let prefix = self.config.prefix.as_str();
        match parse_identifier(prefix, id) {
            None => {
                let message = format!("{noun} `{id}` isn't an identifier {prefix}-<rfc>-<n>");
                self.error(RELATION_TARGET, r, line, message);
            }
            Some((rfc, _)) if self.planned.contains(&rfc) => {
                let message = format!(
                    "{noun} `{id}` is in RFC {rfc}, which the index plans and no file holds yet"
                );
                self.warning(RELATION_UNRESOLVED, r, line, message);
            }
            Some((rfc, _)) => {
                let message = format!(
                    "no requirement holds {noun} `{id}`, and the index doesn't plan RFC {rfc}"
                );
                self.error(RELATION_TARGET, r, line, message);
            }
        }
    }

    /// Adds a finding where `id` is a requirement of another system than the requirement's own.
    fn boundary(&mut self, r: Requirement<'a>, id: &str, noun: &str, line: u32) {
        let Some(own) = self.system_of(r.rfc) else {
            return;
        };
        let Some(other) = self.rfc_of(id).and_then(|number| self.system_of(number)) else {
            return;
        };
        if own != other {
            let message = format!(
                "{noun} `{id}` is a requirement of system `{other}`, and this one is of system \
                 `{own}`: two systems meet only through the requirements of a contract RFC"
            );
            self.error(SYSTEM_BOUNDARY, r, line, message);
        }
    }

    /// Adds a finding where a formula that a contract RFC, or an RFC of no system, holds joins
    /// the requirements of two systems or more. A formula of a system's own RFC is checked atom
    /// by atom, by [`Self::boundary`].
    fn joined(&mut self, r: Requirement<'a>, names: &BTreeSet<String>, line: u32) {
        if self.system_of(r.rfc).is_some() {
            return;
        }
        let systems: BTreeSet<&str> = names
            .iter()
            .filter_map(|name| self.rfc_of(name))
            .filter_map(|number| self.system_of(number))
            .collect();
        if systems.len() > 1 {
            let list: Vec<String> = systems.iter().map(|system| format!("`{system}`")).collect();
            let message = format!(
                "the formula joins requirements of systems {}: two systems meet only through \
                 the requirements of a contract RFC",
                list.join(", ")
            );
            self.error(SYSTEM_BOUNDARY, r, line, message);
        }
    }

    /// The system an RFC is of, where it isn't a contract RFC.
    fn system_of(&self, number: u32) -> Option<&'a str> {
        let config = self.config;
        if config
            .contracts
            .as_ref()
            .is_some_and(|range| range.contains(number))
        {
            return None;
        }
        let system = config
            .systems
            .iter()
            .find(|system| system.rfcs.contains(number));
        system.map(|system| system.name.as_str())
    }

    /// The RFC of an identifier: the RFC that holds it, or else the one its form names.
    fn rfc_of(&self, id: &str) -> Option<u32> {
        if let Some(&index) = self.held.get(id) {
            return Some(self.requirements[index].rfc);
        }
        parse_identifier(&self.config.prefix, id).map(|(rfc, _)| rfc)
    }

    /// Adds a finding for each cycle of `implies` and `refines` relations.
    fn cycles(&mut self) {
        let edges = std::mem::take(&mut self.cycle);
        for component in components(&edges) {
            let first = component[0];
            let cyclic = component.len() > 1
                || edges
                    .get(first)
                    .is_some_and(|targets| targets.contains(first));
            let Some(&index) = self.held.get(first) else {
                continue;
            };
            if !cyclic {
                continue;
            }
            let r = self.requirements[index];
            let members: Vec<String> = component.iter().map(|id| format!("`{id}`")).collect();
            let members = members.join(", ");
            let message =
                format!("`implies` and `refines` relations form a cycle through {members}");
            self.error(RELATION_CYCLE, r, line_of(r.block, "relations"), message);
        }
    }

    /// Adds a finding for each requirement required at the first level whose relations lead to
    /// a requirement forbidden at it. The walk starts from each forbidden requirement and goes
    /// back along the relations, so the check reads the relations once for each requirement
    /// forbidden at the first level, and not once for each one required at it.
    fn polarity(&mut self) {
        let config = self.config;
        let Some(top) = config.levels.first() else {
            return;
        };
        let top = top.as_str();
        let at = |block: &Block, polarity: &str| {
            block.text("level") == Some(top) && block.text("polarity") == Some(polarity)
        };
        let mut reverse: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (&from, targets) in &self.leads {
            for &to in targets {
                reverse.entry(to).or_default().insert(from);
            }
        }
        let mut found = Vec::new();
        for (index, forbidden) in self.requirements.iter().enumerate() {
            if self.held.get(forbidden.id) != Some(&index) || !at(forbidden.block, "forbidden") {
                continue;
            }
            let id = forbidden.id;
            for node in leading_to(&reverse, id) {
                let Some(&other) = self.held.get(node) else {
                    continue;
                };
                let r = self.requirements[other];
                if at(r.block, "required") {
                    let message = format!(
                        "the requirement is required at {top}, and its relations lead to `{id}`, \
                         which is forbidden at {top}"
                    );
                    found.push((r, message));
                }
            }
        }
        for (r, message) in found {
            self.error(POLARITY_CONFLICT, r, r.line, message);
        }
    }

    /// Runs checks 1 and 2 over the formula set, or reads their result from `cache`.
    fn solve<B: Backend + Default, C: Cache>(&mut self, cache: &C) {
        let found = cached_consistency::<B, _>(&self.set, cache);
        let formulas = &self.set.formulas;
        let place = |entry: &Entry| {
            let (file, line) = (&entry.origin.file, entry.origin.line);
            format!("`{}` at {file}:{line}", entry.formula)
        };
        for &index in found.conflict.iter().flatten() {
            let entry = &formulas[index];
            let others: Vec<String> = found
                .conflict
                .iter()
                .flatten()
                .filter(|&&other| other != index)
                .map(|&other| place(&formulas[other]))
                .collect();
            let message = if others.is_empty() {
                format!("`{}` can't hold by itself", entry.formula)
            } else {
                let others = others.join(", ");
                format!("`{}` can't hold together with {others}", entry.formula)
            };
            self.found
                .push(at_origin(UNSATISFIABLE, &entry.origin, message));
        }
        for &index in &found.vacuous {
            let condition = &self.set.conditions[index];
            let atom = &condition.atom;
            let message = format!(
                "`{atom}` can't be true while the rest of the set holds, so this requirement \
                 holds only vacuously"
            );
            self.found
                .push(at_origin(CONDITION_VACUOUS, &condition.origin, message));
        }
    }

    /// Finds each held requirement that a held requirement's `supersedes` relation retires,
    /// before any requirement is built, so the checks read only the requirements in force.
    /// Each requirement of a cycle of `supersedes` relations, a requirement that supersedes
    /// itself among them, gets a finding, since the cycle retires each of them and nothing
    /// replaces them.
    fn supersede(&mut self) {
        let mut edges: BTreeMap<&'a str, BTreeSet<&'a str>> = BTreeMap::new();
        for (index, &r) in self.requirements.iter().enumerate() {
            if self.held.get(r.id) != Some(&index) {
                continue;
            }
            let Some(field) = r.block.field("relations") else {
                continue;
            };
            for item in field.value.as_array().into_iter().flatten() {
                let kind = text(item, "kind");
                let (Some("supersedes"), Some(target)) = (kind, text(item, "target")) else {
                    continue;
                };
                if self.held.contains_key(target) {
                    self.superseded.insert(target);
                    edges.entry(r.id).or_default().insert(target);
                }
            }
        }
        for component in components(&edges) {
            let first = component[0];
            let itself = edges
                .get(first)
                .is_some_and(|targets| targets.contains(first));
            let message = if component.len() > 1 {
                let members: Vec<String> = component.iter().map(|id| format!("`{id}`")).collect();
                let members = members.join(", ");
                format!(
                    "`supersedes` relations form a cycle through {members}, so each of them is \
                     retired and nothing replaces them"
                )
            } else if itself {
                format!("`{first}` supersedes itself, so it is retired and nothing replaces it")
            } else {
                continue;
            };
            for id in component {
                let Some(&index) = self.held.get(id) else {
                    continue;
                };
                let r = self.requirements[index];
                self.error(RELATION, r, line_of(r.block, "relations"), message.clone());
            }
        }
    }

    /// Finds the requirements that a composition holds as its parts, or as its exception,
    /// after `supersede`, so a part of a retired composition holds by itself.
    fn compositions(&mut self) {
        for &r in &self.requirements {
            if self.superseded.contains(r.id) {
                continue;
            }
            let parts = strings(r.block.field("parts"));
            match r.block.text("compose") {
                Some("all_of" | "any_of" | "exactly_one") => self.parts.extend(parts),
                Some("unless") => {
                    if let [exception] = parts[..] {
                        self.parts.insert(exception);
                    }
                }
                _ => {}
            }
        }
    }

    fn push(&mut self, formula: Formula, r: Requirement<'a>, line: u32) {
        let origin = origin(r, line);
        self.set.formulas.push(Entry { formula, origin });
    }

    fn error(&mut self, rule: &str, r: Requirement<'a>, line: u32, message: String) {
        let finding = Finding::error(rule, r.file, line, Some(r.id), message);
        self.found.push(finding);
    }

    fn warning(&mut self, rule: &str, r: Requirement<'a>, line: u32, message: String) {
        let finding = Finding::warning(rule, r.file, line, Some(r.id), message);
        self.found.push(finding);
    }
}

/// A finding that fails the check, at the place a formula or a condition comes from.
fn at_origin(rule: &str, origin: &Origin, message: String) -> Finding {
    let requirement = Some(origin.requirement.as_str());
    Finding::error(rule, &origin.file, origin.line, requirement, message)
}

/// What is wrong with a parameter's bound against the bounds the set states before it.
fn problem(interval: &Interval<'_>, name: &str, bound: &str, value: f64) -> Option<String> {
    let at = |b: Bound<'_>| format!("{} at {}:{}", b.value, b.file, b.line);
    let below = |b: Option<Bound<'_>>| b.filter(|b| value < b.value).map(at);
    let above = |b: Option<Bound<'_>>| b.filter(|b| value > b.value).map(at);
    match bound {
        "exact" => {
            if let Some(exact) = interval.exact {
                let exact = at(exact);
                Some(format!(
                    "`{name}` has a second exact value, {value}, beside {exact}"
                ))
            } else if let Some(min) = below(interval.min) {
                Some(format!(
                    "`{name}`'s exact value {value} is below its minimum {min}"
                ))
            } else {
                let max = above(interval.max)?;
                Some(format!(
                    "`{name}`'s exact value {value} is above its maximum {max}"
                ))
            }
        }
        "min" => {
            if let Some(max) = above(interval.max) {
                Some(format!(
                    "`{name}`'s minimum {value} is above its maximum {max}"
                ))
            } else {
                let exact = above(interval.exact)?;
                Some(format!(
                    "`{name}`'s minimum {value} is above its exact value {exact}"
                ))
            }
        }
        _ => {
            if let Some(min) = below(interval.min) {
                Some(format!(
                    "`{name}`'s maximum {value} is below its minimum {min}"
                ))
            } else {
                let exact = below(interval.exact)?;
                Some(format!(
                    "`{name}`'s maximum {value} is below its exact value {exact}"
                ))
            }
        }
    }
}

/// Keeps the tightest minimum and maximum, and the first exact value.
fn tighten<'a>(interval: &mut Interval<'a>, bound: &str, b: Bound<'a>) {
    match bound {
        "exact" => {
            if interval.exact.is_none() {
                interval.exact = Some(b);
            }
        }
        "min" => {
            if interval.min.is_none_or(|min| b.value > min.value) {
                interval.min = Some(b);
            }
        }
        _ => {
            if interval.max.is_none_or(|max| b.value < max.value) {
                interval.max = Some(b);
            }
        }
    }
}

/// The strongly connected components of the graph, each sorted, by Kosaraju's two passes.
fn components<'a>(edges: &BTreeMap<&'a str, BTreeSet<&'a str>>) -> Vec<Vec<&'a str>> {
    let empty = BTreeSet::new();
    let mut nodes: BTreeSet<&'a str> = edges.keys().copied().collect();
    let mut reverse: BTreeMap<&'a str, BTreeSet<&'a str>> = BTreeMap::new();
    for (&from, targets) in edges {
        for &to in targets {
            nodes.insert(to);
            reverse.entry(to).or_default().insert(from);
        }
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for &start in &nodes {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![(start, edges.get(start).unwrap_or(&empty).iter())];
        while let Some((node, targets)) = stack.last_mut() {
            match targets.next() {
                Some(&next) => {
                    if seen.insert(next) {
                        let targets = edges.get(next).unwrap_or(&empty).iter();
                        stack.push((next, targets));
                    }
                }
                None => {
                    let node = *node;
                    order.push(node);
                    stack.pop();
                }
            }
        }
    }
    let mut assigned = BTreeSet::new();
    let mut found = Vec::new();
    for &start in order.iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut component = vec![start];
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &previous in reverse.get(node).unwrap_or(&empty) {
                if assigned.insert(previous) {
                    component.push(previous);
                    stack.push(previous);
                }
            }
        }
        component.sort_unstable();
        found.push(component);
    }
    found
}

/// The nodes whose edges lead to `start`, through one edge or more, read from the graph's edges
/// reversed: `reverse` holds each target with the nodes whose edges lead to it.
fn leading_to<'a>(
    reverse: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    start: &'a str,
) -> BTreeSet<&'a str> {
    let mut reached = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for &previous in reverse.get(node).into_iter().flatten() {
            if reached.insert(previous) {
                stack.push(previous);
            }
        }
    }
    reached
}

fn origin(r: Requirement<'_>, line: u32) -> Origin {
    Origin {
        requirement: r.id.to_owned(),
        file: r.file.to_owned(),
        line,
    }
}

/// The line of the block's field of that key, or of the block where it doesn't hold one.
fn line_of(block: &Block, key: &str) -> u32 {
    block.field(key).map_or(block.line, |field| field.line)
}

/// The strings of an array field.
fn strings(field: Option<&Field>) -> Vec<&str> {
    let items = field.and_then(|field| field.value.as_array());
    items
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The string of a table's key.
fn text<'v>(item: &'v Value, key: &str) -> Option<&'v str> {
    item.get(key)?.as_str()
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(integer) => Some(*integer as f64),
        Value::Float(float) => Some(*float),
        _ => None,
    }
}

fn atoms_of(ids: &[&str]) -> Vec<Formula> {
    ids.iter().map(|&id| Formula::atom(id)).collect()
}

/// Each atom is true; one atom is itself.
fn all(mut atoms: Vec<Formula>) -> Formula {
    if atoms.len() == 1 {
        return atoms.remove(0);
    }
    Formula::And(atoms)
}

/// One atom is true; one atom is itself.
fn any(mut atoms: Vec<Formula>) -> Formula {
    if atoms.len() == 1 {
        return atoms.remove(0);
    }
    Formula::at_least_one(atoms)
}

/// The names of the formula's atoms. It keeps the parts still to read on a stack on the heap, so
/// a formula may nest as deep as memory allows.
fn names_of(formula: &Formula, names: &mut BTreeSet<String>) {
    let mut pending = vec![formula];
    while let Some(formula) = pending.pop() {
        match formula {
            Formula::Atom(name) => {
                names.insert(name.clone());
            }
            Formula::Not(inner) => pending.push(inner),
            Formula::And(parts) | Formula::Or(parts) => pending.extend(parts),
            Formula::ExactlyOne(parts) => pending.extend(parts),
            Formula::Implies(a, b) | Formula::Iff(a, b) => {
                pending.push(a);
                pending.push(b);
            }
        }
    }
}

/// What checks 1 and 2 found in a formula set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Consistency {
    /// Check 1: where the formulas contradict each other, the indexes of a minimal subset of
    /// them that does, in the order of the set's formulas: leaving out any one of them leaves
    /// the rest satisfiable. `None` where every formula holds together.
    pub conflict: Option<Vec<usize>>,
    /// Check 2: the indexes of the conditions that can't hold while every formula does, in the
    /// order of the set's conditions. Empty where check 1 found a conflict, since then no
    /// condition can hold.
    pub vacuous: Vec<usize>,
}

/// Runs checks 1 and 2 over `set` in a new backend of type `B`.
pub fn consistency<B: Backend + Default>(set: &FormulaSet) -> Consistency {
    let mut encoding = Encoding::new(B::default(), set);
    let selectors = encoding.selectors().to_vec();
    let conflict = match encoding.solve(&selectors) {
        Answer::Sat => None,
        Answer::Unsat { core } => Some(minimal_conflict(&mut encoding, &selectors, &core)),
    };
    let mut vacuous = Vec::new();
    if conflict.is_none() {
        let mut assumptions = selectors;
        // One solve for each distinct atom: the conditions that share an atom share its answer.
        let mut holds: BTreeMap<Lit, bool> = BTreeMap::new();
        for (index, condition) in set.conditions.iter().enumerate() {
            let atom = encoding
                .atom_lit(&condition.atom)
                .expect("the encoding holds each condition's atom");
            let can_hold = *holds.entry(atom).or_insert_with(|| {
                assumptions.push(atom);
                let answer = encoding.solve(&assumptions);
                assumptions.pop();
                answer == Answer::Sat
            });
            if !can_hold {
                vacuous.push(index);
            }
        }
    }
    Consistency { conflict, vacuous }
}

/// The result of checks 1 and 2 over `set`, as [`consistency`] returns it, read from `cache` where
/// it keeps one for the set's formulas and conditions. Only a result without a conflict is kept:
/// its conditions that can't hold are the same under every backend, while a conflict's minimal
/// subset starts from the backend's core.
pub(crate) fn cached_consistency<B: Backend + Default, C: Cache>(
    set: &FormulaSet,
    cache: &C,
) -> Consistency {
    if !cache.keeps() {
        return consistency::<B>(set);
    }
    let key = artifact::consistency_key(set);
    let conditions = set.conditions.len();
    let kept = cache.get(&key);
    let vacuous = kept.and_then(|bytes| artifact::decode_consistency(&bytes, conditions));
    if let Some(vacuous) = vacuous {
        return Consistency {
            conflict: None,
            vacuous,
        };
    }
    let found = consistency::<B>(set);
    if found.conflict.is_none() {
        cache.put(key, artifact::encode_consistency(&found.vacuous));
    }
    found
}

/// A minimal subset of the formulas that contradict each other, by their indexes. It starts
/// from the formulas of the backend's core, and leaves out one formula at a time, keeping each
/// removal that leaves the rest contradictory. Each later core narrows the candidates again.
fn minimal_conflict<B: Backend>(
    encoding: &mut Encoding<B>,
    selectors: &[Lit],
    core: &[Lit],
) -> Vec<usize> {
    let mut kept = narrow(selectors, 0..selectors.len(), core);
    let mut next = 0;
    while next < kept.len() {
        let mut trial = kept.clone();
        trial.remove(next);
        let assumptions: Vec<Lit> = trial.iter().map(|&index| selectors[index]).collect();
        match encoding.solve(&assumptions) {
            Answer::Sat => next += 1,
            Answer::Unsat { core } => kept = narrow(selectors, trial, &core),
        }
    }
    kept
}

/// The indexes of `candidates` whose selector `core` holds, in their order.
fn narrow(
    selectors: &[Lit],
    candidates: impl IntoIterator<Item = usize>,
    core: &[Lit],
) -> Vec<usize> {
    let core: BTreeSet<Lit> = core.iter().copied().collect();
    candidates
        .into_iter()
        .filter(|&index| core.contains(&selectors[index]))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    use super::{cached_consistency, consistency, names_of, Consistency};
    use crate::logic::{parse, Condition, Entry, FormulaSet, Origin};
    use crate::solver::{Answer, Backend, Lit};
    use crate::{ByteVec, Cache, Key};

    fn origin(requirement: &str) -> Origin {
        Origin {
            requirement: requirement.to_owned(),
            file: "rfcs/EX-1.md".to_owned(),
            line: 1,
        }
    }

    fn set(formulas: &[&str], conditions: &[&str]) -> FormulaSet {
        let formulas = formulas
            .iter()
            .enumerate()
            .map(|(index, text)| Entry {
                formula: parse(text).unwrap(),
                origin: origin(&format!("EX-{}", index + 1)),
            })
            .collect();
        let conditions = conditions
            .iter()
            .map(|atom| Condition {
                atom: (*atom).to_owned(),
                origin: origin("EX-0"),
            })
            .collect();
        FormulaSet {
            formulas,
            conditions,
        }
    }

    /// A backend whose every core is the whole of its assumptions, which no minimal subset is.
    #[derive(Default)]
    struct Wide<B>(B);

    impl<B: Backend> Backend for Wide<B> {
        fn new_var(&mut self) -> Lit {
            self.0.new_var()
        }

        fn add_clause(&mut self, clause: &[Lit]) {
            self.0.add_clause(clause);
        }

        fn solve(&mut self, assumptions: &[Lit]) -> Answer {
            match self.0.solve(assumptions) {
                Answer::Sat => Answer::Sat,
                Answer::Unsat { .. } => Answer::Unsat {
                    core: assumptions.to_vec(),
                },
            }
        }
    }

    thread_local! {
        static SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// A backend that counts its solves on the thread that runs it.
    #[derive(Default)]
    struct Counted<B>(B);

    impl<B: Backend> Backend for Counted<B> {
        fn new_var(&mut self) -> Lit {
            self.0.new_var()
        }

        fn add_clause(&mut self, clause: &[Lit]) {
            self.0.add_clause(clause);
        }

        fn solve(&mut self, assumptions: &[Lit]) -> Answer {
            SOLVES.with(|count| count.set(count.get() + 1));
            self.0.solve(assumptions)
        }
    }

    fn empty_set<B: Backend + Default>() {
        assert_eq!(
            consistency::<B>(&FormulaSet::default()),
            Consistency::default()
        );
    }

    /// The atoms of a formula of 100,000 nested `!`, and of one of 100,000 nested parentheses,
    /// found without overflowing the stack.
    #[test]
    fn names_of_a_deep_formula() {
        const DEPTH: usize = 100_000;
        let negation = format!("{}a", "!".repeat(DEPTH));
        let parentheses = format!("{}a{}", "(b & ".repeat(DEPTH), ")".repeat(DEPTH));
        for (text, atoms) in [(negation, vec!["a"]), (parentheses, vec!["a", "b"])] {
            let mut names = BTreeSet::new();
            names_of(&parse(&text).unwrap(), &mut names);
            assert_eq!(names.iter().map(String::as_str).collect::<Vec<_>>(), atoms);
        }
    }

    fn consistent_set<B: Backend + Default>() {
        let found = consistency::<B>(&set(&["a -> b", "b -> c", "!d"], &["a", "c"]));
        assert_eq!(found, Consistency::default());
    }

    fn conflict_is_minimal<B: Backend + Default>() {
        let formulas = ["a", "b", "a -> c", "c -> !a", "d | b"];
        let found = consistency::<B>(&set(&formulas, &["b"]));
        assert_eq!(found.conflict, Some(vec![0, 2, 3]));
        assert!(found.vacuous.is_empty());
    }

    fn conflict_is_one_of_two<B: Backend + Default>() {
        let found = consistency::<B>(&set(&["a", "!a", "b", "!b"], &[]));
        let conflict = found.conflict.unwrap();
        assert!(conflict == [0, 1] || conflict == [2, 3], "{conflict:?}");
    }

    fn minimal_from_a_wide_core<B: Backend + Default>() {
        let formulas = ["a", "b", "c <-> b", "!a", "d", "e -> a"];
        let found = consistency::<Wide<B>>(&set(&formulas, &[]));
        assert_eq!(found.conflict, Some(vec![0, 3]));
    }

    fn minimal_from_a_wide_core_of_three<B: Backend + Default>() {
        let formulas = ["x", "y", "x -> p", "y -> q", "!(p & q)", "z"];
        let found = consistency::<Wide<B>>(&set(&formulas, &[]));
        assert_eq!(found.conflict, Some(vec![0, 1, 2, 3, 4]));
    }

    fn vacuous_condition<B: Backend + Default>() {
        let formulas = ["e -> f", "e -> !f", "g -> f"];
        let found = consistency::<B>(&set(&formulas, &["e", "g", "h"]));
        assert_eq!(found.conflict, None);
        assert_eq!(found.vacuous, vec![0]);
    }

    fn condition_vacuous_through_exactly_one<B: Backend + Default>() {
        let formulas = ["r <-> (p | q)", "!(p & q)", "!p", "!q"];
        let found = consistency::<B>(&set(&formulas, &["r", "p"]));
        assert_eq!(found.conflict, None);
        assert_eq!(found.vacuous, vec![0, 1]);
    }

    fn one_solve_per_distinct_atom<B: Backend + Default>() {
        SOLVES.with(|count| count.set(0));
        let formulas = ["e -> f", "e -> !f"];
        let found = consistency::<Counted<B>>(&set(&formulas, &["e", "g", "e", "g"]));
        assert_eq!(found.conflict, None);
        assert_eq!(found.vacuous, vec![0, 2]);
        // One solve for check 1, and one for each of the two distinct atoms.
        assert_eq!(SOLVES.with(std::cell::Cell::get), 3);
    }

    /// A cache that keeps each artifact in memory.
    #[derive(Default)]
    struct Memory(RefCell<BTreeMap<Key, ByteVec>>);

    impl Cache for Memory {
        fn get(&self, key: &Key) -> Option<ByteVec> {
            self.0.borrow().get(key).cloned()
        }

        fn put(&self, key: Key, artifact: ByteVec) {
            self.0.borrow_mut().insert(key, artifact);
        }
    }

    fn a_consistent_result_is_read_from_the_cache<B: Backend + Default>() {
        let cache = Memory::default();
        let consistent = set(&["e -> f", "e -> !f"], &["e", "f"]);
        let found = cached_consistency::<B, _>(&consistent, &cache);
        assert_eq!(found, consistency::<B>(&consistent));
        assert_eq!(found.vacuous, vec![0]);
        assert_eq!(cache.0.borrow().len(), 1);
        SOLVES.with(|count| count.set(0));
        let read = cached_consistency::<Counted<B>, _>(&consistent, &cache);
        assert_eq!(read, found);
        assert_eq!(SOLVES.with(std::cell::Cell::get), 0);
    }

    fn a_conflict_isnt_kept<B: Backend + Default>() {
        let cache = Memory::default();
        let conflicting = set(&["a", "!a"], &["a"]);
        let found = cached_consistency::<B, _>(&conflicting, &cache);
        assert_eq!(found, consistency::<B>(&conflicting));
        assert!(found.conflict.is_some());
        assert!(cache.0.borrow().is_empty());
    }

    crate::solver::backend_tests!(
        empty_set,
        a_consistent_result_is_read_from_the_cache,
        a_conflict_isnt_kept,
        one_solve_per_distinct_atom,
        consistent_set,
        conflict_is_minimal,
        conflict_is_one_of_two,
        minimal_from_a_wide_core,
        minimal_from_a_wide_core_of_three,
        vacuous_condition,
        condition_vacuous_through_exactly_one,
    );
}
