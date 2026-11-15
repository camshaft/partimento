//! The time each step of a check of a wide composition takes, printed in each CI run's log: a
//! requirement that composes 10,000, 30,000 and then 100,000 parts under `exactly_one`, as
//! src/checks.rs's `wide_composition` test builds it, under each enabled solver backend. For each
//! size it prints the time of the check, of checks 1 and 2 alone over the check's formula set, of
//! building the composition's two formulas over every part, of the encoding of the formula set,
//! of reading each part's literal from the encoding, and of each of the five solves that
//! `wide_composition` makes, with the time of building its assumptions apart from the solve. The
//! test asserts each solve's answer; it doesn't bound a time.
//!
//! The test runs without the test harness, so its output shows in the log as it is printed.

use std::time::{Duration, Instant};

use partimento::config::Config;
use partimento::logic::{Encoding, Formula};
use partimento::model::{Block, Field, Rfc, Section, Set};
use partimento::solver::{Answer, Backend, Lit};
use partimento::{ByteVec, Tree};
use toml::Value;

/// The numbers of parts the test measures, the smallest first.
const SIZES: [usize; 3] = [10_000, 30_000, 100_000];

/// The configuration of src/checks.rs's tests: the prefix `EX`, three levels, and one system of
/// RFCs 1 to 99.
const SPEC: &str = r#"prefix = "EX"
levels = ["MUST", "SHOULD", "MAY"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = "requirements"

[[system]]
name = "core"
first = 1
last = 99
"#;

/// The path of the section file that holds the composition.
const GREETING: &str = "rfcs/0001-greetings/01-greeting.toml";

/// A measure under one backend.
type Measure = fn(&str, &Config, usize);

/// Each enabled backend by its name, with the measure of its steps.
#[allow(clippy::vec_init_then_push)]
fn backends() -> Vec<(&'static str, Measure)> {
    let mut backends: Vec<(&'static str, Measure)> = Vec::new();
    #[cfg(feature = "batsat")]
    backends.push(("batsat", steps::<partimento::solver::Batsat>));
    #[cfg(feature = "varisat")]
    backends.push(("varisat", steps::<partimento::solver::Varisat>));
    backends
}

fn config() -> Config {
    let mut tree = Tree::default();
    let bytes = ByteVec::from(SPEC.to_owned());
    tree.files
        .insert(partimento::config::FILE.to_owned(), bytes);
    let mut findings = Vec::new();
    let config = partimento::config::load(&tree, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    config.expect("the configuration loads")
}

/// The identifier of RFC 0001's requirement `n`.
fn id(n: usize) -> String {
    format!("EX-0001-{n:03}")
}

fn field(key: &str, line: u32, value: Value) -> Field {
    Field {
        key: key.to_owned(),
        line,
        value,
    }
}

/// A set whose section file holds EX-0001-001, which composes `parts` requirements under
/// `exactly_one`, and the parts, EX-0001-002 on, each a requirement that holds only its `id`.
fn composed(parts: usize) -> Set {
    let names: Vec<Value> = (2..parts + 2).map(|n| Value::String(id(n))).collect();
    let mut requirements = vec![Block {
        line: 10,
        fields: vec![
            field("id", 10, Value::String(id(1))),
            field("compose", 14, Value::String("exactly_one".to_owned())),
            field("parts", 15, Value::Array(names)),
        ],
    }];
    let at = |n: usize| u32::try_from(20 + 10 * n).expect("a line of the file");
    for n in 0..parts {
        let fields = vec![field("id", at(n), Value::String(id(n + 2)))];
        requirements.push(Block {
            line: at(n),
            fields,
        });
    }
    let section = Section {
        file: GREETING.to_owned(),
        line: 1,
        fields: Vec::new(),
        requirements,
        kinds: Vec::new(),
    };
    let rfc = Rfc {
        dir: "rfcs/0001-greetings".to_owned(),
        meta_file: "rfcs/0001-greetings/rfc.toml".to_owned(),
        number: 1,
        meta: None,
        sections: vec![section],
    };
    Set {
        index: None,
        rfcs: vec![rfc],
    }
}

/// The duration in milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Prints the time of each step under the backend `B` for a composition of `parts` parts.
fn steps<B: Backend + Default>(backend: &str, config: &Config, parts: usize) {
    let print = |step: &str, duration: Duration| {
        let ms = millis(duration);
        println!("wide composition of {parts} parts under {backend}: {step}: {ms:.1} ms");
    };
    let set = composed(parts);

    let start = Instant::now();
    let mut findings = Vec::new();
    let formula_set = partimento::checks::check::<B>(config, &set, &mut findings);
    print("check", start.elapsed());
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(formula_set.formulas.len(), 3);

    let start = Instant::now();
    let consistency = partimento::checks::consistency::<B>(&formula_set);
    print(
        "checks 1 and 2 over the check's formula set",
        start.elapsed(),
    );
    assert!(consistency.conflict.is_none());

    let names: Vec<String> = (2..parts + 2).map(id).collect();
    let own = id(1);
    let start = Instant::now();
    let atoms = || names.iter().map(Formula::atom).collect::<Vec<_>>();
    let iff = Formula::atom(&own).iff(Formula::at_least_one(atoms()));
    let exclusion = Formula::atom(&own).implies(Formula::exactly_one(atoms()));
    print("the composition's two formulas, built", start.elapsed());

    let start = Instant::now();
    let formulas = &formula_set.formulas;
    let same = formulas[1].formula == iff && formulas[2].formula == exclusion;
    print(
        "the two formulas, compared with the check's",
        start.elapsed(),
    );
    assert!(same);

    let start = Instant::now();
    let mut encoding = Encoding::new(B::default(), &formula_set);
    print("encoding", start.elapsed());
    let selectors = encoding.selectors().to_vec();

    let start = Instant::now();
    let lits: Vec<Lit> = names
        .iter()
        .map(|name| encoding.atom_lit(name).expect("an atom of the set"))
        .collect();
    let own_lit = encoding.atom_lit(&own).expect("an atom of the set");
    print("each part's literal, read", start.elapsed());

    let middle = parts / 2;
    let cases: [(&str, usize, &[usize], bool, bool); 5] = [
        ("solve 1, one part true", 0, &[middle], false, true),
        ("solve 2, two parts true", 0, &[0, parts - 1], false, false),
        ("solve 3, no part true", 0, &[], false, false),
        (
            "solve 4, no part and not the requirement",
            1,
            &[],
            true,
            true,
        ),
        (
            "solve 5, one part and not the requirement",
            1,
            &[middle],
            true,
            false,
        ),
    ];
    for (step, from, true_at, own_false, sat) in cases {
        let start = Instant::now();
        let mut assumptions = selectors[from..].to_vec();
        for (at, &lit) in lits.iter().enumerate() {
            assumptions.push(if true_at.contains(&at) { lit } else { !lit });
        }
        if own_false {
            assumptions.push(!own_lit);
        }
        print(&format!("{step}: its assumptions, built"), start.elapsed());
        let start = Instant::now();
        let answer = encoding.solve(&assumptions);
        print(step, start.elapsed());
        assert_eq!(matches!(answer, Answer::Sat), sat, "{step}");
    }
}

fn main() {
    let config = config();
    for (backend, measure) in backends() {
        for parts in SIZES {
            measure(backend, &config, parts);
        }
    }
}
