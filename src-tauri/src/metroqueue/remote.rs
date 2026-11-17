//! The controller side: this app discovers, pairs with and remote-controls other receivers
//! (MetroQueue phones, which advertise `kind=phone`, TVs, other desktops), as a phone does in
//! PROTOCOL.md §4, §6 and §7. Tokens for the receivers paired here live in
//! `metroqueue-remotes.json` in the app data dir; they are secrets and never logged.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{header, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;

use super::control;
use super::protocol::{
    InfoReply, PairReply, QueueSnapshot, RemoteState, RemoteTrack, SERVICE_TYPE,
};
use crate::state::AppState;

const STORE_FILE: &str = "metroqueue-remotes.json";
/// The phone's reconnect delays (PROTOCOL.md §6), then it gives up.
const BACKOFF_SECS: [u64; 6] = [1, 2, 3, 5, 8, 10];
const PING_EVERY: Duration = Duration::from_secs(20);
/// Receivers ping every 15 s, so this long without a frame means the link is gone.
const SILENCE_LIMIT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const READ_TIMEOUT: Duration = Duration::from_secs(15);

/// Error codes handed to the UI, which words them.
type Fail = &'static str;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    id: String,
    name: String,
    kind: String,
    /// Last known `ip:port`, for connecting before discovery has answered.
    addr: String,
    token: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    receivers: Vec<Saved>,
}

#[derive(Clone)]
struct Found {
    id: String,
    name: String,
    kind: String,
    addr: String,
    fullname: String,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Connected,
    Reconnecting,
}

struct Conn {
    gen: u64,
    id: String,
    name: String,
    kind: String,
    addr: String,
    token: String,
    status: Status,
    state: RemoteState,
    queue: Vec<RemoteTrack>,
    /// Dropping or flipping it ends the connection task.
    stop: watch::Sender<bool>,
}

#[derive(Serialize)]
pub struct ReceiverView {
    id: String,
    name: String,
    kind: String,
    online: bool,
    paired: bool,
}

#[derive(Serialize)]
pub struct RemoteView {
    id: String,
    name: String,
    kind: String,
    status: Status,
    state: RemoteState,
    queue: Vec<RemoteTrack>,
}

enum Ended {
    Stopped,
    /// Closed `1008`: the receiver forgot this computer.
    Revoked,
    /// The socket opened, then dropped.
    Dropped,
    /// It never opened.
    Failed,
}

pub struct Controller {
    app: AppHandle,
    /// This app's own receiver id, left out of the list.
    own_id: String,
    store_path: PathBuf,
    saved: Mutex<Vec<Saved>>,
    found: Mutex<Vec<Found>>,
    browser: Mutex<Option<ServiceDaemon>>,
    picker_open: AtomicBool,
    conn: Mutex<Option<Conn>>,
    gen: AtomicU64,
    http: reqwest::Client,
}

pub(super) fn init(app: &AppHandle, dir: PathBuf, own_id: String) {
    let store_path = dir.join(STORE_FILE);
    let store: Store = std::fs::read(&store_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // LAN only, so never through the user's proxy (`crate::http::client` would use it).
    let http = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(READ_TIMEOUT)
        .build()
        .unwrap_or_default();
    app.manage(Arc::new(Controller {
        app: app.clone(),
        own_id,
        store_path,
        saved: Mutex::new(store.receivers),
        found: Default::default(),
        browser: Default::default(),
        picker_open: AtomicBool::new(false),
        conn: Default::default(),
        gen: AtomicU64::new(0),
        http,
    }));
}

fn controller(app: &AppHandle) -> Arc<Controller> {
    app.state::<Arc<Controller>>().inner().clone()
}

/// Delay before reconnect try `attempt` (0-based), `None` once they are used up.
fn backoff(attempt: usize) -> Option<Duration> {
    BACKOFF_SECS.get(attempt).map(|s| Duration::from_secs(*s))
}

/// `Living room TV._metroqueue._tcp.local.` -> `Living room TV`.
fn instance_name(fullname: &str) -> String {
    fullname.strip_suffix(SERVICE_TYPE).unwrap_or(fullname).trim_end_matches('.').to_owned()
}

/// `/api/pair/start` and `/api/pair/finish` answers (PROTOCOL.md §4).
fn pair_result(status: StatusCode) -> Result<(), Fail> {
    match status.as_u16() {
        200 | 204 => Ok(()),
        401 => Err("wrong_code"),
        410 => Err("expired"),
        429 => Err("locked"),
        404 => Err("not_receiver"),
        _ => Err("failed"),
    }
}

fn post_result(status: StatusCode) -> Result<(), Fail> {
    match status.as_u16() {
        200 | 204 => Ok(()),
        403 => Err("not_paired"),
        409 => Err("busy"),
        _ => Err("failed"),
    }
}

fn app_state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

impl Controller {
    async fn call(
        &self,
        method: Method,
        addr: &str,
        path: &str,
        token: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(StatusCode, Vec<u8>), Fail> {
        let mut req = self.http.request(method, format!("http://{addr}{path}"));
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.header(reqwest::header::CONTENT_TYPE, "application/json").body(body);
        }
        let resp = req.send().await.map_err(|_| "unreachable")?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|_| "unreachable")?;
        Ok((status, bytes.to_vec()))
    }

    async fn get<T: DeserializeOwned>(
        &self,
        addr: &str,
        path: &str,
        token: &str,
    ) -> Result<T, Fail> {
        let (status, body) = self.call(Method::GET, addr, path, token, None).await?;
        match status {
            StatusCode::OK => serde_json::from_slice(&body).map_err(|_| "failed"),
            StatusCode::FORBIDDEN => Err("not_paired"),
            StatusCode::NOT_FOUND => Err("not_receiver"),
            _ => Err("failed"),
        }
    }

    async fn post(
        &self,
        addr: &str,
        path: &str,
        token: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(), Fail> {
        post_result(self.call(Method::POST, addr, path, token, body).await?.0)
    }

    fn persist(&self, saved: &[Saved]) {
        let store = Store { receivers: saved.to_vec() };
        if let Err(e) = super::save(&self.store_path, &store) {
            tracing::warn!(error = %e, "metroqueue: could not save paired receivers");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.store_path, std::fs::Permissions::from_mode(0o600));
        }
    }

    fn saved_one(&self, id: &str) -> Option<Saved> {
        self.saved.lock().unwrap().iter().find(|s| s.id == id).cloned()
    }

    fn found_one(&self, id: &str) -> Option<Found> {
        self.found.lock().unwrap().iter().find(|f| f.id == id).cloned()
    }

    fn receivers_changed(&self) {
        let _ = self.app.emit("mq://receivers-changed", ());
    }

    fn views(&self) -> Vec<ReceiverView> {
        let found = self.found.lock().unwrap().clone();
        let saved = self.saved.lock().unwrap().clone();
        let mut out: Vec<ReceiverView> = found
            .iter()
            .map(|f| ReceiverView {
                id: f.id.clone(),
                name: f.name.clone(),
                kind: f.kind.clone(),
                online: true,
                paired: saved.iter().any(|s| s.id == f.id),
            })
            .collect();
        for s in saved.iter().filter(|s| !found.iter().any(|f| f.id == s.id)) {
            out.push(ReceiverView {
                id: s.id.clone(),
                name: s.name.clone(),
                kind: s.kind.clone(),
                online: false,
                paired: true,
            });
        }
        out.sort_by_cached_key(|v| v.name.to_lowercase());
        out
    }

    // ---- discovery -----------------------------------------------------------------------------

    /// Browse while the picker is open or a dropped connection is looking for its receiver again,
    /// like the phone.
    fn sync_browse(self: &Arc<Self>) {
        let reconnecting =
            self.conn.lock().unwrap().as_ref().is_some_and(|c| c.status == Status::Reconnecting);
        let want = reconnecting || self.picker_open.load(Ordering::Relaxed);
        let mut browser = self.browser.lock().unwrap();
        if want && browser.is_none() {
            *browser = self.browse();
        } else if !want {
            if let Some(daemon) = browser.take() {
                let _ = daemon.shutdown();
                self.found.lock().unwrap().clear();
            }
        }
    }

    fn browse(self: &Arc<Self>) -> Option<ServiceDaemon> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| tracing::warn!(error = %e, "metroqueue: mDNS browse failed"))
            .ok()?;
        let events = match daemon.browse(SERVICE_TYPE) {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!(error = %e, "metroqueue: mDNS browse failed");
                let _ = daemon.shutdown();
                return None;
            }
        };
        let me = self.clone();
        // Ends when the daemon shuts down and drops its end of the channel.
        let spawned = std::thread::Builder::new().name("mq-browse".into()).spawn(move || {
            while let Ok(event) = events.recv() {
                match event {
                    ServiceEvent::ServiceResolved(s) => me.on_found(&s),
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        me.found.lock().unwrap().retain(|f| f.fullname != fullname);
                        me.receivers_changed();
                    }
                    _ => {}
                }
            }
        });
        if spawned.is_err() {
            let _ = daemon.shutdown();
            return None;
        }
        Some(daemon)
    }

    // shortcut: IPv4 only, like the receiver's listener. Phones resolve and prefer IPv4 too.
    fn on_found(&self, s: &ResolvedService) {
        let Some(id) = s.get_property_val_str("id").filter(|id| !id.is_empty()) else { return };
        if id == self.own_id {
            return;
        }
        let Some(ip) = s.get_addresses_v4().into_iter().next() else { return };
        let found = Found {
            id: id.to_owned(),
            name: instance_name(s.get_fullname()),
            kind: s.get_property_val_str("kind").unwrap_or("tv").to_owned(),
            addr: format!("{ip}:{}", s.get_port()),
            fullname: s.get_fullname().to_owned(),
        };
        {
            let mut all = self.found.lock().unwrap();
            all.retain(|f| f.id != found.id && f.fullname != found.fullname);
            all.push(found);
        }
        self.receivers_changed();
    }

    fn addr_of(&self, id: &str) -> Option<String> {
        self.found_one(id).map(|f| f.addr).or_else(|| self.saved_one(id).map(|s| s.addr))
    }

    // ---- pairing -------------------------------------------------------------------------------

    async fn pair_start(&self, id: &str) -> Result<(), Fail> {
        let addr = self.addr_of(id).ok_or("unreachable")?;
        let body = serde_json::json!({ "name": super::computer_name() }).to_string();
        let (status, _) =
            self.call(Method::POST, &addr, "/api/pair/start", "", Some(body.into())).await?;
        pair_result(status)
    }

    async fn pair_finish(&self, id: &str, code: &str) -> Result<(), Fail> {
        let found = self.found_one(id).ok_or("unreachable")?;
        let name = super::computer_name();
        let body = serde_json::json!({ "code": code.trim(), "name": name }).to_string();
        let (status, body) =
            self.call(Method::POST, &found.addr, "/api/pair/finish", "", Some(body.into())).await?;
        pair_result(status)?;
        let reply: PairReply = serde_json::from_slice(&body).map_err(|_| "failed")?;
        if reply.token.is_empty() {
            return Err("failed");
        }
        let saved = Saved {
            id: found.id,
            name: if reply.tv_name.is_empty() { found.name } else { reply.tv_name },
            kind: found.kind,
            addr: found.addr,
            token: reply.token,
        };
        {
            let mut all = self.saved.lock().unwrap();
            all.retain(|s| s.id != saved.id);
            all.push(saved);
            self.persist(&all);
        }
        self.receivers_changed();
        Ok(())
    }

    fn forget(self: &Arc<Self>, id: &str) {
        {
            let mut all = self.saved.lock().unwrap();
            all.retain(|s| s.id != id);
            self.persist(&all);
        }
        if self.conn.lock().unwrap().as_ref().is_some_and(|c| c.id == id) {
            self.close();
        }
        self.receivers_changed();
    }

    // ---- remote mode ---------------------------------------------------------------------------

    fn view(&self) -> Option<RemoteView> {
        self.conn.lock().unwrap().as_ref().map(|c| RemoteView {
            id: c.id.clone(),
            name: c.name.clone(),
            kind: c.kind.clone(),
            status: c.status,
            state: c.state.clone(),
            queue: c.queue.clone(),
        })
    }

    fn emit_remote(&self) {
        let _ = self.app.emit("mq://remote", &self.view());
    }

    /// The current connection's address and token, if it is still `gen`.
    fn target(&self, gen: Option<u64>) -> Option<(u64, String, String)> {
        let conn = self.conn.lock().unwrap();
        let c = conn.as_ref().filter(|c| gen.map_or(true, |g| g == c.gen))?;
        Some((c.gen, c.addr.clone(), c.token.clone()))
    }

    fn update(&self, gen: u64, f: impl FnOnce(&mut Conn)) -> bool {
        let mut conn = self.conn.lock().unwrap();
        let Some(c) = conn.as_mut().filter(|c| c.gen == gen) else { return false };
        f(c);
        true
    }

    fn set_status(self: &Arc<Self>, gen: u64, status: Status) {
        if self.update(gen, |c| c.status = status) {
            self.emit_remote();
            self.sync_browse();
        }
    }

    /// Ends remote mode without telling the receiver anything (it keeps playing).
    pub(super) fn close(self: &Arc<Self>) {
        let Some(conn) = self.conn.lock().unwrap().take() else { return };
        let _ = conn.stop.send(true);
        self.emit_remote();
        self.sync_browse();
    }

    fn give_up(self: &Arc<Self>, gen: u64, reason: Fail) {
        let id = match self.conn.lock().unwrap().as_ref().filter(|c| c.gen == gen) {
            Some(c) => c.id.clone(),
            None => return,
        };
        if reason == "not_paired" {
            self.forget(&id);
        }
        self.close();
        let _ = self.app.emit("mq://remote-ended", reason);
    }

    async fn connect(self: &Arc<Self>, id: &str, move_here: bool) -> Result<(), Fail> {
        let saved = self.saved_one(id).ok_or("not_paired")?;
        let token = saved.token.clone();
        let addr = self.addr_of(id).ok_or("unreachable")?;
        let info: InfoReply = self.get(&addr, "/api/info", &token).await?;
        if !info.paired {
            self.forget(id);
            return Err("not_paired");
        }
        let st = app_state(&self.app);
        if move_here {
            let snap = QueueSnapshot { playing: true, ..control::snapshot(&st).await };
            if !snap.items.is_empty() {
                let body = serde_json::to_vec(&snap).map_err(|_| "failed")?;
                self.post(&addr, "/api/load", &token, Some(body)).await?;
            }
        }
        let state: RemoteState = self.get(&addr, "/api/state", &token).await?;
        let queue: Vec<RemoteTrack> = self.get(&addr, "/api/queue", &token).await?;
        let _ = st.player.pause();
        self.close();
        if saved.addr != addr {
            let mut all = self.saved.lock().unwrap();
            if let Some(s) = all.iter_mut().find(|s| s.id == id) {
                s.addr = addr.clone();
            }
            self.persist(&all);
        }
        let gen = self.gen.fetch_add(1, Ordering::Relaxed) + 1;
        let (stop, stop_rx) = watch::channel(false);
        *self.conn.lock().unwrap() = Some(Conn {
            gen,
            id: id.to_owned(),
            name: if info.name.is_empty() { saved.name } else { info.name },
            kind: info.kind,
            addr,
            token,
            status: Status::Connected,
            state,
            queue,
            stop,
        });
        self.emit_remote();
        tauri::async_runtime::spawn(self.clone().run(gen, stop_rx));
        Ok(())
    }

    /// Keeps the WebSocket open, reconnecting after 1, 2, 3, 5, 8, 10 s when it drops.
    async fn run(self: Arc<Self>, gen: u64, mut stop: watch::Receiver<bool>) {
        let mut attempt = 0;
        loop {
            let Some((_, addr, token)) = self.target(Some(gen)) else { return };
            match self.session(gen, &addr, &token, &mut stop).await {
                Ended::Stopped => return,
                Ended::Revoked => return self.give_up(gen, "not_paired"),
                Ended::Dropped => attempt = 0,
                Ended::Failed => {}
            }
            self.set_status(gen, Status::Reconnecting);
            loop {
                let Some(delay) = backoff(attempt) else { return self.give_up(gen, "lost") };
                attempt += 1;
                tokio::select! {
                    _ = stop.changed() => return,
                    _ = tokio::time::sleep(delay) => {}
                }
                match self.revive(gen).await {
                    Ok(()) => break,
                    Err("not_paired") => return self.give_up(gen, "not_paired"),
                    Err(_) => {}
                }
            }
        }
    }

    /// One reconnect try: find the receiver again (its address may have changed), check it still
    /// knows this computer and refresh what it plays.
    async fn revive(&self, gen: u64) -> Result<(), Fail> {
        let (_, old_addr, token) = self.target(Some(gen)).ok_or("lost")?;
        let id = self.conn.lock().unwrap().as_ref().map(|c| c.id.clone()).unwrap_or_default();
        let addr = self.found_one(&id).map_or(old_addr, |f| f.addr);
        let info: InfoReply = self.get(&addr, "/api/info", &token).await?;
        if !info.paired {
            return Err("not_paired");
        }
        let state: RemoteState = self.get(&addr, "/api/state", &token).await?;
        let queue: Vec<RemoteTrack> = self.get(&addr, "/api/queue", &token).await?;
        self.update(gen, |c| {
            c.addr = addr;
            c.state = state;
            c.queue = queue;
        });
        Ok(())
    }

    async fn session(
        self: &Arc<Self>,
        gen: u64,
        addr: &str,
        token: &str,
        stop: &mut watch::Receiver<bool>,
    ) -> Ended {
        let Ok(mut req) = format!("ws://{addr}/api/ws").into_client_request() else {
            return Ended::Failed;
        };
        let Ok(auth) = HeaderValue::from_str(&format!("Bearer {token}")) else {
            return Ended::Failed;
        };
        req.headers_mut().insert(header::AUTHORIZATION, auth);
        let ws = match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(req))
            .await
        {
            Ok(Ok((ws, _))) => ws,
            _ => return Ended::Failed,
        };
        self.set_status(gen, Status::Connected);
        let (mut sink, mut incoming) = ws.split();
        let mut ping =
            tokio::time::interval_at(tokio::time::Instant::now() + PING_EVERY, PING_EVERY);
        let mut heard = Instant::now();
        loop {
            tokio::select! {
                _ = stop.changed() => {
                    let _ = sink.send(Message::Close(None)).await;
                    return Ended::Stopped;
                }
                _ = ping.tick() => {
                    if heard.elapsed() > SILENCE_LIMIT
                        || sink.send(Message::Ping(Vec::new())).await.is_err()
                    {
                        return Ended::Dropped;
                    }
                }
                msg = incoming.next() => match msg {
                    Some(Ok(Message::Text(text))) => {
                        heard = Instant::now();
                        if let Ok(state) = serde_json::from_str::<RemoteState>(&text) {
                            self.on_state(gen, addr, token, state).await;
                        }
                    }
                    // tungstenite queues the pong for the receiver's ping; flush so it goes now.
                    Some(Ok(Message::Ping(_))) => {
                        heard = Instant::now();
                        if sink.flush().await.is_err() {
                            return Ended::Dropped;
                        }
                    }
                    Some(Ok(Message::Close(Some(frame)))) if frame.code == CloseCode::Policy => {
                        return Ended::Revoked;
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Ended::Dropped,
                    Some(Ok(_)) => heard = Instant::now(),
                },
            }
        }
    }

    /// A new `RemoteState`; the queue is refetched only when its hash moved.
    async fn on_state(&self, gen: u64, addr: &str, token: &str, state: RemoteState) {
        let old_hash = self.conn.lock().unwrap().as_ref().map(|c| c.state.queue_hash);
        let queue = if old_hash != Some(state.queue_hash) {
            self.get::<Vec<RemoteTrack>>(addr, "/api/queue", token).await.ok()
        } else {
            None
        };
        let changed = self.update(gen, |c| {
            c.state = state;
            if let Some(queue) = queue {
                c.queue = queue;
            }
        });
        if changed {
            self.emit_remote();
        }
    }

    async fn command(&self, action: &str, a: i64, b: i64) -> Result<(), Fail> {
        let (gen, addr, token) = self.target(None).ok_or("lost")?;
        let path = format!("/api/cmd?action={}&a={a}&b={b}", urlencoding::encode(action));
        self.post(&addr, &path, &token, None).await?;
        // As the phone does: read the state back at once rather than wait for the push.
        if let Ok(state) = self.get::<RemoteState>(&addr, "/api/state", &token).await {
            self.on_state(gen, &addr, &token, state).await;
        }
        Ok(())
    }

    /// Leave remote mode. With `continue_here`, take the receiver's queue over ("Play here"):
    /// snapshot, pause it there, play it here.
    async fn disconnect(self: &Arc<Self>, continue_here: bool) -> Result<(), Fail> {
        let Some((_, addr, token)) = self.target(None) else { return Ok(()) };
        if !continue_here {
            self.close();
            return Ok(());
        }
        let snap: QueueSnapshot = self.get(&addr, "/api/snapshot", &token).await?;
        let _ = self.post(&addr, "/api/cmd?action=pause&a=0&b=0", &token, None).await;
        self.close();
        control::load(&app_state(&self.app), snap).await.map_err(|_| "busy")
    }
}

#[tauri::command]
pub fn mq_remote_discover(app: AppHandle, open: bool) {
    let c = controller(&app);
    c.picker_open.store(open, Ordering::Relaxed);
    c.sync_browse();
}

#[tauri::command]
pub fn mq_remote_receivers(app: AppHandle) -> Vec<ReceiverView> {
    controller(&app).views()
}

#[tauri::command]
pub async fn mq_remote_pair_start(app: AppHandle, id: String) -> Result<(), String> {
    controller(&app).pair_start(&id).await.map_err(str::to_owned)
}

#[tauri::command]
pub async fn mq_remote_pair_finish(app: AppHandle, id: String, code: String) -> Result<(), String> {
    controller(&app).pair_finish(&id, &code).await.map_err(str::to_owned)
}

#[tauri::command]
pub fn mq_remote_forget(app: AppHandle, id: String) {
    controller(&app).forget(&id);
}

#[tauri::command]
pub async fn mq_remote_connect(app: AppHandle, id: String, move_here: bool) -> Result<(), String> {
    controller(&app).connect(&id, move_here).await.map_err(str::to_owned)
}

#[tauri::command]
pub async fn mq_remote_disconnect(app: AppHandle, continue_here: bool) -> Result<(), String> {
    controller(&app).disconnect(continue_here).await.map_err(str::to_owned)
}

#[tauri::command]
pub async fn mq_remote_cmd(app: AppHandle, action: String, a: i64, b: i64) -> Result<(), String> {
    controller(&app).command(&action, a, b).await.map_err(str::to_owned)
}

#[tauri::command]
pub fn mq_remote_status(app: AppHandle) -> Option<RemoteView> {
    controller(&app).view()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_follows_the_phone_then_gives_up() {
        let delays: Vec<u64> = (0..).map_while(backoff).map(|d| d.as_secs()).collect();
        assert_eq!(delays, [1, 2, 3, 5, 8, 10]);
        assert_eq!(backoff(6), None);
    }

    #[test]
    fn instance_names_lose_the_service_type() {
        assert_eq!(instance_name("Living room TV._metroqueue._tcp.local."), "Living room TV");
        assert_eq!(instance_name("Pixel 9"), "Pixel 9");
    }

    #[test]
    fn pairing_statuses_map_to_messages() {
        assert_eq!(pair_result(StatusCode::NO_CONTENT), Ok(()));
        assert_eq!(pair_result(StatusCode::OK), Ok(()));
        assert_eq!(pair_result(StatusCode::UNAUTHORIZED), Err("wrong_code"));
        assert_eq!(pair_result(StatusCode::GONE), Err("expired"));
        assert_eq!(pair_result(StatusCode::TOO_MANY_REQUESTS), Err("locked"));
        assert_eq!(pair_result(StatusCode::NOT_FOUND), Err("not_receiver"));
        assert_eq!(post_result(StatusCode::CONFLICT), Err("busy"));
    }

    #[test]
    fn remote_state_reads_with_defaults() {
        let s: RemoteState = serde_json::from_str(r#"{"playing":true,"queueHash":7}"#).unwrap();
        assert!(s.playing && s.track.is_none());
        assert_eq!((s.queue_hash, s.index, s.playback_state, s.speed), (7, -1, 1, 1.0));
        let i: InfoReply = serde_json::from_str(r#"{"id":"x","name":"TV","paired":true}"#).unwrap();
        assert_eq!((i.kind.as_str(), i.paired), ("tv", true));
    }
}
