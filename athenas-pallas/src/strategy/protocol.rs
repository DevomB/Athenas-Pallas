//! JSON line protocol for external strategies.

#![allow(missing_docs)]

use rust_decimal::Decimal;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::events::{Event, OrderIntent};

use crate::types::{ClientOrderId, InstrumentId, StrategyId};

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct InitMsg {
    pub msg: String,

    pub instruments: Vec<InstrumentInfo>,

    pub balances: HashMap<String, String>,

    pub config: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct InstrumentInfo {
    pub exchange: String,

    pub symbol: String,

    pub base: String,

    pub quote: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct ReadyMsg {
    pub msg: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct StrategySnapshot {
    pub position_qty: String,

    pub mid: Option<String>,

    pub equity: String,

    pub balances: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct IntentsMsg {
    pub msg: String,

    pub seq: u64,

    pub intents: Vec<IntentJson>,
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
    }
}

pub fn intents_to_orders(intents: Vec<IntentJson>) -> crate::Result<Vec<OrderIntent>> {
    let mut out = Vec::with_capacity(intents.len());

    for i in intents {
        let qty: Decimal = i
            .qty
            .parse()
            .map_err(|_| crate::error::Error::Invalid(format!("bad qty {}", i.qty)))?;

        if qty <= Decimal::ZERO {
            return Err(crate::error::Error::Invalid("qty must be positive".into()));
        }

        let price = match i.price {
            Some(ref p) => Some(
                p.parse()
                    .map_err(|_| crate::error::Error::Invalid(format!("bad price {p}")))?,
            ),

            None => None,
        };

        let stop_price = match i.stop_price {
            Some(ref p) => Some(
                p.parse()
                    .map_err(|_| crate::error::Error::Invalid(format!("bad stop_price {p}")))?,
            ),

            None => None,
        };

        out.push(OrderIntent {
            instrument: i.instrument,

            side: i.side,

            order_type: i.order_type,

            price,

            stop_price,

            qty,

            client_order_id: i.client_order_id.map(ClientOrderId),

            source: crate::events::OrderIntentSource::User,

            strategy_id: i.strategy_id.map(StrategyId::new),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::Side;

    #[test]

    fn ready_roundtrip() {
        let j = serde_json::to_string(&ReadyMsg {
            msg: "ready".into(),
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
    }
}
