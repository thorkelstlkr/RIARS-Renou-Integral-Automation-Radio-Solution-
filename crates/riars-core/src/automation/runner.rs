//! Runs the generator and the proof-of-play log next to the engine, from the control
//! loop of the server or the desktop app.
//!
//! Lock order: `tick` holds the `Automation` lock and takes the engine and library
//! locks one at a time. Callers must never lock `Automation` while holding the engine
//! or library lock.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use super::generator::Generator;
use crate::audio::engine::{play_index_unlocked, AudioEngine, PlaybackState};
use crate::library::db::MediaLibrary;

/// With nothing else happening, the lookahead is checked this often (the clock moves
/// on even when the playlist does not).
const CHECK_EVERY: Duration = Duration::from_secs(10);

pub struct Automation {
    generator: Generator,
    /// Engine playlist revision after the last tick.
    last_revision: Option<u64>,
    next_check: Instant,
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
        Self { generator: Generator::new(), last_revision: None, next_check: Instant::now() }
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
    let span = (remaining + ahead.iter().map(|i| length(i.duration_ms).saturating_sub(overlap)).sum::<u64>())
        .saturating_sub(if playing && !ahead.is_empty() { overlap } else { 0 });
    (now + chrono::Duration::milliseconds(span as i64), ahead.len(), ms_ahead, overlap)
}

/// Log what came off air, cut in for a hard anchor, and top up the on-air playlist.
/// Blocking (SQLite, and decoding for a hard anchor): run it from a worker thread.
pub fn tick(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    automation: &Mutex<Automation>,
    now: NaiveDateTime,
) -> Result<TickReport, String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let mut auto = automation.lock().map_err(|e| err(&e))?;
    let mut report = TickReport::default();
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
            if let Some(index) = index {
                log::info!("Automation: hard anchor, cutting to playlist item {index}");
                play_index_unlocked(engine, index, false)?;
                report.cut_to = Some(id);
            }
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
    }
    auto.last_revision = Some(engine.lock().map_err(|e| err(&e))?.playlist_revision());
    Ok(report)
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
        // Generated items do not trigger a session save on their own.
        drop(engine);
        drop(library);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
