---
id: playlist
title: Playlist
section: onair
order: 2
keywords: playlist, start time, reorder, save, open, generated, kinds, drag, drop, folder, explorer
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
| **Album**, **Year** and **Genre** | The track's data in the [library](topic:library). A dash means the track is not in the library or lacks that field; [Complete metadata](topic:metadata) can fill them in. |
| **Duration** | Length of the item |

The dot colour follows the item kind: music, **Jingle**, **Bumper**, **Station ID**, **Sweeper**, **Promo**, **Spot**, **Programme**, **Music bed** or **Other**. Hover over the dot to see the kind and whether the item was added by the automation. Start times move as the current track is seeked, paused or crossfaded.

## Reorder and edit

Use the arrows on each row for **Move up** and **Move down** (or **Alt+Up / Alt+Down**), the cross for **Remove** (or **Delete**, also for several selected items), or **Clear** to empty the list. **Double-click** a row, or press **Enter**, to jump to it and start playing. **Ctrl+click**, **Shift+click** and **Ctrl+A** select several items.

**Right-click** an item for: Play now, Play next, Pre-listen, Marks…, Move up, Move down, Move to top, Move to bottom, Remove from playlist, Remove played items, Clear the playlist, Show in folder and Copy path.

## Tabs: several lists at once

There is a tab above the playlist for each open list, so you can prepare a programme's inserts in one tab while another plays.

- Only one list plays: the one with the **red dot** (on air). The others are edited the same way (dragging, right-click, marks) but play nothing: double-click and **Enter** pre-listen.
- **+** opens an empty list. **Double-click** a tab to rename it; **✕** closes it (asking first when it has items; save it first to keep it). The tab on air cannot be closed.
- **Add to playlist** in the library or Folders adds to the tab you are looking at. **Play next** always goes to the list on air.
- **Starts at**: the planned start time of a list that is not on air. The Start column counts from it, so you see when each item will play.
- **Put on air** (button, or right-click the tab): the track playing ends, then this list follows, with no cut. The list that was playing stays in its tab with what it had left, so putting it back on air carries on where it was.
- **Right-click** items: **Copy to** or **Move to** another tab and, from a list not on air, **Insert next on air**, right after the track playing.

Tabs are saved with every change and come back when RIARS starts.

## Add by dragging

You can build the playlist by dragging:

- **From File Explorer** (or your system's file manager): drag one or more audio files, or a whole folder, and drop them on the list.
- **From the [library](topic:library)**, in its tracks and folders tabs: drag a track onto the list.

While you drag, a **red line** shows where the items will go: above the row under the pointer or, in the lower half of the row, just below it. Drop below the last row to add them at the end. Near the top or bottom edge, the list scrolls by itself.

- Files go in in the order you drag them.
- A **folder** brings all its audio files, including those in its subfolders: first the folder's own files, by name, then each subfolder, also by name. Hidden and system folders are skipped, as in the library's folder tree.
- Anything that is not audio (pictures, text files…) is skipped. MP3, FLAC, WAV, OGG, AAC and M4A are supported.
- Files that are not in the library yet **are added to it** first and analysed in the background, so they get [intros and outros](topic:intro-outro) and [normalisation](topic:normalization) like the rest.
- With a big folder, the foot of the list shows the progress ("Adding to the library: 120 of 3000") and you can keep using the program. When it ends, it says how many were added and how many skipped. At most 10,000 files are added at once.

Items added by the automation appear at the end. You can move or remove them; the generator respects what you leave and only fills the end of the list. It does not rewrite the items already there.

## Save and open

Click **Save**, type a **Playlist name** and confirm: it saves the list of the tab you are looking at. **Open** opens a saved playlist in a new tab, leaving the list on air alone; the list shows their name, number of items and total duration, and a cross deletes them. Saved playlists can also be used as a source in clocks and grid rules.

**Ctrl+S** saves the current playlist and **Ctrl+O** opens a saved one.

## Length

The footer shows the number of items and the total duration. A dash (`--:--`) means the duration is not known yet.

> Note: changing the playlist while the automation is on is allowed. The generator will fill the new end of the list on its next pass. See [Automation overview](topic:automation-overview) and [Playback](topic:playback).
