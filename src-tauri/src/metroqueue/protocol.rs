//! Wire models of the MetroQueue receiver protocol (docs/metroqueue/PROTOCOL.md), mirroring
//! MetroQueue's `playback/tv/TvProtocol.kt`, plus the mapping to and from Limusic's queue rows.
//!
//! The phone leaves out every field equal to its default, so every incoming struct is
//! `#[serde(default)]` with the Kotlin defaults. Outgoing JSON carries every field, nulls included.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;

use innertube::models::metadata::ArtistRun;
use innertube::SongItem;
use serde::{Deserialize, Serialize};

use crate::state::RepeatMode;

pub const PORT: u16 = 8642;
pub const SERVICE_TYPE: &str = "_metroqueue._tcp.local.";
pub const PROTOCOL_VERSION: i32 = 1;
/// The `/api/sync` format served (SYNC.md).
pub const SYNC_VERSION: i32 = 1;
pub const KIND: &str = "desktop";

const AUDIO_TYPE: &str = "MUSIC_VIDEO_TYPE_ATV";
const VIDEO_TYPE: &str = "MUSIC_VIDEO_TYPE_OMV";
const UPLOAD_TYPE: &str = "MUSIC_VIDEO_TYPE_PRIVATELY_OWNED_TRACK";
const EPISODE_TYPE: &str = "MUSIC_VIDEO_TYPE_PODCAST_EPISODE";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteArtist {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub art: Option<String>,
    pub duration: i64,
    pub pick: bool,
    pub thumb: Option<String>,
    pub artists: Vec<RemoteArtist>,
    pub album_id: Option<String>,
    pub album_title: Option<String>,
    pub explicit: bool,
    pub video_type: Option<String>,
    pub set_video_id: Option<String>,
    pub episode: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteState {
    pub track: Option<RemoteTrack>,
    pub playing: bool,
    pub position: i64,
    pub duration: i64,
    pub shuffle: bool,
    pub repeat: i32,
    pub volume: f32,
    pub queue_title: Option<String>,
    pub index: i32,
    pub queue_hash: i32,
    pub playback_state: i32,
    pub speed: f32,
}

#[derive(Debug, Serialize)]
pub struct Info<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: i32,
    pub paired: bool,
    pub kind: &'a str,
    pub sync: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PairStart {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PairFinish {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairFinishResponse<'a> {
    pub token: &'a str,
    pub tv_id: &'a str,
    pub tv_name: &'a str,
}

/// YouTube Music's `watchEndpoint`, as the phone sends it. Only `videoId`, `playlistId` and
/// `index` are acted on; the rest is carried so a snapshot round-trips.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchEndpoint {
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_set_video_id: Option<String>,
    pub params: Option<String>,
    pub index: Option<i32>,
    pub watch_endpoint_music_supported_configs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QueueSnapshot {
    pub title: Option<String>,
    pub items: Vec<RemoteTrack>,
    pub index: i64,
    pub position: i64,
    pub playing: bool,
    pub endpoint: Option<WatchEndpoint>,
    pub continuation: Option<String>,
    pub preload: Option<RemoteTrack>,
}

impl Default for QueueSnapshot {
    fn default() -> Self {
        Self {
            title: None,
            items: Vec::new(),
            index: 0,
            position: 0,
            playing: true,
            endpoint: None,
            continuation: None,
            preload: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ItemsRequest {
    pub items: Vec<RemoteTrack>,
    pub mode: String,
    pub index: i64,
}

impl Default for ItemsRequest {
    fn default() -> Self {
        Self { items: Vec::new(), mode: String::new(), index: -1 }
    }
}

/// `POST /api/cmd`. Indexes are play-order indexes (Limusic's queue is always in play order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    Toggle,
    Resume,
    Pause,
    Next,
    Prev,
    /// Milliseconds.
    Seek(i64),
    Jump(usize),
    /// Index, milliseconds.
    JumpTo(usize, i64),
    /// 0..=100.
    Volume(i64),
    Shuffle,
    SetShuffle(bool),
    Repeat,
    SetRepeat(RepeatMode),
    Remove(usize),
    Move(usize, usize),
    Clear,
    Radio,
}

/// `None` for an unknown action or a negative index (answered `400`). The browser remote's
/// `play` / `playNext` / `add` (by search id) are not served.
pub fn parse_cmd(action: &str, a: i64, b: i64) -> Option<Cmd> {
    let index = |v: i64| usize::try_from(v).ok();
    Some(match action {
        "toggle" => Cmd::Toggle,
        "resume" => Cmd::Resume,
        "pause" => Cmd::Pause,
        "next" => Cmd::Next,
        "prev" => Cmd::Prev,
        "seek" => Cmd::Seek(a.max(0)),
        "jump" => Cmd::Jump(index(a)?),
        "jumpTo" => Cmd::JumpTo(index(a)?, b.max(0)),
        "volume" => Cmd::Volume(a.clamp(0, 100)),
        "shuffle" => Cmd::Shuffle,
        "setShuffle" => Cmd::SetShuffle(a != 0),
        "repeat" => Cmd::Repeat,
        "setRepeat" => Cmd::SetRepeat(repeat_mode(a)),
        "remove" => Cmd::Remove(index(a)?),
        "move" => Cmd::Move(index(a)?, index(b)?),
        "clear" => Cmd::Clear,
        "radio" => Cmd::Radio,
        _ => return None,
    })
}

/// Media3 repeat values: 0 off, 1 one, 2 all (clamped).
pub fn repeat_mode(v: i64) -> RepeatMode {
    match v.clamp(0, 2) {
        0 => RepeatMode::Off,
        1 => RepeatMode::One,
        _ => RepeatMode::All,
    }
}

pub fn repeat_code(mode: RepeatMode) -> i32 {
    match mode {
        RepeatMode::Off => 0,
        RepeatMode::One => 1,
        RepeatMode::All => 2,
    }
}

/// A user-added row ("Play next" / "Add to queue"), the phone's `pick`.
fn is_pick(s: &SongItem) -> bool {
    s.queued || s.queued_end
}

/// Changes whenever the rows, their order or their `pick` flags change (the phone refetches
/// `/api/queue` only then). Deterministic: `DefaultHasher::new` uses fixed keys.
pub fn queue_hash(items: &[SongItem]) -> i32 {
    let mut h = DefaultHasher::new();
    for s in items {
        (&s.video_id, &s.set_video_id, is_pick(s)).hash(&mut h);
    }
    h.finish() as i32
}

// shortcut: `art` is the row's own thumbnail, not resized to 120/720 px; the phone draws
// `thumb ?: art` anyway. Resize if a client starts reading `art`.
pub fn track_from_song(s: &SongItem) -> RemoteTrack {
    let video_type = if s.is_episode {
        EPISODE_TYPE
    } else if s.is_upload {
        UPLOAD_TYPE
    } else if s.is_video {
        VIDEO_TYPE
    } else {
        AUDIO_TYPE
    };
    RemoteTrack {
        id: s.video_id.clone(),
        title: s.title.clone(),
        artist: s.artists.clone(),
        art: s.thumbnail.clone(),
        duration: crate::state::parse_duration_ms(s.duration.as_deref()),
        pick: is_pick(s),
        thumb: s.thumbnail.clone(),
        artists: s
            .artist_runs
            .iter()
            .map(|r| RemoteArtist { id: r.id.clone(), name: r.text.clone() })
            .collect(),
        album_id: s.album_id.clone(),
        album_title: s.album.clone(),
        explicit: s.explicit,
        video_type: Some(video_type.to_owned()),
        set_video_id: s.set_video_id.clone(),
        episode: s.is_episode,
    }
}

pub fn song_from_track(t: RemoteTrack) -> SongItem {
    let artists = if t.artist.is_empty() {
        t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    } else {
        t.artist
    };
    let video_type = t.video_type.as_deref().unwrap_or(AUDIO_TYPE);
    SongItem {
        video_id: t.id,
        title: t.title,
        artists,
        artist_id: t.artists.first().and_then(|a| a.id.clone()),
        artist_runs: t.artists.into_iter().map(|a| ArtistRun { text: a.name, id: a.id }).collect(),
        album: t.album_title,
        album_id: t.album_id,
        duration: (t.duration > 0).then(|| format_duration(t.duration)),
        thumbnail: t.thumb.or(t.art),
        set_video_id: t.set_video_id,
        queued: t.pick,
        is_video: !t.episode && ![AUDIO_TYPE, UPLOAD_TYPE, EPISODE_TYPE].contains(&video_type),
        is_episode: t.episode,
        is_upload: video_type == UPLOAD_TYPE,
        explicit: t.explicit,
        ..Default::default()
    }
}

/// `m:ss` / `h:mm:ss`, the shape `parse_duration_ms` reads back.
fn format_duration(ms: i64) -> String {
    let s = ms / 1000;
    match s / 3600 {
        0 => format!("{}:{:02}", s / 60, s % 60),
        h => format!("{h}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

/// Loopback, RFC 1918, link-local or IPv6 ULA, like the TV's `fromLan`.
pub fn is_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_lan(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || first & 0xffc0 == 0xfe80 || first & 0xfe00 == 0xfc00
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_the_kotlin_defaults() {
        let s: QueueSnapshot = serde_json::from_str("{}").unwrap();
        assert_eq!((s.index, s.position, s.playing), (0, 0, true));
        let r: ItemsRequest = serde_json::from_str(r#"{"items":[],"mode":"add"}"#).unwrap();
        assert_eq!(r.index, -1);
        let t: RemoteTrack = serde_json::from_str(
            r#"{"id":"abc","title":"T","artist":"A","art":null,"duration":61000,"extra":1}"#,
        )
        .unwrap();
        assert!(!t.pick && !t.explicit && t.artists.is_empty() && t.thumb.is_none());
    }

    #[test]
    fn state_serializes_every_field_in_camel_case() {
        let v = serde_json::to_value(RemoteState {
            track: None,
            playing: false,
            position: 0,
            duration: 0,
            shuffle: false,
            repeat: 0,
            volume: 1.0,
            queue_title: None,
            index: -1,
            queue_hash: 0,
            playback_state: 1,
            speed: 1.0,
        })
        .unwrap();
        for key in ["track", "queueTitle", "queueHash", "playbackState", "speed", "volume"] {
            assert!(v.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse_cmd("toggle", 0, 0), Some(Cmd::Toggle));
        assert_eq!(parse_cmd("seek", 83412, 0), Some(Cmd::Seek(83412)));
        assert_eq!(parse_cmd("jumpTo", 3, 5000), Some(Cmd::JumpTo(3, 5000)));
        assert_eq!(parse_cmd("volume", 140, 0), Some(Cmd::Volume(100)));
        assert_eq!(parse_cmd("setShuffle", 1, 0), Some(Cmd::SetShuffle(true)));
        assert_eq!(parse_cmd("setRepeat", 1, 0), Some(Cmd::SetRepeat(RepeatMode::One)));
        assert_eq!(parse_cmd("setRepeat", 9, 0), Some(Cmd::SetRepeat(RepeatMode::All)));
        assert_eq!(parse_cmd("move", 4, 2), Some(Cmd::Move(4, 2)));
        assert_eq!(parse_cmd("remove", -1, 0), None);
        assert_eq!(parse_cmd("play", 0, 0), None);
        assert_eq!(parse_cmd("bogus", 0, 0), None);
    }

    #[test]
    fn tracks_round_trip_through_queue_rows() {
        let t = RemoteTrack {
            id: "dQw4w9WgXcQ".into(),
            title: "Song".into(),
            artist: "A, B".into(),
            art: Some("art".into()),
            duration: 213_000,
            pick: true,
            thumb: Some("thumb".into()),
            artists: vec![
                RemoteArtist { id: Some("UC1".into()), name: "A".into() },
                RemoteArtist { id: None, name: "B".into() },
            ],
            album_id: Some("MPREb_x".into()),
            album_title: Some("Album".into()),
            explicit: true,
            video_type: Some(AUDIO_TYPE.into()),
            set_video_id: Some("set".into()),
            episode: false,
        };
        let song = song_from_track(t.clone());
        assert_eq!(song.duration.as_deref(), Some("3:33"));
        assert!(song.queued && !song.is_video);
        let back = track_from_song(&song);
        assert_eq!(back, RemoteTrack { art: Some("thumb".into()), ..t });
    }

    #[test]
    fn queue_hash_tracks_rows_order_and_picks() {
        let a = SongItem { video_id: "a".into(), ..Default::default() };
        let b = SongItem { video_id: "b".into(), ..Default::default() };
        let h = queue_hash(&[a.clone(), b.clone()]);
        assert_eq!(h, queue_hash(&[a.clone(), b.clone()]));
        assert_ne!(h, queue_hash(&[b.clone(), a.clone()]));
        assert_ne!(h, queue_hash(&[a.clone(), SongItem { queued: true, ..b }]));
    }

    #[test]
    fn lan_check() {
        for ok in ["127.0.0.1", "10.1.2.3", "172.16.0.9", "192.168.1.20", "169.254.3.4", "::1"] {
            assert!(is_lan(ok.parse().unwrap()), "{ok}");
        }
        for ok in ["fe80::1", "fd12:3456::1", "::ffff:192.168.1.2"] {
            assert!(is_lan(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "172.32.0.1", "2001:db8::1", "::ffff:1.1.1.1"] {
            assert!(!is_lan(bad.parse().unwrap()), "{bad}");
        }
    }
}
