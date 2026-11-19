use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::aprs::Position;
use crate::chat::{ChatMessage, Target, TargetInfo};
use crate::clock;
use crate::errors::Result;
use crate::frame::{Payload, Reception};
use crate::graph::{self, KindCounts, Link, Node, Route};
use crate::stats::{self, Ranked, Stats};
use crate::view::View;

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct NodeView {
    call: String,
    own: bool,
    position: Option<Position>,
    hw: Option<u8>,
    firmware: Option<String>,
    frames: u64,
    last_seen: String,
}

pub(crate) fn node_view(n: Node) -> NodeView {
    NodeView {
        call: n.call,
        own: n.own,
        position: n.position,
        hw: n.hw,
        firmware: n.firmware,
        frames: n.frames,
        last_seen: clock::rfc3339_secs(n.last_seen),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct LinkView {
    from: String,
    to: String,
    count: u64,
    last_seen: String,
    rssi: Option<i16>,
    snr: Option<i8>,
    signal_seen: Option<String>,
}

pub(crate) fn link_view(l: Link) -> LinkView {
    LinkView {
        from: l.from,
        to: l.to,
        count: l.count,
        last_seen: clock::rfc3339_secs(l.last_seen),
        rssi: l.signal.map(|s| s.rssi),
        snr: l.signal.map(|s| s.snr),
        signal_seen: l.signal_seen.map(clock::rfc3339_secs),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct PathHopView {
    from: String,
    to: String,
    rssi: Option<i16>,
    snr: Option<i8>,
    signal_seen: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct KindsView {
    message: u64,
    position: u64,
    hey: u64,
    other: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct PathView {
    calls: Vec<String>,
    count: u64,
    first_seen: String,
    last_seen: String,
    hops: Vec<PathHopView>,
    kinds: KindsView,
}

pub(crate) fn path_view(r: Route) -> PathView {
    let hops = r
        .calls
        .windows(2)
        .zip(r.hops.iter())
        .map(|(pair, hop)| PathHopView {
            from: pair[0].clone(),
            to: pair[1].clone(),
            rssi: hop.map(|h| h.signal.rssi),
            snr: hop.map(|h| h.signal.snr),
            signal_seen: hop.map(|h| clock::rfc3339_secs(h.seen)),
        })
        .collect();
    let KindCounts {
        message,
        position,
        hey,
        other,
    } = r.kinds;
    PathView {
        calls: r.calls,
        count: r.count,
        first_seen: clock::rfc3339_secs(r.first_seen),
        last_seen: clock::rfc3339_secs(r.last_seen),
        hops,
        kinds: KindsView {
            message,
            position,
            hey,
            other,
        },
    }
}

fn kinds_view(k: KindCounts) -> KindsView {
    KindsView {
        message: k.message,
        position: k.position,
        hey: k.hey,
        other: k.other,
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct StatsTotalsView {
    frames: u64,
    receptions: u64,
    messages: u64,
    stations: u64,
    relays: u64,
    direct: u64,
    first_seen: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct TimeBucketView {
    start: String,
    #[serde(flatten)]
    kinds: KindsView,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct RankedView {
    name: String,
    count: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct HopsView {
    hops: usize,
    or_more: bool,
    frames: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct SnrView {
    snr: i16,
    receptions: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct StatsBody {
    range: &'static str,
    ranges: Vec<&'static str>,
    bucket: &'static str,
    since: Option<String>,
    until: String,
    totals: StatsTotalsView,
    kinds: KindsView,
    timeline: Vec<TimeBucketView>,
    hour_of_day: [u64; 24],
    top_origins: Vec<RankedView>,
    top_relays: Vec<RankedView>,
    hops: Vec<HopsView>,
    snr: Vec<SnrView>,
    chat: Vec<RankedView>,
}

fn ranked_views(r: Vec<Ranked>) -> Vec<RankedView> {
    r.into_iter()
        .map(|r| RankedView {
            name: r.name,
            count: r.count,
        })
        .collect()
}

pub(crate) fn stats_body(s: Stats) -> StatsBody {
    StatsBody {
        range: s.range.token(),
        ranges: stats::RANGES.iter().map(|r| r.token()).collect(),
        bucket: s.range.bucket().token(),
        since: s.since.map(clock::rfc3339_secs),
        until: clock::rfc3339_secs(s.until),
        totals: StatsTotalsView {
            frames: s.totals.frames,
            receptions: s.totals.receptions,
            messages: s.totals.messages,
            stations: s.totals.stations,
            relays: s.totals.relays,
            direct: s.totals.direct,
            first_seen: s.totals.first_seen.map(clock::rfc3339_secs),
        },
        kinds: kinds_view(s.kinds),
        timeline: s
            .timeline
            .into_iter()
            .map(|t| TimeBucketView {
                start: clock::rfc3339_secs(t.start),
                kinds: kinds_view(t.kinds),
            })
            .collect(),
        hour_of_day: s.hour_of_day,
        top_origins: ranked_views(s.top_origins),
        top_relays: ranked_views(s.top_relays),
        hops: s
            .hops
            .iter()
            .enumerate()
            .map(|(hops, &frames)| HopsView {
                hops,
                or_more: hops == stats::HOPS_CAP,
                frames,
            })
            .collect(),
        snr: s
            .snr
            .into_iter()
            .map(|(snr, receptions)| SnrView { snr, receptions })
            .collect(),
        chat: ranked_views(s.chat),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct HopView {
    hw: u8,
    rssi: i16,
    snr: i8,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct FrameView {
    id: Uuid,
    received_at: String,
    source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg_id: Option<String>,
    copy: u32,
    origin: String,
    relays: Vec<String>,
    path: Vec<String>,
    rssi: Option<i16>,
    snr: Option<i8>,
    hw: Option<u8>,
    firmware: Option<String>,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<Position>,
    #[serde(skip_serializing_if = "Option::is_none")]
    neighbours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hops: Option<Vec<HopView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    type_char: Option<String>,
}

pub(crate) fn frame_view(r: Reception) -> FrameView {
    let f = r.frame;
    let base = FrameView {
        id: r.id,
        received_at: clock::rfc3339_micros(r.at),
        source: f.source.as_str(),
        msg_id: f.msg_id.map(|id| format!("{id:08X}")),
        copy: r.copy,
        origin: f.origin,
        relays: f.relays,
        path: r.path,
        rssi: f.signal.map(|s| s.rssi),
        snr: f.signal.map(|s| s.snr),
        hw: f.hw,
        firmware: f.firmware,
        kind: f.payload.kind().as_str(),
        dest: None,
        text: None,
        position: None,
        neighbours: None,
        hops: None,
        type_char: None,
    };
    match f.payload {
        Payload::Message { dest, text } => FrameView {
            dest: Some(dest),
            text: Some(text),
            ..base
        },
        Payload::Position(p) => FrameView {
            position: Some(p),
            ..base
        },
        Payload::Hey { neighbours, hops } => FrameView {
            neighbours,
            hops: Some(
                hops.into_iter()
                    .map(|h| HopView {
                        hw: h.hw,
                        rssi: h.signal.rssi,
                        snr: h.signal.snr,
                    })
                    .collect(),
            ),
            ..base
        },
        Payload::Other { kind, text } => FrameView {
            type_char: Some(kind.to_string()),
            text: Some(text),
            ..base
        },
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ViewView {
    limit: Option<u32>,
    kinds: Vec<&'static str>,
    copies: bool,
}

pub(crate) fn view_view(v: &View) -> ViewView {
    ViewView {
        limit: v.limit,
        kinds: v.kinds.iter().map(|k| k.as_str()).collect(),
        copies: v.copies,
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct NodesBody {
    pub(crate) nodes: Vec<NodeView>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct FramesBody {
    pub(crate) view: ViewView,
    pub(crate) frames: Vec<FrameView>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct LinksBody {
    pub(crate) view: ViewView,
    pub(crate) links: Vec<LinkView>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct PathsBody {
    pub(crate) view: ViewView,
    pub(crate) paths: Vec<PathView>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum WsMessage {
    Snapshot {
        view: ViewView,
        nodes: Vec<NodeView>,
        frames: Vec<FrameView>,
        links: Vec<LinkView>,
        paths: Vec<PathView>,
    },
    Frame {
        frame: Box<FrameView>,
        dropped: Vec<Uuid>,
        links: Vec<LinkView>,
        paths: Vec<PathView>,
    },
    Node {
        node: NodeView,
    },
    Chat {
        target: String,
        message: Box<ChatMessageView>,
    },
    Error {
        detail: String,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ClientMessage {
    View {
        limit: Option<u32>,
        kinds: Vec<String>,
        copies: bool,
    },
    Chat {
        target: Option<String>,
    },
}

#[derive(Debug, PartialEq)]
pub(crate) enum Request {
    View(View),
    Chat(Option<Target>),
}

impl ClientMessage {
    pub(crate) fn parse(text: &str) -> Result<ClientMessage> {
        Ok(serde_json::from_str(text)?)
    }

    pub(crate) fn request(&self) -> Result<Request> {
        match self {
            ClientMessage::View { limit, kinds, copies } => View::new(*limit, kinds, *copies).map(Request::View),
            ClientMessage::Chat { target } => target.as_deref().map(Target::parse).transpose().map(Request::Chat),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ChatReceptionView {
    id: Uuid,
    received_at: String,
    source: &'static str,
    relays: Vec<String>,
    path: Vec<String>,
    rssi: Option<i16>,
    snr: Option<i8>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ChatMessageView {
    key: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg_id: Option<String>,
    received_at: String,
    origin: String,
    dest: String,
    text: String,
    own: bool,
    hw: Option<u8>,
    firmware: Option<String>,
    receptions: Vec<ChatReceptionView>,
}

fn chat_reception_view(r: &Reception) -> ChatReceptionView {
    ChatReceptionView {
        id: r.id,
        received_at: clock::rfc3339_micros(r.at),
        source: r.frame.source.as_str(),
        relays: r.frame.relays.clone(),
        path: r.path.clone(),
        rssi: r.frame.signal.map(|s| s.rssi),
        snr: r.frame.signal.map(|s| s.snr),
    }
}

pub(crate) fn chat_message_view(m: &ChatMessage, own_call: Option<&str>) -> ChatMessageView {
    let f = &m.first.frame;
    let (dest, text) = match &f.payload {
        Payload::Message { dest, text } => (dest.clone(), text.clone()),
        _ => (String::new(), String::new()),
    };
    ChatMessageView {
        key: m.first.id,
        msg_id: f.msg_id.map(|id| format!("{id:08X}")),
        received_at: clock::rfc3339_micros(m.first.at),
        origin: f.origin.clone(),
        dest,
        text,
        own: own_call == Some(f.origin.as_str()),
        hw: f.hw,
        firmware: f.firmware.clone(),
        receptions: std::iter::once(&m.first)
            .chain(m.copies.iter())
            .map(chat_reception_view)
            .collect(),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct TabView {
    target: String,
    label: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ChatTabsBody {
    tabs: Vec<TabView>,
    node_groups: Vec<String>,
}

pub(crate) fn chat_tabs_body(tabs: &[Target], node_groups: Vec<String>) -> ChatTabsBody {
    ChatTabsBody {
        tabs: tabs
            .iter()
            .map(|t| TabView {
                target: t.to_string(),
                label: t.label(),
            })
            .collect(),
        node_groups,
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct TargetView {
    target: String,
    label: String,
    kind: &'static str,
    messages: u64,
    last_seen: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ChatTargetsBody {
    targets: Vec<TargetView>,
}

pub(crate) fn chat_targets_body(infos: Vec<TargetInfo>) -> ChatTargetsBody {
    ChatTargetsBody {
        targets: infos
            .into_iter()
            .map(|i| TargetView {
                target: i.target.to_string(),
                label: i.target.label(),
                kind: i.target.query_parts().0,
                messages: i.messages,
                last_seen: i.last_seen.map(clock::rfc3339_secs),
            })
            .collect(),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ChatPageBody {
    target: String,
    messages: Vec<ChatMessageView>,
    more: bool,
}

pub(crate) fn chat_page_body(
    target: &Target,
    messages: &[ChatMessage],
    more: bool,
    own_call: Option<&str>,
) -> ChatPageBody {
    ChatPageBody {
        target: target.to_string(),
        messages: messages.iter().map(|m| chat_message_view(m, own_call)).collect(),
        more,
    }
}

pub(crate) fn chat_event(target: &Target, m: &ChatMessage, own_call: Option<&str>) -> WsMessage {
    WsMessage::Chat {
        target: target.to_string(),
        message: Box::new(chat_message_view(m, own_call)),
    }
}

fn window_views(window: &[Reception]) -> (Vec<LinkView>, Vec<PathView>) {
    let (links, routes) = graph::window(window);
    (
        links.into_iter().map(link_view).collect(),
        routes.into_iter().map(path_view).collect(),
    )
}

pub(crate) fn snapshot(view: &View, nodes: Vec<Node>, window: &[Reception]) -> WsMessage {
    let (links, paths) = window_views(window);
    WsMessage::Snapshot {
        view: view_view(view),
        nodes: nodes.into_iter().map(node_view).collect(),
        frames: window.iter().cloned().map(frame_view).collect(),
        links,
        paths,
    }
}

pub(crate) fn frame_added(added: Reception, dropped: Vec<Uuid>, window: &[Reception]) -> WsMessage {
    let (links, paths) = window_views(window);
    WsMessage::Frame {
        frame: Box::new(frame_view(added)),
        dropped,
        links,
        paths,
    }
}

pub(crate) fn links_body(view: &View, window: &[Reception]) -> LinksBody {
    LinksBody {
        view: view_view(view),
        links: window_views(window).0,
    }
}

pub(crate) fn paths_body(view: &View, window: &[Reception]) -> PathsBody {
    PathsBody {
        view: view_view(view),
        paths: window_views(window).1,
    }
}

pub(crate) fn frames_body(view: &View, window: Vec<Reception>) -> FramesBody {
    FramesBody {
        view: view_view(view),
        frames: window.into_iter().map(frame_view).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, HopReport, Kind, Signal, SourceKind};
    use crate::graph::HopSignal;
    use serde_json::json;

    const T: u64 = 1_794_564_903_000_000;

    fn node() -> Node {
        Node {
            call: "QA0OWN-11".into(),
            own: true,
            position: Some(Position {
                lat: 10.2,
                lon: -30.1,
                alt_m: None,
            }),
            hw: Some(3),
            firmware: None,
            frames: 2,
            last_seen: T,
        }
    }

    fn reception(id: u128, payload: Payload) -> Reception {
        Reception {
            id: Uuid::from_u128(id),
            at: T + 123_456,
            copy: 2,
            path: vec!["BRV".into(), "ECH".into(), "OWN".into()],
            frame: Frame {
                origin: "BRV".into(),
                relays: vec!["ECH".into()],
                payload,
                signal: Some(Signal { rssi: -137, snr: -19 }),
                hw: Some(3),
                firmware: Some("4.40a".into()),
                source: SourceKind::Console,
                msg_id: Some(0x4598211D),
            },
        }
    }

    fn hey() -> Payload {
        Payload::Hey {
            neighbours: Some(7),
            hops: vec![HopReport {
                hw: 3,
                signal: Signal { rssi: -109, snr: -7 },
            }],
        }
    }

    #[test]
    fn node_json() {
        assert_eq!(
            serde_json::to_value(WsMessage::Node {
                node: node_view(node())
            })
            .unwrap(),
            json!({"type":"node","node":{"call":"QA0OWN-11","own":true,"position":{"lat":10.2,"lon":-30.1},
                   "hw":3,"firmware":null,"frames":2,"last_seen":"2026-11-13T10:15:03Z"}})
        );
    }

    #[test]
    fn hey_frame_json() {
        assert_eq!(
            serde_json::to_value(frame_view(reception(5, hey()))).unwrap(),
            json!({"id":"00000000-0000-0000-0000-000000000005","received_at":"2026-11-13T10:15:03.123456Z","source":"console","msg_id":"4598211D","copy":2,
                   "origin":"BRV","relays":["ECH"],"path":["BRV","ECH","OWN"],"rssi":-137,"snr":-19,"hw":3,
                   "firmware":"4.40a","kind":"hey","neighbours":7,"hops":[{"hw":3,"rssi":-109,"snr":-7}]})
        );
    }

    #[test]
    fn message_and_other_frame_json() {
        let m = serde_json::to_value(frame_view(reception(
            1,
            Payload::Message {
                dest: "123".into(),
                text: "hello".into(),
            },
        )))
        .unwrap();
        assert_eq!(
            (m["kind"].as_str(), m["dest"].as_str(), m["text"].as_str()),
            (Some("message"), Some("123"), Some("hello"))
        );
        assert!(m.get("hops").is_none());
        let o = serde_json::to_value(frame_view(reception(
            1,
            Payload::Other {
                kind: 'T',
                text: "x".into(),
            },
        )))
        .unwrap();
        assert_eq!(
            (o["kind"].as_str(), o["type_char"].as_str()),
            (Some("other"), Some("T"))
        );
    }

    #[test]
    fn path_json() {
        let route = Route {
            calls: vec!["BRV".into(), "ECH".into(), "OWN".into()],
            count: 3,
            first_seen: T,
            last_seen: T,
            hops: vec![
                None,
                Some(HopSignal {
                    signal: Signal { rssi: -137, snr: -19 },
                    seen: T,
                }),
            ],
            kinds: KindCounts {
                message: 0,
                position: 1,
                hey: 2,
                other: 0,
            },
        };
        assert_eq!(
            serde_json::to_value(path_view(route)).unwrap(),
            json!({"calls":["BRV","ECH","OWN"],"count":3,"first_seen":"2026-11-13T10:15:03Z","last_seen":"2026-11-13T10:15:03Z",
                   "hops":[{"from":"BRV","to":"ECH","rssi":null,"snr":null,"signal_seen":null},
                           {"from":"ECH","to":"OWN","rssi":-137,"snr":-19,"signal_seen":"2026-11-13T10:15:03Z"}],
                   "kinds":{"message":0,"position":1,"hey":2,"other":0}})
        );
    }

    #[test]
    fn snapshot_and_frame_messages() {
        let window = vec![reception(1, hey()), reception(2, hey())];
        let v = serde_json::to_value(snapshot(&View::default(), vec![node()], &window)).unwrap();
        assert_eq!(v["type"], "snapshot");
        assert_eq!(
            v["view"],
            json!({"limit":30,"kinds":["message","position","hey","other"],"copies":true})
        );
        assert_eq!(v["frames"].as_array().unwrap().len(), 2);
        assert_eq!(v["links"].as_array().unwrap().len(), 2);
        assert_eq!(v["paths"][0]["count"], 2);

        let f = serde_json::to_value(frame_added(reception(3, hey()), vec![Uuid::from_u128(1)], &window[1..])).unwrap();
        assert_eq!(
            (f["type"].as_str(), f["frame"]["id"].as_str()),
            (Some("frame"), Some("00000000-0000-0000-0000-000000000003"))
        );
        assert_eq!(f["dropped"], json!(["00000000-0000-0000-0000-000000000001"]));
        assert_eq!(f["paths"][0]["count"], 1);
    }

    #[test]
    fn chat_message_json() {
        let first = reception(
            1,
            Payload::Message {
                dest: "123".into(),
                text: "hello".into(),
            },
        );
        let copy = Reception {
            id: Uuid::from_u128(2),
            copy: 2,
            path: vec!["BRV".into(), "ALF".into(), "OWN".into()],
            ..first.clone()
        };
        let m = ChatMessage {
            first,
            copies: vec![copy],
        };
        let v = serde_json::to_value(chat_event(&Target::Channel("123".into()), &m, Some("BRV"))).unwrap();
        assert_eq!(v["type"], "chat");
        assert_eq!(v["target"], "channel:123");
        let msg = &v["message"];
        assert_eq!(msg["key"], "00000000-0000-0000-0000-000000000001");
        assert_eq!(
            (msg["dest"].as_str(), msg["text"].as_str(), msg["own"].as_bool()),
            (Some("123"), Some("hello"), Some(true))
        );
        assert_eq!(msg["receptions"].as_array().unwrap().len(), 2);
        assert_eq!(msg["receptions"][1]["path"], json!(["BRV", "ALF", "OWN"]));
        assert_eq!(msg["receptions"][0]["rssi"], -137);
    }

    #[test]
    fn chat_bodies() {
        let tabs = serde_json::to_value(chat_tabs_body(
            &[Target::Broadcast, Target::Channel("123".into())],
            vec!["123".into()],
        ))
        .unwrap();
        assert_eq!(
            tabs,
            json!({"tabs":[{"target":"broadcast","label":"all"},{"target":"channel:123","label":"123"}],"node_groups":["123"]})
        );
        let targets = serde_json::to_value(chat_targets_body(vec![TargetInfo {
            target: Target::Callsign("A1A".into()),
            messages: 2,
            last_seen: Some(T),
        }]))
        .unwrap();
        assert_eq!(
            targets,
            json!({"targets":[{"target":"callsign:A1A","label":"A1A","kind":"callsign","messages":2,"last_seen":"2026-11-13T10:15:03Z"}]})
        );
        let page = serde_json::to_value(chat_page_body(&Target::Broadcast, &[], true, None)).unwrap();
        assert_eq!(page, json!({"target":"broadcast","messages":[],"more":true}));
    }

    #[test]
    fn client_view_messages() {
        let m = ClientMessage::parse(r#"{"type":"view","limit":null,"kinds":["hey"],"copies":false}"#).unwrap();
        assert_eq!(
            m.request().unwrap(),
            Request::View(View {
                limit: None,
                kinds: vec![Kind::Hey],
                copies: false
            })
        );
        assert!(
            ClientMessage::parse(r#"{"type":"view","limit":10,"kinds":["x"],"copies":true}"#)
                .unwrap()
                .request()
                .is_err()
        );
        assert!(
            ClientMessage::parse(r#"{"type":"view","limit":0,"kinds":[],"copies":true}"#)
                .unwrap()
                .request()
                .is_err()
        );
        assert!(ClientMessage::parse(r#"{"type":"zoom"}"#).is_err());
        let chat = ClientMessage::parse(r#"{"type":"chat","target":"channel:123"}"#).unwrap();
        assert_eq!(
            chat.request().unwrap(),
            Request::Chat(Some(Target::Channel("123".into())))
        );
        let none = ClientMessage::parse(r#"{"type":"chat","target":null}"#).unwrap();
        assert_eq!(none.request().unwrap(), Request::Chat(None));
        assert!(
            ClientMessage::parse(r#"{"type":"chat","target":"x"}"#)
                .unwrap()
                .request()
                .is_err()
        );
        assert!(ClientMessage::parse(r#"{"type":"view","limit":1,"kinds":[],"copies":true,"x":1}"#).is_err());
    }
}
