//! Runs the generator and the proof-of-play log next to the engine, from the control
//! loop of the server or the desktop app.
//!
//! Lock order: `tick` holds the `Automation` lock and takes the engine and library
//! locks one at a time. Callers must never lock `Automation` while holding the engine
//! or library lock.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::generator::{Catalog, Generator, PeriodInfo};
use super::model::{AutomationConfig, CandidateRow, Campaign, Category, Clock, GridRule};
use crate::library::db::Track;
use crate::playlist::playlist::{Playlist, PlaylistItem};
use crate::audio::engine::{play_index_unlocked, AudioEngine, PlaybackState};
use crate::library::db::MediaLibrary;

/// With nothing else happening, the lookahead is checked this often (the clock moves
/// on even when the playlist does not).
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// Recent warnings kept for the status.
const MAX_WARNINGS: usize = 50;

pub struct Automation {
    generator: Generator,
    /// Engine playlist revision after the last tick.
    last_revision: Option<u64>,
    next_check: Instant,
    warnings: VecDeque<(NaiveDateTime, String)>,
}

/// What the automation is doing, for UIs and agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationStatus {
    pub enabled: bool,
    pub period: Option<PeriodInfo>,
    /// Recent problems, oldest first.
    pub warnings: Vec<(NaiveDateTime, String)>,
}

impl Default for Automation {
    fn default() -> Self {
        Self::new()
    }
}

/// What a tick did.
#[derive(Debug, Default)]
pub struct TickReport {
    /// Proof-of-play lines written.
    pub logged: usize,
    /// Items added to the on-air playlist.
    pub generated: usize,
    /// Problems the station should know about (empty category, missing clock...).
    pub warnings: Vec<String>,
    /// A hard anchor cut to this item.
    pub cut_to: Option<String>,
}

impl Automation {
    pub fn new() -> Self {
        Self { generator: Generator::new(), last_revision: None, next_check: Instant::now(), warnings: VecDeque::new() }
    }

    fn push_warning(&mut self, at: NaiveDateTime, warning: String) {
        if self.warnings.len() >= MAX_WARNINGS {
            self.warnings.pop_front();
        }
        self.warnings.push_back((at, warning));
    }

    pub fn status(&self) -> AutomationStatus {
        AutomationStatus {
            enabled: self.generator.config().enabled,
            period: self.generator.period(),
            warnings: self.warnings.iter().cloned().collect(),
        }
    }

    pub fn generator_mut(&mut self) -> &mut Generator {
        &mut self.generator
    }

    /// Settings, categories, clocks or the grid were edited: take them up now.
    pub fn invalidate(&mut self) {
        self.generator.invalidate();
        self.next_check = Instant::now();
    }

    /// Whether `tick` has work, from what the engine reports (read it first, then
    /// unlock the engine): records to log, a playlist change, or the periodic check.
    pub fn wants_tick(&self, playlist_revision: u64, has_play_records: bool) -> bool {
        has_play_records || self.last_revision != Some(playlist_revision) || Instant::now() >= self.next_check
    }
}

/// Where the generator should go on: when the last item ahead ends, how many items
/// and how much audio are ahead, and the crossfade overlap.
fn lookahead(engine: &AudioEngine, now: NaiveDateTime) -> (NaiveDateTime, usize, u64, u64) {
    let playlist = engine.playlist();
    let info = engine.state_info();
    let playing = info.state == PlaybackState::Playing;
    let overlap = if engine.crossfade_config().enabled { engine.crossfade_config().duration.as_millis() as u64 } else { 0 };
    let first = if playing { playlist.current_index + 1 } else { playlist.current_index };
    let ahead = playlist.items.get(first..).unwrap_or(&[]);
    let length = |ms: u64| if ms > 0 { ms } else { 180_000 };
    let ms_ahead: u64 = ahead.iter().map(|i| length(i.duration_ms)).sum();
    let remaining = if playing { info.duration_ms.saturating_sub(info.position_ms) } else { 0 };
    let overlap_of = |i: &PlaylistItem| if i.no_overlap { 0 } else { overlap };
    let span = (remaining + ahead.iter().map(|i| length(i.duration_ms).saturating_sub(overlap_of(i))).sum::<u64>())
        .saturating_sub(match (playing, ahead.first()) {
            (true, Some(first)) => overlap_of(first),
            _ => 0,
        });
    (now + chrono::Duration::milliseconds(span as i64), ahead.len(), ms_ahead, overlap)
}

/// Log what came off air, top up the on-air playlist, and cut in for a hard anchor.
/// Blocking (SQLite, and decoding for a hard anchor): run it from a worker thread.
/// Failures also go to the status warnings.
pub fn tick(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    automation: &Mutex<Automation>,
    now: NaiveDateTime,
) -> Result<TickReport, String> {
    let result = {
        let mut auto = automation.lock().map_err(|e| e.to_string())?;
        let result = tick_locked(engine, library, &mut auto, now);
        if let Err(e) = &result {
            auto.push_warning(now, format!("automation error: {e}"));
        }
        result
    };
    let (mut report, cut) = result?;
    // Decoded without the automation lock, so status requests and edits do not wait.
    if let Some((id, index)) = cut {
        log::info!("Automation: hard anchor, cutting to playlist item {index}");
        if let Err(e) = play_index_unlocked(engine, index, false) {
            if let Ok(mut auto) = automation.lock() {
                auto.push_warning(now, format!("hard anchor cut failed: {e}"));
            }
            return Err(e);
        }
        report.cut_to = Some(id);
    }
    Ok(report)
}

/// The part of `tick` that needs the automation; returns the hard-anchor cut to make.
fn tick_locked(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    auto: &mut Automation,
    now: NaiveDateTime,
) -> Result<(TickReport, Option<(String, usize)>), String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let mut report = TickReport::default();
    let mut cut = None;
    auto.next_check = Instant::now() + CHECK_EVERY;

    let records = engine.lock().map_err(|e| err(&e))?.take_play_records();
    if !records.is_empty() {
        library.lock().map_err(|e| err(&e))?.log_plays(&records)?;
        report.logged = records.len();
    }

    auto.generator.load(&*library.lock().map_err(|e| err(&e))?)?;
    let config = auto.generator.config().clone();
    if config.enabled {
        if let Some(id) = auto.generator.due_hard_anchor(now) {
            let index = {
                let engine = engine.lock().map_err(|e| err(&e))?;
                engine.playlist().index_of(&id).filter(|&i| i != engine.playlist().current_index)
            };
            cut = index.map(|index| (id, index));
        }

        let (start, items_ahead, ms_ahead, overlap) = lookahead(&*engine.lock().map_err(|e| err(&e))?, now);
        let min_items = config.min_items_ahead as usize;
        let min_ms = config.min_minutes_ahead as u64 * 60_000;
        if items_ahead < min_items || ms_ahead < min_ms {
            let items = {
                let mut library = library.lock().map_err(|e| err(&e))?;
                auto.generator.generate(
                    &mut *library,
                    start,
                    min_items.saturating_sub(items_ahead),
                    min_ms.saturating_sub(ms_ahead),
                    overlap,
                )?
            };
            report.generated = items.len();
            engine.lock().map_err(|e| err(&e))?.append_generated(items, config.keep_played as usize);
        }
        report.warnings = auto.generator.take_warnings();
        for warning in &report.warnings {
            auto.push_warning(now, warning.clone());
        }
    }
    auto.last_revision = Some(engine.lock().map_err(|e| err(&e))?.playlist_revision());
    Ok((report, cut))
}

/// The library, locked only for each query: a long preview does not hold it.
/// Playlist positions are kept in memory, so a preview changes nothing.
struct LockedCatalog<'a> {
    library: &'a Mutex<crate::library::db::MediaLibrary>,
    kv: HashMap<String, String>,
}

impl LockedCatalog<'_> {
    fn with<T>(&self, f: impl FnOnce(&crate::library::db::MediaLibrary) -> Result<T, String>) -> Result<T, String> {
        f(&*self.library.lock().map_err(|e| e.to_string())?)
    }
}

impl Catalog for LockedCatalog<'_> {
    fn revision(&self) -> u64 {
        self.library.lock().map_or(0, |l| l.revision())
    }
    fn automation_config(&self) -> Result<AutomationConfig, String> {
        self.with(|l| l.automation_config())
    }
    fn categories(&self) -> Result<Vec<Category>, String> {
        self.with(|l| l.list_categories())
    }
    fn clocks(&self) -> Result<Vec<Clock>, String> {
        self.with(|l| l.list_clocks())
    }
    fn grid_rules(&self) -> Result<Vec<GridRule>, String> {
        self.with(|l| l.list_grid_rules())
    }
    fn candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String> {
        self.with(|l| l.category_candidates(category_id))
    }
    fn track(&self, id: i64) -> Result<Option<Track>, String> {
        self.with(|l| l.get_track(id))
    }
    fn playlist(&self, id_or_name: &str) -> Result<Playlist, String> {
        self.with(|l| l.load_playlist(id_or_name))
    }
    fn recent_plays(&self, since: NaiveDateTime) -> Result<Vec<(Option<i64>, String, NaiveDateTime)>, String> {
        self.with(|l| Catalog::recent_plays(l, since))
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>, String> {
        match self.kv.get(key) {
            Some(value) => Ok(Some(value.clone())),
            None => self.with(|l| l.kv_get(key)),
        }
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn campaigns(&self) -> Result<Vec<Campaign>, String> {
        self.with(|l| l.list_campaigns())
    }
    fn campaign_plays(&self, from: NaiveDateTime, to: NaiveDateTime) -> Result<Vec<(i64, NaiveDateTime)>, String> {
        self.with(|l| l.campaign_plays(from, to))
    }
    fn track_cues(&self, track_id: i64) -> Result<Option<crate::analysis::TrackCues>, String> {
        self.with(|l| l.track_cues(track_id))
    }
}

/// Longest preview, in hours.
pub const MAX_PREVIEW_HOURS: u32 = 48;

/// What the automation would put on air from `from` for `hours` (at most
/// `MAX_PREVIEW_HOURS`), whether or not it is enabled, with `overlap_ms` of crossfade
/// between items. Writes nothing; the real playlist may differ in its random choices.
/// Blocking: run it on a worker thread.
pub fn preview(
    library: &Mutex<crate::library::db::MediaLibrary>,
    from: NaiveDateTime,
    hours: u32,
    overlap_ms: u64,
) -> Result<Vec<PlaylistItem>, String> {
    let mut catalog = LockedCatalog { library, kv: HashMap::new() };
    let mut generator = Generator::with_seed(from.and_utc().timestamp() as u64 | 1);
    let until = from + chrono::Duration::hours(hours.clamp(1, MAX_PREVIEW_HOURS) as i64);
    let mut items = Vec::new();
    let mut t = from;
    while t < until {
        let batch = generator.generate(&mut catalog, t, 50, 0, overlap_ms)?;
        let Some(last) = batch.last() else { break };
        let length = if last.duration_ms > 0 { last.duration_ms } else { 180_000 };
        let overlap = if last.no_overlap { 0 } else { overlap_ms };
        t = last.planned_at.unwrap_or(t) + chrono::Duration::milliseconds(length.saturating_sub(overlap).max(1000) as i64);
        items.extend(batch.into_iter().filter(|i| i.planned_at.is_none_or(|at| at < until)));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::model::{AutomationConfig, Clock, ClockSlot, SlotItem};
    use crate::playlist::playlist::ItemKind;

    #[test]
    fn tick_fills_the_on_air_playlist_from_the_library() {
        let path = std::env::temp_dir().join(format!("riars-runner-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let category = library
            .create_category(&crate::automation::model::Category {
                id: 0,
                name: "Pop".into(),
                kind: ItemKind::Music,
                color: String::new(),
                parent_id: None,
                filter: None,
                separation: None,
            })
            .unwrap();
        let mut ids = Vec::new();
        for n in 0..30 {
            library
                .conn()
                .execute(
                    "INSERT INTO tracks (title, artist, duration_ms, file_path) VALUES (?1, ?2, 200000, ?3)",
                    rusqlite::params![format!("song {n}"), format!("artist {}", n % 10), format!("/m/{n}.mp3")],
                )
                .unwrap();
            ids.push(library.conn().last_insert_rowid());
        }
        library.add_tracks_to_category(category, &ids, 1.0).unwrap();
        library
            .save_clock(&Clock {
                id: "c".into(),
                name: "c".into(),
                color: String::new(),
                duration_minutes: 60,
                slots: vec![ClockSlot { at: None, item: SlotItem::Fill { category_id: category } }],
                fill_category: None,
            })
            .unwrap();
        library
            .save_automation_config(&AutomationConfig {
                enabled: true,
                default_clock: Some("c".into()),
                ..AutomationConfig::default()
            })
            .unwrap();

        let engine = Mutex::new(AudioEngine::new_headless().unwrap());
        let library = Mutex::new(library);
        let automation = Mutex::new(Automation::new());
        let now = chrono::Local::now().naive_local();
        let report = tick(&engine, &library, &automation, now).unwrap();
        // 15 minutes of 200 s songs (stopped: the current item counts as ahead).
        assert_eq!(report.generated, 5);
        let revision = engine.lock().unwrap().playlist_revision();
        assert!(!automation.lock().unwrap().wants_tick(revision, false));
        // Enough ahead: a second tick adds nothing.
        assert_eq!(tick(&engine, &library, &automation, now).unwrap().generated, 0);
        let engine = engine.lock().unwrap();
        assert!(engine.playlist().items.iter().all(|i| i.generated && i.track_id.is_some()));
        drop(engine);
        // A preview of two hours writes nothing and covers the span.
        let items = preview(&library, now, 2, 0).unwrap();
        let span = items.last().unwrap().planned_at.unwrap() - now;
        assert!(span > chrono::Duration::minutes(110) && span <= chrono::Duration::hours(2), "{span}");
        assert!(automation.lock().unwrap().status().enabled);
        drop(library);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
