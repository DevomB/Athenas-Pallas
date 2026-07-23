//! Scheduled cash flows during replay: perp funding, bond coupons, option exercise.

use rust_decimal::Decimal;
use time::{Date, Month, OffsetDateTime};

use crate::instrument::pricing::{
    apply_perp_funding, bond_coupon_cash, maintenance_margin_required, should_exercise_european,
};
use crate::instrument::AssetClass;
use crate::instrument::InstrumentMeta;
use crate::state::GlobalState;

/// Default 8h perpetual funding rate when not configured on meta (0.01%).
pub fn default_perp_funding_rate_8h() -> Decimal {
    Decimal::new(1, 4)
}

/// Perpetual funding settles on the standard 00:00 / 08:00 / 16:00 UTC schedule.
///
/// A bar is a funding boundary when its timestamp lands exactly on one of those hours. Daily
/// (00:00) bars therefore settle once per day; intraday bars hit all three windows. This replaces
/// the previous (incorrect) every-bar accrual, which over-charged high-frequency replays.
fn is_funding_time(ts: OffsetDateTime) -> bool {
    matches!(ts.hour(), 0 | 8 | 16) && ts.minute() == 0 && ts.second() == 0
}

/// Apply funding, coupons, and expiry exercise after a bar at `ts`.
pub fn apply_bar_lifecycle(state: &mut GlobalState, ts: OffsetDateTime) {
    let instruments: Vec<_> = state
        .registry
        .iter()
        .map(|(ix, _, meta)| (ix.0, meta.clone()))
        .collect();
    for (ix, meta) in instruments {
        let Some(mid) = state.mid_or_last_ix(ix) else {
            continue;
        };
        let position = state.positions.get(ix).copied().unwrap_or(Decimal::ZERO);
        if should_liquidate(state, ix, &meta, mid, position) {
            liquidate_position(state, ix, &meta, mid);
            continue;
        }
        apply_cash_flow(state, ts, ix, &meta, mid, position);
    }
}

fn should_liquidate(
    state: &GlobalState,
    ix: usize,
    meta: &InstrumentMeta,
    mid: Decimal,
    position: Decimal,
) -> bool {
    if !matches!(meta.asset_class, AssetClass::Future | AssetClass::Perpetual)
        || position.is_zero()
        || meta
            .margin_initial_rate
            .is_none_or(|rate| rate >= Decimal::ONE)
    {
        return false;
    }
    state
        .mark_to_market_equity_ix(ix)
        .is_some_and(|equity| equity < maintenance_margin_required(meta, mid, position))
}

fn apply_cash_flow(
    state: &mut GlobalState,
    ts: OffsetDateTime,
    ix: usize,
    meta: &InstrumentMeta,
    mid: Decimal,
    position: Decimal,
) {
    match meta.asset_class {
        AssetClass::Perpetual if !position.is_zero() && is_funding_time(ts) => {
            settle_funding(state, meta, mid, position);
        }
        AssetClass::Bond if is_coupon_date(ts, meta.coupon_payments_per_year.unwrap_or(2)) => {
            settle_coupon(state, meta);
        }
        AssetClass::Option
            if option_expired_on(ts, meta.expiry.as_deref()) && !position.is_zero() =>
        {
            if let Some(underlying_mid) = meta
                .option_underlying
                .as_ref()
                .and_then(|underlying| state.mid_or_last(underlying))
            {
                settle_option(state, ix, meta, underlying_mid, position);
            }
        }
        _ => {}
    }
}

fn settle_funding(state: &mut GlobalState, meta: &InstrumentMeta, mid: Decimal, position: Decimal) {
    let multiplier = meta.contract_multiplier.unwrap_or(Decimal::ONE);
    let notional = mid * position.abs() * multiplier;
    let funding = apply_perp_funding(position, notional, default_perp_funding_rate_8h());
    if !funding.is_zero() {
        state.apply_balance_delta(&meta.quote, funding);
    }
}

fn settle_coupon(state: &mut GlobalState, meta: &InstrumentMeta) {
    let face = meta.face_value.unwrap_or(Decimal::from(1000u64));
    let rate = meta.coupon_rate.unwrap_or(Decimal::new(5, 2));
    let payments = meta.coupon_payments_per_year.unwrap_or(2);
    let held = state
        .balances
        .get(&meta.base)
        .copied()
        .unwrap_or(Decimal::ZERO);
    if held > Decimal::ZERO {
        state.apply_balance_delta(&meta.quote, bond_coupon_cash(rate, face, payments) * held);
    }
}

fn settle_option(
    state: &mut GlobalState,
    ix: usize,
    meta: &InstrumentMeta,
    underlying_mid: Decimal,
    position: Decimal,
) {
    let (Some(kind), Some(strike)) = (meta.option_kind, meta.option_strike) else {
        return;
    };
    let intrinsic = if should_exercise_european(kind, underlying_mid, strike) {
        crate::instrument::option_intrinsic_value(kind, underlying_mid, strike)
    } else {
        Decimal::ZERO
    };
    if !intrinsic.is_zero() {
        let multiplier = meta.contract_multiplier.unwrap_or(Decimal::ONE);
        state.apply_balance_delta(&meta.quote, intrinsic * position * multiplier);
    }
    state.positions[ix] = Decimal::ZERO;
    state.balances.insert(meta.base.clone(), Decimal::ZERO);
}

/// Close a leveraged derivative position at `mid`, realizing its mark-to-market exposure into the
/// quote balance and zeroing the base/position rows. Mirrors the bookkeeping used for option
/// exercise so equity is conserved at the instant of liquidation.
fn liquidate_position(state: &mut GlobalState, ix: usize, meta: &InstrumentMeta, mid: Decimal) {
    let position = state.positions[ix];
    let entry = state.average_entry_price[ix].unwrap_or(mid);
    let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
    let realized = (mid - entry) * position * mult;
    if !realized.is_zero() {
        state.apply_balance_delta(&meta.quote, realized);
    }
    state.balances.insert(meta.base.clone(), Decimal::ZERO);
    if let Some(p) = state.positions.get_mut(ix) {
        *p = Decimal::ZERO;
    }
    state.average_entry_price[ix] = None;
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
    let fmt = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").ok();
    let Some(fmt) = fmt else {
        return false;
    };
    Date::parse(exp, &fmt).ok().is_some_and(|d| ts.date() >= d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{InstrumentMeta, InstrumentRegistry, OptionContractMeta, OptionKind};
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

    fn option_state(
        underlying_mid: Decimal,
        legs: &[(&str, OptionKind, u64, Decimal)],
    ) -> GlobalState {
        let underlying = crate::types::InstrumentId::new("test", "SPY");
        let mut map = HashMap::from([(underlying.clone(), InstrumentMeta::spot("SPY", "USD"))]);
        for (symbol, kind, strike, _) in legs {
            map.insert(
                crate::types::InstrumentId::new("test", *symbol),
                InstrumentMeta::option_meta(
                    *symbol,
                    "USD",
                    OptionContractMeta {
                        contract_multiplier: Decimal::from(100u64),
                        tick_size: Decimal::new(1, 2),
                        margin_initial_rate: None,
                        expiry: "2024-06-03".into(),
                        kind: *kind,
                        strike: Decimal::from(*strike),
                        underlying: underlying.clone(),
                    },
                ),
            );
        }
        let mut balances = HashMap::from([(Asset::new("USD"), Decimal::from(10_000u64))]);
        let registry = InstrumentRegistry::from_instruments(map);
        let mut state = GlobalState::new(registry, balances.clone());
        let underlying_ix = state.registry.index_of(&underlying).unwrap().0;
        state.l1[underlying_ix] = Some((
            time::macros::datetime!(2024-06-03 20:00:00 UTC),
            underlying_mid,
            underlying_mid,
        ));
        for (symbol, _, _, qty) in legs {
            let instrument = crate::types::InstrumentId::new("test", *symbol);
            let ix = state.registry.index_of(&instrument).unwrap().0;
            state.positions[ix] = *qty;
            state.l1[ix] = Some((
                time::macros::datetime!(2024-06-03 20:00:00 UTC),
                Decimal::ONE,
                Decimal::ONE,
            ));
            balances.insert(Asset::new(*symbol), *qty);
        }
        state.balances = balances;
        state
    }

    #[test]
    fn call_and_put_settle_against_linked_underlying() {
        let mut state = option_state(
            Decimal::from(120u64),
            &[
                ("CALL100", OptionKind::Call, 100, Decimal::ONE),
                ("PUT130", OptionKind::Put, 130, Decimal::ONE),
            ],
        );
        apply_bar_lifecycle(&mut state, time::macros::datetime!(2024-06-03 20:00:00 UTC));
        assert_eq!(
            state.balances.get(&Asset::new("USD")),
            Some(&Decimal::from(13_000u64))
        );
        assert!(state.positions.iter().all(Decimal::is_zero));
    }

    #[test]
    fn covered_call_and_vertical_spread_have_signed_cashflows() {
        let mut state = option_state(
            Decimal::from(120u64),
            &[
                ("CALL100", OptionKind::Call, 100, Decimal::ONE),
                ("CALL110", OptionKind::Call, 110, Decimal::NEGATIVE_ONE),
            ],
        );
        state
            .balances
            .insert(Asset::new("SPY"), Decimal::from(100u64));
        apply_bar_lifecycle(&mut state, time::macros::datetime!(2024-06-03 20:00:00 UTC));
        assert_eq!(
            state.balances.get(&Asset::new("USD")),
            Some(&Decimal::from(11_000u64))
        );
        assert_eq!(
            state.balances.get(&Asset::new("SPY")),
            Some(&Decimal::from(100u64))
        );
    }

    fn perp_state(qty: Decimal, usd: Decimal, mid: Decimal) -> (GlobalState, usize) {
        let mut map = HashMap::new();
        let id = crate::types::InstrumentId::new("test", "BTCUSDT");
        map.insert(
            id.clone(),
            InstrumentMeta::perpetual("BTC", "USD", None, Some(Decimal::new(1, 1))), // 10% initial
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USD".into()), usd);
        bal.insert(Asset("BTC".into()), qty);
        let reg = InstrumentRegistry::from_instruments(map);
        let mut state = GlobalState::new(reg, bal);
        state.positions[0] = qty;
        if !qty.is_zero() {
            state.average_entry_price[0] = Some(Decimal::from(100u64));
        }
        state.l1[0] = Some((
            time::macros::datetime!(2024-06-03 08:00:00 UTC),
            mid - Decimal::ONE,
            mid + Decimal::ONE,
        ));
        (state, 0)
    }

    #[test]
    fn perp_funding_only_settles_on_8h_boundary() {
        let (mut off_state, _) =
            perp_state(Decimal::ONE, Decimal::from(1_000u64), Decimal::from(100u64));
        // 09:30 is not a funding boundary -> quote untouched.
        apply_bar_lifecycle(
            &mut off_state,
            time::macros::datetime!(2024-06-03 09:30:00 UTC),
        );
        let usd_off = off_state
            .balances
            .get(&Asset("USD".into()))
            .copied()
            .unwrap();
        assert_eq!(usd_off, Decimal::from(1_000u64));

        let (mut on_state, _) =
            perp_state(Decimal::ONE, Decimal::from(1_000u64), Decimal::from(100u64));
        // 08:00 UTC is a funding boundary -> quote moves by funding.
        apply_bar_lifecycle(
            &mut on_state,
            time::macros::datetime!(2024-06-03 08:00:00 UTC),
        );
        let usd_on = on_state
            .balances
            .get(&Asset("USD".into()))
            .copied()
            .unwrap();
        assert_ne!(usd_on, Decimal::from(1_000u64));
    }

    #[test]
    fn underwater_short_is_liquidated() {
        // Short from 100, only $50 cash, mid 200 -> equity -50 < maintenance 10 -> flatten.
        let (mut state, ix) = perp_state(
            Decimal::NEGATIVE_ONE,
            Decimal::from(50u64),
            Decimal::from(200u64),
        );
        apply_bar_lifecycle(&mut state, time::macros::datetime!(2024-06-03 09:30:00 UTC));
        assert_eq!(state.positions[ix], Decimal::ZERO);
        assert_eq!(
            state.balances.get(&Asset("BTC".into())).copied().unwrap(),
            Decimal::ZERO
        );
    }

    #[test]
    fn healthy_position_is_not_liquidated() {
        // Long 1, $1000 cash, mid 100 -> equity 1100, well above maintenance -> untouched.
        let (mut state, ix) =
            perp_state(Decimal::ONE, Decimal::from(1_000u64), Decimal::from(100u64));
        apply_bar_lifecycle(&mut state, time::macros::datetime!(2024-06-03 09:30:00 UTC));
        assert_eq!(state.positions[ix], Decimal::ONE);
    }
}
