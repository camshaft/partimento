//! The solver interface the checks use.
//!
//! The checks see [`Lit`], [`Answer`] and [`Backend`] alone, never a backend's own types, so
//! another solver replaces a backend without a change to any check. Each backend is behind a
//! feature of its own: `batsat`, which the default features select, and `varisat`.

#[cfg(feature = "batsat")]
mod batsat;
#[cfg(feature = "varisat")]
mod varisat;

#[cfg(feature = "batsat")]
pub use self::batsat::Batsat;
#[cfg(feature = "varisat")]
pub use self::varisat::Varisat;

/// The backend the enabled features select: batsat where its feature is on, else varisat.
#[cfg(feature = "batsat")]
pub type DefaultBackend = Batsat;

/// The backend the enabled features select: batsat where its feature is on, else varisat.
#[cfg(all(feature = "varisat", not(feature = "batsat")))]
pub type DefaultBackend = Varisat;

/// A literal: a variable's index, counted from 1, negative for the variable's negation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lit(i32);

impl Lit {
    /// The literal that `value` writes as DIMACS does: a variable's index, counted from 1,
    /// negative for its negation.
    ///
    /// # Panics
    ///
    /// Where `value` is 0 or `i32::MIN`, neither of which is a literal.
    pub fn from_dimacs(value: i32) -> Self {
        assert!(value != 0 && value != i32::MIN, "{value} isn't a literal");
        Lit(value)
    }

    /// The positive literal of the variable whose index, counted from 1, is `var`.
    ///
    /// # Panics
    ///
    /// Where `var` is 0 or doesn't fit in an `i32`.
    pub fn positive(var: u32) -> Self {
        let value = i32::try_from(var).expect("index fits in i32");
        Self::from_dimacs(value)
    }

    /// The literal as DIMACS writes it.
    pub fn to_dimacs(self) -> i32 {
        self.0
    }

    /// The index of the literal's variable, counted from 1.
    pub fn var(self) -> u32 {
        self.0.unsigned_abs()
    }

    /// Whether the literal is the variable itself rather than its negation.
    pub fn is_positive(self) -> bool {
        self.0 > 0
    }
}

impl core::ops::Not for Lit {
    type Output = Lit;

    fn not(self) -> Lit {
        Lit(-self.0)
    }
}

/// A backend's answer to a solve under assumptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The clauses and the assumptions hold together.
    Sat,
    /// They don't: `core` is a subset of the assumptions that the clauses contradict.
    Unsat { core: Vec<Lit> },
}

/// A SAT solver that solves its clauses again under each call's assumptions.
pub trait Backend {
    /// A new variable, as its positive literal.
    fn new_var(&mut self) -> Lit;

    /// Adds a clause over the backend's variables.
    fn add_clause(&mut self, clause: &[Lit]);

    /// Solves the clauses with each assumption true. An `Unsat` core is a subset of
    /// `assumptions`.
    fn solve(&mut self, assumptions: &[Lit]) -> Answer;
}

/// Writes each named test function, generic over the backend, once for each enabled backend.
#[cfg(test)]
macro_rules! backend_tests {
    ($($name:ident),* $(,)?) => {
        #[cfg(feature = "batsat")]
        mod batsat_backend {
            $(
                #[test]
                fn $name() {
                    super::$name::<crate::solver::Batsat>();
                }
            )*
        }

        #[cfg(feature = "varisat")]
        mod varisat_backend {
            $(
                #[test]
                fn $name() {
                    super::$name::<crate::solver::Varisat>();
                }
            )*
        }
    };
}

#[cfg(test)]
pub(crate) use backend_tests;

#[cfg(test)]
mod tests {
    use super::{Answer, Backend, Lit};

    fn core_of(answer: Answer) -> Vec<Lit> {
        match answer {
            Answer::Sat => panic!("expected Unsat"),
            Answer::Unsat { mut core } => {
                core.sort();
                core
            }
        }
    }

    fn satisfiable_without_assumptions<B: Backend + Default>() {
        let mut backend = B::default();
        let a = backend.new_var();
        let b = backend.new_var();
        backend.add_clause(&[a, b]);
        backend.add_clause(&[!a]);
        assert_eq!(backend.solve(&[]), Answer::Sat);
        assert_eq!(backend.solve(&[b]), Answer::Sat);
        assert!(core_of(backend.solve(&[!b])).iter().all(|&lit| lit == !b));
    }

    fn core_is_a_subset_of_the_assumptions<B: Backend + Default>() {
        let mut backend = B::default();
        let x = backend.new_var();
        let y = backend.new_var();
        let z = backend.new_var();
        backend.add_clause(&[!x, !y]);
        assert_eq!(backend.solve(&[x]), Answer::Sat);
        assert_eq!(backend.solve(&[x, z]), Answer::Sat);
        let core = core_of(backend.solve(&[x, y, z]));
        assert!(core.iter().all(|lit| [x, y, z].contains(lit)));
        assert!(core.contains(&x) && core.contains(&y));
        assert!(matches!(backend.solve(&core), Answer::Unsat { .. }));
    }

    fn solves_again_under_new_assumptions<B: Backend + Default>() {
        let mut backend = B::default();
        let x = backend.new_var();
        let y = backend.new_var();
        backend.add_clause(&[!x, y]);
        assert!(matches!(backend.solve(&[x, !y]), Answer::Unsat { .. }));
        assert_eq!(backend.solve(&[x, y]), Answer::Sat);
        assert_eq!(backend.solve(&[!x, !y]), Answer::Sat);
    }

    /// The count of variables of the wide core's test.
    const WIDE: usize = 10_000;

    /// A core that needs every one of `WIDE` assumptions holds each of them once, and none of
    /// the assumptions that the clauses don't need.
    fn a_core_of_every_assumption<B: Backend + Default>() {
        let mut backend = B::default();
        let vars: Vec<Lit> = (0..WIDE).map(|_| backend.new_var()).collect();
        let free = backend.new_var();
        backend.add_clause(&vars);
        let mut assumptions: Vec<Lit> = vars.iter().map(|&var| !var).collect();
        assumptions.push(free);
        let mut expected: Vec<Lit> = vars.iter().map(|&var| !var).collect();
        expected.sort();
        assert_eq!(core_of(backend.solve(&assumptions)), expected);
    }

    super::backend_tests!(
        satisfiable_without_assumptions,
        core_is_a_subset_of_the_assumptions,
        solves_again_under_new_assumptions,
        a_core_of_every_assumption,
    );

    #[test]
    fn literals() {
        let lit = Lit::positive(3);
        assert_eq!(lit.to_dimacs(), 3);
        assert_eq!((!lit).to_dimacs(), -3);
        assert_eq!((!lit).var(), 3);
        assert!(lit.is_positive());
        assert!(!(!lit).is_positive());
        assert_eq!(Lit::from_dimacs(-3), !lit);
    }

    #[test]
    #[should_panic]
    fn zero_isnt_a_literal() {
        Lit::from_dimacs(0);
    }
}
