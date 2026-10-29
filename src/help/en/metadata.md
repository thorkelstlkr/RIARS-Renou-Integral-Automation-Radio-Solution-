---
id: metadata
title: Complete metadata
section: onair
order: 10
keywords: metadata, tags, artist, album, year, genre, cover, artwork, musicbrainz, acoustid, fingerprint, identify, complete
---

# Complete metadata

RIARS can look up what your [library](topic:library) tracks are missing in open catalogues: **title, artist, album, year, genre and cover**. Looking up changes nothing. You see what was found next to what each track has, and only what you tick is applied.

Filling in year and genre also helps automation: smart [categories](topic:categories) can filter by them.

## Opening it

There are two ways to open it. Neither starts on its own: the top of the dialog says what will be looked up, and the lookup starts when you press **Look up**.

- **Right-click ▸ Complete metadata…** in the library looks up the selected tracks, even if they already have every field. Use it to check a particular track.
- **Edit ▸ Complete library metadata…** looks up the tracks of a minute or longer that have a gap (no artist, album, genre or year). Shorter ones, such as jingles and station IDs, are skipped because no catalogue lists them; the dialog says how many. To look one of them up, select it and use the right-click menu.

When **Look up** can't be pressed, the reason is shown next to it:

- **There is nothing to look up**: no track is missing anything.
- **A lookup is running**: only one can run at a time. Wait for it to finish or press **Stop**.

The lookup runs in the background. You can close the dialog and open it again to see how it is going. **Stop** ends it after the current track and keeps what was found.

## Where the data comes from

- **[MusicBrainz](https://musicbrainz.org)** is an open music encyclopedia whose data is public domain (CC0). RIARS searches for the recording by the **title and artist** in the tags and compares the length. If a track has no tags but the file is named "Artist - Title", it searches by the name.
- **[AcoustID](https://acoustid.org)** is optional. It recognises a song by **how it sounds** (its audio fingerprint), even when the file has no tags at all. It needs an application key, entered at the top of the dialog and kept with the library.
  - It is free for non-commercial use: register one at acoustid.org/new-application.
  - A commercial station needs its own key from acoustid.biz.
- **[Cover Art Archive](https://coverartarchive.org)** supplies the album cover.

MusicBrainz allows one request per second, so each track takes 2 to 4 seconds: about 500 tracks take about 20 minutes.

When the same song is on several releases (the original, a live album, a compilation), RIARS prefers the album the track already names. Otherwise it takes the earliest studio album.

## Review and apply

Each track found shows the cover, how it was found (by tags, by file name or by fingerprint) and the confidence. Below that is every field that would change, with the current value → the new one.

- **Only the gaps** start ticked: empty fields, or a title that is just the file name. What a track already has is never ticked for you; tick it yourself to change it.
- **Apply** stores what is ticked in the library. Each track records where its data came from (`musicbrainz:` and the recording's identifier).
- **Also write the tags into the audio files** is off by default. It also saves the values and the cover inside the file, so other programs see them, and asks first. RIARS writes a copy, checks that it reads back, and only then replaces the original. A file that is playing may not be replaceable, and RIARS says so.
- Covers are kept in the `riars_covers` folder next to the library database.

Tracks with no match are listed at the end, folded, with the reason when there was an error (for example, no connection).

## For agents and the API

The REST API has `POST /api/v1/library/metadata/lookup` to start, `GET` on the same path for the status and proposals, `POST /library/metadata/apply`, and `/library/metadata/settings` for the key. The MCP server offers `lookup_track_metadata`, `metadata_lookup_status`, `apply_track_metadata`, `get_track_metadata` and `set_acoustid_key`. See [API and agents](topic:api-agents).
