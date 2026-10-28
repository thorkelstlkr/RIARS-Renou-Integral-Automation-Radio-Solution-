---
id: intro-outro
title: Intros, outros and talking over the intro
section: automation
order: 8
keywords: intro, outro, talk over, voice, marks, cue, cue points, jingle over, ducking, analysis
---

# Intros, outros and talking over the intro

Radio stations often **talk over the intro**: a jingle or station ID plays over the instrumental start of a song and ends just before the singing begins. RIARS does this on its own, **and never over the voice**.

## The marks of each song

| Mark | Meaning |
|---|---|
| **Cue in** | First moment with sound (leading silence skipped). |
| **Voice starts** | End of the instrumental intro. |
| **Voice ends** | Start of the instrumental outro. |
| **Cue out** | Last moment with sound. |

The **Intro** and **Outro** columns of the [playlist](topic:playlist) show how many seconds can be talked over at the start and at the end.

## Detecting them

- **One song**: the **✎** button in the library, or right-click ▸ **Marks…**. The editor opens with the waveform.
- **The whole library or a category**: in **Categories**, the **Track analysis** box, button **Analyse library** or **Analyse this category**. It runs in the background, one song after another with pauses, so on-air playback keeps its CPU. Each song is analysed only once.

Detection looks for the voice by its energy in the vocal band, the rhythm of syllables and its tone. **It is approximate**: it suggests, and you confirm or correct. Each automatic mark has a **confidence** value; below the minimum (0.35 by default) it is not used for talking over.

## Correcting them by hand

In the editor:

1. Drag the marks on the waveform or type the times. The arrow keys move a mark 0.1 s; Shift + arrows, 1 s.
2. The coloured strip below shows, second by second, how much it sounds like a voice.
3. Listen on the [pre-listen card](topic:sound-cards): **▶ Last 5 s of intro** checks that the mark falls just before the voice.
4. Press **Save**. Hand-set marks are **never** replaced by a later analysis.

If the song has no intro, tick **No intro**.

## Detection with an AI agent

In the **Track analysis** box you can choose another detector:

- **External program**: a program that separates the voice with AI (for example Demucs or Spleeter with a small script). RIARS passes it each file and reads the result. It is more accurate but uses more computer power. A program that takes longer than two minutes on one file is stopped.
- **Agent over MCP**: an agent such as Claude can review the doubtful songs with `list_tracks_needing_review`, read the voice line of each one with `analyze_track` and set the marks with `set_track_cues`. They are saved with source *Agent*. An agent cannot replace marks you set by hand. See [API and AI agents](topic:api-agents).

## How the automation talks over

In **Station settings ▸ Talk over intros and outros**:

| Setting | Default |
|---|---|
| Enabled, over intros, over outros | Yes |
| Margin before the voice | 500 ms |
| Lower the song (*ducking*) | 6 dB |
| Minimum confidence of automatic marks | 0.35 |
| Kinds that may talk over | Jingle, station ID, sweeper, bumper |

When a jingle is due, RIARS looks at the next song. If the whole jingle fits in the intro with the margin, it plays over it, placed to **end just before the voice**. While it plays, the song is lowered a few dB: quickly on the way in and gently on the way out. If it does not fit, it tries the outro of the previous song. If that does not fit either, it plays before the song, as usual.

Spots never talk over, and neither do hard fixed times. What was played over is in the [proof-of-play log](topic:proof-of-play) with its real time.
