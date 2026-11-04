//! The phone's commands mapped onto Limusic's player and queue (`AppState`), the same calls the
//! UI's own buttons make, so Listen Together, persistence and the OS media controls stay in step.

use std::sync::Arc;

use innertube::SongItem;
use tauri::AppHandle;

use super::protocol::{
    parse_cmd, queue_hash, repeat_code, song_from_track, track_from_song, Cmd, ItemsRequest,
    QueueSnapshot, RemoteState, RemoteTrack,
};
use crate::state::AppState;

/// Why a control request was refused. `Rejected` is the protocol's `400`, `Busy` its `409`.
#[derive(Debug, PartialEq)]
pub enum Refused {
    Rejected,
    Busy,
}

fn ms(secs: f64) -> i64 {
    (secs.max(0.0) * 1000.0) as i64
}

pub async fn state(st: &AppState) -> RemoteState {
    let snap = st.playback_snapshot().await;
    let paused = snap["paused"].as_bool().unwrap_or(true);
    let (track, index, queue_hash, shuffle, repeat, queue_title) = st
        .read_queue(|items, current, shuffle, repeat, title| {
            let track = items.get(current).map(track_from_song);
            let index = if track.is_some() { current as i32 } else { -1 };
            (track, index, queue_hash(items), shuffle, repeat, title.map(str::to_owned))
        })
        .await;
    // shortcut: no buffering (2) or ended (4); mpv's idle reads as idle. The phone only uses this
    // to decide whether to extrapolate the position.
    let playback_state = if track.is_none() || st.player.is_idle() { 1 } else { 3 };
    RemoteState {
        track,
        playing: !paused,
        position: ms(st.current_position()),
        duration: ms(snap["duration"].as_f64().unwrap_or(0.0)),
        shuffle,
        repeat: repeat_code(repeat),
        volume: st.player.volume().clamp(0, 100) as f32 / 100.0,
        queue_title,
        index,
        queue_hash,
        playback_state,
        speed: snap["speed"].as_f64().unwrap_or(1.0) as f32,
    }
}

pub async fn queue(st: &AppState) -> Vec<RemoteTrack> {
    st.read_queue(|items, _, _, _, _| items.iter().map(track_from_song).collect()).await
}

/// The queue for "Play here" on the phone. No `endpoint`/`continuation`: the phone plays it as a
/// list, and its own autoplay takes over at the end.
pub async fn snapshot(st: &AppState) -> QueueSnapshot {
    let paused = st.playback_snapshot().await["paused"].as_bool().unwrap_or(true);
    let (title, items, index) = st
        .read_queue(|items, current, _, _, title| {
            (title.map(str::to_owned), items.iter().map(track_from_song).collect(), current)
        })
        .await;
    QueueSnapshot {
        title,
        items,
        index: index as i64,
        position: ms(st.current_position()),
        playing: !paused,
        ..Default::default()
    }
}

/// Start the track `video_id` `secs` in, the way every other start offset is applied: mpv drops
/// a seek sent before the file is loaded.
fn start_at(st: &AppState, video_id: &str, secs: f64) {
    if secs > 0.0 {
        *st.pending_seek.lock().unwrap() = Some((video_id.to_owned(), secs));
    }
}

/// `POST /api/load` (`MusicService.playSnapshot`).
// shortcut: `continuation` is not used; Limusic's own autoplay continues the queue from its radio
// seed instead. Use it if a phone-started radio ever drifts from what the phone would play.
pub async fn load(st: &Arc<AppState>, snap: QueueSnapshot) -> Result<(), Refused> {
    if st.lt.is_guest().await {
        return Err(Refused::Busy);
    }
    let secs = snap.position.max(0) as f64 / 1000.0;
    let items: Vec<SongItem> =
        snap.items.into_iter().filter(|t| !t.id.is_empty()).map(song_from_track).collect();
    if !items.is_empty() {
        let index = (snap.index.max(0) as usize).min(items.len() - 1);
        start_at(st, &items[index].video_id, secs);
        st.play_tracks(items, Some(index), None, snap.title, false, None).await;
    } else if let Some(ep) = snap.endpoint {
        match (ep.video_id, ep.playlist_id) {
            (video_id, Some(playlist_id)) => {
                let client = st.clients.get(innertube::METADATA_CLIENT).ok_or(Refused::Rejected)?;
                let next = st
                    .it
                    .next(client, video_id.as_deref(), Some(&playlist_id))
                    .await
                    .map_err(|e| {
                        tracing::warn!(error = %e, "metroqueue: could not resolve the endpoint");
                        Refused::Rejected
                    })?;
                if next.items.is_empty() {
                    return Err(Refused::Rejected);
                }
                let index = video_id
                    .as_ref()
                    .and_then(|v| next.items.iter().position(|i| &i.video_id == v))
                    .or_else(|| ep.index.and_then(|i| usize::try_from(i).ok()))
                    .unwrap_or(0)
                    .min(next.items.len() - 1);
                start_at(st, &next.items[index].video_id, secs);
                st.play_tracks(next.items, Some(index), Some(playlist_id), snap.title, false, None)
                    .await;
            }
            (Some(video_id), None) => {
                let seed = snap
                    .preload
                    .filter(|t| t.id == video_id)
                    .map(song_from_track)
                    .unwrap_or_else(|| SongItem { video_id, ..Default::default() });
                start_at(st, &seed.video_id, secs);
                // A song click: the radio behind it is hydrated like one started here.
                st.play_song(seed).await;
            }
            (None, None) => return Ok(()),
        }
    } else {
        return Ok(());
    }
    if !snap.playing {
        let _ = st.player.pause();
    }
    Ok(())
}

/// `POST /api/items`.
pub async fn items(st: &Arc<AppState>, req: ItemsRequest) -> Result<(), Refused> {
    let items: Vec<SongItem> =
        req.items.into_iter().filter(|t| !t.id.is_empty()).map(song_from_track).collect();
    if items.is_empty() || st.lt.is_guest().await {
        return Err(Refused::Rejected);
    }
    let after_current = st.read_queue(|_, current, _, _, _| current as i64 + 1).await;
    match req.mode.as_str() {
        "next" => st.play_next(items, None).await,
        // shortcut: "at" right behind the current track is Play next, anywhere else Add to queue.
        // Limusic's queue has no insert-at (adds go into its up-next / queued blocks), and the
        // phone refetches the queue and shows where they really went.
        "at" if req.index == after_current => st.play_next(items, None).await,
        "add" | "at" => st.add_to_queue(items, None, None).await,
        _ => return Err(Refused::Rejected),
    }
    Ok(())
}

/// `POST /api/cmd`.
pub async fn command(
    st: &Arc<AppState>,
    app: &AppHandle,
    action: &str,
    a: i64,
    b: i64,
) -> Result<(), Refused> {
    let cmd = parse_cmd(action, a, b).ok_or(Refused::Rejected)?;
    if st.lt.is_guest().await {
        return Err(Refused::Rejected);
    }
    let (len, shuffle) = st.read_queue(|items, _, shuffle, _, _| (items.len(), shuffle)).await;
    let in_range = |i: usize| if i < len { Ok(i) } else { Err(Refused::Rejected) };
    match cmd {
        Cmd::Toggle => st.resume_or_toggle().await,
        Cmd::Resume if st.player.is_idle() => st.resume_or_toggle().await,
        Cmd::Resume => {
            let _ = st.player.play();
        }
        Cmd::Pause => {
            let _ = st.player.pause();
        }
        Cmd::Next => st.next_in_queue().await,
        Cmd::Prev => {
            st.prev_in_queue().await;
            // Media3's seekToPrevious + play: a rewind of a paused track plays it too.
            let _ = st.player.play();
        }
        Cmd::Seek(pos) => st.user_seek(pos as f64 / 1000.0).await.map_err(|_| Refused::Rejected)?,
        Cmd::Jump(i) => st.play_index(in_range(i)?).await,
        Cmd::JumpTo(i, pos) => st.play_index_at(in_range(i)?, Some(pos as f64 / 1000.0)).await,
        Cmd::Volume(v) => crate::hotkeys::change_volume(st, app, |_| v),
        Cmd::Shuffle => st.toggle_shuffle().await,
        Cmd::SetShuffle(on) if on != shuffle => st.toggle_shuffle().await,
        Cmd::SetShuffle(_) => {}
        Cmd::Repeat => st.cycle_repeat().await,
        Cmd::SetRepeat(mode) => st.set_repeat(mode).await,
        Cmd::Remove(i) => st.remove_from_queue(in_range(i)?).await,
        Cmd::Move(from, to) => st.move_in_queue(in_range(from)?, in_range(to)?).await,
        // shortcut: drops everything upcoming but keeps the history above the current track
        // (Limusic's queue keeps what was played). Fine for the phone's "Clear queue".
        Cmd::Clear => st.drop_upcoming(|_| true).await,
        Cmd::Radio => {
            let seed = st
                .read_queue(|items, current, _, _, _| items.get(current).cloned())
                .await
                .ok_or(Refused::Rejected)?;
            st.start_radio("song", &seed.video_id, Some(seed.title), None)
                .await
                .map_err(|_| Refused::Rejected)?;
        }
    }
    Ok(())
}
