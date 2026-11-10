# RIARS — Renou Integral Automation Radio Solution

Open-source radio station automation with native AI agent support.

## Architecture

Rust workspace with eight members:

- **riars-core** (`crates/riars-core/`) — Platform-independent audio engine, playlist management, scheduling, automation, analysis, live broadcasting, and media library. No GUI and no network. It does local I/O only: sound cards (cpal), the SQLite library database and the settings files next to it.
- **riars-stream** (`crates/riars-stream/`) — Icecast source client and MP3 encoder. riars-core holds only the streaming settings (`riars-core/src/streaming/icecast.rs`).
- **riars-meta** (`crates/riars-meta/`) — Metadata lookups over HTTP (MusicBrainz, AcoustID, Cover Art Archive).
- **riars-rds** (`crates/riars-rds/`) — Sends RDS to external encoders (TCP, UDP, serial, file) and opens the MPX output.
- **riars-api** (`crates/riars-api/`) — REST + WebSocket API server (axum), plus the web control panel at `/`. AI agents and remote clients control the radio through this.
- **riars-mcp** (`crates/riars-mcp/`) — Model Context Protocol server over stdio. Claude and other AI agents connect here to manage programming, playlists, and streaming with structured tool calls.
- **riars-server** (`crates/riars-server/`) — Headless binary for servers and Raspberry Pi. One process runs either the REST API (the default) or the MCP server (`--mcp-stdio`), never both: in MCP mode there is no HTTP server, WebSocket or web panel.
- **src-tauri/** (package `riars-desktop`) — Tauri v2 desktop app (Linux, Windows, macOS). Wraps riars-core, riars-stream, riars-rds and riars-meta with a React/TypeScript UI. It does not include riars-api, so the desktop app has no REST or MCP surface. An Android app is planned; there is no Android project in the repository yet.

There are three hosts: the API server, the MCP server and the desktop app. Each one owns its own 250 ms control loop (auto-advance, session save, stream title, RDS update, automation tick) and its own scheduler thread: `riars-api/src/lib.rs`, `riars-mcp/src/lib.rs` and `src-tauri/src/lib.rs`. A behaviour added to one loop must go to all three, or into riars-core.

Frontend: React 19 + TypeScript + Vite in `src/`.

## Platform targets

| Platform | Binary | GUI |
|----------|--------|-----|
| Linux x86_64 | riars-server, Tauri app | Optional |
| Windows x86_64 | Tauri app | Yes |
| macOS (Intel + Apple Silicon) | Tauri app | Yes |
| Raspberry Pi (aarch64 and armv7) | riars-server | Headless |
| Android | Planned, not started | — |

## Development

```bash
# Desktop app (requires Rust + Node.js)
npm install
npm run tauri dev

# Headless server (Rust only — works on Raspberry Pi)
cargo run -p riars-server

# Run tests (CI adds `--exclude riars-desktop`: on Linux the desktop crate needs WebKitGTK)
cargo test --workspace
```

## AI Agent Integration

Agents connect via two protocols:

1. **REST API** (riars-api) — HTTP endpoints at `http://host:3100/api/v1/...`. Every request needs `Authorization: Bearer <token>` (`--api-token` or `RIARS_API_TOKEN`; generated once and saved in `riars_api_token` if unset). It listens on 127.0.0.1 unless `--bind` says otherwise. Paths sent by API and MCP clients must lie in a library folder or a `--media-root` (`MediaLibrary::check_media_path`); an external cue detector cannot be set over the API. The WebSocket at `/ws/events` also accepts `?token=`, because browsers cannot set headers there; it sends the engine state when it changes and typed messages (`"type": "automation"`, `"mic"`, `"cart"`). The page of the web panel at `/` is served without a token; everything it does goes through the API with the token. The API is plain HTTP: with a non-loopback `--bind`, put a TLS proxy in front of it. Errors are 4xx/5xx with `{"error": "..."}`.
2. **MCP Server** (riars-mcp) — stdio transport (`riars-server --mcp-stdio`) for Claude and compatible agents

Both cover playback control, playlist management, library search, scheduling, streaming, automation, analysis, live, RDS and status monitoring. They are not identical: WebSocket events, editing cart banks and the microphone settings are REST only.

The files described below as "next to the library database" follow `--db-path` (default `riars_library.db`). riars-server and the desktop app take the lock `riars.lock` there (`riars-core/src/instance.rs`) and refuse to start when another process holds it: one process per database. The session (playlist, position, volume, crossfade, streaming) is saved there as `riars_session.json` and resumed on start unless `--no-resume` is given.

Completing missing track metadata (MusicBrainz, optional AcoustID fingerprints, Cover Art Archive) lives in `crates/riars-meta`, the network side; `riars-core/src/library/metadata.rs` holds the offline part (fingerprint, applying changes, writing tags). A lookup only proposes; nothing changes until a client applies what the operator accepts. MusicBrainz allows one request per second per IP: send every request through the gates in `riars-meta/src/http.rs`.

Long-term automation (categories, format clocks, dayparts grid, playlist generator, proof-of-play log) lives in `riars-core/src/automation/` and is described in `docs/automation.md`; its data is in the library database. Proof-of-play lines older than `AutomationConfig.log_keep_months` (13 by default, 0 keeps them all) are deleted at start and once a day.

Playlist tabs (`/api/v1/playlist/tabs`, MCP `list_playlist_tabs` etc.) are lists open for editing next to the one on air; only the on-air tab plays (`riars-core/src/playlist/tabs.rs`, stored in the library database by `riars-core/src/library/tabs.rs`). Putting a tab on air never cuts: the track playing ends first.

RDS for external encoders (`/api/v1/rds`, MCP `get_rds` etc.): `riars-core/src/rds/` makes the PS, RadioText and RT+ from what is on air and holds the settings; `crates/riars-rds` sends them (UECP or ASCII templates over TCP, UDP, serial or a `.txt` file) on its own thread. Settings are in `riars_rds.json` next to the library database. The built-in FM stereo encoder (`riars-core/src/audio/mpx.rs`, settings `RdsConfig::mpx`) turns a second tap of the on-air output into an MPX signal (pilot, 38 kHz, RDS groups from `rds/groups.rs` on 57 kHz) on a sound card at 4× the engine rate; the RDS worker opens it. `rds/decode.rs` is a test-only decoder, written independently of the encoder, that checks it.

The schedule (`/api/v1/schedule`, MCP `add_scheduled_event` etc.) is saved as `riars_schedule.json` next to the library database and runs in all three hosts (`riars_core::automation::scheduler_loop`), with second precision; one-shot `Time` events disable themselves after firing.

Cue points and loudness (`/api/v1/analysis`, `/api/v1/tracks/{id}/cues`, MCP `get_track_cues` etc.) live in `riars-core/src/analysis/`: intro and outro detection, EBU R128 loudness for normalization (with own targets for spots and imaging, and ReplayGain tags for tracks not analysed yet: `riars-core/src/audio/normalize.rs`), and batch analysis of the library. The plan is in `docs/intro-outro.md`.

Live broadcasting (`/api/v1/live`, MCP `get_live_status` etc.) lives in `riars-core/src/live/` and `riars-core/src/audio/live.rs`: the microphone channel, the cart banks and the play modes (automatic, assisted, manual). Opening the microphone over the API or MCP is refused unless `allow_remote` is set in the microphone settings; it is off by default. The plan is in `docs/live.md`.

## User manual

The in-app manual (Help ▸ Manual, F1) lives in `src/help/<lang>/<topic>.md`, one Markdown file per topic with front matter (`id`, `title`, `section`, `order`, `keywords`). English and Spanish are complete; other languages fall back to English topic by topic. When a change alters what users see or do, update the matching topics in `en` and `es`. Topic links are written `[text](topic:<id>)`.

The same topics are published on the website as static pages (`website/manual/` in Spanish, `website/en/manual/` in English, plus `search.json` and the sitemap entries). They are generated, not edited: after changing `src/help/`, run `npm run manual` and commit the result. CI fails (`--check`) when they are out of date or a `topic:` link is broken.

## Conventions

- Audio processing code in riars-core must be real-time safe: no allocations, no locks, no I/O in the audio callback
- Between the control threads and the audio thread: a bounded crossbeam channel for commands, atomics for settings and levels, and `rtrb` ring buffers for samples
- Types that cross the JSON boundary (REST, MCP, Tauri) implement Serialize, and also Deserialize when a client can send them. Response-only types such as `EngineStateInfo` and `PlaybackState` implement Serialize only
- riars-core has zero network dependencies — the HTTP listener lives in riars-api; outbound networking lives in riars-stream (Icecast), riars-rds (RDS encoders) and riars-meta (metadata lookups). riars-mcp is stdio only
- Tauri commands delegate to riars-core; they should hold no business logic. Validation shared by REST, MCP and the desktop app belongs in riars-core, as `riars-core/src/automation/validate.rs` does. A check a second host needs goes into riars-core first (as `audio::crossfade::validate_crossfade_ms`); do not copy it
