---
id: library
title: Media library
section: onair
order: 1
keywords: library, folders, folder tree, explorer, scanning, rescan, search, tracks, add music
---

# Media library

The **Library** is the list of audio files RIARS knows about. The **Library** tab shows them in one searchable list; the **Folders** tab is a folder tree for choosing which folders the library is built from and for browsing their files.

## Add music

The library is built from **library folders**. RIARS remembers them and rescans them every time it starts, so new music you copy into them shows up by itself.

There are three ways to add folders:

- **Folders tab**: a tree of the computer that opens with ▸. Tick the box of each folder you want in the library; you can tick several in a row.
- **Add folder**, above the library, or **File ▸ Add folder to the library…** (**Ctrl+Shift+A**): the chooser lets you pick several folders at once.
- **Add…** in the Folders tab does the same.

RIARS scans in the background, subfolders included, and the notice line says which folder it is reading and how many new tracks it found. You can keep working meanwhile. Rescanning only adds what is new: tracks already in the library are not duplicated and their tags are not read again.

It reads **MP3, FLAC, WAV, OGG, AAC and M4A** files, taking title, artist, album, genre and duration from their tags; a file without tags uses its file name as the title. What is missing can be [completed from MusicBrainz](topic:metadata).

On the [headless server](topic:headless-server), `--scan` adds a library folder at startup, and the REST API and MCP server can list, add, remove and rescan library folders.

## The Folders tab

At the top is the list of **Library folders**, with how many tracks each holds, and **Rescan** to look for new music without restarting.

Below it is the tree of the computer:

- A **ticked** box is a library folder. A ticked but greyed box means the folder is already included through a folder higher up.
- When you **untick** a folder, or press ✕ in the list, RIARS asks whether to remove only the folder (its tracks stay) or its tracks too. Files are **never deleted** from disk.
- Clicking a folder's **name** lists its audio files underneath; **double-clicking** it also opens or closes it, like ▸. Files not yet in the library are dimmed, but you can still use them. Opening the **Marks…** of one of them from the playlist adds it to the library there and then, since marks are kept with the track.

Files in that list work like library tracks: **double-click** or **Enter** adds them to the playlist, you can **drag** them onto a [sound bank](topic:live) button, and they have a right-click menu.

## Search

Type in the **Search...** box. RIARS searches the library in the background shortly after you stop typing, matching title, artist, album and genre; every word must match, and case and accents are ignored. Clear the box to see the whole library again. **Ctrl+L** puts the cursor in the search box.

The counter under the search box shows the number of matching tracks. The list loads in pages as you scroll, so a very large library does not have to load all at once.

## Add a track to the playlist

**Double-click** a track, or press **Enter**, to add it to the **Playlist**. It goes at the end of the current list. You can then start it from the playlist, or let it wait its turn.

You can select several tracks: **Ctrl+click** adds or removes one, **Shift+click** selects a range and **Ctrl+A** selects all.

## Right-click menu

Right-click a track, or the selection:

| Option | What it does |
|---|---|
| **Add to playlist** | At the end of the playlist. |
| **Play next** | Right after what is playing now. |
| **Pre-listen** | On the [pre-listen card](topic:sound-cards), not on air. |
| **Marks…** | Opens the [intro and outro](topic:intro-outro) editor. |
| **Detect marks** | Analyses the chosen tracks; hand-set marks are kept. |
| **Complete metadata…** | Looks up the missing artist, album, year, genre and cover in MusicBrainz. See [Complete metadata](topic:metadata). |
| **Show in folder** | Shows the file in the file manager. |
| **Copy path** | Copies the file path. |
| **Remove from library…** | Asks first. The file **stays on disk**, but the track leaves its categories. **Delete** does the same. |

> Tip: the library list is for choosing music. For the order of the show, use the [Playlist](topic:playlist) tab.

If a track will not play, RIARS reports **Cannot play this file** with the reason; check that the file still exists and that its format is supported. See [First steps](topic:first-steps) for a quick start.
