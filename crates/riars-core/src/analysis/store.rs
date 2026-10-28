//! SQLite storage for cue points and loudness.

use chrono::NaiveDateTime;
use rusqlite::{params, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::library::db::MediaLibrary;

use super::model::{AnalysisStats, CueSource, TrackCues};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

type CuesRaw = (
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    String,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<String>,
);

pub(crate) fn init_cues_schema(db: &rusqlite::Connection) -> Result<(), String> {
    let columns = [
        ("cue_in_ms", "INTEGER"),
        ("cue_out_ms", "INTEGER"),
        ("intro_end_ms", "INTEGER"),
        ("outro_start_ms", "INTEGER"),
        ("cues_source", "TEXT"),
        ("cues_confidence", "REAL"),
        ("loudness_lufs", "REAL"),
        ("peak_dbfs", "REAL"),
        ("analyzed_at", "TEXT"),
    ];
    for (name, kind) in columns {
        let exists = db
            .prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1")
            .and_then(|mut stmt| stmt.exists(params![name]))
            .map_err(|e| format!("DB error: {e}"))?;
        if !exists {
            db.execute_batch(&format!("ALTER TABLE tracks ADD COLUMN {name} {kind}"))
                .map_err(|e| format!("DB error: {e}"))?;
        }
    }
    db.execute_batch("CREATE INDEX IF NOT EXISTS idx_tracks_analyzed ON tracks(analyzed_at)")
        .map_err(|e| format!("DB error: {e}"))
}

fn enum_text<T: Serialize>(value: &T) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|e| format!("DB error: {e}"))?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "DB error: enum is not a string".to_string())
}

fn enum_from_text<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|e| format!("DB error: {e}"))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format(DATETIME_FORMAT).to_string()
}

fn format_optional_datetime(value: Option<NaiveDateTime>) -> Option<String> {
    value.map(format_datetime)
}

fn parse_datetime(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text, DATETIME_FORMAT).map_err(|e| format!("DB error: {e}"))
}

fn parse_optional_datetime(text: Option<String>) -> Result<Option<NaiveDateTime>, String> {
    text.map(|value| parse_datetime(&value)).transpose()
}

fn to_u64(value: i64) -> u64 {
    value.max(0) as u64
}

fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn raw_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CuesRaw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn cues_from_raw(raw: CuesRaw) -> Result<TrackCues, String> {
    let (
        track_id,
        duration_ms,
        cue_in_ms,
        cue_out_ms,
        intro_end_ms,
        outro_start_ms,
        source,
        confidence,
        loudness_lufs,
        peak_dbfs,
        analyzed_at,
    ) = raw;
    Ok(TrackCues {
        track_id,
        duration_ms: to_u64(duration_ms),
        cue_in_ms: cue_in_ms.map(to_u64).unwrap_or(0),
        cue_out_ms: cue_out_ms.map(to_u64).unwrap_or(0),
        intro_end_ms: intro_end_ms.map(to_u64),
        outro_start_ms: outro_start_ms.map(to_u64),
        source: enum_from_text(&source)?,
        confidence: confidence.unwrap_or(0.0) as f32,
        loudness_lufs: loudness_lufs.map(|value| value as f32),
        peak_dbfs: peak_dbfs.map(|value| value as f32),
        analyzed_at: parse_optional_datetime(analyzed_at)?,
    })
}

const CUES_COLUMNS: &str = "id, duration_ms, cue_in_ms, cue_out_ms, intro_end_ms, outro_start_ms,
     cues_source, cues_confidence, loudness_lufs, peak_dbfs, analyzed_at";

impl MediaLibrary {
    pub fn track_cues(&self, track_id: i64) -> Result<Option<TrackCues>, String> {
        let raw = self
            .conn()
            .query_row(
                &format!(
                    "SELECT {CUES_COLUMNS} FROM tracks WHERE id = ?1 AND cues_source IS NOT NULL"
                ),
                params![track_id],
                raw_from_row,
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        raw.map(cues_from_raw).transpose()
    }

    /// Cues of the track at `file_path`, for playlist items that carry no track id
    /// (added by hand, loaded from a saved playlist or over the API).
    pub fn track_cues_by_path(&self, file_path: &str) -> Result<Option<TrackCues>, String> {
        let raw = self
            .conn()
            .query_row(
                &format!(
                    "SELECT {CUES_COLUMNS} FROM tracks WHERE file_path = ?1 AND cues_source IS NOT NULL"
                ),
                params![file_path],
                raw_from_row,
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        raw.map(cues_from_raw).transpose()
    }

    pub fn save_analysis(&mut self, cues: &TrackCues) -> Result<bool, String> {
        let stored: Option<(Option<String>, i64)> = self
            .conn()
            .query_row(
                "SELECT cues_source, duration_ms FROM tracks WHERE id = ?1",
                params![cues.track_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        let keeps_marks =
            stored.as_ref().is_some_and(|(source, _)| {
                matches!(source.as_deref(), Some("Manual") | Some("Agent"))
            });

        let rows = if keeps_marks {
            self.conn()
                .execute(
                    "UPDATE tracks SET
                        duration_ms = CASE WHEN duration_ms = 0 THEN ?1 ELSE duration_ms END,
                        loudness_lufs = ?2, peak_dbfs = ?3, analyzed_at = ?4
                     WHERE id = ?5",
                    params![
                        to_i64(cues.duration_ms),
                        cues.loudness_lufs.map(|value| value as f64),
                        cues.peak_dbfs.map(|value| value as f64),
                        format_optional_datetime(cues.analyzed_at),
                        cues.track_id
                    ],
                )
                .map_err(|e| format!("DB error: {e}"))?
        } else {
            let source = enum_text(&CueSource::Auto)?;
            self.conn()
                .execute(
                    "UPDATE tracks SET
                        duration_ms = ?1, cue_in_ms = ?2, cue_out_ms = ?3, intro_end_ms = ?4,
                        outro_start_ms = ?5, cues_source = ?6, cues_confidence = ?7,
                        loudness_lufs = ?8, peak_dbfs = ?9, analyzed_at = ?10
                     WHERE id = ?11",
                    params![
                        to_i64(cues.duration_ms),
                        to_i64(cues.cue_in_ms),
                        to_i64(cues.cue_out_ms),
                        cues.intro_end_ms.map(to_i64),
                        cues.outro_start_ms.map(to_i64),
                        source,
                        cues.confidence as f64,
                        cues.loudness_lufs.map(|value| value as f64),
                        cues.peak_dbfs.map(|value| value as f64),
                        format_optional_datetime(cues.analyzed_at),
                        cues.track_id
                    ],
                )
                .map_err(|e| format!("DB error: {e}"))?
        };

        if keeps_marks {
            return Ok(false);
        }
        if rows > 0 {
            self.bump_revision();
        }
        Ok(true)
    }

    /// Save marks set by hand or by an agent. They are checked against the length stored
    /// for the track, not the one in `cues`. An agent cannot replace hand-set marks: the
    /// operator clears them first. Loudness and peak are written only when given.
    pub fn set_track_cues(&mut self, cues: &TrackCues) -> Result<(), String> {
        let confidence = match cues.source {
            CueSource::Manual => 1.0,
            CueSource::Agent => cues.confidence.clamp(0.0, 1.0),
            CueSource::Auto => {
                return Err("Automatic marks must be saved with save_analysis".to_string());
            }
        };
        let stored: Option<(i64, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT duration_ms, cues_source FROM tracks WHERE id = ?1",
                params![cues.track_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        let Some((duration_ms, stored_source)) = stored else {
            return Err(format!("No track {}", cues.track_id));
        };
        if cues.source == CueSource::Agent && stored_source.as_deref() == Some("Manual") {
            return Err(format!(
                "Track {} has hand-set marks; the operator must clear them first",
                cues.track_id
            ));
        }
        let duration_ms = u64::try_from(duration_ms).unwrap_or(0);
        let checked = TrackCues {
            duration_ms: if duration_ms > 0 { duration_ms } else { cues.duration_ms },
            ..cues.clone()
        };
        checked.validate()?;
        let source = enum_text(&cues.source)?;
        self.conn()
            .execute(
                "UPDATE tracks SET
                    cue_in_ms = ?1, cue_out_ms = ?2, intro_end_ms = ?3, outro_start_ms = ?4,
                    cues_source = ?5, cues_confidence = ?6,
                    loudness_lufs = COALESCE(?7, loudness_lufs), peak_dbfs = COALESCE(?8, peak_dbfs)
                 WHERE id = ?9",
                params![
                    to_i64(cues.cue_in_ms),
                    to_i64(cues.cue_out_ms),
                    cues.intro_end_ms.map(to_i64),
                    cues.outro_start_ms.map(to_i64),
                    source,
                    confidence as f64,
                    cues.loudness_lufs.map(|value| value as f64),
                    cues.peak_dbfs.map(|value| value as f64),
                    cues.track_id
                ],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(())
    }

    pub fn clear_track_cues(&mut self, track_id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute(
                "UPDATE tracks SET
                    cue_in_ms = NULL, cue_out_ms = NULL, intro_end_ms = NULL,
                    outro_start_ms = NULL, cues_source = NULL, cues_confidence = NULL
                 WHERE id = ?1",
                params![track_id],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(rows > 0)
    }

    pub fn tracks_to_analyze(
        &self,
        category_id: Option<i64>,
        reanalyze: bool,
        limit: usize,
    ) -> Result<Vec<(i64, String)>, String> {
        let analyzed_filter = if reanalyze { "" } else { " AND t.analyzed_at IS NULL" };
        let limit = limit.min(i64::MAX as usize) as i64;

        let rows: Vec<(i64, String)> = match category_id {
            Some(category_id) => {
                let sql = format!(
                    "WITH RECURSIVE tree(id) AS (
                        SELECT id FROM categories WHERE id = ?1
                        UNION
                        SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id
                     )
                     SELECT DISTINCT t.id, t.file_path
                     FROM tracks t
                     JOIN track_categories tc ON tc.track_id = t.id
                     JOIN tree ON tree.id = tc.category_id
                     WHERE t.enabled = 1{analyzed_filter}
                     ORDER BY t.id
                     LIMIT ?2"
                );
                let mut stmt = self.conn().prepare(&sql).map_err(|e| format!("DB error: {e}"))?;
                let mapped = stmt
                    .query_map(params![category_id, limit], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|e| format!("DB error: {e}"))?;
                mapped
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("DB error: {e}"))?
            }
            None => {
                let sql = format!(
                    "SELECT t.id, t.file_path FROM tracks t
                     WHERE t.enabled = 1{analyzed_filter}
                     ORDER BY t.id
                     LIMIT ?1"
                );
                let mut stmt = self.conn().prepare(&sql).map_err(|e| format!("DB error: {e}"))?;
                let mapped = stmt
                    .query_map(params![limit], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|e| format!("DB error: {e}"))?;
                mapped
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("DB error: {e}"))?
            }
        };
        Ok(rows)
    }

    pub fn loudness_by_path(
        &self,
        file_path: &str,
    ) -> Result<Option<(f32, Option<f32>)>, String> {
        let raw: Option<(Option<f64>, Option<f64>)> = self
            .conn()
            .query_row(
                "SELECT loudness_lufs, peak_dbfs FROM tracks WHERE file_path = ?1",
                params![file_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(match raw {
            Some((Some(loudness), peak)) => {
                Some((loudness as f32, peak.map(|value| value as f32)))
            }
            _ => None,
        })
    }

    pub fn analysis_stats(&self, min_confidence: f32) -> Result<AnalysisStats, String> {
        let raw: (i64, i64, i64, i64, i64, i64) = self
            .conn()
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE WHEN analyzed_at IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN cues_source IN ('Manual', 'Agent') THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN intro_end_ms > cue_in_ms THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN outro_start_ms < cue_out_ms THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN cues_source = 'Auto' AND cues_confidence < ?1
                                      THEN 1 ELSE 0 END), 0)
                 FROM tracks WHERE enabled = 1",
                params![min_confidence as f64],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(AnalysisStats {
            tracks: to_u64(raw.0) as usize,
            analyzed: to_u64(raw.1) as usize,
            manual: to_u64(raw.2) as usize,
            with_intro: to_u64(raw.3) as usize,
            with_outro: to_u64(raw.4) as usize,
            low_confidence: to_u64(raw.5) as usize,
        })
    }

    pub fn tracks_needing_review(
        &self,
        min_confidence: f32,
        limit: usize,
    ) -> Result<Vec<TrackCues>, String> {
        let limit = limit.min(i64::MAX as usize) as i64;
        let mut stmt = self
            .conn()
            .prepare(&format!(
                "SELECT {CUES_COLUMNS} FROM tracks
                 WHERE enabled = 1 AND cues_source = 'Auto' AND cues_confidence < ?1
                 ORDER BY cues_confidence, id
                 LIMIT ?2"
            ))
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![min_confidence as f64, limit], raw_from_row)
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let raw = row.map_err(|e| format!("DB error: {e}"))?;
            cues_from_raw(raw)
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::{NaiveDate, NaiveDateTime};

    use super::*;
    use crate::automation::model::Category;
    use crate::playlist::playlist::ItemKind;

    fn remove_db_files(path: &Path) {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let _ = std::fs::remove_file(PathBuf::from(shm));
    }

    fn temp_db(name: &str) -> (MediaLibrary, PathBuf) {
        let path = std::env::temp_dir().join(format!("riars-cues-{}-{name}.db", std::process::id()));
        remove_db_files(&path);
        let lib = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        (lib, path)
    }

    fn insert_track(lib: &MediaLibrary, title: &str, file_path: &str, duration_ms: i64) -> i64 {
        lib.conn()
            .execute(
                "INSERT INTO tracks (title, artist, album, genre, duration_ms, file_path)
                 VALUES (?1, '', '', '', ?2, ?3)",
                params![title, duration_ms, file_path],
            )
            .unwrap();
        lib.conn().last_insert_rowid()
    }

    fn category(name: &str, parent_id: Option<i64>) -> Category {
        Category {
            id: 0,
            name: name.to_string(),
            kind: ItemKind::Music,
            color: String::new(),
            parent_id,
            filter: None,
            separation: None,
        }
    }

    fn datetime() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 27).unwrap().and_hms_opt(12, 34, 56).unwrap()
    }

    fn auto_cues(
        track_id: i64,
        cue_in_ms: u64,
        intro_end_ms: Option<u64>,
        outro_start_ms: Option<u64>,
        cue_out_ms: u64,
        confidence: f32,
    ) -> TrackCues {
        TrackCues {
            track_id,
            duration_ms: 200_000,
            cue_in_ms,
            cue_out_ms,
            intro_end_ms,
            outro_start_ms,
            source: CueSource::Auto,
            confidence,
            loudness_lufs: Some(-14.25),
            peak_dbfs: Some(-1.5),
            analyzed_at: Some(datetime()),
        }
    }

    fn manual_cues(track_id: i64) -> TrackCues {
        TrackCues {
            track_id,
            duration_ms: 200_000,
            cue_in_ms: 1_000,
            cue_out_ms: 198_000,
            intro_end_ms: Some(10_000),
            outro_start_ms: Some(185_000),
            source: CueSource::Manual,
            confidence: 1.0,
            loudness_lufs: None,
            peak_dbfs: None,
            analyzed_at: None,
        }
    }

    #[test]
    fn save_read_round_trip() {
        let (mut lib, db_path) = temp_db("roundtrip");
        let track = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        let other = insert_track(&lib, "B", "/m/b.mp3", 100_000);

        let cues = auto_cues(track, 500, Some(12_500), Some(180_000), 195_000, 0.8);
        assert!(lib.save_analysis(&cues).unwrap());
        assert_eq!(lib.track_cues(track).unwrap(), Some(cues.clone()));
        assert!(lib.track_cues(other).unwrap().is_none());
        assert!(lib.track_cues(9_999).unwrap().is_none());

        let changed = auto_cues(track, 700, None, None, 190_000, 0.6);
        assert!(lib.save_analysis(&changed).unwrap());
        assert_eq!(lib.track_cues(track).unwrap(), Some(changed));

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn manual_marks_survive_analysis_but_loudness_updates() {
        let (mut lib, db_path) = temp_db("manual-survives");
        let track = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        let mut marks = manual_cues(track);
        marks.confidence = 0.2;
        lib.set_track_cues(&marks).unwrap();

        let loaded = lib.track_cues(track).unwrap().unwrap();
        assert_eq!(loaded.source, CueSource::Manual);
        assert_eq!(loaded.confidence, 1.0);
        assert_eq!(loaded.loudness_lufs, None);

        let mut analysis = auto_cues(track, 800, Some(20_000), Some(170_000), 197_000, 0.4);
        analysis.loudness_lufs = Some(-20.0);
        analysis.peak_dbfs = Some(-3.0);
        assert!(!lib.save_analysis(&analysis).unwrap());

        let loaded = lib.track_cues(track).unwrap().unwrap();
        assert_eq!(loaded.source, CueSource::Manual);
        assert_eq!(loaded.cue_in_ms, 1_000);
        assert_eq!(loaded.intro_end_ms, Some(10_000));
        assert_eq!(loaded.outro_start_ms, Some(185_000));
        assert_eq!(loaded.cue_out_ms, 198_000);
        assert_eq!(loaded.loudness_lufs, Some(-20.0));
        assert_eq!(loaded.peak_dbfs, Some(-3.0));
        assert_eq!(loaded.analyzed_at, Some(datetime()));

        let zero_duration = insert_track(&lib, "Zero", "/m/zero.mp3", 0);
        let mut marks = manual_cues(zero_duration);
        marks.duration_ms = 150_000;
        marks.cue_out_ms = 140_000;
        marks.outro_start_ms = Some(130_000);
        lib.set_track_cues(&marks).unwrap();
        let mut analysis = auto_cues(zero_duration, 0, None, None, 150_000, 0.9);
        analysis.duration_ms = 150_000;
        assert!(!lib.save_analysis(&analysis).unwrap());
        assert_eq!(lib.track_cues(zero_duration).unwrap().unwrap().duration_ms, 150_000);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn set_track_cues_validates_marks_and_source() {
        let (mut lib, db_path) = temp_db("set-validation");
        let track = insert_track(&lib, "A", "/m/a.mp3", 200_000);

        let mut out_of_order = manual_cues(track);
        out_of_order.cue_in_ms = 5_000;
        out_of_order.intro_end_ms = Some(4_000);
        assert!(lib.set_track_cues(&out_of_order).is_err());

        let mut automatic = manual_cues(track);
        automatic.source = CueSource::Auto;
        assert!(lib.set_track_cues(&automatic).is_err());

        let mut missing = manual_cues(9_999);
        missing.source = CueSource::Agent;
        assert!(lib.set_track_cues(&missing).is_err());

        // The stored length counts, not the one sent: no end mark past the real end.
        let mut too_long = manual_cues(track);
        too_long.source = CueSource::Agent;
        too_long.duration_ms = 0;
        too_long.cue_out_ms = 999_999_999;
        too_long.outro_start_ms = None;
        assert!(lib.set_track_cues(&too_long).is_err());

        // An agent may set marks and loudness while nobody has set them by hand...
        let mut agent = manual_cues(track);
        agent.source = CueSource::Agent;
        agent.confidence = 0.7;
        agent.loudness_lufs = Some(-12.5);
        assert!(lib.set_track_cues(&agent).is_ok());
        let saved = lib.track_cues(track).unwrap().unwrap();
        assert_eq!((saved.source, saved.loudness_lufs), (CueSource::Agent, Some(-12.5)));

        assert!(lib.set_track_cues(&manual_cues(track)).is_ok());
        let saved = lib.track_cues(track).unwrap().unwrap();
        assert_eq!((saved.source, saved.loudness_lufs), (CueSource::Manual, Some(-12.5)));
        // ...but not over hand-set marks.
        assert!(lib.set_track_cues(&agent).is_err());

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn cues_by_path() {
        let (mut lib, db_path) = temp_db("by-path");
        let track = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        assert!(lib.track_cues_by_path("/m/a.mp3").unwrap().is_none());
        lib.set_track_cues(&manual_cues(track)).unwrap();
        assert_eq!(lib.track_cues_by_path("/m/a.mp3").unwrap().unwrap().track_id, track);
        assert!(lib.track_cues_by_path("/m/other.mp3").unwrap().is_none());
        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn clear_marks_keeps_loudness() {
        let (mut lib, db_path) = temp_db("clear");
        let track = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        lib.save_analysis(&auto_cues(track, 500, Some(12_500), Some(180_000), 195_000, 0.8))
            .unwrap();
        lib.set_track_cues(&manual_cues(track)).unwrap();

        assert!(lib.clear_track_cues(track).unwrap());
        assert!(lib.track_cues(track).unwrap().is_none());
        assert_eq!(lib.loudness_by_path("/m/a.mp3").unwrap(), Some((-14.25, Some(-1.5))));

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn tracks_to_analyze_filters_categories_and_limit() {
        let (mut lib, db_path) = temp_db("to-analyze");
        let a = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        let b = insert_track(&lib, "B", "/m/b.mp3", 200_000);
        let c = insert_track(&lib, "C", "/m/c.mp3", 200_000);
        let d = insert_track(&lib, "D", "/m/d.mp3", 200_000);
        let e = insert_track(&lib, "E", "/m/e.mp3", 200_000);

        let parent = lib.create_category(&category("Music", None)).unwrap();
        let child = lib.create_category(&category("Rock", Some(parent))).unwrap();
        lib.add_tracks_to_category(parent, &[a, c], 1.0).unwrap();
        lib.add_tracks_to_category(child, &[b], 1.0).unwrap();
        lib.set_track_enabled(e, false).unwrap();
        lib.save_analysis(&auto_cues(c, 500, None, None, 195_000, 0.9)).unwrap();

        let ids = |rows: Vec<(i64, String)>| rows.into_iter().map(|row| row.0).collect::<Vec<_>>();

        assert_eq!(ids(lib.tracks_to_analyze(None, false, 100).unwrap()), vec![a, b, d]);
        assert_eq!(ids(lib.tracks_to_analyze(None, true, 100).unwrap()), vec![a, b, c, d]);
        assert_eq!(ids(lib.tracks_to_analyze(Some(parent), false, 100).unwrap()), vec![a, b]);
        assert_eq!(ids(lib.tracks_to_analyze(Some(parent), true, 100).unwrap()), vec![a, b, c]);
        assert_eq!(ids(lib.tracks_to_analyze(Some(child), true, 100).unwrap()), vec![b]);
        assert_eq!(ids(lib.tracks_to_analyze(None, true, 2).unwrap()), vec![a, b]);

        let paths = lib.tracks_to_analyze(Some(child), false, 100).unwrap();
        assert_eq!(paths, vec![(b, "/m/b.mp3".to_string())]);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn loudness_by_path_returns_known_loudness() {
        let (mut lib, db_path) = temp_db("loudness");
        let a = insert_track(&lib, "A", "/m/a.mp3", 200_000);
        let b = insert_track(&lib, "B", "/m/b.mp3", 200_000);

        lib.save_analysis(&auto_cues(a, 500, None, None, 195_000, 0.9)).unwrap();
        assert_eq!(lib.loudness_by_path("/m/a.mp3").unwrap(), Some((-14.25, Some(-1.5))));
        assert_eq!(lib.loudness_by_path("/m/b.mp3").unwrap(), None);
        assert_eq!(lib.loudness_by_path("/m/missing.mp3").unwrap(), None);

        let mut no_loudness = auto_cues(b, 500, None, None, 195_000, 0.9);
        no_loudness.loudness_lufs = None;
        no_loudness.peak_dbfs = None;
        lib.save_analysis(&no_loudness).unwrap();
        assert_eq!(lib.loudness_by_path("/m/b.mp3").unwrap(), None);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn analysis_stats_counts_enabled_tracks() {
        let (mut lib, db_path) = temp_db("stats");
        let auto = insert_track(&lib, "Auto", "/m/auto.mp3", 200_000);
        let manual = insert_track(&lib, "Manual", "/m/manual.mp3", 200_000);
        let disabled = insert_track(&lib, "Disabled", "/m/disabled.mp3", 200_000);
        let untouched = insert_track(&lib, "Untouched", "/m/untouched.mp3", 200_000);

        lib.save_analysis(&auto_cues(auto, 500, Some(12_500), Some(180_000), 195_000, 0.5))
            .unwrap();
        lib.set_track_cues(&manual_cues(manual)).unwrap();
        lib.save_analysis(&auto_cues(disabled, 500, Some(12_500), Some(180_000), 195_000, 0.5))
            .unwrap();
        lib.set_track_enabled(disabled, false).unwrap();

        let stats = lib.analysis_stats(0.7).unwrap();
        assert_eq!(
            stats,
            AnalysisStats {
                tracks: 3,
                analyzed: 1,
                manual: 1,
                with_intro: 2,
                with_outro: 2,
                low_confidence: 1,
            }
        );
        assert_eq!(lib.analysis_stats(0.4).unwrap().low_confidence, 0);
        assert!(untouched > 0);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn tracks_needing_review_lowest_confidence_first() {
        let (mut lib, db_path) = temp_db("review");
        let high = insert_track(&lib, "High", "/m/high.mp3", 200_000);
        let lowest = insert_track(&lib, "Lowest", "/m/lowest.mp3", 200_000);
        let middle = insert_track(&lib, "Middle", "/m/middle.mp3", 200_000);
        let low_middle = insert_track(&lib, "LowMiddle", "/m/low-middle.mp3", 200_000);
        let trusted = insert_track(&lib, "Trusted", "/m/trusted.mp3", 200_000);
        let manual = insert_track(&lib, "Manual", "/m/manual.mp3", 200_000);

        lib.save_analysis(&auto_cues(high, 500, None, None, 195_000, 0.8)).unwrap();
        lib.save_analysis(&auto_cues(lowest, 500, None, None, 195_000, 0.2)).unwrap();
        lib.save_analysis(&auto_cues(middle, 500, None, None, 195_000, 0.5)).unwrap();
        lib.save_analysis(&auto_cues(low_middle, 500, None, None, 195_000, 0.6)).unwrap();
        lib.save_analysis(&auto_cues(trusted, 500, None, None, 195_000, 0.95)).unwrap();
        let mut manual_marks = manual_cues(manual);
        manual_marks.confidence = 0.1;
        lib.set_track_cues(&manual_marks).unwrap();

        let review = lib.tracks_needing_review(0.7, 10).unwrap();
        assert_eq!(
            review.iter().map(|cues| cues.track_id).collect::<Vec<_>>(),
            vec![lowest, middle, low_middle]
        );
        assert_eq!(
            review.iter().map(|cues| cues.confidence).collect::<Vec<_>>(),
            vec![0.2, 0.5, 0.6]
        );
        assert_eq!(
            lib.tracks_needing_review(0.7, 2).unwrap().iter().map(|c| c.track_id).collect::<Vec<_>>(),
            vec![lowest, middle]
        );

        drop(lib);
        remove_db_files(&db_path);
    }
}
