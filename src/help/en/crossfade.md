---
id: crossfade
title: Crossfade
section: onair
order: 4
keywords: crossfade, fade, curve, overlap, spot, no overlap
---

# Crossfade

Crossfade makes one item fade out while the next fades in. RIARS uses it to join tracks smoothly instead of leaving a hard cut or a gap.

## Defaults

Crossfade is on by default, lasts **3 seconds** and uses the **Equal Power** curve. You can change the length, choose a curve and turn it off from **Playback › Crossfade…** in the desktop menu. The same settings are available through the API.

The three curves are:

| Curve | Character |
|---|---|
| **Linear** | Simple, even fade; good for similar material |
| **Equal Power** | Keeps the perceived loudness steady through the join |
| **S-Curve** | Gentle start and end; a smooth, musical transition |

## Spots and other protected kinds

By default, **Spot** items never crossfade: a commercial starts and ends cleanly, without overlapping the music or the next spot. This is the **Kinds that never crossfade** setting in **Station settings**; you can add other kinds there, for example **Station ID** or **Jingle**.

The generator marks each item according to that setting, and the audio engine skips the fade on both sides of it. This is why ad breaks sound separated from the music.

## Where the time comes from

The start times in the **Playlist** and the length of a generated hour take the crossfade into account. If you change the crossfade length, the plan for the next items follows the new value.

> Note: a crossfade is not a fix for very different levels. Set the volume and the item's own audio sensibly; the **VU Meter** warns with **CLIP** if the output reaches full scale.

See [Station settings](topic:station-settings) for the no-overlap kinds, and [Silence detector](topic:dead-air) for what happens when an item contains silence.
