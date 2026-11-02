//! Cron schedules, read in the server's zone (or the one the config names).

use chrono::{DateTime, Utc};
use croner::Cron;
use std::str::FromStr;

pub fn parse(expr: &str) -> Result<Cron, String> {
    Cron::from_str(expr.trim()).map_err(|e| e.to_string())
}

fn zone(tz: &str) -> Option<chrono_tz::Tz> {
    if tz.is_empty() { None } else { tz.parse().ok() }
}

/// The next time `expr` fires after `after`. `None` for an empty or invalid expression.
pub fn next_after(expr: &str, tz: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if expr.trim().is_empty() {
        return None;
    }
    let cron = parse(expr).ok()?;
    match zone(tz) {
        Some(z) => cron
            .find_next_occurrence(&after.with_timezone(&z), false)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        None => cron
            .find_next_occurrence(&after.with_timezone(&chrono::Local), false)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
    }
}

/// The last time `expr` fired at or before `at`.
pub fn previous_before(expr: &str, tz: &str, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if expr.trim().is_empty() {
        return None;
    }
    let cron = parse(expr).ok()?;
    match zone(tz) {
        Some(z) => cron
            .find_previous_occurrence(&at.with_timezone(&z), true)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        None => cron
            .find_previous_occurrence(&at.with_timezone(&chrono::Local), true)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn daily_at_three_in_istanbul() {
        let after = Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap();
        let next = next_after("0 3 * * *", "Europe/Istanbul", after).unwrap();
        // 03:00 in Istanbul (UTC+3) is midnight UTC
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 3, 0, 0, 0).unwrap());
        let prev = previous_before("0 3 * * *", "Europe/Istanbul", after).unwrap();
        assert_eq!(prev, Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap());
    }

    #[test]
    fn empty_and_invalid_never_fire() {
        let now = Utc::now();
        assert!(next_after("", "", now).is_none());
        assert!(next_after("not cron", "", now).is_none());
    }
}
