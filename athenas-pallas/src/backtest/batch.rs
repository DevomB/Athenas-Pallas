//! Parallel replay of multiple synthetic or recorded scenarios.

use crate::dispatch_event_sync;
use crate::error::Result;
use crate::events::Event;
use crate::metrics::{summarize, PerformanceSummary};
use crate::risk::RiskPipeline;
use crate::state::GlobalState;
use crate::strategy::Strategy;
use crate::types::{EquityPoint, InstrumentId};
use time::OffsetDateTime;

/// Named replay bundle.
#[derive(Clone, Debug)]
pub struct Scenario {
    /// Label for reporting.
    pub name: String,
    /// Deterministic event stream.
    pub events: Vec<Event>,
    /// Optional RNG seed for strategies that use one (reporting only).
    pub seed: Option<u64>,
}

/// Aggregated performance for one scenario run.
#[derive(Clone, Debug)]
pub struct RunReport {
    /// Scenario name.
    pub name: String,
    /// Metrics summary from mark-to-market equity samples.
    pub summary: PerformanceSummary,
    /// Echo of [`Scenario::seed`] when provided.
    pub seed: Option<u64>,
}

fn equity_ts(ev: &Event) -> OffsetDateTime {
    ev.timestamp_or_now()
}

/// Deterministic single-threaded batch (preserves event order within each scenario).
pub fn run_scenarios_serial<S, E, B>(
    scenarios: Vec<Scenario>,
    build: &B,
    risk: &RiskPipeline,
    exec: &E,
    equity_instrument: InstrumentId,
    periods_per_year: f64,
) -> Result<Vec<RunReport>>
where
    B: Fn(&Scenario) -> (GlobalState, S),
    S: Strategy,
    E: crate::execution::SyncExecutionGateway,
{
    let mut out = Vec::new();
    let mut intents = Vec::new();
    for sc in scenarios {
        let (mut state, mut strat) = build(&sc);
        let mut curve: Vec<EquityPoint> = Vec::new();
        for ev in sc.events {
            let ts = equity_ts(&ev);
            dispatch_event_sync(&mut state, &mut strat, risk, exec, ev, &mut intents)?;
            if let Some(eq) = state.mark_to_market_equity(&equity_instrument) {
                curve.push(EquityPoint {
                    ts,
                    equity_quote: eq,
                });
            }
        }
        let summary = summarize(curve, periods_per_year);
        out.push(RunReport {
            name: sc.name,
            summary,
            seed: sc.seed,
        });
    }
    Ok(out)
}

/// Parallel replay using rayon (sync hot path).
pub fn run_scenarios_parallel_sync<S, E, B>(
    scenarios: Vec<Scenario>,
    build: &B,
    risk: &RiskPipeline,
    exec: &E,
    equity_instrument: InstrumentId,
    periods_per_year: f64,
) -> Vec<RunReport>
where
    B: Fn(&Scenario) -> (GlobalState, S) + Sync,
    S: Strategy + Send,
    E: crate::execution::SyncExecutionGateway + Sync,
{
    use rayon::prelude::*;
    let mut out: Vec<RunReport> = scenarios
        .into_par_iter()
        .map(|sc| {
            let (mut state, mut strat) = build(&sc);
            let mut curve: Vec<EquityPoint> = Vec::new();
            let mut intents = Vec::new();
            for ev in sc.events {
                let ts = equity_ts(&ev);
                let _ = dispatch_event_sync(&mut state, &mut strat, risk, exec, ev, &mut intents);
                if let Some(eq) = state.mark_to_market_equity(&equity_instrument) {
                    curve.push(EquityPoint {
                        ts,
                        equity_quote: eq,
                    });
                }
            }
            let summary = summarize(curve, periods_per_year);
            RunReport {
                name: sc.name,
                summary,
                seed: sc.seed,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
