mod api;
mod assets;
mod encoding;
mod ws;

use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::extract::{Query, Request, State, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::app::Hub;
use crate::chat::{self, Target};
use crate::config::WebConfig;
use crate::errors::{Error, Result};
use crate::frame::Reception;
use crate::logging::ACCESS;
use crate::view::View;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    hub: Arc<Hub>,
    assets: Arc<assets::Assets>,
    shutdown: watch::Receiver<bool>,
}

#[derive(Serialize)]
struct Problem {
    r#type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

fn problem(status: StatusCode, detail: impl Into<String>) -> Response {
    let body = Problem {
        r#type: "about:blank",
        title: status.canonical_reason().unwrap_or("Error"),
        status: status.as_u16(),
        detail: detail.into(),
    };
    let json = serde_json::to_string(&body).unwrap_or_default();
    (status, [(header::CONTENT_TYPE, "application/problem+json")], json).into_response()
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {self}");
        problem(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

fn json<T: Serialize>(value: &T) -> Result<Response> {
    let body = serde_json::to_string(value)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn index(State(s): State<AppState>, headers: HeaderMap) -> Response {
    resource(&s, assets::INDEX, &headers).await
}

async fn nodes(State(s): State<AppState>) -> Result<Response> {
    json(&api::NodesBody {
        nodes: s.hub.nodes().into_iter().map(api::node_view).collect(),
    })
}

#[derive(Debug, Deserialize)]
struct ViewQuery {
    limit: Option<String>,
    kinds: Option<String>,
    copies: Option<String>,
}

async fn windowed<T: Serialize>(s: &AppState, q: ViewQuery, body: impl FnOnce(&View, Vec<Reception>) -> T) -> Response {
    let view = match View::from_query(q.limit.as_deref(), q.kinds.as_deref(), q.copies.as_deref()) {
        Ok(v) => v,
        Err(e) => return problem(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
    };
    match s.hub.receptions(&view).await.and_then(|w| json(&body(&view, w))) {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

async fn frames(State(s): State<AppState>, Query(q): Query<ViewQuery>) -> Response {
    windowed(&s, q, api::frames_body).await
}

async fn links(State(s): State<AppState>, Query(q): Query<ViewQuery>) -> Response {
    windowed(&s, q, |v, w| api::links_body(v, &w)).await
}

async fn paths(State(s): State<AppState>, Query(q): Query<ViewQuery>) -> Response {
    windowed(&s, q, |v, w| api::paths_body(v, &w)).await
}

async fn chat_tabs(State(s): State<AppState>) -> Result<Response> {
    json(&api::chat_tabs_body(&s.hub.chat_tabs(), s.hub.node_groups()))
}

#[derive(Debug, Deserialize)]
struct StatsQuery {
    range: Option<String>,
    tz: Option<String>,
}

async fn stats(State(s): State<AppState>, Query(q): Query<StatsQuery>) -> Response {
    let range = match crate::stats::Range::parse(q.range.as_deref()) {
        Ok(r) => r,
        Err(e) => return problem(StatusCode::UNPROCESSABLE_ENTITY, format!("range: {e}")),
    };
    let tz = q.tz.unwrap_or_else(|| "UTC".to_owned());
    if !crate::stats::plausible_time_zone(&tz) {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "tz: an IANA time zone name, such as Europe/Berlin",
        );
    }
    match s.hub.stats(range, &tz).await {
        Ok(st) => json(&api::stats_body(st)).unwrap_or_else(IntoResponse::into_response),
        Err(e @ Error::Invalid { .. }) => problem(StatusCode::UNPROCESSABLE_ENTITY, format!("tz: {e}")),
        Err(e) => e.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct TargetsQuery {
    q: Option<String>,
    limit: Option<u32>,
    exclude: Option<String>,
}

async fn chat_targets(State(s): State<AppState>, Query(q): Query<TargetsQuery>) -> Response {
    let prefix = q.q.unwrap_or_default().trim().to_ascii_uppercase();
    if prefix.len() > 12 || !prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, "q: up to 12 letters, digits or '-'");
    }
    let limit = q.limit.unwrap_or(chat::PAGE).clamp(1, chat::MAX_PAGE);
    let exclude = match q
        .exclude
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|t| !t.is_empty())
        .map(Target::parse)
        .collect::<Result<Vec<_>>>()
    {
        Ok(e) if e.len() <= 100 => e,
        Ok(_) => return problem(StatusCode::UNPROCESSABLE_ENTITY, "exclude: at most 100 targets"),
        Err(e) => return problem(StatusCode::UNPROCESSABLE_ENTITY, format!("exclude: {e}")),
    };
    match s
        .hub
        .chat_targets(&prefix, limit, &exclude)
        .await
        .and_then(|t| json(&api::chat_targets_body(t)))
    {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    target: String,
    before: Option<String>,
    limit: Option<u32>,
}

async fn chat_messages(State(s): State<AppState>, Query(q): Query<MessagesQuery>) -> Response {
    let target = match Target::parse(&q.target) {
        Ok(t) => t,
        Err(e) => return problem(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
    };
    let before = match q.before.as_deref().map(Uuid::parse_str).transpose() {
        Ok(b) => b,
        Err(_) => return problem(StatusCode::UNPROCESSABLE_ENTITY, "before: not a message key (UUID)"),
    };
    let limit = q.limit.unwrap_or(chat::PAGE);
    if !(1..=chat::MAX_PAGE).contains(&limit) {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, "limit: between 1 and 200");
    }
    let own = s.hub.own_call();
    match s
        .hub
        .chat_page(&target, before, limit)
        .await
        .and_then(|(messages, more)| json(&api::chat_page_body(&target, &messages, more, own.as_deref())))
    {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

async fn websocket(State(s): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| ws::serve(socket, s.hub, s.shutdown))
}

async fn static_file(State(s): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    match uri.path().trim_start_matches('/') {
        assets::INDEX => problem(StatusCode::NOT_FOUND, "no such resource"),
        path => resource(&s, path, &headers).await,
    }
}

async fn resource(s: &AppState, path: &str, headers: &HeaderMap) -> Response {
    let Some(r) = s.assets.get(path) else {
        return problem(StatusCode::NOT_FOUND, "no such resource");
    };
    let accept = headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok());
    let (used, body) = match r.body(path, encoding::negotiate(accept)).await {
        Ok(b) => b,
        Err(e) => return e.into_response(),
    };
    let mut response = (
        [
            (header::CONTENT_TYPE, r.mime.clone()),
            (header::CACHE_CONTROL, r.cache_control.to_owned()),
        ],
        body,
    )
        .into_response();
    let h = response.headers_mut();
    if r.compressible {
        h.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
    if used != encoding::Encoding::Identity {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static(used.token()));
    }
    response
}

async fn access_log(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    if path == "/health" {
        tracing::debug!(target: ACCESS, "{method} {path} {status} {ms:.1}ms");
    } else {
        tracing::info!(target: ACCESS, "{method} {path} {status} {ms:.1}ms");
    }
    response
}

fn router(state: AppState, api_prefix: &str) -> Router {
    let api = Router::new()
        .route("/public/v1/nodes", get(nodes))
        .route("/public/v1/links", get(links))
        .route("/public/v1/paths", get(paths))
        .route("/public/v1/frames", get(frames))
        .route("/public/v1/chat/tabs", get(chat_tabs))
        .route("/public/v1/chat/targets", get(chat_targets))
        .route("/public/v1/chat/messages", get(chat_messages))
        .route("/public/v1/stats", get(stats))
        .route("/public/v1/ws", get(websocket));
    Router::new()
        .route("/health", get(health))
        .route("/", get(index))
        .nest(api_prefix, api)
        .fallback(static_file)
        .with_state(state)
        .layer(middleware::from_fn(access_log))
}

pub(crate) async fn serve(web: &WebConfig, hub: Arc<Hub>, shutdown: watch::Receiver<bool>) -> Result<()> {
    let state = AppState {
        hub,
        assets: Arc::new(assets::Assets::load(web)?),
        shutdown: shutdown.clone(),
    };
    let address = web.listen_address;
    let listener = TcpListener::bind(address).await.map_err(|source| Error::Bind {
        address: address.to_string(),
        source,
    })?;
    tracing::info!(
        "web server listening on http://{address}/ (API under {})",
        web.api_prefix
    );
    let mut stop = shutdown;
    axum::serve(listener, router(state, &web.api_prefix))
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|&s| s).await;
        })
        .await
        .map_err(Error::Serve)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_body() {
        let r = problem(StatusCode::NOT_FOUND, "x");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "application/problem+json");
    }

    #[test]
    fn error_is_a_500_problem() {
        let r = Error::Invalid {
            what: "x",
            input: "y".into(),
            reason: "z".into(),
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
