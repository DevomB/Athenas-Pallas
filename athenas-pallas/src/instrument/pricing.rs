//! Instrument-specific sizing, margin, and simple payoff helpers.

use rust_decimal::prelude::Signed;
use rust_decimal::Decimal;

use super::registry::{AssetClass, InstrumentMeta};

/// Option call vs put.
#[derive(
    Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OptionKind {
    /// Call.
    Call,
    /// Put.
    Put,
}

/// Maintenance margin is modeled as this fraction of initial margin when not separately configured.
///
/// Exchanges typically set maintenance below initial (e.g. ~50–60%); 0.5 is a conservative default
/// used for the replay liquidation check.
pub fn maintenance_margin_fraction() -> Decimal {
    Decimal::new(5, 1) // 0.5
}

/// Notional exposure of `qty` at `price` (contract-multiplier aware for derivatives).
pub fn position_notional(meta: &InstrumentMeta, price: Decimal, qty: Decimal) -> Decimal {
    match meta.asset_class {
        AssetClass::Future | AssetClass::Perpetual | AssetClass::Option => {
            let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
            price * qty.abs() * mult
        }
        _ => price * qty.abs(),
    }
}

/// Initial margin required to open or hold `qty` at `price`.
pub fn margin_required(meta: &InstrumentMeta, price: Decimal, qty: Decimal) -> Decimal {
    let rate = meta.margin_initial_rate.unwrap_or(Decimal::ONE);
    position_notional(meta, price, qty) * rate
}

/// Maintenance margin required to keep `qty` open at `price`.
///
/// Returns [`MAINTENANCE_MARGIN_FRACTION`] of the initial requirement. A position whose
/// mark-to-market equity falls below this is eligible for liquidation during replay.
pub fn maintenance_margin_required(meta: &InstrumentMeta, price: Decimal, qty: Decimal) -> Decimal {
    margin_required(meta, price, qty) * maintenance_margin_fraction()
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
    fn maintenance_is_half_of_initial() {
        let meta = InstrumentMeta::perpetual(
            Asset::new("BTC"),
            Asset::new("USDT"),
            Some(Decimal::ONE),
            Some(Decimal::new(1, 1)),
        );
        let initial = margin_required(&meta, Decimal::from(50_000u64), Decimal::ONE);
        let maint = maintenance_margin_required(&meta, Decimal::from(50_000u64), Decimal::ONE);
        assert_eq!(maint, initial * Decimal::new(5, 1));
        assert_eq!(maint, Decimal::from(2_500u64));
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
