//! JSON line protocol for external strategies.

#![allow(missing_docs)]

use rust_decimal::Decimal;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::events::{Event, FillRecord, OrderIntent, RejectionRecord};

use crate::types::{ClientOrderId, InstrumentId, OpenOrder, OrderId, StrategyId};

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct InitMsg {
    pub msg: String,

    pub protocol_version: u32,

    pub instruments: Vec<InstrumentInfo>,

    pub balances: HashMap<String, String>,

    pub config: HashMap<String, String>,

    pub parameters: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct InstrumentInfo {
    pub exchange: String,

    pub symbol: String,

    pub base: String,

    pub quote: String,

    pub asset_class: String,

    pub lot_size: Option<String>,

    pub tick_size: Option<String>,

    pub contract_multiplier: Option<String>,

    pub expiry: Option<String>,

    pub margin_initial_rate: Option<String>,

    pub option_kind: Option<String>,

    pub option_exercise_style: Option<String>,

    pub option_strike: Option<String>,

    pub option_underlying: Option<InstrumentId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct ReadyMsg {
    pub msg: String,

    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]

pub struct StrategySnapshot {
    pub position_qty: String,

    pub mid: Option<String>,

    pub equity: String,

    pub balances: HashMap<String, String>,

    pub instruments: Vec<InstrumentSnapshot>,

    pub pending_orders: Vec<OpenOrder>,

    pub fills: Vec<FillRecord>,

    pub rejections: Vec<RejectionRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstrumentSnapshot {
    pub instrument: InstrumentId,

    pub position_qty: String,

    pub mid: Option<String>,
}

#[derive(Clone, Debug, Serialize)]

pub struct EventMsg {
    pub msg: String,

    pub seq: u64,

    pub event: Event,

    pub ctx: StrategySnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct IntentJson {
    pub instrument: InstrumentId,

    pub side: crate::types::Side,

    pub order_type: crate::types::OrderType,

    pub qty: String,

    pub price: Option<String>,

    #[serde(default)]
    pub stop_price: Option<String>,

    #[serde(default)]
    pub strategy_id: Option<String>,

    #[serde(default)]
    pub client_order_id: Option<String>,

    #[serde(default)]
    pub oco_group: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct IntentsMsg {
    pub msg: String,

    pub seq: u64,

    pub intents: Vec<IntentJson>,

    #[serde(default)]
    pub cancel_order_ids: Vec<OrderId>,

    #[serde(default)]
    pub cancel_client_order_ids: Vec<String>,

    #[serde(default)]
    pub cancel_all: bool,

    #[serde(default)]
    pub flatten: bool,

    #[serde(default)]
    pub diagnostics: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FinishMsg {
    pub msg: String,

    pub seq: u64,

    pub ctx: StrategySnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct ShutdownMsg {
    pub msg: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct ErrorMsg {
    pub msg: String,

    pub detail: String,
}

pub fn snapshot_from(
    state: &crate::state::GlobalState,

    instrument: &InstrumentId,

    fills_from: usize,

    rejections_from: usize,
) -> StrategySnapshot {
    let mut balances = HashMap::new();

    for (a, v) in &state.balances {
        balances.insert(a.0.to_string(), v.to_string());
    }

    StrategySnapshot {
        position_qty: state.position_qty(instrument).to_string(),

        mid: state.mid_or_last(instrument).map(|d| d.to_string()),

        equity: state
            .mark_to_market_equity(instrument)
            .unwrap_or(Decimal::ZERO)
            .to_string(),

        balances,

        instruments: state
            .registry
            .iter()
            .map(|(index, instrument, _)| InstrumentSnapshot {
                instrument: instrument.clone(),
                position_qty: state.positions[index.0].to_string(),
                mid: state.mid_or_last_ix(index.0).map(|value| value.to_string()),
            })
            .collect(),

        pending_orders: state.open_orders.working_orders(),

        fills: state.fill_log[fills_from.min(state.fill_log.len())..].to_vec(),

        rejections: state.rejection_log[rejections_from.min(state.rejection_log.len())..].to_vec(),
    }
}

pub fn intents_to_orders(intents: Vec<IntentJson>) -> crate::Result<Vec<OrderIntent>> {
    intents.into_iter().map(intent_to_order).collect()
}

fn intent_to_order(intent: IntentJson) -> crate::Result<OrderIntent> {
    let qty: Decimal = intent
        .qty
        .parse()
        .map_err(|_| crate::error::Error::Invalid(format!("bad qty {}", intent.qty)))?;

    if qty <= Decimal::ZERO {
        return Err(crate::error::Error::Invalid("qty must be positive".into()));
    }

    let price = parse_optional_decimal(intent.price.as_deref(), "price")?;
    let stop_price = parse_optional_decimal(intent.stop_price.as_deref(), "stop_price")?;

    Ok(OrderIntent {
        instrument: intent.instrument,
        side: intent.side,
        order_type: intent.order_type,
        price,
        stop_price,
        qty,
        client_order_id: intent.client_order_id.map(ClientOrderId),
        oco_group: intent.oco_group,
        source: crate::events::OrderIntentSource::User,
        strategy_id: intent.strategy_id.map(StrategyId::new),
    })
}

fn parse_optional_decimal(value: Option<&str>, name: &str) -> crate::Result<Option<Decimal>> {
    value
        .map(|raw| {
            raw.parse()
                .map_err(|_| crate::error::Error::Invalid(format!("bad {name} {raw}")))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::Side;

    #[test]

    fn ready_roundtrip() {
        let j = serde_json::to_string(&ReadyMsg {
            msg: "ready".into(),
            capabilities: Vec::new(),
        })
        .unwrap();

        let _: ReadyMsg = serde_json::from_str(&j).unwrap();
    }

    #[test]

    fn intent_parses_stop_and_strategy_id() {
        let j = IntentJson {
            instrument: InstrumentId::new("test", "BTCUSDT"),

            side: Side::Buy,

            order_type: crate::types::OrderType::StopMarket,

            qty: "1".into(),

            price: None,

            stop_price: Some("50000".into()),

            strategy_id: Some("sleeve_a".into()),

            client_order_id: Some("cid-1".into()),

            oco_group: Some("entry-bracket".into()),
        };

        let orders = intents_to_orders(vec![j]).unwrap();

        assert_eq!(orders[0].stop_price, Some(Decimal::from(50_000u64)));

        assert_eq!(
            orders[0].strategy_id.as_ref().map(|s| s.0.as_str()),
            Some("sleeve_a")
        );

        assert_eq!(
            orders[0].client_order_id.as_ref().map(|c| c.0.as_str()),
            Some("cid-1")
        );

        assert_eq!(orders[0].oco_group.as_deref(), Some("entry-bracket"));
    }
}
