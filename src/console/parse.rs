use crate::errors::{Error, Result};
use crate::frame::{self, Frame, Signal, SourceKind};

const RX_FRAME: &str = "[MC-TEST] RX_FRAME ";
const DROPPED: &str = "[MC-TEST] CAPTURE_DROPPED ";
const NONCE: &str = "NONCE:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    RxFrame { signal: Signal, bytes: Vec<u8> },
    Dropped(u64),
    AuthRequired,
    Other,
}

fn bad(what: &'static str, input: &str, reason: &str) -> Error {
    Error::Invalid {
        what,
        input: input.to_owned(),
        reason: reason.to_owned(),
    }
}

fn field<'a>(line: &'a str, key: &str) -> Result<&'a str> {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
        .ok_or_else(|| bad("net console line", line, &format!("missing {key}=")))
}

fn hex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err(bad("hex", text, "odd length"));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(Error::from))
        .collect()
}

pub(crate) fn line(text: &str) -> Result<Line> {
    let text = text.trim_end_matches(['\r', '\n']);
    if let Some(rest) = text.strip_prefix(RX_FRAME) {
        let len: usize = field(rest, "len")?.parse()?;
        let signal = Signal {
            rssi: field(rest, "rssi")?.parse()?,
            snr: field(rest, "snr")?.parse()?,
        };
        let bytes = hex(field(rest, "hex")?)?;
        if bytes.len() != len {
            return Err(bad("net console line", text, "len does not match the hex bytes"));
        }
        Ok(Line::RxFrame { signal, bytes })
    } else if let Some(rest) = text.strip_prefix(DROPPED) {
        Ok(Line::Dropped(field(rest, "n")?.parse()?))
    } else if text.starts_with(NONCE) {
        Ok(Line::AuthRequired)
    } else {
        Ok(Line::Other)
    }
}

fn take_until(bytes: &[u8], stop: impl Fn(u8) -> bool) -> Option<(&[u8], &[u8])> {
    bytes
        .iter()
        .position(|&b| stop(b))
        .map(|i| (&bytes[..i], &bytes[i + 1..]))
}

fn printable(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| (0x20..=0x7E).contains(b))
}

pub(crate) fn decode(bytes: &[u8], signal: Signal) -> Result<Option<Frame>> {
    let dump = || bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let bad = |reason: &str| Error::Invalid {
        what: "MeshCom frame",
        input: dump(),
        reason: reason.to_owned(),
    };

    let type_byte = *bytes.first().ok_or_else(|| bad("empty"))?;
    if !b":!@".contains(&type_byte) {
        return Ok(None);
    }
    if bytes.len() < 16 {
        return Err(bad("too short"));
    }
    let after_header = &bytes[6..];
    let (path, rest) = take_until(after_header, |b| b == b'>').ok_or_else(|| bad("no '>' after the path"))?;
    let (dest, rest) =
        take_until(rest, |b| b == type_byte).ok_or_else(|| bad("no type character after the destination"))?;
    let (payload, trailer) = take_until(rest, |b| b == 0).ok_or_else(|| bad("payload not terminated"))?;
    if !printable(path) || !printable(dest) {
        return Err(bad("path or destination not printable"));
    }
    let [hw, _modulation, fcs_hi, fcs_lo, tail @ ..] = trailer else {
        return Err(bad("trailer too short"));
    };
    let fcs_end = bytes.len() - trailer.len() + 2;
    let sum: u32 = bytes[..fcs_end].iter().map(|&b| u32::from(b)).sum();
    if sum != u32::from(u16::from_be_bytes([*fcs_hi, *fcs_lo])) {
        return Err(bad("checksum mismatch"));
    }
    let firmware = match tail {
        [fw, _last_hw, sub, ..] if *fw > 0 && sub.is_ascii_alphanumeric() => Some(format!("4.{fw:02}{}", *sub as char)),
        _ => None,
    };

    let (origin, relays) = frame::path(&String::from_utf8_lossy(path))?;
    let payload = frame::info(
        type_byte as char,
        &String::from_utf8_lossy(dest),
        &String::from_utf8_lossy(payload),
    );
    Ok(Some(Frame {
        origin,
        relays,
        payload,
        signal: Some(signal),
        hw: Some(*hw),
        firmware,
        source: SourceKind::Console,
        msg_id: Some(u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{HopReport, Payload};

    const CAPTURE: &[u8] = include_bytes!("../../tests/fixtures/netconsole.log");

    fn s(rssi: i16, snr: i8) -> Signal {
        Signal { rssi, snr }
    }

    const HEY_LINE: &str = "[MC-TEST] RX_FRAME len=50 rssi=-137 snr=-19 hex=401D219845135143304252562D31302C5146304543482D31303E48474052373B332C3130392C2D373B0003880A1B2883617E\r\n";

    #[test]
    fn rx_frame_line() {
        let Line::RxFrame { signal, bytes } = line(HEY_LINE).unwrap() else {
            panic!("not a frame")
        };
        assert_eq!(signal, s(-137, -19));
        assert_eq!(bytes.len(), 50);
    }

    #[test]
    fn other_lines() {
        assert_eq!(
            line("[MC-TEST] CAPTURE_DROPPED n=3 serial_bytes=10").unwrap(),
            Line::Dropped(3)
        );
        assert_eq!(line("NONCE: 0123abcd").unwrap(), Line::AuthRequired);
        assert_eq!(line("[MC-SM] IDLE -> RX_LISTEN rc=0").unwrap(), Line::Other);
        assert_eq!(line("").unwrap(), Line::Other);
    }

    #[test]
    fn malformed_rx_frame_lines() {
        assert!(line("[MC-TEST] RX_FRAME len=2 rssi=-1 snr=1 hex=ABC").is_err());
        assert!(line("[MC-TEST] RX_FRAME len=3 rssi=-1 snr=1 hex=ABCD").is_err());
        assert!(line("[MC-TEST] RX_FRAME len=2 rssi=-1 hex=ABCD").is_err());
        assert!(line("[MC-TEST] RX_FRAME len=2 rssi=x snr=1 hex=ABCD").is_err());
        assert!(line("[MC-TEST] RX_FRAME len=2 rssi=-1 snr=1 hex=ZZZZ").is_err());
    }

    #[test]
    fn decodes_hey() {
        let Line::RxFrame { signal, bytes } = line(HEY_LINE).unwrap() else {
            panic!()
        };
        let f = decode(&bytes, signal).unwrap().unwrap();
        assert_eq!(f.origin, "QC0BRV-10");
        assert_eq!(f.relays, vec!["QF0ECH-10"]);
        assert_eq!(f.hw, Some(3));
        assert_eq!(f.firmware.as_deref(), Some("4.40a"));
        assert_eq!(f.msg_id, Some(0x4598_211D));
        assert_eq!(
            f.payload,
            Payload::Hey {
                neighbours: Some(7),
                hops: vec![HopReport {
                    hw: 3,
                    signal: s(-109, -7)
                }]
            }
        );
    }

    #[test]
    fn whole_capture_decodes() {
        let text = String::from_utf8_lossy(CAPTURE);
        let frames: Vec<Frame> = text
            .lines()
            .filter_map(|l| match line(l).unwrap() {
                Line::RxFrame { signal, bytes } => Some(decode(&bytes, signal).unwrap().unwrap()),
                _ => None,
            })
            .collect();
        assert_eq!(frames.len(), 16);
        let count = |pred: fn(&Payload) -> bool| frames.iter().filter(|f| pred(&f.payload)).count();
        assert_eq!(count(|p| matches!(p, Payload::Hey { .. })), 7);
        assert_eq!(count(|p| matches!(p, Payload::Message { .. })), 8);
        assert_eq!(count(|p| matches!(p, Payload::Position(_))), 1);
        assert!(frames.iter().any(|f| f.origin == "QJ0IND-99"));
    }

    #[test]
    fn checksum_mismatch_is_an_error() {
        let Line::RxFrame { signal, mut bytes } = line(HEY_LINE).unwrap() else {
            panic!()
        };
        bytes[10] ^= 0x01;
        assert!(decode(&bytes, signal).is_err());
    }

    #[test]
    fn unsupported_and_truncated_frames() {
        assert_eq!(decode(&[0x41, 1, 2, 3], s(0, 0)).unwrap(), None);
        assert!(decode(&[], s(0, 0)).is_err());
        assert!(decode(b"@12345", s(0, 0)).is_err());
        assert!(decode(b"@\x01\x02\x03\x04\x05ABCDEFGHIJKLMNOP", s(0, 0)).is_err());
    }
}
