# RIARS — Renou Integral Automation Radio Solution

Open-source radio station automation with native AI agent support.

## Architecture

Rust workspace with five crates:

- **riars-core** (`crates/riars-core/`) — Platform-independent audio engine, playlist management, scheduling, streaming, and media library. No GUI, no network — pure logic.
- **riars-api** (`crates/riars-api/`) — REST + WebSocket API server (axum). AI agents and remote clients control the radio through this.
- **riars-mcp** (`crates/riars-mcp/`) — Model Context Protocol server. Claude and other AI agents connect here to manage programming, playlists, and streaming with structured tool calls.
- **riars-server** (`crates/riars-server/`) — Headless binary for servers and Raspberry Pi. Runs riars-core + riars-api + riars-mcp without any GUI.
- **src-tauri/** — Tauri v2 desktop app (Linux, Windows, macOS) and Android app. Wraps riars-core with a React/TypeScript UI.

Frontend: React 19 + TypeScript + Vite in `src/`.

## Platform targets

| Platform | Binary | GUI |
|----------|--------|-----|
| Linux x86_64 | riars-server, Tauri app | Optional |
| Windows x86_64 | Tauri app | Yes |
| macOS (Intel + Apple Silicon) | Tauri app | Yes |
| Raspberry Pi (aarch64) | riars-server | Headless |
| Android | Tauri mobile app | Yes |

## Development

```bash
# Desktop app (requires Rust + Node.js)
npm install
npm run tauri dev

# Headless server (Rust only — works on Raspberry Pi)
cargo run -p riars-server

# Run tests
cargo test --workspace
```

## AI Agent Integration

Agents connect via two protocols:

1. **REST API** (riars-api) — HTTP endpoints at `http://host:3100/api/v1/...`. Every request needs `Authorization: Bearer <token>` (`--api-token` or `RIARS_API_TOKEN`; generated once and saved in `riars_api_token` if unset). It listens on 127.0.0.1 unless `--bind` says otherwise. Paths sent by API and MCP clients must lie in a library folder or a `--media-root` (`MediaLibrary::check_media_path`); an external cue detector cannot be set over the API. The WebSocket at `/ws/events` takes `?token=` instead. Errors are 4xx/5xx with `{"error": "..."}`.
2. **MCP Server** (riars-mcp) — stdio transport (`riars-server --mcp-stdio`) for Claude and compatible agents

Both expose the same capabilities: playback control, playlist management, library search, scheduling, streaming, and status monitoring.

Completing missing track metadata (MusicBrainz, optional AcoustID fingerprints, Cover Art Archive) lives in `crates/riars-meta`, the network side; `riars-core/src/library/metadata.rs` holds the offline part (fingerprint, applying changes, writing tags). A lookup only proposes; nothing changes until a client applies what the operator accepts. MusicBrainz allows one request per second per IP: send every request through the gates in `riars-meta/src/http.rs`.

Long-term automation (categories, format clocks, dayparts grid, playlist generator, proof-of-play log) lives in `riars-core/src/automation/` and is described in `docs/automation.md`; its data is in the library database.

Playlist tabs (`/api/v1/playlist/tabs`, MCP `list_playlist_tabs` etc.) are lists open for editing next to the one on air; only the on-air tab plays (`riars-core/src/playlist/tabs.rs`, stored in the library database). Putting a tab on air never cuts: the track playing ends first.

RDS for external encoders (`/api/v1/rds`, MCP `get_rds` etc.): `riars-core/src/rds/` makes the PS, RadioText and RT+ from what is on air and holds the settings; `crates/riars-rds` sends them (UECP or ASCII templates over TCP, UDP, serial or a `.txt` file) on its own thread. Settings are in `riars_rds.json` next to the library database. Generating the FM/MPX signal itself is not done.

The schedule (`/api/v1/schedule`, MCP `add_scheduled_event` etc.) is saved as `riars_schedule.json` next to the library database and runs in riars-server, with second precision; one-shot `Time` events disable themselves after firing.

## User manual

The in-app manual (Help ▸ Manual, F1) lives in `src/help/<lang>/<topic>.md`, one Markdown file per topic with front matter (`id`, `title`, `section`, `order`, `keywords`). English and Spanish are complete; other languages fall back to English topic by topic. When a change alters what users see or do, update the matching topics in `en` and `es`. Topic links are written `[text](topic:<id>)`.

## Conventions

- Audio processing code in riars-core must be real-time safe: no allocations, no locks, no I/O in the audio callback
- Use channels (crossbeam or std::sync::mpsc) between UI thread and audio thread
- All public API types implement Serialize + Deserialize for JSON interop
- riars-core has zero network dependencies — networking lives in riars-api and riars-mcp
- Tauri commands delegate to riars-core; they hold no business logic
