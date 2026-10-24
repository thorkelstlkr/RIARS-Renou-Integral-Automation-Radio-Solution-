---
id: playlist
title: Playlist
section: onair
order: 2
keywords: playlist, start time, reorder, save, open, generated, kinds
---

# Playlist

The **Playlist** tab is the running order of the station. It shows what has played, what is on air and what comes next, including items the automation adds.

## What the columns mean

| Column | Meaning |
|---|---|
| **#** | Position in the list |
| **Start** | Estimated start time, based on the position of the track on air |
| **Title** | Track title, with a coloured dot for its kind |
| **Artist** | Artist, or **Unknown** |
| **Duration** | Length of the item |

The dot colour follows the item kind: music, **Jingle**, **Bumper**, **Station ID**, **Sweeper**, **Promo**, **Spot**, **Programme**, **Music bed** or **Other**. Hover over the dot to see the kind and whether the item was added by the automation. Start times move as the current track is seeked, paused or crossfaded.

## Reorder and edit

Use the arrows on each row for **Move up** and **Move down**, the cross for **Remove**, or **Clear** to empty the list. **Double-click** a row to jump to it and start playing.

Items added by the automation appear at the end. You can move or remove them; the generator respects what you leave and only fills the end of the list. It does not rewrite the items already there.

## Save and open

Click **Save**, type a **Playlist name** and confirm. Saved playlists can be opened later with **Open**; the list shows their name, number of items and total duration, and a cross deletes them. Saved playlists can also be used as a source in clocks and grid rules.

**Ctrl+S** saves the current playlist and **Ctrl+O** opens a saved one.

## Length

The footer shows the number of items and the total duration. A dash (`--:--`) means the duration is not known yet.

> Note: changing the playlist while the automation is on is allowed. The generator will fill the new end of the list on its next pass. See [Automation overview](topic:automation-overview) and [Playback](topic:playback).
