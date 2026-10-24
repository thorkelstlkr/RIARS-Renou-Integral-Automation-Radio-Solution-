---
id: headless-server
title: Headless server
section: remote
order: 2
keywords: server, Raspberry Pi, flags, port, api token, db path, data folder, SD card
---

# Headless server

`riars-server` runs RIARS without a window on Linux, including a Raspberry Pi. It hosts the REST API, the WebSocket and the browser panel, and can run the MCP server for AI agents.

## Starting it

From a terminal:

```bash
riars-server --port 3100 --api-token MY_SECRET_TOKEN
```

Then open `http://YOUR-SERVER:3100/` in a browser and enter the token. If you do not give a token, RIARS generates a random one and prints it in the log at startup.

## Flags

| Flag | Default | Purpose |
|---|---|---|
| `--port` | 3100 | Port for the API, WebSocket and panel |
| `--api-token` | from `RIARS_API_TOKEN`, else random | Token required by the API |
| `--db-path` | `riars_library.db` | Path to the library database |
| `--scan` | none | Folder to scan at startup |
| `--no-sound-card` | off | Play on the internal clock, for servers that only stream |
| `--no-resume` | off | Start fresh instead of restoring the last session |
| `--mcp-stdio` | off | Run as an MCP server on standard input/output |
| `--lang` | `en` | Messages: `en`, `es`, `fr`, `de`, `pt` or `ca` |

`--no-resume` ignores the saved playlist, position, volume, crossfade and streaming. Without it, the server continues where it stopped.

## Data files

The library database holds the tracks, categories, clocks, grid, campaigns and proof-of-play log. Next to it live `riars_schedule.json` (scheduled events) and `riars_session.json` (the last session). With `--db-path /srv/riars/riars_library.db`, all three are in `/srv/riars/`. The desktop app uses its per-user data folder; set `RIARS_DATA_DIR` to move that folder.

## Raspberry Pi and SD cards

Use a good power supply and shut the machine down cleanly. SQLite runs in WAL mode to reduce writes, but a power cut can still corrupt a card. Keep backups of the data folder and export the CSV reports regularly. `--no-sound-card` is intended for a Pi that only streams; without it, RIARS tries to use a sound device.

See [Web panel](topic:web-panel), [API and AI agents](topic:api-agents) and [Scheduled events](topic:events).
