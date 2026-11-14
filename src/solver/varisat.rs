//! The varisat backend, behind the `varisat` feature.

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
                let failed = self.solver.failed_core().unwrap_or(&[]);
                let core = assumptions
                    .iter()
                    .copied()
                    .filter(|&each| failed.iter().any(|f| f.var() == lit(each).var()))
                    .collect();
                Answer::Unsat { core }
            }
            Err(error) => panic!("varisat failed: {error}"),
        }
    }
}
