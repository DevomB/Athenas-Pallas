//! Instrument-specific sizing, margin, and simple payoff helpers.

use rust_decimal::prelude::Signed;
use rust_decimal::Decimal;

use super::kind::OptionKind;
use super::registry::{AssetClass, InstrumentMeta};

/// Initial margin required to open or hold `qty` at `price`.
pub fn margin_required(meta: &InstrumentMeta, price: Decimal, qty: Decimal) -> Decimal {
    let notional = match meta.asset_class {
        AssetClass::Future | AssetClass::Perpetual | AssetClass::Option => {
            let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
            price * qty.abs() * mult
        }
        _ => price * qty.abs(),
    };
    let rate = meta.margin_initial_rate.unwrap_or(Decimal::ONE);
    notional * rate
}

/// Cash coupon payment per period for a bond.
pub fn bond_coupon_cash(coupon_rate: Decimal, face: Decimal, payments_per_year: u32) -> Decimal {
    if payments_per_year == 0 {
        return Decimal::ZERO;
    }
    let n = Decimal::from(payments_per_year);
    face * coupon_rate / n
}

/// Funding PnL applied to a perpetual position (signed position, notional, rate).
pub fn apply_perp_funding(position: Decimal, notional: Decimal, rate: Decimal) -> Decimal {
    if position.is_zero() || notional.is_zero() {
        return Decimal::ZERO;
    }
    -position.signum() * notional * rate
}

/// Intrinsic value of a European option at expiry (or for exercise check).
pub fn option_intrinsic_value(kind: OptionKind, spot: Decimal, strike: Decimal) -> Decimal {
    match kind {
        OptionKind::Call => (spot - strike).max(Decimal::ZERO),
        OptionKind::Put => (strike - spot).max(Decimal::ZERO),
    }
}

/// True when a European option should be exercised at expiry.
pub fn should_exercise_european(kind: OptionKind, spot: Decimal, strike: Decimal) -> bool {
    !option_intrinsic_value(kind, spot, strike).is_zero()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::asset::Asset;

    #[test]
    fn margin_scales_with_rate() {
        let meta = InstrumentMeta::perpetual(
            Asset::new("BTC"),
            Asset::new("USDT"),
            Some(Decimal::ONE),
            Some(Decimal::new(1, 1)),
        );
        let req = margin_required(&meta, Decimal::from(50_000u64), Decimal::ONE);
        assert_eq!(req, Decimal::from(5_000u64));
    }

    #[test]
    fn bond_coupon_semiannual() {
        let cash = bond_coupon_cash(Decimal::new(5, 2), Decimal::from(1000u64), 2);
        assert_eq!(cash, Decimal::from(25u64));
    }

    #[test]
    fn call_intrinsic() {
        assert_eq!(
            option_intrinsic_value(
                OptionKind::Call,
                Decimal::from(110u64),
                Decimal::from(100u64)
            ),
            Decimal::from(10u64)
        );
        assert!(should_exercise_european(
            OptionKind::Call,
            Decimal::from(110u64),
            Decimal::from(100u64)
        ));
    }
}
