//! Formulas over requirement identifiers, the grammar of a requirement's `logic` field, and the
//! clauses a solver reads.
//!
//! Each requirement identifier is a propositional atom. The checker turns each formula of the set
//! into clauses by the Tseitin transform, and gives each formula a selector literal, so that the
//! formula holds only where its selector is assumed true.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::solver::{Answer, Backend, Lit};

/// A formula over requirement identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Formula {
    /// A requirement identifier, or a condition's atom.
    Atom(String),
    Not(Box<Formula>),
    /// True where each part is true; an empty conjunction is true.
    And(Vec<Formula>),
    /// True where a part is true; an empty disjunction is false.
    Or(Vec<Formula>),
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

    /// Exactly one of the parts is true: at least one, and no two together.
    pub fn exactly_one(parts: Vec<Formula>) -> Self {
        let mut all = vec![Formula::Or(parts.clone())];
        for (i, first) in parts.iter().enumerate() {
            for second in &parts[i + 1..] {
                all.push(Formula::And(vec![first.clone(), second.clone()]).not());
            }
        }
        Formula::And(all)
    }
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn join(f: &mut fmt::Formatter<'_>, parts: &[Formula], op: &str) -> fmt::Result {
            write!(f, "(")?;
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    write!(f, " {op} ")?;
                }
                write!(f, "{part}")?;
            }
            write!(f, ")")
        }

        match self {
            Formula::Atom(name) => write!(f, "{name}"),
            Formula::Not(inner) => write!(f, "!{inner}"),
            Formula::And(parts) => join(f, parts, "&"),
            Formula::Or(parts) => join(f, parts, "|"),
            Formula::Implies(a, b) => write!(f, "({a} -> {b})"),
            Formula::Iff(a, b) => write!(f, "({a} <-> {b})"),
        }
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

    // iff := implies ('<->' implies)*
    fn iff(&mut self) -> Result<Formula, ParseError> {
        let mut formula = self.implies()?;
        while *self.peek() == Token::Iff {
            self.take();
            formula = formula.iff(self.implies()?);
        }
        Ok(formula)
    }

    // implies := or ('->' implies)?, so `a -> b -> c` reads as `a -> (b -> c)`
    fn implies(&mut self) -> Result<Formula, ParseError> {
        let formula = self.or()?;
        if *self.peek() == Token::Implies {
            self.take();
            return Ok(formula.implies(self.implies()?));
        }
        Ok(formula)
    }

    // or := and ('|' and)*
    fn or(&mut self) -> Result<Formula, ParseError> {
        let mut parts = vec![self.and()?];
        while *self.peek() == Token::Or {
            self.take();
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Formula::Or(parts)
        })
    }

    // and := unary ('&' unary)*
    fn and(&mut self) -> Result<Formula, ParseError> {
        let mut parts = vec![self.unary()?];
        while *self.peek() == Token::And {
            self.take();
            parts.push(self.unary()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Formula::And(parts)
        })
    }

    // unary := '!' unary | primary
    fn unary(&mut self) -> Result<Formula, ParseError> {
        if *self.peek() == Token::Not {
            self.take();
            return Ok(self.unary()?.not());
        }
        self.primary()
    }

    // primary := identifier | function '(' iff (',' iff)* ')' | '(' iff ')'
    fn primary(&mut self) -> Result<Formula, ParseError> {
        let offset = self.offset();
        match self.take() {
            Token::Open => {
                let formula = self.iff()?;
                self.expect(Token::Close, "`)`")?;
                Ok(formula)
            }
            Token::Ident(name) if *self.peek() == Token::Open => {
                self.take();
                let mut parts = vec![self.iff()?];
                while *self.peek() == Token::Comma {
                    self.take();
                    parts.push(self.iff()?);
                }
                self.expect(Token::Close, "`,` or `)`")?;
                match name.as_str() {
                    "at_least_one" => Ok(Formula::at_least_one(parts)),
                    "exactly_one" => Ok(Formula::exactly_one(parts)),
                    _ => Err(ParseError {
                        offset,
                        message: format!("`{name}` isn't at_least_one or exactly_one"),
                    }),
                }
            }
            Token::Ident(name) => Ok(Formula::Atom(name)),
            _ => Err(ParseError {
                offset,
                message: "expected an identifier, `!` or `(`".to_owned(),
            }),
        }
    }
}

/// Reads a `logic` formula: identifiers, `!`, `&`, `|`, `->`, `<->`, parentheses,
/// `at_least_one(...)` and `exactly_one(...)`. `!` binds tightest, then `&`, `|`, `->`, which
/// groups to the right, and `<->`.
pub fn parse(text: &str) -> Result<Formula, ParseError> {
    let mut parser = Parser {
        tokens: lex(text)?,
        next: 0,
    };
    let formula = parser.iff()?;
    if *parser.peek() != Token::End {
        return parser.error("expected an operator or the formula's end");
    }
    Ok(formula)
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
}

/// A condition's atom, which check 2 assumes true with the rest of the set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Condition {
    pub atom: String,
    pub origin: Origin,
}

/// The formula set of a specification.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FormulaSet {
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

    /// A literal equivalent to `formula`, with the clauses that define it.
    fn encode(&mut self, formula: &Formula) -> Lit {
        match formula {
            Formula::Atom(name) => self.atom(name),
            Formula::Not(inner) => !self.encode(inner),
            Formula::And(parts) if parts.is_empty() => self.constant(true),
            Formula::Or(parts) if parts.is_empty() => self.constant(false),
            Formula::And(parts) => {
                let lits: Vec<Lit> = parts.iter().map(|part| self.encode(part)).collect();
                let x = self.backend.new_var();
                let mut any_false = vec![x];
                for &lit in &lits {
                    self.backend.add_clause(&[!x, lit]);
                    any_false.push(!lit);
                }
                self.backend.add_clause(&any_false);
                x
            }
            Formula::Or(parts) => {
                let lits: Vec<Lit> = parts.iter().map(|part| self.encode(part)).collect();
                let x = self.backend.new_var();
                let mut any_true = vec![!x];
                for &lit in &lits {
                    self.backend.add_clause(&[x, !lit]);
                    any_true.push(lit);
                }
                self.backend.add_clause(&any_true);
                x
            }
            Formula::Implies(a, b) => {
                let a = self.encode(a);
                let b = self.encode(b);
                let x = self.backend.new_var();
                self.backend.add_clause(&[!x, !a, b]);
                self.backend.add_clause(&[x, a]);
                self.backend.add_clause(&[x, !b]);
                x
            }
            Formula::Iff(a, b) => {
                let a = self.encode(a);
                let b = self.encode(b);
                let x = self.backend.new_var();
                self.backend.add_clause(&[!x, !a, b]);
                self.backend.add_clause(&[!x, a, !b]);
                self.backend.add_clause(&[x, a, b]);
                self.backend.add_clause(&[x, !a, !b]);
                x
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Encoding, Entry, Formula, FormulaSet, Origin};
    use crate::solver::{Answer, Backend};

    fn a(name: &str) -> Formula {
        Formula::atom(name)
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
            Formula::And(vec![
                Formula::Or(vec![a("A"), a("B")]),
                Formula::And(vec![a("A"), a("B")]).not(),
            ])
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

    /// Each formula is encoded as itself: the encoding agrees with the formula's truth table.
    fn encoding_agrees_with_truth_tables<B: Backend + Default>() {
        let cases: [(&str, fn(bool, bool) -> bool); 6] = [
            ("A & B", |x, y| x && y),
            ("A | B", |x, y| x || y),
            ("A -> B", |x, y| !x || y),
            ("A <-> B", |x, y| x == y),
            ("!(A & !B)", |x, y| !(x && !y)),
            ("exactly_one(A, B)", |x, y| x != y),
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

    /// An empty conjunction is true and an empty disjunction false.
    fn empty_compositions<B: Backend + Default>() {
        let mut empty = FormulaSet::default();
        for formula in [Formula::And(Vec::new()), Formula::Or(Vec::new())] {
            empty.formulas.push(Entry {
                formula,
                origin: Origin {
                    requirement: "EX-1-001".to_owned(),
                    file: "rfcs/1-example/01-example.toml".to_owned(),
                    line: 1,
                },
            });
        }
        let mut encoding = Encoding::new(B::default(), &empty);
        let selectors = encoding.selectors().to_vec();
        assert_eq!(encoding.solve(&selectors[..1]), Answer::Sat);
        let or = encoding.solve(&selectors[1..]);
        assert!(matches!(or, Answer::Unsat { .. }));
    }

    crate::solver::backend_tests!(
        encoding_agrees_with_truth_tables,
        selectors_switch_formulas,
        empty_compositions,
    );
}
