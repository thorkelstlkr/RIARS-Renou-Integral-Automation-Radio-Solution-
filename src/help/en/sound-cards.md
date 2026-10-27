---
id: sound-cards
title: Sound cards and pre-listening
section: onair
order: 8
keywords: sound card, output, device, pre-listen, cue, pfl, headphones, listen
---

# Sound cards and pre-listening

RIARS can use two sound cards: one for **on air** (what the station sends out) and one for **pre-listening** (hearing something on headphones without it going on air).

## Choosing them

**Playback ▸ Sound cards…**

- **On air**: the card the broadcast goes out on. By default, the system one. The change applies when RIARS **restarts**, so the broadcast is not cut.
- **Pre-listen**: the headphones card, or *Off*. It applies at once.

> Warning: if you choose the on-air card for pre-listening too, what you listen to **will be heard on air**. The dialog warns you in yellow.

Pre-listening has its own audio engine, separate from the on-air one: a failure in it cannot affect the broadcast. If the pre-listen card goes away, for example when USB headphones are unplugged, it simply stops playing.

## Pre-listening

- In the library: right-click a track ▸ **Pre-listen**.
- In the [cue-point editor](topic:intro-outro): the **▶** buttons play the start, the end of the intro or the end of the voice.

Pre-listening plays at the level the track would have on air, with [normalisation](topic:normalization) applied.

## On the headless server

`riars-server --list-devices` lists the sound cards, and `--device "<name>"` chooses the on-air one. The server has no pre-listening. See [Headless server](topic:headless-server).
