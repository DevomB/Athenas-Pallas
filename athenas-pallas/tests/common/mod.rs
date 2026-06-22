//! Shared helpers for integration tests.

mod fixtures;

#[allow(unused_imports)]
pub use fixtures::{
    crypto_fixture_instrument, crypto_fixture_meta, fixture_path, test_balances, test_instrument,
    test_instrument_meta,
};
/// Path to a file under `tests/fixtures/data/`.
pub fn fixture(name: &str) -> std::path::PathBuf {
    fixture_path(name)
}
