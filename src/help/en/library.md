---
id: library
title: Media library
section: onair
order: 1
keywords: library, folders, scanning, search, tracks, add music
---

# Media library

The **Library** is the list of audio files RIARS knows about. It is a catalogue, not a folder tree: scanning a folder adds its files to the library database, and the left-hand list shows them in one searchable list.

## Add a folder

1. In the **Library** area, click **Add folder**.
2. Choose a folder. RIARS scans it and shows **Scanning folder...**.
3. When it finishes, a message reports how many new tracks were added.

Audio files already in the library are not duplicated. You can scan several folders, one after another; each track keeps its file path, which you can see by hovering over it.

On the headless server, use the `--scan` flag at startup or the library scan endpoint. On the desktop, **Ctrl+Shift+A** also opens the folder chooser.

## Search

Type in the **Search...** box. RIARS searches the library in the background shortly after you stop typing, matching title, artist, album and genre; every word must match, and case and accents are ignored. Clear the box to see the whole library again. **Ctrl+L** puts the cursor in the search box.

The counter under the search box shows the number of matching tracks. The list loads in pages as you scroll, so a very large library does not have to load all at once.

## Add a track to the playlist

**Double-click** a track to add it to the **Playlist**. It goes at the end of the current list. You can then start it from the playlist, or let it wait its turn.

> Tip: the library list is for choosing music. For the order of the show, use the [Playlist](topic:playlist) tab.

If a track will not play, RIARS reports **Cannot play this file** with the reason; check that the file still exists and that its format is supported. See [First steps](topic:first-steps) for a quick start.
