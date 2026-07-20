//! Historical event source contract.

use crate::events::Event;

/// Produces timestamped events in replay order.
pub trait HistoricalSource: Send {
    /// Return the next event, or `None` at end of input.
    fn next_event(&mut self) -> Option<Event>;
}
