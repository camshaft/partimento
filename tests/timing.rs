//! The time a check of a generated set of 2,500 requirements takes under each enabled solver
//! backend, printed in each CI run's log. The test asserts that the check passes and that its
//! formula set holds each requirement as an atom; it doesn't bound the time.
//!
//! The test runs without the test harness, so its output shows in the log as it is printed.

use std::time::Instant;

use partimento::{ByteVec, Options, Report, Tree};

/// The RFCs of the generated set.
const RFCS: usize = 25;
/// The section files of each RFC.
const PARTS: usize = 4;
/// The requirements of each section file.
const REQUIREMENTS: usize = 25;

/// The configuration the fixtures under tests/fixtures hold.
const SPEC: &str = r#"prefix = "EX"
levels = ["MUST", "SHOULD", "MAY"]
implementation_terms = ["sqlite", "message queue"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = ".duvet/requirements"

[[system]]
name = "core"
first = 1
last = 99

[contracts]
name = "contracts"
first = 100
last = 199
"#;

/// A check of a tree under one backend.
type Check = fn(&Tree, &Options) -> Report;

/// Each enabled backend by its name.
fn backends() -> Vec<(&'static str, Check)> {
    let mut backends: Vec<(&'static str, Check)> = Vec::new();
    #[cfg(feature = "batsat")]
    {
        use partimento::solver::Batsat;
        backends.push(("batsat", partimento::check::<Batsat>));
    }
    #[cfg(feature = "varisat")]
    {
        use partimento::solver::Varisat;
        backends.push(("varisat", partimento::check::<Varisat>));
    }
    backends
}

/// The identifier of requirement `n` of the RFC.
fn id(rfc: usize, n: usize) -> String {
    format!("EX-{rfc:04}-{n:03}")
}

/// A relation of a requirement block, as an inline table.
fn relation(kind: &str, target: String) -> String {
    format!("{{ kind = \"{kind}\", target = \"{target}\" }}")
}

/// Requirement `k` of the section file `part` of the RFC. Each requirement but the first of its
/// section depends on the one before it, and some of them imply, hold a condition, a `logic`
/// formula or a composition of two parts, so the set holds each kind of formula.
fn requirement(rfc: usize, part: usize, k: usize) -> String {
    let n = (part - 1) * REQUIREMENTS + k;
    let mut lines = vec![
        String::new(),
        "[[requirement]]".to_string(),
        format!("id = \"{}\"", id(rfc, n)),
        "level = \"MUST\"".to_string(),
        "polarity = \"required\"".to_string(),
        format!("statement = \"The program MUST record event {n} of part {part}.\""),
        "binds_to = \"system\"".to_string(),
        "rationale = \"The tests need this requirement.\"".to_string(),
    ];
    let mut relations = Vec::new();
    if k > 1 {
        relations.push(relation("depends_on", id(rfc, n - 1)));
    }
    if k == 7 || k == 17 {
        relations.push(relation("implies", id(rfc, n - 2)));
    }
    if !relations.is_empty() {
        lines.push(format!("relations = [{}]", relations.join(", ")));
    }
    if k.is_multiple_of(5) {
        lines.push(format!("conditions = [\"{}\"]", id(rfc, n - 1)));
    }
    if k == 3 || k == 23 {
        lines.push(format!(
            "logic = [\"{} -> {}\"]",
            id(rfc, n),
            id(rfc, n - 1)
        ));
    }
    if k == 24 {
        lines.push("compose = \"any_of\"".to_string());
        lines.push(format!(
            "parts = [\"{}\", \"{}\"]",
            id(rfc, n - 3),
            id(rfc, n - 2)
        ));
    }
    lines.join("\n") + "\n"
}

/// The section file `part` of the RFC, with its requirements.
fn section(rfc: usize, part: usize) -> String {
    let mut text = format!(
        "[section]\nanchor = \"part-{rfc}-{part}\"\ntitle = \"Part {part}\"\n\
         problem = \"The program loses the events of part {part}.\"\n\
         motivation = \"A record of each event keeps it.\"\n"
    );
    for k in 1..=REQUIREMENTS {
        text.push_str(&requirement(rfc, part, k));
    }
    text
}

/// The generated set, with the files that `partimento render` generates from it.
fn generated() -> Tree {
    let mut files = vec![("spec.toml".to_string(), SPEC.to_string())];
    let mut index = String::new();
    for rfc in 1..=RFCS {
        let entry = format!(
            "[[rfc]]\nnumber = {rfc}\ntitle = \"Generated {rfc}\"\n\
             area = \"core\"\n\n"
        );
        index.push_str(&entry);
        let dir = format!("rfcs/{rfc:04}-generated");
        let meta = format!(
            "number = {rfc}\ntitle = \"Generated {rfc}\"\narea = \"core\"\n\
             abstract = \"How the program records the events of RFC {rfc}.\"\nretired = []\n"
        );
        files.push((format!("{dir}/rfc.toml"), meta));
        for part in 1..=PARTS {
            files.push((format!("{dir}/{part:02}-part.toml"), section(rfc, part)));
        }
    }
    files.push(("rfcs/index.toml".to_string(), index));
    let mut tree = Tree::default();
    for (path, text) in files {
        tree.files.insert(path, ByteVec::from(text));
    }
    match partimento::render(&tree) {
        Ok(generated) => tree.files.extend(generated.files),
        Err(findings) => panic!("the generated set doesn't render: {findings:?}"),
    }
    tree
}

fn main() {
    let tree = generated();
    let count = RFCS * PARTS * REQUIREMENTS;
    for (backend, check) in backends() {
        let start = Instant::now();
        let report = check(&tree, &Options::default());
        let millis = start.elapsed().as_secs_f64() * 1000.0;
        for finding in &report.findings {
            eprintln!("{finding}");
        }
        assert!(
            report.findings.is_empty(),
            "the check finds nothing in the set"
        );
        assert_eq!(
            report.formulas.atoms.len(),
            count,
            "each requirement is an atom"
        );
        println!("check of {count} generated requirements under {backend}: {millis:.1} ms");
    }
}
