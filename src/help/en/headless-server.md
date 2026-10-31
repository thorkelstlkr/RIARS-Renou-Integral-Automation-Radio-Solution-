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
riars-server --port 3100 --api-token MY_SECRET_TOKEN --bind 0.0.0.0
```

Then open `http://YOUR-SERVER:3100/` in a browser and enter the token. Without `--bind`, the server only accepts connections from the same machine; `--bind 0.0.0.0` opens it to the network. If you do not give a token, RIARS generates one the first time and saves it in `riars_api_token`, next to the database.

## Flags

| Flag | Default | Purpose |
|---|---|---|
| `--port` | 3100 | Port for the API, WebSocket and panel |
| `--bind` | `127.0.0.1` (this machine only) | Address the API listens on; `0.0.0.0` opens it to the network |
| `--api-token` | from `RIARS_API_TOKEN`, else generated and saved in `riars_api_token` | Token required by the API |
| `--media-root` | library folders only | Folder API and agent clients may play, add, scan and tag files from; repeatable. Library folders always count |
| `--stream-host` | any | Icecast server streaming may connect to, as `host` or `host:port`; repeatable |
| `--cors-origin` | none | Browser origin on another site allowed to call the API, e.g. `https://panel.example.com`; repeatable. The built-in panel needs none |
| `--db-path` | `riars_library.db` | Path to the library database |
| `--scan` | none | Folder to add to the library; every library folder is rescanned in the background at startup |
| `--no-sound-card` | off | Play on the internal clock, for servers that only stream |
| `--device` | system default | On-air sound card by name; if it is missing, the system default is used |
| `--list-devices` | — | List the sound cards and exit |
| `--no-resume` | off | Start fresh instead of restoring the last session |
| `--mcp-stdio` | off | Run as an MCP server on standard input/output |
| `--lang` | `en` | Messages: `en`, `es`, `fr`, `de`, `pt` or `ca` |

`--no-resume` ignores the saved playlist, position, volume, crossfade and streaming. Without it, the server continues where it stopped.

## Data files

The library database holds the tracks, categories, clocks, grid, campaigns and proof-of-play log. Next to it live `riars_schedule.json` (scheduled events), `riars_session.json` (the last session) and `riars_api_token` (the generated token). On Linux only the server's user can read the session and the token: the session holds the Icecast password. With `--db-path /srv/riars/riars_library.db`, all three are in `/srv/riars/`. The desktop app uses its per-user data folder; set `RIARS_DATA_DIR` to move that folder.

## Security

- The API is plain HTTP. To control the station from outside the local network, put an HTTPS proxy (such as Caddy or nginx) in front of it, or use a VPN.
- Whoever has the token controls the station: keep it like a password.
- The API and agents can only use files in the library folders and in the folders given with `--media-root`. Paths elsewhere are refused.
- An external intro detector program can only be chosen in the desktop app, never over the API.

## Raspberry Pi and SD cards

Use a good power supply and shut the machine down cleanly. SQLite runs in WAL mode to reduce writes, but a power cut can still corrupt a card. Keep backups of the data folder and export the CSV reports regularly. `--no-sound-card` is intended for a Pi that only streams; without it, RIARS tries to use a sound device.

See [Web panel](topic:web-panel), [API and AI agents](topic:api-agents) and [Scheduled events](topic:events).
