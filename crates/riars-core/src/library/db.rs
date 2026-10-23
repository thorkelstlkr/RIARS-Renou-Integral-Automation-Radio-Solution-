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
