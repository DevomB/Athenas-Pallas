//! Shared paths for integration tests (fixtures are not part of the public `data/` workspace).

use std::path::PathBuf;

/// Path to a file under `tests/fixtures/data/`.
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/data")
        .join(name)
}
