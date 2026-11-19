use std::io::{self, Read, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Identity,
    Brotli,
    Zstd,
    Gzip,
}

const PREFERENCE: [Encoding; 4] = [Encoding::Brotli, Encoding::Zstd, Encoding::Gzip, Encoding::Identity];

const BROTLI_QUALITY: i32 = 11;
const BROTLI_WINDOW_BITS: i32 = 24;
const ZSTD_LEVEL: i32 = 19;
const ZSTD_WINDOW_LOG: u32 = 23;

impl Encoding {
    pub(crate) const COUNT: usize = 4;

    pub(crate) fn token(self) -> &'static str {
        match self {
            Encoding::Identity => "identity",
            Encoding::Brotli => "br",
            Encoding::Zstd => "zstd",
            Encoding::Gzip => "gzip",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Encoding::Identity => 0,
            Encoding::Brotli => 1,
            Encoding::Zstd => 2,
            Encoding::Gzip => 3,
        }
    }

    fn matches(self, token: &str) -> bool {
        token.eq_ignore_ascii_case(self.token()) || (self == Encoding::Gzip && token.eq_ignore_ascii_case("x-gzip"))
    }
}

fn element(item: &str) -> (&str, f32) {
    let mut parts = item.split(';').map(str::trim);
    let token = parts.next().unwrap_or_default();
    let q = parts
        .find_map(|p| p.strip_prefix("q=").or_else(|| p.strip_prefix("Q=")))
        .and_then(|q| q.parse::<f32>().ok())
        .map_or(1.0, |q| q.clamp(0.0, 1.0));
    (token, q)
}

fn weight(elements: &[(&str, f32)], encoding: Encoding) -> f32 {
    let own = elements.iter().find(|(t, _)| encoding.matches(t)).map(|e| e.1);
    let star = elements.iter().find(|(t, _)| *t == "*").map(|e| e.1);
    own.or(star).unwrap_or(if encoding == Encoding::Identity {
        f32::MIN_POSITIVE
    } else {
        0.0
    })
}

pub(crate) fn negotiate(accept_encoding: Option<&str>) -> Encoding {
    let Some(header) = accept_encoding else {
        return Encoding::Identity;
    };
    let elements: Vec<(&str, f32)> = header.split(',').map(element).filter(|(t, _)| !t.is_empty()).collect();
    PREFERENCE
        .iter()
        .map(|&e| (e, weight(&elements, e)))
        .filter(|&(_, q)| q > 0.0)
        .fold(None, |best: Option<(Encoding, f32)>, (e, q)| match best {
            Some((_, b)) if b >= q => best,
            _ => Some((e, q)),
        })
        .map_or(Encoding::Identity, |(e, _)| e)
}

pub(crate) fn compressible(mime: &str) -> bool {
    let mime = mime.split(';').next().unwrap_or_default().trim();
    mime.starts_with("text/")
        || mime == "image/svg+xml"
        || mime == "application/wasm"
        || ["javascript", "json", "xml"].iter().any(|t| mime.contains(t))
}

pub(crate) fn encode(encoding: Encoding, data: &[u8]) -> io::Result<Vec<u8>> {
    match encoding {
        Encoding::Identity => Ok(data.to_vec()),
        Encoding::Brotli => {
            let params = brotli::enc::BrotliEncoderParams {
                quality: BROTLI_QUALITY,
                lgwin: BROTLI_WINDOW_BITS,
                size_hint: data.len(),
                ..Default::default()
            };
            let mut out = Vec::new();
            brotli::BrotliCompress(&mut &data[..], &mut out, &params)?;
            Ok(out)
        }
        Encoding::Zstd => {
            let mut compressor = zstd::bulk::Compressor::new(ZSTD_LEVEL)?;
            compressor.set_parameter(zstd::zstd_safe::CParameter::WindowLog(ZSTD_WINDOW_LOG))?;
            compressor.compress(data)
        }
        Encoding::Gzip => {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(data)?;
            encoder.finish()
        }
    }
}

pub(crate) fn decode(encoding: Encoding, data: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    match encoding {
        Encoding::Identity => out.extend_from_slice(data),
        Encoding::Brotli => {
            brotli::BrotliDecompress(&mut &data[..], &mut out)?;
        }
        Encoding::Zstd => out = zstd::stream::decode_all(data)?,
        Encoding::Gzip => {
            flate2::read::GzDecoder::new(data).read_to_end(&mut out)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Encoding; 4] = PREFERENCE;

    #[test]
    fn no_header_or_nothing_acceptable_is_identity() {
        assert_eq!(negotiate(None), Encoding::Identity);
        assert_eq!(negotiate(Some("")), Encoding::Identity);
        assert_eq!(negotiate(Some("deflate, compress")), Encoding::Identity);
        assert_eq!(negotiate(Some("br;q=0, gzip;q=0")), Encoding::Identity);
        assert_eq!(negotiate(Some("*;q=0")), Encoding::Identity);
    }

    #[test]
    fn ties_prefer_the_smallest_coding() {
        assert_eq!(negotiate(Some("gzip, deflate, br, zstd")), Encoding::Brotli);
        assert_eq!(negotiate(Some("gzip, zstd")), Encoding::Zstd);
        assert_eq!(negotiate(Some("gzip")), Encoding::Gzip);
        assert_eq!(negotiate(Some("x-gzip")), Encoding::Gzip);
        assert_eq!(negotiate(Some("*")), Encoding::Brotli);
    }

    #[test]
    fn weights_win_over_preference() {
        assert_eq!(negotiate(Some("br;q=0.5, gzip;q=1.0")), Encoding::Gzip);
        assert_eq!(negotiate(Some("BR;Q=0.9 , Zstd;q=0.95")), Encoding::Zstd);
        assert_eq!(negotiate(Some("*;q=0.1, identity;q=0.5")), Encoding::Identity);
        assert_eq!(negotiate(Some("*, br;q=0")), Encoding::Zstd);
        assert_eq!(negotiate(Some("gzip;q=nope")), Encoding::Gzip);
        assert_eq!(negotiate(Some("gzip;q=0.001")), Encoding::Gzip);
        assert_eq!(negotiate(Some("gzip;q=0.5, identity")), Encoding::Identity);
    }

    #[test]
    fn every_coding_round_trips() {
        let data = "function f(){return 42}\n".repeat(500).into_bytes();
        for e in ALL {
            let packed = encode(e, &data).unwrap();
            if e != Encoding::Identity {
                assert!(packed.len() < data.len() / 10, "{e:?}: {}", packed.len());
            }
            assert_eq!(decode(e, &packed).unwrap(), data, "{e:?}");
        }
    }

    #[test]
    fn corrupt_input_is_an_error() {
        for e in [Encoding::Brotli, Encoding::Zstd, Encoding::Gzip] {
            assert!(decode(e, b"definitely not compressed").is_err(), "{e:?}");
        }
    }

    #[test]
    fn indexes_and_tokens_are_distinct() {
        let mut seen = [false; Encoding::COUNT];
        for e in ALL {
            assert!(!seen[e.index()]);
            seen[e.index()] = true;
        }
        assert_eq!(ALL.map(Encoding::token), ["br", "zstd", "gzip", "identity"]);
    }

    #[test]
    fn text_formats_are_compressible() {
        for m in [
            "text/html",
            "text/css",
            "text/javascript",
            "application/javascript",
            "application/json",
            "image/svg+xml",
            "text/html; charset=utf-8",
        ] {
            assert!(compressible(m), "{m}");
        }
        for m in ["image/png", "font/woff2", "image/jpeg", "application/octet-stream"] {
            assert!(!compressible(m), "{m}");
        }
    }
}
