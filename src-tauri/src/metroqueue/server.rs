//! The receiver's HTTP + WebSocket server on TCP 8642 (PROTOCOL.md §1, §5, §6), MetroQueue's
//! `RemoteControlServer` minus the browser remote. LAN peers only; everything but `/api/info` and
//! `/api/pair/*` needs a paired phone's bearer token.

use std::convert::Infallible;
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{self, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::Serialize;
use tauri::Manager;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Role};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::control::{self, Refused};
use super::pairing::Finish;
use super::protocol::{
    is_lan, Info, ItemsRequest, PairFinish, PairFinishResponse, PairStart, QueueSnapshot, KIND,
    PORT, PROTOCOL_VERSION, SYNC_VERSION,
};
use super::Receiver;
use crate::state::AppState;

const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_UPLOAD_JSON_BYTES: u64 = 64 * 1024 * 1024;
const PING_EVERY: Duration = Duration::from_secs(15);
const PONG_TIMEOUT: Duration = Duration::from_secs(30);
/// Bursts of player events collapse into one push, like the TV's.
const PUSH_COALESCE: Duration = Duration::from_millis(100);
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

type Body = Full<Bytes>;

/// Bind 8642 and serve until `stop` flips. `None` when the port is taken.
// shortcut: IPv4 only (phones prefer the IPv4 address they resolve). Bind [::] too if a phone on
// an IPv6-only network ever needs it.
pub fn start(rx: Arc<Receiver>, mut stop: watch::Receiver<bool>) -> Option<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, PORT))
        .and_then(|l| l.set_nonblocking(true).map(|_| l))
        .map_err(|e| tracing::warn!(error = %e, "metroqueue: cannot listen on port {PORT}"))
        .ok()?;
    tauri::async_runtime::spawn(async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { return };
        loop {
            let (stream, peer) = tokio::select! {
                _ = stop.changed() => return,
                accepted = listener.accept() => match accepted {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "metroqueue: accept failed");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                },
            };
            let rx = rx.clone();
            let stop = stop.clone();
            tauri::async_runtime::spawn(async move {
                let svc = service_fn(move |req| serve(req, peer, rx.clone(), stop.clone()));
                let _ = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(15))
                    .serve_connection(TokioIo::new(stream), svc)
                    .with_upgrades()
                    .await;
            });
        }
    });
    Some(())
}

fn status(code: StatusCode) -> Response<Body> {
    let mut r = Response::new(Body::default());
    *r.status_mut() = code;
    r
}

fn json(value: &impl Serialize) -> Response<Body> {
    match serde_json::to_vec(value) {
        Ok(bytes) => {
            let mut r = Response::new(Body::from(bytes));
            r.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            r
        }
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Run a control request on its own task and answer when it ends or after a few seconds, whichever
/// comes first. Starting a track waits for YouTube, and a slow answer must not run into the
/// phone's 15 s read timeout; the request still completes, and the WebSocket reports it.
async fn detached(
    work: impl std::future::Future<Output = Result<(), Refused>> + Send + 'static,
) -> Result<(), Refused> {
    let task = tauri::async_runtime::spawn(work);
    match tokio::time::timeout(ANSWER_WITHIN, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Refused::Rejected),
        Err(_) => Ok(()),
    }
}

fn refused(result: Result<(), Refused>) -> Response<Body> {
    status(match result {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(Refused::Rejected) => StatusCode::BAD_REQUEST,
        Err(Refused::Busy) => StatusCode::CONFLICT,
    })
}

/// The request body, at most 16 MiB on the wire, inflated (at most 64 MiB) when gzipped.
async fn body_bytes(req: Request<Incoming>) -> Option<Vec<u8>> {
    let gzipped = req
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"gzip"));
    let raw = Limited::new(req.into_body(), MAX_UPLOAD_BYTES).collect().await.ok()?.to_bytes();
    if !gzipped {
        return Some(raw.to_vec());
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .take(MAX_UPLOAD_JSON_BYTES + 1)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() as u64 <= MAX_UPLOAD_JSON_BYTES).then_some(out)
}

async fn body_json<T: serde::de::DeserializeOwned>(req: Request<Incoming>) -> Option<T> {
    serde_json::from_slice(&body_bytes(req).await?).ok()
}

fn bearer(req: &Request<Incoming>) -> &str {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer").unwrap_or(v).trim())
        .unwrap_or_default()
}

fn query(req: &Request<Incoming>, key: &str) -> Option<String> {
    req.uri().query()?.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| urlencoding::decode(v).map(|v| v.into_owned()).unwrap_or_default())
    })
}

fn query_i64(req: &Request<Incoming>, key: &str) -> i64 {
    query(req, key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

async fn serve(
    req: Request<Incoming>,
    peer: SocketAddr,
    rx: Arc<Receiver>,
    stop: watch::Receiver<bool>,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_owned();
    let public = path == "/api/info" || path.starts_with("/api/pair/");
    // A keep-alive connection opened before the receiver was switched off must not outlive it.
    if *stop.borrow() || !is_lan(peer.ip()) {
        return Ok(status(if public { StatusCode::NOT_FOUND } else { StatusCode::FORBIDDEN }));
    }
    let device = rx.device_for(bearer(&req));
    let method = req.method().clone();
    let st = rx.app.state::<Arc<AppState>>().inner().clone();

    let response = match (method, path.as_str()) {
        (Method::GET, "/api/info") => json(&Info {
            id: &rx.id,
            name: &rx.name,
            version: PROTOCOL_VERSION,
            paired: device.is_some(),
            kind: KIND,
            sync: SYNC_VERSION,
        }),
        (Method::POST, "/api/pair/start") => match body_json::<PairStart>(req).await {
            None => status(StatusCode::BAD_REQUEST),
            Some(r) if rx.pair_start(&r.name) => status(StatusCode::NO_CONTENT),
            Some(_) => status(StatusCode::TOO_MANY_REQUESTS),
        },
        (Method::POST, "/api/pair/finish") => match body_json::<PairFinish>(req).await {
            None => status(StatusCode::BAD_REQUEST),
            Some(r) => match rx.pair_finish(&r.code, &r.name) {
                (Finish::Paired, Some(token)) => {
                    json(&PairFinishResponse { token: &token, tv_id: &rx.id, tv_name: &rx.name })
                }
                (Finish::WrongCode, _) => status(StatusCode::UNAUTHORIZED),
                (Finish::Locked, _) => status(StatusCode::TOO_MANY_REQUESTS),
                _ => status(StatusCode::GONE),
            },
        },
        _ if device.is_none() => status(StatusCode::FORBIDDEN),
        (Method::GET, "/api/state") => json(&control::state(&st).await),
        (Method::GET, "/api/queue") => json(&control::queue(&st).await),
        (Method::GET, "/api/snapshot") => json(&control::snapshot(&st).await),
        (Method::POST, "/api/load") => match body_json::<QueueSnapshot>(req).await {
            Some(snap) => {
                // Hand-off: whatever this app was remote-controlling, it plays here now.
                rx.app.state::<Arc<super::remote::Controller>>().close();
                refused(detached(async move { control::load(&st, snap).await }).await)
            }
            None => status(StatusCode::BAD_REQUEST),
        },
        (Method::POST, "/api/items") => match body_json::<ItemsRequest>(req).await {
            Some(r) => refused(detached(async move { control::items(&st, r).await }).await),
            None => status(StatusCode::BAD_REQUEST),
        },
        (Method::POST, "/api/cmd") => {
            let action = query(&req, "action").unwrap_or_default();
            let (a, b) = (query_i64(&req, "a"), query_i64(&req, "b"));
            let app = rx.app.clone();
            refused(detached(async move { control::command(&st, &app, &action, a, b).await }).await)
        }
        (Method::GET, "/api/sync") => {
            let since = query_i64(&req, "since");
            let app = rx.app.clone();
            match tokio::task::spawn_blocking(move || crate::mqsync::export(&app, since)).await {
                Ok(Ok(state)) => json(&state),
                _ => status(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        (Method::POST, "/api/sync") => match body_json::<serde_json::Value>(req).await {
            None => status(StatusCode::BAD_REQUEST),
            Some(body) => {
                let app = rx.app.clone();
                let merged = tokio::task::spawn_blocking(move || crate::mqsync::import(&app, body));
                match merged.await {
                    Ok(Ok(_)) => status(StatusCode::NO_CONTENT),
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "metroqueue: sync import failed");
                        status(StatusCode::BAD_REQUEST)
                    }
                    Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
                }
            }
        },
        (Method::GET, "/api/ws") => {
            upgrade(req, rx.clone(), device.map(|d| d.id).unwrap_or_default(), stop)
        }
        _ => status(StatusCode::NOT_FOUND),
    };
    Ok(response)
}

/// Answer the WebSocket handshake and hand the upgraded connection to [`push`].
fn upgrade(
    req: Request<Incoming>,
    rx: Arc<Receiver>,
    device_id: String,
    stop: watch::Receiver<bool>,
) -> Response<Body> {
    let is_ws = req
        .headers()
        .get(header::UPGRADE)
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let Some(key) = req.headers().get(header::SEC_WEBSOCKET_KEY).filter(|_| is_ws) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let accept = derive_accept_key(key.as_bytes());
    tauri::async_runtime::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let ws =
                    WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None)
                        .await;
                push(ws, rx, device_id, stop).await;
            }
            Err(e) => tracing::debug!(error = %e, "metroqueue: websocket upgrade failed"),
        }
    });
    let mut r = status(StatusCode::SWITCHING_PROTOCOLS);
    let h = r.headers_mut();
    h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    h.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(v) = HeaderValue::from_str(&accept) {
        h.insert(header::SEC_WEBSOCKET_ACCEPT, v);
    }
    r
}

fn close(code: CloseCode, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame { code, reason: reason.into() }))
}

/// Server -> phone: a `RemoteState` on open and after every change; pings every 15 s and gives up
/// after 30 s without hearing back; closes `1008 revoked` when this phone is forgotten.
async fn push<S>(
    ws: WebSocketStream<S>,
    rx: Arc<Receiver>,
    device_id: String,
    mut stop: watch::Receiver<bool>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut incoming) = ws.split();
    let mut changed = rx.changed.subscribe();
    let mut devices = rx.devices.subscribe();
    let mut ping = tokio::time::interval_at(tokio::time::Instant::now() + PING_EVERY, PING_EVERY);
    let mut heard = Instant::now();
    let send_state = |st: Arc<AppState>| async move {
        serde_json::to_string(&control::state(&st).await).map(Message::Text)
    };
    let st = rx.app.state::<Arc<AppState>>().inner().clone();
    let Ok(first) = send_state(st.clone()).await else { return };
    if sink.send(first).await.is_err() {
        return;
    }
    let bye = loop {
        tokio::select! {
            r = changed.changed() => {
                if r.is_err() {
                    break None;
                }
                let Ok(msg) = send_state(st.clone()).await else { continue };
                if sink.send(msg).await.is_err() {
                    break None;
                }
                tokio::time::sleep(PUSH_COALESCE).await;
            }
            _ = ping.tick() => {
                if heard.elapsed() > PONG_TIMEOUT || sink.send(Message::Ping(Vec::new())).await.is_err() {
                    break None;
                }
            }
            msg = incoming.next() => match msg {
                // tungstenite queues the pong for a phone's ping; flush so it goes out now.
                Some(Ok(Message::Ping(_))) => {
                    heard = Instant::now();
                    if sink.flush().await.is_err() {
                        break None;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break None,
                Some(Ok(_)) => heard = Instant::now(),
            },
            r = devices.changed() => {
                if r.is_err() {
                    break None;
                }
                if !rx.is_paired(&device_id) {
                    break Some(close(CloseCode::Policy, "revoked"));
                }
            }
            _ = stop.changed() => break Some(close(CloseCode::Away, "")),
        }
    };
    if let Some(msg) = bye {
        let _ = sink.send(msg).await;
    }
    let _ = sink.close().await;
}
