//! Trading session filters for bar replay.

use time::{Date, Month, OffsetDateTime, Weekday};

/// Which session calendar to apply when filtering historical bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionFilter {
    /// No filtering (crypto 24/7 default).
    #[default]
    None,
    /// US equity regular trading hours (Mon–Fri 09:30–16:00 America/New_York, NYSE holidays closed).
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
    if is_us_market_holiday(local.date()) {
        return false;
    }
    let secs = local.hour() as u32 * 3_600 + local.minute() as u32 * 60 + local.second() as u32;
    let open = 9 * 3_600 + 30 * 60;
    let close = 16 * 3_600;
    secs >= open && secs < close
}

/// True when `date` is a full NYSE market holiday (regular-session close).
///
/// Covers the standard recurring closures with weekend "observed" shifts: New Year's Day, MLK Day,
/// Washington's Birthday, Good Friday, Memorial Day, Juneteenth (2022+), Independence Day, Labor
/// Day, Thanksgiving, and Christmas. One-off closures (e.g. national days of mourning) are not
/// modeled.
pub fn is_us_market_holiday(date: Date) -> bool {
    let year = date.year();

    let fixed_observed = [
        (Month::January, 1u8),
        (Month::July, 4),
        (Month::December, 25),
    ];
    for (m, d) in fixed_observed {
        if let Some(h) = Date::from_calendar_date(year, m, d).ok().map(observed) {
            if h == date {
                return true;
            }
        }
    }

    // Juneteenth became a federal/market holiday in 2022.
    if year >= 2022 {
        if let Some(h) = Date::from_calendar_date(year, Month::June, 19)
            .ok()
            .map(observed)
        {
            if h == date {
                return true;
            }
        }
    }

    let floating = [
        nth_weekday(year, Month::January, Weekday::Monday, 3), // MLK Day
        nth_weekday(year, Month::February, Weekday::Monday, 3), // Washington's Birthday
        last_weekday(year, Month::May, Weekday::Monday),       // Memorial Day
        nth_weekday(year, Month::September, Weekday::Monday, 1), // Labor Day
        nth_weekday(year, Month::November, Weekday::Thursday, 4), // Thanksgiving
        good_friday(year),
    ];
    floating.into_iter().flatten().any(|h| h == date)
}

/// Shift a holiday landing on a weekend to the observed weekday (Sat -> Fri, Sun -> Mon).
fn observed(date: Date) -> Date {
    match date.weekday() {
        Weekday::Saturday => shift_days(date, -1),
        Weekday::Sunday => shift_days(date, 1),
        _ => date,
    }
}

fn shift_days(date: Date, days: i64) -> Date {
    Date::from_julian_day((date.to_julian_day() as i64 + days) as i32).unwrap_or(date)
}

fn nth_weekday(year: i32, month: Month, weekday: Weekday, n: u8) -> Option<Date> {
    let first = Date::from_calendar_date(year, month, 1).ok()?;
    let offset = (weekday.number_days_from_monday() as i64
        - first.weekday().number_days_from_monday() as i64)
        .rem_euclid(7);
    let day = 1 + offset + (n as i64 - 1) * 7;
    Date::from_calendar_date(year, month, day as u8).ok()
}

fn last_weekday(year: i32, month: Month, weekday: Weekday) -> Option<Date> {
    let last = Date::from_calendar_date(year, month, month.length(year)).ok()?;
    let back = (last.weekday().number_days_from_monday() as i64
        - weekday.number_days_from_monday() as i64)
        .rem_euclid(7);
    Some(shift_days(last, -back))
}

/// Good Friday = two days before Easter Sunday (Anonymous Gregorian computus).
fn good_friday(year: i32) -> Option<Date> {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month_num = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    let month = Month::try_from(month_num as u8).ok()?;
    let easter = Date::from_calendar_date(year, month, day as u8).ok()?;
    Some(shift_days(easter, -2))
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

    #[test]
    fn christmas_closed() {
        // 2024-12-25 is a Wednesday.
        let xmas = datetime!(2024-12-25 12:00:00 -05:00);
        assert!(!equity_rth_open(xmas));
    }

    #[test]
    fn thanksgiving_2024_closed() {
        // 4th Thursday of November 2024 = Nov 28.
        let turkey = datetime!(2024-11-28 12:00:00 -05:00);
        assert!(!equity_rth_open(turkey));
    }

    #[test]
    fn good_friday_2024_closed() {
        // Easter 2024 = Mar 31, so Good Friday = Mar 29.
        let gf = datetime!(2024-03-29 12:00:00 -05:00);
        assert!(is_us_market_holiday(gf.date()));
    }

    #[test]
    fn independence_day_observed_2026() {
        // 2026-07-04 is a Saturday; observed close is Friday 2026-07-03.
        let observed_fri = datetime!(2026-07-03 12:00:00 -05:00);
        assert!(is_us_market_holiday(observed_fri.date()));
    }

    #[test]
    fn regular_weekday_open() {
        let tue = datetime!(2024-07-02 11:00:00 -05:00);
        assert!(equity_rth_open(tue));
        assert!(!is_us_market_holiday(tue.date()));
    }
}
