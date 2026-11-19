use std::collections::BTreeMap;

use crate::aprs::Position;
use crate::frame::{Call, Frame, OwnNode, Payload, Reception, Signal};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub(crate) call: Call,
    pub(crate) own: bool,
    pub(crate) position: Option<Position>,
    pub(crate) hw: Option<u8>,
    pub(crate) firmware: Option<String>,
    pub(crate) frames: u64,
    pub(crate) last_seen: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Link {
    pub(crate) from: Call,
    pub(crate) to: Call,
    pub(crate) count: u64,
    pub(crate) last_seen: u64,
    pub(crate) signal: Option<Signal>,
    pub(crate) signal_seen: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HopSignal {
    pub(crate) signal: Signal,
    pub(crate) seen: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KindCounts {
    pub(crate) message: u64,
    pub(crate) position: u64,
    pub(crate) hey: u64,
    pub(crate) other: u64,
}

impl KindCounts {
    fn add(&mut self, payload: &Payload) {
        match payload {
            Payload::Message { .. } => self.message += 1,
            Payload::Position(_) => self.position += 1,
            Payload::Hey { .. } => self.hey += 1,
            Payload::Other { .. } => self.other += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Route {
    pub(crate) calls: Vec<Call>,
    pub(crate) count: u64,
    pub(crate) first_seen: u64,
    pub(crate) last_seen: u64,
    pub(crate) hops: Vec<Option<HopSignal>>,
    pub(crate) kinds: KindCounts,
}

fn folded(frame: &Frame) -> Vec<Call> {
    let mut calls: Vec<Call> = Vec::with_capacity(frame.relays.len() + 2);
    std::iter::once(&frame.origin).chain(frame.relays.iter()).for_each(|c| {
        if calls.last() != Some(c) {
            calls.push(c.clone());
        }
    });
    calls
}

fn chain(frame: &Frame, own: Option<&str>) -> (Vec<Call>, Vec<Option<Signal>>) {
    let mut calls = folded(frame);
    let path_links = calls.len() - 1;
    let hey_signals: Vec<Option<Signal>> = match &frame.payload {
        Payload::Hey { hops, .. } if hops.len() == path_links => hops.iter().map(|h| Some(h.signal)).collect(),
        _ => vec![None; path_links],
    };
    let heard_by_own = own.filter(|o| frame.signal.is_some() && calls.last().map(String::as_str) != Some(*o));
    match heard_by_own {
        Some(o) => {
            calls.push(o.to_owned());
            let signals = hey_signals.into_iter().chain(std::iter::once(frame.signal)).collect();
            (calls, signals)
        }
        None => (calls, hey_signals),
    }
}

fn hop_signals(r: &Reception) -> Vec<Option<Signal>> {
    let own_appended = r.path.len() > folded(&r.frame).len();
    let own = if own_appended {
        r.path.last().map(String::as_str)
    } else {
        None
    };
    let (_, signals) = chain(&r.frame, own);
    if signals.len() + 1 == r.path.len() {
        signals
    } else {
        vec![None; r.path.len().saturating_sub(1)]
    }
}

pub(crate) fn window(receptions: &[Reception]) -> (Vec<Link>, Vec<Route>) {
    let mut links: BTreeMap<(Call, Call), Link> = BTreeMap::new();
    let mut routes: BTreeMap<Vec<Call>, Route> = BTreeMap::new();
    receptions.iter().filter(|r| r.path.len() >= 2).for_each(|r| {
        let signals = hop_signals(r);
        r.path
            .windows(2)
            .zip(signals.iter())
            .filter(|(pair, _)| pair[0] != pair[1])
            .for_each(|(pair, signal)| {
                let link = links.entry((pair[0].clone(), pair[1].clone())).or_insert_with(|| Link {
                    from: pair[0].clone(),
                    to: pair[1].clone(),
                    count: 0,
                    last_seen: r.at,
                    signal: None,
                    signal_seen: None,
                });
                link.count += 1;
                link.last_seen = r.at;
                if let Some(s) = signal {
                    link.signal = Some(*s);
                    link.signal_seen = Some(r.at);
                }
            });
        let route = routes.entry(r.path.clone()).or_insert_with(|| Route {
            calls: r.path.clone(),
            count: 0,
            first_seen: r.at,
            last_seen: r.at,
            hops: vec![None; r.path.len() - 1],
            kinds: KindCounts::default(),
        });
        route.count += 1;
        route.last_seen = r.at;
        route.kinds.add(&r.frame.payload);
        route.hops.iter_mut().zip(signals.iter()).for_each(|(hop, signal)| {
            if let Some(s) = signal {
                *hop = Some(HopSignal { signal: *s, seen: r.at });
            }
        });
    });
    (links.into_values().collect(), routes.into_values().collect())
}

fn new_node(call: &str, now: u64) -> Node {
    Node {
        call: call.to_owned(),
        own: false,
        position: None,
        hw: None,
        firmware: None,
        frames: 0,
        last_seen: now,
    }
}

#[derive(Debug, Default)]
pub(crate) struct Nodes {
    own: Option<Call>,
    nodes: BTreeMap<Call, Node>,
}

impl Nodes {
    pub(crate) fn from_stored(stored: Vec<Node>) -> Nodes {
        let own = stored.iter().find(|n| n.own).map(|n| n.call.clone());
        Nodes {
            own,
            nodes: stored.into_iter().map(|n| (n.call.clone(), n)).collect(),
        }
    }

    pub(crate) fn all(&self) -> Vec<Node> {
        self.nodes.values().cloned().collect()
    }

    pub(crate) fn path_of(&self, frame: &Frame) -> Vec<Call> {
        chain(frame, self.own.as_deref()).0
    }

    pub(crate) fn own(&self) -> Option<&str> {
        self.own.as_deref()
    }

    pub(crate) fn apply_own(&mut self, own: OwnNode, now: u64) -> Vec<Node> {
        let previous = self.own.replace(own.call.clone()).filter(|p| *p != own.call);
        let demoted = previous.and_then(|p| self.nodes.get_mut(&p)).map(|n| {
            n.own = false;
            n.clone()
        });
        let node = self
            .nodes
            .entry(own.call.clone())
            .or_insert_with(|| new_node(&own.call, now));
        node.own = true;
        node.last_seen = node.last_seen.max(now);
        node.position = own.position.or(node.position);
        demoted.into_iter().chain(std::iter::once(node.clone())).collect()
    }

    pub(crate) fn apply(&mut self, r: &Reception) -> Vec<Node> {
        let frame = &r.frame;
        r.path.iter().for_each(|c| {
            let n = self.nodes.entry(c.clone()).or_insert_with(|| new_node(c, r.at));
            n.last_seen = n.last_seen.max(r.at);
        });
        if let Some(origin) = self.nodes.get_mut(&frame.origin) {
            origin.frames += 1;
            origin.hw = frame.hw.or(origin.hw);
            origin.firmware = frame.firmware.clone().or(origin.firmware.take());
            if let Payload::Position(p) = frame.payload {
                origin.position = Some(p);
            }
        }
        if let Payload::Hey { hops, .. } = &frame.payload {
            r.path.iter().skip(1).zip(hops.iter()).for_each(|(receiver, hop)| {
                if let Some(n) = self.nodes.get_mut(receiver) {
                    n.hw = n.hw.or(Some(hop.hw));
                }
            });
        }
        let mut changed: Vec<&Call> = r.path.iter().collect();
        changed.sort();
        changed.dedup();
        changed.into_iter().filter_map(|c| self.nodes.get(c).cloned()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{HopReport, SourceKind};
    use uuid::Uuid;

    fn s(rssi: i16, snr: i8) -> Signal {
        Signal { rssi, snr }
    }

    fn other() -> Payload {
        Payload::Other {
            kind: 'T',
            text: String::new(),
        }
    }

    fn frame(path: &[&str], payload: Payload, signal: Option<Signal>) -> Frame {
        Frame {
            origin: path[0].into(),
            relays: path[1..].iter().map(|c| c.to_string()).collect(),
            payload,
            signal,
            hw: Some(3),
            firmware: Some("4.40a".into()),
            source: SourceKind::Console,
            msg_id: None,
        }
    }

    fn rx(id: u128, at: u64, f: Frame, own: Option<&str>) -> Reception {
        let path = chain(&f, own).0;
        Reception {
            id: Uuid::from_u128(id),
            at,
            copy: 1,
            path,
            frame: f,
        }
    }

    #[test]
    fn chain_appends_own_node_when_measured() {
        let f = frame(&["A1A", "B1B"], other(), Some(s(-100, 5)));
        let (calls, signals) = chain(&f, Some("OWN"));
        assert_eq!(calls, vec!["A1A", "B1B", "OWN"]);
        assert_eq!(signals, vec![None, Some(s(-100, 5))]);
    }

    #[test]
    fn chain_without_signal_or_own_stops_at_last_relay() {
        let f = frame(&["A1A", "B1B"], other(), None);
        assert_eq!(chain(&f, Some("OWN")).0, vec!["A1A", "B1B"]);
        let f = frame(&["A1A", "B1B"], other(), Some(s(-100, 5)));
        assert_eq!(chain(&f, None).0, vec!["A1A", "B1B"]);
    }

    #[test]
    fn chain_does_not_repeat_own_node_or_duplicates() {
        let f = frame(&["A1A", "OWN"], other(), Some(s(-1, 1)));
        assert_eq!(chain(&f, Some("OWN")).0, vec!["A1A", "OWN"]);
        let f = frame(&["A1A", "A1A", "B1B"], other(), None);
        assert_eq!(chain(&f, None).0, vec!["A1A", "B1B"]);
    }

    #[test]
    fn chain_uses_hey_hop_signals() {
        let hops = vec![HopReport {
            hw: 3,
            signal: s(-109, -7),
        }];
        let f = frame(
            &["BRV", "ECH"],
            Payload::Hey {
                neighbours: Some(7),
                hops,
            },
            Some(s(-137, -19)),
        );
        let (calls, signals) = chain(&f, Some("OWN"));
        assert_eq!(calls, vec!["BRV", "ECH", "OWN"]);
        assert_eq!(signals, vec![Some(s(-109, -7)), Some(s(-137, -19))]);
    }

    #[test]
    fn chain_ignores_mismatched_hey_hops() {
        let f = frame(
            &["BRV", "ECH", "ALF"],
            Payload::Hey {
                neighbours: None,
                hops: vec![],
            },
            None,
        );
        assert_eq!(chain(&f, None).1, vec![None, None]);
    }

    #[test]
    fn hop_signals_follow_the_stored_path() {
        let hops = vec![HopReport {
            hw: 3,
            signal: s(-109, -7),
        }];
        let hey = frame(
            &["BRV", "ECH"],
            Payload::Hey { neighbours: None, hops },
            Some(s(-137, -19)),
        );
        assert_eq!(
            hop_signals(&rx(1, 1, hey.clone(), Some("OWN"))),
            vec![Some(s(-109, -7)), Some(s(-137, -19))]
        );
        assert_eq!(hop_signals(&rx(1, 1, hey, None)), vec![Some(s(-109, -7))]);
        let broken = Reception {
            id: Uuid::from_u128(1),
            at: 1,
            copy: 1,
            path: vec!["X1X".into(), "Y1Y".into()],
            frame: frame(&["A1A", "B1B", "C1C"], other(), None),
        };
        assert_eq!(hop_signals(&broken), vec![None]);
    }

    #[test]
    fn window_builds_links_and_routes() {
        let hops = vec![HopReport {
            hw: 3,
            signal: s(-109, -7),
        }];
        let receptions = vec![
            rx(
                1,
                10,
                frame(
                    &["BRV", "ECH"],
                    Payload::Hey { neighbours: None, hops },
                    Some(s(-137, -19)),
                ),
                Some("OWN"),
            ),
            rx(2, 20, frame(&["BRV", "ECH"], other(), Some(s(-120, -9))), Some("OWN")),
            rx(3, 30, frame(&["ECH"], other(), None), Some("OWN")),
        ];
        let (links, routes) = window(&receptions);
        assert_eq!(links.len(), 2);
        let last = links.iter().find(|l| l.to == "OWN").unwrap();
        assert_eq!((last.count, last.last_seen, last.signal), (2, 20, Some(s(-120, -9))));
        let first = links.iter().find(|l| l.to == "ECH").unwrap();
        assert_eq!(
            (first.count, first.signal, first.signal_seen),
            (2, Some(s(-109, -7)), Some(10))
        );

        assert_eq!(routes.len(), 1);
        let r = &routes[0];
        assert_eq!(r.calls, vec!["BRV", "ECH", "OWN"]);
        assert_eq!((r.count, r.first_seen, r.last_seen), (2, 10, 20));
        assert_eq!((r.kinds.hey, r.kinds.other), (1, 1));
        assert_eq!(
            r.hops,
            vec![
                Some(HopSignal {
                    signal: s(-109, -7),
                    seen: 10
                }),
                Some(HopSignal {
                    signal: s(-120, -9),
                    seen: 20
                })
            ]
        );
    }

    #[test]
    fn empty_window() {
        assert_eq!(window(&[]), (vec![], vec![]));
    }

    #[test]
    fn nodes_follow_receptions() {
        let mut nodes = Nodes::default();
        nodes.apply_own(
            OwnNode {
                call: "OWN".into(),
                position: None,
                groups: vec![],
            },
            1,
        );
        let pos = Position {
            lat: 10.0,
            lon: -30.0,
            alt_m: None,
        };
        let f = frame(&["A1A", "B1B"], Payload::Position(pos), Some(s(-90, 3)));
        let path = nodes.path_of(&f);
        assert_eq!(path, vec!["A1A", "B1B", "OWN"]);
        let changed = nodes.apply(&Reception {
            id: Uuid::from_u128(1),
            at: 5,
            copy: 1,
            path,
            frame: f,
        });
        assert_eq!(changed.len(), 3);
        let a = nodes.all().into_iter().find(|n| n.call == "A1A").unwrap();
        assert_eq!((a.position, a.frames, a.hw, a.last_seen), (Some(pos), 1, Some(3), 5));
    }

    #[test]
    fn older_reception_does_not_move_last_seen_back() {
        let mut nodes = Nodes::default();
        nodes.apply(&rx(1, 50, frame(&["A1A"], other(), None), None));
        nodes.apply(&rx(2, 10, frame(&["A1A"], other(), None), None));
        assert_eq!(nodes.all()[0].last_seen, 50);
    }

    #[test]
    fn hey_sets_receiver_hardware() {
        let mut nodes = Nodes::default();
        let hops = vec![HopReport {
            hw: 12,
            signal: s(-121, -6),
        }];
        nodes.apply(&rx(
            1,
            1,
            frame(&["BRV", "HTL"], Payload::Hey { neighbours: None, hops }, None),
            None,
        ));
        assert_eq!(nodes.all().into_iter().find(|n| n.call == "HTL").unwrap().hw, Some(12));
    }

    #[test]
    fn own_node_change_demotes_previous_and_is_restored() {
        let mut nodes = Nodes::default();
        nodes.apply_own(
            OwnNode {
                call: "OLD".into(),
                position: None,
                groups: vec![],
            },
            1,
        );
        assert_eq!(
            nodes
                .apply_own(
                    OwnNode {
                        call: "NEW".into(),
                        position: None,
                        groups: vec![],
                    },
                    2
                )
                .len(),
            2
        );
        let all = nodes.all();
        assert!(!all.iter().find(|n| n.call == "OLD").unwrap().own);
        let restored = Nodes::from_stored(all);
        assert_eq!(restored.own.as_deref(), Some("NEW"));
    }
}
