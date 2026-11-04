//! Mix: how one track moves into the next, as MetroQueue does it (`playback/mix/` and the
//! transition logic in its `MusicService`). The user's custom transition for the pair (synced from
//! the phone, `mqsync::transitions`) wins, then a gapless album plays gaplessly, then Smart mix
//! plans from an analysis of both songs, else the configured crossfade. The player's two decks do
//! the blending (`player::Transition`); this decides what it does.
//!
//! Settings (the keys the phone syncs, `mqsync::MAPPED`): `crossfade` (Mix on/off), `mix_mode`
//! (`CROSSFADE`/`SMART`), `mix_style` (`AUTO`/`FADE`/…), `crossfade_secs`, `crossfade_on_skip`,
//! `crossfade_gapless`.

mod analyzer;
mod planner;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use innertube::SongItem;
use player::{Style, Transition};

use crate::db::Db;
use crate::state::{parse_duration_ms, AppState};
use planner::{manual_skip, MixAnalysis, TransitionPlan};

/// Analysed at both ends of a song, as MetroQueue does.
const REGION_SECS: f64 = 30.0;
/// MetroQueue's analysis rate: enough for the key's lowest notes, cheap for the rest.
const ANALYSIS_RATE: u32 = 11_025;
const HEAD: i64 = 0;
const TAIL: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Off,
    Crossfade,
    Smart,
}

struct Settings {
    mode: Mode,
    /// `None` is Auto: Smart mix picks per transition, Crossfade fades.
    style: Option<Style>,
    duration_ms: i64,
    on_skip: bool,
    gapless: bool,
}

fn settings(db: &Db) -> Settings {
    let on = |key: &str| db.get_setting(key).as_deref() != Some("false");
    let mode = match (crate::state::saved_crossfade(db), db.get_setting("mix_mode").as_deref()) {
        (None, _) => Mode::Off,
        (Some(_), Some("SMART")) => Mode::Smart,
        _ => Mode::Crossfade,
    };
    Settings {
        mode,
        style: db.get_setting("mix_style").and_then(|s| planner::style_named(&s)),
        duration_ms: (crate::state::saved_crossfade_secs(db) * 1000.0) as i64,
        on_skip: on("crossfade_on_skip"),
        gapless: on("crossfade_gapless"),
    }
}

/// Hand the next track to the player with the transition into it: the player's `enqueue`, and
/// the plan for this pair. With Smart mix (or a custom transition) the songs' analyses are made in
/// the background when they aren't cached yet, and the plan is redone once they are.
pub fn enqueue(
    state: &Arc<AppState>,
    from: &SongItem,
    to: &SongItem,
    duration_secs: f64,
    url: &str,
) -> Result<(), player::Error> {
    let player = &state.player;
    // Off, or suspended in a Listen Together room.
    if !player.crossfades() {
        return player.enqueue(url);
    }
    let s = settings(&state.db);
    let custom = custom_transition(state, from, to);
    if custom.is_none() && s.gapless && same_album(from, to) {
        return player.enqueue_gapless(url);
    }
    player.enqueue(url)?;
    let track_ms = track_ms(from, duration_secs);
    let (tail, head) =
        (cached(&state.db, &from.video_id, TAIL), cached(&state.db, &to.video_id, HEAD));
    player.set_transition(plan(state, &s, custom.as_ref(), track_ms, tail.as_ref(), head.as_ref()));
    if (s.mode == Mode::Smart || custom.is_some()) && (tail.is_none() || head.is_none()) {
        // The deck being heard is `from`'s; asked now, before a gapless advance could change it.
        let from_url = player.current_path();
        analyse_and_replan(state.clone(), from.clone(), to.clone(), from_url, url.to_owned());
    }
    Ok(())
}

/// A manual Next into the primed lookahead, blended for at most a second when "Crossfade on
/// manual skip" is on (MetroQueue's `skipWithCrossfade`). True when the player took it: its
/// `TrackEnded` then advances the queue like an automatic transition. A skip right after another,
/// or during a transition, cuts.
pub fn skip(state: &AppState, primed: bool) -> bool {
    static LAST: Mutex<Option<i64>> = Mutex::new(None);
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let now = EPOCH.get_or_init(Instant::now).elapsed().as_millis() as i64;
    let last = LAST.lock().unwrap().replace(now);
    if !manual_skip::should_blend(now, last, state.player.is_fading()) {
        return false;
    }
    let s = settings(&state.db);
    if !primed || !s.on_skip || s.mode == Mode::Off || s.style == Some(Style::Cut) {
        return false;
    }
    let secs = manual_skip::blend_duration_ms(s.duration_ms) as f64 / 1000.0;
    state.player.skip_crossfade(secs)
}

fn custom_transition(
    state: &Arc<AppState>,
    from: &SongItem,
    to: &SongItem,
) -> Option<crate::mqsync::Transition> {
    crate::mqsync::transitions(&state.app)
        .into_iter()
        .find(|t| t.from_id == from.video_id && t.to_id == to.video_id)
}

/// Album songs that follow each other (MetroQueue compares the album title).
fn same_album(from: &SongItem, to: &SongItem) -> bool {
    match (&from.album_id, &to.album_id) {
        (Some(a), Some(b)) => a == b,
        _ => from.album.as_ref().is_some_and(|a| !a.is_empty() && Some(a) == to.album.as_ref()),
    }
}

/// mpv's length when it has reported one, the metadata's before.
fn track_ms(from: &SongItem, duration_secs: f64) -> i64 {
    if duration_secs > 1.0 {
        (duration_secs * 1000.0) as i64
    } else {
        parse_duration_ms(from.duration.as_deref())
    }
}

fn plan(
    state: &AppState,
    s: &Settings,
    custom: Option<&crate::mqsync::Transition>,
    track_ms: i64,
    tail: Option<&MixAnalysis>,
    head: Option<&MixAnalysis>,
) -> Option<Transition> {
    // Never on top of the user's own tempo or pitch.
    let (speed, semitones, _) = state.player.playback_params();
    let nudge = speed == 1.0 && semitones == 0;
    if let Some(p) = custom.and_then(|c| planner::custom(c, track_ms, tail, head, nudge)) {
        return Some(to_player(p));
    }
    if s.mode == Mode::Smart {
        let smart =
            tail.and_then(|t| planner::smart_mix(t, head, track_ms, s.duration_ms, s.style, nudge));
        if let Some((p, reason)) = smart {
            tracing::info!(?reason, start = p.start_position_ms, ms = p.duration_ms, "smart mix");
            return Some(to_player(p));
        }
    }
    // Crossfade, or Smart mix before the analysis: the configured style over the configured
    // length (a cut switches right at the end). A plain fade is the player's own crossfade.
    let style = s.style.unwrap_or_default();
    (style != Style::Fade).then(|| Transition {
        fade: if style == Style::Cut { 0.0 } else { s.duration_ms as f64 / 1000.0 },
        style,
        ..Transition::default()
    })
}

fn to_player(p: TransitionPlan) -> Transition {
    Transition {
        start: Some(p.start_position_ms as f64 / 1000.0),
        fade: p.duration_ms as f64 / 1000.0,
        incoming_start: p.incoming_start_ms.unwrap_or(0) as f64 / 1000.0,
        style: p.style,
        echo_delay: p.echo_delay_ms.map_or(0.375, |ms| ms as f64 / 1000.0),
        incoming_speed: p.incoming_speed,
    }
}

fn cached(db: &Db, video_id: &str, region: i64) -> Option<MixAnalysis> {
    serde_json::from_str(&db.get_mix_analysis(video_id, region)?).ok()
}

/// Regions that couldn't be decoded or analysed this session; not retried.
static FAILED: Mutex<Option<HashSet<(String, i64)>>> = Mutex::new(None);
/// One analysis at a time: each is an mpv decode plus a few thousand FFTs.
static ANALYSING: Mutex<()> = Mutex::new(());

fn analyse_and_replan(
    state: Arc<AppState>,
    from: SongItem,
    to: SongItem,
    from_url: Option<String>,
    to_url: String,
) {
    tauri::async_runtime::spawn(async move {
        let st = state.clone();
        let (from_id, to_id) = (from.video_id.clone(), to.video_id.clone());
        let work = tauri::async_runtime::spawn_blocking(move || {
            let _one = ANALYSING.lock().unwrap();
            if let Some(url) = from_url {
                ensure(&st, &from_id, TAIL, &url);
            }
            ensure(&st, &to_id, HEAD, &to_url);
        });
        if work.await.is_err() {
            return;
        }
        // Only for the pair it was made for: the queue may have moved on meanwhile.
        let Some((current, next, duration)) = state.mix_pair().await else { return };
        if current != from.video_id || next != to.video_id || !state.player.crossfades() {
            return;
        }
        let s = settings(&state.db);
        let custom = custom_transition(&state, &from, &to);
        let tail = cached(&state.db, &from.video_id, TAIL);
        let head = cached(&state.db, &to.video_id, HEAD);
        let track_ms = track_ms(&from, duration);
        state.player.set_transition(plan(
            &state,
            &s,
            custom.as_ref(),
            track_ms,
            tail.as_ref(),
            head.as_ref(),
        ));
    });
}

fn ensure(state: &AppState, video_id: &str, region: i64, url: &str) {
    let key = (video_id.to_owned(), region);
    if state.db.get_mix_analysis(video_id, region).is_some()
        || FAILED.lock().unwrap().as_ref().is_some_and(|f| f.contains(&key))
    {
        return;
    }
    let analysis = match state.player.decode(url, region == TAIL, REGION_SECS, ANALYSIS_RATE) {
        Ok(pcm) => analyzer::analyze(
            &pcm.samples,
            pcm.rate as usize,
            (pcm.start * 1000.0).round() as i64,
            (pcm.duration * 1000.0).round() as i64,
        ),
        Err(e) => {
            tracing::warn!(video_id, region, error = %e, "mix analysis: decode failed");
            None
        }
    };
    match analysis.and_then(|a| serde_json::to_string(&a).ok()) {
        Some(json) => state.db.put_mix_analysis(video_id, region, &json),
        None => {
            FAILED.lock().unwrap().get_or_insert_with(HashSet::new).insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_follow_metroqueues_mode_rules() {
        let db = Db::open(std::path::Path::new(":memory:")).unwrap();
        let s = settings(&db);
        assert_eq!(
            (s.mode, s.style, s.duration_ms, s.on_skip, s.gapless),
            (Mode::Off, None, 8_000, true, true)
        );
        db.set_setting("mix_mode", "SMART");
        assert_eq!(settings(&db).mode, Mode::Off, "Mix stays off until switched on");
        db.set_setting("crossfade", "true");
        assert_eq!(settings(&db).mode, Mode::Smart);
        db.set_setting("mix_mode", "CROSSFADE");
        db.set_setting("mix_style", "AUTO");
        db.set_setting("crossfade_secs", "40");
        let s = settings(&db);
        assert_eq!((s.mode, s.style, s.duration_ms), (Mode::Crossfade, None, 15_000));
        db.set_setting("mix_style", "ECHO");
        assert_eq!(settings(&db).style, Some(Style::Echo));
    }

    #[test]
    fn album_neighbours_are_gapless() {
        let song = |album: Option<&str>, id: Option<&str>| SongItem {
            album: album.map(Into::into),
            album_id: id.map(Into::into),
            ..SongItem::default()
        };
        assert!(same_album(&song(Some("A"), Some("MPREb1")), &song(Some("A"), Some("MPREb1"))));
        assert!(!same_album(&song(Some("A"), Some("MPREb1")), &song(Some("A"), Some("MPREb2"))));
        assert!(same_album(&song(Some("A"), None), &song(Some("A"), None)));
        assert!(!same_album(&song(None, None), &song(None, None)));
        assert!(!same_album(&song(Some(""), None), &song(Some(""), None)));
    }
}
