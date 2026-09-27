//! Human-readable times and sizes.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// "Just now", "5m ago", "3h ago", "2d ago", then a date ("Jan 2", or
/// "Jan 2, 2025" outside the current year).
pub fn relative(t: Timestamp, now: Timestamp, tz: &TimeZone) -> String {
    let secs = now.as_second() - t.as_second();
    match secs {
        ..60 => "Just now".to_owned(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86_400 => format!("{}h ago", secs / 3600),
        86_400..604_800 => format!("{}d ago", secs / 86_400),
        _ => {
            let then = t.to_zoned(tz.clone());
            if then.year() == now.to_zoned(tz.clone()).year() {
                then.strftime("%b %-d").to_string()
            } else {
                then.strftime("%b %-d, %Y").to_string()
            }
        }
    }
}

/// "4h 12m", "4h", "8m", "<1m", or "Never" for zero.
pub fn play_time(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs / 60 % 60);
    match (h, m) {
        _ if secs == 0 => "Never".to_owned(),
        (0, 0) => "<1m".to_owned(),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// "1.2 MB", "412 KB", "80 B".
pub fn size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    match bytes {
        MIB.. => {
            // u128: `bytes * 10` can't overflow.
            let (bytes, mib) = (u128::from(bytes), u128::from(MIB));
            let tenths = (bytes * 10 + mib / 2) / mib;
            format!("{}.{} MB", tenths / 10, tenths % 10)
        }
        KIB.. => format!("{} KB", bytes / KIB),
        _ => format!("{bytes} B"),
    }
}

/// "1 world", "2 worlds".
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn relative_times() {
        let now = at("2026-09-27T12:00:00Z");
        let tz = TimeZone::UTC;
        let ago = |s| relative(at(s), now, &tz);
        assert_eq!(ago("2026-09-27T11:59:30Z"), "Just now");
        assert_eq!(ago("2026-09-27T12:00:30Z"), "Just now");
        assert_eq!(ago("2026-09-27T11:55:00Z"), "5m ago");
        assert_eq!(ago("2026-09-27T09:00:00Z"), "3h ago");
        assert_eq!(ago("2026-09-25T12:00:00Z"), "2d ago");
        assert_eq!(ago("2026-01-02T12:00:00Z"), "Jan 2");
        assert_eq!(ago("2025-01-02T12:00:00Z"), "Jan 2, 2025");
    }

    #[test]
    fn play_times() {
        assert_eq!(play_time(0), "Never");
        assert_eq!(play_time(30), "<1m");
        assert_eq!(play_time(8 * 60), "8m");
        assert_eq!(play_time(2 * 3600), "2h");
        assert_eq!(play_time(4 * 3600 + 12 * 60 + 5), "4h 12m");
    }

    #[test]
    fn sizes() {
        assert_eq!(size(80), "80 B");
        assert_eq!(size(412 * 1024), "412 KB");
        assert_eq!(size(1_258_291), "1.2 MB");
    }
}
