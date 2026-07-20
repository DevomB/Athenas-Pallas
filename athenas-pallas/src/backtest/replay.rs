//! Sync replay of recorded [`crate::events::Event`] streams.

use std::io::{BufRead, BufReader, Read};

use crate::error::Result;
use crate::events::Event;

/// Parse one JSON object per line into engine events (blank lines skipped).
pub fn read_events_jsonl(r: impl Read) -> Result<Vec<Event>> {
    let mut out = Vec::new();
    for line in BufReader::new(r).lines() {
        let line = line.map_err(|e| crate::error::Error::Invalid(e.to_string()))?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let ev: Event = serde_json::from_str(t).map_err(crate::error::Error::from)?;
        out.push(ev);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::read_events_jsonl;
    use crate::events::{Event, TimerEvent};
    use time::OffsetDateTime;

    #[test]
    fn jsonl_round_trip_timer() {
        let ts = OffsetDateTime::UNIX_EPOCH;
        let ev = Event::Timer(TimerEvent { ts, id: 9 });
        let line = serde_json::to_string(&ev).unwrap();
        let parsed = read_events_jsonl(line.as_bytes()).unwrap();
        assert_eq!(parsed.len(), 1);
    }
}
