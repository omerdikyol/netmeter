use crate::config::{Cycle, Plan};
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, NaiveDateTime, TimeZone};

pub const MINUTE: i64 = 60;
pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;

/// Current wall-clock time as unix seconds.
pub fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Round a unix timestamp down to the start of its minute (our storage bucket).
pub fn floor_minute(secs: i64) -> i64 {
    secs - secs.rem_euclid(MINUTE)
}

fn localize(naive: NaiveDateTime, fallback: DateTime<Local>) -> DateTime<Local> {
    // Around a DST transition the naive time may not exist; fall back to `now`
    // rather than panicking, since a slightly off window is harmless.
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or(fallback)
}

/// Local midnight at the start of today.
pub fn local_midnight(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    localize(date.and_hms_opt(0, 0, 0).expect("valid midnight"), now)
}

/// Local midnight on the first day of the current month.
pub fn month_start(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid date");
    localize(first.and_hms_opt(0, 0, 0).expect("valid midnight"), now)
}

/// A preset or custom reporting window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    LastHour,
    Today,
    Last24Hours,
    Last7Days,
    ThisMonth,
    BillingCycle,
    Custom { from: i64, to: i64 },
}

impl Range {
    pub const PRESETS: [Range; 5] = [
        Range::LastHour,
        Range::Today,
        Range::Last7Days,
        Range::ThisMonth,
        Range::BillingCycle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Range::LastHour => "Last hour",
            Range::Today => "Today",
            Range::Last24Hours => "Last 24 hours",
            Range::Last7Days => "Last 7 days",
            Range::ThisMonth => "This month",
            Range::BillingCycle => "Billing cycle",
            Range::Custom { .. } => "Custom",
        }
    }

    /// Resolve to `(from_inclusive, to_exclusive)` unix seconds.
    pub fn resolve(self, now: DateTime<Local>, plan: &Plan) -> (i64, i64) {
        let now_ts = now.timestamp();
        match self {
            Range::LastHour => (now_ts - HOUR, now_ts),
            Range::Today => (local_midnight(now).timestamp(), now_ts),
            Range::Last24Hours => (now_ts - DAY, now_ts),
            Range::Last7Days => (now_ts - 7 * DAY, now_ts),
            Range::ThisMonth => (month_start(now).timestamp(), now_ts),
            Range::BillingCycle => (cycle_start(now, plan).timestamp(), now_ts),
            Range::Custom { from, to } => (from, to),
        }
    }
}

/// Start of the current billing cycle.
pub fn cycle_start(now: DateTime<Local>, plan: &Plan) -> DateTime<Local> {
    match plan.cycle {
        Cycle::Weekly => week_start(now),
        Cycle::Monthly => month_cycle_start(now, plan.reset_day),
    }
}

fn week_start(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    let back = date.weekday().num_days_from_monday() as u64;
    let monday = date.checked_sub_days(Days::new(back)).unwrap_or(date);
    localize(monday.and_hms_opt(0, 0, 0).expect("valid midnight"), now)
}

fn month_cycle_start(now: DateTime<Local>, reset_day: u32) -> DateTime<Local> {
    let today = now.date_naive();
    let this_month = day_in_month(today.year(), today.month(), reset_day);
    let start = if today >= this_month {
        this_month
    } else {
        let (year, month) = previous_month(today.year(), today.month());
        day_in_month(year, month, reset_day)
    };
    localize(start.and_hms_opt(0, 0, 0).expect("valid midnight"), now)
}

/// Clamp a day-of-month to the actual length of that month.
fn day_in_month(year: i32, month: u32, day: u32) -> NaiveDate {
    let clamped = day.clamp(1, days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, clamped).expect("valid date")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid date");
    first_of_next.pred_opt().expect("valid previous day").day()
}

fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

/// Used as a percentage of the cap (0.0 if no cap is set).
pub fn cap_percent(used: u64, cap: u64) -> f64 {
    if cap == 0 {
        0.0
    } else {
        used as f64 / cap as f64 * 100.0
    }
}

/// The highest warning threshold the usage has crossed, if any.
pub fn alarm_level(used: u64, cap: u64, warn_at: &[f64]) -> Option<f64> {
    if cap == 0 {
        return None;
    }
    let fraction = used as f64 / cap as f64;
    warn_at
        .iter()
        .copied()
        .filter(|threshold| *threshold > 0.0 && fraction >= *threshold)
        .fold(None, |acc: Option<f64>, threshold| {
            Some(acc.map_or(threshold, |a| a.max(threshold)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: u32, day: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn monthly_cycle_before_reset_day_uses_previous_month() {
        let plan = Plan {
            cycle: Cycle::Monthly,
            reset_day: 15,
            ..Plan::default()
        };
        let start = cycle_start(at(2026, 10, 2), &plan);
        assert_eq!(
            start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
        );
    }

    #[test]
    fn monthly_cycle_after_reset_day_uses_this_month() {
        let plan = Plan {
            cycle: Cycle::Monthly,
            reset_day: 15,
            ..Plan::default()
        };
        let start = cycle_start(at(2026, 10, 20), &plan);
        assert_eq!(
            start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 15).unwrap()
        );
    }

    #[test]
    fn reset_day_one_is_first_of_month() {
        let plan = Plan {
            cycle: Cycle::Monthly,
            reset_day: 1,
            ..Plan::default()
        };
        let start = cycle_start(at(2026, 10, 2), &plan);
        assert_eq!(
            start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
        );
    }

    #[test]
    fn reset_day_is_clamped_to_short_months() {
        // Day 31 in February 2026 (28 days) clamps to the 28th.
        let plan = Plan {
            cycle: Cycle::Monthly,
            reset_day: 31,
            ..Plan::default()
        };
        let start = cycle_start(at(2026, 3, 5), &plan);
        assert_eq!(
            start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
    }

    #[test]
    fn weekly_cycle_starts_on_monday() {
        let plan = Plan {
            cycle: Cycle::Weekly,
            ..Plan::default()
        };
        // 2026-10-02 is a Friday, so the cycle began Monday 2026-09-28.
        let start = cycle_start(at(2026, 10, 2), &plan);
        assert_eq!(
            start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
        );
    }

    #[test]
    fn today_range_starts_at_local_midnight() {
        let now = at(2026, 10, 2);
        let (from, to) = Range::Today.resolve(now, &Plan::default());
        assert_eq!(from, local_midnight(now).timestamp());
        assert_eq!(to, now.timestamp());
    }

    #[test]
    fn cap_percent_handles_no_cap() {
        assert_eq!(cap_percent(100, 0), 0.0);
        assert!((cap_percent(42_500, 50_000) - 85.0).abs() < 1e-9);
    }

    #[test]
    fn alarm_level_reports_highest_threshold_crossed() {
        let warn = [0.8, 1.0];
        assert_eq!(alarm_level(100, 1000, &warn), None);
        assert_eq!(alarm_level(850, 1000, &warn), Some(0.8));
        assert_eq!(alarm_level(1_050, 1000, &warn), Some(1.0));
        assert_eq!(alarm_level(500, 0, &warn), None);
    }
}
