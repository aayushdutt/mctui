//! Human-readable times, sizes and counts.

use jiff::Timestamp;
use jiff::tz::TimeZone;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

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
    match bytes {
        MIB.. => format!("{} MB", megabytes(bytes)),
        KIB.. => format!("{} KB", bytes / KIB),
        _ => format!("{bytes} B"),
    }
}

/// How much of a download is done: "1.2 of 3.4 MB", or "2 of 10 KB" for a
/// small one — both in the total's unit.
pub fn size_of_total(done: u64, total: u64) -> String {
    if total >= MIB {
        format!("{} of {} MB", megabytes(done), megabytes(total))
    } else {
        format!("{} of {} KB", done / KIB, total.div_ceil(KIB))
    }
}

/// "1.2": megabytes to one decimal, rounded.
fn megabytes(bytes: u64) -> String {
    // u128: `bytes * 10` can't overflow.
    let (bytes, mib) = (u128::from(bytes), u128::from(MIB));
    let tenths = (bytes * 10 + mib / 2) / mib;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// "1 world", "2 worlds".
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// "1 of 3 files".
pub fn count_of_total(done: usize, total: usize, noun: &str) -> String {
    format!("{done} of {}", count(total, noun))
}

/// A big count, short: "231.7M", "12.3K", "999".
pub fn compact(n: u64) -> String {
    let per = |unit: u64| n as f64 / unit as f64;
    match n {
        1_000_000_000.. => format!("{:.1}B", per(1_000_000_000)),
        1_000_000.. => format!("{:.1}M", per(1_000_000)),
        1_000.. => format!("{:.1}K", per(1_000)),
        _ => n.to_string(),
    }
}

/// A count in full, its thousands marked: "5,056".
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
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
        assert_eq!(size_of_total(310_000, 3_500_000), "0.3 of 3.3 MB");
        assert_eq!(size_of_total(2048, 10_000), "2 of 10 KB");
        assert_eq!(size_of_total(0, 0), "0 of 0 KB");
    }

    #[test]
    fn counts() {
        assert_eq!(count(1, "world"), "1 world");
        assert_eq!(count(0, "world"), "0 worlds");
        assert_eq!(count_of_total(1, 3, "file"), "1 of 3 files");
        assert_eq!(count_of_total(0, 1, "file"), "0 of 1 file");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(12_345), "12.3K");
        assert_eq!(compact(146_600_000), "146.6M");
        assert_eq!(compact(1_200_000_000), "1.2B");
        assert_eq!(thousands(34), "34");
        assert_eq!(thousands(5056), "5,056");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
