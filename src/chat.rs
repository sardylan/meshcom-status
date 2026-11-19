use std::fmt;

use crate::errors::{Error, Result};
use crate::frame::{self, Call, Payload, Reception};

pub(crate) const PAGE: u32 = 20;
pub(crate) const MAX_PAGE: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    Broadcast,
    Channel(String),
    Callsign(Call),
}

pub(crate) fn is_group(dest: &str) -> bool {
    !dest.is_empty() && dest.len() <= 6 && dest.bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn is_broadcast(dest: &str) -> bool {
    dest.is_empty() || dest == "*"
}

pub(crate) fn is_ack(text: &str) -> bool {
    let t = text.trim();
    let digits = t.strip_prefix("ack").or_else(|| t.strip_prefix("rej"));
    digits.is_some_and(|d| (1..=5).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_alphanumeric()))
}

impl Target {
    pub(crate) fn parse(text: &str) -> Result<Target> {
        let bad = |reason: &str| Error::Invalid {
            what: "chat target",
            input: text.to_owned(),
            reason: reason.into(),
        };
        match text.split_once(':') {
            None if text == "broadcast" => Ok(Target::Broadcast),
            Some(("channel", g)) if is_group(g) => Ok(Target::Channel(g.to_owned())),
            Some(("channel", _)) => Err(bad("a channel is a group number of up to 6 digits")),
            Some(("callsign", c)) => frame::call(c).map(Target::Callsign),
            _ => Err(bad("expected broadcast, channel:<group> or callsign:<call>")),
        }
    }

    pub(crate) fn query_parts(&self) -> (&'static str, &str) {
        match self {
            Target::Broadcast => ("broadcast", ""),
            Target::Channel(g) => ("channel", g),
            Target::Callsign(c) => ("callsign", c),
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Target::Broadcast => "all".to_owned(),
            Target::Channel(g) => g.clone(),
            Target::Callsign(c) => c.clone(),
        }
    }

    pub(crate) fn matches(&self, r: &Reception) -> bool {
        let Payload::Message { dest, text } = &r.frame.payload else {
            return false;
        };
        if is_ack(text) {
            return false;
        }
        match self {
            Target::Broadcast => is_broadcast(dest),
            Target::Channel(g) => dest == g,
            Target::Callsign(c) => dest == c || (r.frame.origin == *c && !is_group(dest) && !is_broadcast(dest)),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Broadcast => f.write_str("broadcast"),
            Target::Channel(g) => write!(f, "channel:{g}"),
            Target::Callsign(c) => write!(f, "callsign:{c}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChatMessage {
    pub(crate) first: Reception,
    pub(crate) copies: Vec<Reception>,
}

const COPY_WINDOW_US: u64 = 3_600_000_000;

pub(crate) fn group(firsts: Vec<Reception>, mut copies: Vec<Reception>) -> Vec<ChatMessage> {
    copies.sort_by_key(|c| c.at);
    let mut messages: Vec<ChatMessage> = firsts
        .into_iter()
        .map(|first| ChatMessage { first, copies: vec![] })
        .collect();
    copies.into_iter().for_each(|c| {
        let owner = messages.iter_mut().rev().find(|m| {
            m.first.frame.msg_id.is_some()
                && m.first.frame.msg_id == c.frame.msg_id
                && m.first.at <= c.at
                && c.at - m.first.at <= COPY_WINDOW_US
        });
        if let Some(m) = owner {
            m.copies.push(c);
        }
    });
    messages
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TargetInfo {
    pub(crate) target: Target,
    pub(crate) messages: u64,
    pub(crate) last_seen: Option<u64>,
}

pub(crate) fn rank(mut infos: Vec<TargetInfo>, exclude: &[Target], limit: usize) -> Vec<TargetInfo> {
    infos.retain(|i| !exclude.contains(&i.target));
    infos.sort_by(|a, b| {
        let is_channel = |t: &TargetInfo| matches!(t.target, Target::Channel(_));
        is_channel(b)
            .cmp(&is_channel(a))
            .then(b.last_seen.cmp(&a.last_seen))
            .then(a.target.cmp(&b.target))
    });
    infos.dedup_by(|a, b| a.target == b.target);
    infos.truncate(limit);
    infos
}

pub(crate) fn node_groups(setgrc: &str) -> Vec<String> {
    let mut groups: Vec<String> = setgrc
        .split(';')
        .map(str::trim)
        .filter(|g| is_group(g) && !g.trim_start_matches('0').is_empty())
        .map(str::to_owned)
        .collect();
    groups.dedup();
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, SourceKind};
    use uuid::Uuid;

    fn msg(id: u128, at: u64, copy: u32, msg_id: Option<u32>, origin: &str, dest: &str, text: &str) -> Reception {
        Reception {
            id: Uuid::from_u128(id),
            at,
            copy,
            path: vec![origin.into()],
            frame: Frame {
                origin: origin.into(),
                relays: vec![],
                payload: Payload::Message {
                    dest: dest.into(),
                    text: text.into(),
                },
                signal: None,
                hw: None,
                firmware: None,
                source: SourceKind::Console,
                msg_id,
            },
        }
    }

    #[test]
    fn targets_parse_and_print() {
        ["broadcast", "channel:123", "callsign:QA0OWN-11"]
            .into_iter()
            .for_each(|t| {
                assert_eq!(Target::parse(t).unwrap().to_string(), t);
            });
        assert_eq!(
            Target::parse("callsign:qa0own-11").unwrap(),
            Target::Callsign("QA0OWN-11".into())
        );
        [
            "",
            "all",
            "channel:",
            "channel:abc",
            "channel:1234567",
            "callsign:",
            "group:123",
        ]
        .into_iter()
        .for_each(|t| assert!(Target::parse(t).is_err(), "{t}"));
        assert_eq!(Target::Broadcast.label(), "all");
        assert_eq!(Target::Channel("123".into()).query_parts(), ("channel", "123"));
    }

    #[test]
    fn acks_are_not_chat() {
        assert!(is_ack("ack123"));
        assert!(is_ack("rej7"));
        assert!(!is_ack("ack"));
        assert!(!is_ack("acknowledged, thanks"));
        assert!(!is_ack("hello"));
    }

    #[test]
    fn matching() {
        let ch = Target::Channel("123".into());
        assert!(ch.matches(&msg(1, 1, 1, None, "A1A", "123", "hi")));
        assert!(!ch.matches(&msg(1, 1, 1, None, "A1A", "12302", "hi")));
        assert!(!ch.matches(&msg(1, 1, 1, None, "A1A", "123", "ack12")));

        assert!(Target::Broadcast.matches(&msg(1, 1, 1, None, "A1A", "*", "hi")));
        assert!(Target::Broadcast.matches(&msg(1, 1, 1, None, "A1A", "", "hi")));
        assert!(!Target::Broadcast.matches(&msg(1, 1, 1, None, "A1A", "123", "hi")));

        let dm = Target::Callsign("QA0OWN-11".into());
        assert!(dm.matches(&msg(1, 1, 1, None, "A1A", "QA0OWN-11", "hi")));
        assert!(dm.matches(&msg(1, 1, 1, None, "QA0OWN-11", "B1B", "hi")));
        assert!(!dm.matches(&msg(1, 1, 1, None, "QA0OWN-11", "123", "hi")));
        assert!(!dm.matches(&msg(1, 1, 1, None, "QA0OWN-11", "*", "hi")));
    }

    #[test]
    fn copies_join_their_message() {
        let firsts = vec![
            msg(1, 10, 1, Some(7), "A1A", "123", "a"),
            msg(2, 20, 1, Some(8), "A1A", "123", "b"),
        ];
        let copies = vec![
            msg(4, 25, 2, Some(8), "A1A", "123", "b"),
            msg(3, 12, 2, Some(7), "A1A", "123", "a"),
            msg(5, 30, 2, Some(9), "A1A", "123", "c"),
            msg(6, 10 + COPY_WINDOW_US + 1, 2, Some(7), "A1A", "123", "a"),
        ];
        let grouped = group(firsts, copies);
        assert_eq!(grouped.len(), 2);
        assert_eq!(
            grouped[0].copies.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(3)]
        );
        assert_eq!(
            grouped[1].copies.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(4)]
        );
    }

    #[test]
    fn messages_without_id_have_no_copies() {
        let grouped = group(
            vec![msg(1, 10, 1, None, "A1A", "*", "x")],
            vec![msg(2, 11, 2, None, "A1A", "*", "x")],
        );
        assert!(grouped[0].copies.is_empty());
    }

    #[test]
    fn ranking_puts_channels_first_then_recent() {
        let info = |t: Target, last: Option<u64>| TargetInfo {
            target: t,
            messages: 1,
            last_seen: last,
        };
        let ranked = rank(
            vec![
                info(Target::Callsign("A1A".into()), Some(50)),
                info(Target::Channel("9".into()), Some(10)),
                info(Target::Channel("123".into()), Some(40)),
                info(Target::Callsign("B1B".into()), None),
                info(Target::Channel("123".into()), Some(40)),
            ],
            &[Target::Channel("9".into())],
            3,
        );
        let labels: Vec<String> = ranked.iter().map(|i| i.target.to_string()).collect();
        assert_eq!(labels, vec!["channel:123", "callsign:A1A", "callsign:B1B"]);
    }

    #[test]
    fn node_group_setting() {
        assert_eq!(node_groups("123;12302;0;0;0;0;"), vec!["123", "12302"]);
        assert_eq!(node_groups(""), Vec::<String>::new());
        assert_eq!(node_groups("0;00;x;9"), vec!["9"]);
    }
}
