use crate::errors::{Error, Result};
use crate::frame::{self, Call, Frame, Payload, Signal, SourceKind};

const FEND: u8 = 0xC0;
const FESC: u8 = 0xDB;
const TFEND: u8 = 0xDC;
const TFESC: u8 = 0xDD;

#[derive(Debug, Default)]
pub(crate) struct Deframer {
    buf: Vec<u8>,
    escape: bool,
}

impl Deframer {
    pub(crate) fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        data.iter()
            .filter_map(|&b| match (self.escape, b) {
                (true, _) => {
                    self.escape = false;
                    self.buf.push(match b {
                        TFEND => FEND,
                        TFESC => FESC,
                        other => other,
                    });
                    None
                }
                (false, FEND) => Some(std::mem::take(&mut self.buf)).filter(|f| !f.is_empty()),
                (false, FESC) => {
                    self.escape = true;
                    None
                }
                (false, other) => {
                    self.buf.push(other);
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KissFrame {
    Data {
        src: Call,
        digis: Vec<Call>,
        dest: Call,
        info: Vec<u8>,
    },
    Meta(Option<Signal>),
    SrcInfo(Call),
    Other,
}

fn bad(what: &'static str, bytes: &[u8], reason: &str) -> Error {
    Error::Invalid {
        what,
        input: bytes.iter().map(|b| format!("{b:02X}")).collect(),
        reason: reason.to_owned(),
    }
}

fn address(a: &[u8]) -> Result<Call> {
    let base: String = a[..6]
        .iter()
        .map(|&b| char::from(b >> 1))
        .collect::<String>()
        .trim_end()
        .to_owned();
    let ssid = (a[6] >> 1) & 0x0F;
    let text = if ssid == 0 { base } else { format!("{base}-{ssid}") };
    frame::call(&text)
}

fn ax25(bytes: &[u8]) -> Result<KissFrame> {
    let blocks: Vec<&[u8]> = bytes.chunks(7).take_while(|c| c.len() == 7).collect();
    let count = blocks
        .iter()
        .position(|b| b[6] & 0x01 == 1)
        .map(|i| i + 1)
        .ok_or_else(|| bad("AX.25 frame", bytes, "address field not terminated"))?;
    if count < 2 {
        return Err(bad("AX.25 frame", bytes, "fewer than two addresses"));
    }
    let rest = &bytes[count * 7..];
    if rest.len() < 2 || rest[0] != 0x03 || rest[1] != 0xF0 {
        return Err(bad("AX.25 frame", bytes, "not a UI frame"));
    }
    let mut calls = blocks[..count]
        .iter()
        .map(|b| address(b))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    match (calls.next(), calls.next()) {
        (Some(dest), Some(src)) => Ok(KissFrame::Data {
            src,
            digis: calls.collect(),
            dest,
            info: rest[2..].to_vec(),
        }),
        _ => Err(bad("AX.25 frame", bytes, "fewer than two addresses")),
    }
}

pub(crate) fn kiss(bytes: &[u8]) -> Result<KissFrame> {
    match bytes.split_first() {
        Some((0x00, rest)) => ax25(rest),
        Some((0x10, [snr, lo, hi, ..])) => {
            let (snr, rssi) = (*snr as i8, i16::from_le_bytes([*lo, *hi]));
            Ok(KissFrame::Meta(if snr == 0 && rssi == 99 {
                None
            } else {
                Some(Signal { rssi, snr })
            }))
        }
        Some((0x10, _)) => Err(bad("RxMeta frame", bytes, "too short")),
        Some((0x20, rest)) => Ok(KissFrame::SrcInfo(frame::call(&String::from_utf8_lossy(rest))?)),
        Some(_) => Ok(KissFrame::Other),
        None => Err(bad("KISS frame", bytes, "empty")),
    }
}

fn payload(info: &[u8]) -> Payload {
    let text = String::from_utf8_lossy(info);
    let mut chars = text.chars();
    match chars.next() {
        Some(':') => {
            let body = chars.as_str();
            body.split_once(':')
                .map(|(dest, text)| frame::info(':', dest, text))
                .unwrap_or_else(|| Payload::Other {
                    kind: ':',
                    text: body.to_owned(),
                })
        }
        Some(t) => frame::info(t, "", chars.as_str()),
        None => Payload::Other {
            kind: ' ',
            text: String::new(),
        },
    }
}

#[derive(Debug, Default)]
pub(crate) struct Assembler {
    src_info: Option<Call>,
    pending: Option<Frame>,
}

impl Assembler {
    pub(crate) fn push(&mut self, k: KissFrame) -> Vec<Frame> {
        match k {
            KissFrame::Data { src, digis, info, .. } => {
                let origin = self.src_info.take().unwrap_or(src);
                let frame = Frame {
                    origin,
                    relays: digis,
                    payload: payload(&info),
                    signal: None,
                    hw: None,
                    firmware: None,
                    source: SourceKind::Kiss,
                    msg_id: None,
                };
                self.pending.replace(frame).into_iter().collect()
            }
            KissFrame::Meta(signal) => self.pending.take().map(|f| Frame { signal, ..f }).into_iter().collect(),
            KissFrame::SrcInfo(call) => {
                self.src_info = Some(call);
                self.flush().into_iter().collect()
            }
            KissFrame::Other => Vec::new(),
        }
    }

    pub(crate) fn flush(&mut self) -> Option<Frame> {
        self.pending.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE: &[u8] = include_bytes!("../../tests/fixtures/kiss.bin");

    fn frames_of(capture: &[u8]) -> Vec<Frame> {
        let mut d = Deframer::default();
        let mut a = Assembler::default();
        let mut frames: Vec<Frame> = d.feed(capture).iter().flat_map(|f| a.push(kiss(f).unwrap())).collect();
        frames.extend(a.flush());
        frames
    }

    #[test]
    fn deframer_unescapes_and_splits() {
        let mut d = Deframer::default();
        assert_eq!(d.feed(&[FEND, 0x00, 0x01, FESC]), Vec::<Vec<u8>>::new());
        assert_eq!(
            d.feed(&[TFEND, FESC, TFESC, FEND, FEND]),
            vec![vec![0x00, 0x01, FEND, FESC]]
        );
    }

    #[test]
    fn whole_capture() {
        let frames = frames_of(CAPTURE);
        assert_eq!(frames.len(), 10);
        let first = &frames[0];
        assert_eq!(first.origin, "QF0ECH-10");
        assert_eq!(first.relays, vec!["QN0MKE-11", "QB0ALF-12"]);
        assert_eq!(first.signal, Some(Signal { rssi: -107, snr: 7 }));
        assert!(matches!(first.payload, Payload::Position(_)));
        assert_eq!(frames[1].origin, "QJ0IND-99");
        assert!(matches!(&frames[1].payload, Payload::Message { dest, .. } if dest == "123"));
        assert!(matches!(&frames[2].payload, Payload::Message { dest, .. } if dest.is_empty()));
    }

    #[test]
    fn meta_sentinel_means_not_measured() {
        assert_eq!(kiss(&[0x10, 0x00, 99, 0x00]).unwrap(), KissFrame::Meta(None));
        assert_eq!(
            kiss(&[0x10, 0x06, 0xD1, 0xFF]).unwrap(),
            KissFrame::Meta(Some(Signal { rssi: -47, snr: 6 }))
        );
        assert!(kiss(&[0x10, 0x06]).is_err());
    }

    #[test]
    fn other_and_bad_frames() {
        assert_eq!(kiss(&[0xF0, 0x01]).unwrap(), KissFrame::Other);
        assert!(kiss(&[]).is_err());
        assert!(kiss(&[0x00, 1, 2, 3]).is_err());
    }

    #[test]
    fn data_without_meta_is_released_by_the_next_frame() {
        let mut a = Assembler::default();
        let data = |src: &str| KissFrame::Data {
            src: src.into(),
            digis: vec![],
            dest: "APRSMC".into(),
            info: b":X        :hi".to_vec(),
        };
        assert!(a.push(data("AA1A")).is_empty());
        let out = a.push(data("BB1B"));
        assert_eq!((out.len(), out[0].origin.as_str(), out[0].signal), (1, "AA1A", None));
        assert_eq!(a.flush().unwrap().origin, "BB1B");
    }
}
