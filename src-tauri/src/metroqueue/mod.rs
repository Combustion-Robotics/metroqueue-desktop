//! MetroQueue receiver, "Play on desktop": MetroQueue phones discover, pair with and control this
//! app exactly like a MetroQueue TV, which announces itself as `kind=desktop`. The protocol is
//! docs/metroqueue/PROTOCOL.md; `/api/sync` is served by [`crate::mqsync`] (SYNC.md).
//!
//! Paired phones (token hashes only) and this receiver's stable id live in `metroqueue.json` in
//! the app data dir. On by default; the `metroqueue_receiver` setting (`"false"`) turns it off.
//!
//! The other direction, this app controlling phones and other receivers, is [`remote`].

mod control;
mod mdns;
mod pairing;
mod protocol;
pub mod remote;
mod server;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::watch;

use crate::state::AppState;
use pairing::Finish;

const SETTING: &str = "metroqueue_receiver";
const STORE_FILE: &str = "metroqueue.json";

/// Player events that change what a phone shows. `position` is handled apart: it ticks at 4 Hz
/// and only a jump in it (a seek) is news, the phone extrapolates the rest.
const PUSHED_EVENTS: [&str; 8] = [
    "now-playing",
    "playback-state",
    "queue-changed",
    "queue-index",
    "queue-appended",
    "duration",
    "volume",
    "playback-params",
];

/// A phone paired with this computer. Only a hash of its token is kept.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairedDevice {
    id: String,
    name: String,
    token_hash: String,
    paired_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Store {
    device_id: String,
    devices: Vec<PairedDevice>,
}

struct Running {
    stop: watch::Sender<bool>,
    /// Unregisters on drop.
    _advert: Option<mdns::Advert>,
}

pub struct Receiver {
    app: AppHandle,
    /// Stable receiver id (TXT `id`, `tvId`).
    id: String,
    /// The computer's name, shown in the phone's picker.
    name: String,
    store_path: PathBuf,
    paired: Mutex<Vec<PairedDevice>>,
    pairing: Mutex<pairing::Pairing>,
    running: Mutex<Option<Running>>,
    /// Bumped on every player change; each open WebSocket pushes a state when it moves.
    changed: watch::Sender<u64>,
    /// Bumped when a phone is forgotten, so its WebSocket closes at once.
    devices: watch::Sender<u64>,
    /// The last position tick (secs) and when it came, to spot seeks.
    position: Mutex<(f64, Instant)>,
}

/// Load the store, register the change listeners and start serving when enabled. Called once from
/// `setup`, after `AppState` is managed.
pub fn init(app: &AppHandle) {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
    let store_path = dir.join(STORE_FILE);
    let mut store = match std::fs::read(&store_path) {
        Ok(bytes) => serde_json::from_slice::<Store>(&bytes).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "metroqueue: unreadable store, starting over");
            Store::default()
        }),
        Err(_) => Store::default(),
    };
    if store.device_id.is_empty() {
        store.device_id = pairing::new_uuid();
        if let Err(e) = save(&store_path, &store) {
            tracing::warn!(error = %e, "metroqueue: could not save the receiver id");
        }
    }
    remote::init(app, dir, store.device_id.clone());
    let rx = Arc::new(Receiver {
        app: app.clone(),
        id: store.device_id,
        name: computer_name(),
        store_path,
        paired: Mutex::new(store.devices),
        pairing: Default::default(),
        running: Default::default(),
        changed: watch::Sender::new(0),
        devices: watch::Sender::new(0),
        position: Mutex::new((0.0, Instant::now())),
    });
    app.manage(rx.clone());
    rx.listen();
    if enabled(app) {
        rx.start();
    }
}

fn enabled(app: &AppHandle) -> bool {
    app.state::<Arc<AppState>>().db.get_setting(SETTING).as_deref() != Some("false")
}

/// This computer's name, without a domain.
pub(crate) fn computer_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let name = host.split('.').next().unwrap_or_default().trim();
    if name.is_empty() {
        "MetroQueue Desktop".into()
    } else {
        name.to_owned()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Written whole to a temporary file and renamed over, so a crash can't leave half a store.
fn save(path: &Path, store: &impl Serialize) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(store)?)?;
    std::fs::rename(tmp, path)
}

/// A position tick that playback could not have produced since the previous one: a seek, or a
/// new track starting at 0.
fn is_jump(prev: f64, elapsed: f64, pos: f64) -> bool {
    (pos - prev).abs() > 1.0 && (pos - prev - elapsed).abs() > 1.0
}

impl Receiver {
    fn start(self: &Arc<Self>) {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            return;
        }
        let (stop, stop_rx) = watch::channel(false);
        if server::start(self.clone(), stop_rx).is_none() {
            return;
        }
        tracing::info!(name = %self.name, "metroqueue: receiver listening on {}", protocol::PORT);
        *running = Some(Running { stop, _advert: mdns::advertise(&self.name, &self.id) });
    }

    fn stop(&self) {
        if let Some(running) = self.running.lock().unwrap().take() {
            let _ = running.stop.send(true);
        }
        self.cancel_pairing();
    }

    fn listen(self: &Arc<Self>) {
        for event in PUSHED_EVENTS {
            let me = self.clone();
            self.app.listen_any(event, move |_| me.bump());
        }
        let me = self.clone();
        self.app.listen_any("position", move |e| {
            let pos = serde_json::from_str::<serde_json::Value>(e.payload())
                .ok()
                .and_then(|v| v["position"].as_f64());
            let Some(pos) = pos else { return };
            let now = Instant::now();
            let (prev, at) = std::mem::replace(&mut *me.position.lock().unwrap(), (pos, now));
            if is_jump(prev, now.duration_since(at).as_secs_f64(), pos) {
                me.bump();
            }
        });
    }

    fn bump(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }

    fn device_for(&self, token: &str) -> Option<PairedDevice> {
        if token.is_empty() {
            return None;
        }
        let hash = pairing::hash_token(token);
        let paired = self.paired.lock().unwrap();
        paired.iter().find(|d| pairing::ct_eq(d.token_hash.as_bytes(), hash.as_bytes())).cloned()
    }

    fn is_paired(&self, id: &str) -> bool {
        self.paired.lock().unwrap().iter().any(|d| d.id == id)
    }

    fn save_paired(&self, paired: &[PairedDevice]) -> bool {
        let store = Store { device_id: self.id.clone(), devices: paired.to_vec() };
        save(&self.store_path, &store)
            .map_err(|e| tracing::warn!(error = %e, "metroqueue: could not save paired phones"))
            .is_ok()
    }

    fn devices_changed(&self) {
        self.devices.send_modify(|v| *v = v.wrapping_add(1));
        let _ = self.app.emit("mq://devices-changed", ());
    }

    fn pairing_done(&self, device_name: &str, success: bool) {
        let _ = self.app.emit(
            "mq://pairing-done",
            serde_json::json!({ "device_name": device_name, "success": success }),
        );
    }

    /// `POST /api/pair/start`: show a code. False when rate limited or locked out.
    fn pair_start(self: &Arc<Self>, name: &str) -> bool {
        let (code, device_name) = {
            let mut pairing = self.pairing.lock().unwrap();
            let Some(prompt) = pairing.start(name, Instant::now()) else { return false };
            (prompt.code.clone(), prompt.device_name.clone())
        };
        // The code has to be seen, and the app may be sitting in the tray.
        crate::tray::show_main(&self.app);
        let _ = self.app.emit(
            "mq://pairing-code",
            serde_json::json!({
                "code": code,
                "device_name": device_name,
                "expires_at_ms": now_ms() + pairing::CODE_TTL.as_millis() as i64,
            }),
        );
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(pairing::CODE_TTL).await;
            if me.pairing.lock().unwrap().expire(&code) {
                me.pairing_done(&device_name, false);
            }
        });
        true
    }

    /// `POST /api/pair/finish`. The token comes back only with [`Finish::Paired`].
    fn pair_finish(&self, code: &str, name: &str) -> (Finish, Option<String>) {
        let result = self.pairing.lock().unwrap().finish(code, Instant::now());
        let name = pairing::clean_name(name);
        match result {
            Finish::Paired => {
                let token = pairing::new_token();
                let device = PairedDevice {
                    id: pairing::new_uuid(),
                    name: name.clone(),
                    token_hash: pairing::hash_token(&token),
                    paired_at: now_ms(),
                };
                let saved = {
                    let mut paired = self.paired.lock().unwrap();
                    paired.push(device);
                    let saved = self.save_paired(&paired);
                    if !saved {
                        paired.pop();
                    }
                    saved
                };
                self.pairing_done(&name, saved);
                if !saved {
                    return (Finish::Expired, None);
                }
                tracing::info!(phone = %name, "metroqueue: paired");
                self.devices_changed();
                (Finish::Paired, Some(token))
            }
            Finish::WrongCode => (result, None),
            Finish::Expired | Finish::Locked => {
                self.pairing_done(&name, false);
                (result, None)
            }
        }
    }

    fn cancel_pairing(&self) {
        let name = self.pairing.lock().unwrap().cancel();
        if let Some(device_name) = name {
            self.pairing_done(&device_name, false);
        }
    }
}

fn receiver(app: &AppHandle) -> Arc<Receiver> {
    app.state::<Arc<Receiver>>().inner().clone()
}

#[tauri::command]
pub fn mq_get_enabled(app: AppHandle) -> bool {
    enabled(&app)
}

#[tauri::command]
pub fn mq_set_enabled(app: AppHandle, enabled: bool) {
    let st = app.state::<Arc<AppState>>();
    st.db.set_setting(SETTING, if enabled { "true" } else { "false" });
    let rx = receiver(&app);
    if enabled {
        rx.start();
    } else {
        rx.stop();
    }
}

#[derive(Serialize)]
pub struct DeviceView {
    id: String,
    name: String,
    paired_at_ms: i64,
}

#[tauri::command]
pub fn mq_paired_devices(app: AppHandle) -> Vec<DeviceView> {
    let rx = receiver(&app);
    let paired = rx.paired.lock().unwrap();
    paired
        .iter()
        .map(|d| DeviceView { id: d.id.clone(), name: d.name.clone(), paired_at_ms: d.paired_at })
        .collect()
}

/// Revoke a phone: its token stops working at once and its WebSocket closes `1008 revoked`.
#[tauri::command]
pub fn mq_forget_device(app: AppHandle, id: String) {
    let rx = receiver(&app);
    {
        let mut paired = rx.paired.lock().unwrap();
        let before = paired.len();
        paired.retain(|d| d.id != id);
        if paired.len() == before {
            return;
        }
        rx.save_paired(&paired);
    }
    rx.devices_changed();
}

#[tauri::command]
pub fn mq_cancel_pairing(app: AppHandle) {
    receiver(&app).cancel_pairing();
}

#[cfg(test)]
mod tests {
    use super::is_jump;

    #[test]
    fn only_seeks_count_as_jumps() {
        assert!(!is_jump(10.0, 0.25, 10.25)); // playing
        assert!(!is_jump(10.0, 0.25, 10.5)); // playing at 2x
        assert!(!is_jump(10.0, 60.0, 10.0)); // paused
        assert!(is_jump(10.0, 0.25, 95.0)); // seek forward
        assert!(is_jump(95.0, 0.25, 0.0)); // seek back / next track
        assert!(is_jump(10.0, 60.0, 40.0)); // seek while paused
    }

    #[test]
    fn store_reads_the_tv_shape() {
        let raw = r#"{"deviceId":"x","devices":[{"id":"p","name":"Pixel","tokenHash":"ab","pairedAt":5}]}"#;
        let s: super::Store = serde_json::from_str(raw).unwrap();
        assert_eq!((s.device_id.as_str(), s.devices[0].paired_at), ("x", 5));
    }
}
