---
id: live
title: Live broadcasting
section: onair
order: 9
keywords: live, microphone, mic, sound bank, cartwall, jingles, effects, manual mode, assisted, on air, presenter, DJ, echo, direct monitoring
---

# Live broadcasting

The **On air** tab (**Ctrl+8**) brings together what a presenter needs: the **microphone**, the **sound bank** and the **mode** the playlist moves on in.

## Microphone

- The **MIC** button (or **F9** from any tab) opens and closes it, with a short fade so no click is heard. While it is open the button turns red and **ON AIR** shows.
- When it opens, the music **goes down by itself** (12 dB by default) and comes back when it closes. To lower it only while you speak, choose **While speaking** in the settings.
- The meter shows the level even with the microphone closed, so you can check your voice before going on air. Keep the peaks in the green or yellow.
- If the microphone's card fails or is unplugged, the microphone falls silent and the error shows in red. The broadcast goes on.

**Playback ▸ Microphone…** (or the ⚙ button):

| Setting | Default |
|---|---|
| Input card | The system one |
| The sound card already plays the microphone (direct monitoring) | No |
| Gain | 0 dB |
| Low cut (removes pops and hum below 80 Hz) | Yes |
| Noise gate (silences room noise between phrases) | No |
| Compressor (evens out loud and quiet speech) | Yes |
| Lower the music | 12 dB, while open |
| Let the API and AI agents open it | **No** |

RIARS does not send the microphone to the [pre-listen](topic:sound-cards) headphones: to hear yourself, use the mixing desk or your audio interface's direct monitoring.

### If you hear an echo

Many audio interfaces (a Roland UA-4FX, for example) send the microphone input straight to their output, with no delay. If RIARS plays it on that card as well, you hear yourself twice: the second copy arrives a fraction of a second late and sounds like an echo. If the card output is your on-air signal, listeners hear it too.

Tick **The sound card already plays the microphone (direct monitoring)**. RIARS then stops playing the microphone on the card, which already does so with no delay, and adds it only to the [Internet stream](topic:streaming). The music still ducks when the microphone opens and the meter still works. If your interface has no direct monitoring, leave the box unticked; otherwise the microphone would not reach the card at all.

## Sound bank

A grid of **6 × 4 buttons** with **8 pages**, for jingles, effects, sweepers and music beds. You can keep several banks, one per show for example, and pick the active one at the top.

- **Assign a sound:** drag a track from the library onto a button, or click an empty button and search for it.
- **Fire:** click the button, or keys **1–0** and **Q–P** for the first twenty buttons of the visible page. **Page Up / Page Down** change page. **Esc** or **Stop all** stops every bank sound.
- Each button shows the time it has left while it plays. Up to **8** can play at once, over the music, and with the playlist stopped too.
- **Right-click** a button: **Edit…** (label, colour, gain, mode and whether it lowers the music), **Pre-listen** and **Clear**.

| Button mode | What it does |
|---|---|
| **Fire** | Every press starts it from the top. |
| **Toggle** | One press starts it, the next stops it. |
| **Loop** | It repeats until you stop it. Handy for music beds. |

Short sounds (under 30 s) are loaded into memory when their page opens, so they play at once. Everything played from the bank goes into the [proof-of-play log](topic:proof-of-play).

## Playlist mode

| Mode | What it does |
|---|---|
| **Automatic** | The playlist moves on by itself and the [automation](topic:automation-overview) fills it. |
| **Assisted** | The playlist moves on by itself, but the automation **adds nothing and makes no cuts**: what you prepared plays. |
| **Manual** | When each item ends the playlist **stops** and waits for **Play** (F5) or **Next** (F8). |

In any mode, **right-click ▸ Stop after this item** in the [playlist](topic:playlist) stops the broadcast when that item ends. Handy to come in and talk. The item shows ⏹.

## Talking over the intro

When a song with [intro marks](topic:intro-outro) starts, the panel shows in green the **seconds left before the voice comes in**; the last three in yellow. You can introduce the song and stop talking just in time.
