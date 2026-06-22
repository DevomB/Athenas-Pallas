//! Alternate CSV layouts.

mod csv_common;
mod future;
mod fx;
mod yahoo;

pub use csv_common::{headers_are_yahoo, parse_yahoo_csv, read_yahoo_rows, YahooRow};
pub use future::FutureCsvSource;
pub use fx::FxCsvSource;
pub use yahoo::YahooCsvSource;
