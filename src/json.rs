//! The formula set as JSON, in the shape of serde's derived `Serialize`, written with a
//! stack on the heap.
//!
//! [`FormulaSet`] and the types it holds derive `Serialize`, so a host stores or returns the set
//! as JSON. That serialization recurses once for each level of a formula's nesting: each method
//! of serde's `Serializer` that writes a nested value, such as `serialize_newtype_variant` for
//! the part of `Not` and `SerializeSeq::serialize_element` for each part of `And`, is handed
//! that value as a `&T` with `T: Serialize` and serializes it before it returns. So a formula
//! nested deep enough overflows the stack there.
//!
//! The functions here return the JSON that `serde_json::to_string` returns for the derived
//! `Serialize`, and walk a formula with a stack on the heap, so a formula may nest as deep as
//! memory allows. A host that writes a set whose formulas come from section files writes it with
//! [`formula_set`], and the command writes its `--formulas` file from [`atom`], [`entry`] and
//! [`condition`].

use std::fmt::Write as _;

use crate::logic::{Atom, Condition, Entry, Formula, FormulaSet, Origin, Relation};

/// The formula set as one JSON object of its atoms, its formulas and its conditions.
pub fn formula_set(set: &FormulaSet) -> String {
    let mut output = String::new();
    output.push_str("{\"atoms\":");
    write_list(&mut output, &set.atoms, write_atom);
    output.push_str(",\"formulas\":");
    write_list(&mut output, &set.formulas, write_entry);
    output.push_str(",\"conditions\":");
    write_list(&mut output, &set.conditions, write_condition);
    output.push('}');
    output
}

/// An atom of the formula set as a JSON object.
pub fn atom(atom: &Atom) -> String {
    let mut output = String::new();
    write_atom(&mut output, atom);
    output
}

/// A formula of the set, with its origin and its relation, as a JSON object.
pub fn entry(entry: &Entry) -> String {
    let mut output = String::new();
    write_entry(&mut output, entry);
    output
}

/// A condition of the set as a JSON object.
pub fn condition(condition: &Condition) -> String {
    let mut output = String::new();
    write_condition(&mut output, condition);
    output
}

/// A formula as a JSON object whose one key is its kind: `atom`, `not`, `and`, `or`, `implies`,
/// `iff` or `exactly_one`.
pub fn formula(formula: &Formula) -> String {
    let mut output = String::new();
    write_formula(&mut output, formula);
    output
}

fn write_list<T>(output: &mut String, items: &[T], write: fn(&mut String, &T)) {
    output.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        write(output, item);
    }
    output.push(']');
}

fn write_atom(output: &mut String, atom: &Atom) {
    output.push_str("{\"id\":");
    write_string(output, &atom.id);
    output.push_str(",\"rfc\":");
    match atom.rfc {
        Some(rfc) => {
            let _ = write!(output, "{rfc}");
        }
        None => output.push_str("null"),
    }
    output.push_str(",\"section\":");
    write_option(output, atom.section.as_deref());
    output.push_str(",\"level\":");
    write_option(output, atom.level.as_deref());
    output.push_str(",\"polarity\":");
    write_option(output, atom.polarity.as_deref());
    output.push('}');
}

fn write_entry(output: &mut String, entry: &Entry) {
    output.push_str("{\"formula\":");
    write_formula(output, &entry.formula);
    output.push_str(",\"origin\":");
    write_origin(output, &entry.origin);
    output.push_str(",\"relation\":");
    match &entry.relation {
        Some(relation) => write_relation(output, relation),
        None => output.push_str("null"),
    }
    output.push('}');
}

fn write_relation(output: &mut String, relation: &Relation) {
    output.push_str("{\"kind\":");
    write_string(output, &relation.kind);
    output.push_str(",\"target\":");
    write_string(output, &relation.target);
    output.push('}');
}

fn write_condition(output: &mut String, condition: &Condition) {
    output.push_str("{\"atom\":");
    write_string(output, &condition.atom);
    output.push_str(",\"origin\":");
    write_origin(output, &condition.origin);
    output.push('}');
}

fn write_origin(output: &mut String, origin: &Origin) {
    output.push_str("{\"requirement\":");
    write_string(output, &origin.requirement);
    output.push_str(",\"file\":");
    write_string(output, &origin.file);
    let _ = write!(output, ",\"line\":{}}}", origin.line);
}

/// Writes the formula from a stack of steps on the heap, so a deep formula doesn't grow the call
/// stack: each step is a formula still to write or the text that closes one.
fn write_formula(output: &mut String, formula: &Formula) {
    enum Step<'a> {
        Formula(&'a Formula),
        Text(&'static str),
    }
    let mut stack = vec![Step::Formula(formula)];
    while let Some(step) = stack.pop() {
        let formula = match step {
            Step::Text(text) => {
                output.push_str(text);
                continue;
            }
            Step::Formula(formula) => formula,
        };
        let (key, parts): (&str, Vec<&Formula>) = match formula {
            Formula::Atom(name) => {
                output.push_str("{\"atom\":");
                write_string(output, name);
                output.push('}');
                continue;
            }
            Formula::Not(inner) => {
                output.push_str("{\"not\":");
                stack.push(Step::Text("}"));
                stack.push(Step::Formula(inner));
                continue;
            }
            Formula::And(parts) => ("and", parts.iter().collect()),
            Formula::Or(parts) => ("or", parts.iter().collect()),
            Formula::Implies(a, b) => ("implies", vec![&**a, &**b]),
            Formula::Iff(a, b) => ("iff", vec![&**a, &**b]),
            Formula::ExactlyOne(parts) => ("exactly_one", parts.iter().collect()),
        };
        let _ = write!(output, "{{\"{key}\":[");
        stack.push(Step::Text("]}"));
        for (index, part) in parts.into_iter().enumerate().rev() {
            stack.push(Step::Formula(part));
            if index > 0 {
                stack.push(Step::Text(","));
            }
        }
    }
}

fn write_option(output: &mut String, text: Option<&str>) {
    match text {
        Some(text) => write_string(output, text),
        None => output.push_str("null"),
    }
}

/// Writes the text as a JSON string, with the escapes serde_json writes: a short escape for `"`,
/// `\`, backspace, form feed, line feed, carriage return and tab, `\u00` and two lowercase hex
/// digits for each other character below U+0020, and every other character as it is.
fn write_string(output: &mut String, text: &str) {
    output.push('"');
    for c in text.chars() {
        match c {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(output, "\\u{:04x}", u32::from(c));
            }
            c => output.push(c),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::{atom, condition, entry, formula, formula_set};
    use crate::logic::{parse, Atom, Condition, Entry, Formula, FormulaSet, Origin, Relation};

    /// The nesting of the deep formulas' tests.
    const DEPTH: usize = 100_000;

    /// Each character a JSON string escapes, beside characters it keeps as they are.
    const ESCAPES: &str = "q\" b\\ \u{8}\u{c}\n\r\t \u{0}\u{1}\u{1f} \u{7f} \u{e9} \u{1f600}";

    fn origin(line: u32) -> Origin {
        Origin {
            requirement: format!("EX-1-{line:03}"),
            file: "rfcs/1-example/01-example.toml".to_owned(),
            line,
        }
    }

    /// A formula set with each kind of formula, an empty `and`, `or` and `exactly_one` among
    /// them, a relation, an atom with each field set and one with none, a condition, and names
    /// that hold each character of [`ESCAPES`].
    fn shallow() -> FormulaSet {
        let texts = [
            "A",
            "!A",
            "A & B & C",
            "A | B",
            "A -> B",
            "A <-> !B",
            "(A | !B) -> C & D",
            "exactly_one(A, !B, C | D)",
        ];
        let mut formulas: Vec<Entry> = texts
            .iter()
            .zip(1..)
            .map(|(text, line)| Entry {
                formula: parse(text).unwrap(),
                origin: origin(line),
                relation: None,
            })
            .collect();
        formulas.push(Entry {
            formula: Formula::And(Vec::new()).iff(Formula::Or(Vec::new())),
            origin: origin(8),
            relation: Some(Relation {
                kind: "depends_on".to_owned(),
                target: "EX-1-002".to_owned(),
            }),
        });
        formulas.push(Entry {
            formula: Formula::ExactlyOne(Vec::new()),
            origin: origin(11),
            relation: None,
        });
        formulas.push(Entry {
            formula: Formula::atom(ESCAPES).not(),
            origin: Origin {
                requirement: ESCAPES.to_owned(),
                file: ESCAPES.to_owned(),
                line: u32::MAX,
            },
            relation: None,
        });
        let atoms = vec![
            Atom {
                id: "EX-1-001".to_owned(),
                rfc: Some(1),
                section: Some("rfcs/1-example/01-example.toml".to_owned()),
                level: Some("MUST".to_owned()),
                polarity: Some("required".to_owned()),
            },
            Atom {
                id: ESCAPES.to_owned(),
                rfc: None,
                section: None,
                level: None,
                polarity: None,
            },
        ];
        let conditions = vec![Condition {
            atom: ESCAPES.to_owned(),
            origin: origin(10),
        }];
        FormulaSet {
            atoms,
            formulas,
            conditions,
        }
    }

    /// The JSON of a shallow formula set, and of each of its parts, is the JSON serde_json
    /// writes from the derived `Serialize`.
    #[test]
    fn shallow_sets_write_the_derived_json() {
        let set = shallow();
        assert_eq!(formula_set(&set), serde_json::to_string(&set).unwrap());
        for item in &set.atoms {
            assert_eq!(atom(item), serde_json::to_string(item).unwrap());
        }
        for item in &set.formulas {
            assert_eq!(entry(item), serde_json::to_string(item).unwrap());
            let derived = serde_json::to_string(&item.formula).unwrap();
            assert_eq!(formula(&item.formula), derived);
        }
        for item in &set.conditions {
            assert_eq!(condition(item), serde_json::to_string(item).unwrap());
        }
        let empty = FormulaSet::default();
        assert_eq!(formula_set(&empty), serde_json::to_string(&empty).unwrap());
        let not = parse("!A").unwrap();
        assert_eq!(formula(&not), "{\"not\":{\"atom\":\"A\"}}");
    }

    /// Writes `deep` alone and as the one formula of a set, and asserts each JSON.
    fn assert_writes(deep: Formula, expected: &str) {
        assert_eq!(formula(&deep), expected);
        let set = FormulaSet {
            atoms: Vec::new(),
            formulas: vec![Entry {
                formula: deep,
                origin: origin(1),
                relation: None,
            }],
            conditions: Vec::new(),
        };
        let place = serde_json::to_string(&origin(1)).unwrap();
        let written = format!("{{\"formula\":{expected},\"origin\":{place},\"relation\":null}}");
        assert_eq!(entry(&set.formulas[0]), written);
        let whole = format!("{{\"atoms\":[],\"formulas\":[{written}],\"conditions\":[]}}");
        assert_eq!(formula_set(&set), whole);
    }

    /// A formula of `DEPTH` nested `!` is written without overflowing the stack.
    #[test]
    fn deep_negation() {
        let deep = parse(&format!("{}A", "!".repeat(DEPTH))).unwrap();
        let (open, close) = ("{\"not\":".repeat(DEPTH), "}".repeat(DEPTH));
        assert_writes(deep, &format!("{open}{{\"atom\":\"A\"}}{close}"));
    }

    /// A formula of `DEPTH` nested parentheses is written without overflowing the stack.
    #[test]
    fn deep_parentheses() {
        let text = format!("{}A{}", "(A & ".repeat(DEPTH), ")".repeat(DEPTH));
        let deep = parse(&text).unwrap();
        let open = "{\"and\":[{\"atom\":\"A\"},".repeat(DEPTH);
        let close = "]}".repeat(DEPTH);
        assert_writes(deep, &format!("{open}{{\"atom\":\"A\"}}{close}"));
    }

    /// A chain of `DEPTH` implications, a variant of two parts, is written without overflowing
    /// the stack.
    #[test]
    fn deep_implications() {
        let deep = parse(&format!("{}A", "A -> ".repeat(DEPTH))).unwrap();
        let open = "{\"implies\":[{\"atom\":\"A\"},".repeat(DEPTH);
        let close = "]}".repeat(DEPTH);
        assert_writes(deep, &format!("{open}{{\"atom\":\"A\"}}{close}"));
    }
}
