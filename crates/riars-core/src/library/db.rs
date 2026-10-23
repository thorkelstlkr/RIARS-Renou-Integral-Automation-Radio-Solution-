use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
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
            CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);",
        )
        .map_err(|e| format!("DB init error: {e}"))?;
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
        let mut pending: Vec<PathBuf> = vec![path.to_path_buf()];

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
                let title = file_path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let result = tx.execute(
                    "INSERT OR IGNORE INTO tracks (title, artist, file_path) VALUES (?1, '', ?2)",
                    params![title, file_path_str],
                );

                if let Ok(rows) = result {
                    count += rows;
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
