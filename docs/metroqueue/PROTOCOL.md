# MetroQueue receiver protocol (v1)

How a MetroQueue phone discovers, pairs with and controls a **receiver** (MetroQueue on Android TV
today, MetroQueue Desktop next). Derived from the MetroQueue code on `dev`:

| Piece | File (app/src/main/kotlin/com/metrolist/music/…) |
|---|---|
| Wire models | `playback/tv/TvProtocol.kt` |
| Pairing, mDNS advertising | `playback/tv/TvHost.kt` (`TvPairing`, `TvAdvertiser`) |
| Discovery (phone) | `playback/tv/TvDiscovery.kt` |
| Receiver HTTP + WebSocket server | `playback/RemoteControlServer.kt` |
| Phone HTTP client | `playback/tv/TvRemoteClient.kt` |
| Phone-side player proxy (what commands it sends) | `playback/tv/TvRemotePlayer.kt` |
| Phone connection/reconnect/handoff logic | `playback/tv/TvRemoteManager.kt` |
| Library profiles (TV only) | `playback/tv/TvProfiles.kt`, `TvLibrarySync.kt` |
| Two-way data sync | see `SYNC.md` (`playback/tv/TvSync.kt`, `TvDataSync.kt`) |

A receiver is "compatible" when it implements everything marked **required** below. Things marked
*optional* may answer `404` and the phone copes.

---

## 1. Transport, JSON conventions

- Plain HTTP/1.1 + WebSocket on **TCP port 8642** (`RemoteControlServer.PORT`), all interfaces.
  No TLS: trust comes from the LAN-only check and the pairing token.
- **LAN-only.** The TV rejects any request whose peer address is not loopback, RFC 1918 site-local
  (10/8, 172.16/12, 192.168/16), link-local (169.254/16, fe80::/10) or IPv6 ULA (fc00::/7). The
  peer must be a literal IP (no DNS lookups). A desktop receiver should do the same.
- Bodies are UTF-8 JSON (`Content-Type: application/json`). Responses from the receiver include
  every field, `null`s included (kotlinx `encodeDefaults = true`).
- **The phone omits fields that equal their default** (its `Json` has `encodeDefaults = false`),
  e.g. a `QueueSnapshot` with `index = 0`, `position = 0`, `playing = true` arrives without those
  keys, a `RemoteTrack` without `pick`/`explicit`/`episode` when false and without `null`s.
  **A receiver must apply the defaults listed below for missing keys.**
- Both sides ignore unknown keys. New fields must be optional with a default.
- Times are milliseconds. Durations/positions are ms (`Long`). Volume is `0.0..1.0`.

## 2. Discovery (mDNS / DNS-SD)

| | |
|---|---|
| Service type | `_metroqueue._tcp` (i.e. `_metroqueue._tcp.local.`) |
| Instance name | the receiver's user-visible name, e.g. `Living room TV`, `Salva's laptop`. Shown in the picker before pairing. |
| Port | `8642` |
| TXT `id` | the receiver's stable id (TV: a random UUID generated once, persisted). **Required.** The phone matches discovered receivers to paired ones by this id. |
| TXT `v` | protocol version, `"1"` |
| TXT `kind` | `"tv"` or `"desktop"`. Absent = `"tv"` (older TVs). A desktop receiver **must** send `kind=desktop`. |

Phone behaviour: discovery runs only while the device picker is open or while reconnecting. It
resolves each service, prefers an IPv4 address, de-duplicates by instance name or `id`, and drops
an entry on "service lost". Users can also type `host[:port]` manually (default port 8642); the
phone then calls `GET /api/info` to learn the id/name/kind.

## 3. Authentication

Every endpoint except `/api/info`, `/api/pair/*` requires one of:

1. **Paired phone**: header `Authorization: Bearer <token>` (the phone always sends it, also on the
   WebSocket upgrade). The receiver hashes the token (SHA-256, lowercase hex of the UTF-8 bytes)
   and compares it, in constant time, with the stored hashes of paired phones.
2. *(TV/phone browser remote only, optional for desktop)*: query parameter `?key=<browserKey>`.

Failure: `403 Forbidden` (HTTP) or WebSocket close code `1008` (policy violation) with reason
`forbidden`. `/api/profile` and `/api/sync` accept **only** a paired token (not `?key=`).

Revocation: when a paired phone is forgotten on the receiver, its open WebSocket must be closed with
`1008` reason `revoked` and its token must stop working immediately.

## 4. Pairing

```
phone                                   receiver
  | POST /api/pair/start {name}  ------>  shows "Pair with <name>?" + 6-digit code (2 min)
  |  <------------------------------ 204
  | (user types the code on the phone)
  | POST /api/pair/finish {code,name} ->  checks code, creates token, stores SHA-256(token)
  |  <---------------- 200 {token, tvId, tvName}
  | GET /api/info  (Bearer token) ----->  {"paired": true, ...}
```

Receiver rules (`TvPairing`), which a desktop receiver should copy:

- Code: `"%06d"` of a `SecureRandom` int in `0..999999`; valid **2 minutes**; a new `start` replaces
  the previous code. The prompt shows the phone's `name` (trimmed, max 40 chars, empty -> `"Phone"`).
- `start` rate limit: at most **3 starts per 60 s** (sliding window); refused while locked out.
- Each code allows **5 wrong attempts**; after the 5th it is discarded (answer `410`).
- Lockout: **10 wrong codes within 10 minutes** -> locked for **5 minutes** (code discarded).
- Code compare is constant time, after trimming whitespace.
- Token: **32 bytes** from `SecureRandom`, Base64 **URL-safe, no padding, no wrap** (43 chars).
  Only `SHA-256(token)` as lowercase hex is stored, with a new random UUID as the paired phone's id,
  its name and `pairedAt` (epoch ms).
- The receiver keeps a list of paired phones (UI: "Paired phones", Forget = revoke).

### `POST /api/pair/start`  (no auth, LAN only)

Request: `{"name": "Pixel 9"}` (the phone's `Settings.Global.DEVICE_NAME`, else `Build.MODEL`).

| Status | Meaning |
|---|---|
| `204` | code is now shown |
| `400` | body not parseable |
| `404` | not a receiver / not from LAN |
| `429` | rate limited or locked out |

### `POST /api/pair/finish`  (no auth, LAN only)

Request: `{"code": "042917", "name": "Pixel 9"}`

`200` response:
```json
{"token": "q5n0m3...43chars", "tvId": "6f1c8a3e-...", "tvName": "Living room TV"}
```

| Status | Meaning (phone message) |
|---|---|
| `200` | paired |
| `400` | body not parseable |
| `401` | wrong code, try again ("That code isn't right") |
| `410` | no code / expired / too many tries on this code / couldn't save ("The code expired") |
| `429` | locked out ("Too many tries") |
| `404` | not a receiver / not from LAN |

## 5. Endpoints

All paths below are relative to `http://<host>:8642`. "Auth" = section 3.

### `GET /api/info`  (no auth required; LAN only) — **required**

The "hello". Sending the bearer token makes `paired` meaningful.

```json
{"id": "6f1c8a3e-...", "name": "Living room TV", "version": 1, "paired": true, "kind": "tv", "sync": 1}
```

| Field | Type | Default | |
|---|---|---|---|
| `id` | string | — | same as TXT `id` |
| `name` | string | — | display name |
| `version` | int | `1` | protocol version |
| `paired` | bool | `false` | `true` iff the request carried a valid paired token |
| `kind` | string | `"tv"` | `"tv"` / `"desktop"` |
| `sync` | int | `0` | data-sync version the receiver serves at `/api/sync` (see SYNC.md); `0` = none |

`404` when the server isn't a receiver (a phone's browser-remote server) or the caller isn't on
the LAN. On connect the phone calls this with its token; `paired: false` means the receiver
forgot the phone, so the phone deletes its pairing and asks to pair again.

### `GET /api/state` — **required**

Current player state (`RemoteState`):

```json
{
  "track": { /* RemoteTrack, art resized to 720 px */ },
  "playing": true,
  "position": 83412,
  "duration": 215000,
  "shuffle": false,
  "repeat": 0,
  "volume": 0.8,
  "queueTitle": "Radio · Song name",
  "index": 3,
  "queueHash": -1823370121,
  "playbackState": 3,
  "speed": 1.0
}
```

| Field | Type | Meaning |
|---|---|---|
| `track` | RemoteTrack? | current item, `null` when the queue is empty |
| `playing` | bool | play-when-ready (true also while buffering) |
| `position` | ms | position at the moment the state was built |
| `duration` | ms | current item's duration, `0` if unknown |
| `shuffle` | bool | |
| `repeat` | int | `0` off, `1` repeat one, `2` repeat all (Media3 values) |
| `volume` | float | app volume `0..1` |
| `queueTitle` | string? | |
| `index` | int | current item's index **in play order** (the order `/api/queue` returns), `-1` if empty |
| `queueHash` | int | any int that changes whenever the queue's items, order or `pick` flags change. The phone refetches `/api/queue` only when it differs. |
| `playbackState` | int | `1` idle, `2` buffering, `3` ready, `4` ended. Default `1`. |
| `speed` | float | playback speed, default `1.0` |

The phone **extrapolates** the position locally: while `playing && playbackState == 3` it shows
`position + (now - receivedAt) * speed`. So a receiver need not push periodic position updates,
but must push on every seek/discontinuity, play/pause, buffering change, track change, queue
change, shuffle/repeat/speed/volume change.

### `GET /api/queue` — **required**

The whole queue in **play order** (shuffled order when shuffle is on), as `RemoteTrack[]`:

```json
[
  {
    "id": "dQw4w9WgXcQ",
    "title": "Song",
    "artist": "Artist A, Artist B",
    "art": "https://lh3.googleusercontent.com/...=w120-h120-...",
    "duration": 213000,
    "pick": false,
    "thumb": "https://lh3.googleusercontent.com/...",
    "artists": [{"id": "UC...", "name": "Artist A"}, {"id": null, "name": "Artist B"}],
    "albumId": "MPREb_...",
    "albumTitle": "Album",
    "explicit": false,
    "videoType": "MUSIC_VIDEO_TYPE_ATV",
    "setVideoId": null,
    "episode": false
  }
]
```

`RemoteTrack` fields:

| Field | Type | Default | |
|---|---|---|---|
| `id` | string | required | YouTube video id |
| `title` | string | required | |
| `artist` | string | required | artists joined with `", "` |
| `art` | string? | required (may be null) | thumbnail resized (120 px in lists, 720 px in `state.track`) |
| `duration` | ms | required | `0` if unknown |
| `pick` | bool | `false` | user-added ("played next / added") item, as opposed to radio/autoplay |
| `thumb` | string? | `null` | original thumbnail URL |
| `artists` | `[{id?, name}]` | `[]` | |
| `albumId`, `albumTitle` | string? | `null` | |
| `explicit` | bool | `false` | |
| `videoType` | string? | `null` | `MUSIC_VIDEO_TYPE_ATV` (song), `_OMV`, `_UGC`, … |
| `setVideoId` | string? | `null` | playlist item id |
| `episode` | bool | `false` | podcast episode |

The phone uses `thumb ?: art` for artwork and `artists` (falling back to `artist`) for credits.
Note: `/api/state` and `/api/queue` are fetched separately, so `index` refers to the queue as of
the same `queueHash`.

### `GET /api/snapshot` — **required**

The receiver's queue as a `QueueSnapshot` (below) so the phone can continue it locally ("Play
here"). `endpoint` + `continuation` are set when the queue is a radio that can load more.

### `POST /api/load` — **required**: play a queue (index + position)

Body `QueueSnapshot`:

```json
{
  "title": "My playlist",
  "items": [ /* RemoteTrack */ ],
  "index": 4,
  "position": 61000,
  "playing": true,
  "endpoint": {"videoId": "abc", "playlistId": "RDAMVMabc", "params": "wAEB", "index": null,
               "playlistSetVideoId": null, "watchEndpointMusicSupportedConfigs": null},
  "continuation": "CBQSExIL...",
  "preload": { /* RemoteTrack */ }
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `title` | string? | `null` | queue title |
| `items` | RemoteTrack[] | `[]` | the queue, in play order |
| `index` | int | `0` | item to start (clamp into range) |
| `position` | ms | `0` | start position within that item |
| `playing` | bool | `true` | start playing (else load paused) |
| `endpoint` | WatchEndpoint? | `null` | YouTube Music `watchEndpoint` (`videoId`, `playlistId`, `playlistSetVideoId`, `params`, `index`) |
| `continuation` | string? | `null` | YouTube "next" continuation token for `endpoint` |
| `preload` | RemoteTrack? | `null` | the first track, to show at once while `endpoint` resolves |

Semantics (`MusicService.playSnapshot`):
- **`items` non-empty**: replace the queue with `items`, start at `index`/`position`, play if
  `playing`. Items with `pick = true` keep that flag. If `endpoint` **and** `continuation` are set,
  when the queue runs out continue the radio by calling YouTube Music `next` with that endpoint and
  continuation (autoplay).
- **`items` empty, `endpoint` set**: the receiver resolves the queue itself: YouTube Music `next`
  for `endpoint` (a radio / playlist / album "watch" queue), showing `preload` meanwhile, then
  plays (`playing`).
- both empty: no-op.

| Status | |
|---|---|
| `204` | loaded |
| `400` | body not parseable |
| `409` | receiver currently can't be controlled (TV: it is a Listen Together guest) |

The phone sends this when it connects with "Move what's playing here" on (`playing: true`), and
whenever the user starts something while connected (a song, playlist, album, radio: `YouTubeQueue`
is sent as `endpoint` + `preload` only; other queues as `items` + `index` + `position`).

### `POST /api/items` — **required**: play next / add to queue / insert at

```json
{"items": [ /* RemoteTrack */ ], "mode": "next", "index": -1}
```

| `mode` | Meaning |
|---|---|
| `"next"` | insert right after the current item ("Play next"). On the TV the inserted items are marked `pick` if the "prioritize added songs" setting is on. If the queue is empty/idle, play them at once. |
| `"add"` | append to the end of the queue ("Add to queue") |
| `"at"` | insert at play-order `index` (`0..queue.size`). If shuffle is on or `index` is out of range, behaves like `"add"`. |

`index` default `-1` (only used with `"at"`). `204` ok; `400` for an unparseable body, empty
`items`, unknown `mode`, or not controllable.

### `POST /api/cmd?action=<a>&a=<long>&b=<long>[&id=<songId>]` — **required**

Empty body. `a`/`b` default `0`; **queue indexes are play-order indexes** (as in `/api/queue`).
The phone always sends `a` and `b`. `204` ok, `400` unknown action / bad index / not controllable.
After every command the phone immediately `GET`s `/api/state`.

| action | a | b | Effect |
|---|---|---|---|
| `toggle` | | | play/pause toggle |
| `resume` | | | play (re-prepare if idle/ended) |
| `pause` | | | pause |
| `next` | | | skip to next (TV: with crossfade if Mix is on), then play |
| `prev` | | | previous (Media3 `seekToPrevious`: restart if > 3 s in, else previous item), then play |
| `seek` | position ms | | seek within the current item |
| `jump` | index | | play item `a` from its start |
| `jumpTo` | index | position ms | play item `a` from position `b` |
| `volume` | 0..100 | | set volume to `a/100` (clamped). The phone has **25 volume steps** (`MAX_VOLUME = 25`) and sends `round(volume*100)`; volume keys move ±1/25. Mute = `0`, unmute restores the last audible volume. |
| `shuffle` | | | toggle shuffle |
| `setShuffle` | 0/1 | | set shuffle off/on |
| `repeat` | | | cycle repeat off -> all -> one -> off |
| `setRepeat` | 0/1/2 | | set repeat mode (clamped to 0..2) |
| `remove` | index | | remove item `a` |
| `move` | from | to | move item from play-order index `a` to `b` (`b` must be a valid index). With shuffle on, the TV rewrites the shuffle order instead of the playlist. |
| `clear` | | | remove every item except the current one |
| `radio` | | | **start radio**: replace everything after the current item with a YouTube Music radio for the current song (`watchEndpoint{videoId}`), keeping the current song playing |
| `play` / `playNext` / `add` | | | browser-remote only: act on a song by `id` from the last `/api/search` (optional for desktop) |

How the phone maps its player actions (`TvRemotePlayer`):
- seek in current song -> `seek`; seek to another item -> `jumpTo index position`; "next" ->
  `next`; "previous" -> `seek 0` or `jumpTo prevIndex 0` (computed on the phone).
- moving `n` items is sent as `n` single `move` commands; removing a range as repeated
  `remove fromIndex`.
- `setPlayWhenReady(true/false)` -> `resume`/`pause`; stop -> `pause`.
- adding items at the end -> `/api/items` `add`; anywhere else -> `at` with the index.
- replacing the queue (`setMediaItems`) -> `/api/load` with `items`, `index`, `position`.
- "Start radio" in the phone UI -> `cmd radio`.
- Disconnect + "Play here": `GET /api/snapshot`, then `cmd pause`, then the phone plays the
  snapshot locally.

### `GET /api/search?q=<text>` — optional (browser remote)

Song search on YouTube Music; returns `RemoteTrack[]` and remembers them for `cmd play/playNext/add`.

### `GET /api/theme` — optional (browser remote)

Material 3 colours of the current artwork: `{"light": {"primary": "#RRGGBB", ...}, "dark": {...}}`.

### `GET /` — optional (browser remote page, `?key=` auth)

### `POST /api/profile` — optional (TV only; desktop may answer `404`)

The phone's library, stored as that phone's profile on the TV. Auth: paired token only.
Body: `LibrarySnapshot` JSON, usually with `Content-Encoding: gzip` (max 16 MiB compressed, 64 MiB
inflated). `{likedSongs: RemoteTrack[], playlists: [{id, name, thumbnail?, songs: RemoteTrack[]}],
albums: [{id, title, artist?, thumbnail?, year?}], artists: [{id, name, thumbnail?}],
history: RemoteTrack[]}`; caps 5000 songs, 200 playlists, 2000 albums/artists, 500 history.
`204` ok, `400` bad body, `403` not paired, `404` unsupported. The phone only offers this for
`kind = "tv"`.

### `GET /api/sync`, `POST /api/sync` — optional, advertised by `info.sync >= 1`

Two-way sync of MetroQueue-only data. Specified in **SYNC.md**.

## 6. WebSocket `GET /api/ws` — **required**

- Upgrade with the same auth as HTTP (`Authorization: Bearer <token>` header). Unauthorized ->
  accept then close with `1008` `"forbidden"` (the TV) — closing/refusing is fine too.
- **Server -> client**: text frames, each a complete `RemoteState` JSON (same as `/api/state`).
  - one right after the connection opens;
  - then on every relevant change (play/pause, buffering/ready/ended, track change, queue/timeline
    change, seek/discontinuity, shuffle, repeat, speed, volume); bursts are coalesced (the TV waits
    100 ms after each push).
  - there are no other message types. The phone compares `queueHash` and refetches `/api/queue`.
- **Client -> server**: nothing. The server ignores incoming data frames.
- Keepalive: the TV pings every **15 s** and drops the connection after **30 s** without a pong;
  the phone pings every **20 s**.
- Revocation: close `1008` `"revoked"` when that phone is forgotten.
- A normal close is `1000`.

Phone reconnect logic: when the socket closes or fails, the phone shows "Reconnecting…", restarts
discovery and retries after 1, 2, 3, 5, 8, 10 s: each try looks the receiver up by TXT `id` (its
address may have changed), calls `GET /api/info` and, if `paired`, refreshes state and reopens the
WebSocket. If every try fails (or `paired` is false) it gives up and continues the last known queue
locally, paused.

## 7. Phone connect sequence (for reference)

1. `GET /api/info` with token -> must be `paired: true` (else forget + re-pair).
2. If "Move what's playing here" is on and the phone has a queue: `POST /api/load` with the phone's
   snapshot (`playing: true`).
3. `GET /api/state`, `GET /api/queue`.
4. Open `GET /api/ws`.
5. Save the receiver's address, name and `kind`.
6. TV + auto library sync on: `POST /api/profile`. Receiver with `sync >= 1` + auto data sync on
   (default on for desktop, off for TV): the data sync in SYNC.md.

## 8. Status codes summary

| Code | Where | Meaning |
|---|---|---|
| `200` | GETs, pair/finish | JSON body |
| `204` | POSTs | done |
| `400` | any POST, cmd | unparseable body / invalid request / not controllable (`items`, `cmd`) |
| `401` | pair/finish | wrong code |
| `403` | authenticated endpoints | not LAN / no valid token or key |
| `404` | info, pair/*, profile, sync | not a receiver / not LAN / feature unsupported |
| `409` | load | not controllable right now |
| `410` | pair/finish | code expired / missing / used up |
| `429` | pair/start, pair/finish | rate limited / locked out |

Timeouts on the phone: connect 4 s, read/write 15 s (60 s for `/api/profile` and `/api/sync`
uploads). A receiver should answer quickly; a slow `/api/info` makes the phone think it is offline.
