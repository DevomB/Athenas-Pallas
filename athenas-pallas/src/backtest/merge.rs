//! Merge multiple historical sources by event timestamp.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::events::Event;

use super::HistoricalSource;

fn event_ts(ev: &Event) -> time::OffsetDateTime {
    // Non-timestamped events sort to the front of the merge rather than reading wall-clock time.
    ev.timestamp().unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
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
pub struct MergedSources<'a> {
    sources: &'a mut [Box<dyn HistoricalSource>],
    heap: BinaryHeap<HeapItem>,
}

impl<'a> MergedSources<'a> {
    /// Build a streaming merger with one pending event per source.
    pub fn new(sources: &'a mut [Box<dyn HistoricalSource>]) -> Self {
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
        Self { sources, heap }
    }
}

impl Iterator for MergedSources<'_> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.heap.pop()?;
        if let Some(ev) = self.sources[item.source_ix].next_event() {
            let ts = event_ts(&ev);
            self.heap.push(HeapItem {
                ts,
                source_ix: item.source_ix,
                event: ev,
            });
        }
        Some(item.event)
    }
}

/// Stream events from multiple sources ordered by timestamp.
pub fn merge_sources_iter(sources: &mut [Box<dyn HistoricalSource>]) -> MergedSources<'_> {
    MergedSources::new(sources)
}

/// K-way merge events from multiple sources ordered by timestamp.
pub fn merge_sources(sources: &mut [Box<dyn HistoricalSource>]) -> Vec<Event> {
    merge_sources_iter(sources).collect()
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
            ExchangeId::new("test"),
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

    #[test]
    fn merge_iterator_streams_in_order() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let a = CsvBarSource::from_path(
            &base.join("BTCUSDT_1d.csv"),
            ExchangeId::new("test"),
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
        let mut prev = None;
        let mut count = 0usize;
        for ev in merge_sources_iter(&mut sources) {
            let ts = event_ts(&ev);
            if let Some(prev) = prev {
                assert!(prev <= ts);
            }
            prev = Some(ts);
            count += 1;
        }
        assert!(count > 0);
    }
}
