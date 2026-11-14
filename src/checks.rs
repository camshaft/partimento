//! The checks that need a solver: check 1, that the formulas of a specification hold together,
//! and check 2, that each condition of a requirement can hold while they do.
//!
//! Both checks read the formula set through [`Encoding`] and a [`Backend`] alone, so they run
//! the same under each backend.

use std::collections::BTreeSet;

use crate::logic::{Encoding, FormulaSet};
use crate::solver::{Answer, Backend, Lit};

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
        for (index, condition) in set.conditions.iter().enumerate() {
            let atom = encoding
                .atom_lit(&condition.atom)
                .expect("the encoding holds each condition's atom");
            assumptions.push(atom);
            if let Answer::Unsat { .. } = encoding.solve(&assumptions) {
                vacuous.push(index);
            }
            assumptions.pop();
        }
    }
    Consistency { conflict, vacuous }
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
    use super::{consistency, Consistency};
    use crate::logic::{parse, Condition, Entry, FormulaSet, Origin};
    use crate::solver::{Answer, Backend, Lit};

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

    fn empty_set<B: Backend + Default>() {
        assert_eq!(
            consistency::<B>(&FormulaSet::default()),
            Consistency::default()
        );
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

    crate::solver::backend_tests!(
        empty_set,
        consistent_set,
        conflict_is_minimal,
        conflict_is_one_of_two,
        minimal_from_a_wide_core,
        minimal_from_a_wide_core_of_three,
        vacuous_condition,
        condition_vacuous_through_exactly_one,
    );
}
