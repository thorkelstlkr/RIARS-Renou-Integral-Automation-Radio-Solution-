use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::playlist::playlist::Playlist;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_ms: u64,
    pub file_path: String,
    pub bpm: Option<f32>,
    pub sample_rate: u32,
}

pub struct MediaLibrary {
    db: Connection,
}

impl MediaLibrary {
    pub fn new() -> Result<Self, String> {
        Self::open("riars_library.db")
    }

    pub fn open(path: &str) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| format!("DB error: {e}"))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS tracks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                title       TEXT NOT NULL DEFAULT '',
                artist      TEXT NOT NULL DEFAULT '',
                album       TEXT NOT NULL DEFAULT '',
                genre       TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL DEFAULT 0,
                file_path   TEXT NOT NULL UNIQUE,
                bpm         REAL,
                sample_rate INTEGER NOT NULL DEFAULT 44100
            );
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
            CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
            CREATE TABLE IF NOT EXISTS playlists (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                -- Lowercased in Rust: SQLite's NOCASE only folds ASCII (not Ñ, Ç, À...).
                name_key    TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS playlist_items (
                playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                position    INTEGER NOT NULL,
                track_path  TEXT NOT NULL,
                title       TEXT NOT NULL DEFAULT '',
                artist      TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (playlist_id, position)
            );
            PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| format!("DB init error: {e}"))?;
        migrate(&db)?;
        Ok(Self { db })
    }

    /// Add every playable audio file under `folder_path`, including subfolders.
    pub fn scan_folder(&mut self, folder_path: &str) -> Result<usize, String> {
        let path = Path::new(folder_path);
        if !path.is_dir() {
            return Err(format!("Not a directory: {folder_path}"));
        }

        // Formats the symphonia features enabled in Cargo.toml can decode.
        let audio_extensions = ["mp3", "flac", "wav", "ogg", "aac", "m4a"];
        let mut count = 0;

        let tx = self
            .db
            .transaction()
            .map_err(|e| format!("DB error: {e}"))?;
        // Rebuilding from components normalizes separators ("D:/Musica" and "D:\Musica"
        // must give the same stored paths, or a rescan would duplicate every track).
        let mut pending: Vec<PathBuf> = vec![path.components().collect()];

        while let Some(dir) = pending.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    log::warn!("Cannot read directory {}: {e}", dir.display());
                    continue;
                }
            };

            for entry in entries.filter_map(|e| e.ok()) {
                let file_path = entry.path();
                if file_path.is_dir() {
                    pending.push(file_path);
                    continue;
                }

                let ext = file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !audio_extensions.contains(&ext.as_str()) {
                    continue;
                }

                let file_path_str = file_path.to_string_lossy().to_string();
                let tags = read_tags(&file_path);
                let title = tags.title.unwrap_or_else(|| {
                    file_path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string()
                });
                let values = params![
                    title,
                    tags.artist.unwrap_or_default(),
                    tags.album.unwrap_or_default(),
                    tags.genre.unwrap_or_default(),
                    tags.duration_ms as i64,
                    file_path_str
                ];

                match tx.execute(
                    "INSERT OR IGNORE INTO tracks (title, artist, album, genre, duration_ms, file_path)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    values,
                ) {
                    Ok(0) => {
                        // Already known: fill in tags for tracks scanned before tags were read.
                        let _ = tx.execute(
                            "UPDATE tracks SET title = ?1, artist = ?2, album = ?3, genre = ?4, duration_ms = ?5
                             WHERE file_path = ?6 AND duration_ms = 0",
                            values,
                        );
                    }
                    Ok(rows) => count += rows,
                    Err(e) => log::warn!("Cannot add {file_path_str}: {e}"),
                }
            }
        }

        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        log::info!("Scanned {folder_path}: {count} new tracks added");
        Ok(count)
    }

    pub fn get_all_tracks(&self) -> Result<Vec<Track>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, title, artist, album, genre, duration_ms, file_path, bpm, sample_rate
                 FROM tracks ORDER BY artist, title",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let tracks = stmt
            .query_map([], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    genre: row.get(4)?,
                    duration_ms: row.get(5)?,
                    file_path: row.get(6)?,
                    bpm: row.get(7)?,
                    sample_rate: row.get(8)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tracks)
    }

    pub fn search(&self, query: &str) -> Result<Vec<Track>, String> {
        let pattern = format!("%{query}%");
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, title, artist, album, genre, duration_ms, file_path, bpm, sample_rate
                 FROM tracks
                 WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 OR genre LIKE ?1
                 ORDER BY artist, title",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let tracks = stmt
            .query_map(params![pattern], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    genre: row.get(4)?,
                    duration_ms: row.get(5)?,
                    file_path: row.get(6)?,
                    bpm: row.get(7)?,
                    sample_rate: row.get(8)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tracks)
    }

    pub fn get_track(&self, id: i64) -> Result<Option<Track>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, title, artist, album, genre, duration_ms, file_path, bpm, sample_rate
                 FROM tracks WHERE id = ?1",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let track = stmt
            .query_row(params![id], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    genre: row.get(4)?,
                    duration_ms: row.get(5)?,
                    file_path: row.get(6)?,
                    bpm: row.get(7)?,
                    sample_rate: row.get(8)?,
                })
            })
            .ok();

        Ok(track)
    }

    /// Save `playlist` under `name`, replacing a saved playlist of the same name
    /// (case-insensitive). Returns the saved playlist's id.
    pub fn save_playlist(&mut self, name: &str, playlist: &Playlist) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Playlist name is required".to_string());
        }
        let tx = self.db.transaction().map_err(|e| format!("DB error: {e}"))?;
        let existing: Option<String> = tx
            .query_row("SELECT id FROM playlists WHERE name_key = ?1", params![name.to_lowercase()], |r| r.get(0))
            .ok();
        let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        tx.execute(
            "INSERT INTO playlists (id, name, name_key) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, name_key = excluded.name_key,
                                           updated_at = datetime('now')",
            params![id, name, name.to_lowercase()],
        )
        .map_err(|e| format!("DB error: {e}"))?;
        tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        for (position, item) in playlist.items.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_items (playlist_id, position, track_path, title, artist, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, position as i64, item.track_path, item.title, item.artist, item.duration_ms as i64],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        Ok(id)
    }

    pub fn list_playlists(&self) -> Result<Vec<SavedPlaylist>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT p.id, p.name, p.updated_at, COUNT(i.position), COALESCE(SUM(i.duration_ms), 0)
                 FROM playlists p LEFT JOIN playlist_items i ON i.playlist_id = p.id
                 GROUP BY p.id ORDER BY p.name_key",
            )
            .map_err(|e| format!("Query error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SavedPlaylist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                    items: row.get::<_, i64>(3)? as usize,
                    duration_ms: row.get::<_, i64>(4)? as u64,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("Query error: {e}"))
    }

    /// Load a saved playlist by id or by name (case-insensitive).
    pub fn load_playlist(&self, id_or_name: &str) -> Result<Playlist, String> {
        let (id, name): (String, String) = self
            .db
            .query_row(
                "SELECT id, name FROM playlists WHERE id = ?1 OR name_key = ?2",
                params![id_or_name, id_or_name.trim().to_lowercase()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| format!("No saved playlist {id_or_name}"))?;
        let mut playlist = Playlist::new(name);
        playlist.id = id.clone();
        let mut stmt = self
            .db
            .prepare(
                "SELECT track_path, title, artist, duration_ms FROM playlist_items
                 WHERE playlist_id = ?1 ORDER BY position",
            )
            .map_err(|e| format!("Query error: {e}"))?;
        let rows = stmt
            .query_map(params![id], |r| {
                Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?))
            })
            .map_err(|e| format!("Query error: {e}"))?;
        for row in rows {
            let (path, title, artist, duration) = row.map_err(|e| format!("Query error: {e}"))?;
            playlist.add_item(path, title, artist, duration as u64);
        }
        Ok(playlist)
    }

    pub fn delete_playlist(&mut self, id: &str) -> Result<(), String> {
        let rows = self
            .db
            .execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        if rows == 0 {
            return Err(format!("No saved playlist {id}"));
        }
        Ok(())
    }

    pub fn track_count(&self) -> Result<usize, String> {
        self.db
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .map_err(|e| format!("Count error: {e}"))
    }

    pub fn delete_track(&self, id: i64) -> Result<bool, String> {
        let rows = self
            .db
            .execute("DELETE FROM tracks WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete error: {e}"))?;
        Ok(rows > 0)
    }
}

/// Bring databases created by older versions up to the current schema.
fn migrate(db: &Connection) -> Result<(), String> {
    let has_name_key = db
        .prepare("SELECT 1 FROM pragma_table_info('playlists') WHERE name = 'name_key'")
        .and_then(|mut stmt| stmt.exists([]))
        .map_err(|e| format!("DB migration error: {e}"))?;
    if !has_name_key {
        db.execute_batch("ALTER TABLE playlists ADD COLUMN name_key TEXT NOT NULL DEFAULT ''")
            .map_err(|e| format!("DB migration error: {e}"))?;
    }
    // Fill keys left empty (lowercased in Rust, see the schema), then enforce them.
    let rows: Vec<(String, String)> = db
        .prepare("SELECT id, name FROM playlists WHERE name_key = ''")
        .and_then(|mut stmt| stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(|e| format!("DB migration error: {e}"))?;
    for (id, name) in rows {
        db.execute("UPDATE playlists SET name_key = ?1 WHERE id = ?2", params![name.to_lowercase(), id])
            .map_err(|e| format!("DB migration error: {e}"))?;
    }
    db.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_playlists_name_key ON playlists(name_key);")
        .map_err(|e| format!("DB migration error: {e}"))
}

/// A saved playlist as listed (without its items).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPlaylist {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    pub items: usize,
    pub duration_ms: u64,
}

#[derive(Default)]
struct TrackTags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    duration_ms: u64,
}

/// Read tags and duration from the container without decoding audio.
fn read_tags(path: &Path) -> TrackTags {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
    use symphonia::core::probe::Hint;

    let mut tags = TrackTags::default();
    let Ok(file) = std::fs::File::open(path) else {
        return tags;
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let Ok(mut probed) = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return tags;
    };

    if let Some(track) = probed.format.default_track() {
        let p = &track.codec_params;
        if let (Some(frames), Some(rate)) = (p.n_frames, p.sample_rate) {
            if rate > 0 {
                tags.duration_ms = frames * 1000 / rate as u64;
            }
        }
    }

    let mut apply = |revision: &MetadataRevision| {
        for tag in revision.tags() {
            let value = tag.value.to_string().trim().to_string();
            if value.is_empty() {
                continue;
            }
            let slot = match tag.std_key {
                Some(StandardTagKey::TrackTitle) => &mut tags.title,
                Some(StandardTagKey::Artist) => &mut tags.artist,
                Some(StandardTagKey::Album) => &mut tags.album,
                Some(StandardTagKey::Genre) => &mut tags.genre,
                _ => continue,
            };
            slot.get_or_insert(value);
        }
    };
    // Tags can sit before the container (ID3v2) or inside it (Vorbis comments, MP4).
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            apply(revision);
        }
    }
    if let Some(revision) = probed.format.metadata().current() {
        apply(revision);
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_list_delete_playlists() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        let mut pl = Playlist::new("tmp".into());
        pl.add_item("/m/a.mp3".into(), "A".into(), "X".into(), 1000);
        pl.add_item("/m/b.mp3".into(), "B".into(), "".into(), 2500);

        assert!(lib.save_playlist("  ", &pl).is_err());
        let id = lib.save_playlist("Mañanas", &pl).unwrap();
        let listed = lib.list_playlists().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].items, listed[0].duration_ms), (2, 3500));

        // Same name (any case) replaces it, keeping the id.
        pl.remove_item(0);
        assert_eq!(lib.save_playlist("MAÑANAS", &pl).unwrap(), id);
        assert_eq!(lib.list_playlists().unwrap()[0].items, 1);

        let loaded = lib.load_playlist(&id).unwrap();
        assert_eq!(loaded.name, "MAÑANAS");
        assert_eq!(loaded.items[0].track_path, "/m/b.mp3");
        assert_eq!(loaded.items[0].duration_ms, 2500);
        assert_eq!(lib.load_playlist("mañanas").unwrap().id, id); // by name
        assert!(lib.load_playlist("nope").is_err());

        lib.delete_playlist(&id).unwrap();
        assert!(lib.list_playlists().unwrap().is_empty());
        assert!(lib.delete_playlist(&id).is_err());
    }

    #[test]
    fn migrates_playlists_without_name_key() {
        let path = std::env::temp_dir().join(format!("riars-migrate-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            // The schema of the first saved-playlists build.
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE playlists (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     updated_at TEXT NOT NULL DEFAULT (datetime('now')));
                 INSERT INTO playlists (id, name) VALUES ('p1', 'Mañanas');",
            )
            .unwrap();
        }
        let mut lib = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        assert_eq!(lib.list_playlists().unwrap()[0].name, "Mañanas");
        assert_eq!(lib.load_playlist("MAÑANAS").unwrap().id, "p1");
        assert_eq!(lib.save_playlist("mañanas", &Playlist::new("x".into())).unwrap(), "p1");
        drop(lib);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rescan_with_other_separators_adds_nothing() {
        let dir = std::env::temp_dir().join(format!("riars-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("a.mp3"), b"not really audio").unwrap();
        let mut lib = MediaLibrary::open(":memory:").unwrap();

        assert_eq!(lib.scan_folder(&dir.to_string_lossy()).unwrap(), 1);
        let forward = dir.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        assert_eq!(lib.scan_folder(&forward).unwrap(), 0);
        let track = &lib.get_all_tracks().unwrap()[0];
        assert_eq!(track.title, "a");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
