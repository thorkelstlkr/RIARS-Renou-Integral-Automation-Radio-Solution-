---
id: dead-air
title: Silence detector
section: onair
order: 5
keywords: silence, dead air, detector, threshold, skip, alert
---

# Silence detector

The silence detector watches the output while a track is playing. If the signal stays below a level for too long, it reports dead air and can skip the item. It protects the station when a file is empty, corrupt or has a long silent lead.

## Defaults

| Setting | Default |
|---|---|
| Enabled | Yes |
| Threshold | **-50 dBFS** |
| Length | **10 seconds** |
| Action | **Skip** |

The level is measured before the volume control, so turning the studio monitors down does not count as silence. It is reset by sound or by stopping playback.

## What you see

When silence passes the limit, the **Now Playing** card shows a red **SILENCE** badge and the bottom bar changes to **SILENCE ON AIR**. The badge tooltip gives the number of incidents since the engine started. In the browser panel, the header shows **SILENCE** and the automation log may show a warning.

With the **Skip** action, RIARS jumps to the next playlist item and marks the skipped item with the reason **cut for silence** in the proof-of-play log. With **Alert**, it only raises the alarm and keeps playing.

## Configure it

In the desktop app, open **Playback › Silence detector…**. In the API, use `GET` and `POST /api/v1/player/dead-air` with `enabled`, `threshold_db` (from -90 to 0), `seconds` (1 to 600) and `action` (`Skip` or `Alert`). The MCP tool `set_dead_air` exposes the same fields.

> Tip: if music with very quiet passages triggers false alarms, lower the threshold (for example to -60 dBFS) or lengthen the time. Do not disable the detector on an unattended station.

See [Playback](topic:playback) and [Troubleshooting](topic:troubleshooting).
