//! Serde helpers for Go's `time.Time` JSON, used with `#[serde(with = "gotime")]`
//! on `Option<Timestamp>` fields.
//!
//! Go has no "unset" time: it writes its zero value, `0001-01-01T00:00:00Z`.
//! That (any time in year 1), `null`, and `""` read as `None`; `None` is
//! written back as Go's zero so Go builds read the file unchanged.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

const GO_ZERO: &str = "0001-01-01T00:00:00Z";

#[expect(clippy::ref_option, reason = "serde `with` passes `&T`")]
pub(crate) fn serialize<S: Serializer>(time: &Option<Timestamp>, s: S) -> Result<S::Ok, S::Error> {
    match time {
        Some(t) => s.collect_str(t),
        None => s.serialize_str(GO_ZERO),
    }
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Timestamp>, D::Error> {
    let Some(s) = Option::<String>::deserialize(d)? else {
        return Ok(None);
    };
    if s.is_empty() {
        return Ok(None);
    }
    let t: Timestamp = s.parse().map_err(D::Error::custom)?;
    Ok((TimeZone::UTC.to_datetime(t).year() > 1).then_some(t))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct T {
        #[serde(default, with = "super")]
        t: Option<Timestamp>,
    }

    fn read(json: &str) -> Option<Timestamp> {
        serde_json::from_str::<T>(json).unwrap().t
    }

    #[test]
    fn reads_go_times() {
        let t = read(r#"{"t":"2026-06-15T19:37:10.324669+05:30"}"#).unwrap();
        assert_eq!(t.to_string(), "2026-06-15T14:07:10.324669Z");
        assert_eq!(read(r#"{"t":"0001-01-01T00:00:00Z"}"#), None);
        // Go's zero time rendered in a local zone is still year 1.
        assert_eq!(read(r#"{"t":"0001-01-01T05:53:28+05:53"}"#), None);
        assert_eq!(read(r#"{"t":null}"#), None);
        assert_eq!(read(r#"{"t":""}"#), None);
        assert_eq!(read("{}"), None);
        assert!(serde_json::from_str::<T>(r#"{"t":"yesterday"}"#).is_err());
    }

    #[test]
    fn writes_go_zero_for_none() {
        let json = serde_json::to_string(&T { t: None }).unwrap();
        assert_eq!(json, r#"{"t":"0001-01-01T00:00:00Z"}"#);
        let t: Timestamp = "2026-01-02T03:04:05.5Z".parse().unwrap();
        let json = serde_json::to_string(&T { t: Some(t) }).unwrap();
        assert_eq!(json, r#"{"t":"2026-01-02T03:04:05.5Z"}"#);
        assert_eq!(read(&json), Some(t));
    }
}
