use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use crate::aprs::Position;
use crate::chat::{self, Target, TargetInfo};
use crate::errors::{Error, Result};
use crate::frame::{self, Frame, HopReport, Kind, Payload, Reception, Signal, SourceKind};
use crate::graph::Node;
use crate::stats::{self, KindCounts, Range, Stats, TimeBucket, Totals};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(super) struct HopJson {
    hw: u8,
    rssi: i16,
    snr: i8,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ReceptionRow {
    pub(super) id: Uuid,
    pub(super) at_us: i64,
    pub(super) source: String,
    pub(super) msg_id: Option<i64>,
    pub(super) copy: i32,
    pub(super) origin: String,
    pub(super) relays: Vec<String>,
    pub(super) path: Vec<String>,
    pub(super) rssi: Option<i16>,
    pub(super) snr: Option<i16>,
    pub(super) hw: Option<i16>,
    pub(super) firmware: Option<String>,
    pub(super) kind: String,
    pub(super) dest: Option<String>,
    pub(super) text: Option<String>,
    pub(super) lat: Option<f64>,
    pub(super) lon: Option<f64>,
    pub(super) alt_m: Option<f64>,
    pub(super) neighbours: Option<i32>,
    pub(super) hops: Option<Json<Vec<HopJson>>>,
    pub(super) type_char: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(super) struct Columns {
    pub(super) source: &'static str,
    pub(super) msg_id: Option<i64>,
    pub(super) origin: String,
    pub(super) relays: Vec<String>,
    pub(super) rssi: Option<i16>,
    pub(super) snr: Option<i16>,
    pub(super) hw: Option<i16>,
    pub(super) firmware: Option<String>,
    pub(super) kind: &'static str,
    pub(super) dest: Option<String>,
    pub(super) text: Option<String>,
    pub(super) lat: Option<f64>,
    pub(super) lon: Option<f64>,
    pub(super) alt_m: Option<f64>,
    pub(super) neighbours: Option<i32>,
    pub(super) hops: Option<Json<Vec<HopJson>>>,
    pub(super) type_char: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct NodeRow {
    pub(super) call: String,
    pub(super) own: bool,
    pub(super) lat: Option<f64>,
    pub(super) lon: Option<f64>,
    pub(super) alt_m: Option<f64>,
    pub(super) hw: Option<i16>,
    pub(super) firmware: Option<String>,
    pub(super) frames: i64,
    pub(super) last_seen_us: i64,
}

fn bad(what: &'static str, input: impl ToString, reason: &str) -> Error {
    Error::Invalid {
        what,
        input: input.to_string(),
        reason: reason.to_owned(),
    }
}

fn micros(value: i64, what: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| bad(what, value, "before 1970"))
}

fn narrow<T: TryFrom<i64>>(value: Option<impl Into<i64>>, what: &'static str) -> Result<Option<T>> {
    value
        .map(|v| {
            let v: i64 = v.into();
            T::try_from(v).map_err(|_| bad(what, v, "out of range"))
        })
        .transpose()
}

fn position(lat: Option<f64>, lon: Option<f64>, alt_m: Option<f64>) -> Option<Position> {
    lat.zip(lon).map(|(lat, lon)| Position { lat, lon, alt_m })
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TargetRow {
    pub(super) value: String,
    pub(super) messages: i64,
    pub(super) last_us: Option<i64>,
}

pub(super) fn target_info(row: TargetRow, channel: bool) -> Option<TargetInfo> {
    let target = if channel {
        chat::is_group(&row.value).then(|| Target::Channel(row.value.clone()))?
    } else {
        Target::Callsign(frame::call(&row.value).ok()?)
    };
    Some(TargetInfo {
        target,
        messages: u64::try_from(row.messages).unwrap_or(0),
        last_seen: row.last_us.and_then(|v| u64::try_from(v).ok()),
    })
}

pub(super) fn columns(frame: &Frame) -> Columns {
    let base = Columns {
        source: frame.source.as_str(),
        msg_id: frame.msg_id.map(i64::from),
        origin: frame.origin.clone(),
        relays: frame.relays.clone(),
        rssi: frame.signal.map(|s| s.rssi),
        snr: frame.signal.map(|s| i16::from(s.snr)),
        hw: frame.hw.map(i16::from),
        firmware: frame.firmware.clone(),
        kind: frame.payload.kind().as_str(),
        dest: None,
        text: None,
        lat: None,
        lon: None,
        alt_m: None,
        neighbours: None,
        hops: None,
        type_char: None,
    };
    match &frame.payload {
        Payload::Message { dest, text } => Columns {
            dest: Some(dest.clone()),
            text: Some(text.clone()),
            ..base
        },
        Payload::Position(p) => Columns {
            lat: Some(p.lat),
            lon: Some(p.lon),
            alt_m: p.alt_m,
            ..base
        },
        Payload::Hey { neighbours, hops } => Columns {
            neighbours: neighbours.and_then(|n| i32::try_from(n).ok()),
            hops: Some(Json(
                hops.iter()
                    .map(|h| HopJson {
                        hw: h.hw,
                        rssi: h.signal.rssi,
                        snr: h.signal.snr,
                    })
                    .collect(),
            )),
            ..base
        },
        Payload::Other { kind, text } => Columns {
            type_char: Some(kind.to_string()),
            text: Some(text.clone()),
            ..base
        },
    }
}

fn payload(row: &ReceptionRow) -> Result<Payload> {
    match Kind::parse(&row.kind)? {
        Kind::Message => Ok(Payload::Message {
            dest: row.dest.clone().unwrap_or_default(),
            text: row.text.clone().unwrap_or_default(),
        }),
        Kind::Position => position(row.lat, row.lon, row.alt_m)
            .map(Payload::Position)
            .ok_or_else(|| bad("stored position", row.id, "lat or lon missing")),
        Kind::Hey => Ok(Payload::Hey {
            neighbours: narrow(row.neighbours, "neighbours")?,
            hops: row
                .hops
                .as_ref()
                .map(|Json(h)| {
                    h.iter()
                        .map(|h| HopReport {
                            hw: h.hw,
                            signal: Signal {
                                rssi: h.rssi,
                                snr: h.snr,
                            },
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
        Kind::Other => Ok(Payload::Other {
            kind: row.type_char.as_deref().and_then(|t| t.chars().next()).unwrap_or(' '),
            text: row.text.clone().unwrap_or_default(),
        }),
    }
}

pub(super) fn reception(row: ReceptionRow) -> Result<Reception> {
    let payload = payload(&row)?;
    let signal = match (row.rssi, narrow::<i8>(row.snr, "snr")?) {
        (Some(rssi), Some(snr)) => Some(Signal { rssi, snr }),
        _ => None,
    };
    Ok(Reception {
        id: row.id,
        at: micros(row.at_us, "received_at")?,
        copy: u32::try_from(row.copy).map_err(|_| bad("copy", row.copy, "negative"))?,
        path: row.path,
        frame: Frame {
            origin: row.origin,
            relays: row.relays,
            payload,
            signal,
            hw: narrow(row.hw, "hw")?,
            firmware: row.firmware,
            source: SourceKind::parse(&row.source)?,
            msg_id: narrow(row.msg_id, "msg_id")?,
        },
    })
}

pub(super) fn node(row: NodeRow) -> Result<Node> {
    Ok(Node {
        position: position(row.lat, row.lon, row.alt_m),
        hw: narrow(row.hw, "hw")?,
        frames: u64::try_from(row.frames).map_err(|_| bad("frames", row.frames, "negative"))?,
        last_seen: micros(row.last_seen_us, "last_seen")?,
        call: row.call,
        own: row.own,
        firmware: row.firmware,
    })
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TotalsRow {
    pub(super) frames: i64,
    pub(super) receptions: i64,
    pub(super) messages: i64,
    pub(super) direct: i64,
    pub(super) first_us: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TimelineRow {
    pub(super) start_us: i64,
    pub(super) message: i64,
    pub(super) position: i64,
    pub(super) hey: i64,
    pub(super) other: i64,
}

pub(super) struct StatsRows {
    pub(super) range: Range,
    pub(super) since: Option<u64>,
    pub(super) until: u64,
    pub(super) totals: TotalsRow,
    pub(super) kinds: Vec<(String, i64)>,
    pub(super) timeline: Vec<TimelineRow>,
    pub(super) hours: Vec<(i32, i64)>,
    pub(super) origins: Vec<(String, i64, i64)>,
    pub(super) relays: Vec<(String, i64, i64)>,
    pub(super) hops: Vec<(i32, i64)>,
    pub(super) snr: Vec<(i16, i64)>,
    pub(super) chat: Vec<(String, i64)>,
}

fn ranking(rows: Vec<(String, i64, i64)>) -> (u64, Vec<(String, i64)>) {
    let distinct = rows.first().map_or(0, |r| stats::count(r.2));
    (distinct, rows.into_iter().map(|(name, n, _)| (name, n)).collect())
}

pub(super) fn stats(r: StatsRows) -> Stats {
    let (stations, origins) = ranking(r.origins);
    let (relays, relay_rows) = ranking(r.relays);
    Stats {
        range: r.range,
        since: r.since,
        until: r.until,
        totals: Totals {
            frames: stats::count(r.totals.frames),
            receptions: stats::count(r.totals.receptions),
            messages: stats::count(r.totals.messages),
            stations,
            relays,
            direct: stats::count(r.totals.direct),
            first_seen: r.totals.first_us.and_then(|t| u64::try_from(t).ok()),
        },
        kinds: r.kinds.iter().fold(KindCounts::default(), |k, (kind, n)| {
            stats::add_kind(k, kind, stats::count(*n))
        }),
        timeline: r
            .timeline
            .into_iter()
            .filter_map(|t| {
                Some(TimeBucket {
                    start: u64::try_from(t.start_us).ok()?,
                    kinds: KindCounts {
                        message: stats::count(t.message),
                        position: stats::count(t.position),
                        hey: stats::count(t.hey),
                        other: stats::count(t.other),
                    },
                })
            })
            .collect(),
        hour_of_day: stats::hour_of_day(&r.hours),
        top_origins: stats::ranked(origins),
        top_relays: stats::ranked(relay_rows),
        hops: stats::hops(&r.hops),
        snr: r.snr.into_iter().map(|(s, n)| (s, stats::count(n))).collect(),
        chat: stats::ranked(r.chat.into_iter().map(|(d, n)| (stats::chat_label(&d), n)).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: Payload) -> Frame {
        Frame {
            origin: "BRV".into(),
            relays: vec!["ECH".into()],
            payload,
            signal: Some(Signal { rssi: -137, snr: -19 }),
            hw: Some(3),
            firmware: Some("4.40a".into()),
            source: SourceKind::Console,
            msg_id: Some(0xC52A_B2C2),
        }
    }

    fn row_of(f: &Frame) -> ReceptionRow {
        let c = columns(f);
        ReceptionRow {
            id: Uuid::from_u128(7),
            at_us: 1_794_564_903_123_456,
            source: c.source.into(),
            msg_id: c.msg_id,
            copy: 2,
            origin: c.origin,
            relays: c.relays,
            path: vec!["BRV".into(), "ECH".into(), "OWN".into()],
            rssi: c.rssi,
            snr: c.snr,
            hw: c.hw,
            firmware: c.firmware,
            kind: c.kind.into(),
            dest: c.dest,
            text: c.text,
            lat: c.lat,
            lon: c.lon,
            alt_m: c.alt_m,
            neighbours: c.neighbours,
            hops: c.hops,
            type_char: c.type_char,
        }
    }

    #[test]
    fn every_payload_round_trips() {
        let payloads = vec![
            Payload::Message {
                dest: "123".into(),
                text: "hello".into(),
            },
            Payload::Position(Position {
                lat: 10.2,
                lon: -30.1,
                alt_m: Some(57.0),
            }),
            Payload::Hey {
                neighbours: Some(7),
                hops: vec![HopReport {
                    hw: 3,
                    signal: Signal { rssi: -109, snr: -7 },
                }],
            },
            Payload::Other {
                kind: 'T',
                text: "raw".into(),
            },
        ];
        payloads.into_iter().for_each(|p| {
            let f = frame(p);
            let r = reception(row_of(&f)).unwrap();
            assert_eq!(r.frame, f);
            assert_eq!((r.id, r.at, r.copy), (Uuid::from_u128(7), 1_794_564_903_123_456, 2));
            assert_eq!(r.path, vec!["BRV", "ECH", "OWN"]);
        });
    }

    #[test]
    fn bad_rows_are_errors() {
        let f = frame(Payload::Position(Position {
            lat: 1.0,
            lon: 2.0,
            alt_m: None,
        }));
        let broken = [
            ReceptionRow {
                lat: None,
                ..row_of(&f)
            },
            ReceptionRow {
                kind: "nope".into(),
                ..row_of(&f)
            },
            ReceptionRow {
                source: "udp".into(),
                ..row_of(&f)
            },
            ReceptionRow {
                at_us: -1,
                ..row_of(&f)
            },
            ReceptionRow {
                hw: Some(300),
                ..row_of(&f)
            },
            ReceptionRow {
                snr: Some(200),
                ..row_of(&f)
            },
            ReceptionRow { copy: -1, ..row_of(&f) },
        ];
        broken.into_iter().for_each(|row| assert!(reception(row).is_err()));
    }

    #[test]
    fn target_suggestions() {
        let row = |v: &str| TargetRow {
            value: v.into(),
            messages: 3,
            last_us: Some(10),
        };
        assert_eq!(
            target_info(row("123"), true).unwrap().target,
            Target::Channel("123".into())
        );
        assert_eq!(
            target_info(row("qa0own-11"), false).unwrap().target,
            Target::Callsign("QA0OWN-11".into())
        );
        assert!(target_info(row("CALL"), true).is_none());
        assert!(target_info(row("not a call!"), false).is_none());
        let none = TargetRow {
            value: "A1A".into(),
            messages: 0,
            last_us: None,
        };
        assert_eq!(target_info(none, false).unwrap().last_seen, None);
    }

    #[test]
    fn nodes() {
        let row = NodeRow {
            call: "QA0OWN-11".into(),
            own: true,
            lat: Some(10.2),
            lon: Some(-30.1),
            alt_m: None,
            hw: Some(3),
            firmware: None,
            frames: 4,
            last_seen_us: 10,
        };
        let n = node(row).unwrap();
        assert_eq!(
            n.position,
            Some(Position {
                lat: 10.2,
                lon: -30.1,
                alt_m: None
            })
        );
        assert_eq!((n.own, n.hw, n.frames, n.last_seen), (true, Some(3), 4, 10));
        let no_pos = NodeRow {
            call: "X1X".into(),
            own: false,
            lat: Some(1.0),
            lon: None,
            alt_m: None,
            hw: None,
            firmware: None,
            frames: -1,
            last_seen_us: 0,
        };
        assert!(node(no_pos).is_err());
    }
    #[test]
    fn stats_rows_become_stats() {
        let s = stats(StatsRows {
            range: Range::Day,
            since: Some(10),
            until: 20,
            totals: TotalsRow {
                frames: 5,
                receptions: 8,
                messages: 2,
                direct: 3,
                first_us: Some(11),
            },
            kinds: vec![("message".into(), 2), ("position".into(), 3)],
            timeline: vec![
                TimelineRow {
                    start_us: 10,
                    message: 2,
                    position: 1,
                    hey: 0,
                    other: 0,
                },
                TimelineRow {
                    start_us: -1,
                    message: 0,
                    position: 0,
                    hey: 0,
                    other: 0,
                },
            ],
            hours: vec![(13, 5)],
            origins: vec![("AA1A".into(), 4, 2), ("BB1B".into(), 1, 2)],
            relays: vec![],
            hops: vec![(0, 3), (2, 2)],
            snr: vec![(-7, 4)],
            chat: vec![("*".into(), 1), ("12351".into(), 1)],
        });
        assert_eq!(
            (s.totals.frames, s.totals.stations, s.totals.relays, s.totals.first_seen),
            (5, 2, 0, Some(11))
        );
        assert_eq!((s.kinds.message, s.kinds.position), (2, 3));
        assert_eq!(s.timeline.len(), 1, "a negative start is dropped");
        assert_eq!(s.hour_of_day[13], 5);
        assert_eq!(s.top_origins[0].name, "AA1A");
        assert_eq!(s.hops, [3, 0, 2, 0, 0]);
        assert_eq!(s.snr, vec![(-7, 4)]);
        assert_eq!(
            s.chat.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["all", "12351"]
        );
    }
}
