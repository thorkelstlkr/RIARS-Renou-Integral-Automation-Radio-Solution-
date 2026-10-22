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

1. **REST API** (riars-api) — HTTP endpoints at `http://host:3100/api/v1/...`
2. **MCP Server** (riars-mcp) — stdio or SSE transport for Claude and compatible agents

Both expose the same capabilities: playback control, playlist management, library search, scheduling, streaming, and status monitoring.

## Conventions

- Audio processing code in riars-core must be real-time safe: no allocations, no locks, no I/O in the audio callback
- Use channels (crossbeam or std::sync::mpsc) between UI thread and audio thread
- All public API types implement Serialize + Deserialize for JSON interop
- riars-core has zero network dependencies — networking lives in riars-api and riars-mcp
- Tauri commands delegate to riars-core; they hold no business logic
