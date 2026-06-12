//! Alternate CSV layouts.

mod future;
mod fx;
mod yahoo;

pub use future::FutureCsvSource;
pub use fx::FxCsvSource;
pub use yahoo::YahooCsvSource;
