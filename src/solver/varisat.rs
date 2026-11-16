//! The varisat backend, behind the `varisat` feature.

use std::collections::HashSet;

use super::{Answer, Backend, Lit};

use ::varisat::ExtendFormula;

/// A [`Backend`] over varisat's solver.
#[derive(Default)]
pub struct Varisat {
    solver: ::varisat::Solver<'static>,
}

fn lit(lit: Lit) -> ::varisat::Lit {
    ::varisat::Lit::from_dimacs(lit.to_dimacs() as isize)
}

impl Backend for Varisat {
    fn new_var(&mut self) -> Lit {
        let var = self.solver.new_var();
        let value = i32::try_from(var.to_dimacs()).expect("index fits in i32");
        Lit::from_dimacs(value)
    }

    fn add_clause(&mut self, clause: &[Lit]) {
        let lits: Vec<::varisat::Lit> = clause.iter().map(|&each| lit(each)).collect();
        self.solver.add_clause(&lits);
    }

    fn solve(&mut self, assumptions: &[Lit]) -> Answer {
        let lits: Vec<::varisat::Lit> = assumptions.iter().map(|&each| lit(each)).collect();
        self.solver.assume(&lits);
        match self.solver.solve() {
            Ok(true) => Answer::Sat,
            Ok(false) => {
                // The core's variables, read into a set once, so that the filter looks each
                // assumption up once: a search of the core for each assumption costs the
                // assumptions times the core, n squared where the core holds every assumption.
                let failed: HashSet<isize> = self
                    .solver
                    .failed_core()
                    .unwrap_or(&[])
                    .iter()
                    .map(|f| f.var().to_dimacs())
                    .collect();
                let core = assumptions
                    .iter()
                    .copied()
                    .filter(|&each| failed.contains(&lit(each).var().to_dimacs()))
                    .collect();
                Answer::Unsat { core }
            }
            Err(error) => panic!("varisat failed: {error}"),
        }
    }
}
