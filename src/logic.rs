//! Formulas over requirement identifiers, the grammar of a requirement's `logic` field, and the
//! clauses a solver reads.
//!
//! Each requirement identifier is a propositional atom. The checker turns each formula of the set
//! into clauses by the Tseitin transform, and assigns each formula a selector literal, so that the
//! formula holds only where its selector is assumed true.
//!
//! The parser and each walk of a formula, its clone, drop, comparison, debug output, display and
//! encoding, keep their own stack on the heap, so a formula may nest as deep as memory allows.
//!
//! serde derives the serialization, in the shape a host reads as JSON, and it still recurses once
//! for each level of nesting. Each method of serde's `Serializer` that writes a nested value, such
//! as `serialize_newtype_variant` for the part of `Not` and `SerializeSeq::serialize_element` for
//! each part of `And`, is handed that value as a `&T` with `T: Serialize` and serializes it before
//! it returns, so a `Serialize` written by hand would make the same nested calls.
//! [`crate::json`] writes the same JSON with a stack on the heap, for a host whose formulas nest
//! deep.

use std::collections::BTreeMap;
use std::fmt;
use std::mem;
use std::vec::Drain;

use serde::Serialize;

use crate::solver::{Answer, Backend, Lit};

/// A formula over requirement identifiers.
///
/// Its comparison and debug output are written by hand, below, each with a stack on the heap;
/// serde derives its serialization, as the module's documentation says.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Formula {
    /// A requirement identifier, or a condition's atom.
    Atom(String),
    Not(Box<Formula>),
    /// True where each part is true; an empty conjunction is true.
    And(Vec<Formula>),
    /// True where a part is true; an empty disjunction is false.
    Or(Vec<Formula>),
    /// True where exactly one part is true; with no part, false.
    #[serde(rename = "exactly_one")]
    ExactlyOne(Vec<Formula>),
    Implies(Box<Formula>, Box<Formula>),
    Iff(Box<Formula>, Box<Formula>),
}

impl Formula {
    pub fn atom(name: impl Into<String>) -> Self {
        Formula::Atom(name.into())
    }

    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        Formula::Not(Box::new(self))
    }

    pub fn implies(self, other: Formula) -> Self {
        Formula::Implies(Box::new(self), Box::new(other))
    }

    pub fn iff(self, other: Formula) -> Self {
        Formula::Iff(Box::new(self), Box::new(other))
    }

    /// At least one of the parts is true.
    pub fn at_least_one(parts: Vec<Formula>) -> Self {
        Formula::Or(parts)
    }

    /// Exactly one of the parts is true: at least one, and no two together. The formula holds
    /// each part once, and its encoding adds clauses linear in the count of parts.
    pub fn exactly_one(parts: Vec<Formula>) -> Self {
        Formula::ExactlyOne(parts)
    }

    /// Folds the formula from its atoms up: `visit` is called on each formula after its parts,
    /// with the values it returned for them, in order. The formulas still to visit wait on a
    /// stack on the heap, so a formula may nest as deep as memory allows.
    fn fold<'f, T>(&'f self, mut visit: impl FnMut(&'f Formula, Drain<'_, T>) -> T) -> T {
        enum Step<'s> {
            Enter(&'s Formula),
            Fold(&'s Formula),
        }

        let mut steps = vec![Step::Enter(self)];
        let mut values: Vec<T> = Vec::new();
        while let Some(step) = steps.pop() {
            match step {
                Step::Enter(formula) => {
                    steps.push(Step::Fold(formula));
                    match formula {
                        Formula::Atom(_) => {}
                        Formula::Not(inner) => steps.push(Step::Enter(inner)),
                        Formula::And(parts) | Formula::Or(parts) | Formula::ExactlyOne(parts) => {
                            steps.extend(parts.iter().rev().map(Step::Enter));
                        }
                        Formula::Implies(a, b) | Formula::Iff(a, b) => {
                            steps.push(Step::Enter(b));
                            steps.push(Step::Enter(a));
                        }
                    }
                }
                Step::Fold(formula) => {
                    let count = match formula {
                        Formula::Atom(_) => 0,
                        Formula::Not(_) => 1,
                        Formula::And(parts) | Formula::Or(parts) => parts.len(),
                        Formula::ExactlyOne(parts) => parts.len(),
                        Formula::Implies(..) | Formula::Iff(..) => 2,
                    };
                    let value = visit(formula, values.drain(values.len() - count..));
                    values.push(value);
                }
            }
        }
        values.pop().expect("the fold leaves the formula's value")
    }

    /// Moves each part of the formula that isn't an atom onto `pending`, and leaves an empty
    /// atom in its place, so that dropping the formula doesn't recurse.
    fn take_parts(&mut self, pending: &mut Vec<Formula>) {
        fn take(part: &mut Formula, pending: &mut Vec<Formula>) {
            if !matches!(part, Formula::Atom(_)) {
                pending.push(mem::replace(part, Formula::Atom(String::new())));
            }
        }

        match self {
            Formula::Atom(_) => {}
            Formula::Not(inner) => take(inner, pending),
            Formula::And(parts) | Formula::Or(parts) | Formula::ExactlyOne(parts) => {
                for part in parts {
                    take(part, pending);
                }
            }
            Formula::Implies(a, b) | Formula::Iff(a, b) => {
                take(a, pending);
                take(b, pending);
            }
        }
    }
}

impl Clone for Formula {
    fn clone(&self) -> Self {
        self.fold(|formula, mut parts: Drain<'_, Formula>| match formula {
            Formula::Atom(name) => Formula::Atom(name.clone()),
            Formula::Not(_) => next_part(&mut parts).not(),
            Formula::And(_) => Formula::And(parts.collect()),
            Formula::Or(_) => Formula::Or(parts.collect()),
            Formula::ExactlyOne(_) => Formula::ExactlyOne(parts.collect()),
            Formula::Implies(..) => next_part(&mut parts).implies(next_part(&mut parts)),
            Formula::Iff(..) => next_part(&mut parts).iff(next_part(&mut parts)),
        })
    }
}

impl Drop for Formula {
    fn drop(&mut self) {
        // Each formula moves its parts that aren't atoms here before it drops, so no drop
        // recurses past the parts of one formula.
        let mut pending = Vec::new();
        self.take_parts(&mut pending);
        while let Some(mut formula) = pending.pop() {
            formula.take_parts(&mut pending);
        }
    }
}

/// The next value a fold passes for a formula's parts.
fn next_part<T>(parts: &mut impl Iterator<Item = T>) -> T {
    parts.next().expect("a fold passes a value for each part")
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        enum Step<'a> {
            Part(&'a Formula),
            Text(&'a str),
        }

        /// Pushes the steps that write `parts`, `op` between each two, and then `)`.
        fn join<'a>(steps: &mut Vec<Step<'a>>, parts: &'a [Formula], op: &'a str) {
            steps.push(Step::Text(")"));
            for (i, part) in parts.iter().enumerate().rev() {
                steps.push(Step::Part(part));
                if i > 0 {
                    steps.push(Step::Text(op));
                }
            }
        }

        /// Pushes the steps that write `a`, `op` and `b`, and then `)`.
        fn pair<'a>(steps: &mut Vec<Step<'a>>, a: &'a Formula, b: &'a Formula, op: &'a str) {
            steps.extend([
                Step::Text(")"),
                Step::Part(b),
                Step::Text(op),
                Step::Part(a),
            ]);
        }

        // The steps still to write wait on a stack on the heap, the next one last, so a
        // formula may nest as deep as memory allows.
        let mut steps = vec![Step::Part(self)];
        while let Some(step) = steps.pop() {
            match step {
                Step::Text(text) => f.write_str(text)?,
                Step::Part(Formula::Atom(name)) => f.write_str(name)?,
                Step::Part(Formula::Not(inner)) => {
                    f.write_str("!")?;
                    steps.push(Step::Part(inner));
                }
                Step::Part(Formula::And(parts)) => {
                    f.write_str("(")?;
                    join(&mut steps, parts, " & ");
                }
                Step::Part(Formula::Or(parts)) => {
                    f.write_str("(")?;
                    join(&mut steps, parts, " | ");
                }
                Step::Part(Formula::ExactlyOne(parts)) => {
                    f.write_str("exactly_one(")?;
                    join(&mut steps, parts, ", ");
                }
                Step::Part(Formula::Implies(a, b)) => {
                    f.write_str("(")?;
                    pair(&mut steps, a, b, " -> ");
                }
                Step::Part(Formula::Iff(a, b)) => {
                    f.write_str("(")?;
                    pair(&mut steps, a, b, " <-> ");
                }
            }
        }
        Ok(())
    }
}

impl PartialEq for Formula {
    /// Compares as `#[derive(PartialEq)]` does: the same variant, the same name of an atom, and
    /// parts that compare equal, in order. The pairs still to compare wait on a stack on the
    /// heap, so a formula may nest as deep as memory allows.
    fn eq(&self, other: &Formula) -> bool {
        let mut pairs = vec![(self, other)];
        while let Some(pair) = pairs.pop() {
            match pair {
                (Formula::Atom(a), Formula::Atom(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (Formula::Not(a), Formula::Not(b)) => pairs.push((&**a, &**b)),
                (Formula::And(a), Formula::And(b))
                | (Formula::Or(a), Formula::Or(b))
                | (Formula::ExactlyOne(a), Formula::ExactlyOne(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    pairs.extend(a.iter().zip(b).rev());
                }
                (Formula::Implies(a, c), Formula::Implies(b, d))
                | (Formula::Iff(a, c), Formula::Iff(b, d)) => {
                    pairs.push((&**c, &**d));
                    pairs.push((&**a, &**b));
                }
                _ => return false,
            }
        }
        true
    }
}

impl Eq for Formula {}

impl fmt::Debug for Formula {
    /// Writes what `#[derive(Debug)]` writes, in `{:#?}` as in `{:?}`. The steps still to write
    /// wait on a stack on the heap, the next one last, so a formula may nest as deep as memory
    /// allows.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// A step of the output: a formula or the list of a composition's parts, each with the
        /// depth of its indent in `{:#?}`, an atom's name, a text, or the indent of a depth.
        #[derive(Clone, Copy)]
        enum Step<'a> {
            Formula(&'a Formula, usize),
            List(&'a [Formula], usize),
            Name(&'a str),
            Text(&'static str),
            Indent(usize),
        }

        /// Pushes the steps that write `fields` in parentheses, as `debug_tuple` writes them,
        /// after the name of a variant at `depth`.
        fn tuple<'a>(steps: &mut Vec<Step<'a>>, pretty: bool, depth: usize, fields: &[Step<'a>]) {
            steps.push(Step::Text(")"));
            if pretty {
                steps.push(Step::Indent(depth));
                for &field in fields.iter().rev() {
                    steps.extend([Step::Text(",\n"), field, Step::Indent(depth + 1)]);
                }
                steps.push(Step::Text("(\n"));
            } else {
                for (i, &field) in fields.iter().enumerate().rev() {
                    steps.push(field);
                    if i > 0 {
                        steps.push(Step::Text(", "));
                    }
                }
                steps.push(Step::Text("("));
            }
        }

        let pretty = f.alternate();
        let mut steps = vec![Step::Formula(self, 0)];
        while let Some(step) = steps.pop() {
            match step {
                Step::Text(text) => f.write_str(text)?,
                Step::Indent(depth) => {
                    for _ in 0..depth {
                        f.write_str("    ")?;
                    }
                }
                Step::Name(name) => fmt::Debug::fmt(name, f)?,
                Step::List(parts, depth) => {
                    // As `debug_list` writes its entries: `[]` where the list is empty.
                    steps.push(Step::Text("]"));
                    if pretty && !parts.is_empty() {
                        steps.push(Step::Indent(depth));
                        for part in parts.iter().rev() {
                            let part = Step::Formula(part, depth + 1);
                            steps.extend([Step::Text(",\n"), part, Step::Indent(depth + 1)]);
                        }
                        steps.push(Step::Text("[\n"));
                    } else {
                        for (i, part) in parts.iter().enumerate().rev() {
                            steps.push(Step::Formula(part, depth + 1));
                            if i > 0 {
                                steps.push(Step::Text(", "));
                            }
                        }
                        steps.push(Step::Text("["));
                    }
                }
                Step::Formula(formula, depth) => {
                    let inner = depth + 1;
                    let (name, first, second) = match formula {
                        Formula::Atom(name) => ("Atom", Step::Name(name), None),
                        Formula::Not(part) => ("Not", Step::Formula(part, inner), None),
                        Formula::And(parts) => ("And", Step::List(parts, inner), None),
                        Formula::Or(parts) => ("Or", Step::List(parts, inner), None),
                        Formula::ExactlyOne(parts) => {
                            ("ExactlyOne", Step::List(parts, inner), None)
                        }
                        Formula::Implies(a, b) => {
                            let b = Step::Formula(b, inner);
                            ("Implies", Step::Formula(a, inner), Some(b))
                        }
                        Formula::Iff(a, b) => {
                            let b = Step::Formula(b, inner);
                            ("Iff", Step::Formula(a, inner), Some(b))
                        }
                    };
                    f.write_str(name)?;
                    match second {
                        Some(second) => tuple(&mut steps, pretty, depth, &[first, second]),
                        None => tuple(&mut steps, pretty, depth, &[first]),
                    }
                }
            }
        }
        Ok(())
    }
}

/// A `logic` formula that doesn't follow the grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The byte of the formula's text where the error is.
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Not,
    And,
    Or,
    Implies,
    Iff,
    Open,
    Close,
    Comma,
    End,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c == '.' || c == '-'
}

fn lex(text: &str) -> Result<Vec<(usize, Token)>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        let token = match c {
            c if c.is_whitespace() => continue,
            '!' => Token::Not,
            '&' => Token::And,
            '|' => Token::Or,
            '(' => Token::Open,
            ')' => Token::Close,
            ',' => Token::Comma,
            '-' if text[offset..].starts_with("->") => {
                chars.next();
                Token::Implies
            }
            '<' if text[offset..].starts_with("<->") => {
                chars.next();
                chars.next();
                Token::Iff
            }
            c if is_ident_start(c) => {
                let mut end = offset + c.len_utf8();
                while let Some(&(next, n)) = chars.peek() {
                    // A `-` that begins `->` ends the identifier.
                    if !is_ident_char(n) || text[next..].starts_with("->") {
                        break;
                    }
                    end = next + n.len_utf8();
                    chars.next();
                }
                Token::Ident(text[offset..end].to_owned())
            }
            c => {
                return Err(ParseError {
                    offset,
                    message: format!("unexpected character `{c}`"),
                });
            }
        };
        tokens.push((offset, token));
    }
    tokens.push((text.len(), Token::End));
    Ok(tokens)
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    next: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.next].1
    }

    fn offset(&self) -> usize {
        self.tokens[self.next].0
    }

    fn take(&mut self) -> Token {
        let token = self.tokens[self.next].1.clone();
        if token != Token::End {
            self.next += 1;
        }
        token
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            offset: self.offset(),
            message: message.into(),
        })
    }

    fn expect(&mut self, token: Token, what: &str) -> Result<(), ParseError> {
        if *self.peek() == token {
            self.take();
            Ok(())
        } else {
            self.error(format!("expected {what}"))
        }
    }

    /// Reads the formula of the text, to its end. It keeps a run of `!` as a count, and each
    /// group around the one being read, a parenthesis or a function's arguments, on a stack on
    /// the heap, so a formula may nest as deep as memory allows.
    fn formula(&mut self) -> Result<Formula, ParseError> {
        let mut group = Group::Text;
        let mut pending = Pending::default();
        let mut outer = Vec::new();
        loop {
            // An operand: after a run of `!`, an identifier, or the start of a call or of a
            // parenthesis, whose formula is the operand once its group ends.
            let mut operand = loop {
                let offset = self.offset();
                match self.take() {
                    Token::Not => pending.nots += 1,
                    Token::Open => {
                        outer.push((group, mem::take(&mut pending)));
                        group = Group::Parens;
                    }
                    Token::Ident(name) if *self.peek() == Token::Open => {
                        self.take();
                        outer.push((group, mem::take(&mut pending)));
                        group = Group::Call(name, offset, Vec::new());
                    }
                    Token::Ident(name) => break Formula::Atom(name),
                    _ => {
                        return Err(ParseError {
                            offset,
                            message: "expected an identifier, `!` or `(`".to_owned(),
                        });
                    }
                }
            };
            // Then a binary operator, or the end of the operand's group, whose formula is an
            // operand of the group around it.
            loop {
                pending.operand(operand);
                if pending.operator(self.peek()) {
                    self.take();
                    break;
                }
                operand = mem::take(&mut pending).end();
                match group {
                    Group::Text if *self.peek() == Token::End => return Ok(operand),
                    Group::Text => return self.error("expected an operator or the formula's end"),
                    Group::Parens => self.expect(Token::Close, "`)`")?,
                    Group::Call(name, offset, mut args) => {
                        args.push(operand);
                        if *self.peek() == Token::Comma {
                            self.take();
                            group = Group::Call(name, offset, args);
                            break;
                        }
                        self.expect(Token::Close, "`,` or `)`")?;
                        operand = call(&name, offset, args)?;
                    }
                }
                (group, pending) = outer.pop().expect("only the text has no outer group");
            }
        }
    }
}

/// A group of a formula's text: the whole text, a formula in parentheses, or the arguments of
/// a call, with the function's name, the byte where the name starts, and the arguments read
/// before the one being read.
enum Group {
    Text,
    Parens,
    Call(String, usize, Vec<Formula>),
}

/// The operators of a group that wait for the operands after them: the count of `!` before the
/// next operand, the parts of the conjunction, the disjunction and the chain of implications
/// being read, each a part of the next, and the equivalence read so far.
#[derive(Default)]
struct Pending {
    nots: usize,
    and: Vec<Formula>,
    or: Vec<Formula>,
    implies: Vec<Formula>,
    iff: Option<Formula>,
}

impl Pending {
    /// Adds `formula`, under the run of `!` before it, to the conjunction being read.
    fn operand(&mut self, mut formula: Formula) {
        for _ in 0..mem::take(&mut self.nots) {
            formula = formula.not();
        }
        self.and.push(formula);
    }

    /// Reads `token` where it is a binary operator, and ends each part that binds tighter than
    /// it: `&` binds tightest, then `|`, `->` and `<->`. False where it isn't one.
    fn operator(&mut self, token: &Token) -> bool {
        match token {
            Token::And => {}
            Token::Or => self.end_and(),
            Token::Implies => self.end_or(),
            Token::Iff => self.end_implies(),
            _ => return false,
        }
        true
    }

    /// Ends the conjunction being read, as a part of the disjunction.
    fn end_and(&mut self) {
        let parts = mem::take(&mut self.and);
        self.or.push(one_or_all(parts, Formula::And));
    }

    /// Ends the disjunction being read, as a part of the chain of implications.
    fn end_or(&mut self) {
        self.end_and();
        let parts = mem::take(&mut self.or);
        self.implies.push(one_or_all(parts, Formula::Or));
    }

    /// Ends the chain of implications, which groups to the right, as a side of the equivalence.
    fn end_implies(&mut self) {
        self.end_or();
        let mut formula = self
            .implies
            .pop()
            .expect("a chain of implications has a part");
        while let Some(left) = self.implies.pop() {
            formula = left.implies(formula);
        }
        self.iff = Some(match self.iff.take() {
            Some(left) => left.iff(formula),
            None => formula,
        });
    }

    /// The group's formula, once its last operand is read.
    fn end(mut self) -> Formula {
        self.end_implies();
        self.iff.take().expect("a group's formula has a part")
    }
}

/// The formula of a call of the function `name`, whose name starts at byte `offset`.
fn call(name: &str, offset: usize, args: Vec<Formula>) -> Result<Formula, ParseError> {
    match name {
        "at_least_one" => Ok(Formula::at_least_one(args)),
        "exactly_one" => Ok(Formula::exactly_one(args)),
        _ => Err(ParseError {
            offset,
            message: format!("`{name}` isn't at_least_one or exactly_one"),
        }),
    }
}

/// The one part, or `all` of the parts.
fn one_or_all(mut parts: Vec<Formula>, all: fn(Vec<Formula>) -> Formula) -> Formula {
    if parts.len() == 1 {
        return parts.pop().expect("one part");
    }
    all(parts)
}

/// Reads a `logic` formula: identifiers, `!`, `&`, `|`, `->`, `<->`, parentheses,
/// `at_least_one(...)` and `exactly_one(...)`. `!` binds tightest, then `&`, `|`, `->`, which
/// groups to the right, and `<->`. A formula may nest as deep as memory allows.
pub fn parse(text: &str) -> Result<Formula, ParseError> {
    let mut parser = Parser {
        tokens: lex(text)?,
        next: 0,
    };
    parser.formula()
}

/// Where a formula or a condition comes from: the requirement, and the file and line that state
/// it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Origin {
    pub requirement: String,
    pub file: String,
    pub line: u32,
}

/// One formula of the set, with the requirement or relation it comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub formula: Formula,
    pub origin: Origin,
    /// The relation that states the formula, where a relation does.
    pub relation: Option<Relation>,
}

/// A relation that states a formula: its kind, such as `depends_on`, and its target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Relation {
    pub kind: String,
    pub target: String,
}

/// A condition's atom, which check 2 assumes true with the rest of the set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Condition {
    pub atom: String,
    pub origin: Origin,
}

/// An atom of the formula set: a requirement identifier, with what the set states of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Atom {
    pub id: String,
    /// The RFC whose directory holds the requirement, or else the RFC the identifier's form
    /// names.
    pub rfc: Option<u32>,
    /// The path of the section file that holds the requirement, where the set holds it.
    pub section: Option<String>,
    /// The requirement's level, such as `MUST`.
    pub level: Option<String>,
    /// The requirement's polarity, `required` or `forbidden`.
    pub polarity: Option<String>,
}

/// The formula set of a specification.
///
/// Its derived `Serialize` recurses once for each level of a formula's nesting, as the module's
/// documentation says; [`crate::json::formula_set`] writes the same JSON with a stack on the
/// heap.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FormulaSet {
    /// Each atom of the formulas and the conditions, in the order of its identifier.
    pub atoms: Vec<Atom>,
    pub formulas: Vec<Entry>,
    pub conditions: Vec<Condition>,
}

/// A formula set's clauses in a backend, with a selector literal for each formula.
pub struct Encoding<B> {
    backend: B,
    atoms: BTreeMap<String, Lit>,
    selectors: Vec<Lit>,
    truth: Option<Lit>,
}

impl<B: Backend> Encoding<B> {
    /// Adds the clauses of each formula of `set` to `backend`, each guarded by its selector.
    pub fn new(backend: B, set: &FormulaSet) -> Self {
        let mut encoding = Encoding {
            backend,
            atoms: BTreeMap::new(),
            selectors: Vec::with_capacity(set.formulas.len()),
            truth: None,
        };
        for condition in &set.conditions {
            encoding.atom(&condition.atom);
        }
        for entry in &set.formulas {
            let holds = encoding.encode(&entry.formula);
            let selector = encoding.backend.new_var();
            encoding.backend.add_clause(&[!selector, holds]);
            encoding.selectors.push(selector);
        }
        encoding
    }

    /// The selector of each formula, in the order of the set's formulas.
    pub fn selectors(&self) -> &[Lit] {
        &self.selectors
    }

    /// The literal of an atom of the set.
    pub fn atom_lit(&self, name: &str) -> Option<Lit> {
        self.atoms.get(name).copied()
    }

    /// Solves the clauses with each of `assumptions` true.
    pub fn solve(&mut self, assumptions: &[Lit]) -> Answer {
        self.backend.solve(assumptions)
    }

    fn atom(&mut self, name: &str) -> Lit {
        if let Some(&lit) = self.atoms.get(name) {
            return lit;
        }
        let lit = self.backend.new_var();
        self.atoms.insert(name.to_owned(), lit);
        lit
    }

    fn constant(&mut self, value: bool) -> Lit {
        let truth = match self.truth {
            Some(truth) => truth,
            None => {
                let truth = self.backend.new_var();
                self.backend.add_clause(&[truth]);
                self.truth = Some(truth);
                truth
            }
        };
        if !value {
            return !truth;
        }
        truth
    }

    /// A literal equivalent to `formula`, with the clauses that define it, which a fold of the
    /// formula adds from its atoms up.
    fn encode(&mut self, formula: &Formula) -> Lit {
        formula.fold(|formula, mut lits: Drain<'_, Lit>| match formula {
            Formula::Atom(name) => self.atom(name),
            Formula::Not(_) => !next_part(&mut lits),
            Formula::And(parts) if parts.is_empty() => self.constant(true),
            Formula::Or(parts) if parts.is_empty() => self.constant(false),
            Formula::And(_) => {
                let x = self.backend.new_var();
                let mut any_false = vec![x];
                for lit in lits {
                    self.backend.add_clause(&[!x, lit]);
                    any_false.push(!lit);
                }
                self.backend.add_clause(&any_false);
                x
            }
            Formula::Or(_) => {
                let x = self.backend.new_var();
                let mut any_true = Vec::new();
                for lit in lits {
                    self.backend.add_clause(&[x, !lit]);
                    any_true.push(lit);
                }
                self.add_chained_clause(!x, &any_true);
                x
            }
            Formula::ExactlyOne(_) => self.exactly_one_of(lits),
            Formula::Implies(..) => {
                let a = next_part(&mut lits);
                let b = next_part(&mut lits);
                let x = self.backend.new_var();
                self.backend.add_clause(&[!x, !a, b]);
                self.backend.add_clause(&[x, a]);
                self.backend.add_clause(&[x, !b]);
                x
            }
            Formula::Iff(..) => {
                let a = next_part(&mut lits);
                let b = next_part(&mut lits);
                let x = self.backend.new_var();
                self.backend.add_clause(&[!x, !a, b]);
                self.backend.add_clause(&[!x, a, !b]);
                self.backend.add_clause(&[x, a, b]);
                self.backend.add_clause(&[x, !a, !b]);
                x
            }
        })
    }

    /// Adds the clause `first | rest[0] | ... | rest[n - 1]` as a chain of clauses of at most
    /// three literals each, `first | rest[0] | c1`, `!c1 | rest[1] | c2`, and on to
    /// `!cm | rest[n - 2] | rest[n - 1]`, with a new variable `c` between each two. An assignment
    /// satisfies the chain, with each `c` set to match, exactly where it satisfies the clause, so
    /// the chain says what the clause says. A solver that sets the literals false one at a time,
    /// as a solve with an assumption for each of them does, looks over a clause's literals for a
    /// new one to watch each time a watched literal turns false, so one clause of n literals
    /// costs it about n squared steps in all, and each clause of the chain at most three.
    fn add_chained_clause(&mut self, first: Lit, mut rest: &[Lit]) {
        let mut head = first;
        while rest.len() > 2 {
            let next = self.backend.new_var();
            self.backend.add_clause(&[head, rest[0], next]);
            head = !next;
            rest = &rest[1..];
        }
        let mut last = vec![head];
        last.extend_from_slice(rest);
        self.backend.add_clause(&last);
    }

    /// A literal equivalent to exactly one of `lits` being true, with clauses linear in their
    /// count: walking `lits` in order, `some` holds where at least one of the literals so far is
    /// true and `two` where at least two are, and the result holds where `some` does and `two`
    /// doesn't. With no literal, the result is false.
    fn exactly_one_of(&mut self, lits: impl IntoIterator<Item = Lit>) -> Lit {
        let mut some: Option<Lit> = None;
        let mut two: Option<Lit> = None;
        for lit in lits {
            let before = match some {
                Some(before) => before,
                None => {
                    some = Some(lit);
                    continue;
                }
            };
            // The next `two` holds where `two` did, or where `before` and `lit` both hold.
            let next_two = self.backend.new_var();
            match two {
                Some(two) => {
                    self.backend.add_clause(&[!next_two, two, before]);
                    self.backend.add_clause(&[!next_two, two, lit]);
                    self.backend.add_clause(&[next_two, !two]);
                }
                None => {
                    self.backend.add_clause(&[!next_two, before]);
                    self.backend.add_clause(&[!next_two, lit]);
                }
            }
            self.backend.add_clause(&[next_two, !before, !lit]);
            // The next `some` holds where `before` or `lit` does.
            let next_some = self.backend.new_var();
            self.backend.add_clause(&[!next_some, before, lit]);
            self.backend.add_clause(&[next_some, !before]);
            self.backend.add_clause(&[next_some, !lit]);
            some = Some(next_some);
            two = Some(next_two);
        }
        match (some, two) {
            (None, _) => self.constant(false),
            (Some(some), None) => some,
            (Some(some), Some(two)) => {
                let x = self.backend.new_var();
                self.backend.add_clause(&[!x, some]);
                self.backend.add_clause(&[!x, !two]);
                self.backend.add_clause(&[x, !some, two]);
                x
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{parse, Encoding, Entry, Formula, FormulaSet, Origin};
    use crate::solver::{Answer, Backend, Lit};

    fn a(name: &str) -> Formula {
        Formula::atom(name)
    }

    /// `Formula`'s variants, with the comparison and the debug output that `#[derive]` writes,
    /// to check the ones `Formula` writes by hand against.
    #[derive(Debug, PartialEq)]
    enum Derived {
        Atom(String),
        Not(Box<Derived>),
        And(Vec<Derived>),
        Or(Vec<Derived>),
        ExactlyOne(Vec<Derived>),
        Implies(Box<Derived>, Box<Derived>),
        Iff(Box<Derived>, Box<Derived>),
    }

    /// `formula` as a `Derived`. It recurses once for each level, so it reads shallow formulas
    /// alone.
    fn derived(formula: &Formula) -> Derived {
        match formula {
            Formula::Atom(name) => Derived::Atom(name.clone()),
            Formula::Not(part) => Derived::Not(Box::new(derived(part))),
            Formula::And(parts) => Derived::And(parts.iter().map(derived).collect()),
            Formula::Or(parts) => Derived::Or(parts.iter().map(derived).collect()),
            Formula::ExactlyOne(parts) => Derived::ExactlyOne(parts.iter().map(derived).collect()),
            Formula::Implies(a, b) => Derived::Implies(Box::new(derived(a)), Box::new(derived(b))),
            Formula::Iff(a, b) => Derived::Iff(Box::new(derived(a)), Box::new(derived(b))),
        }
    }

    /// Shallow formulas of each variant compare, and debug-print in `{:?}` and in `{:#?}`, alone
    /// and as an entry of a list, as the derived implementations write them.
    #[test]
    fn comparison_and_debug_output_are_the_derived_ones() {
        let formulas = [
            a("A"),
            a("B"),
            a("say \"hi\"\n"),
            a("A").not(),
            a("B").not(),
            Formula::And(Vec::new()),
            Formula::Or(Vec::new()),
            Formula::And(vec![a("A"), a("B")]),
            Formula::And(vec![a("A"), a("B"), a("C")]),
            Formula::Or(vec![a("A"), a("B")]),
            Formula::Or(vec![a("A"), Formula::And(vec![a("B"), a("C").not()])]),
            Formula::ExactlyOne(Vec::new()),
            Formula::ExactlyOne(vec![a("A"), a("B")]),
            a("A").implies(a("B")),
            a("B").implies(a("A")),
            a("A").iff(a("B")),
            a("A").implies(a("B").iff(Formula::Or(Vec::new()))),
            parse("exactly_one(A, B, C) <-> !(D -> E)").unwrap(),
        ];
        for x in &formulas {
            let expected = derived(x);
            assert_eq!(format!("{x:?}"), format!("{expected:?}"));
            assert_eq!(format!("{x:#?}"), format!("{expected:#?}"));
            assert_eq!(format!("{:#?}", [x]), format!("{:#?}", [&expected]));
            for y in &formulas {
                assert_eq!(x == y, expected == derived(y), "{x} and {y}");
            }
        }
    }

    #[test]
    fn precedence() {
        assert_eq!(
            parse("!A & B | C -> D <-> E").unwrap(),
            Formula::Or(vec![Formula::And(vec![a("A").not(), a("B")]), a("C")])
                .implies(a("D"))
                .iff(a("E"))
        );
        assert_eq!(
            parse("A -> B -> C").unwrap(),
            a("A").implies(a("B").implies(a("C")))
        );
        assert_eq!(
            parse("!(A | B)").unwrap(),
            Formula::Or(vec![a("A"), a("B")]).not()
        );
        assert_eq!(
            parse("A <-> B <-> C").unwrap(),
            a("A").iff(a("B")).iff(a("C"))
        );
        assert_eq!(
            parse("A & B & C").unwrap(),
            Formula::And(vec![a("A"), a("B"), a("C")])
        );
        assert_eq!(
            parse("A | B | C").unwrap(),
            Formula::Or(vec![a("A"), a("B"), a("C")])
        );
    }

    #[test]
    fn identifiers_hold_dashes() {
        assert_eq!(
            parse("EX-1-001->!EX-1-002").unwrap(),
            a("EX-1-001").implies(a("EX-1-002").not())
        );
        assert_eq!(parse("  EX-1-001.a_b  ").unwrap(), a("EX-1-001.a_b"));
    }

    #[test]
    fn functions() {
        assert_eq!(
            parse("at_least_one(A, B & C)").unwrap(),
            Formula::Or(vec![a("A"), Formula::And(vec![a("B"), a("C")])])
        );
        assert_eq!(
            parse("exactly_one(A, B)").unwrap(),
            Formula::ExactlyOne(vec![a("A"), a("B")])
        );
        assert_eq!(parse("at_least_one").unwrap(), a("at_least_one"));
    }

    #[test]
    fn errors() {
        for (text, offset) in [
            ("", 0),
            ("A &", 3),
            ("(A | B", 6),
            ("A B", 2),
            ("A # B", 2),
            ("some(A)", 0),
            ("exactly_one()", 12),
            ("A <- B", 2),
        ] {
            let error = parse(text).unwrap_err();
            assert_eq!(error.offset, offset, "{text:?}: {error}");
        }

        // These texts stop after `!` or inside a call's arguments, or close a parenthesis that
        // wasn't opened; each error says what the parser expected at its offset.
        for (text, offset, message) in [
            ("A)", 1, "expected an operator or the formula's end"),
            ("!", 1, "expected an identifier, `!` or `(`"),
            ("at_least_one(A,", 15, "expected an identifier, `!` or `(`"),
            ("at_least_one(A", 14, "expected `,` or `)`"),
        ] {
            let error = parse(text).unwrap_err();
            assert_eq!(error.offset, offset, "{text:?}: {error}");
            assert_eq!(error.message, message, "{text:?}");
        }
    }

    #[test]
    fn display_reads_back() {
        for text in ["!A & (B | C) -> D", "A <-> !B", "exactly_one(A, B, C)"] {
            let formula = parse(text).unwrap();
            assert_eq!(parse(&formula.to_string()).unwrap(), formula);
        }
    }

    fn set(formulas: &[&str]) -> FormulaSet {
        FormulaSet {
            atoms: Vec::new(),
            formulas: formulas
                .iter()
                .enumerate()
                .map(|(i, text)| Entry {
                    formula: parse(text).unwrap(),
                    origin: Origin {
                        requirement: format!("EX-1-{i:03}"),
                        file: "rfcs/1-example/01-example.toml".to_owned(),
                        line: i as u32 + 1,
                    },
                    relation: None,
                })
                .collect(),
            conditions: Vec::new(),
        }
    }

    fn sat<B: Backend>(encoding: &mut Encoding<B>, atoms: &[(&str, bool)]) -> bool {
        let mut assumptions = encoding.selectors().to_vec();
        for &(name, value) in atoms {
            let lit = encoding.atom_lit(name).unwrap();
            assumptions.push(if value { lit } else { !lit });
        }
        encoding.solve(&assumptions) == Answer::Sat
    }

    /// A formula over `A` and `B`, as its truth table.
    type Table = fn(bool, bool) -> bool;

    /// Each formula is encoded as itself: the encoding agrees with the formula's truth table.
    fn encoding_agrees_with_truth_tables<B: Backend + Default>() {
        let cases: [(&str, Table); 10] = [
            ("A & B", |x, y| x && y),
            ("A | B", |x, y| x || y),
            ("A -> B", |x, y| !x || y),
            ("A <-> B", |x, y| x == y),
            ("!(A & !B)", |x, y| !(x && !y)),
            ("exactly_one(A, B)", |x, y| x != y),
            ("exactly_one(A)", |x, _| x),
            ("exactly_one(A, B, A)", |x, y| !x && y),
            ("exactly_one(A, B, !A)", |_, y| !y),
            ("!exactly_one(A, B, !B)", |x, _| x),
        ];
        for (text, table) in cases {
            let mut encoding = Encoding::new(B::default(), &set(&[text, "A | !A", "B | !B"]));
            for x in [false, true] {
                for y in [false, true] {
                    let found = sat(&mut encoding, &[("A", x), ("B", y)]);
                    assert_eq!(found, table(x, y), "{text} with A={x} and B={y}");
                }
            }
        }
    }

    /// A formula holds only where its selector is assumed.
    fn selectors_switch_formulas<B: Backend + Default>() {
        let mut encoding = Encoding::new(B::default(), &set(&["A", "!A"]));
        let selectors = encoding.selectors().to_vec();
        assert_eq!(selectors.len(), 2);
        let both = encoding.solve(&selectors);
        assert!(matches!(both, Answer::Unsat { .. }));
        assert_eq!(encoding.solve(&selectors[..1]), Answer::Sat);
        assert_eq!(encoding.solve(&selectors[1..]), Answer::Sat);
    }

    /// An empty conjunction is true; an empty disjunction and an empty `ExactlyOne` are false.
    fn empty_compositions<B: Backend + Default>() {
        let mut empty = FormulaSet::default();
        for formula in [
            Formula::And(Vec::new()),
            Formula::Or(Vec::new()),
            Formula::ExactlyOne(Vec::new()),
        ] {
            empty.formulas.push(Entry {
                formula,
                origin: Origin {
                    requirement: "EX-1-001".to_owned(),
                    file: "rfcs/1-example/01-example.toml".to_owned(),
                    line: 1,
                },
                relation: None,
            });
        }
        let mut encoding = Encoding::new(B::default(), &empty);
        let selectors = encoding.selectors().to_vec();
        assert_eq!(selectors.len(), 3);
        assert_eq!(encoding.solve(&selectors[..1]), Answer::Sat);
        let or = encoding.solve(&selectors[1..2]);
        assert!(matches!(or, Answer::Unsat { .. }));
        let exactly_one = encoding.solve(&selectors[2..]);
        assert!(matches!(exactly_one, Answer::Unsat { .. }));
    }

    /// The nesting of the deep formulas' tests.
    const DEPTH: usize = 100_000;

    /// A formula of `DEPTH` nested `!` reads, prints, debug-prints, compares, clones, drops and
    /// encodes without overflowing the stack.
    fn deep_negation<B: Backend + Default>() {
        let text = format!("{}A", "!".repeat(DEPTH));
        let formula = parse(&text).unwrap();
        let mut inner = &formula;
        let mut nots = 0;
        while let Formula::Not(part) = inner {
            inner = part;
            nots += 1;
        }
        assert_eq!(nots, DEPTH);
        assert_eq!(*inner, a("A"));
        assert_eq!(formula.to_string(), text);
        assert_eq!(formula.clone().to_string(), text);
        assert!(formula == parse(&text).unwrap());
        assert!(formula != parse(&text[1..]).unwrap());
        assert!(formula != parse(&format!("{}B", "!".repeat(DEPTH))).unwrap());
        let debug = format!("{}Atom(\"A\"){}", "Not(".repeat(DEPTH), ")".repeat(DEPTH));
        assert_eq!(format!("{formula:?}"), debug);
        let mut encoding = Encoding::new(B::default(), &set(&[text.as_str()]));
        assert!(sat(&mut encoding, &[("A", true)]));
        assert!(!sat(&mut encoding, &[("A", false)]));
    }

    /// A formula of `DEPTH` nested parentheses reads, prints, debug-prints, compares, clones,
    /// drops and encodes without overflowing the stack, and so do bare and unclosed parentheses.
    fn deep_parentheses<B: Backend + Default>() {
        let text = format!("{}A{}", "(A & ".repeat(DEPTH), ")".repeat(DEPTH));
        let formula = parse(&text).unwrap();
        let mut inner = &formula;
        let mut ands = 0;
        while let Formula::And(parts) = inner {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], a("A"));
            inner = &parts[1];
            ands += 1;
        }
        assert_eq!(ands, DEPTH);
        assert_eq!(*inner, a("A"));
        assert_eq!(formula.to_string(), text);
        assert_eq!(formula.clone().to_string(), text);
        assert!(formula == parse(&text).unwrap());
        let last = format!("{}B{}", "(A & ".repeat(DEPTH), ")".repeat(DEPTH));
        assert!(formula != parse(&last).unwrap());
        let and = "And([Atom(\"A\"), ".repeat(DEPTH);
        let debug = format!("{and}Atom(\"A\"){}", "])".repeat(DEPTH));
        assert_eq!(format!("{formula:?}"), debug);
        let mut encoding = Encoding::new(B::default(), &set(&[text.as_str()]));
        assert!(sat(&mut encoding, &[("A", true)]));
        assert!(!sat(&mut encoding, &[("A", false)]));
        let bare = format!("{}A{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
        assert_eq!(parse(&bare).unwrap(), a("A"));
        let unclosed = format!("{}A", "(".repeat(DEPTH));
        assert_eq!(parse(&unclosed).unwrap_err().offset, DEPTH + 1);
    }

    /// A formula of `DEPTH` nested `at_least_one(` calls reads, prints, clones, drops and
    /// encodes without overflowing the stack.
    fn deep_calls<B: Backend + Default>() {
        let text = format!("{}A{}", "at_least_one(A, ".repeat(DEPTH), ")".repeat(DEPTH));
        let formula = parse(&text).unwrap();
        let mut inner = &formula;
        let mut ors = 0;
        while let Formula::Or(parts) = inner {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], a("A"));
            inner = &parts[1];
            ors += 1;
        }
        assert_eq!(ors, DEPTH);
        assert_eq!(*inner, a("A"));
        let printed = format!("{}A{}", "(A | ".repeat(DEPTH), ")".repeat(DEPTH));
        assert_eq!(formula.to_string(), printed);
        assert_eq!(formula.clone().to_string(), printed);
        let mut encoding = Encoding::new(B::default(), &set(&[text.as_str()]));
        assert!(sat(&mut encoding, &[("A", true)]));
        assert!(!sat(&mut encoding, &[("A", false)]));
    }

    /// A chain of `DEPTH` implications, which groups to the right, reads, prints, clones, drops
    /// and encodes without overflowing the stack.
    fn deep_implications<B: Backend + Default>() {
        let text = format!("{}!A", "A -> ".repeat(DEPTH));
        let formula = parse(&text).unwrap();
        let mut inner = &formula;
        let mut implications = 0;
        while let Formula::Implies(left, right) = inner {
            assert_eq!(**left, a("A"));
            inner = right;
            implications += 1;
        }
        assert_eq!(implications, DEPTH);
        assert_eq!(*inner, a("A").not());
        let printed = format!("{}!A{}", "(A -> ".repeat(DEPTH), ")".repeat(DEPTH));
        assert_eq!(formula.to_string(), printed);
        assert_eq!(formula.clone().to_string(), printed);
        let mut encoding = Encoding::new(B::default(), &set(&[text.as_str()]));
        assert!(!sat(&mut encoding, &[("A", true)]));
        assert!(sat(&mut encoding, &[("A", false)]));
    }

    /// A backend that counts the clauses an encoding adds to the backend it wraps, and the
    /// literals of the widest of them.
    struct Counted<B> {
        backend: B,
        clauses: Rc<Cell<usize>>,
        widest: Rc<Cell<usize>>,
    }

    impl<B: Backend> Backend for Counted<B> {
        fn new_var(&mut self) -> Lit {
            self.backend.new_var()
        }

        fn add_clause(&mut self, clause: &[Lit]) {
            self.clauses.set(self.clauses.get() + 1);
            self.widest.set(self.widest.get().max(clause.len()));
            self.backend.add_clause(clause);
        }

        fn solve(&mut self, assumptions: &[Lit]) -> Answer {
            self.backend.solve(assumptions)
        }
    }

    /// The encoding of the formula `text`, and the count of clauses it added.
    fn counted<B: Backend + Default>(text: &str) -> (Encoding<Counted<B>>, usize) {
        let (encoding, clauses, _) = tallied::<B>(text);
        (encoding, clauses)
    }

    /// The encoding of the formula `text`, the count of clauses it added, and the literals of
    /// the widest of them.
    fn tallied<B: Backend + Default>(text: &str) -> (Encoding<Counted<B>>, usize, usize) {
        let clauses = Rc::new(Cell::new(0));
        let widest = Rc::new(Cell::new(0));
        let backend = Counted {
            backend: B::default(),
            clauses: Rc::clone(&clauses),
            widest: Rc::clone(&widest),
        };
        let encoding = Encoding::new(backend, &set(&[text]));
        (encoding, clauses.get(), widest.get())
    }

    /// Each of `names`, true where `true_at` holds its index and false elsewhere.
    fn assume<'n>(names: &'n [String], true_at: &[usize]) -> Vec<(&'n str, bool)> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.as_str(), true_at.contains(&i)))
            .collect()
    }

    /// The count of parts of the wide `exactly_one` test.
    const PARTS: usize = 100_000;

    /// An `exactly_one` of `PARTS` atoms reads, prints and clones as itself, and encodes in
    /// clauses linear in its parts, true where exactly one of them is.
    fn wide_exactly_one<B: Backend + Default>() {
        let names: Vec<String> = (0..PARTS).map(|i| format!("P{i}")).collect();
        let text = format!("exactly_one({})", names.join(", "));
        let formula = parse(&text).unwrap();
        let parts: Vec<Formula> = names.iter().map(Formula::atom).collect();
        assert_eq!(formula, Formula::ExactlyOne(parts));
        assert_eq!(formula.to_string(), text);
        assert_eq!(formula.clone().to_string(), text);
        let (mut encoding, clauses) = counted::<B>(&text);
        assert!(clauses <= 8 * PARTS, "{clauses} clauses for {PARTS} parts");
        assert!(!sat(&mut encoding, &assume(&names, &[])));
        for part in [0, PARTS / 2, PARTS - 1] {
            let found = sat(&mut encoding, &assume(&names, &[part]));
            assert!(found, "P{part} alone");
        }
        assert!(!sat(&mut encoding, &assume(&names, &[0, PARTS - 1])));
        let neighbors = assume(&names, &[PARTS / 2, PARTS / 2 + 1]);
        assert!(!sat(&mut encoding, &neighbors));
    }

    /// An `at_least_one` of `PARTS` atoms encodes in clauses linear in its parts and of at most
    /// three literals each, so a solve that assumes each part false looks at each clause a
    /// bounded number of times, and holds where at least one part is true. An `at_least_one` of
    /// one to five atoms agrees with its truth table.
    fn wide_at_least_one<B: Backend + Default>() {
        for count in 1..=5usize {
            let names: Vec<String> = (0..count).map(|i| format!("P{i}")).collect();
            let text = format!("at_least_one({})", names.join(", "));
            let mut encoding = Encoding::new(B::default(), &set(&[text.as_str()]));
            for row in 0..1usize << count {
                let true_at: Vec<usize> = (0..count).filter(|&i| (row >> i) & 1 == 1).collect();
                let found = sat(&mut encoding, &assume(&names, &true_at));
                assert_eq!(found, !true_at.is_empty(), "{text} with {true_at:?} true");
            }
        }
        let names: Vec<String> = (0..PARTS).map(|i| format!("P{i}")).collect();
        let text = format!("at_least_one({})", names.join(", "));
        let (mut encoding, clauses, widest) = tallied::<B>(&text);
        assert!(clauses <= 3 * PARTS, "{clauses} clauses for {PARTS} parts");
        assert!(widest <= 3, "a clause of {widest} literals");
        assert!(!sat(&mut encoding, &assume(&names, &[])));
        for part in [0, PARTS / 2, PARTS - 1] {
            let found = sat(&mut encoding, &assume(&names, &[part]));
            assert!(found, "P{part} alone");
        }
        assert!(sat(&mut encoding, &assume(&names, &[0, PARTS - 1])));
    }

    /// An `exactly_one(B, ...)` nested `DEPTH` deep reads, prints, clones, drops and encodes
    /// with each part once, so its clauses grow with the depth alone. A call over `B` and a part
    /// `F` holds where `B` and `F` differ, so an even count of calls holds where `A` does.
    fn nested_exactly_one<B: Backend + Default>() {
        let text = format!("{}A{}", "exactly_one(B, ".repeat(DEPTH), ")".repeat(DEPTH));
        let formula = parse(&text).unwrap();
        let mut inner = &formula;
        let mut calls = 0;
        while let Formula::ExactlyOne(parts) = inner {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], a("B"));
            inner = &parts[1];
            calls += 1;
        }
        assert_eq!(calls, DEPTH);
        assert_eq!(*inner, a("A"));
        assert_eq!(formula.to_string(), text);
        assert_eq!(formula.clone().to_string(), text);
        let (mut encoding, clauses) = counted::<B>(&text);
        assert!(clauses <= 10 * DEPTH, "{clauses} clauses for {DEPTH} calls");
        for x in [false, true] {
            for y in [false, true] {
                let holds = if DEPTH.is_multiple_of(2) { x } else { x != y };
                let found = sat(&mut encoding, &[("A", x), ("B", y)]);
                assert_eq!(found, holds, "A={x} B={y}");
            }
        }
    }

    crate::solver::backend_tests!(
        encoding_agrees_with_truth_tables,
        selectors_switch_formulas,
        empty_compositions,
        deep_negation,
        deep_parentheses,
        deep_calls,
        deep_implications,
        wide_exactly_one,
        wide_at_least_one,
        nested_exactly_one,
    );
}
