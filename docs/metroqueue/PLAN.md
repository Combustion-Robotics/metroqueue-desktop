# MetroQueue Desktop — plan (agreed with user)
Base: fork of SimoHypers/limusic (Rust + Tauri 2, SvelteKit UI, SQLite, libmpv, GPL-3), rebranded "MetroQueue Desktop"
in the repo Combustion-Robotics/metroqueue-desktop. Credit Limusic + Metrolist; keep the upstream remote for regular merges.
No Listen Together work.

## Phase 0: fork + CI
- Create the repo (fork or mirror), rebrand: name, icons, app id, updater pointing at our releases.
- GitHub Actions: Linux AppImage, Windows setup.exe, macOS .app. Reuse Limusic's workflow; dev → pre-release, main → release.

## Phase 1: Play on desktop (receiver)
- A Rust module in the desktop app implementing MetroQueue's TV protocol exactly. Read MetroQueue playback/tv/*:
  - TvProtocol, TvHost;
  - the RemoteControlServer endpoints;
  - mDNS `_metroqueue._tcp`, 6-digit pairing, hashed tokens, the WebSocket push of state/queue/position;
  - the commands (play/pause/seek/skip/volume/play queue at index+position/play next/add/radio/reorder/remove).
- Map these onto the existing orchestrator/player. UI: pairing code dialog, Paired phones list (forget).
- MetroQueue side: the device picker shows a desktop icon/type (a "device kind" field in the mDNS TXT record / hello). Handoff both ways like TV.

## Phase 2: Two-way sync of MetroQueue-only data
- Over the paired connection (token).
- Data:
  - local play history (events with timestamps, dedup by id+time);
  - custom transitions (mix_transitions.json);
  - custom vibes (vibes.json);
  - shared settings (Mix mode/style/duration, crossfade on skip, lyrics options, theme accent/pure black — a whitelist).
- Per-item updatedAt + tombstones; last-writer-wins; history is a union. A manual "Sync now" plus automatic sync on connect.
- A shared JSON schema doc in both repos (versioned).

## Phase 3: Mix on desktop
- Port crossfade + transition styles + Smart mix + custom transitions to the desktop app.
- Two mpv decks (or mpv lavfi acrossfade), pre-buffering the next track, the MetroQueue planner logic ported to Rust (SmartMixPlanner, CustomTransitionPlanner, MixAnalyzer tempo/key/structure from decoded audio — reuse ideas, or decode via mpv/ffmpeg).
- Settings UI matching MetroQueue's Mix section.

## Phase 4: Vibe mode on desktop
- Port the pixel-art scene engine (MetroQueue ui/vibe/scene/SceneRenderer.kt) to TypeScript Canvas in the Svelte UI, with the same SceneSpec JSON so synced vibes render identically.
- Vibes page + full-screen vibe player with the small song bar.

## Parallelism
- Phase 0 first.
- Then Phase 1 (desktop receiver) ‖ the MetroQueue-side picker/device-kind ‖ the sync schema.
- Phases 3 and 4 in parallel after 1–2.

## Environment note (2026-11-04)
The container has full network access (dl.google.com, Maven Central OK).
Agents should build and test locally (./gradlew :app:assembleFossDebug, unit tests) before pushing, not rely on CI only.

## Status (2026-11-04)
- Phase 0 done: rebrand + CI on main/dev (v1.0.0, dev pre-releases). Builds run unsigned until the TAURI_SIGNING_PRIVATE_KEY secret is set (updater signing).
- MetroQueue phone side done (MetroQueue dev 7d4c538, 71ac485):
  - `kind=desktop` in mDNS TXT and /api/info;
  - device picker with a computer icon;
  - GET/POST /api/sync, v1, see SYNC.md.
- Next: Phase 1+2 desktop receiver (not started; branch feat/receiver was created from upstream/master but is empty — rebase it on dev). Then Phases 3 (Mix) and 4 (Vibe) in parallel.
