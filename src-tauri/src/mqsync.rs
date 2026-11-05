//! Two-way sync of MetroQueue-only data with a paired phone: play history, custom transitions,
//! custom vibes and a few shared settings. Format v1 and merge rules: docs/metroqueue/SYNC.md,
//! ported from MetroQueue's `playback/tv/TvSync.kt` + `TvDataSync.kt`. This is the receiver side,
//! so local copies win ties.
//!
//! Plays live in `listens` (the stats page and On Repeat read them too). Transitions, vibes, the
//! settings' change times and the tombstones are JSON rows in `settings`, kept for the Mix and
//! Vibe phases: [`transitions`] and [`mq_vibes`] read them.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use innertube::models::metadata::ArtistRun;
use innertube::SongItem;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;
use crate::state::AppState;

const MAX_HISTORY: usize = 5000;
const MAX_TOMBSTONES: usize = 500;
const MAX_TRANSITIONS: usize = 2000;

const TRANSITIONS_KEY: &str = "mq_transitions";
const VIBES_KEY: &str = "mq_vibes";
const SETTINGS_KEY: &str = "mq_settings";
const DELETED_KEY: &str = "mq_deleted";

/// Imports and setting stamps read, merge and write several rows; one at a time.
static LOCK: Mutex<()> = Mutex::new(());

type Map<T> = BTreeMap<String, T>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Play {
    song_id: String,
    timestamp: i64,
    #[serde(default)]
    play_time: i64,
}

/// `RemoteTrack` from PROTOCOL.md, as much of it as a `SongItem` holds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Track {
    id: String,
    title: String,
    artist: String,
    art: Option<String>,
    duration: i64,
    pick: bool,
    thumb: Option<String>,
    artists: Vec<Artist>,
    album_id: Option<String>,
    album_title: Option<String>,
    explicit: bool,
    video_type: Option<String>,
    set_video_id: Option<String>,
    episode: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Artist {
    id: Option<String>,
    name: String,
}

/// A custom transition, as in MetroQueue's `mix_transitions.json`. Key: [`Transition::key`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub from_id: String,
    pub to_id: String,
    #[serde(default = "fade")]
    pub style: String,
    #[serde(default = "default_duration")]
    pub duration_ms: i64,
    #[serde(default)]
    pub outgoing_end_ms: Option<i64>,
    #[serde(default)]
    pub incoming_start_ms: Option<i64>,
    #[serde(default = "yes")]
    pub tempo_match: bool,
    #[serde(default)]
    pub updated_at: i64,
}

impl Transition {
    pub fn key(&self) -> String {
        format!("{}\n{}", self.from_id, self.to_id)
    }
}

fn fade() -> String {
    "FADE".into()
}
fn default_duration() -> i64 {
    5000
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Setting {
    value: Value,
    updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Tombstones {
    transitions: Map<i64>,
    vibes: Map<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SyncState {
    version: i64,
    now: i64,
    history: Vec<Play>,
    songs: Vec<Track>,
    transitions: Vec<Transition>,
    /// Kept whole: a vibe's `scene` and `music` go back exactly as they came.
    vibes: Vec<Value>,
    settings: Map<Setting>,
    deleted: Tombstones,
}

/// This machine's state, history from `since_ms` on (`GET /api/sync`).
pub fn export(app: &AppHandle, since_ms: i64) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(export_db(&db(app), since_ms, now_ms()))?)
}

/// Merges a phone's state into this machine's (`POST /api/sync`) and returns the result, history
/// left out (the phone already has it).
pub fn import(app: &AppHandle, body: Value) -> anyhow::Result<Value> {
    let state: SyncState = serde_json::from_value(body)?;
    let db = db(app);
    let now = now_ms();
    if import_db(&db, &state, now)? {
        let app_state = app.state::<Arc<AppState>>().inner().clone();
        tauri::async_runtime::spawn(async move { app_state.apply_crossfade().await });
        // The UI re-reads the ones it shows (the theme colours).
        let _ = app.emit("shared-settings-changed", ());
    }
    Ok(serde_json::to_value(export_db(&db, i64::MAX, now))?)
}

/// Stamps a whitelisted setting's change time when the user changes it here, so it beats an older
/// change on the phone. Called after every `set_setting`; ignores keys that aren't synced.
pub fn shared_setting_changed(app: &AppHandle, key: &str) {
    if MAPPED.iter().any(|(_, k)| *k == key) {
        let _guard = LOCK.lock().unwrap();
        stamp_settings(&db(app), now_ms());
    }
}

/// The custom transitions (all of them, any order).
pub fn transitions(app: &AppHandle) -> Vec<Transition> {
    load(&db(app), TRANSITIONS_KEY)
}

/// The custom vibes, as MetroQueue stores them (`Vibe` JSON).
#[tauri::command]
pub fn mq_vibes(app: AppHandle) -> Vec<Value> {
    load(&db(&app), VIBES_KEY)
}

/// Saves a custom vibe made or edited here, stamped now so the phone takes it on the next sync.
#[tauri::command]
pub fn mq_save_vibe(app: AppHandle, vibe: Value) -> Result<(), String> {
    save_vibe(&db(&app), vibe, now_ms()).map_err(|e| e.to_string())
}

/// Deletes a custom vibe, leaving a tombstone so the phone deletes its copy too.
#[tauri::command]
pub fn mq_delete_vibe(app: AppHandle, id: String) {
    delete_vibe(&db(&app), &id, now_ms());
}

fn save_vibe(db: &Db, mut vibe: Value, now: i64) -> anyhow::Result<()> {
    let id = match vibe.get("id").and_then(Value::as_str) {
        Some(id) if id.starts_with("custom_") => id.to_owned(),
        _ => anyhow::bail!("not a custom vibe"),
    };
    vibe["updatedAt"] = json!(now);
    let _guard = LOCK.lock().unwrap();
    let mut vibes: Vec<Value> = load(db, VIBES_KEY);
    match vibes.iter_mut().find(|v| v.get("id").and_then(Value::as_str) == Some(id.as_str())) {
        Some(v) => *v = vibe,
        None => vibes.push(vibe),
    }
    save(db, VIBES_KEY, &vibes);
    Ok(())
}

fn delete_vibe(db: &Db, id: &str, now: i64) {
    let _guard = LOCK.lock().unwrap();
    let mut vibes: Vec<Value> = load(db, VIBES_KEY);
    vibes.retain(|v| v.get("id").and_then(Value::as_str) != Some(id));
    save(db, VIBES_KEY, &vibes);
    let mut tombstones: Tombstones = load(db, DELETED_KEY);
    tombstones.vibes.insert(id.to_owned(), now);
    tombstones.vibes = cap(tombstones.vibes);
    save(db, DELETED_KEY, &tombstones);
}

fn db(app: &AppHandle) -> Arc<Db> {
    app.state::<Arc<AppState>>().db.clone()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn load<T: serde::de::DeserializeOwned + Default>(db: &Db, key: &str) -> T {
    db.get_setting(key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save<T: Serialize>(db: &Db, key: &str, value: &T) {
    if let Ok(s) = serde_json::to_string(value) {
        db.set_setting(key, &s);
    }
}

fn export_db(db: &Db, since_ms: i64, now: i64) -> SyncState {
    let settings = {
        let _guard = LOCK.lock().unwrap();
        stamp_settings(db, now)
    };
    let mut history = Vec::new();
    let mut songs = Vec::new();
    let mut seen = HashSet::new();
    for (video_id, ms, secs, json) in db.synced_plays(since_ms.max(0), MAX_HISTORY) {
        let Ok(song) = serde_json::from_str::<SongItem>(&json) else { continue };
        if seen.insert(video_id.clone()) {
            songs.push(song_to_track(&song));
        }
        history.push(Play { song_id: video_id, timestamp: ms, play_time: secs * 1000 });
    }
    SyncState {
        version: 1,
        now,
        history,
        songs,
        transitions: load(db, TRANSITIONS_KEY),
        vibes: load(db, VIBES_KEY),
        settings,
        deleted: load(db, DELETED_KEY),
    }
}

/// True when a setting the player reads (crossfade) changed. Every mapped setting counts; the Mix
/// ones other than these two are read per transition, so reapplying is harmless.
fn import_db(db: &Db, state: &SyncState, now: i64) -> anyhow::Result<bool> {
    let changed = {
        let _guard = LOCK.lock().unwrap();
        let tombstones: Tombstones = load(db, DELETED_KEY);

        let local: Map<Transition> = load::<Vec<Transition>>(db, TRANSITIONS_KEY)
            .into_iter()
            .map(|t| (t.key(), t))
            .collect();
        let remote: Map<Transition> = state
            .transitions
            .iter()
            .filter(|t| !t.from_id.trim().is_empty() && !t.to_id.trim().is_empty())
            .map(|t| {
                let mut t = t.clone();
                t.duration_ms = t.duration_ms.clamp(1000, 16000);
                if !["FADE", "FILTER", "ECHO", "RISE", "CUT"].contains(&t.style.as_str()) {
                    t.style = fade();
                }
                (t.key(), t)
            })
            .collect();
        let (transitions, deleted_transitions) =
            merge(local, &tombstones.transitions, remote, &state.deleted.transitions, false, |t| {
                t.updated_at
            });
        let mut transitions: Vec<Transition> = transitions.into_values().collect();
        // Past the cap, the least recently changed are dropped.
        transitions.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        transitions.truncate(MAX_TRANSITIONS);

        let custom = |v: &Value| -> Option<(String, Value)> {
            let id = v.get("id")?.as_str()?;
            id.starts_with("custom_").then(|| (id.to_owned(), v.clone()))
        };
        let local: Map<Value> =
            load::<Vec<Value>>(db, VIBES_KEY).iter().filter_map(custom).collect();
        let remote: Map<Value> = state.vibes.iter().filter_map(custom).collect();
        let (vibes, deleted_vibes) =
            merge(local, &tombstones.vibes, remote, &state.deleted.vibes, false, vibe_updated_at);

        let changed = merge_settings(db, &state.settings, now);
        save(db, TRANSITIONS_KEY, &transitions);
        save(db, VIBES_KEY, &vibes.into_values().collect::<Vec<_>>());
        save(
            db,
            DELETED_KEY,
            &Tombstones { transitions: deleted_transitions, vibes: deleted_vibes },
        );
        insert_history(db, state)?;
        changed
    };
    Ok(changed)
}

fn vibe_updated_at(v: &Value) -> i64 {
    v.get("updatedAt").and_then(Value::as_i64).unwrap_or(0)
}

fn insert_history(db: &Db, state: &SyncState) -> anyhow::Result<()> {
    let remote = &state.history[..state.history.len().min(MAX_HISTORY)];
    let Some(oldest) = remote.iter().map(|p| p.timestamp).min() else { return Ok(()) };
    let songs: BTreeMap<&str, &Track> = state.songs.iter().map(|t| (t.id.as_str(), t)).collect();
    let rows: Vec<_> = new_history(&db.play_keys(oldest), remote)
        .into_iter()
        .filter_map(|p| {
            let json =
                serde_json::to_string(&track_to_song(songs.get(p.song_id.as_str())?)).ok()?;
            Some((p.song_id, p.timestamp, p.play_time.max(0) / 1000, json))
        })
        .collect();
    if !rows.is_empty() {
        db.insert_synced_plays(&rows)?;
    }
    Ok(())
}

// --- merge rules (TvSync.kt `SyncMerge`) ----------------------------------------------------

/// Keeps the newer copy of each item and drops items deleted at or after their last change. On
/// equal times `remote_wins_ties` decides (the receiver passes false). Tombstones of surviving
/// items are dropped and the rest capped to the newest [`MAX_TOMBSTONES`].
fn merge<T>(
    local: Map<T>,
    local_deleted: &Map<i64>,
    mut remote: Map<T>,
    remote_deleted: &Map<i64>,
    remote_wins_ties: bool,
    updated_at: impl Fn(&T) -> i64,
) -> (Map<T>, Map<i64>) {
    let mut deleted = local_deleted.clone();
    for (key, &time) in remote_deleted {
        let at = deleted.entry(key.clone()).or_insert(time);
        *at = (*at).max(time);
    }
    let mut items = Map::new();
    for (key, mine) in local {
        let winner = match remote.remove(&key) {
            Some(theirs)
                if updated_at(&theirs) > updated_at(&mine)
                    || (updated_at(&theirs) == updated_at(&mine) && remote_wins_ties) =>
            {
                theirs
            }
            _ => mine,
        };
        items.insert(key, winner);
    }
    items.extend(remote);
    items.retain(|key, item| !matches!(deleted.get(key), Some(&at) if updated_at(item) <= at));
    deleted.retain(|key, _| !items.contains_key(key));
    (items, cap(deleted))
}

/// The newest [`MAX_TOMBSTONES`] tombstones.
fn cap(tombstones: Map<i64>) -> Map<i64> {
    if tombstones.len() <= MAX_TOMBSTONES {
        return tombstones;
    }
    let mut entries: Vec<_> = tombstones.into_iter().collect();
    entries.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    entries.truncate(MAX_TOMBSTONES);
    entries.into_iter().collect()
}

/// Plays in `remote` not yet in `local`, one per (songId, timestamp).
fn new_history(local: &HashSet<(String, i64)>, remote: &[Play]) -> Vec<Play> {
    let mut seen = HashSet::new();
    remote
        .iter()
        .filter(|p| !p.song_id.trim().is_empty() && p.timestamp > 0)
        .filter(|p| {
            let key = (p.song_id.clone(), p.timestamp);
            !local.contains(&key) && seen.insert(key)
        })
        .cloned()
        .collect()
}

// --- shared settings (TvDataSync.kt `SYNCED_SETTINGS`) --------------------------------------

/// The whitelist, with whether a value from the phone is valid for it. `null` (unset) always is.
type Valid = fn(&Value) -> bool;

const WHITELIST: [(&str, Valid); 11] = [
    ("crossfadeEnabled", Value::is_boolean),
    ("mixMode", |v| one_of(v, &["OFF", "CROSSFADE", "SMART"])),
    ("mixTransitionStyle", |v| one_of(v, &["AUTO", "FADE", "FILTER", "ECHO", "RISE", "CUT"])),
    ("crossfadeDurationFloat", |v| v.as_f64().is_some_and(|d| (1.0..=15.0).contains(&d))),
    ("crossfadeOnSkip", Value::is_boolean),
    ("crossfadeGapless", Value::is_boolean),
    ("lyricsEffects", |v| one_of(v, &["FULL", "SUBTLE", "OFF"])),
    ("experimentalLyrics", Value::is_boolean),
    ("pureBlack", Value::is_boolean),
    ("dynamicTheme", Value::is_boolean),
    ("selectedThemeColor", |v| v.as_i64().is_some_and(|c| i32::try_from(c).is_ok())),
];

/// Whitelisted names this app has a setting for, and that setting's key. The rest are only kept
/// and sent back. The theme ones are the UI's M3 colour settings (theme.svelte.ts); the seed is
/// stored as the phone stores it, a signed ARGB int.
const MAPPED: [(&str, &str); 9] = [
    ("crossfadeEnabled", "crossfade"),
    ("mixMode", "mix_mode"),
    ("mixTransitionStyle", "mix_style"),
    ("crossfadeDurationFloat", "crossfade_secs"),
    ("crossfadeOnSkip", "crossfade_on_skip"),
    ("crossfadeGapless", "crossfade_gapless"),
    ("pureBlack", "theme_pure_black"),
    ("dynamicTheme", "theme_dynamic"),
    ("selectedThemeColor", "theme_seed"),
];

fn one_of(v: &Value, names: &[&str]) -> bool {
    v.as_str().is_some_and(|s| names.contains(&s))
}

/// A mapped setting's value here, as sync JSON. `None` for a name that isn't mapped.
fn read(db: &Db, name: &str) -> Option<Value> {
    let (_, key) = MAPPED.iter().find(|(n, _)| *n == name)?;
    let raw = db.get_setting(key);
    Some(match name {
        "crossfadeEnabled" | "crossfadeOnSkip" | "crossfadeGapless" | "pureBlack"
        | "dynamicTheme" => raw.map_or(Value::Null, |v| Value::Bool(v == "true")),
        "mixMode" | "mixTransitionStyle" => raw.map_or(Value::Null, Value::String),
        "selectedThemeColor" => {
            raw.and_then(|v| v.parse::<i32>().ok()).map_or(Value::Null, |v| json!(v))
        }
        _ => raw.and_then(|v| v.parse::<f64>().ok()).map_or(Value::Null, |v| json!(v)),
    })
}

fn write(db: &Db, name: &str, value: &Value) {
    let Some((_, key)) = MAPPED.iter().find(|(n, _)| *n == name) else { return };
    match value {
        Value::Null => db.delete_setting(key),
        Value::Bool(b) => db.set_setting(key, &b.to_string()),
        Value::String(s) => db.set_setting(key, s),
        v => db.set_setting(key, &v.to_string()),
    }
}

/// The whitelist with this machine's values and when each changed: a value that differs from the
/// one last seen changed `now`; one seen for the first time is older than any real change (`1`,
/// or `0` when unset). Unmapped names keep what was stored.
fn current_settings(db: &Db, stored: &Map<Setting>, now: i64) -> Map<Setting> {
    WHITELIST
        .iter()
        .map(|(name, _)| {
            let known = stored.get(*name);
            let value =
                read(db, name).unwrap_or_else(|| known.map_or(Value::Null, |s| s.value.clone()));
            let setting = match known {
                None => Setting { updated_at: if value.is_null() { 0 } else { 1 }, value },
                Some(known) if known.value != value => Setting { value, updated_at: now },
                Some(known) => known.clone(),
            };
            (name.to_string(), setting)
        })
        .collect()
}

/// [`current_settings`], saved when anything changed. Call with [`LOCK`] held.
fn stamp_settings(db: &Db, now: i64) -> Map<Setting> {
    let stored: Map<Setting> = load(db, SETTINGS_KEY);
    let current = current_settings(db, &stored, now);
    if current != stored {
        save(db, SETTINGS_KEY, &current);
    }
    current
}

/// Merges the phone's settings in, local winning ties. True when a mapped one changed here.
fn merge_settings(db: &Db, remote: &Map<Setting>, now: i64) -> bool {
    let local = current_settings(db, &load(db, SETTINGS_KEY), now);
    // A value never set over there (time 0) is no reason to reset this machine's.
    let remote: Map<Setting> = remote
        .iter()
        .filter(|(name, s)| {
            s.updated_at > 0
                && WHITELIST
                    .iter()
                    .any(|(n, valid)| *n == name.as_str() && (s.value.is_null() || valid(&s.value)))
        })
        .map(|(name, s)| (name.clone(), s.clone()))
        .collect();
    let (merged, _) =
        merge(local.clone(), &Map::new(), remote, &Map::new(), false, |s| s.updated_at);
    let mut changed = false;
    for (name, setting) in &merged {
        if local.get(name) != Some(setting) && MAPPED.iter().any(|(n, _)| *n == name.as_str()) {
            write(db, name, &setting.value);
            changed = true;
        }
    }
    // Stored as read back, so the next stamp sees no change and keeps these times.
    let stored: Map<Setting> = merged
        .into_iter()
        .map(|(name, s)| {
            let value = read(db, &name).unwrap_or(s.value);
            (name, Setting { value, updated_at: s.updated_at })
        })
        .collect();
    save(db, SETTINGS_KEY, &stored);
    changed
}

// --- songs ------------------------------------------------------------------------------------

const ATV: &str = "MUSIC_VIDEO_TYPE_ATV";
const OMV: &str = "MUSIC_VIDEO_TYPE_OMV";
const UPLOAD: &str = "MUSIC_VIDEO_TYPE_PRIVATELY_OWNED_TRACK";
const EPISODE: &str = "MUSIC_VIDEO_TYPE_PODCAST_EPISODE";

fn song_to_track(song: &SongItem) -> Track {
    let video_type = if song.is_episode {
        EPISODE
    } else if song.is_upload {
        UPLOAD
    } else if song.is_video {
        OMV
    } else {
        ATV
    };
    Track {
        id: song.video_id.clone(),
        title: song.title.clone(),
        artist: song.artists.clone(),
        duration: crate::state::parse_duration_ms(song.duration.as_deref()),
        thumb: song.thumbnail.clone(),
        artists: song
            .artist_runs
            .iter()
            .map(|r| Artist { id: r.id.clone(), name: r.text.clone() })
            .collect(),
        album_id: song.album_id.clone(),
        album_title: song.album.clone(),
        explicit: song.explicit,
        video_type: Some(video_type.into()),
        episode: song.is_episode,
        ..Track::default()
    }
}

fn track_to_song(track: &Track) -> SongItem {
    let video_type = track.video_type.as_deref().unwrap_or(ATV);
    let artist =
        if track.artist.is_empty() { track.artists.first().map(|a| a.name.clone()) } else { None };
    SongItem {
        video_id: track.id.clone(),
        title: track.title.clone(),
        artists: artist.unwrap_or_else(|| track.artist.clone()),
        artist_id: track.artists.first().and_then(|a| a.id.clone()),
        artist_runs: track
            .artists
            .iter()
            .map(|a| ArtistRun { text: a.name.clone(), id: a.id.clone() })
            .collect(),
        album: track.album_title.clone(),
        album_id: track.album_id.clone(),
        duration: (track.duration > 0).then(|| crate::state::format_duration(track.duration)),
        thumbnail: track.thumb.clone().or_else(|| track.art.clone()),
        is_video: ![ATV, UPLOAD, EPISODE].contains(&video_type),
        is_episode: track.episode || video_type == EPISODE,
        is_upload: video_type == UPLOAD,
        explicit: track.explicit,
        ..SongItem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ported from MetroQueue's SyncMergeTest.kt ---

    type Item = (&'static str, i64);

    fn items(list: &[(&str, Item)]) -> Map<Item> {
        list.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn times(list: &[(&str, i64)]) -> Map<i64> {
        list.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn m(
        local: Map<Item>,
        remote: Map<Item>,
        local_deleted: Map<i64>,
        remote_deleted: Map<i64>,
        remote_wins_ties: bool,
    ) -> (Map<Item>, Map<i64>) {
        merge(local, &local_deleted, remote, &remote_deleted, remote_wins_ties, |i| i.1)
    }

    #[test]
    fn newer_copy_wins_either_way() {
        let (merged, _) = m(
            items(&[("a", ("mine", 20)), ("b", ("mine", 5))]),
            items(&[("a", ("theirs", 10)), ("b", ("theirs", 6))]),
            Map::new(),
            Map::new(),
            true,
        );
        assert_eq!(merged["a"].0, "mine");
        assert_eq!(merged["b"].0, "theirs");
    }

    #[test]
    fn items_on_one_side_are_kept() {
        let (merged, _) = m(
            items(&[("a", ("mine", 0))]),
            items(&[("b", ("theirs", 0))]),
            Map::new(),
            Map::new(),
            true,
        );
        assert_eq!(merged.keys().collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn ties_converge_on_the_receivers_copy() {
        let (phone, _) = m(
            items(&[("a", ("phone", 7))]),
            items(&[("a", ("receiver", 7))]),
            Map::new(),
            Map::new(),
            true,
        );
        assert_eq!(phone["a"].0, "receiver");
        let (receiver, _) =
            m(items(&[("a", ("receiver", 7))]), phone.clone(), Map::new(), Map::new(), false);
        assert_eq!(phone, receiver);
    }

    #[test]
    fn deletion_beats_older_or_equal_changes() {
        let (merged, deleted) = m(
            items(&[("a", ("mine", 10)), ("b", ("mine", 10))]),
            Map::new(),
            Map::new(),
            times(&[("a", 10), ("b", 9)]),
            true,
        );
        assert_eq!(merged.keys().collect::<Vec<_>>(), ["b"]);
        assert_eq!(deleted, times(&[("a", 10)]));
    }

    #[test]
    fn remote_item_deleted_here_stays_deleted() {
        let (merged, deleted) =
            m(Map::new(), items(&[("a", ("theirs", 3))]), times(&[("a", 4)]), Map::new(), true);
        assert!(merged.is_empty());
        assert_eq!(deleted, times(&[("a", 4)]));
    }

    #[test]
    fn tombstones_keep_the_newest_deletion_time() {
        let (_, deleted) = m(Map::new(), Map::new(), times(&[("a", 4)]), times(&[("a", 9)]), true);
        assert_eq!(deleted, times(&[("a", 9)]));
    }

    #[test]
    fn tombstones_are_capped() {
        let many = (1..=MAX_TOMBSTONES as i64 + 50).map(|i| (format!("k{i}"), i)).collect();
        let (_, deleted) = m(Map::new(), Map::new(), Map::new(), many, true);
        assert_eq!(deleted.len(), MAX_TOMBSTONES);
        assert_eq!(deleted.values().min(), Some(&51));
    }

    #[test]
    fn merging_is_idempotent() {
        let local = items(&[("a", ("mine", 2)), ("c", ("mine", 1))]);
        let remote = items(&[("a", ("theirs", 3)), ("b", ("theirs", 1))]);
        let (once, once_deleted) = m(local, remote.clone(), Map::new(), times(&[("c", 5)]), true);
        let (twice, twice_deleted) =
            m(once.clone(), remote, once_deleted.clone(), times(&[("c", 5)]), true);
        assert_eq!(once, twice);
        assert_eq!(once_deleted, twice_deleted);
    }

    #[test]
    fn history_is_a_union_by_play() {
        let play = |id: &str, ts, pt| Play { song_id: id.into(), timestamp: ts, play_time: pt };
        let local = HashSet::from([("song1".to_string(), 100)]);
        let remote = [
            play("song1", 100, 30_000),
            play("song1", 200, 30_000),
            play("song1", 200, 31_000),
            play("song2", 100, 0),
            play("", 300, 0),
            play("song3", 0, 0),
        ];
        assert_eq!(
            new_history(&local, &remote),
            [play("song1", 200, 30_000), play("song2", 100, 0)]
        );
    }

    // --- the receiver end to end, against a real database ---

    fn db() -> Db {
        Db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn state(body: Value) -> SyncState {
        serde_json::from_value(body).unwrap()
    }

    #[test]
    fn plays_round_trip_to_the_millisecond_without_duplicates() {
        let d = db();
        let song = SongItem {
            video_id: "here".into(),
            title: "Mine".into(),
            artists: "Me".into(),
            duration: Some("3:00".into()),
            ..SongItem::default()
        };
        d.record_listen("here", &serde_json::to_string(&song).unwrap(), 1_000, 60);
        d.record_listen("LOCAL:/x.mp3", "{}", 1_001, 60);
        let phone = state(json!({
            "history": [
                {"songId": "phone", "timestamp": 2_000_123, "playTime": 90_000},
                {"songId": "here", "timestamp": 1_000_000},
                {"songId": "nosong", "timestamp": 3_000_000}
            ],
            "songs": [{"id": "phone", "title": "Theirs", "artist": "", "duration": 213_000,
                       "artists": [{"id": "UC1", "name": "Them"}], "videoType": "MUSIC_VIDEO_TYPE_ATV"}]
        }));
        import_db(&d, &phone, 5_000_000).unwrap();
        import_db(&d, &phone, 5_000_001).unwrap();

        let out = export_db(&d, 0, 6_000_000);
        let plays: Vec<_> = out.history.iter().map(|p| (p.song_id.as_str(), p.timestamp)).collect();
        assert_eq!(plays, [("phone", 2_000_123), ("here", 1_000_000)]);
        assert_eq!(out.history[0].play_time, 90_000);
        let theirs = &out.songs[0];
        assert_eq!(
            (theirs.title.as_str(), theirs.artist.as_str(), theirs.duration),
            ("Theirs", "Them", 213_000)
        );
        // And the phone's play counts on the stats page like one made here.
        assert!(d.play_counts(0).contains(&("phone".to_string(), 1)));
        assert_eq!(export_db(&d, 1_500_000, 6_000_000).history.len(), 1);
    }

    #[test]
    fn transitions_and_vibes_merge_with_tombstones() {
        let d = db();
        save(
            &d,
            TRANSITIONS_KEY,
            &vec![Transition {
                from_id: "a".into(),
                to_id: "b".into(),
                style: "ECHO".into(),
                duration_ms: 6000,
                outgoing_end_ms: None,
                incoming_start_ms: None,
                tempo_match: true,
                updated_at: 10,
            }],
        );
        save(&d, VIBES_KEY, &vec![json!({"id": "custom_1", "name": "Mine", "updatedAt": 5})]);
        let phone = state(json!({
            "transitions": [
                {"fromId": "a", "toId": "b", "style": "CUT", "updatedAt": 10},
                {"fromId": "c", "toId": "d", "style": "WOBBLE", "durationMs": 99_000, "updatedAt": 3}
            ],
            "vibes": [
                {"id": "custom_2", "name": "New", "scene": {"future": [1, 2]}, "music": {}, "updatedAt": 8},
                {"id": "preset_x", "name": "Preset", "updatedAt": 9}
            ],
            "deleted": {"vibes": {"custom_1": 5, "custom_gone": 4}}
        }));
        import_db(&d, &phone, 100).unwrap();

        let out = export_db(&d, 0, 100);
        let mut ts = out.transitions.clone();
        ts.sort_by_key(Transition::key);
        // A tie keeps this machine's copy; an unknown style and an out-of-range length are fixed.
        assert_eq!(ts[0].style, "ECHO");
        assert_eq!(
            (ts[1].style.as_str(), ts[1].duration_ms, ts[1].tempo_match),
            ("FADE", 16000, true)
        );
        assert_eq!(
            out.vibes,
            [
                json!({"id": "custom_2", "name": "New", "scene": {"future": [1, 2]}, "music": {}, "updatedAt": 8})
            ]
        );
        assert_eq!(out.deleted.vibes, times(&[("custom_1", 5), ("custom_gone", 4)]));
    }

    #[test]
    fn vibes_saved_and_deleted_here_sync_back() {
        let d = db();
        let vibe =
            json!({"id": "custom_1", "name": "Mine", "scene": {"future": 1}, "updatedAt": 2});
        save_vibe(&d, vibe, 10).unwrap();
        save_vibe(&d, json!({"id": "custom_1", "name": "Renamed"}), 20).unwrap();
        assert!(save_vibe(&d, json!({"id": "winter_dusk"}), 30).is_err());
        assert_eq!(
            export_db(&d, 0, 40).vibes,
            [json!({"id": "custom_1", "name": "Renamed", "updatedAt": 20})]
        );
        delete_vibe(&d, "custom_1", 50);
        let out = export_db(&d, 0, 60);
        assert!(out.vibes.is_empty());
        assert_eq!(out.deleted.vibes, times(&[("custom_1", 50)]));
    }

    #[test]
    fn settings_map_onto_crossfade_and_keep_the_rest() {
        let d = db();
        d.set_setting("crossfade", "false");
        // Seen once: older than any change.
        assert_eq!(
            export_db(&d, 0, 50).settings["crossfadeEnabled"],
            Setting { value: json!(false), updated_at: 1 }
        );
        let phone = state(json!({"settings": {
            "crossfadeEnabled": {"value": true, "updatedAt": 40},
            "crossfadeDurationFloat": {"value": 8, "updatedAt": 40},
            "selectedThemeColor": {"value": -16776961, "updatedAt": 40},
            "mixMode": {"value": "WILD", "updatedAt": 40},
            "pureBlack": {"value": true, "updatedAt": 0},
            "notSynced": {"value": 1, "updatedAt": 40}
        }}));
        assert!(import_db(&d, &phone, 60).unwrap());
        assert_eq!(d.get_setting("crossfade").as_deref(), Some("true"));
        assert_eq!(d.get_setting("crossfade_secs").as_deref(), Some("8"));
        assert_eq!(d.get_setting("theme_seed").as_deref(), Some("-16776961"));
        assert_eq!(d.get_setting("theme_pure_black"), None);

        let out = export_db(&d, 0, 70);
        assert_eq!(
            out.settings["crossfadeEnabled"],
            Setting { value: json!(true), updated_at: 40 }
        );
        assert_eq!(
            out.settings["crossfadeDurationFloat"],
            Setting { value: json!(8.0), updated_at: 40 }
        );
        assert_eq!(
            out.settings["selectedThemeColor"],
            Setting { value: json!(-16776961), updated_at: 40 }
        );
        assert_eq!(out.settings["mixMode"].updated_at, 0);
        assert_eq!(out.settings["pureBlack"].updated_at, 0);
        assert!(!out.settings.contains_key("notSynced"));
        // Nothing new: a second import changes nothing.
        assert!(!import_db(&d, &phone, 80).unwrap());

        // Changed here later: stamped, and an older phone change no longer wins.
        d.set_setting("crossfade", "false");
        assert_eq!(export_db(&d, 0, 90).settings["crossfadeEnabled"].updated_at, 90);
        import_db(&d, &phone, 95).unwrap();
        assert_eq!(d.get_setting("crossfade").as_deref(), Some("false"));

        // The UI's theme switches go out as the phone's booleans.
        d.set_setting("theme_dynamic", "true");
        assert_eq!(
            export_db(&d, 0, 100).settings["dynamicTheme"],
            Setting { value: json!(true), updated_at: 100 }
        );
    }

    #[test]
    fn mix_settings_map_onto_their_own_keys() {
        let d = db();
        let phone = state(json!({"settings": {
            "mixMode": {"value": "SMART", "updatedAt": 40},
            "mixTransitionStyle": {"value": "ECHO", "updatedAt": 40},
            "crossfadeOnSkip": {"value": false, "updatedAt": 40},
            "crossfadeGapless": {"value": true, "updatedAt": 40}
        }}));
        assert!(import_db(&d, &phone, 50).unwrap());
        assert_eq!(d.get_setting("mix_mode").as_deref(), Some("SMART"));
        assert_eq!(d.get_setting("mix_style").as_deref(), Some("ECHO"));
        assert_eq!(d.get_setting("crossfade_on_skip").as_deref(), Some("false"));
        assert_eq!(d.get_setting("crossfade_gapless").as_deref(), Some("true"));
        let out = export_db(&d, 0, 60);
        assert_eq!(out.settings["mixMode"], Setting { value: json!("SMART"), updated_at: 40 });
        assert_eq!(out.settings["crossfadeOnSkip"].value, json!(false));
        // Changed here: stamped with this machine's time.
        d.set_setting("mix_style", "RISE");
        assert_eq!(
            export_db(&d, 0, 70).settings["mixTransitionStyle"],
            Setting { value: json!("RISE"), updated_at: 70 }
        );
    }

    #[test]
    fn songs_convert_both_ways() {
        let track = Track {
            id: "v".into(),
            title: "T".into(),
            artist: "A & B".into(),
            duration: 3_723_000,
            thumb: Some("https://x".into()),
            artists: vec![
                Artist { id: Some("UC1".into()), name: "A".into() },
                Artist { id: None, name: "B".into() },
            ],
            album_id: Some("MPREb".into()),
            album_title: Some("Al".into()),
            explicit: true,
            video_type: Some(OMV.into()),
            ..Track::default()
        };
        let song = track_to_song(&track);
        assert_eq!(song.duration.as_deref(), Some("1:02:03"));
        assert!(song.is_video && !song.is_episode);
        assert_eq!(song_to_track(&song), track);
    }
}
