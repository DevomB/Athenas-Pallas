//! Merge multiple historical sources by event timestamp.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::events::Event;

use super::HistoricalSource;

fn event_ts(ev: &Event) -> time::OffsetDateTime {
    match ev {
        Event::Market(crate::events::MarketEvent::Trade { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL1 { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::Bar { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL2Snapshot(s)) => s.ts,
        Event::Timer(t) => t.ts,
        _ => time::OffsetDateTime::UNIX_EPOCH,
    }
}

struct HeapItem {
    ts: time::OffsetDateTime,
    source_ix: usize,
    event: Event,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.ts == other.ts && self.source_ix == other.source_ix
    }
}

impl Eq for HeapItem {}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .ts
            .cmp(&self.ts)
            .then_with(|| other.source_ix.cmp(&self.source_ix))
    }
}

/// K-way merge events from multiple sources ordered by timestamp.
pub fn merge_sources(sources: &mut [Box<dyn HistoricalSource>]) -> Vec<Event> {
    let mut heap = BinaryHeap::new();
    for (ix, src) in sources.iter_mut().enumerate() {
        if let Some(ev) = src.next_event() {
            let ts = event_ts(&ev);
            heap.push(HeapItem {
                ts,
                source_ix: ix,
                event: ev,
            });
        }
    }
    let mut out = Vec::new();
    while let Some(item) = heap.pop() {
        out.push(item.event);
        if let Some(ev) = sources[item.source_ix].next_event() {
            let ts = event_ts(&ev);
            heap.push(HeapItem {
                ts,
                source_ix: item.source_ix,
                event: ev,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::sources::YahooCsvSource;
    use crate::backtest::CsvBarSource;
    use crate::events::MarketEvent;
    use crate::types::{ExchangeId, Symbol};
    use std::path::PathBuf;

    #[test]
    fn merge_two_csvs_by_ts() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let a = CsvBarSource::from_path(
            &base.join("BTCUSDT_1d.csv"),
            ExchangeId::new("binance"),
            Symbol::new("BTCUSDT"),
        )
        .unwrap();
        let b = YahooCsvSource::from_path(
            &base.join("AAPL_1d.csv"),
            ExchangeId::new("yahoo"),
            Symbol::new("AAPL"),
        )
        .unwrap();
        let mut sources: Vec<Box<dyn HistoricalSource>> = vec![Box::new(a), Box::new(b)];
        let merged = merge_sources(&mut sources);
        assert!(!merged.is_empty());
        for w in merged.windows(2) {
            let t0 = event_ts(&w[0]);
            let t1 = event_ts(&w[1]);
            assert!(t0 <= t1);
        }
        assert!(merged
            .iter()
            .any(|e| matches!(e, Event::Market(MarketEvent::Bar { .. }))));
    }
}
