//! The times the screen shows, in local time as the web console's `Date` and `Intl` showed them: a
//! saved session's ("16 Oct 2026, 19:05"), a mix backup's ("18 Oct 2026, 19:04:05") and a link
//! second's ("21:34:05", 24 hours). The zone is a value, so tests pin it and the app reads the
//! system's offset for each instant (summer time included).

use chrono::{DateTime, Datelike, Local, Offset, TimeZone as _, Timelike};

/// The zone the times are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zone {
    /// The system's, as the web console's `Date` was.
    #[default]
    Local,
    /// A fixed offset from UTC, in minutes (UTC is 0).
    Fixed(i32),
}

impl Zone {
    /// The offset from UTC at the instant `at_ms` (Unix milliseconds), in seconds.
    fn offset_seconds(self, at_ms: i64) -> i32 {
        match self {
            Self::Fixed(minutes) => minutes * 60,
            Self::Local => Local
                .timestamp_millis_opt(at_ms)
                .single()
                .map(|time| time.offset().fix().local_minus_utc())
                .unwrap_or(0),
        }
    }

    /// The wall clock at `at_ms` in this zone, or `None` for an instant out of range.
    fn civil(self, at_ms: i64) -> Option<chrono::NaiveDateTime> {
        let shifted = at_ms.checked_add(i64::from(self.offset_seconds(at_ms)) * 1000)?;
        DateTime::from_timestamp_millis(shifted).map(|time| time.naive_utc())
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// "16 Oct 2026, 19:05" for `at_ms`, or with `seconds` "16 Oct 2026, 19:05:07"; the day has no
/// leading zero, the time is 24-hour.
fn date_time(at_ms: i64, zone: Zone, seconds: bool) -> Option<String> {
    let time = zone.civil(at_ms)?;
    let month = MONTHS[time.month0() as usize];
    let mut text = format!(
        "{} {month} {}, {:02}:{:02}",
        time.day(),
        time.year(),
        time.hour(),
        time.minute()
    );
    if seconds {
        text.push_str(&format!(":{:02}", time.second()));
    }
    Some(text)
}

/// When a session was saved (`formatSavedAt`), or `None` when the desk does not know.
pub fn format_saved_at(at_ms: Option<u64>, zone: Zone) -> Option<String> {
    date_time(i64::try_from(at_ms?).ok()?, zone, false)
}

/// When a mix backup was taken, to the second (`formatBackupTime`), or `None` when not known.
pub fn format_backup_time(at_ms: Option<u64>, zone: Zone) -> Option<String> {
    date_time(i64::try_from(at_ms?).ok()?, zone, true)
}

/// A link second's local time, "21:34:05" (`formatClock`).
pub fn format_clock(at_ms: u64, zone: Zone) -> String {
    i64::try_from(at_ms)
        .ok()
        .and_then(|at| zone.civil(at))
        .map(|time| {
            format!(
                "{:02}:{:02}:{:02}",
                time.hour(),
                time.minute(),
                time.second()
            )
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-16 19:05:07 UTC.
    const AT: u64 = 1_792_177_507_000;

    #[test]
    fn a_saved_session_shows_its_day_month_year_and_minute() {
        assert_eq!(
            format_saved_at(Some(AT), Zone::Fixed(0)).as_deref(),
            Some("16 Oct 2026, 19:05")
        );
        assert_eq!(format_saved_at(None, Zone::Fixed(0)), None);
        assert_eq!(
            format_saved_at(Some(AT), Zone::Fixed(120)).as_deref(),
            Some("16 Oct 2026, 21:05"),
            "in the zone shown"
        );
    }

    #[test]
    fn a_backup_shows_its_time_to_the_second_and_a_link_second_its_clock() {
        assert_eq!(
            format_backup_time(Some(AT), Zone::Fixed(0)).as_deref(),
            Some("16 Oct 2026, 19:05:07")
        );
        assert_eq!(format_backup_time(None, Zone::Fixed(0)), None);
        assert_eq!(format_clock(AT + 65_000, Zone::Fixed(0)), "19:06:12");
        assert_eq!(
            format_saved_at(Some(1_775_026_800_000), Zone::Fixed(0)).as_deref(),
            Some("1 Apr 2026, 07:00"),
            "no leading zero on the day"
        );
    }
}
