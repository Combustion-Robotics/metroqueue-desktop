# MetroQueue data sync (format v1)

Two-way sync of data that only MetroQueue has (not the YouTube account) between a phone and a
paired receiver (MetroQueue Desktop; MetroQueue on Android TV implements it too). Code:
`app/src/main/kotlin/com/metrolist/music/playback/tv/TvSync.kt` (format + merge rules, unit tested
in `app/src/test/.../playback/tv/SyncMergeTest.kt`) and `TvDataSync.kt` (reading/applying data).
Transport, pairing and auth are in PROTOCOL.md.

## 1. What is synced

| Data | Phone storage | Merge |
|---|---|---|
| Play history | Room table `event` (`songId`, `timestamp`, `playTime`), songs in `song` | union, deduplicated by `(songId, timestamp)` |
| Custom transitions | `files/mix_transitions.json` (`CustomTransitionRepository`) | per item last-writer-wins + tombstones, key = pair key |
| Custom vibes | `files/vibes.json` (`CustomVibes`) | per item last-writer-wins + tombstones, key = vibe `id` |
| Shared settings | DataStore prefs (whitelist below) | per key last-writer-wins |
| Local playlists | Room tables `playlist` + `playlist_song_map`, songs in `song`; tombstones in `files/playlist_tombstones.json` | per playlist last-writer-wins + tombstones, key = playlist `id` (section 8) |

Not synced: deletions of history ("clear history" stays local), the YouTube account, liked songs /
YouTube playlists (those are the TV's library profiles, `/api/profile`), any other setting.

## 2. Endpoints (served by the receiver)

A receiver advertises support with `"sync": 1` in `GET /api/info` (`0`/absent = unsupported).
Both endpoints need a **paired** bearer token (`Authorization: Bearer <token>`) and a LAN peer;
`?key=` browser access is not enough. Errors: `403` (not paired / not LAN), `404` (unsupported),
`400` (unparseable body).

### `GET /api/sync?since=<epoch ms>`

Returns the receiver's `SyncState` (section 3). `since` (default `0`) only limits `history` to plays
with `timestamp >= since`; transitions, vibes, settings, playlists and tombstones are always complete (they
are small). `history` holds at most the **5000 newest** plays from `since` on.

### `POST /api/sync`

Body: the phone's `SyncState` after it merged the receiver's into its own. May be sent with
`Content-Encoding: gzip` (the phone always gzips). Limits: 16 MiB on the wire, 64 MiB inflated.
The receiver merges it into its own data with the rules in section 4 (as the "local" side, see
ties) and answers `204`.

## 3. `SyncState` JSON

```json
{
  "version": 1,
  "now": 1762250000000,
  "history": [
    {"songId": "dQw4w9WgXcQ", "timestamp": 1762240000000, "playTime": 212000}
  ],
  "songs": [
    {"id": "dQw4w9WgXcQ", "title": "Song", "artist": "Artist", "art": null, "duration": 213000,
     "pick": false, "thumb": "https://lh3.googleusercontent.com/...", "artists": [{"id": "UC...", "name": "Artist"}],
     "albumId": "MPREb_...", "albumTitle": "Album", "explicit": false, "videoType": "MUSIC_VIDEO_TYPE_ATV",
     "setVideoId": null, "episode": false}
  ],
  "transitions": [
    {"fromId": "aaa", "toId": "bbb", "style": "ECHO", "durationMs": 6000, "outgoingEndMs": 201000,
     "incomingStartMs": null, "tempoMatch": true, "updatedAt": 1762100000000}
  ],
  "vibes": [
    {"id": "custom_4b0c...", "name": "Late night", "scene": { /* SceneSpec */ },
     "music": {"localPlaylistId": null, "playlistId": "PL...", "seed": null, "playlistTitle": "…", "playlistThumbnail": "…"},
     "updatedAt": 1762000000000}
  ],
  "settings": {
    "mixMode": {"value": "SMART", "updatedAt": 1761000000000},
    "crossfadeDurationFloat": {"value": 8.0, "updatedAt": 1}
  },
  "deleted": {
    "transitions": {"aaa\nccc": 1762200000000},
    "vibes": {"custom_77aa...": 1762150000000}
  },
  "playlists": [
    {"id": "LPabcdEFGH", "name": "Road trip", "updatedAt": 1762300000000, "deleted": false,
     "thumbnailUrl": null,
     "songs": [{"id": "dQw4w9WgXcQ", "title": "Song", "artists": [{"id": "UC...", "name": "Artist"}],
                "album": {"id": "MPREb_...", "title": "Album"}, "thumbnail": "https://...",
                "durationMs": 213000, "local": false}]},
    {"id": "LPDq3...", "name": "", "updatedAt": 1762310000000, "deleted": true, "thumbnailUrl": null, "songs": []}
  ]
}
```

General rules: all times are **epoch milliseconds (UTC)**. Unknown keys must be ignored; missing
keys take the defaults below. MetroQueue writes every field (also defaults and `null`s), but a
receiver must accept missing ones. A receiver must keep a vibe's `scene` and `music` objects as
received (it may not know every field yet) and send them back unchanged.

| Field | Type | Default | |
|---|---|---|---|
| `version` | int | `1` | format version. Future versions only add optional fields; a reader ignores what it doesn't know. |
| `now` | long | `0` | sender's clock when the state was built. The phone uses the receiver's `now` as the next `since`. |
| `history` | `SyncHistoryItem[]` | `[]` | plays, newest first |
| `songs` | `RemoteTrack[]` | `[]` | metadata of every song in `history` (each once), as in PROTOCOL.md; `art` is `null`, use `thumb` |
| `transitions` | `CustomTransition[]` | `[]` | **all** custom transitions |
| `vibes` | `Vibe[]` | `[]` | **all** custom vibes (ids start with `custom_`; presets are never sent) |
| `settings` | `{name: SyncSetting}` | `{}` | the whitelisted settings (section 5) |
| `deleted` | `{transitions: {key: ms}, vibes: {id: ms}}` | empty | tombstones: when each item was deleted |
| `playlists` | `SyncPlaylist[]` | `[]` | **all** local playlists, then a tombstone per deleted one (section 8). Added later in v1; older peers ignore it. |

`SyncHistoryItem`: `songId` (string, required), `timestamp` (long, required, when the play
happened), `playTime` (long ms listened, default `0`).

`CustomTransition` (as in `mix_transitions.json`): `fromId`, `toId` (required video ids),
`style` (`"FADE"` default, `"FILTER"`, `"ECHO"`, `"RISE"`, `"CUT"`; unknown -> `"FADE"`),
`durationMs` (default `5000`, clamped to `1000..16000`), `outgoingEndMs` (long?, default `null` =
where the outgoing audio ends), `incomingStartMs` (long?, default `null` = skip silent intro),
`tempoMatch` (default `true`), `updatedAt` (default `0`). Its key is
**`fromId + "\n" + toId`** (a newline between the two ids); that is the key in
`deleted.transitions`. At most 2000 are kept (the least recently changed are dropped).

`Vibe` (as in `vibes.json`): `id`, `name`, `scene` (SceneSpec, see `ui/vibe/scene/SceneSpec.kt`),
`music` (`VibeMusic`), `updatedAt` (default `0`). Key = `id`.

`SyncSetting`: `value` (JSON boolean / number / string, or `null` = not set, i.e. the app default),
`updatedAt` (long, default `0`).

### Timestamps on stored items

`updatedAt` was added to `mix_transitions.json` and `vibes.json` entries; older files have none and
read as `0` (older app versions ignore the new field). Saving an item in the app sets `updatedAt` to
the current time. Items merged in from another device keep the other device's `updatedAt`.

## 4. Merge rules

Both sides use the same function (`SyncMerge.merge`), for transitions, vibes and settings.

For every key present locally or remotely:

1. Pick the copy with the larger `updatedAt`. **On a tie the receiver's copy wins**: the phone
   (merging the receiver's GET response) prefers the remote copy, the receiver (merging the
   phone's POST) prefers its local copy. Since the phone sends back what it adopted, both converge.
2. Tombstones are merged per key, keeping the **larger** deletion time.
3. The picked item survives only if `updatedAt > deletedAt` (strictly). A deletion wins over a
   change made at the same millisecond or earlier.
4. Tombstones whose item survived are dropped. Remaining tombstones are capped to the **500
   newest per kind** (transitions / vibes). Deleting an item adds a tombstone `now`; an item is never
   "un-deleted" except by changing it after the deletion.

History: the union. A play from the other side is inserted unless a local play with the same
`songId` and `timestamp` (to the millisecond) exists. Plays with an empty `songId`, a
`timestamp <= 0` or no entry in `songs` are skipped. The receiver must create the song from
`songs` when it doesn't know it. Plays are never deleted by sync.

Settings, extra rules:
- A remote setting with `updatedAt == 0` (never set on that device) is ignored, so a fresh install
  never resets the other side's settings.
- A value that isn't valid for its key (wrong JSON type, unknown enum name, out of range) is
  ignored.
- Change times: each device stores, per whitelisted key, the last value it saw and when it changed
  (DataStore key `syncSettings`, JSON `{name: {value, updatedAt}}`). A watcher updates it whenever
  one of the prefs changes. A value first seen without history gets `updatedAt = 1` if set
  (older than any real change, but it beats "never set") or `0` if unset. A value applied by sync
  is stored with the other device's time.

## 5. Settings whitelist

| Name (DataStore key) | JSON type | Values |
|---|---|---|
| `crossfadeEnabled` | boolean | Mix on/off (with `mixMode`: off = `false`) |
| `mixMode` | string | `OFF`, `CROSSFADE`, `SMART` |
| `mixTransitionStyle` | string | `AUTO`, `FADE`, `FILTER`, `ECHO`, `RISE`, `CUT` |
| `crossfadeDurationFloat` | number | Mix duration in seconds, `1.0..15.0` |
| `crossfadeOnSkip` | boolean | mix when skipping |
| `crossfadeGapless` | boolean | keep gapless albums gapless |
| `lyricsEffects` | string | `FULL`, `SUBTLE`, `OFF` |
| `experimentalLyrics` | boolean | |
| `pureBlack` | boolean | pure black dark theme |
| `dynamicTheme` | boolean | colours from the artwork |
| `selectedThemeColor` | number (int) | accent colour as ARGB int (used when `dynamicTheme` is off) |

The effective Mix mode on MetroQueue: `crossfadeEnabled` unset -> `SMART`; `false` -> `OFF`;
`true` -> `SMART` if `mixMode == "SMART"`, else `CROSSFADE`. A receiver may ignore keys it has no
equivalent for, but must keep and send them back (so they reach the user's other devices).

## 6. Flow (the phone is the client)

```
phone                                         receiver
  | GET /api/sync?since=<mark.remote - 5 min> --> builds SyncState (history since `since`)
  |  <------------------------------------------- 200 SyncState (now = receiver clock)
  | merge into local data (remote wins ties)
  | POST /api/sync  (gzip, phone's merged state,
  |   history since mark.local - 5 min) --------> merges (local wins ties)
  |  <------------------------------------------- 204
  | mark = {remote: response.now, local: phone time when the sync started}
```

- The 5-minute overlap absorbs small clock differences; merging is idempotent, so resent plays and
  items change nothing.
- First sync with a device: `since = 0` both ways (the newest 5000 plays each way).
- When: "Sync now" in the device sheet while connected, and automatically right after connecting
  when "Sync automatically when connecting" is on (default **on for desktop**, off for TV; stored
  per kind as `syncDataAuto_desktop` / `syncDataAuto_tv`).
- Known limit: history moves by play time, so plays older than the last sync that reach the phone
  later from a third device are not forwarded to the receiver.

## 7. Receiver checklist

- Serve `GET /api/info` with `"sync": 1`.
- Keep, per item, `updatedAt` (set it to now on every local edit) and tombstones for deletions
  (time of deletion; cap 500 newest per kind).
- Keep the settings' change times (above). Map the whitelist to your own settings where you can.
- `GET /api/sync`: send everything as in section 3; history newest first, max 5000, from `since`.
- `POST /api/sync`: validate, merge with "local wins ties", insert new plays (creating songs from
  `songs`), answer `204`.

## 8. Playlists

Only **local, user-editable** playlists sync. Phone: `playlist` rows with `browseId == null` and
`isEditable == true`, except the built-in `LP_LIKED`, `LP_DOWNLOADED`, `LP_WEEKLY_MOST` and
`LP_MONTHLY_MOST`. YouTube playlists are not included; they sync through the account.

`SyncPlaylist`: `id` (string, required), `name` (default `""`), `updatedAt` (long, default `0`),
`deleted` (default `false`), `thumbnailUrl` (string?, default `null`), `songs` (ordered
`SyncPlaylistSong[]`, default `[]`).
- `id`: a phone-created playlist keeps the phone's `playlist.id` (`LP` + 8 letters); a
  desktop-created one is `LPD` + 22 random `[A-Za-z0-9]`.
- `updatedAt`: the last change of the name, the songs or their order, or the cover. The phone uses
  `playlist.lastUpdateTime`, which every edit (rename, add, remove, reorder, cover) sets to now.
- `thumbnailUrl`: a custom cover, only `http(s)` URLs. A cover the phone picked from its gallery is
  a `content://` URI and is sent as `null`; applying a remote copy with `null` keeps such a cover.
- `deleted: true` is a tombstone: `updatedAt` is the deletion time and the other fields are empty.

`SyncPlaylistSong`: `id` (YouTube video id, or the sender's local-file id when `local` is `true`),
`title` (default `""`), `artists` (`[{id?, name}]`), `album` (`{id?, title}` or `null`),
`thumbnail` (string?), `durationMs` (long, default `0` = unknown), `local` (default `false`).

Merge: `SyncMerge.playlists`, i.e. section 4 per playlist `id`, with the tombstones taken from the
list (`deleted: true`, time = `updatedAt`). The newer `updatedAt` wins, the receiver on a tie, and
the winner **replaces** the loser's whole playlist (name, ordered songs, cover); there is no
per-song merge. A deletion beats changes made at the same millisecond or earlier. Tombstones are
capped to the 500 newest. A remote playlist whose `id` names a playlist that is not synced here
(a YouTube or built-in one) is ignored.

Apply: create or update the playlist (new ones are added to the library), replace its songs in the
received order, set its `updatedAt` to the winner's, and delete playlists the merge dropped.
Songs this device doesn't know are created from the entry's metadata, so the playlist shows without
a network fetch; known songs are left as they are. Local-file songs of the other device stay in
the list (a device that can't play them should grey them out or skip them).
