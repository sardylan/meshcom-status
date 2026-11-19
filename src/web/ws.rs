use std::collections::VecDeque;
use std::sync::Arc;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use super::api::{self, ClientMessage, Request, WsMessage};
use crate::app::{Event, Hub};
use crate::chat::Target;
use crate::frame::Reception;
use crate::view::View;
use uuid::Uuid;

fn text(message: &WsMessage) -> Option<Message> {
    serde_json::to_string(message)
        .map(|s| Message::Text(Utf8Bytes::from(s)))
        .map_err(|e| tracing::error!("cannot encode a WebSocket message: {e}"))
        .ok()
}

async fn load(hub: &Hub, view: &View) -> (VecDeque<Reception>, Option<Message>) {
    match hub.receptions(view).await {
        Ok(rows) => {
            let message = text(&api::snapshot(view, hub.nodes(), &rows));
            (VecDeque::from(rows), message)
        }
        Err(e) => {
            tracing::error!("cannot load the frames of a view: {e}");
            (
                VecDeque::new(),
                text(&WsMessage::Error {
                    detail: "cannot load the frames".into(),
                }),
            )
        }
    }
}

fn slide(window: &mut VecDeque<Reception>, view: &View, r: Reception) -> Vec<Uuid> {
    window.push_back(r);
    let limit = view.limit.map(|l| l as usize).unwrap_or(usize::MAX);
    let excess = window.len().saturating_sub(limit);
    window.drain(..excess).map(|old| old.id).collect()
}

async fn chat_update(hub: &Hub, target: Option<&Target>, r: &Reception) -> Option<Message> {
    let target = target.filter(|t| t.matches(r))?;
    match hub.chat_message_of(r).await {
        Ok(Some(m)) => text(&api::chat_event(target, &m, hub.own_call().as_deref())),
        Ok(None) => None,
        Err(e) => {
            tracing::error!("cannot load a chat message: {e}");
            None
        }
    }
}

pub(crate) async fn serve(mut socket: WebSocket, hub: Arc<Hub>, mut shutdown: watch::Receiver<bool>) {
    let mut events = hub.subscribe();
    let mut view = View::default();
    let mut chat: Option<Target> = None;
    let (mut window, first) = load(&hub, &view).await;
    if let Some(m) = first
        && socket.send(m).await.is_err()
    {
        return;
    }
    loop {
        let outgoing: Vec<Message> = tokio::select! {
            _ = shutdown.changed() => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            event = events.recv() => match event {
                Ok(Event::Node(n)) => text(&WsMessage::Node { node: api::node_view(n) }).into_iter().collect(),
                Ok(Event::Reception(r)) => {
                    let chat_message = chat_update(&hub, chat.as_ref(), &r).await;
                    let frame = view.matches(&r).then(|| {
                        let dropped = slide(&mut window, &view, r.clone());
                        text(&api::frame_added(r, dropped, window.make_contiguous()))
                    });
                    frame.flatten().into_iter().chain(chat_message).collect()
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::debug!("WebSocket client lagged by {n} events, reloading its view");
                    let (w, m) = load(&hub, &view).await;
                    window = w;
                    m.into_iter().collect()
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(Message::Text(t))) => match ClientMessage::parse(&t).and_then(|m| m.request()) {
                    Ok(Request::View(v)) => {
                        view = v;
                        let (w, m) = load(&hub, &view).await;
                        window = w;
                        m.into_iter().collect()
                    }
                    Ok(Request::Chat(target)) => {
                        chat = target;
                        vec![]
                    }
                    Err(e) => text(&WsMessage::Error { detail: e.to_string() }).into_iter().collect(),
                },
                Some(Ok(_)) => vec![],
            },
        };
        for m in outgoing {
            if socket.send(m).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, Payload, SourceKind};

    fn rx(id: u128) -> Reception {
        Reception {
            id: Uuid::from_u128(id),
            at: 1,
            copy: 1,
            path: vec!["A1A".into()],
            frame: Frame {
                origin: "A1A".into(),
                relays: vec![],
                payload: Payload::Other {
                    kind: 'T',
                    text: String::new(),
                },
                signal: None,
                hw: None,
                firmware: None,
                source: SourceKind::Console,
                msg_id: None,
            },
        }
    }

    #[test]
    fn window_slides_at_the_limit() {
        let view = View {
            limit: Some(2),
            ..View::default()
        };
        let mut w = VecDeque::from(vec![rx(1), rx(2)]);
        assert_eq!(slide(&mut w, &view, rx(3)), vec![Uuid::from_u128(1)]);
        assert_eq!(
            w.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    #[test]
    fn unlimited_window_keeps_everything() {
        let view = View {
            limit: None,
            ..View::default()
        };
        let mut w = VecDeque::from(vec![rx(1)]);
        assert!(slide(&mut w, &view, rx(2)).is_empty());
        assert_eq!(w.len(), 2);
    }
}
