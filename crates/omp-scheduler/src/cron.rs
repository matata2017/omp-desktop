//! Five-field cron in the machine's local zone: `minute hour day-of-month month day-of-week`.
//!
//! Supported field syntax is `*`, `*/n`, `a`, `a-b`, `a-b/n` and comma unions of those. A field
//! that is a plain star (including `*/n`, as in vixie cron) does not restrict the day, and when
//! both day fields are restricted the day matches if either one does. `7` is Sunday. Wall times
//! skipped by a spring-forward gap do not fire; wall times repeated by a fall-back fire once.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
use omp_core::{Error, Result};

/// Cron searches stop after this many days without a match (`0 0 31 2 *` never fires).
const SEARCH_DAYS: i64 = 366 * 5;

#[derive(Debug, Clone)]
pub struct CronSchedule {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days: Vec<u32>,
    months: Vec<u32>,
    /// 0 = Sunday through 6 = Saturday; `7` is folded into `0`.
    weekdays: Vec<u32>,
    any_day: bool,
    any_weekday: bool,
}

impl CronSchedule {
    /// Parse `minute hour day-of-month month day-of-week`; a seconds field is refused.
    pub fn parse(expression: &str) -> Result<Self> {
        let fields: Vec<&str> = expression.split_whitespace().collect();
        match fields.len() {
            5 => {}
            6 => {
                return Err(Error::msg(format!(
                    "cron: six fields given, seconds are not supported in `{expression}`"
                )));
            }
            count => {
                return Err(Error::msg(format!(
                    "cron: expected 5 fields (minute hour day-of-month month day-of-week), found {count} in `{expression}`"
                )));
            }
        }
        Ok(Self {
            minutes: parse_field(expression, fields[0], "minute", 0, 59)?,
            hours: parse_field(expression, fields[1], "hour", 0, 23)?,
            days: parse_field(expression, fields[2], "day", 1, 31)?,
            months: parse_field(expression, fields[3], "month", 1, 12)?,
            weekdays: parse_field(expression, fields[4], "weekday", 0, 7)?
                .into_iter()
                .map(|day| if day == 7 { 0 } else { day })
                .collect(),
            any_day: fields[2].starts_with('*'),
            any_weekday: fields[4].starts_with('*'),
        })
    }

    /// The first fire strictly after `after_ms`, as epoch milliseconds, or `None` when the
    /// expression cannot fire inside the search window.
    pub fn next_after(&self, after_ms: i64, tz: chrono::Local) -> Option<i64> {
        self.next_in(after_ms, &tz)
    }

    fn next_in<Tz: TimeZone>(&self, after_ms: i64, tz: &Tz) -> Option<i64> {
        let utc = DateTime::from_timestamp_millis(after_ms)?;
        let start = tz.from_utc_datetime(&utc.naive_utc()).date_naive();
        for offset in 0..SEARCH_DAYS {
            let Some(date) = start.checked_add_signed(Duration::days(offset)) else {
                break;
            };
            if !self.matches_date(date) {
                continue;
            }
            for &hour in &self.hours {
                for &minute in &self.minutes {
                    let Some(naive) = date.and_hms_opt(hour, minute, 0) else {
                        continue;
                    };
                    let Some(instant) = local_instant(tz, naive) else {
                        continue;
                    };
                    let millis = instant.timestamp_millis();
                    if millis > after_ms {
                        return Some(millis);
                    }
                }
            }
        }
        None
    }

    fn matches_date(&self, date: NaiveDate) -> bool {
        if !self.months.contains(&date.month()) {
            return false;
        }
        let day = self.days.contains(&date.day());
        let weekday = self.weekdays.contains(&date.weekday().num_days_from_sunday());
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => weekday,
            (false, true) => day,
            // Both day fields restricted: a day matching either one fires.
            (false, false) => day || weekday,
        }
    }
}

fn local_instant<Tz: TimeZone>(tz: &Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(instant) => Some(instant),
        // A repeated wall time (fall-back) fires once, at its first occurrence.
        LocalResult::Ambiguous(first, _) => Some(first),
        // A skipped wall time (spring-forward) has no instant.
        LocalResult::None => None,
    }
}

fn parse_field(
    expression: &str,
    field: &str,
    name: &str,
    min: u32,
    max: u32,
) -> Result<Vec<u32>> {
    let mut values = Vec::new();
    for part in field.split(',') {
        if part.is_empty() {
            return Err(invalid(expression, format!("{name} field has an empty list item")));
        }
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, parse_step(expression, name, step)?),
            None => (part, 1),
        };
        let (low, high) = match range {
            "*" => (min, max),
            _ => match range.split_once('-') {
                Some((low, high)) => (
                    parse_value(expression, name, low)?,
                    parse_value(expression, name, high)?,
                ),
                None => {
                    let value = parse_value(expression, name, range)?;
                    // `5/15` means "from 5 to the end in steps of 15", as in vixie cron.
                    (value, if part.contains('/') { max } else { value })
                }
            },
        };
        if low < min || high > max {
            return Err(invalid(
                expression,
                format!("{name} range {low}-{high} is outside {min}-{max}"),
            ));
        }
        if low > high {
            return Err(invalid(expression, format!("{name} range {low}-{high} is backwards")));
        }
        let mut value = low;
        while value <= high {
            values.push(value);
            value = value.saturating_add(step);
        }
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn parse_value(expression: &str, name: &str, token: &str) -> Result<u32> {
    token
        .parse::<u32>()
        .map_err(|_| invalid(expression, format!("{name} value `{token}` is not a number")))
}

fn parse_step(expression: &str, name: &str, token: &str) -> Result<u32> {
    let step = token
        .parse::<u32>()
        .map_err(|_| invalid(expression, format!("{name} step `{token}` is not a number")))?;
    if step == 0 {
        return Err(invalid(expression, format!("{name} step must be at least 1")));
    }
    Ok(step)
}

fn invalid(expression: &str, detail: impl std::fmt::Display) -> Error {
    Error::msg(format!("cron: {detail} in `{expression}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone, Timelike, Weekday};

    fn local(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
            .timestamp_millis()
    }

    fn fire(cron: &CronSchedule, after: i64) -> DateTime<Local> {
        let next = cron.next_after(after, Local).expect("a fire");
        Local.timestamp_millis_opt(next).unwrap()
    }

    #[test]
    fn every_five_minutes_steps_five_minutes() {
        let cron = CronSchedule::parse("*/5 * * * *").unwrap();
        let mut cursor = local(2026, 3, 4, 10, 2);
        let mut previous = None;
        for _ in 0..6 {
            let next = cron.next_after(cursor, Local).unwrap();
            let at = Local.timestamp_millis_opt(next).unwrap();
            assert_eq!(at.minute() % 5, 0, "{at}");
            assert_eq!(at.second(), 0);
            if let Some(previous) = previous {
                assert_eq!(next - previous, 5 * 60_000);
            }
            previous = Some(next);
            cursor = next;
        }
        // Strictly after: the fire at 10:05 itself does not count again.
        let at = fire(&cron, local(2026, 3, 4, 10, 5));
        assert_eq!((at.hour(), at.minute()), (10, 10));
    }

    #[test]
    fn weekday_mornings_fire_monday_to_friday_only() {
        let cron = CronSchedule::parse("0 9 * * 1-5").unwrap();
        let mut cursor = local(2026, 1, 4, 12, 0); // Sunday
        for _ in 0..10 {
            let at = fire(&cron, cursor);
            assert_eq!((at.hour(), at.minute()), (9, 0));
            assert!(
                matches!(
                    at.weekday(),
                    Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
                ),
                "{at}"
            );
            cursor = at.timestamp_millis();
        }
        // Friday evening skips the weekend for Monday 09:00.
        let at = fire(&cron, local(2026, 1, 9, 18, 0));
        assert_eq!(at.weekday(), Weekday::Mon);
        assert_eq!(at.day(), 12);
        assert_eq!((at.hour(), at.minute()), (9, 0));
    }

    #[test]
    fn comma_and_range_fields_union() {
        let cron = CronSchedule::parse("0,30 9-10 * * *").unwrap();
        let mut cursor = local(2026, 3, 4, 8, 59);
        let mut fires = Vec::new();
        for _ in 0..5 {
            let at = fire(&cron, cursor);
            fires.push((at.hour(), at.minute()));
            cursor = at.timestamp_millis();
        }
        assert_eq!(fires, [(9, 0), (9, 30), (10, 0), (10, 30), (9, 0)]);
    }

    #[test]
    fn stepped_ranges_start_at_the_low_end() {
        let cron = CronSchedule::parse("10-20/5 * * * *").unwrap();
        assert_eq!(cron.minutes, [10, 15, 20]);
        let hourly = CronSchedule::parse("*/15 * * * *").unwrap();
        assert_eq!(hourly.minutes, [0, 15, 30, 45]);
    }

    #[test]
    fn sunday_is_zero_and_seven() {
        let cron = CronSchedule::parse("0 9 * * 7").unwrap();
        let at = fire(&cron, local(2026, 1, 5, 10, 0)); // Monday
        assert_eq!(at.weekday(), Weekday::Sun);
        assert_eq!(at.day(), 11);
    }

    #[test]
    fn restricted_day_fields_fire_on_either_match() {
        let cron = CronSchedule::parse("0 0 1 * 1").unwrap();
        // 2026-01-01 is a Thursday and the 1st; 2026-01-05 is the next Monday.
        let first = fire(&cron, local(2025, 12, 31, 12, 0));
        assert_eq!((first.month(), first.day()), (1, 1));
        let second = fire(&cron, first.timestamp_millis());
        assert_eq!(second.weekday(), Weekday::Mon);
        assert_eq!(second.day(), 5);
    }

    #[test]
    fn rejects_malformed_expressions() {
        let cases = [
            ("0 9 * *", "expected 5 fields"),
            ("", "expected 5 fields"),
            ("garbage", "expected 5 fields"),
            ("0 9 * * * *", "seconds"),
            ("60 * * * *", "minute"),
            ("* 24 * * *", "hour"),
            ("* * 0 * *", "day"),
            ("* * * 13 *", "month"),
            ("* * * * 8", "weekday"),
            ("*/0 * * * *", "step"),
            ("*/ * * * *", "step"),
            ("5-1 * * * *", "backwards"),
            ("1,,2 * * * *", "empty"),
            ("1-2-3 * * * *", "not a number"),
        ];
        for (expression, expected) in cases {
            let error = CronSchedule::parse(expression)
                .expect_err(expression)
                .to_string();
            assert!(error.contains(expected), "`{expression}`: {error}");
        }
    }

    #[test]
    fn huge_steps_stop_after_one_value() {
        let cron = CronSchedule::parse("10/4294967295 * * * *").unwrap();
        assert_eq!(cron.minutes, [10]);
    }

    #[test]
    fn search_window_reaches_the_next_leap_day() {
        let cron = CronSchedule::parse("0 0 29 2 *").unwrap();
        let at = fire(&cron, local(2026, 3, 1, 0, 0));
        assert_eq!((at.year(), at.month(), at.day()), (2028, 2, 29));
        let impossible = CronSchedule::parse("0 0 31 2 *").unwrap();
        assert!(impossible.next_after(local(2026, 3, 1, 0, 0), Local).is_none());
    }
}
