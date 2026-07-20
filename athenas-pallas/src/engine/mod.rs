//! Engine: sync dispatch for backtest replay.

mod replay;
mod sync;

pub use replay::replay_events_sync;
pub(crate) use sync::{
    collect_replay_bar_intents_sync, collect_replay_event_intents_sync, finalize_strategy_sync,
    poll_replay_market_instrument_sync, process_pending_intents_for_instrument_sync,
};
pub use sync::{dispatch_event_sync, dispatch_replay_sync, dispatch_strategy_sync};
