use super::{GlobalState, InstrumentIndex};
use crate::events::{AccountEvent, FillRecord, RejectionKind, RejectionRecord};
use crate::types::{OpenOrder, Side};
use rust_decimal::prelude::Signed;
use rust_decimal::Decimal;

impl GlobalState {
    /// Apply account event (balances, orders, fills).
    pub fn apply_account(&mut self, event: &AccountEvent) {
        match event {
            AccountEvent::Balance { asset, free } => {
                self.balances.insert(asset.clone(), *free);
            }
            AccountEvent::BalanceDelta { asset, delta } => {
                self.apply_balance_delta(asset, *delta);
            }
            AccountEvent::OrderUpdate { .. } => self.apply_order_update(event),
            AccountEvent::Fill { .. } => self.apply_fill(event),
            AccountEvent::Rejection(rejection) => self.apply_rejection(rejection),
        }
    }

    fn apply_order_update(&mut self, event: &AccountEvent) {
        let AccountEvent::OrderUpdate {
            id,
            instrument,
            side,
            order_type,
            price,
            stop_price,
            remaining_qty,
            original_qty,
            status,
            client_order_id,
            oco_group,
            strategy_id,
        } = event
        else {
            return;
        };
        self.open_orders.apply_order_update(OpenOrder {
            id: id.clone(),
            instrument: instrument.clone(),
            side: *side,
            order_type: *order_type,
            price: *price,
            stop_price: *stop_price,
            remaining_qty: *remaining_qty,
            original_qty: *original_qty,
            status: *status,
            client_order_id: client_order_id.clone(),
            oco_group: oco_group.clone(),
            strategy_id: strategy_id.clone(),
        });
    }

    fn apply_fill(&mut self, event: &AccountEvent) {
        let AccountEvent::Fill {
            order_id,
            instrument,
            side,
            price,
            qty,
            fee,
            client_order_id,
            oco_group,
            strategy_id,
            ..
        } = event
        else {
            return;
        };
        let Some(ix) = self.registry.index_of(instrument).map(|index| index.0) else {
            return;
        };
        let delta = if *side == Side::Buy { *qty } else { -*qty };
        let current = self.positions[ix];
        if matches!(
            self.registry
                .meta(InstrumentIndex(ix))
                .map(|meta| meta.asset_class),
            Some(crate::instrument::AssetClass::Future | crate::instrument::AssetClass::Perpetual)
        ) {
            self.average_entry_price[ix] =
                next_average_entry(current, self.average_entry_price[ix], delta, *price);
        }
        self.positions[ix] += delta;
        if let Some(strategy_id) = strategy_id {
            *self
                .strategy_positions
                .entry((ix, strategy_id.clone()))
                .or_insert(Decimal::ZERO) += delta;
        }
        self.fill_count += 1;
        if let Some(ts) = self.last_event_ts {
            let contract_multiplier = self
                .registry
                .meta(InstrumentIndex(ix))
                .and_then(|meta| meta.contract_multiplier)
                .map(|value| value.to_string());
            self.fill_log.push(FillRecord {
                ts,
                order_id: order_id.clone(),
                instrument: instrument.clone(),
                side: *side,
                qty: qty.to_string(),
                price: price.to_string(),
                fee: fee.to_string(),
                contract_multiplier,
                client_order_id: client_order_id.clone(),
                oco_group: oco_group.clone(),
                strategy_id: strategy_id.clone(),
            });
        }
    }

    fn apply_rejection(&mut self, rejection: &RejectionRecord) {
        match rejection.kind {
            RejectionKind::Risk => self.risk_rejection_count += 1,
            RejectionKind::Execution => self.execution_rejection_count += 1,
        }
        self.rejection_log.push(rejection.clone());
    }
}

fn next_average_entry(
    current: Decimal,
    average: Option<Decimal>,
    delta: Decimal,
    fill_price: Decimal,
) -> Option<Decimal> {
    let next = current + delta;
    if next.is_zero() {
        return None;
    }
    if current.is_zero() || current.signum() != next.signum() {
        return Some(fill_price);
    }
    if current.signum() != delta.signum() {
        return average;
    }
    let average = average.unwrap_or(fill_price);
    Some((average * current.abs() + fill_price * delta.abs()) / next.abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{InstrumentMeta, InstrumentRegistry};
    use crate::types::{Asset, InstrumentId, OrderId, StrategyId};
    use std::collections::HashMap;

    #[test]
    fn strategy_positions_from_tagged_fills() {
        let instrument = InstrumentId::new("test", "BTCUSDT");
        let instruments = HashMap::from([(
            instrument.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        )]);
        let balances = HashMap::from([(Asset("USDT".into()), Decimal::from(1000u64))]);
        let registry = InstrumentRegistry::from_instruments(instruments);
        let mut state = GlobalState::new(registry, balances);
        let strategy_a = StrategyId::new("momentum");
        let strategy_b = StrategyId::new("mean_rev");
        state.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: instrument.clone(),
            side: Side::Buy,
            price: Decimal::from(50u64),
            qty: Decimal::new(1, 1),
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            client_order_id: None,
            oco_group: None,
            strategy_id: Some(strategy_a.clone()),
        });
        state.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: instrument.clone(),
            side: Side::Sell,
            price: Decimal::from(50u64),
            qty: Decimal::new(5, 2),
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            client_order_id: None,
            oco_group: None,
            strategy_id: Some(strategy_b.clone()),
        });

        assert_eq!(state.position_qty(&instrument), Decimal::new(5, 2));
        assert_eq!(
            state.strategy_position_qty(&instrument, &strategy_a),
            Decimal::new(1, 1)
        );
        assert_eq!(
            state.strategy_position_qty(&instrument, &strategy_b),
            -Decimal::new(5, 2)
        );
    }
}
