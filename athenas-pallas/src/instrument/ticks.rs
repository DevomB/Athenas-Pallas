//! Integer fixed-point price/quantity newtypes for exact, allocation-free arithmetic.
//!
//! Prices and quantities on real venues live on discrete grids: a price is an integer number of
//! `tick_size` increments, a quantity an integer number of `lot_size` increments. Representing them
//! as `i64` tick/lot counts lets the hot path do exact integer math (no `Decimal` 96-bit multiply,
//! no rounding drift) and recover the `Decimal` value only at the boundary.
//!
//! These types are deliberately additive: the proven `Decimal` fill/fee pipeline is unchanged, and
//! every conversion is covered by round-trip and Decimal-equivalence tests (see the module tests
//! and `tests/`). Adopt them incrementally in fill/fee code where a hot loop dominates.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

/// A price expressed as a signed integer number of `tick_size` increments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PriceTicks(pub i64);

/// A quantity expressed as a signed integer number of `lot_size` increments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QtyLots(pub i64);

/// Quantize `value` onto the `step` grid, returning the (possibly rounded) integer step count.
///
/// Rounds to nearest using `Decimal::round` (half-to-even / banker's rounding). Returns `None`
/// when `step` is not strictly positive or the result does not fit in `i64`.
fn to_steps(value: Decimal, step: Decimal) -> Option<i64> {
    if step <= Decimal::ZERO {
        return None;
    }
    (value / step).round().to_i64()
}

impl PriceTicks {
    /// Quantize a decimal price onto the `tick_size` grid (nearest tick).
    pub fn from_decimal(price: Decimal, tick_size: Decimal) -> Option<Self> {
        to_steps(price, tick_size).map(Self)
    }

    /// Recover the decimal price for this tick count: `ticks * tick_size`.
    pub fn to_decimal(self, tick_size: Decimal) -> Decimal {
        Decimal::from(self.0) * tick_size
    }

    /// True when `price` lies exactly on the `tick_size` grid (no rounding occurred).
    pub fn is_exact(price: Decimal, tick_size: Decimal) -> bool {
        match Self::from_decimal(price, tick_size) {
            Some(t) => t.to_decimal(tick_size) == price,
            None => false,
        }
    }
}

impl QtyLots {
    /// Quantize a decimal quantity onto the `lot_size` grid (nearest lot).
    pub fn from_decimal(qty: Decimal, lot_size: Decimal) -> Option<Self> {
        to_steps(qty, lot_size).map(Self)
    }

    /// Recover the decimal quantity for this lot count: `lots * lot_size`.
    pub fn to_decimal(self, lot_size: Decimal) -> Decimal {
        Decimal::from(self.0) * lot_size
    }

    /// True when `qty` lies exactly on the `lot_size` grid (no rounding occurred).
    pub fn is_exact(qty: Decimal, lot_size: Decimal) -> bool {
        match Self::from_decimal(qty, lot_size) {
            Some(q) => q.to_decimal(lot_size) == qty,
            None => false,
        }
    }
}

/// Exact notional of `price x qty` computed in integer tick/lot space.
///
/// Returns `price_ticks * qty_lots` as `i128` (the dimensionless step product). Multiply by
/// `tick_size * lot_size` to recover the quote-currency notional; see [`notional_decimal`]. The
/// `i128` accumulator cannot overflow for any `i64 x i64` product.
pub fn notional_steps(price: PriceTicks, qty: QtyLots) -> i128 {
    (price.0 as i128) * (qty.0 as i128)
}

/// Quote-currency notional of `price x qty`, derived from the integer step product.
///
/// `notional_steps(price, qty) * tick_size * lot_size`. When `price`/`qty` were produced by exact
/// (non-rounding) conversions this equals `price_decimal * qty_decimal` exactly.
pub fn notional_decimal(
    price: PriceTicks,
    qty: QtyLots,
    tick_size: Decimal,
    lot_size: Decimal,
) -> Decimal {
    Decimal::from_i128_with_scale(notional_steps(price, qty), 0) * tick_size * lot_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_round_trip_exact() {
        let tick = Decimal::new(5, 2); // 0.05
        let price = Decimal::new(10025, 2); // 100.25
        assert!(PriceTicks::is_exact(price, tick));
        let pt = PriceTicks::from_decimal(price, tick).unwrap();
        assert_eq!(pt, PriceTicks(2005));
        assert_eq!(pt.to_decimal(tick), price);
    }

    #[test]
    fn qty_round_trip_exact() {
        let lot = Decimal::new(1, 3); // 0.001
        let qty = Decimal::new(2500, 3); // 2.500
        assert!(QtyLots::is_exact(qty, lot));
        let ql = QtyLots::from_decimal(qty, lot).unwrap();
        assert_eq!(ql, QtyLots(2500));
        assert_eq!(ql.to_decimal(lot), qty);
    }

    #[test]
    fn off_grid_price_is_not_exact_but_rounds() {
        let tick = Decimal::new(5, 2); // 0.05
        let price = Decimal::new(10026, 2); // 100.26 -> nearest tick 100.25
        assert!(!PriceTicks::is_exact(price, tick));
        let pt = PriceTicks::from_decimal(price, tick).unwrap();
        assert_eq!(pt, PriceTicks(2005));
    }

    #[test]
    fn integer_notional_matches_decimal_product() {
        // 100.25 x 3 = 300.75, computed via integer ticks/lots.
        let tick = Decimal::new(5, 2);
        let lot = Decimal::ONE;
        let price = Decimal::new(10025, 2);
        let qty = Decimal::from(3u64);
        let pt = PriceTicks::from_decimal(price, tick).unwrap();
        let ql = QtyLots::from_decimal(qty, lot).unwrap();
        assert_eq!(notional_steps(pt, ql), 6015);
        assert_eq!(
            notional_decimal(pt, ql, tick, lot),
            price * qty // 300.75
        );
    }

    #[test]
    fn negative_quantity_for_shorts() {
        let lot = Decimal::ONE;
        let qty = Decimal::from(-4i64);
        let ql = QtyLots::from_decimal(qty, lot).unwrap();
        assert_eq!(ql, QtyLots(-4));
        assert_eq!(ql.to_decimal(lot), qty);
    }

    #[test]
    fn rejects_non_positive_step() {
        assert!(PriceTicks::from_decimal(Decimal::ONE, Decimal::ZERO).is_none());
        assert!(QtyLots::from_decimal(Decimal::ONE, Decimal::new(-1, 0)).is_none());
    }
}
