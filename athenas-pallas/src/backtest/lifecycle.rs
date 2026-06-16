//! Scheduled cash flows during replay: perp funding, bond coupons, option exercise.

use rust_decimal::Decimal;
use time::{Date, Month, OffsetDateTime};

use crate::instrument::pricing::{apply_perp_funding, bond_coupon_cash, should_exercise_european};
use crate::instrument::AssetClass;
use crate::instrument::InstrumentIndex;
use crate::instrument::OptionKind;
use crate::state::GlobalState;

/// Default 8h perpetual funding rate when not configured on meta (0.01%).
pub fn default_perp_funding_rate_8h() -> Decimal {
    Decimal::new(1, 4)
}

/// Apply funding, coupons, and expiry exercise after a bar at `ts`.
pub fn apply_bar_lifecycle(state: &mut GlobalState, ts: OffsetDateTime) {
    let n = state.registry.len();
    for ix in 0..n {
        let Some(meta) = state.registry.meta(InstrumentIndex(ix)).cloned() else {
            continue;
        };
        let Some(mid) = state.mid_or_last_ix(ix) else {
            continue;
        };
        let position = state.positions.get(ix).copied().unwrap_or(Decimal::ZERO);
        match meta.asset_class {
            AssetClass::Perpetual if !position.is_zero() => {
                let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
                let notional = mid * position.abs() * mult;
                let funding =
                    apply_perp_funding(position, notional, default_perp_funding_rate_8h());
                if !funding.is_zero() {
                    state.apply_balance_delta(&meta.quote, funding);
                }
            }
            AssetClass::Bond => {
                if is_coupon_date(ts, meta.coupon_payments_per_year.unwrap_or(2)) {
                    let face = meta.face_value.unwrap_or(Decimal::from(1000u64));
                    let rate = meta.coupon_rate.unwrap_or(Decimal::new(5, 2));
                    let ppy = meta.coupon_payments_per_year.unwrap_or(2);
                    let coupon = bond_coupon_cash(rate, face, ppy);
                    let held = state
                        .balances
                        .get(&meta.base)
                        .copied()
                        .unwrap_or(Decimal::ZERO);
                    if held > Decimal::ZERO {
                        state.apply_balance_delta(&meta.quote, coupon * held);
                    }
                }
            }
            AssetClass::Option
                if option_expired_on(ts, meta.expiry.as_deref()) && !position.is_zero() =>
            {
                let strike = meta.face_value.unwrap_or(Decimal::ONE);
                let call_itm = should_exercise_european(OptionKind::Call, mid, strike);
                let put_itm = should_exercise_european(OptionKind::Put, mid, strike);
                let intrinsic = if call_itm {
                    (mid - strike).max(Decimal::ZERO)
                } else if put_itm {
                    (strike - mid).max(Decimal::ZERO)
                } else {
                    Decimal::ZERO
                };
                if !intrinsic.is_zero() {
                    let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
                    let payout = intrinsic * position.abs() * mult;
                    state.apply_balance_delta(&meta.quote, payout);
                    state.positions[ix] = Decimal::ZERO;
                    state.balances.insert(meta.base.clone(), Decimal::ZERO);
                }
            }
            _ => {}
        }
    }
}

fn is_coupon_date(ts: OffsetDateTime, payments_per_year: u32) -> bool {
    if payments_per_year == 0 {
        return false;
    }
    let month = ts.month();
    let day = ts.day();
    match payments_per_year {
        2 => matches!(month, Month::June | Month::December) && day == 1,
        4 => {
            matches!(
                month,
                Month::March | Month::June | Month::September | Month::December
            ) && day == 1
        }
        12 => day == 1,
        _ => day == 1,
    }
}

fn option_expired_on(ts: OffsetDateTime, expiry: Option<&str>) -> bool {
    let Some(exp) = expiry else {
        return false;
    };
    let fmt = time::format_description::parse("[year]-[month]-[day]").ok();
    let Some(fmt) = fmt else {
        return false;
    };
    Date::parse(exp, &fmt).ok().is_some_and(|d| ts.date() >= d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{InstrumentMeta, InstrumentRegistry};
    use crate::types::Asset;
    use std::collections::HashMap;

    #[test]
    fn bond_coupon_credits_quote_on_june_first() {
        let mut map = HashMap::new();
        let id = crate::types::InstrumentId::new("treasury", "UST10Y");
        map.insert(
            id.clone(),
            InstrumentMeta::bond(
                "UST10Y",
                "USD",
                Decimal::from(1000u64),
                Decimal::new(5, 2),
                2,
                None,
            ),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USD".into()), Decimal::from(10_000u64));
        bal.insert(Asset("UST10Y".into()), Decimal::ONE);
        let reg = InstrumentRegistry::from_instruments(map);
        let mut state = GlobalState::new(reg, bal);
        state.l1[0] = Some((
            time::macros::datetime!(2024-06-01 00:00:00 UTC),
            Decimal::from(99u64),
            Decimal::from(101u64),
        ));
        state.bar_close[0] = Some(Decimal::from(100u64));
        apply_bar_lifecycle(&mut state, time::macros::datetime!(2024-06-01 00:00:00 UTC));
        let usd = state.balances.get(&Asset("USD".into())).copied().unwrap();
        assert!(usd > Decimal::from(10_000u64));
    }
}
