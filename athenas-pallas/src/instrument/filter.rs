//! Instrument filters for system commands.

use crate::instrument::registry::LegacyInstrumentId;

/// Filter for cancel/close commands (barter `InstrumentFilter`).
#[derive(Clone, Debug, Default)]
pub enum InstrumentFilter {
    /// All instruments (barter `None`).
    #[default]
    None,
    /// All instruments (alias).
    All,
    /// Single instrument.
    One(LegacyInstrumentId),
}

impl InstrumentFilter {
    /// True if `id` matches this filter.
    pub fn matches(&self, id: &LegacyInstrumentId) -> bool {
        match self {
            InstrumentFilter::None | InstrumentFilter::All => true,
            InstrumentFilter::One(one) => one == id,
        }
    }
}
