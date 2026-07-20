//! Dense instrument index used by state vectors.

/// Row index into per-instrument vectors.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentIndex(pub usize);
