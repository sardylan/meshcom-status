use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Position {
    pub(crate) lat: f64,
    pub(crate) lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) alt_m: Option<f64>,
}

fn bad(input: &str, reason: &str) -> Error {
    Error::Invalid {
        what: "APRS position",
        input: input.to_owned(),
        reason: reason.to_owned(),
    }
}

fn coordinate(text: &str, deg_digits: usize, positive: char, negative: char, max: f64) -> Result<f64> {
    let (number, hemisphere) = text.split_at(text.len() - 1);
    let sign = match hemisphere.chars().next() {
        Some(h) if h == positive => 1.0,
        Some(h) if h == negative => -1.0,
        _ => return Err(bad(text, "bad hemisphere")),
    };
    let degrees: f64 = number
        .get(..deg_digits)
        .ok_or_else(|| bad(text, "too short"))?
        .parse()?;
    let minutes: f64 = number
        .get(deg_digits..)
        .ok_or_else(|| bad(text, "too short"))?
        .parse()?;
    let value = degrees + minutes / 60.0;
    if minutes >= 60.0 || value > max {
        Err(bad(text, "out of range"))
    } else {
        Ok(sign * value)
    }
}

fn altitude(comment: &str) -> Option<f64> {
    comment
        .find("/A=")
        .and_then(|i| comment.get(i + 3..i + 9))
        .and_then(|digits| digits.parse::<f64>().ok())
        .map(|feet| (feet * 0.3048 * 10.0).round() / 10.0)
}

pub(crate) fn position(body: &str) -> Result<Position> {
    let lat = body.get(0..8).ok_or_else(|| bad(body, "too short"))?;
    let lon = body.get(9..18).ok_or_else(|| bad(body, "too short"))?;
    let comment = body.get(19..).unwrap_or("");
    Ok(Position {
        lat: coordinate(lat, 2, 'N', 'S', 90.0)?,
        lon: coordinate(lon, 3, 'E', 'W', 180.0)?,
        alt_m: altitude(comment),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn meshcom_beacon() {
        let p = position("1015.30N\\03009.84WLTest Station 1#QL0KIL/B=100/A=000187/R=123;12302;").unwrap();
        assert!(close(p.lat, 10.255), "{p:?}");
        assert!(close(p.lon, -30.164), "{p:?}");
        assert_eq!(p.alt_m, Some(57.0));
    }

    #[test]
    fn southern_western_and_no_altitude() {
        let p = position("3351.00S/15112.00W#").unwrap();
        assert!(close(p.lat, -33.85));
        assert!(close(p.lon, -151.2));
        assert_eq!(p.alt_m, None);
    }

    #[test]
    fn negative_altitude() {
        assert_eq!(position("1015.30N/03009.84W#/A=-00010").unwrap().alt_m, Some(-3.0));
    }

    #[test]
    fn rejects_malformed() {
        assert!(position("").is_err());
        assert!(position("3915.30X/03009.84W#").is_err());
        assert!(position("1075.30N/03009.84W#").is_err());
        assert!(position("9915.30N/03009.84W#").is_err());
        assert!(position("39AB.30N/03009.84W#").is_err());
        assert!(position("1015.30N/00909.84Q#").is_err());
    }
}
