use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{Error, Result};

pub(crate) fn unix_micros(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn parse_rfc3339(text: &str) -> Result<u64> {
    let bad = |reason: &str| Error::Invalid {
        what: "RFC 3339 time",
        input: text.to_owned(),
        reason: reason.to_owned(),
    };
    let body = text.strip_suffix('Z').ok_or_else(|| bad("must end with Z (UTC)"))?;
    let (date, time) = body.split_once('T').ok_or_else(|| bad("missing T"))?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let num = |s: &str, len: usize| -> Result<u32> {
        if s.len() == len && s.bytes().all(|b| b.is_ascii_digit()) {
            Ok(s.parse()?)
        } else {
            Err(bad("malformed"))
        }
    };
    let d: Vec<&str> = date.split('-').collect();
    let t: Vec<&str> = clock.split(':').collect();
    let ([y, mo, da], [h, mi, se]) = (d.as_slice(), t.as_slice()) else {
        return Err(bad("malformed"));
    };
    let (y, mo, da, h, mi, se) = (
        num(y, 4)?,
        num(mo, 2)?,
        num(da, 2)?,
        num(h, 2)?,
        num(mi, 2)?,
        num(se, 2)?,
    );
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 || se > 60 {
        return Err(bad("out of range"));
    }
    if fraction.len() > 6 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad("fraction must have at most six digits"));
    }
    let micros: u64 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<6}").parse()?
    };
    let days = days_from_civil(i64::from(y), mo, da);
    let secs = days * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60 + i64::from(se);
    u64::try_from(secs)
        .map(|s| s * 1_000_000 + micros)
        .map_err(|_| bad("before 1970"))
}

fn date_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub(crate) fn rfc3339_micros(micros: u64) -> String {
    format!("{}.{:06}Z", date_time(micros / 1_000_000), micros % 1_000_000)
}

pub(crate) fn rfc3339_secs(micros: u64) -> String {
    format!("{}Z", date_time(micros / 1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn epoch() {
        assert_eq!(rfc3339_micros(0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(rfc3339_secs(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_instant() {
        let micros = 1_794_564_903_123_456;
        assert_eq!(rfc3339_micros(micros), "2026-11-13T10:15:03.123456Z");
        assert_eq!(rfc3339_secs(micros), "2026-11-13T10:15:03Z");
    }

    #[test]
    fn leap_day() {
        assert_eq!(rfc3339_secs(1_709_251_199_000_000), "2024-02-29T23:59:59Z");
        assert_eq!(rfc3339_secs(1_709_251_200_000_000), "2024-03-01T00:00:00Z");
    }

    #[test]
    fn parse_round_trips() {
        [0, 1_794_564_903_123_456, 1_709_251_199_000_000, 1_709_251_200_000_001]
            .into_iter()
            .for_each(|m| {
                assert_eq!(parse_rfc3339(&rfc3339_micros(m)).unwrap(), m);
            });
        assert_eq!(parse_rfc3339("2026-11-13T10:15:03Z").unwrap(), 1_794_564_903_000_000);
        assert_eq!(parse_rfc3339("2026-11-13T10:15:03.5Z").unwrap(), 1_794_564_903_500_000);
    }

    #[test]
    fn parse_refuses_malformed() {
        [
            "",
            "2026-11-13 10:15:03Z",
            "2026-11-13T10:15:03+01:00",
            "2026-13-13T10:15:03Z",
            "2026-11-13T24:00:00Z",
            "2026-11-13T10:15:03.1234567Z",
            "1969-12-31T23:59:59Z",
            "26-11-13T10:15:03Z",
            "2026-11-13T10:15Z",
        ]
        .into_iter()
        .for_each(|t| assert!(parse_rfc3339(t).is_err(), "{t}"));
    }

    #[test]
    fn unix_micros_of_system_time() {
        assert_eq!(unix_micros(UNIX_EPOCH + Duration::from_micros(42)), 42);
        assert_eq!(unix_micros(UNIX_EPOCH - Duration::from_secs(1)), 0);
    }
}
