//! SQLite storage for automation: categories, rules, clocks, the grid, settings
//! and the proof-of-play log.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use rusqlite::{params, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::audio::engine::{EndReason, PlayRecord};
use crate::library::db::{fold, MediaLibrary};

use super::model::{
    AutomationConfig, CandidateRow, Category, CategoryRule, Clock, GridRule, PlayLogEntry, RuleField,
    TrackFilter,
};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

type CategoryRaw = (i64, String, String, String, Option<i64>, Option<String>, Option<String>);
type CandidateRaw = (i64, String, String, String, i64, f64, Option<String>);
type SmartRaw = (i64, String, String, String, i64, String, String, Option<i32>, Option<i32>, Option<String>);
type PlayLogRaw = (
    i64,
    String,
    String,
    Option<i64>,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    i64,
    String,
    i64,
);

pub(crate) fn init_schema(db: &rusqlite::Connection) -> Result<(), String> {
    let columns = [
        ("year", "INTEGER"),
        ("energy", "INTEGER"),
        ("intro_ms", "INTEGER NOT NULL DEFAULT 0"),
        ("outro_ms", "INTEGER NOT NULL DEFAULT 0"),
        ("enabled", "INTEGER NOT NULL DEFAULT 1"),
        ("last_played_at", "TEXT"),
        ("play_count", "INTEGER NOT NULL DEFAULT 0"),
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

    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS categories (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT NOT NULL,
            kind            TEXT NOT NULL,
            color           TEXT NOT NULL DEFAULT '',
            parent_id       INTEGER REFERENCES categories(id) ON DELETE SET NULL,
            filter_json     TEXT,
            separation_json TEXT
        );
        CREATE TABLE IF NOT EXISTS track_categories (
            track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
            weight      REAL NOT NULL DEFAULT 1.0,
            PRIMARY KEY (track_id, category_id)
        );
        CREATE INDEX IF NOT EXISTS idx_track_categories_category ON track_categories(category_id);
        CREATE TABLE IF NOT EXISTS track_codes (
            track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            code     TEXT NOT NULL,
            PRIMARY KEY (track_id, code)
        );
        CREATE TABLE IF NOT EXISTS category_rules (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
            field       TEXT NOT NULL,
            pattern     TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS clocks (
            id        TEXT PRIMARY KEY,
            data_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS grid_rules (
            id        TEXT PRIMARY KEY,
            data_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_kv (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS play_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            aired_at    TEXT NOT NULL,
            ended_at    TEXT NOT NULL,
            track_id    INTEGER,
            file_path   TEXT NOT NULL,
            title       TEXT NOT NULL DEFAULT '',
            artist      TEXT NOT NULL DEFAULT '',
            kind        TEXT NOT NULL,
            category_id INTEGER,
            campaign_id INTEGER,
            planned_at  TEXT,
            played_ms   INTEGER NOT NULL,
            reason      TEXT NOT NULL,
            complete    INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_play_log_aired ON play_log(aired_at);
        CREATE INDEX IF NOT EXISTS idx_play_log_campaign ON play_log(campaign_id, aired_at);",
    )
    .map_err(|e| format!("DB error: {e}"))?;
    crate::automation::traffic_store::init_traffic_schema(db)?;
    crate::analysis::store::init_cues_schema(db)?;
    Ok(())
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

fn parse_datetime(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text, DATETIME_FORMAT).map_err(|e| format!("DB error: {e}"))
}

fn parse_optional_datetime(text: Option<String>) -> Result<Option<NaiveDateTime>, String> {
    text.map(|value| parse_datetime(&value)).transpose()
}

fn optional_json<T: Serialize>(value: &Option<T>) -> Result<Option<String>, String> {
    value
        .as_ref()
        .map(|value| serde_json::to_string(value).map_err(|e| format!("DB error: {e}")))
        .transpose()
}

fn category_from_raw(raw: CategoryRaw) -> Result<Category, String> {
    let (id, name, kind, color, parent_id, filter_json, separation_json) = raw;
    Ok(Category {
        id,
        name,
        kind: enum_from_text(&kind)?,
        color,
        parent_id,
        filter: filter_json
            .map(|json| serde_json::from_str(&json).map_err(|e| format!("DB error: {e}")))
            .transpose()?,
        separation: separation_json
            .map(|json| serde_json::from_str(&json).map_err(|e| format!("DB error: {e}")))
            .transpose()?,
    })
}

fn normalize_slashes(text: &str) -> String {
    fold(&text.replace('\\', "/"))
}

fn rule_matches(field: RuleField, pattern: &str, genre: &str, artist: &str, file_path: &str) -> bool {
    match field {
        RuleField::Genre => fold(genre) == fold(pattern),
        RuleField::Artist => fold(artist) == fold(pattern),
        RuleField::Folder => normalize_slashes(file_path).starts_with(&normalize_slashes(pattern)),
    }
}

fn filter_matches(
    filter: &TrackFilter,
    genre: &str,
    artist: &str,
    file_path: &str,
    year: Option<i32>,
    energy: Option<i32>,
    duration_ms: u64,
) -> bool {
    if let Some(expected) = &filter.genre {
        if fold(genre) != fold(expected) {
            return false;
        }
    }
    if let Some(expected) = &filter.artist {
        if fold(artist) != fold(expected) {
            return false;
        }
    }
    if let Some(folder) = &filter.folder {
        if !normalize_slashes(file_path).starts_with(&normalize_slashes(folder)) {
            return false;
        }
    }
    if let Some(from) = filter.year_from {
        if !year.is_some_and(|value| value >= from) {
            return false;
        }
    }
    if let Some(to) = filter.year_to {
        if !year.is_some_and(|value| value <= to) {
            return false;
        }
    }
    if let Some(min) = filter.energy_min {
        if !energy.is_some_and(|value| value >= min as i32) {
            return false;
        }
    }
    if let Some(max) = filter.energy_max {
        if !energy.is_some_and(|value| value <= max as i32) {
            return false;
        }
    }
    if let Some(min) = filter.min_duration_ms {
        if duration_ms < min {
            return false;
        }
    }
    if let Some(max) = filter.max_duration_ms {
        if duration_ms > max {
            return false;
        }
    }
    true
}

fn play_log_from_raw(raw: PlayLogRaw) -> Result<PlayLogEntry, String> {
    let (
        id,
        aired_at,
        ended_at,
        track_id,
        file_path,
        title,
        artist,
        kind,
        category_id,
        campaign_id,
        planned_at,
        played_ms,
        reason,
        complete,
    ) = raw;
    Ok(PlayLogEntry {
        id,
        aired_at: parse_datetime(&aired_at)?,
        ended_at: parse_datetime(&ended_at)?,
        track_id,
        file_path,
        title,
        artist,
        kind: enum_from_text(&kind)?,
        category_id,
        campaign_id,
        planned_at: parse_optional_datetime(planned_at)?,
        played_ms: played_ms.max(0) as u64,
        reason: enum_from_text(&reason)?,
        complete: complete != 0,
    })
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

impl MediaLibrary {
    pub fn list_categories(&self) -> Result<Vec<Category>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, name, kind, color, parent_id, filter_json, separation_json
                 FROM categories ORDER BY name",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| -> rusqlite::Result<CategoryRaw> {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let raw = row.map_err(|e| format!("DB error: {e}"))?;
            category_from_raw(raw)
        })
        .collect()
    }

    pub fn create_category(&mut self, category: &Category) -> Result<i64, String> {
        let kind = enum_text(&category.kind)?;
        let filter = optional_json(&category.filter)?;
        let separation = optional_json(&category.separation)?;
        self.conn()
            .execute(
                "INSERT INTO categories (name, kind, color, parent_id, filter_json, separation_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![category.name, kind, category.color, category.parent_id, filter, separation],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn update_category(&mut self, category: &Category) -> Result<(), String> {
        let exists = self
            .conn()
            .query_row("SELECT 1 FROM categories WHERE id = ?1", params![category.id], |_| Ok(()))
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        if exists.is_none() {
            return Err(format!("No category {}", category.id));
        }
        if let Some(parent) = category.parent_id {
            if parent == category.id {
                return Err("A category cannot be its own parent".to_string());
            }
            let mut current = Some(parent);
            while let Some(id) = current {
                if id == category.id {
                    return Err("A category cannot be moved under its own descendant".to_string());
                }
                current = self
                    .conn()
                    .query_row("SELECT parent_id FROM categories WHERE id = ?1", params![id], |row| {
                        row.get::<_, Option<i64>>(0)
                    })
                    .optional()
                    .map_err(|e| format!("DB error: {e}"))?
                    .flatten();
            }
        }
        let kind = enum_text(&category.kind)?;
        let filter = optional_json(&category.filter)?;
        let separation = optional_json(&category.separation)?;
        self.conn()
            .execute(
                "UPDATE categories SET name = ?1, kind = ?2, color = ?3, parent_id = ?4,
                                       filter_json = ?5, separation_json = ?6
                 WHERE id = ?7",
                params![
                    category.name,
                    kind,
                    category.color,
                    category.parent_id,
                    filter,
                    separation,
                    category.id
                ],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision(); // a parent or filter change changes candidates
        Ok(())
    }

    pub fn delete_category(&mut self, id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM categories WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(rows > 0)
    }

    pub fn add_tracks_to_category(
        &mut self,
        category_id: i64,
        track_ids: &[i64],
        weight: f32,
    ) -> Result<usize, String> {
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        let mut written = 0;
        for &track_id in track_ids {
            written += tx
                .execute(
                    "INSERT OR REPLACE INTO track_categories (track_id, category_id, weight)
                     VALUES (?1, ?2, ?3)",
                    params![track_id, category_id, weight as f64],
                )
                .map_err(|e| format!("DB error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(written)
    }

    pub fn remove_tracks_from_category(
        &mut self,
        category_id: i64,
        track_ids: &[i64],
    ) -> Result<usize, String> {
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        let mut removed = 0;
        for &track_id in track_ids {
            removed += tx
                .execute(
                    "DELETE FROM track_categories WHERE track_id = ?1 AND category_id = ?2",
                    params![track_id, category_id],
                )
                .map_err(|e| format!("DB error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(removed)
    }

    pub fn track_categories(&self, track_id: i64) -> Result<Vec<(i64, f32)>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT category_id, weight FROM track_categories WHERE track_id = ?1")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![track_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)? as f32))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("DB error: {e}"))
    }

    pub fn set_track_codes(&mut self, track_id: i64, codes: &[String]) -> Result<(), String> {
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        tx.execute("DELETE FROM track_codes WHERE track_id = ?1", params![track_id])
            .map_err(|e| format!("DB error: {e}"))?;
        for code in codes {
            let code = code.trim().to_lowercase();
            if code.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO track_codes (track_id, code) VALUES (?1, ?2)",
                params![track_id, code],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(())
    }

    pub fn list_codes(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT DISTINCT code FROM track_codes ORDER BY code")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("DB error: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("DB error: {e}"))
    }

    pub fn set_track_enabled(&mut self, track_id: i64, enabled: bool) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE tracks SET enabled = ?1 WHERE id = ?2",
                params![enabled as i64, track_id],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(())
    }

    pub fn list_category_rules(&self) -> Result<Vec<CategoryRule>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, category_id, field, pattern FROM category_rules ORDER BY id")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let (id, category_id, field, pattern) = row.map_err(|e| format!("DB error: {e}"))?;
            Ok(CategoryRule { id, category_id, field: enum_from_text(&field)?, pattern })
        })
        .collect()
    }

    pub fn add_category_rule(&mut self, rule: &CategoryRule) -> Result<i64, String> {
        let field = enum_text(&rule.field)?;
        self.conn()
            .execute(
                "INSERT INTO category_rules (category_id, field, pattern) VALUES (?1, ?2, ?3)",
                params![rule.category_id, field, rule.pattern],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn delete_category_rule(&mut self, id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM category_rules WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(rows > 0)
    }

    pub fn apply_category_rules(&mut self) -> Result<usize, String> {
        let rules = self.list_category_rules()?;
        let mut stmt = self
            .conn()
            .prepare("SELECT id, genre, artist, file_path FROM tracks")
            .map_err(|e| format!("DB error: {e}"))?;
        let tracks = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DB error: {e}"))?;
        drop(stmt);

        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        let mut inserted = 0;
        for (track_id, genre, artist, file_path) in &tracks {
            for rule in &rules {
                if rule_matches(rule.field, &rule.pattern, genre, artist, file_path) {
                    inserted += tx
                        .execute(
                            "INSERT OR IGNORE INTO track_categories (track_id, category_id, weight)
                             VALUES (?1, ?2, 1.0)",
                            params![track_id, rule.category_id],
                        )
                        .map_err(|e| format!("DB error: {e}"))?;
                }
            }
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(inserted)
    }

    pub fn category_candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String> {
        let filter_json: Option<String> = self
            .conn()
            .query_row(
                "SELECT filter_json FROM categories WHERE id = ?1",
                params![category_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?
            .flatten();
        let candidates = match filter_json {
            Some(json) => {
                let filter: TrackFilter =
                    serde_json::from_str(&json).map_err(|e| format!("DB error: {e}"))?;
                self.smart_candidates(&filter)?
            }
            None => self.assigned_candidates(category_id)?,
        };
        Ok(candidates)
    }

    fn assigned_candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "WITH RECURSIVE tree(id) AS (
                    SELECT id FROM categories WHERE id = ?1
                    UNION
                    SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id
                 )
                 SELECT t.id, t.title, t.artist, t.album, t.duration_ms, tc.weight, t.last_played_at
                 FROM tracks t
                 JOIN track_categories tc ON tc.track_id = t.id
                 JOIN tree ON tree.id = tc.category_id
                 WHERE t.enabled = 1",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![category_id], |row| -> rusqlite::Result<CandidateRaw> {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;

        let mut by_track: HashMap<i64, CandidateRow> = HashMap::new();
        for row in rows {
            let (track_id, title, artist, album, duration_ms, weight, last_played_at) =
                row.map_err(|e| format!("DB error: {e}"))?;
            let last_played_at = parse_optional_datetime(last_played_at)?;
            let weight = weight as f32;
            match by_track.get_mut(&track_id) {
                Some(existing) => {
                    if weight > existing.weight {
                        existing.weight = weight;
                    }
                }
                None => {
                    by_track.insert(
                        track_id,
                        CandidateRow {
                            track_id,
                            title,
                            artist,
                            album,
                            duration_ms: duration_ms.max(0) as u64,
                            weight,
                            last_played_at,
                            codes: Vec::new(),
                        },
                    );
                }
            }
        }

        let mut candidates: Vec<CandidateRow> = by_track.into_values().collect();
        candidates.sort_by(|a, b| a.artist.cmp(&b.artist).then_with(|| a.title.cmp(&b.title)));
        let mut codes = self.codes_by_track()?;
        for candidate in &mut candidates {
            candidate.codes = codes.remove(&candidate.track_id).unwrap_or_default();
        }
        Ok(candidates)
    }

    fn smart_candidates(&self, filter: &TrackFilter) -> Result<Vec<CandidateRow>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, title, artist, album, duration_ms, genre, file_path, year, energy,
                        last_played_at
                 FROM tracks WHERE enabled = 1",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| -> rusqlite::Result<SmartRaw> {
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
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;

        let mut candidates = Vec::new();
        for row in rows {
            let (track_id, title, artist, album, duration_ms, genre, file_path, year, energy, last_played_at) =
                row.map_err(|e| format!("DB error: {e}"))?;
            let duration_ms = duration_ms.max(0) as u64;
            if !filter_matches(filter, &genre, &artist, &file_path, year, energy, duration_ms) {
                continue;
            }
            candidates.push(CandidateRow {
                track_id,
                title,
                artist,
                album,
                duration_ms,
                weight: 1.0,
                last_played_at: parse_optional_datetime(last_played_at)?,
                codes: Vec::new(),
            });
        }

        candidates.sort_by(|a, b| a.artist.cmp(&b.artist).then_with(|| a.title.cmp(&b.title)));
        let mut codes = self.codes_by_track()?;
        for candidate in &mut candidates {
            candidate.codes = codes.remove(&candidate.track_id).unwrap_or_default();
        }
        Ok(candidates)
    }

    fn codes_by_track(&self) -> Result<HashMap<i64, Vec<String>>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT track_id, code FROM track_codes ORDER BY track_id, code")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| format!("DB error: {e}"))?;
        let mut codes: HashMap<i64, Vec<String>> = HashMap::new();
        for row in rows {
            let (track_id, code) = row.map_err(|e| format!("DB error: {e}"))?;
            codes.entry(track_id).or_default().push(code);
        }
        Ok(codes)
    }

    pub fn list_clocks(&self) -> Result<Vec<Clock>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT data_json FROM clocks")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("DB error: {e}"))?;
        let mut clocks = rows
            .map(|row| {
                let json = row.map_err(|e| format!("DB error: {e}"))?;
                serde_json::from_str::<Clock>(&json).map_err(|e| format!("DB error: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        clocks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(clocks)
    }

    pub fn save_clock(&mut self, clock: &Clock) -> Result<(), String> {
        let json = serde_json::to_string(clock).map_err(|e| format!("DB error: {e}"))?;
        self.conn()
            .execute(
                "INSERT INTO clocks (id, data_json) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET data_json = excluded.data_json",
                params![clock.id, json],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(())
    }

    pub fn delete_clock(&mut self, id: &str) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM clocks WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(rows > 0)
    }

    pub fn list_grid_rules(&self) -> Result<Vec<GridRule>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT data_json FROM grid_rules ORDER BY id")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let json = row.map_err(|e| format!("DB error: {e}"))?;
            serde_json::from_str::<GridRule>(&json).map_err(|e| format!("DB error: {e}"))
        })
        .collect()
    }

    pub fn save_grid_rule(&mut self, rule: &GridRule) -> Result<(), String> {
        let json = serde_json::to_string(rule).map_err(|e| format!("DB error: {e}"))?;
        self.conn()
            .execute(
                "INSERT INTO grid_rules (id, data_json) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET data_json = excluded.data_json",
                params![rule.id, json],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(())
    }

    pub fn delete_grid_rule(&mut self, id: &str) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM grid_rules WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(rows > 0)
    }

    pub fn automation_config(&self) -> Result<AutomationConfig, String> {
        match self.kv_get("config")? {
            Some(json) => serde_json::from_str(&json).map_err(|e| format!("DB error: {e}")),
            None => Ok(AutomationConfig::default()),
        }
    }

    pub fn save_automation_config(&mut self, config: &AutomationConfig) -> Result<(), String> {
        let json = serde_json::to_string(config).map_err(|e| format!("DB error: {e}"))?;
        self.kv_set("config", &json)
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row("SELECT value FROM automation_kv WHERE key = ?1", params![key], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| format!("DB error: {e}"))
    }

    pub fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "INSERT INTO automation_kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(())
    }

    pub fn log_plays(&mut self, records: &[PlayRecord]) -> Result<(), String> {
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        for record in records {
            let played_ms = (record.ended_at - record.started_at).num_milliseconds().max(0) as u64;
            let complete = record.reason == EndReason::Finished
                || (record.item.duration_ms > 0
                    && played_ms >= record.item.duration_ms.saturating_mul(95) / 100);
            let track_id = match record.item.track_id {
                Some(track_id) => Some(track_id),
                None => tx
                    .query_row(
                        "SELECT id FROM tracks WHERE file_path = ?1",
                        params![record.item.track_path],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|e| format!("DB error: {e}"))?,
            };
            let kind = enum_text(&record.item.kind)?;
            let reason = enum_text(&record.reason)?;
            let aired_at = format_datetime(record.started_at);
            tx.execute(
                "INSERT INTO play_log
                    (aired_at, ended_at, track_id, file_path, title, artist, kind, category_id,
                     campaign_id, planned_at, played_ms, reason, complete)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    aired_at,
                    format_datetime(record.ended_at),
                    track_id,
                    record.item.track_path,
                    record.item.title,
                    record.item.artist,
                    kind,
                    record.item.category_id,
                    record.item.campaign_id,
                    record.item.planned_at.map(format_datetime),
                    played_ms as i64,
                    reason,
                    complete as i64
                ],
            )
            .map_err(|e| format!("DB error: {e}"))?;
            if complete {
                if let Some(track_id) = track_id {
                    tx.execute(
                        "UPDATE tracks SET last_played_at = ?1, play_count = play_count + 1
                         WHERE id = ?2",
                        params![aired_at, track_id],
                    )
                    .map_err(|e| format!("DB error: {e}"))?;
                }
            }
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        Ok(())
    }

    pub fn play_log(
        &self,
        from: NaiveDateTime,
        to: NaiveDateTime,
        limit: usize,
    ) -> Result<Vec<PlayLogEntry>, String> {
        let limit = limit.min(i64::MAX as usize) as i64;
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, aired_at, ended_at, track_id, file_path, title, artist, kind,
                        category_id, campaign_id, planned_at, played_ms, reason, complete
                 FROM play_log
                 WHERE aired_at >= ?1 AND aired_at < ?2
                 ORDER BY aired_at, id
                 LIMIT ?3",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(
                params![format_datetime(from), format_datetime(to), limit],
                |row| -> rusqlite::Result<PlayLogRaw> {
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
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                    ))
                },
            )
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let raw = row.map_err(|e| format!("DB error: {e}"))?;
            play_log_from_raw(raw)
        })
        .collect()
    }

    pub fn play_log_csv(&self, from: NaiveDateTime, to: NaiveDateTime) -> Result<String, String> {
        let entries = self.play_log(from, to, usize::MAX)?;
        let mut csv = String::from(
            "aired_at,ended_at,kind,title,artist,file_path,category_id,campaign_id,planned_at,played_seconds,complete,reason\r\n",
        );
        for entry in entries {
            let fields = [
                format_datetime(entry.aired_at),
                format_datetime(entry.ended_at),
                enum_text(&entry.kind)?,
                entry.title,
                entry.artist,
                entry.file_path,
                entry.category_id.map(|id| id.to_string()).unwrap_or_default(),
                entry.campaign_id.map(|id| id.to_string()).unwrap_or_default(),
                entry.planned_at.map(format_datetime).unwrap_or_default(),
                (entry.played_ms / 1000).to_string(),
                if entry.complete { "true".to_string() } else { "false".to_string() },
                enum_text(&entry.reason)?,
            ];
            csv.push_str(&fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","));
            csv.push_str("\r\n");
        }
        Ok(csv)
    }

    pub fn purge_play_log(&mut self, before: NaiveDateTime) -> Result<usize, String> {
        self.conn()
            .execute("DELETE FROM play_log WHERE aired_at < ?1", params![format_datetime(before)])
            .map_err(|e| format!("DB error: {e}"))
    }
}

impl super::generator::Catalog for MediaLibrary {
    fn revision(&self) -> u64 {
        MediaLibrary::revision(self)
    }
    fn automation_config(&self) -> Result<AutomationConfig, String> {
        MediaLibrary::automation_config(self)
    }
    fn categories(&self) -> Result<Vec<Category>, String> {
        self.list_categories()
    }
    fn clocks(&self) -> Result<Vec<Clock>, String> {
        self.list_clocks()
    }
    fn grid_rules(&self) -> Result<Vec<GridRule>, String> {
        self.list_grid_rules()
    }
    fn candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String> {
        self.category_candidates(category_id)
    }
    fn track(&self, id: i64) -> Result<Option<crate::library::db::Track>, String> {
        self.get_track(id)
    }
    fn playlist(&self, id_or_name: &str) -> Result<crate::playlist::playlist::Playlist, String> {
        self.load_playlist(id_or_name)
    }
    fn recent_plays(&self, since: NaiveDateTime) -> Result<Vec<(Option<i64>, String, NaiveDateTime)>, String> {
        let until = since + chrono::Duration::days(3650);
        Ok(self
            .play_log(since, until, 10_000)?
            .into_iter()
            .filter(|e| e.complete)
            .map(|e| (e.track_id, e.artist, e.aired_at))
            .collect())
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>, String> {
        MediaLibrary::kv_get(self, key)
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        MediaLibrary::kv_set(self, key, value)
    }
    fn campaigns(&self) -> Result<Vec<super::model::Campaign>, String> {
        self.list_campaigns()
    }
    fn campaign_plays(&self, from: NaiveDateTime, to: NaiveDateTime) -> Result<Vec<(i64, NaiveDateTime)>, String> {
        MediaLibrary::campaign_plays(self, from, to)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::{NaiveDate, NaiveTime};

    use super::*;
    use crate::automation::model::GridTarget;
    use crate::playlist::playlist::{ItemKind, PlaylistItem};

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
        let path = std::env::temp_dir().join(format!("riars-store-{}-{name}.db", std::process::id()));
        remove_db_files(&path);
        let lib = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        (lib, path)
    }

    fn insert_track(lib: &MediaLibrary, title: &str, artist: &str, genre: &str, file_path: &str) -> i64 {
        lib.conn()
            .execute(
                "INSERT INTO tracks (title, artist, album, genre, duration_ms, file_path)
                 VALUES (?1, ?2, '', ?3, 1000, ?4)",
                params![title, artist, genre, file_path],
            )
            .unwrap();
        lib.conn().last_insert_rowid()
    }

    fn set_track_fields(lib: &MediaLibrary, id: i64, year: Option<i32>, energy: Option<i32>) {
        lib.conn()
            .execute(
                "UPDATE tracks SET year = ?1, energy = ?2 WHERE id = ?3",
                params![year, energy, id],
            )
            .unwrap();
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

    fn datetime(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, s)
            .unwrap()
    }

    fn time(h: u32, min: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, min, s).unwrap()
    }

    #[test]
    fn category_crud_and_cycle() {
        let (mut lib, db_path) = temp_db("categories");

        let mut parent = category("Music", None);
        let parent_id = lib.create_category(&parent).unwrap();
        parent.id = parent_id;
        let mut child = category("Rock", Some(parent_id));
        let child_id = lib.create_category(&child).unwrap();
        child.id = child_id;

        let listed = lib.list_categories().unwrap();
        assert_eq!(listed.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Music", "Rock"]);
        assert_eq!(listed[1].parent_id, Some(parent_id));

        let mut self_parent = parent.clone();
        self_parent.parent_id = Some(parent_id);
        assert!(lib.update_category(&self_parent).is_err());

        let mut cyclic = parent.clone();
        cyclic.parent_id = Some(child_id);
        assert!(lib.update_category(&cyclic).is_err());

        let mut missing = parent.clone();
        missing.id = 9999;
        assert!(lib.update_category(&missing).is_err());

        child.name = "Rock and Pop".to_string();
        lib.update_category(&child).unwrap();
        assert!(lib.list_categories().unwrap().iter().any(|c| c.name == "Rock and Pop"));

        assert!(lib.delete_category(child_id).unwrap());
        assert!(!lib.delete_category(child_id).unwrap());
        assert_eq!(lib.list_categories().unwrap().len(), 1);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn subcategory_candidates_use_max_weight() {
        let (mut lib, db_path) = temp_db("candidates");

        let parent_id = lib.create_category(&category("Music", None)).unwrap();
        let child_id = lib.create_category(&category("Rock", Some(parent_id))).unwrap();
        let a = insert_track(&lib, "A", "Artist", "Rock", "/m/a.mp3");
        let b = insert_track(&lib, "B", "Artist", "Rock", "/m/b.mp3");
        let c = insert_track(&lib, "C", "Artist", "Rock", "/m/c.mp3");

        lib.add_tracks_to_category(parent_id, &[a], 1.0).unwrap();
        lib.add_tracks_to_category(child_id, &[a], 2.5).unwrap();
        lib.add_tracks_to_category(child_id, &[b], 0.5).unwrap();
        lib.add_tracks_to_category(parent_id, &[c], 3.0).unwrap();
        lib.set_track_enabled(c, false).unwrap();

        let candidates = lib.category_candidates(parent_id).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates.iter().find(|row| row.track_id == a).unwrap().weight, 2.5);
        assert_eq!(candidates.iter().find(|row| row.track_id == b).unwrap().weight, 0.5);
        assert!(candidates.iter().all(|row| row.track_id != c));

        assert_eq!(lib.remove_tracks_from_category(child_id, &[b]).unwrap(), 1);
        assert_eq!(lib.category_candidates(parent_id).unwrap().len(), 1);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn smart_filters_fold_and_year() {
        let (mut lib, db_path) = temp_db("smart");

        let rock = insert_track(&lib, "Rock One", "Artist A", "Rock", "/m/1.mp3");
        let rock_lower = insert_track(&lib, "Rock Two", "Artist B", "rock", "/m/2.mp3");
        let cancion = insert_track(&lib, "Cancion", "Artist C", "Canción", "/m/3.mp3");
        let pop = insert_track(&lib, "Pop", "Artist D", "Pop", "/m/4.mp3");
        let unknown = insert_track(&lib, "Unknown", "Artist E", "Jazz", "/m/5.mp3");
        set_track_fields(&lib, rock, Some(2000), Some(5));
        set_track_fields(&lib, rock_lower, Some(2010), Some(7));
        set_track_fields(&lib, cancion, Some(1995), Some(3));
        set_track_fields(&lib, pop, Some(2020), Some(9));

        let mut by_genre = category("Smart Rock", None);
        by_genre.filter = Some(TrackFilter { genre: Some("ROCK".to_string()), ..Default::default() });
        let by_genre_id = lib.create_category(&by_genre).unwrap();
        let mut ids: Vec<i64> =
            lib.category_candidates(by_genre_id).unwrap().iter().map(|row| row.track_id).collect();
        ids.sort();
        let mut expected = vec![rock, rock_lower];
        expected.sort();
        assert_eq!(ids, expected);

        let mut accented = category("Smart Cancion", None);
        accented.filter = Some(TrackFilter { genre: Some("cancion".to_string()), ..Default::default() });
        let accented_id = lib.create_category(&accented).unwrap();
        let ids: Vec<i64> =
            lib.category_candidates(accented_id).unwrap().iter().map(|row| row.track_id).collect();
        assert_eq!(ids, vec![cancion]);

        let mut by_year = category("Smart Years", None);
        by_year.filter = Some(TrackFilter {
            year_from: Some(2000),
            year_to: Some(2010),
            ..Default::default()
        });
        let by_year_id = lib.create_category(&by_year).unwrap();
        let mut ids: Vec<i64> =
            lib.category_candidates(by_year_id).unwrap().iter().map(|row| row.track_id).collect();
        ids.sort();
        let mut expected = vec![rock, rock_lower];
        expected.sort();
        assert_eq!(ids, expected);
        assert!(!ids.contains(&unknown)); // no year: never matches a range

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn codes_are_trimmed_and_lowercased() {
        let (mut lib, db_path) = temp_db("codes");

        let track = insert_track(&lib, "A", "Artist", "Rock", "/m/a.mp3");
        lib.set_track_codes(
            track,
            &[" Main ".to_string(), "MAIN".to_string(), String::new(), "Hot".to_string()],
        )
        .unwrap();
        assert_eq!(lib.list_codes().unwrap(), vec!["hot", "main"]);

        lib.set_track_codes(track, &["new".to_string()]).unwrap();
        assert_eq!(lib.list_codes().unwrap(), vec!["new"]);

        let category_id = lib.create_category(&category("Cat", None)).unwrap();
        lib.add_tracks_to_category(category_id, &[track], 1.0).unwrap();
        let candidates = lib.category_candidates(category_id).unwrap();
        assert_eq!(candidates[0].codes, vec!["new"]);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn apply_folder_rules_normalises_slashes() {
        let (mut lib, db_path) = temp_db("rules");

        let category_id = lib.create_category(&category("Rock", None)).unwrap();
        let a = insert_track(&lib, "A", "Artist", "Rock", "D:/Music/Rock/a.mp3");
        let b = insert_track(&lib, "B", "Artist", "Rock", "D:\\Music\\Rock\\b.mp3");
        insert_track(&lib, "C", "Artist", "Rock", "D:/Music/Pop/c.mp3");

        let rule = CategoryRule {
            id: 0,
            category_id,
            field: RuleField::Folder,
            pattern: "D:\\music\\rock".to_string(),
        };
        let rule_id = lib.add_category_rule(&rule).unwrap();
        assert_eq!(lib.list_category_rules().unwrap()[0].id, rule_id);

        assert_eq!(lib.apply_category_rules().unwrap(), 2);
        assert_eq!(lib.apply_category_rules().unwrap(), 0);

        let mut ids: Vec<i64> =
            lib.category_candidates(category_id).unwrap().iter().map(|row| row.track_id).collect();
        ids.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(ids, expected);

        assert!(lib.delete_category_rule(rule_id).unwrap());
        assert!(!lib.delete_category_rule(rule_id).unwrap());

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn log_plays_and_csv_quoting() {
        let (mut lib, db_path) = temp_db("playlog");

        let track = insert_track(&lib, "Stored", "Artist", "Rock", "/m/a.mp3");
        let started = datetime(2024, 1, 2, 3, 4, 5);
        let ended = datetime(2024, 1, 2, 3, 4, 15);

        let mut item =
            PlaylistItem::new("/m/a.mp3".to_string(), "He said \"hi\", ok".to_string(), "Artist".to_string(), 10_000);
        item.track_id = Some(track);
        item.category_id = Some(7);
        item.campaign_id = Some(9);
        item.planned_at = Some(started);
        lib.log_plays(&[PlayRecord {
            item: item.clone(),
            started_at: started,
            ended_at: ended,
            reason: EndReason::Finished,
        }])
        .unwrap();

        let short_started = datetime(2024, 1, 2, 3, 5, 0);
        let short_ended = datetime(2024, 1, 2, 3, 5, 5);
        let mut short = PlaylistItem::new("/m/a.mp3".to_string(), "Short".to_string(), "Artist".to_string(), 10_000);
        short.track_id = None;
        lib.log_plays(&[PlayRecord {
            item: short,
            started_at: short_started,
            ended_at: short_ended,
            reason: EndReason::Skipped,
        }])
        .unwrap();

        let (play_count, last_played_at): (i64, Option<String>) = lib
            .conn()
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = ?1",
                params![track],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(play_count, 1);
        assert_eq!(last_played_at.as_deref(), Some("2024-01-02 03:04:05"));

        let entries = lib
            .play_log(started, datetime(2024, 1, 2, 4, 0, 0), 10)
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].played_ms, 10_000);
        assert!(entries[0].complete);
        assert_eq!(entries[0].track_id, Some(track));
        assert_eq!(entries[1].played_ms, 5_000);
        assert!(!entries[1].complete);
        assert_eq!(entries[1].track_id, Some(track));

        let csv = lib.play_log_csv(started, datetime(2024, 1, 2, 4, 0, 0)).unwrap();
        assert!(csv.starts_with(
            "aired_at,ended_at,kind,title,artist,file_path,category_id,campaign_id,planned_at,played_seconds,complete,reason\r\n"
        ));
        assert!(csv.contains("\"He said \"\"hi\"\", ok\""));
        assert!(csv.contains(",2024-01-02 03:04:05,10,true,Finished\r\n"));
        assert!(csv.ends_with("\r\n"));

        assert_eq!(lib.purge_play_log(started).unwrap(), 0);
        assert_eq!(lib.purge_play_log(datetime(2024, 1, 3, 0, 0, 0)).unwrap(), 2);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn clocks_grid_and_config_round_trip() {
        let (mut lib, db_path) = temp_db("settings");

        let clock = Clock {
            id: "c1".to_string(),
            name: "Morning".to_string(),
            color: "#fff".to_string(),
            duration_minutes: 60,
            slots: Vec::new(),
            fill_category: Some(3),
        };
        let afternoon = Clock {
            id: "c2".to_string(),
            name: "Afternoon".to_string(),
            color: String::new(),
            duration_minutes: 45,
            slots: Vec::new(),
            fill_category: None,
        };
        lib.save_clock(&afternoon).unwrap();
        lib.save_clock(&clock).unwrap();

        let clocks = lib.list_clocks().unwrap();
        assert_eq!(clocks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Afternoon", "Morning"]);
        assert_eq!(clocks[1], clock);

        let mut updated = clock.clone();
        updated.name = "Morning Show".to_string();
        lib.save_clock(&updated).unwrap();
        assert_eq!(lib.list_clocks().unwrap().len(), 2);
        assert!(lib.delete_clock("c1").unwrap());
        assert!(!lib.delete_clock("c1").unwrap());

        let grid = GridRule {
            id: "g1".to_string(),
            name: "Weekday mornings".to_string(),
            priority: 5,
            target: GridTarget::Clock { clock_id: "c2".to_string() },
            weekdays: 0x1f,
            months: 0xfff,
            weeks_of_month: 0x3f,
            start: time(6, 0, 0),
            end: time(12, 0, 0),
            date_from: None,
            date_to: None,
            enabled: true,
        };
        lib.save_grid_rule(&grid).unwrap();
        assert_eq!(lib.list_grid_rules().unwrap(), vec![grid]);
        assert!(lib.delete_grid_rule("g1").unwrap());
        assert!(!lib.delete_grid_rule("g1").unwrap());
        assert!(lib.list_grid_rules().unwrap().is_empty());

        assert_eq!(lib.automation_config().unwrap(), AutomationConfig::default());
        let mut config = AutomationConfig::default();
        config.enabled = true;
        config.min_items_ahead = 7;
        lib.save_automation_config(&config).unwrap();
        assert_eq!(lib.automation_config().unwrap(), config);
        assert!(lib.kv_get("config").unwrap().is_some());
        assert_eq!(lib.kv_get("missing").unwrap(), None);
        lib.kv_set("key", "value").unwrap();
        assert_eq!(lib.kv_get("key").unwrap().as_deref(), Some("value"));

        drop(lib);
        remove_db_files(&db_path);
    }
}
