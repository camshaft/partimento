//! The batsat backend, behind the `batsat` feature.

use super::{Answer, Backend, Lit};

use ::batsat::{lbool, BasicSolver, SolverInterface};

/// A [`Backend`] over batsat's solver.
#[derive(Default)]
pub struct Batsat {
    solver: BasicSolver,
    vars: Vec<::batsat::Var>,
}

impl Batsat {
    fn lit(&self, lit: Lit) -> ::batsat::Lit {
        let var = self.vars[lit.var() as usize - 1];
        ::batsat::Lit::new(var, lit.is_positive())
    }
}

impl Backend for Batsat {
    fn new_var(&mut self) -> Lit {
        self.vars.push(self.solver.new_var_default());
        Lit::positive(self.vars.len() as u32)
    }

    fn add_clause(&mut self, clause: &[Lit]) {
        let mut lits: Vec<::batsat::Lit> = clause.iter().map(|&lit| self.lit(lit)).collect();
        // A false answer says the clauses contradict each other with no assumption; the next solve
        // answers Unsat with an empty core.
        let _ = self.solver.add_clause_reuse(&mut lits);
    }

    fn solve(&mut self, assumptions: &[Lit]) -> Answer {
        let lits: Vec<::batsat::Lit> = assumptions.iter().map(|&lit| self.lit(lit)).collect();
        let result = self.solver.solve_limited(&lits);
        if result == lbool::TRUE {
            return Answer::Sat;
        }
        assert!(result == lbool::FALSE, "batsat answered undefined");
        // batsat writes its core as a clause, the assumptions negated, so read it by variable.
        let core = assumptions
            .iter()
            .copied()
            .filter(|&lit| {
                let var = self.lit(lit).var();
                self.solver.unsat_core_contains_var(var)
            })
            .collect();
        Answer::Unsat { core }
    }
}
