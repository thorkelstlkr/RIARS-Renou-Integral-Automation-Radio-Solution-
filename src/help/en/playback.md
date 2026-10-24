---
id: playback
title: Playback
section: onair
order: 3
keywords: play, pause, stop, next, volume, seek, keyboard
---

# Playback

Playback controls are in the header and in the **Now Playing** card. They work the same whether a person or the automation loaded the track.

## Transport

- **Play** starts or resumes the current item.
- **Pause** holds it at the current position; **Play** resumes.
- **Stop** stops playback and resets the position.
- **Next** jumps to the next item in the playlist.

The **On air bar** calls the current item **On air**, or **Paused** when paused. The **Now Playing** card shows **ON AIR**, **PAUSED** or **STOPPED**, the track title and a progress bar.

## Volume and seek

Drag the **Vol** slider to set the output level; the percentage is shown next to it. The setting is saved with the session and restored on the next start unless the server started with `--no-resume`.

To seek, click anywhere on the progress bar in **Now Playing**. RIARS moves the playhead to that point and keeps playing. The bar is only active when a track with a known duration is loaded.

## Keyboard

In the desktop app: **F5** Play, **F6** Pause, **F7** Stop, **F8** Next track. **Ctrl+Q** quits. The full list is in [Keyboard shortcuts](topic:shortcuts).

## What happens at the end of a track

By default the engine plays items in order. At the end of the list it stops, unless the automation is adding more items or you turned on repeat-by-hand. Before an item ends, the next one is prepared so the change is immediate; with crossfade on, the two overlap. Seamless changes are covered in [Crossfade](topic:crossfade).

> Tip: if the station falls silent while an item is playing, the **Silence detector** may skip it. See [Silence detector](topic:dead-air).
