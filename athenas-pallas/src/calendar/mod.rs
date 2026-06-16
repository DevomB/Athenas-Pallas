//! Trading session filters for bar replay.

use time::{OffsetDateTime, Weekday};

/// Which session calendar to apply when filtering historical bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionFilter {
    /// No filtering (crypto 24/7 default).
    #[default]
    None,
    /// US equity regular trading hours (Mon–Fri 09:30–16:00 America/New_York, no holidays).
    EquityRth,
    /// FX 24/5 (Sun 17:00 ET open through Fri 17:00 ET close, simplified).
    Forex245,
}

impl SessionFilter {
    /// Parse from config string (`none`, `equity_rth`, `forex_245`).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "equity" | "equity_rth" | "rth" | "nyse" => Self::EquityRth,
            "forex" | "forex_245" | "fx" | "24_5" => Self::Forex245,
            _ => Self::None,
        }
    }
}

/// Whether `ts` falls inside the configured session.
pub fn is_session_open(filter: SessionFilter, ts: OffsetDateTime) -> bool {
    match filter {
        SessionFilter::None => true,
        SessionFilter::EquityRth => equity_rth_open(ts),
        SessionFilter::Forex245 => forex_245_open(ts),
    }
}

/// Filter helper for iterators of timestamped rows.
pub fn filter_bar_timestamp(filter: SessionFilter, ts: OffsetDateTime) -> bool {
    is_session_open(filter, ts)
}

fn equity_rth_open(ts: OffsetDateTime) -> bool {
    let local = ts.to_offset(time::macros::offset!(-5));
    match local.weekday() {
        Weekday::Saturday | Weekday::Sunday => return false,
        _ => {}
    }
    let secs = local.hour() as u32 * 3_600 + local.minute() as u32 * 60 + local.second() as u32;
    let open = 9 * 3_600 + 30 * 60;
    let close = 16 * 3_600;
    secs >= open && secs < close
}

fn forex_245_open(ts: OffsetDateTime) -> bool {
    let local = ts.to_offset(time::macros::offset!(-5));
    match local.weekday() {
        Weekday::Saturday => false,
        Weekday::Sunday => {
            let secs = local.hour() as u32 * 3_600 + local.minute() as u32 * 60;
            secs >= 17 * 3_600
        }
        Weekday::Friday => {
            let secs = local.hour() as u32 * 3_600 + local.minute() as u32 * 60;
            secs < 17 * 3_600
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn equity_closed_on_weekend() {
        let sat = datetime!(2024-01-06 12:00:00 -05:00);
        assert!(!equity_rth_open(sat));
    }

    #[test]
    fn equity_open_midday() {
        let tue = datetime!(2024-01-02 11:00:00 -05:00);
        assert!(equity_rth_open(tue));
    }

    #[test]
    fn forex_closed_saturday() {
        let sat = datetime!(2024-01-06 12:00:00 -05:00);
        assert!(!forex_245_open(sat));
    }
}
