---
id: web-panel
title: Web panel
section: remote
order: 1
keywords: web panel, browser, token, phone, remote, automation card
---

# Web panel

`riars-server` serves a small control panel in the browser. Open `http://YOUR-SERVER:3100/`, replacing `YOUR-SERVER` with the machine's name or IP address. It is designed to work on a phone as well as on a computer. The desktop app does not serve this panel.

## Signing in

The panel asks for the API token. Use the token given with `--api-token`, the value of `RIARS_API_TOKEN`, or the random token printed in the server log at startup. The browser remembers it on that device; use the token only on trusted devices. A wrong token shows **Wrong token**.

## What you can do

The panel has these cards:

- **Player:** the current track, a progress bar you can click to seek, **Play**, **Pause**, **Stop**, **Next** and a volume slider.
- **Playlist:** the current list with the item on air marked.
- **Library:** a search box and a **+** button to add a track to the playlist.
- **Automation:** the **ON**/**OFF** toggle, the **Clock** or **Playlist** and **Period** being prepared, the **Rule**, recent **Warnings**, and **Coming up** for the next hour.
- **Today's advertising:** **Advertiser**, **Campaign**, **Target**, **Aired** and **Missed**, with missed figures in red.
- **Downloads:** **Play log (CSV)**, **Delivery (CSV)** and **Spot log (CSV)**.

The header shows the streaming state and a **SILENCE** badge when the dead-air detector is active. If the connection drops, the panel says **Connection lost, retrying…** and reconnects by itself.

> Note: **Coming up** is a preview. It says so: **Preview only. The Playlist card shows what is really on air.**

The panel is for control and monitoring; full editing of clocks, the grid, campaigns and settings is in the desktop app and the API. See [Headless server](topic:headless-server), [API and AI agents](topic:api-agents) and [Streaming](topic:streaming).
