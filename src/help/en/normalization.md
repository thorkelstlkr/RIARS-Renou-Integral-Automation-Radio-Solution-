---
id: normalization
title: Loudness normalisation
section: onair
order: 7
keywords: normalise, normalize, volume, loudness, lufs, ebu r128, level, replaygain, limiter
---

# Loudness normalisation

Every file is mastered at a different level: a recent song sounds much louder than one from the 80s, and a jingle can be very quiet. Normalisation evens out the **perceived loudness** so listeners do not have to reach for the volume.

## How it works

1. The analysis measures each track's **integrated loudness** to the **EBU R128** standard (in LUFS) and its peak. It is the same pass that finds [intros and outros](topic:intro-outro), and it is done once per track.
2. On air, each track gets one fixed gain to reach the **target**: −16 LUFS by default, usual for Internet radio.
3. A quiet track is raised only as far as its peak allows, and never more than the limit.
4. A **soft limiter** on the output prevents clipping when something was raised or when a jingle plays over a song. Tracks that were not raised pass untouched.

Tracks that have not been analysed play as they are.

## Settings

**Playback ▸ Normalisation…**

| Setting | Default |
|---|---|
| Enabled | Yes |
| Target | −16 LUFS |
| Most gain | 10 dB |
| Most cut | 12 dB |
| Peak ceiling | −1 dBFS |

The **LUFS** column of the [playlist](topic:playlist) shows each track's loudness. The cue-point editor shows the gain the track will get.

> Tip: analyse the whole library once (**Categories ▸ Track analysis ▸ Analyse library**). Whatever you add later can be analysed with the same button; it only processes what is new.
