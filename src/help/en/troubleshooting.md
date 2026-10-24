---
id: troubleshooting
title: Troubleshooting
section: reference
order: 2
keywords: problem, no sound, silence, stream, automation, nothing, playlist stops
---

# Troubleshooting

## No sound

Check the **Vol** slider and the **Now Playing** state. If it says **STOPPED**, click **Play**. If a track is loaded but the meter stays still, the file may be empty or corrupt: the silence detector will eventually skip it. On a server, the machine may have no sound card; `--no-sound-card` is for streaming only, and the stream requires a working audio output or the internal clock setup described in [Headless server](topic:headless-server).

## Silence alarms

A red **SILENCE** badge or **SILENCE ON AIR** means the output stayed below the threshold for longer than the limit. The default is -50 dBFS for 10 seconds. If quiet material triggers false alarms, lower the threshold or increase the time; if a bad file causes them, the **Skip** action is doing its job. See [Silence detector](topic:dead-air).

## The stream will not connect

Check **Server**, **Port**, **Mount point**, **User** and **Password**. A mount point can have only one source: if another encoder is connected, Icecast refuses it. **STREAM ERROR** shows the server's message. **NO AUDIO** means RIARS has no audio output to stream. After changing settings, disconnect and connect again. See [Streaming](topic:streaming).

## Automation adds nothing

Work through these in order:

1. Is the **Automation** card **ON**?
2. Do the [categories](topic:categories) contain playable tracks, or does a smart filter match any?
3. Does the [clock](topic:clocks) have slots, and do they reference existing categories?
4. Does a [grid](topic:grid) rule cover this hour, or is a **Default clock** set in [Station settings](topic:station-settings)?
5. Look at the **Warnings** on the **Automation** card and in the **Log** pane. They name the category or clock that has nothing to play.
6. Is the **Fallback category** set? Without it, an empty category means the generator warns and adds nothing.

Remember that changes apply from the next generated item; what is already in the playlist is not rewritten. To see what should happen, use [Preview](topic:preview).

## The playlist stops

If the automation is off and the list ends, playback stops. Turn automation on, add more items, or load a saved playlist. If the automation is on but the list still stops, the generator is probably warning about an empty category or a missing clock: check the warnings as above. A hard anchor may also cut in earlier than expected. See [Playlist](topic:playlist) and [Automation overview](topic:automation-overview).
