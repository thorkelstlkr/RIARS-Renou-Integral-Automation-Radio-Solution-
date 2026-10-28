---
id: streaming
title: Streaming
section: onair
order: 6
keywords: streaming, Icecast, SHOUTcast, MP3, bitrate, metadata, mount
---

# Streaming

RIARS can send the output of the station to an **Icecast** server as an MP3 stream. Listeners then connect to the Icecast server, not to RIARS directly. The desktop app needs an audio output device. The headless server can run on its internal clock with `--no-sound-card`, and streaming still works because RIARS encodes the mixed output; see [Headless server](topic:headless-server).

## Configure and connect

In the desktop app, click the streaming badge or use **Broadcast › Streaming settings…**. In the browser panel of `riars-server`, use the stream badge. Fill in:

| Field | Meaning |
|---|---|
| **Server** | Icecast host name or IP address |
| **Port** | Icecast port, usually 8000 |
| **Mount point** | Stream path, for example `/stream` |
| **Bitrate** | 64, 96, 128, 160, 192, 256 or 320 kbps |
| **User** and **Password** | Icecast source credentials |
| **Station name** and **Genre** | Information sent to the server |

The defaults are `localhost`, port `8000`, mount `/stream`, user `source`, 128 kbps and station name **RIARS**. The format is MP3. Click **Connect** to start; the badge shows **CONNECTING**, then **STREAMING**. **Disconnect** stops the stream.

## Status and metadata

The badge can also show **OFFLINE**, **STREAM ERROR** or **NO AUDIO**. When connected, the panel shows the URL and the amount of data sent. The current song title is sent to Icecast automatically as **Artist - Title** when the track changes, and is sent again after a reconnection. The password is not saved between sessions; the other settings are remembered.

## Notes

- RIARS reconnects on its own if the connection drops.
- A mount point can only have one source at a time. If Icecast refuses the mount, another encoder may be using it.
- On the server, streaming settings can be resumed from the saved session unless it started with `--no-resume`.

See [Web panel](topic:web-panel) for remote control and [Troubleshooting](topic:troubleshooting) if the stream will not connect.
