use crate::errors::{Error, Result};
use crate::frame::{Kind, Reception};

pub(crate) const LIMITS: [u32; 5] = [10, 30, 100, 500, 1000];
pub(crate) const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) limit: Option<u32>,
    pub(crate) kinds: Vec<Kind>,
    pub(crate) copies: bool,
}

impl Default for View {
    fn default() -> View {
        View {
            limit: Some(DEFAULT_LIMIT),
            kinds: Kind::ALL.to_vec(),
            copies: true,
        }
    }
}

fn bad(what: &'static str, input: &str, reason: &str) -> Error {
    Error::Invalid {
        what,
        input: input.to_owned(),
        reason: reason.to_owned(),
    }
}

fn check_limit(limit: Option<u32>) -> Result<Option<u32>> {
    match limit {
        Some(n) if !(1..=MAX_LIMIT).contains(&n) => Err(bad("limit", &n.to_string(), "must be between 1 and 100000")),
        other => Ok(other),
    }
}

impl View {
    pub(crate) fn new(limit: Option<u32>, kinds: &[impl AsRef<str>], copies: bool) -> Result<View> {
        let mut kinds = kinds
            .iter()
            .map(|k| Kind::parse(k.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        kinds.sort();
        kinds.dedup();
        Ok(View {
            limit: check_limit(limit)?,
            kinds,
            copies,
        })
    }

    pub(crate) fn from_query(limit: Option<&str>, kinds: Option<&str>, copies: Option<&str>) -> Result<View> {
        let d = View::default();
        let limit = match limit {
            None => d.limit,
            Some("all") => None,
            Some(n) => Some(
                n.parse::<u32>()
                    .map_err(|_| bad("limit", n, "must be a number or all"))?,
            ),
        };
        let kinds: Vec<&str> = match kinds {
            None => Kind::ALL.iter().map(|k| k.as_str()).collect(),
            Some(list) => list.split(',').map(str::trim).filter(|k| !k.is_empty()).collect(),
        };
        let copies = match copies {
            None => d.copies,
            Some("true") => true,
            Some("false") => false,
            Some(other) => return Err(bad("copies", other, "must be true or false")),
        };
        View::new(limit, &kinds, copies)
    }

    pub(crate) fn matches(&self, r: &Reception) -> bool {
        self.kinds.contains(&r.frame.payload.kind()) && (self.copies || r.copy <= 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, Payload, SourceKind};

    fn rx(payload: Payload, copy: u32) -> Reception {
        Reception {
            id: uuid::Uuid::from_u128(1),
            at: 1,
            copy,
            path: vec!["A1A".into()],
            frame: Frame {
                origin: "A1A".into(),
                relays: vec![],
                payload,
                signal: None,
                hw: None,
                firmware: None,
                source: SourceKind::Console,
                msg_id: None,
            },
        }
    }

    #[test]
    fn defaults() {
        assert_eq!(View::from_query(None, None, None).unwrap(), View::default());
        assert_eq!(View::default().limit, Some(30));
    }

    #[test]
    fn query_values() {
        let v = View::from_query(Some("all"), Some("hey, message,hey"), Some("false")).unwrap();
        assert_eq!(
            v,
            View {
                limit: None,
                kinds: vec![Kind::Message, Kind::Hey],
                copies: false
            }
        );
        assert_eq!(View::from_query(None, Some(""), None).unwrap().kinds, vec![]);
        assert_eq!(View::from_query(Some("100"), None, None).unwrap().limit, Some(100));
    }

    #[test]
    fn bad_query_values() {
        assert!(View::from_query(Some("0"), None, None).is_err());
        assert!(View::from_query(Some("100001"), None, None).is_err());
        assert!(View::from_query(Some("x"), None, None).is_err());
        assert!(View::from_query(None, Some("hey,udp"), None).is_err());
        assert!(View::from_query(None, None, Some("yes")).is_err());
    }

    #[test]
    fn matching() {
        let hey = || Payload::Hey {
            neighbours: None,
            hops: vec![],
        };
        let v = View::new(Some(5), &["hey"], false).unwrap();
        assert!(v.matches(&rx(hey(), 1)));
        assert!(!v.matches(&rx(hey(), 2)));
        assert!(!v.matches(&rx(
            Payload::Other {
                kind: 'T',
                text: String::new()
            },
            1
        )));
        assert!(View::default().matches(&rx(hey(), 3)));
    }
}
