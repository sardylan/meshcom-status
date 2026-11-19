use uuid::Uuid;

use crate::aprs::{self, Position};
use crate::errors::{Error, Result};

pub(crate) type Call = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Signal {
    pub(crate) rssi: i16,
    pub(crate) snr: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HopReport {
    pub(crate) hw: u8,
    pub(crate) signal: Signal,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Payload {
    Position(Position),
    Message {
        dest: String,
        text: String,
    },
    Hey {
        neighbours: Option<u32>,
        hops: Vec<HopReport>,
    },
    Other {
        kind: char,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Message,
    Position,
    Hey,
    Other,
}

impl Kind {
    pub(crate) const ALL: [Kind; 4] = [Kind::Message, Kind::Position, Kind::Hey, Kind::Other];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Message => "message",
            Kind::Position => "position",
            Kind::Hey => "hey",
            Kind::Other => "other",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| Error::Invalid {
                what: "frame kind",
                input: s.to_owned(),
                reason: "expected message, position, hey or other".into(),
            })
    }
}

impl Payload {
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Payload::Message { .. } => Kind::Message,
            Payload::Position(_) => Kind::Position,
            Payload::Hey { .. } => Kind::Hey,
            Payload::Other { .. } => Kind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Console,
    Kiss,
}

impl SourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SourceKind::Console => "console",
            SourceKind::Kiss => "kiss",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<SourceKind> {
        match s {
            "console" => Ok(SourceKind::Console),
            "kiss" => Ok(SourceKind::Kiss),
            _ => Err(Error::Invalid {
                what: "frame source",
                input: s.to_owned(),
                reason: "expected console or kiss".into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub(crate) origin: Call,
    pub(crate) relays: Vec<Call>,
    pub(crate) payload: Payload,
    pub(crate) signal: Option<Signal>,
    pub(crate) hw: Option<u8>,
    pub(crate) firmware: Option<String>,
    pub(crate) source: SourceKind,
    pub(crate) msg_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reception {
    pub(crate) id: Uuid,
    pub(crate) at: u64,
    pub(crate) copy: u32,
    pub(crate) path: Vec<Call>,
    pub(crate) frame: Frame,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OwnNode {
    pub(crate) call: Call,
    pub(crate) position: Option<Position>,
    pub(crate) groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Observation {
    Frame(Frame),
    Own(OwnNode),
}

pub(crate) fn call(input: &str) -> Result<Call> {
    let c = input.trim().to_ascii_uppercase();
    let ok = (3..=12).contains(&c.len())
        && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && c.matches('-').count() <= 1
        && !c.starts_with('-')
        && !c.ends_with('-');
    if ok {
        Ok(c)
    } else {
        Err(Error::Invalid {
            what: "callsign",
            input: input.to_owned(),
            reason: "not a callsign".into(),
        })
    }
}

pub(crate) fn path(input: &str) -> Result<(Call, Vec<Call>)> {
    let calls = input.split(',').map(call).collect::<Result<Vec<_>>>()?;
    calls
        .split_first()
        .map(|(o, r)| (o.clone(), r.to_vec()))
        .ok_or_else(|| Error::Invalid {
            what: "path",
            input: input.to_owned(),
            reason: "empty".into(),
        })
}

pub(crate) fn hey(payload: &str) -> Result<Payload> {
    let bad = |reason: &str| Error::Invalid {
        what: "HEY payload",
        input: payload.to_owned(),
        reason: reason.into(),
    };
    let mut parts = payload.split(';').filter(|p| !p.is_empty());
    let head = parts.next().ok_or_else(|| bad("empty"))?;
    let neighbours = head
        .strip_prefix('R')
        .ok_or_else(|| bad("missing R field"))?
        .split(',')
        .next()
        .map(str::parse::<u32>)
        .transpose()?;
    let hops = parts
        .map(|p| {
            let f = p.split(',').collect::<Vec<_>>();
            match f.as_slice() {
                [hw, rssi, snr] => Ok(HopReport {
                    hw: hw.trim().parse()?,
                    signal: Signal {
                        rssi: -rssi.trim().parse::<i16>()?.abs(),
                        snr: snr.trim().parse()?,
                    },
                }),
                _ => Err(bad("hop report is not hw,rssi,snr")),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Payload::Hey { neighbours, hops })
}

pub(crate) fn info(type_char: char, dest: &str, body: &str) -> Payload {
    let other = || Payload::Other {
        kind: type_char,
        text: body.to_owned(),
    };
    match type_char {
        '!' | '=' => aprs::position(body).map(Payload::Position).unwrap_or_else(|_| other()),
        ':' => Payload::Message {
            dest: dest.trim().to_owned(),
            text: body.to_owned(),
        },
        '@' => hey(body).unwrap_or_else(|_| other()),
        _ => other(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls() {
        assert_eq!(call(" qa0own-11 ").unwrap(), "QA0OWN-11");
        assert_eq!(call("QE0DLT").unwrap(), "QE0DLT");
        assert!(call("").is_err());
        assert!(call("A-B-C").is_err());
        assert!(call("QA0 OWN").is_err());
        assert!(call("-QA0OWN").is_err());
        assert!(call("THISISWAYTOOLONG").is_err());
    }

    #[test]
    fn paths() {
        let (o, r) = path("QC0BRV-10,QI0HTL-12,QB0ALF-12").unwrap();
        assert_eq!(o, "QC0BRV-10");
        assert_eq!(r, vec!["QI0HTL-12", "QB0ALF-12"]);
        assert_eq!(path("QB0ALF-12").unwrap().1, Vec::<Call>::new());
        assert!(path("").is_err());
        assert!(path("A1B,,C").is_err());
    }

    #[test]
    fn hey_with_hops() {
        let p = hey("R7;3,109,-7;3,137,-19;1,69,11;").unwrap();
        let s = |rssi, snr| Signal { rssi, snr };
        assert_eq!(
            p,
            Payload::Hey {
                neighbours: Some(7),
                hops: vec![
                    HopReport {
                        hw: 3,
                        signal: s(-109, -7)
                    },
                    HopReport {
                        hw: 3,
                        signal: s(-137, -19)
                    },
                    HopReport {
                        hw: 1,
                        signal: s(-69, 11)
                    },
                ],
            }
        );
    }

    #[test]
    fn hey_variants() {
        assert_eq!(
            hey("R12;").unwrap(),
            Payload::Hey {
                neighbours: Some(12),
                hops: vec![]
            }
        );
        assert_eq!(
            hey("R2,4,5;").unwrap(),
            Payload::Hey {
                neighbours: Some(2),
                hops: vec![]
            }
        );
        assert!(hey("").is_err());
        assert!(hey("X7;").is_err());
        assert!(hey("R7;3,109;").is_err());
        assert!(hey("R7;a,b,c;").is_err());
    }

    #[test]
    fn kind_and_source_names_round_trip() {
        Kind::ALL
            .into_iter()
            .for_each(|k| assert_eq!(Kind::parse(k.as_str()).unwrap(), k));
        assert!(Kind::parse("HEY").is_err());
        [SourceKind::Console, SourceKind::Kiss]
            .into_iter()
            .for_each(|s| assert_eq!(SourceKind::parse(s.as_str()).unwrap(), s));
        assert!(SourceKind::parse("udp").is_err());
        assert_eq!(hey("R1;").unwrap().kind(), Kind::Hey);
    }

    #[test]
    fn info_kinds() {
        assert!(matches!(info('!', "*", "1015.30N\\03009.84WL"), Payload::Position(_)));
        assert!(matches!(info('!', "*", "garbage"), Payload::Other { kind: '!', .. }));
        assert_eq!(
            info(':', "123      ", "hi"),
            Payload::Message {
                dest: "123".into(),
                text: "hi".into()
            }
        );
        assert!(matches!(info('@', "HG", "R7;"), Payload::Hey { .. }));
        assert_eq!(
            info('T', "", "x"),
            Payload::Other {
                kind: 'T',
                text: "x".into()
            }
        );
    }
}
