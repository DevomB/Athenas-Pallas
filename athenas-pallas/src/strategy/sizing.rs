//! Position-sizing helpers shared by in-process Rust strategies.
//!
//! These mirror the `position_size_pct_equity` helpers in the Python (`pallas_strategy.py`) and
//! C++ (`pallas_strategy.hpp`) SDKs so sizing logic is consistent across runtimes.

use rust_decimal::Decimal;

/// Base quantity for `pct` of mark-to-market `equity` at `mid` (spot-style).
///
/// Returns `Decimal::ZERO` for a non-positive `mid` or `pct`. The result is the raw base quantity;
/// the execution layer still applies `lot_size` rounding at submission.
pub fn position_size_pct_equity(equity: Decimal, mid: Decimal, pct: Decimal) -> Decimal {
    if mid <= Decimal::ZERO || pct <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let qty = (equity * pct) / mid;
    if qty > Decimal::ZERO {
        qty
    } else {
        Decimal::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_pct_of_equity() {
        let qty = position_size_pct_equity(
            Decimal::from(10_000),
            Decimal::from(100),
            Decimal::new(1, 1),
        );
        assert_eq!(qty, Decimal::from(10));
    }

    #[test]
    fn non_positive_mid_is_zero() {
        assert_eq!(
            position_size_pct_equity(Decimal::from(10_000), Decimal::ZERO, Decimal::new(1, 1)),
            Decimal::ZERO
        );
    }
}
