//! The playlist generator: picks and orders tracks from the grid, the clocks and the
//! rotation rules, a little ahead of what is on air. Runs on the control side, never
//! on the audio thread.

use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

use chrono::{Duration, NaiveDateTime, NaiveTime, Timelike};

use super::grid;
use super::model::{
    AutomationConfig, Campaign, CandidateRow, Category, Clock, GridRule, GridTarget, Separation, SlotItem,
};
use super::traffic::{SpotPick, Traffic};
use crate::library::db::{fold, Track};
use crate::playlist::playlist::{ItemKind, OverlaySpec, Playlist, PlaylistItem};

/// What the generator reads from the library. `MediaLibrary` implements it; tests
/// use an in-memory one.
pub trait Catalog {
    /// Changes when tracks or their categories change.
    fn revision(&self) -> u64;
    fn automation_config(&self) -> Result<AutomationConfig, String>;
    fn categories(&self) -> Result<Vec<Category>, String>;
    fn clocks(&self) -> Result<Vec<Clock>, String>;
    fn grid_rules(&self) -> Result<Vec<GridRule>, String>;
    fn candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String>;
    fn track(&self, id: i64) -> Result<Option<Track>, String>;
    fn playlist(&self, id_or_name: &str) -> Result<Playlist, String>;
    /// Plays logged since `since` as (track id, artist, start), oldest first.
    fn recent_plays(&self, since: NaiveDateTime) -> Result<Vec<(Option<i64>, String, NaiveDateTime)>, String>;
    fn kv_get(&self, key: &str) -> Result<Option<String>, String>;
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn campaigns(&self) -> Result<Vec<Campaign>, String>;
    /// Complete spot plays as (campaign id, start) in `[from, to)`.
    fn campaign_plays(&self, from: NaiveDateTime, to: NaiveDateTime) -> Result<Vec<(i64, NaiveDateTime)>, String>;
    /// Cue points of a track, if analysed or marked.
    fn track_cues(&self, track_id: i64) -> Result<Option<crate::analysis::TrackCues>, String>;
}

/// An anchor may fire this early (the item before it ends a little before the time).
const EARLY_SECS: i64 = 30;
/// Within this distance of an anchor, songs are picked by length to land on it.
const BACKTIME_WINDOW_SECS: i64 = 12 * 60;
/// Among this many best-rotated songs, the one landing closest to the anchor wins.
const BACKTIME_CHOICES: usize = 12;
/// Planning length of items whose length is not known yet.
const UNKNOWN_DURATION_MS: u64 = 180_000;
/// "Last played" of a track never played, for rotation.
const NEVER_PLAYED_SECS: i64 = 30 * 86_400;
/// Separation is relaxed step by step, up to this level, before giving up.
const MAX_RELAX: u8 = 5;
const MAX_ITEMS_PER_CALL: usize = 100;
const MAX_WARNINGS: usize = 50;

/// Where the generator is: the clock period it fills and what chose it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PeriodInfo {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub clock_id: Option<String>,
    /// A saved playlist played by the grid instead of a clock.
    pub playlist_id: Option<String>,
    pub rule_id: Option<String>,
    /// Next clock slot to fill.
    pub slot: usize,
}

/// A track as rotation sees it: small, so a whole library fits in little RAM.
#[derive(Debug, Clone)]
struct Candidate {
    track_id: i64,
    duration_ms: u64,
    weight: f32,
    /// Hash of the folded artist / album; 0 when empty.
    artist: u64,
    album: u64,
    last_played: Option<NaiveDateTime>,
    codes: Vec<String>,
}

fn key(text: &str) -> u64 {
    let folded = fold(text.trim());
    if folded.is_empty() {
        return 0;
    }
    let mut hasher = DefaultHasher::new();
    folded.hash(&mut hasher);
    hasher.finish().max(1)
}

impl From<CandidateRow> for Candidate {
    fn from(row: CandidateRow) -> Self {
        Self {
            track_id: row.track_id,
            duration_ms: row.duration_ms,
            weight: row.weight.max(0.01),
            artist: key(&row.artist),
            album: key(&row.album),
            last_played: row.last_played_at,
            codes: row.codes.iter().map(|c| c.to_lowercase()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Source {
    Clock(String),
    Playlist(String),
    /// No grid rule and no default clock: fallback music, if any.
    Nothing,
}

/// Where the generator is in the current clock period.
#[derive(Debug, Clone)]
struct Cursor {
    start: NaiveDateTime,
    end: NaiveDateTime,
    source: Source,
    /// Grid rule that chose the source (for sponsorships).
    rule_id: Option<String>,
    slot: usize,
    /// Items made for the current slot.
    done: u32,
}

/// Result of one step through a clock.
enum Step {
    Item(PlaylistItem),
    /// The slot was used up or skipped: look again.
    Next,
    /// Nothing can be made.
    Stuck,
}

fn item_ms(item: &PlaylistItem) -> u64 {
    if item.duration_ms > 0 {
        item.duration_ms
    } else {
        UNKNOWN_DURATION_MS
    }
}

fn ms(ms: u64) -> Duration {
    Duration::milliseconds(ms as i64)
}

fn is_music(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Music | ItemKind::Program)
}

pub struct Generator {
    config: AutomationConfig,
    categories: HashMap<i64, Category>,
    clocks: HashMap<String, Clock>,
    grid: Vec<GridRule>,
    loaded: bool,
    cache: HashMap<i64, Vec<Candidate>>,
    cache_revision: Option<u64>,
    /// When each track, artist and album last played (or is planned to).
    last_played: HashMap<i64, NaiveDateTime>,
    artist_at: HashMap<u64, NaiveDateTime>,
    album_at: HashMap<u64, NaiveDateTime>,
    /// Last imaging elements of each category, newest last.
    imaging_recent: HashMap<i64, VecDeque<i64>>,
    history_loaded: bool,
    cursor: Option<Cursor>,
    /// Items already decided (a song after the jingle put before it).
    pending: VecDeque<PlaylistItem>,
    songs_since_jingle: u32,
    last_music_category: Option<i64>,
    last_was_music: bool,
    /// Position in saved playlists played by the grid or a clock.
    playlist_pos: HashMap<String, usize>,
    /// Items that must start at a fixed time, cutting what plays then.
    hard_anchors: Vec<(String, NaiveDateTime)>,
    warnings: Vec<String>,
    seed: u64,
    traffic: Traffic,
    /// Sponsorship spots to play before the first song of the period.
    opening: VecDeque<SpotPick>,
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

impl Generator {
    pub fn new() -> Self {
        Self::with_seed(chrono::Local::now().timestamp_nanos_opt().unwrap_or(1) as u64)
    }

    /// A generator whose random choices repeat (for tests and previews).
    pub fn with_seed(seed: u64) -> Self {
        Self {
            config: AutomationConfig::default(),
            categories: HashMap::new(),
            clocks: HashMap::new(),
            grid: Vec::new(),
            loaded: false,
            cache: HashMap::new(),
            cache_revision: None,
            last_played: HashMap::new(),
            artist_at: HashMap::new(),
            album_at: HashMap::new(),
            imaging_recent: HashMap::new(),
            history_loaded: false,
            cursor: None,
            pending: VecDeque::new(),
            songs_since_jingle: 0,
            last_music_category: None,
            last_was_music: false,
            playlist_pos: HashMap::new(),
            hard_anchors: Vec::new(),
            warnings: Vec::new(),
            seed: seed | 1,
            traffic: Traffic::default(),
            opening: VecDeque::new(),
        }
    }

    /// Settings, categories, clocks or the grid changed: read them again on the next
    /// call, and pick up the clock afresh.
    pub fn invalidate(&mut self) {
        self.loaded = false;
        self.cursor = None;
        self.cache.clear();
    }

    /// Settings as last read (call `load` or `generate` first).
    pub fn config(&self) -> &AutomationConfig {
        &self.config
    }

    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    fn warn(&mut self, message: String) {
        if self.warnings.last() != Some(&message) && self.warnings.len() < MAX_WARNINGS {
            log::warn!("Automation: {message}");
            self.warnings.push(message);
        }
    }

    /// Read settings, categories, clocks and the grid if they changed.
    pub fn load(&mut self, catalog: &dyn Catalog) -> Result<(), String> {
        if !self.loaded {
            self.config = catalog.automation_config()?;
            self.categories = catalog.categories()?.into_iter().map(|c| (c.id, c)).collect();
            self.clocks = catalog.clocks()?.into_iter().map(|c| (c.id.clone(), c)).collect();
            self.grid = catalog.grid_rules()?;
            self.traffic.set_campaigns(catalog.campaigns()?);
            self.loaded = true;
        }
        let revision = catalog.revision();
        if self.cache_revision != Some(revision) {
            self.cache.clear();
            self.cache_revision = Some(revision);
        }
        Ok(())
    }

    /// The id of the item that must cut in now, if a hard anchor's time came.
    pub fn due_hard_anchor(&mut self, now: NaiveDateTime) -> Option<String> {
        let due: Vec<_> = self.hard_anchors.iter().filter(|(_, at)| *at <= now).cloned().collect();
        self.hard_anchors.retain(|(_, at)| *at > now);
        // Anchors missed by more than a minute (e.g. the machine slept) are dropped.
        due.into_iter().rev().find(|(_, at)| now - *at < Duration::seconds(60)).map(|(id, _)| id)
    }

    /// The clock period the generator is filling (it runs a little ahead of the air).
    pub fn period(&self) -> Option<PeriodInfo> {
        self.cursor.as_ref().map(|c| PeriodInfo {
            start: c.start,
            end: c.end,
            clock_id: match &c.source {
                Source::Clock(id) => Some(id.clone()),
                _ => None,
            },
            playlist_id: match &c.source {
                Source::Playlist(id) => Some(id.clone()),
                _ => None,
            },
            rule_id: c.rule_id.clone(),
            slot: c.slot,
        })
    }

    /// Make items starting at `start` until there are at least `min_items` and
    /// `min_ms` of audio. `overlap_ms` is the crossfade between items.
    pub fn generate(
        &mut self,
        catalog: &mut dyn Catalog,
        start: NaiveDateTime,
        min_items: usize,
        min_ms: u64,
        overlap_ms: u64,
    ) -> Result<Vec<PlaylistItem>, String> {
        self.load(catalog)?;
        if !self.history_loaded {
            self.load_history(catalog, start)?;
        }
        let mut t = start;
        let mut out: Vec<PlaylistItem> = Vec::new();
        let mut total = 0;
        while (out.len() < min_items || total < min_ms) && out.len() < MAX_ITEMS_PER_CALL {
            let Some(mut item) = self.next_item(catalog, t)? else { break };
            item.no_overlap = self.config.no_overlap_kinds.contains(&item.kind);
            // Imaging may go over the next song's intro, or the previous song's outro.
            if self.may_talk_over(&item) {
                // Picked for when it would start in sequence, so separation stays on the
                // safe side if it then moves earlier, under the imaging.
                let after = t + ms(item_ms(&item).saturating_sub(if item.no_overlap { 0 } else { overlap_ms }).max(1000));
                if let Some(mut song) = self.next_item(catalog, after)? {
                    song.no_overlap = self.config.no_overlap_kinds.contains(&song.kind);
                    if let Some(spec) = self.over_intro(catalog, &song, &item)? {
                        song.overlay = Some(Box::new(spec));
                        song.planned_at = Some(t);
                        item = song;
                    } else if let Some(spec) = out.last().map(|prev| self.over_outro(catalog, prev, &item)).transpose()?.flatten() {
                        if let Some(prev) = out.last_mut() {
                            prev.overlay = Some(Box::new(spec));
                        }
                        song.planned_at = Some(t);
                        item = song;
                    } else {
                        // In sequence after all: the song follows the imaging.
                        total += item_ms(&item);
                        out.push(item);
                        song.planned_at = Some(after);
                        t = after;
                        item = song;
                    }
                }
            }
            let duration = item_ms(&item);
            let overlap = if item.no_overlap { 0 } else { overlap_ms };
            t = item.planned_at.unwrap_or(t) + ms(duration.saturating_sub(overlap).max(1000));
            total += duration;
            out.push(item);
        }
        Ok(out)
    }

    /// Imaging of a kind allowed over songs, not tied to a hard time.
    fn may_talk_over(&self, item: &PlaylistItem) -> bool {
        let talk = &self.config.talk_over;
        talk.enabled
            && talk.kinds.contains(&item.kind)
            && item.duration_ms > 0
            && !self.hard_anchors.iter().any(|(id, _)| *id == item.id)
    }

    /// The song's marks, when they can be trusted for talking over.
    fn trusted_cues(&self, catalog: &dyn Catalog, song: &PlaylistItem) -> Result<Option<crate::analysis::TrackCues>, String> {
        if !is_music(song.kind) || song.overlay.is_some() {
            return Ok(None);
        }
        let Some(id) = song.track_id else { return Ok(None) };
        Ok(catalog.track_cues(id)?.filter(|c| {
            c.source != crate::analysis::CueSource::Auto || c.confidence >= self.config.talk_over.min_confidence
        }))
    }

    /// `imaging` over `song`'s intro, ending `margin_ms` before the voice.
    fn over_intro(&self, catalog: &dyn Catalog, song: &PlaylistItem, imaging: &PlaylistItem) -> Result<Option<OverlaySpec>, String> {
        let talk = &self.config.talk_over;
        let Some(cues) = self.trusted_cues(catalog, song)?.filter(|_| talk.on_intro) else { return Ok(None) };
        let need = imaging.duration_ms + talk.margin_ms;
        Ok((cues.intro_room_ms() >= need).then(|| OverlaySpec {
            item: imaging.clone(),
            start_ms: cues.intro_end_ms.unwrap_or(0).saturating_sub(need).max(cues.cue_in_ms),
            duck_db: talk.duck_db,
        }))
    }

    /// `imaging` over `song`'s outro, starting `margin_ms` after the voice ends and
    /// finishing before the song does.
    fn over_outro(&self, catalog: &dyn Catalog, song: &PlaylistItem, imaging: &PlaylistItem) -> Result<Option<OverlaySpec>, String> {
        let talk = &self.config.talk_over;
        let Some(cues) = self.trusted_cues(catalog, song)?.filter(|_| talk.on_outro) else { return Ok(None) };
        let need = imaging.duration_ms + talk.margin_ms;
        Ok((cues.outro_room_ms() >= need).then(|| OverlaySpec {
            item: imaging.clone(),
            start_ms: cues.outro_start_ms.unwrap_or(cues.cue_out_ms) + talk.margin_ms,
            duck_db: talk.duck_db,
        }))
    }

    fn load_history(&mut self, catalog: &dyn Catalog, start: NaiveDateTime) -> Result<(), String> {
        for (track_id, artist, at) in catalog.recent_plays(start - Duration::days(1))? {
            if let Some(id) = track_id {
                self.last_played.insert(id, at);
            }
            let artist = key(&artist);
            if artist != 0 {
                self.artist_at.insert(artist, at);
            }
        }
        self.history_loaded = true;
        Ok(())
    }

    fn next_item(&mut self, catalog: &mut dyn Catalog, t: NaiveDateTime) -> Result<Option<PlaylistItem>, String> {
        if self.traffic.needs_day(t.date()) {
            let midnight = t.date().and_time(NaiveTime::MIN);
            let aired = catalog.campaign_plays(midnight, midnight + Duration::days(1))?;
            self.traffic.start_day(t.date(), &aired);
        }
        self.ensure_period(catalog, t)?;
        if let Some(mut item) = self.pending.pop_front() {
            item.planned_at = Some(t);
            return Ok(Some(item));
        }
        // Bounded: a clock of empty slots must not spin forever.
        for _ in 0..256 {
            self.ensure_period(catalog, t)?;
            let source = self.cursor.as_ref().map(|c| c.source.clone()).unwrap_or(Source::Nothing);
            let step = match source {
                Source::Clock(id) => self.clock_step(catalog, &id, t)?,
                Source::Playlist(id) => match self.playlist_item(catalog, &id, t)? {
                    Some((item, _)) => Step::Item(item),
                    None => self.fallback(catalog, t)?,
                },
                Source::Nothing => self.fallback(catalog, t)?,
            };
            match step {
                Step::Item(item) => return Ok(Some(item)),
                Step::Next => continue,
                Step::Stuck => return Ok(None),
            }
        }
        self.warn("the clock makes no items".to_string());
        Ok(None)
    }

    /// Start a new clock period when `t` left the current one. Like anchors, a period
    /// may start a little early, so a song ending just before the hour is followed by
    /// the top of the hour, not by one more song.
    /// Closing sponsorships of the period that ends go first; opening ones of the new
    /// period play before its first song.
    fn ensure_period(&mut self, catalog: &mut dyn Catalog, planned: NaiveDateTime) -> Result<(), String> {
        let t = planned + Duration::seconds(EARLY_SECS);
        if self.cursor.as_ref().is_some_and(|c| c.start <= t && t < c.end) {
            return Ok(());
        }
        let rule = grid::resolve(&self.grid, t);
        let rule_id = rule.map(|r| r.id.clone());
        let source = match rule.map(|r| r.target.clone()) {
            Some(GridTarget::Clock { clock_id }) => Source::Clock(clock_id),
            Some(GridTarget::Playlist { playlist_id }) => Source::Playlist(playlist_id),
            None => self.config.default_clock.clone().map_or(Source::Nothing, Source::Clock),
        };
        let clock_of = |source: &Source| match source {
            Source::Clock(id) => Some(id.clone()),
            _ => None,
        };
        if let Some(old) = self.cursor.take() {
            // The same programme going on (next hour, same rule and clock) is not
            // closed and opened again.
            if old.rule_id != rule_id || old.source != source {
                for pick in self.traffic.sponsors(clock_of(&old.source).as_deref(), old.rule_id.as_deref(), planned, false) {
                    if let Some(item) = self.spot_item(catalog, pick, planned)? {
                        self.pending.push_back(item);
                    }
                }
                self.opening = self.traffic.sponsors(clock_of(&source).as_deref(), rule_id.as_deref(), t, true).into();
            }
        } else {
            self.opening = self.traffic.sponsors(clock_of(&source).as_deref(), rule_id.as_deref(), t, true).into();
        }
        let minutes = match &source {
            Source::Clock(id) => self.clocks.get(id).map_or(60, |c| c.duration_minutes.clamp(5, 24 * 60)),
            _ => 60,
        } as i64;
        let length = minutes * 60;
        let since_midnight = t.time().num_seconds_from_midnight() as i64;
        let start = t.date().and_time(NaiveTime::MIN) + Duration::seconds(since_midnight / length * length);
        self.cursor = Some(Cursor { start, end: start + Duration::seconds(length), source, rule_id, slot: 0, done: 0 });
        Ok(())
    }

    fn spot_item(&mut self, catalog: &dyn Catalog, pick: SpotPick, t: NaiveDateTime) -> Result<Option<PlaylistItem>, String> {
        let Some(track) = catalog.track(pick.track_id)? else {
            self.warn(format!("spot {} of campaign {} is missing", pick.track_id, pick.campaign_id));
            return Ok(None);
        };
        let mut item = PlaylistItem::new(track.file_path, track.title, track.artist, track.duration_ms);
        item.kind = ItemKind::Spot;
        item.track_id = Some(track.id);
        item.campaign_id = Some(pick.campaign_id);
        item.planned_at = Some(t);
        item.generated = true;
        Ok(Some(item))
    }

    /// Put `items` on air in order: the first now, the rest next.
    fn sequence(&mut self, mut items: Vec<PlaylistItem>) -> Option<PlaylistItem> {
        if items.is_empty() {
            return None;
        }
        let first = items.remove(0);
        for item in items.into_iter().rev() {
            self.pending.push_front(item);
        }
        Some(first)
    }

    /// Fallback music when nothing is scheduled.
    fn fallback(&mut self, catalog: &mut dyn Catalog, t: NaiveDateTime) -> Result<Step, String> {
        match self.config.fallback_category {
            Some(category) => Ok(self
                .music(catalog, category, t, &[], &[], None)?
                .map_or(Step::Stuck, Step::Item)),
            None => {
                self.warn("nothing scheduled now and no fallback category".to_string());
                Ok(Step::Stuck)
            }
        }
    }

    fn advance_slot(&mut self) {
        if let Some(c) = self.cursor.as_mut() {
            c.slot += 1;
            c.done = 0;
        }
    }

    fn clock_step(&mut self, catalog: &mut dyn Catalog, clock_id: &str, t: NaiveDateTime) -> Result<Step, String> {
        let Some(clock) = self.clocks.get(clock_id).cloned() else {
            self.warn(format!("clock {clock_id} does not exist"));
            return self.fallback(catalog, t);
        };
        let Some(cursor) = self.cursor.as_mut() else { return Ok(Step::Stuck) };
        let period_start = cursor.start;
        let anchor_time = |i: usize| clock.slots[i].at.map(|a| period_start + Duration::seconds(a.offset_seconds()));
        // Jump to the latest anchored slot whose time came, skipping what is left before it.
        let due = (cursor.slot..clock.slots.len())
            .rev()
            .find(|&i| anchor_time(i).is_some_and(|at| t >= at - Duration::seconds(EARLY_SECS)));
        let mut skipped_breaks = Vec::new();
        if let Some(j) = due.filter(|&j| j > cursor.slot) {
            for slot in &clock.slots[cursor.slot..j] {
                if let SlotItem::AdBreak { name, .. } = &slot.item {
                    skipped_breaks.push(format!("ad break \"{name}\" skipped: its time passed"));
                }
            }
            cursor.slot = j;
            cursor.done = 0;
        }
        let (slot_index, done, period_end) = (cursor.slot, cursor.done, cursor.end);
        // The next anchor ahead (or the end of the period), to land songs on it.
        let target = (slot_index + usize::from(done > 0 || clock.slots.get(slot_index).is_some_and(|s| s.at.is_none()))
            ..clock.slots.len())
            .find_map(|i| clock.slots[i].at.map(|a| (anchor_time(i).unwrap(), a.max_wait_seconds as i64)))
            .or(Some((period_end, 120)));
        for warning in skipped_breaks {
            self.warn(warning);
        }
        let fill_category = clock.fill_category.or(self.last_music_category).or(self.config.fallback_category);

        let Some(slot) = clock.slots.get(slot_index) else {
            // Slots used up: fill to the end of the period.
            return self.fill(catalog, fill_category, t, target, clock_id);
        };
        let mut planned = t;
        if let (Some(anchor), Some(at)) = (slot.at, anchor_time(slot_index)) {
            if done == 0 {
                if t < at - Duration::seconds(EARLY_SECS) {
                    // Too early for this slot: fill until its time.
                    return self.fill(catalog, fill_category, t, target, clock_id);
                }
                if anchor.hard && t > at {
                    planned = at;
                }
            }
        }

        let step = match &slot.item {
            SlotItem::Category { category_id, count, require_codes, exclude_codes } => {
                if done >= *count {
                    self.advance_slot();
                    return Ok(Step::Next);
                }
                let target = if done + 1 >= *count { target } else { None };
                match self.music(catalog, *category_id, planned, require_codes, exclude_codes, target)? {
                    Some(item) => {
                        if let Some(c) = self.cursor.as_mut() {
                            c.done += 1;
                        }
                        Step::Item(item)
                    }
                    None => {
                        self.warn(format!("category {category_id} has nothing to play"));
                        self.advance_slot();
                        Step::Next
                    }
                }
            }
            SlotItem::Imaging { category_id } => {
                self.advance_slot();
                match self.imaging(catalog, *category_id, planned)? {
                    Some(item) => Step::Item(item),
                    None => Step::Next,
                }
            }
            SlotItem::AdBreak { max_seconds, intro_category, outro_category, .. } => {
                self.advance_slot();
                let max_ms = if *max_seconds > 0 { *max_seconds } else { self.config.ad_break_seconds } as u64 * 1000;
                let picks = {
                    let catalog: &dyn Catalog = catalog;
                    let length = |id: i64| catalog.track(id).ok().flatten().map_or(30_000, |t| length_ms(t.duration_ms));
                    self.traffic.fill_break(planned, max_ms, &length)
                };
                if picks.is_empty() {
                    Step::Next
                } else {
                    let mut items = Vec::new();
                    if let Some(category) = intro_category {
                        items.extend(self.imaging(catalog, *category, planned)?);
                    }
                    for pick in picks {
                        items.extend(self.spot_item(catalog, pick, planned)?);
                    }
                    if let Some(category) = outro_category {
                        items.extend(self.imaging(catalog, *category, planned)?);
                    }
                    self.last_was_music = false;
                    self.sequence(items).map_or(Step::Next, Step::Item)
                }
            }
            SlotItem::Playlist { playlist_id, count } => match self.playlist_item(catalog, playlist_id, planned)? {
                Some((item, at_end)) => {
                    let done = done + 1;
                    if count.map_or(at_end, |n| done >= n) {
                        self.advance_slot();
                    } else if let Some(c) = self.cursor.as_mut() {
                        c.done = done;
                    }
                    Step::Item(item)
                }
                None => {
                    self.warn(format!("playlist {playlist_id} is empty or missing"));
                    self.advance_slot();
                    Step::Next
                }
            },
            SlotItem::Track { path } => {
                self.advance_slot();
                let title = std::path::Path::new(path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut item = PlaylistItem::new(path.clone(), title, String::new(), 0);
                item.kind = ItemKind::Program;
                item.planned_at = Some(planned);
                item.generated = true;
                self.last_was_music = false;
                Step::Item(item)
            }
            SlotItem::Fill { category_id } => {
                return self.fill(catalog, Some(*category_id), planned, target, clock_id);
            }
        };
        if let (Step::Item(item), Some(anchor)) = (&step, slot.at) {
            if anchor.hard && done == 0 && planned < t {
                self.hard_anchors.push((item.id.clone(), planned));
            }
        }
        Ok(step)
    }

    fn fill(
        &mut self,
        catalog: &mut dyn Catalog,
        category: Option<i64>,
        t: NaiveDateTime,
        target: Option<(NaiveDateTime, i64)>,
        clock_id: &str,
    ) -> Result<Step, String> {
        let Some(category) = category else {
            self.warn(format!("clock {clock_id} has nothing to fill with"));
            return Ok(Step::Stuck);
        };
        Ok(self.music(catalog, category, t, &[], &[], target)?.map_or(Step::Stuck, Step::Item))
    }

    /// A song from `category_id`, with the station's imaging rules applied around it.
    fn music(
        &mut self,
        catalog: &mut dyn Catalog,
        category_id: i64,
        t: NaiveDateTime,
        require: &[String],
        exclude: &[String],
        target: Option<(NaiveDateTime, i64)>,
    ) -> Result<Option<PlaylistItem>, String> {
        let mut before = Vec::new();
        while let Some(pick) = self.opening.pop_front() {
            before.extend(self.spot_item(catalog, pick, t)?);
        }
        if before.is_empty() && self.last_was_music {
            let imaging = self.config.imaging.clone();
            if let Some(jingles) = imaging.jingle_category.filter(|_| {
                imaging.jingle_every > 0 && self.songs_since_jingle >= imaging.jingle_every
            }) {
                before.extend(self.imaging(catalog, jingles, t)?);
            }
            if before.is_empty() {
                if let Some(sweepers) = imaging
                    .sweeper_on_change
                    .filter(|_| self.last_music_category.is_some_and(|c| c != category_id))
                {
                    before.extend(self.imaging(catalog, sweepers, t)?);
                }
            }
        }
        let start = t + ms(before.iter().map(item_ms).sum());
        if let Some(item) = self.pick_music(catalog, category_id, start, require, exclude, target)? {
            self.songs_since_jingle += 1;
            self.last_music_category = Some(category_id);
            self.last_was_music = true;
            before.push(item);
        }
        Ok(self.sequence(before))
    }

    fn ensure_candidates(&mut self, catalog: &dyn Catalog, category_id: i64) -> Result<(), String> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.cache.entry(category_id) {
            entry.insert(catalog.candidates(category_id)?.into_iter().map(Candidate::from).collect());
        }
        Ok(())
    }

    fn separation(&self, category_id: i64) -> Separation {
        self.categories
            .get(&category_id)
            .and_then(|c| c.separation)
            .unwrap_or(self.config.separation)
    }

    /// Does `c` keep the separation rules at relaxation `level` (0 = all rules)?
    fn separated(&self, c: &Candidate, sep: &Separation, level: u8, t: NaiveDateTime) -> bool {
        let minutes = |m: u32, halve_at: u8, drop_at: u8| match level {
            l if l >= drop_at => 0,
            l if l >= halve_at => m / 2,
            _ => m,
        };
        let within = |at: Option<&NaiveDateTime>, m: u32| {
            m > 0 && at.is_some_and(|at| (t - *at).num_seconds().abs() < m as i64 * 60)
        };
        let last = self.last_played.get(&c.track_id).or(c.last_played.as_ref());
        if within(last, minutes(sep.song_minutes, 3, 5)) {
            return false;
        }
        if sep.horizontal && level == 0 {
            if let Some(last) = last {
                let yesterday = t - Duration::days(1);
                if (yesterday - *last).num_minutes().abs() < 30 {
                    return false;
                }
            }
        }
        if c.artist != 0 && within(self.artist_at.get(&c.artist), minutes(sep.artist_minutes, 2, 4)) {
            return false;
        }
        if c.album != 0 && within(self.album_at.get(&c.album), if level >= 1 { 0 } else { sep.album_minutes }) {
            return false;
        }
        true
    }

    /// Least recently played first (counted from the end of its last play), scaled
    /// by weight, with a little randomness so the order is not predictable.
    fn score(&self, c: &Candidate, t: NaiveDateTime) -> f64 {
        let since = self.last_played.get(&c.track_id).or(c.last_played.as_ref()).map_or(NEVER_PLAYED_SECS, |at| {
            ((t - *at).num_seconds() - length_ms(c.duration_ms) as i64 / 1000).clamp(1, NEVER_PLAYED_SECS)
        });
        let mut x = self.seed ^ (c.track_id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        let jitter = 0.85 + 0.3 * (x >> 11) as f64 / (1u64 << 53) as f64;
        since as f64 * c.weight as f64 * jitter
    }

    fn next_seed(&mut self) {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
    }

    fn pick_music(
        &mut self,
        catalog: &mut dyn Catalog,
        category_id: i64,
        t: NaiveDateTime,
        require: &[String],
        exclude: &[String],
        target: Option<(NaiveDateTime, i64)>,
    ) -> Result<Option<PlaylistItem>, String> {
        self.ensure_candidates(catalog, category_id)?;
        self.next_seed();
        let sep = self.separation(category_id);
        let require: Vec<String> = require.iter().map(|c| c.to_lowercase()).collect();
        let exclude: Vec<String> = exclude.iter().map(|c| c.to_lowercase()).collect();
        let candidates = &self.cache[&category_id];
        let mut chosen = None;
        for level in 0..=MAX_RELAX {
            let mut scored: Vec<(f64, usize)> = candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    require.iter().all(|r| c.codes.contains(r)) && !exclude.iter().any(|x| c.codes.contains(x))
                })
                .filter(|(_, c)| self.separated(c, &sep, level, t))
                .map(|(i, c)| (self.score(c, t), i))
                .collect();
            if scored.is_empty() {
                continue;
            }
            if level > 0 {
                log::debug!("Automation: separation relaxed to level {level} in category {category_id}");
            }
            // Only the best few matter: select them instead of sorting everything.
            let best = BACKTIME_CHOICES.min(scored.len());
            if scored.len() > best {
                scored.select_nth_unstable_by(best - 1, |a, b| b.0.total_cmp(&a.0));
                scored.truncate(best);
            }
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            // Landing on an anchor may trade some rotation, not much: only songs
            // rotated nearly as well as the best are considered.
            let good_enough = scored[0].0 / 4.0;
            chosen = Some(match target {
                Some((at, max_wait)) if (at - t).num_seconds() <= BACKTIME_WINDOW_SECS => {
                    let mut lengths: Vec<i64> =
                        candidates.iter().map(|c| length_ms(c.duration_ms) as i64 / 1000).collect();
                    lengths.sort_unstable();
                    lengths.dedup();
                    scored
                        .iter()
                        .take(BACKTIME_CHOICES)
                        .filter(|(score, _)| *score >= good_enough)
                        .min_by_key(|(_, i)| landing_penalty(t, candidates[*i].duration_ms, at, max_wait, &lengths))
                        .map(|(_, i)| *i)
                        .unwrap_or(scored[0].1)
                }
                _ => scored[0].1,
            });
            break;
        }
        let Some(i) = chosen else { return Ok(None) };
        let candidate = candidates[i].clone();
        self.make_item(catalog, &candidate, category_id, t)
    }

    fn imaging(&mut self, catalog: &mut dyn Catalog, category_id: i64, t: NaiveDateTime) -> Result<Option<PlaylistItem>, String> {
        self.ensure_candidates(catalog, category_id)?;
        self.next_seed();
        let others = self.separation(category_id).imaging_others as usize;
        let recent = self.imaging_recent.get(&category_id);
        let candidates = &self.cache[&category_id];
        let fresh = |c: &&Candidate| recent.is_none_or(|r| !r.contains(&c.track_id));
        let pool: Vec<&Candidate> = match candidates.iter().filter(fresh).count() {
            0 => candidates.iter().collect(),
            _ => candidates.iter().filter(fresh).collect(),
        };
        let Some(candidate) = pool
            .into_iter()
            .max_by(|a, b| self.score(a, t).total_cmp(&self.score(b, t)))
            .cloned()
        else {
            self.warn(format!("imaging category {category_id} is empty"));
            return Ok(None);
        };
        let recent = self.imaging_recent.entry(category_id).or_default();
        recent.push_back(candidate.track_id);
        while recent.len() > others {
            recent.pop_front();
        }
        let item = self.make_item(catalog, &candidate, category_id, t)?;
        if let Some(item) = &item {
            if matches!(item.kind, ItemKind::Jingle | ItemKind::StationId) {
                self.songs_since_jingle = 0;
            }
            self.last_was_music = false;
        }
        Ok(item)
    }

    fn make_item(
        &mut self,
        catalog: &dyn Catalog,
        c: &Candidate,
        category_id: i64,
        t: NaiveDateTime,
    ) -> Result<Option<PlaylistItem>, String> {
        let Some(track) = catalog.track(c.track_id)? else {
            // Deleted since the cache was filled.
            self.cache.remove(&category_id);
            return Ok(None);
        };
        let kind = self.categories.get(&category_id).map_or(ItemKind::Music, |c| c.kind);
        let mut item = PlaylistItem::new(track.file_path, track.title, track.artist, track.duration_ms);
        item.kind = kind;
        item.track_id = Some(track.id);
        item.category_id = Some(category_id);
        item.planned_at = Some(t);
        item.generated = true;
        self.last_played.insert(c.track_id, t);
        if is_music(kind) {
            if c.artist != 0 {
                self.artist_at.insert(c.artist, t);
            }
            if c.album != 0 {
                self.album_at.insert(c.album, t);
            }
        }
        Ok(Some(item))
    }

    /// Next item of a saved playlist, continuing where it was left; also says
    /// whether it was the last one.
    fn playlist_item(
        &mut self,
        catalog: &mut dyn Catalog,
        playlist_id: &str,
        t: NaiveDateTime,
    ) -> Result<Option<(PlaylistItem, bool)>, String> {
        let playlist = match catalog.playlist(playlist_id) {
            Ok(p) if !p.is_empty() => p,
            _ => return Ok(None),
        };
        let kv_key = format!("playlist_pos:{playlist_id}");
        let pos = match self.playlist_pos.get(playlist_id) {
            Some(pos) => *pos,
            None => catalog.kv_get(&kv_key)?.and_then(|v| v.parse().ok()).unwrap_or(0),
        } % playlist.len();
        let next = pos + 1;
        self.playlist_pos.insert(playlist_id.to_string(), next % playlist.len());
        catalog.kv_set(&kv_key, &(next % playlist.len()).to_string())?;
        let mut item = playlist.items[pos].clone();
        item.id = uuid::Uuid::new_v4().to_string();
        item.planned_at = Some(t);
        item.generated = true;
        self.last_was_music = is_music(item.kind);
        Ok(Some((item, next >= playlist.len())))
    }
}

fn length_ms(duration_ms: u64) -> u64 {
    if duration_ms > 0 { duration_ms } else { UNKNOWN_DURATION_MS }
}

/// How badly a song of `duration_ms` starting at `t` misses an anchor at `at`
/// (seconds; lower is better). When another song must come first, how well the
/// best of `lengths` (seconds, sorted) would land then.
fn landing_penalty(t: NaiveDateTime, duration_ms: u64, at: NaiveDateTime, max_wait: i64, lengths: &[i64]) -> i64 {
    let left = (at - (t + ms(length_ms(duration_ms)))).num_seconds();
    // Penalty when the anchor follows right after a song ending `late` seconds after it.
    let miss = |late: i64| match late {
        l if l < -EARLY_SECS => None,
        l if l <= max_wait => Some(l.max(0)),
        l => Some(l * 10),
    };
    if let Some(penalty) = miss(-left) {
        return penalty;
    }
    let (shortest, longest) = (lengths.first().copied().unwrap_or(180), lengths.last().copied().unwrap_or(300));
    if left > longest + max_wait {
        return 0; // still far: later picks aim
    }
    lengths
        .iter()
        .map(|d| match miss(d - left) {
            Some(penalty) => penalty,
            // Still short of the anchor: a third song, if one fits.
            None if left - d >= shortest => 60,
            None => 300,
        })
        .min()
        .unwrap_or(300)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::model::{Anchor, CampaignSpot, ClockSlot, ImagingRules, SponsorTarget, Sponsorship};
    use chrono::NaiveDate;

    /// An in-memory library.
    #[derive(Default)]
    struct Fake {
        tracks: Vec<Track>,
        by_id: HashMap<i64, usize>,
        members: HashMap<i64, Vec<i64>>,
        categories: Vec<Category>,
        clocks: Vec<Clock>,
        grid: Vec<GridRule>,
        config: AutomationConfig,
        playlists: HashMap<String, Playlist>,
        kv: HashMap<String, String>,
        campaigns: Vec<Campaign>,
        cues: HashMap<i64, crate::analysis::TrackCues>,
    }

    impl Fake {
        fn category(&mut self, id: i64, kind: ItemKind, count: usize, artists: usize, minutes: u64) {
            self.categories.push(Category {
                id,
                name: format!("cat{id}"),
                kind,
                color: String::new(),
                parent_id: None,
                filter: None,
                separation: None,
            });
            for n in 0..count {
                let track_id = id * 1000 + n as i64;
                self.by_id.insert(track_id, self.tracks.len());
                self.tracks.push(Track {
                    id: track_id,
                    title: format!("t{track_id}"),
                    artist: if artists == 0 { String::new() } else { format!("a{id}-{}", n % artists) },
                    album: String::new(),
                    genre: String::new(),
                    // 3 to 5 minutes, varied
                    duration_ms: minutes * 60_000 + (n as u64 * 37_000) % 120_000,
                    file_path: format!("/m/{track_id}.mp3"),
                    bpm: None,
                    sample_rate: 44100,
                });
                self.members.entry(id).or_default().push(track_id);
            }
        }
    }

    impl Catalog for Fake {
        fn revision(&self) -> u64 {
            0
        }
        fn automation_config(&self) -> Result<AutomationConfig, String> {
            Ok(self.config.clone())
        }
        fn categories(&self) -> Result<Vec<Category>, String> {
            Ok(self.categories.clone())
        }
        fn clocks(&self) -> Result<Vec<Clock>, String> {
            Ok(self.clocks.clone())
        }
        fn grid_rules(&self) -> Result<Vec<GridRule>, String> {
            Ok(self.grid.clone())
        }
        fn candidates(&self, category_id: i64) -> Result<Vec<CandidateRow>, String> {
            let ids = self.members.get(&category_id).cloned().unwrap_or_default();
            Ok(ids
                .into_iter()
                .filter_map(|id| self.by_id.get(&id).map(|&i| &self.tracks[i]))
                .map(|t| CandidateRow {
                    track_id: t.id,
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    duration_ms: t.duration_ms,
                    weight: 1.0,
                    last_played_at: None,
                    codes: if t.id % 2 == 0 { vec!["even".into()] } else { vec![] },
                })
                .collect())
        }
        fn track(&self, id: i64) -> Result<Option<Track>, String> {
            Ok(self.by_id.get(&id).map(|&i| self.tracks[i].clone()))
        }
        fn playlist(&self, id: &str) -> Result<Playlist, String> {
            self.playlists.get(id).cloned().ok_or_else(|| "missing".into())
        }
        fn recent_plays(&self, _: NaiveDateTime) -> Result<Vec<(Option<i64>, String, NaiveDateTime)>, String> {
            Ok(Vec::new())
        }
        fn kv_get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.kv.get(key).cloned())
        }
        fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.kv.insert(key.into(), value.into());
            Ok(())
        }
        fn campaigns(&self) -> Result<Vec<Campaign>, String> {
            Ok(self.campaigns.clone())
        }
        fn campaign_plays(&self, _: NaiveDateTime, _: NaiveDateTime) -> Result<Vec<(i64, NaiveDateTime)>, String> {
            Ok(Vec::new())
        }
        fn track_cues(&self, track_id: i64) -> Result<Option<crate::analysis::TrackCues>, String> {
            Ok(self.cues.get(&track_id).cloned())
        }
    }

    impl Fake {
        /// Spot audio: `count` tracks of 30 s, ids from `first_id`.
        fn spots(&mut self, first_id: i64, count: i64) -> Vec<i64> {
            (first_id..first_id + count)
                .map(|id| {
                    self.by_id.insert(id, self.tracks.len());
                    self.tracks.push(Track {
                        id,
                        title: format!("spot{id}"),
                        artist: String::new(),
                        album: String::new(),
                        genre: String::new(),
                        duration_ms: 30_000,
                        file_path: format!("/spots/{id}.mp3"),
                        bpm: None,
                        sample_rate: 44100,
                    });
                    id
                })
                .collect()
        }

        fn campaign(&mut self, id: i64, advertiser_id: i64, plays_per_day: u32, spots: &[i64]) -> &mut Campaign {
            self.campaigns.push(Campaign {
                id,
                advertiser_id,
                name: format!("campaign{id}"),
                date_from: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                date_to: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
                plays_per_day,
                max_per_hour: 0,
                dayparts: vec![],
                priority: 0,
                sponsorship: None,
                enabled: true,
                spots: spots.iter().map(|&track_id| CampaignSpot { track_id, weight: 1.0 }).collect(),
            });
            self.campaigns.last_mut().unwrap()
        }
    }

    const POP: i64 = 1;
    const ROCK: i64 = 2;
    const JINGLES: i64 = 3;
    const IDS: i64 = 4;

    fn slot(at: Option<u32>, item: SlotItem) -> ClockSlot {
        ClockSlot { at: at.map(|minute| Anchor { minute, second: 0, hard: false, max_wait_seconds: 120 }), item }
    }

    fn cat(category_id: i64) -> SlotItem {
        SlotItem::Category { category_id, count: 1, require_codes: vec![], exclude_codes: vec![] }
    }

    fn station() -> Fake {
        let mut f = Fake::default();
        f.category(POP, ItemKind::Music, 80, 25, 3);
        f.category(ROCK, ItemKind::Music, 50, 15, 3);
        f.category(JINGLES, ItemKind::Jingle, 6, 0, 0);
        f.category(IDS, ItemKind::StationId, 3, 0, 0);
        f.clocks.push(Clock {
            id: "pop".into(),
            name: "Pop".into(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![
                slot(Some(0), SlotItem::Imaging { category_id: IDS }),
                slot(None, cat(POP)),
                slot(None, cat(ROCK)),
                slot(None, cat(POP)),
                slot(Some(30), SlotItem::Imaging { category_id: IDS }),
                slot(None, cat(POP)),
            ],
            fill_category: Some(POP),
        });
        f.clocks.push(Clock {
            id: "rock".into(),
            name: "Rock".into(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![slot(Some(0), SlotItem::Imaging { category_id: IDS }), slot(None, SlotItem::Fill { category_id: ROCK })],
            fill_category: Some(ROCK),
        });
        f.config.enabled = true;
        f.config.default_clock = Some("pop".into());
        f
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 19).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    /// Generate one item at a time, back to back, for `hours`.
    fn run(f: &mut Fake, g: &mut Generator, from: NaiveDateTime, hours: i64) -> Vec<PlaylistItem> {
        let mut t = from;
        let mut out = Vec::new();
        while t < from + Duration::hours(hours) {
            let items = g.generate(f, t, 1, 0, 0).unwrap();
            assert!(!items.is_empty(), "generator ran dry at {t}");
            for item in items {
                t = item.planned_at.unwrap() + ms(item_ms(&item));
                out.push(item);
            }
        }
        out
    }

    /// Random choices differ by seed: each test runs with many.
    const SEEDS: std::ops::RangeInclusive<u64> = 1..=40;

    #[test]
    fn a_day_keeps_separation_and_hits_the_top_of_the_hour() {
        for seed in SEEDS {
            let mut f = station();
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(0, 0), 24);
            let mut last_song: HashMap<String, NaiveDateTime> = HashMap::new();
            let mut last_artist: HashMap<String, NaiveDateTime> = HashMap::new();
            for item in items.iter().filter(|i| i.kind == ItemKind::Music) {
                let t = item.planned_at.unwrap();
                if let Some(prev) = last_song.insert(item.track_path.clone(), t) {
                    assert!(t - prev >= Duration::minutes(240), "{} repeated after {}", item.title, t - prev);
                }
                if let Some(prev) = last_artist.insert(item.artist.clone(), t) {
                    assert!(t - prev >= Duration::minutes(45), "{} repeated after {}", item.artist, t - prev);
                }
            }
            // A station ID close to every hour and half hour.
            for h in 0..24 {
                for m in [0, 30] {
                    let anchor = at(h, m);
                    assert!(
                        items.iter().any(|i| i.kind == ItemKind::StationId
                            && (i.planned_at.unwrap() - anchor).num_seconds().abs() <= 180),
                        "seed {seed}: no ID near {anchor}"
                    );
                }
            }
        }
    }

    #[test]
    fn grid_switches_clock() {
        for seed in SEEDS {
            let mut f = station();
            let mut rock = GridRule {
                id: "r".into(),
                name: "rock nights".into(),
                priority: 0,
                target: GridTarget::Clock { clock_id: "rock".into() },
                weekdays: 0x7f,
                months: 0xfff,
                weeks_of_month: 0x3f,
                start: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
                date_from: None,
                date_to: None,
                enabled: true,
            };
            rock.priority = 1;
            f.grid.push(rock);
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(19, 0), 4);
            let in_span = |i: &&PlaylistItem| {
                let t = i.planned_at.unwrap();
                t >= at(20, 5) && t < at(21, 55) && i.kind == ItemKind::Music
            };
            assert!(items.iter().filter(in_span).all(|i| i.category_id == Some(ROCK)));
            assert!(items.iter().any(|i| i.planned_at.unwrap() < at(19, 55) && i.category_id == Some(POP)));
        }
    }

    #[test]
    fn small_category_relaxes_instead_of_going_silent() {
        for seed in SEEDS {
            let mut f = Fake::default();
            f.category(POP, ItemKind::Music, 3, 1, 3);
            f.clocks.push(Clock {
                id: "c".into(),
                name: "c".into(),
                color: String::new(),
                duration_minutes: 60,
                slots: vec![slot(None, SlotItem::Fill { category_id: POP })],
                fill_category: None,
            });
            f.config.enabled = true;
            f.config.default_clock = Some("c".into());
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(8, 0), 2);
            assert!(items.len() > 20);
            // Still no immediate repeat of the same track.
            assert!(items.windows(2).all(|w| w[0].track_path != w[1].track_path), "seed {seed}");
        }
    }

    #[test]
    fn jingle_every_n_songs_and_codes() {
        for seed in SEEDS {
            let mut f = station();
            f.clocks[0].slots = vec![slot(
                None,
                SlotItem::Category { category_id: POP, count: 1000, require_codes: vec!["EVEN".into()], exclude_codes: vec![] },
            )];
            f.config.imaging = ImagingRules { jingle_category: Some(JINGLES), jingle_every: 3, sweeper_on_change: None };
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(8, 0), 3);
            let kinds: String = items.iter().map(|i| if i.kind == ItemKind::Jingle { 'J' } else { 'M' }).collect();
            assert!(kinds.contains("MMMJMMMJ"), "{kinds}");
            assert!(items.iter().filter(|i| i.kind == ItemKind::Music).all(|i| i.track_id.unwrap() % 2 == 0));
            // No jingle repeats until 3 others played.
            let jingles: Vec<_> = items.iter().filter(|i| i.kind == ItemKind::Jingle).map(|i| &i.track_path).collect();
            assert!(jingles.windows(4).all(|w| w[0] != w[1] && w[0] != w[2] && w[0] != w[3]));
        }
    }

    #[test]
    fn hard_anchor_cuts_in_on_time() {
        for seed in SEEDS {
            let mut f = station();
            f.clocks[0].slots[4].at = Some(Anchor { minute: 30, second: 0, hard: true, max_wait_seconds: 0 });
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(8, 0), 1);
            let id = items.iter().find(|i| i.kind == ItemKind::StationId && i.planned_at == Some(at(8, 30)));
            if let Some(id) = id {
                assert_eq!(g.due_hard_anchor(at(8, 30)), Some(id.id.clone()));
            } else {
                // Landed exactly on time: nothing to cut.
                assert!(g.due_hard_anchor(at(8, 30)).is_none());
            }
            assert!(g.due_hard_anchor(at(8, 31)).is_none());
        }
    }

    #[test]
    fn playlist_daypart_continues_where_it_left() {
        for seed in SEEDS {
            let mut f = station();
            let mut pl = Playlist::new("show".into());
            for n in 0..30 {
                pl.add_item(format!("/show/{n}.mp3"), format!("s{n}"), String::new(), 240_000);
            }
            f.playlists.insert("show".into(), pl);
            f.grid.push(GridRule {
                id: "s".into(),
                name: "show".into(),
                priority: 0,
                target: GridTarget::Playlist { playlist_id: "show".into() },
                weekdays: 0x7f,
                months: 0xfff,
                weeks_of_month: 0x3f,
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
                date_from: None,
                date_to: None,
                enabled: true,
            });
            let mut g = Generator::with_seed(seed);
            let first: Vec<_> = run(&mut f, &mut g, at(9, 0), 1).into_iter().filter(|i| i.track_path.starts_with("/show")).collect();
            assert_eq!(first[0].title, "s0");
            let n = first.len();
            // Next day, a new generator (restart): carries on from the saved position.
            let mut g = Generator::with_seed(seed);
            let next = g.generate(&mut f, at(9, 0) + Duration::days(1), 1, 0, 0).unwrap();
            assert_eq!(next[0].title, format!("s{n}"));
        }
    }

    fn ad_break(minute: u32) -> ClockSlot {
        slot(
            Some(minute),
            SlotItem::AdBreak { name: format!(":{minute}"), max_seconds: 90, intro_category: Some(JINGLES), outro_category: None },
        )
    }

    #[test]
    fn ad_breaks_meet_daily_targets() {
        for seed in SEEDS {
            let mut f = station();
            f.clocks[0].slots.insert(2, ad_break(15));
            f.clocks[0].slots.push(ad_break(45));
            let a = f.spots(9000, 3);
            let b = f.spots(9100, 2);
            let c = f.spots(9200, 1);
            f.campaign(1, 1, 20, &a);
            f.campaign(2, 2, 10, &b);
            f.campaign(3, 1, 10, &c); // same advertiser as campaign 1
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(0, 0), 24);
            let today: Vec<_> = items.iter().filter(|i| i.planned_at.unwrap() < at(0, 0) + Duration::days(1)).collect();
            for (campaign, target) in [(1, 20), (2, 10), (3, 10)] {
                let n = today.iter().filter(|i| i.campaign_id == Some(campaign)).count();
                assert_eq!(n, target, "seed {seed}: campaign {campaign}");
            }
            // Breaks: a jingle intro, then spots of different advertisers.
            for (n, item) in items.iter().enumerate().filter(|(_, i)| i.kind == ItemKind::Spot) {
                let prev = &items[n - 1];
                assert!(prev.kind == ItemKind::Jingle || prev.kind == ItemKind::Spot, "seed {seed}: spot after {:?}", prev.kind);
                if prev.kind == ItemKind::Spot {
                    let advertiser = |i: &PlaylistItem| if i.campaign_id == Some(2) { 2 } else { 1 };
                    assert_ne!(advertiser(prev), advertiser(item), "seed {seed}: same advertiser twice in a break");
                }
            }
        }
    }

    #[test]
    fn sponsorship_opens_and_closes_the_programme() {
        for seed in SEEDS {
            let mut f = station();
            f.grid.push(GridRule {
                id: "sports".into(),
                name: "sports".into(),
                priority: 1,
                target: GridTarget::Clock { clock_id: "rock".into() },
                weekdays: 0x7f,
                months: 0xfff,
                weeks_of_month: 0x3f,
                start: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
                date_from: None,
                date_to: None,
                enabled: true,
            });
            let spot = f.spots(9000, 1);
            f.campaign(7, 1, 0, &spot).sponsorship = Some(Sponsorship {
                target: SponsorTarget::GridRule { rule_id: "sports".into() },
                opening: true,
                closing: true,
            });
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(19, 0), 4);
            let mentions: Vec<NaiveDateTime> =
                items.iter().filter(|i| i.campaign_id == Some(7)).map(|i| i.planned_at.unwrap()).collect();
            assert_eq!(mentions.len(), 2, "seed {seed}: {mentions:?}");
            assert!((mentions[0] - at(20, 0)).num_seconds().abs() < 300, "seed {seed}: opening at {}", mentions[0]);
            assert!((mentions[1] - at(22, 0)).num_seconds().abs() < 120, "seed {seed}: closing at {}", mentions[1]);
            // The opening comes right before the first rock song.
            let n = items.iter().position(|i| i.campaign_id == Some(7)).unwrap();
            assert_eq!(items[n + 1].category_id, Some(ROCK), "seed {seed}");
        }
    }

    #[test]
    fn imaging_goes_over_intros_that_fit_and_never_over_the_voice() {
        for seed in SEEDS {
            let mut f = station();
            f.clocks[0].slots = vec![slot(None, SlotItem::Category { category_id: POP, count: 1000, require_codes: vec![], exclude_codes: vec![] })];
            f.config.imaging = ImagingRules { jingle_category: Some(JINGLES), jingle_every: 2, sweeper_on_change: None };
            // Jingles of 3 s; even pop tracks have a 12 s intro (manual marks), odd ones none.
            for t in f.tracks.iter_mut().filter(|t| t.id / 1000 == JINGLES) {
                t.duration_ms = 3_000;
            }
            for id in f.members[&POP].clone() {
                let intro = if id % 2 == 0 { Some(12_000) } else { None };
                f.cues.insert(id, crate::analysis::TrackCues {
                    track_id: id, duration_ms: 200_000, cue_in_ms: 0, cue_out_ms: 200_000,
                    intro_end_ms: intro, outro_start_ms: None, source: crate::analysis::CueSource::Manual,
                    confidence: 1.0, loudness_lufs: None, peak_dbfs: None, analyzed_at: None,
                });
            }
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(8, 0), 2);
            let overlaid: Vec<_> = items.iter().filter(|i| i.overlay.is_some()).collect();
            assert!(!overlaid.is_empty(), "seed {seed}: no talk-over");
            for song in &overlaid {
                assert_eq!(song.track_id.unwrap() % 2, 0, "only songs with an intro");
                let spec = song.overlay.as_ref().unwrap();
                // Ends 500 ms before the voice at 12 s.
                assert_eq!(spec.start_ms + spec.item.duration_ms + 500, 12_000);
                assert_eq!(spec.item.kind, ItemKind::Jingle);
            }
            // Jingles that could not go over a song still play, in sequence, before an odd song.
            for (n, item) in items.iter().enumerate().filter(|(_, i)| i.kind == ItemKind::Jingle) {
                if let Some(next) = items.get(n + 1) {
                    assert!(next.track_id.unwrap() % 2 == 1 || next.kind != ItemKind::Music, "seed {seed}: a jingle in sequence before a song with room");
                }
                let _ = item;
            }
        }
    }

    #[test]
    fn lands_near_soft_anchor() {
        for seed in SEEDS {
            // Songs before the half-hour ID are picked by length: the ID starts within the wait.
            let mut f = station();
            let mut g = Generator::with_seed(seed);
            let items = run(&mut f, &mut g, at(10, 0), 6);
            for h in 10..16 {
                let id = items
                    .iter()
                    .find(|i| i.kind == ItemKind::StationId && i.planned_at.unwrap() >= at(h, 29))
                    .unwrap();
                let late = (id.planned_at.unwrap() - at(h, 30)).num_seconds();
                assert!((-EARLY_SECS..=120).contains(&late), "seed {seed}: ID at {h}:30 off by {late} s");
            }
        }
    }

    /// Cost of picking from a 50 000-track library (run with --ignored --nocapture).
    #[test]
    #[ignore]
    fn bench_big_library() {
        let mut f = Fake::default();
        f.category(POP, ItemKind::Music, 50_000, 5_000, 3);
        f.category(IDS, ItemKind::StationId, 3, 0, 0);
        f.clocks.push(Clock {
            id: "c".into(),
            name: "c".into(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![slot(Some(0), SlotItem::Imaging { category_id: IDS }), slot(None, SlotItem::Fill { category_id: POP })],
            fill_category: None,
        });
        f.config.enabled = true;
        f.config.default_clock = Some("c".into());
        let mut g = Generator::with_seed(7);
        let started = std::time::Instant::now();
        g.generate(&mut f, at(0, 0), 1, 0, 0).unwrap();
        println!("first call (loads the category): {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let items = run(&mut f, &mut g, at(0, 1), 24);
        println!("{} items in {:?}: {:?} per item", items.len(), started.elapsed(), started.elapsed() / items.len() as u32);
    }
}
