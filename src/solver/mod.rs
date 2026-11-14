//! The solver backends of the consistency check.

/// A solver backend of the consistency check.
pub trait Backend {}

/// The backend that runs the format rules alone, which `check_default` uses until the solver
/// backends of the consistency check exist.
#[derive(Clone, Copy, Debug, Default)]
pub struct FormatOnly;

impl Backend for FormatOnly {}
