use crate::errors::{Error, Result};

fn bad(input: &[u8], reason: &str) -> Error {
    Error::Invalid {
        what: "node HTTP response",
        input: String::from_utf8_lossy(input).chars().take(80).collect(),
        reason: reason.to_owned(),
    }
}

pub(crate) fn response(raw: &[u8]) -> Result<(u16, &[u8])> {
    let status_end = raw
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| bad(raw, "no status line"))?;
    let status_line = String::from_utf8_lossy(&raw[..status_end]);
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .filter(|_| status_line.starts_with("HTTP/1."))
        .ok_or_else(|| bad(raw, "malformed status line"))?
        .parse()?;
    let body_start = (status_end..raw.len())
        .find_map(|i| match &raw[i..] {
            [b'\n', b'\r', b'\n', ..] => Some(i + 3),
            [b'\n', b'\n', ..] => Some(i + 2),
            _ => None,
        })
        .ok_or_else(|| bad(raw, "header not terminated"))?;
    Ok((code, &raw[body_start..]))
}

pub(crate) fn getparam(raw: &[u8], name: &str) -> Result<String> {
    let (code, body) = response(raw)?;
    let json: serde_json::Value = serde_json::from_slice(body)?;
    let returncode = json.get("returncode").and_then(serde_json::Value::as_i64);
    match (code, returncode, json.get(name).and_then(serde_json::Value::as_str)) {
        (200, Some(0), Some(value)) => Ok(value.to_owned()),
        _ => Err(bad(raw, &format!("parameter {name} not returned"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETCALL: &[u8] = include_bytes!("../../tests/fixtures/getparam_setcall.http");

    #[test]
    fn real_node_response() {
        let (code, body) = response(SETCALL).unwrap();
        assert_eq!(code, 200);
        assert!(body.starts_with(b"{\"returncode\":0"));
        assert_eq!(getparam(SETCALL, "setcall").unwrap(), "QA0OWN-11");
    }

    #[test]
    fn standard_response() {
        let raw =
            b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\n\r\n{\"returncode\":2,\"x\":\"\"}";
        assert_eq!(response(raw).unwrap().0, 422);
        assert!(getparam(raw, "x").is_err());
    }

    #[test]
    fn malformed_responses() {
        assert!(response(b"").is_err());
        assert!(response(b"garbage\n\n").is_err());
        assert!(response(b"HTTP/1.1 abc OK\n\n").is_err());
        assert!(response(b"HTTP/1.1 200 OK\nX: y\r\n").is_err());
        assert!(getparam(b"HTTP/1.1 200 OK\n\nnot json", "x").is_err());
        assert!(getparam(b"HTTP/1.1 200 OK\n\n{\"returncode\":0}", "x").is_err());
    }
}
