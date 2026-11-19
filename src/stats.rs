use crate::chat;
use crate::errors::{Error, Result};
pub(crate) use crate::graph::KindCounts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Range {
    Day,
    Week,
    Month,
    All,
}

pub(crate) const RANGES: [Range; 4] = [Range::Day, Range::Week, Range::Month, Range::All];

pub(crate) const TOP: i64 = 10;

pub(crate) const HOPS_CAP: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bucket {
    Hour,
    Day,
}

impl Bucket {
    pub(crate) fn token(self) -> &'static str {
        match self {
            Bucket::Hour => "hour",
            Bucket::Day => "day",
        }
    }
}

impl Range {
    pub(crate) fn parse(text: Option<&str>) -> Result<Range> {
        match text {
            None => Ok(RANGES[0]),
            Some(t) => RANGES
                .into_iter()
                .find(|r| r.token() == t)
                .ok_or_else(|| Error::Invalid {
                    what: "statistics range",
                    input: t.to_owned(),
                    reason: "expected 24h, 7d, 30d or all".into(),
                }),
        }
    }

    pub(crate) fn token(self) -> &'static str {
        match self {
            Range::Day => "24h",
            Range::Week => "7d",
            Range::Month => "30d",
            Range::All => "all",
        }
    }

    pub(crate) fn since(self, now: u64) -> Option<u64> {
        let hours: u64 = match self {
            Range::Day => 24,
            Range::Week => 7 * 24,
            Range::Month => 30 * 24,
            Range::All => return None,
        };
        Some(now.saturating_sub(hours * 3_600_000_000))
    }

    pub(crate) fn bucket(self) -> Bucket {
        match self {
            Range::Day | Range::Week => Bucket::Hour,
            Range::Month | Range::All => Bucket::Day,
        }
    }
}

pub(crate) fn plausible_time_zone(tz: &str) -> bool {
    (1..=64).contains(&tz.len())
        && tz.starts_with(|c: char| c.is_ascii_alphabetic())
        && tz.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
}

pub(crate) fn add_kind(mut k: KindCounts, kind: &str, n: u64) -> KindCounts {
    match kind {
        "message" => k.message += n,
        "position" => k.position += n,
        "hey" => k.hey += n,
        _ => k.other += n,
    }
    k
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) frames: u64,
    pub(crate) receptions: u64,
    pub(crate) messages: u64,
    pub(crate) stations: u64,
    pub(crate) relays: u64,
    pub(crate) direct: u64,
    pub(crate) first_seen: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeBucket {
    pub(crate) start: u64,
    pub(crate) kinds: KindCounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ranked {
    pub(crate) name: String,
    pub(crate) count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stats {
    pub(crate) range: Range,
    pub(crate) since: Option<u64>,
    pub(crate) until: u64,
    pub(crate) totals: Totals,
    pub(crate) kinds: KindCounts,
    pub(crate) timeline: Vec<TimeBucket>,
    pub(crate) hour_of_day: [u64; 24],
    pub(crate) top_origins: Vec<Ranked>,
    pub(crate) top_relays: Vec<Ranked>,
    pub(crate) hops: [u64; HOPS_CAP + 1],
    pub(crate) snr: Vec<(i16, u64)>,
    pub(crate) chat: Vec<Ranked>,
}

pub(crate) fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

pub(crate) fn hour_of_day(rows: &[(i32, i64)]) -> [u64; 24] {
    rows.iter().fold([0; 24], |mut acc, &(h, n)| {
        if let Some(slot) = usize::try_from(h).ok().and_then(|h| acc.get_mut(h)) {
            *slot += count(n);
        }
        acc
    })
}

pub(crate) fn hops(rows: &[(i32, i64)]) -> [u64; HOPS_CAP + 1] {
    rows.iter().fold([0; HOPS_CAP + 1], |mut acc, &(h, n)| {
        acc[usize::try_from(h).unwrap_or(0).min(HOPS_CAP)] += count(n);
        acc
    })
}

pub(crate) fn chat_label(dest: &str) -> String {
    if chat::is_broadcast(dest) {
        "all".to_owned()
    } else {
        dest.to_owned()
    }
}

pub(crate) fn ranked(rows: Vec<(String, i64)>) -> Vec<Ranked> {
    rows.into_iter()
        .map(|(name, n)| Ranked { name, count: count(n) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_parse_with_a_default() {
        assert_eq!(Range::parse(None).unwrap(), Range::Day);
        for r in RANGES {
            assert_eq!(Range::parse(Some(r.token())).unwrap(), r);
        }
        assert!(Range::parse(Some("1y")).is_err());
    }

    #[test]
    fn ranges_start_and_bucket() {
        let now = 100 * 86_400_000_000;
        assert_eq!(Range::Day.since(now), Some(now - 86_400_000_000));
        assert_eq!(Range::Month.since(now), Some(now - 30 * 86_400_000_000));
        assert_eq!(Range::All.since(now), None);
        assert_eq!(Range::Week.since(5), Some(0));
        assert_eq!(Range::Week.bucket().token(), "hour");
        assert_eq!(Range::All.bucket().token(), "day");
    }

    #[test]
    fn time_zone_names() {
        for tz in [
            "UTC",
            "Europe/Berlin",
            "America/Port-au-Prince",
            "Etc/GMT+1",
            "America/Argentina/Buenos_Aires",
        ] {
            assert!(plausible_time_zone(tz), "{tz}");
        }
        for tz in [
            "",
            "Europe/Berlin'; --",
            "../etc",
            "/Rome",
            "Europe Rome",
            &"A".repeat(65),
        ] {
            assert!(!plausible_time_zone(tz), "{tz}");
        }
    }

    #[test]
    fn kinds_add_up() {
        let k = [("message", 2), ("hey", 1), ("weird", 3), ("other", 1)]
            .into_iter()
            .fold(KindCounts::default(), |k, (kind, n)| add_kind(k, kind, n));
        assert_eq!(
            k,
            KindCounts {
                message: 2,
                position: 0,
                hey: 1,
                other: 4
            }
        );
    }

    #[test]
    fn histograms() {
        let h = hour_of_day(&[(0, 3), (23, 2), (24, 9), (-1, 9)]);
        assert_eq!((h[0], h[23], h.iter().sum::<u64>()), (3, 2, 5));
        assert_eq!(
            hops(&[(0, 5), (1, 4), (3, 1), (4, 2), (7, 1), (-2, 1)]),
            [6, 4, 0, 1, 3]
        );
        assert_eq!(count(-5), 0);
    }

    #[test]
    fn chat_labels_match_the_tabs() {
        assert_eq!(chat_label("*"), "all");
        assert_eq!(chat_label(""), "all");
        assert_eq!(chat_label("12351"), "12351");
        assert_eq!(
            ranked(vec![("A".into(), 2)]),
            vec![Ranked {
                name: "A".into(),
                count: 2
            }]
        );
    }
}
