use serde::Deserialize;

use crate::aprs::Position;
use crate::clock;
use crate::errors::{Error, Result};
use crate::frame::{self, Call, Frame, HopReport, Kind, Payload, Signal, SourceKind};
use crate::graph::Node;

#[derive(Debug, Deserialize)]
struct ImportFile {
    #[serde(default)]
    nodes: Vec<ImportNode>,
    #[serde(default)]
    frames: Vec<ImportFrame>,
}

#[derive(Debug, Deserialize)]
struct ImportNode {
    call: String,
    own: bool,
    position: Option<Position>,
    hw: Option<u8>,
    firmware: Option<String>,
    frames: u64,
    last_seen: String,
}

#[derive(Debug, Deserialize)]
struct ImportHop {
    hw: u8,
    rssi: i16,
    snr: i8,
}

#[derive(Debug, Deserialize)]
struct ImportFrame {
    received_at: String,
    source: String,
    msg_id: Option<String>,
    origin: String,
    relays: Vec<String>,
    path: Vec<String>,
    rssi: Option<i16>,
    snr: Option<i8>,
    hw: Option<u8>,
    firmware: Option<String>,
    kind: String,
    dest: Option<String>,
    text: Option<String>,
    position: Option<Position>,
    neighbours: Option<u32>,
    hops: Option<Vec<ImportHop>>,
    type_char: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportedReception {
    pub(crate) at: u64,
    pub(crate) path: Vec<Call>,
    pub(crate) frame: Frame,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Imported {
    pub(crate) nodes: Vec<Node>,
    pub(crate) receptions: Vec<ImportedReception>,
}

fn calls(list: &[String]) -> Result<Vec<Call>> {
    list.iter().map(|c| frame::call(c)).collect()
}

fn node(n: ImportNode) -> Result<Node> {
    Ok(Node {
        call: frame::call(&n.call)?,
        own: n.own,
        position: n.position,
        hw: n.hw,
        firmware: n.firmware,
        frames: n.frames,
        last_seen: clock::parse_rfc3339(&n.last_seen)?,
    })
}

fn payload(f: &ImportFrame) -> Result<Payload> {
    Ok(match Kind::parse(&f.kind)? {
        Kind::Message => Payload::Message {
            dest: f.dest.clone().unwrap_or_default(),
            text: f.text.clone().unwrap_or_default(),
        },
        Kind::Position => Payload::Position(f.position.ok_or_else(|| Error::Invalid {
            what: "imported position frame",
            input: f.received_at.clone(),
            reason: "no position".into(),
        })?),
        Kind::Hey => Payload::Hey {
            neighbours: f.neighbours,
            hops: f
                .hops
                .iter()
                .flatten()
                .map(|h| HopReport {
                    hw: h.hw,
                    signal: Signal {
                        rssi: h.rssi,
                        snr: h.snr,
                    },
                })
                .collect(),
        },
        Kind::Other => Payload::Other {
            kind: f.type_char.as_deref().and_then(|t| t.chars().next()).unwrap_or(' '),
            text: f.text.clone().unwrap_or_default(),
        },
    })
}

fn reception(f: ImportFrame) -> Result<ImportedReception> {
    let msg_id = f.msg_id.as_deref().map(|id| u32::from_str_radix(id, 16)).transpose()?;
    let signal = f.rssi.zip(f.snr).map(|(rssi, snr)| Signal { rssi, snr });
    Ok(ImportedReception {
        at: clock::parse_rfc3339(&f.received_at)?,
        path: calls(&f.path)?,
        frame: Frame {
            origin: frame::call(&f.origin)?,
            relays: calls(&f.relays)?,
            payload: payload(&f)?,
            signal,
            hw: f.hw,
            firmware: f.firmware,
            source: SourceKind::parse(&f.source)?,
            msg_id,
        },
    })
}

pub(crate) fn parse(text: &str) -> Result<Imported> {
    let file: ImportFile = serde_json::from_str(text)?;
    let nodes = file.nodes.into_iter().map(node).collect::<Result<Vec<_>>>()?;
    let mut receptions = file.frames.into_iter().map(reception).collect::<Result<Vec<_>>>()?;
    receptions.sort_by_key(|r| r.at);
    Ok(Imported { nodes, receptions })
}

pub(crate) fn merge_node(current: Option<&Node>, imported: Node) -> Node {
    match current {
        None => imported,
        Some(c) => Node {
            call: c.call.clone(),
            own: c.own || imported.own,
            position: c.position.or(imported.position),
            hw: c.hw.or(imported.hw),
            firmware: c.firmware.clone().or(imported.firmware),
            frames: c.frames.max(imported.frames),
            last_seen: c.last_seen.max(imported.last_seen),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPORT: &str = r#"{
      "nodes": [
        {"call":"QA0OWN-11","own":true,"position":{"lat":10.23462,"lon":-30.1933,"alt_m":13.0},"hw":null,
         "firmware":null,"frames":0,"last_seen":"2026-11-18T12:00:13Z"}
      ],
      "frames": [
        {"received_at":"2026-11-18T12:20:11.500000Z","source":"console","msg_id":"B05C914F","origin":"QD0CHL-10",
         "relays":[],"path":["QD0CHL-10","QA0OWN-11"],"rssi":-127,"snr":-10,"hw":52,"firmware":"4.40a",
         "kind":"hey","neighbours":2,"hops":[]},
        {"received_at":"2026-11-18T12:15:18.060634Z","source":"console","msg_id":"6AFCB829","origin":"QE0DLT-22",
         "relays":["QB0ALF-12"],"path":["QE0DLT-22","QB0ALF-12","QA0OWN-11"],"rssi":-106,"snr":6,"hw":0,
         "firmware":null,"kind":"message","dest":"*","text":"{UTC}2026-11-18 12:15:17"},
        {"received_at":"2026-11-18T12:12:08.827125Z","source":"console","msg_id":"C52AB2C2","origin":"QA0OWN-10",
         "relays":["QA0OWN-11"],"path":["QA0OWN-10","QA0OWN-11"],"rssi":null,"snr":null,"hw":3,"firmware":"4.40a",
         "kind":"position","position":{"lat":10.2346,"lon":-30.1933,"alt_m":53.9}},
        {"received_at":"2026-11-18T12:12:00Z","source":"kiss","origin":"QA0OWN-10","relays":[],"path":["QA0OWN-10"],
         "rssi":null,"snr":null,"hw":null,"firmware":null,"kind":"other","type_char":"T","text":"x"}
      ]
    }"#;

    #[test]
    fn parses_an_export() {
        let imported = parse(EXPORT).unwrap();
        assert_eq!(imported.nodes.len(), 1);
        assert!(imported.nodes[0].own);
        assert_eq!(imported.receptions.len(), 4);
        assert_eq!(
            imported.receptions[0].frame.payload,
            Payload::Other {
                kind: 'T',
                text: "x".into()
            }
        );
        let hey = &imported.receptions[3];
        assert_eq!(hey.frame.msg_id, Some(0xB05C_914F));
        assert_eq!(hey.frame.signal, Some(Signal { rssi: -127, snr: -10 }));
        assert_eq!(
            hey.frame.payload,
            Payload::Hey {
                neighbours: Some(2),
                hops: vec![]
            }
        );
        assert_eq!(hey.path, vec!["QD0CHL-10", "QA0OWN-11"]);
        assert!(matches!(imported.receptions[1].frame.payload, Payload::Position(_)));
        assert_eq!(imported.receptions[1].frame.signal, None);
    }

    #[test]
    fn empty_and_bad_exports() {
        assert_eq!(
            parse("{}").unwrap(),
            Imported {
                nodes: vec![],
                receptions: vec![]
            }
        );
        assert!(parse("42").is_err());
        let bad_kind = EXPORT.replace("\"kind\":\"hey\"", "\"kind\":\"beacon\"");
        assert!(parse(&bad_kind).is_err());
        let bad_time = EXPORT.replace("2026-11-18T12:00:13Z", "yesterday");
        assert!(parse(&bad_time).is_err());
        let bad_id = EXPORT.replace("B05C914F", "XYZ");
        assert!(parse(&bad_id).is_err());
        let no_position = EXPORT.replace(",\"position\":{\"lat\":10.2346,\"lon\":-30.1933,\"alt_m\":53.9}", "");
        assert!(parse(&no_position).is_err());
    }

    #[test]
    fn merging_keeps_the_most_complete_node() {
        let pos = Position {
            lat: 1.0,
            lon: 2.0,
            alt_m: None,
        };
        let imported = Node {
            call: "A1A".into(),
            own: false,
            position: Some(pos),
            hw: Some(3),
            firmware: None,
            frames: 5,
            last_seen: 10,
        };
        assert_eq!(merge_node(None, imported.clone()), imported);
        let current = Node {
            call: "A1A".into(),
            own: true,
            position: None,
            hw: None,
            firmware: Some("4.40a".into()),
            frames: 2,
            last_seen: 20,
        };
        let merged = merge_node(Some(&current), imported);
        assert_eq!(
            merged,
            Node {
                call: "A1A".into(),
                own: true,
                position: Some(pos),
                hw: Some(3),
                firmware: Some("4.40a".into()),
                frames: 5,
                last_seen: 20
            }
        );
    }
}
