use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::crossfade::CrossfadeConfig;
use super::mixer::Mixer;
use super::output::{AudioOutput, Command, Levels, StreamTap};
use super::normalize::{db_to_gain, NormalizationConfig};
use super::live::{CartVoice, MicConfig, MicInput, MAX_CART_VOICES};
use super::player::{Overlay, TrackPlayer};
use crate::live::{CartButton, CartMode};
use crate::playlist::playlist::{Playlist, PlaylistItem};
use crate::session::SessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// What to do on dead air (silence while playing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeadAirAction {
    /// Jump to the next playlist item.
    #[default]
    Skip,
    /// Only raise the alarm.
    Alert,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadAirConfig {
    pub enabled: bool,
    /// Level (dBFS, before volume) under which audio counts as silence.
    pub threshold_db: f32,
    /// How long silence must last to count as dead air.
    pub seconds: u32,
    pub action: DeadAirAction,
}

impl DeadAirConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=600).contains(&self.seconds) {
            return Err("seconds must be 1 to 600".to_string());
        }
        if !(-90.0..=0.0).contains(&self.threshold_db) {
            return Err("threshold_db must be -90 to 0".to_string());
        }
        Ok(())
    }
}

impl Default for DeadAirConfig {
    fn default() -> Self {
        Self { enabled: true, threshold_db: -50.0, seconds: 10, action: DeadAirAction::Skip }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineStateInfo {
    pub state: PlaybackState,
    pub volume: f32,
    pub current_track: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// Silence has lasted longer than the dead-air limit right now.
    pub dead_air: bool,
    /// Dead-air incidents since start.
    pub dead_air_count: u64,
}

/// Why an item stopped being on air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// Played to its end (or into the crossfade with the next one).
    Finished,
    /// Cut short by the operator or a scheduled jump.
    Skipped,
    /// Skipped by dead-air detection.
    DeadAir,
    /// Playback stopped.
    Stopped,
}

/// A playlist item that went on air and came off it: what proof of play records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayRecord {
    pub item: PlaylistItem,
    pub started_at: chrono::NaiveDateTime,
    pub ended_at: chrono::NaiveDateTime,
    pub reason: EndReason,
}

/// Measured loudness of a file by path: (LUFS, peak dBFS). Supplied by the app, which
/// owns the library; called on control threads only, never with the engine locked.
pub type LoudnessLookup = std::sync::Arc<dyn Fn(&str) -> Option<(f32, Option<f32>)> + Send + Sync>;

/// How the playlist moves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlayMode {
    /// The list moves on by itself and the automation fills it.
    #[default]
    Automatic,
    /// The list moves on by itself, but the automation adds nothing: what the
    /// presenter prepared plays.
    Assisted,
    /// Every item stops at its end and waits for Next.
    Manual,
}

/// The microphone as the screens show it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MicStatus {
    /// The input card is open (the microphone can go on air).
    pub available: bool,
    /// On air.
    pub on: bool,
    pub device: Option<String>,
    /// Loudest level since the previous read (linear, 1 = full scale).
    pub level: f32,
    pub speaking: bool,
    pub error: Option<String>,
}

/// A sound-bank button playing now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartPlaying {
    pub button: u32,
    /// Milliseconds left; `None` while it loops.
    pub left_ms: Option<u32>,
}

/// A bank sound sent to the audio thread, until it comes off air.
struct CartAir {
    serial: u64,
    item: PlaylistItem,
    started: chrono::NaiveDateTime,
    fired: std::time::Instant,
    seen: bool,
}

/// Decoded bank sounds, shared by every press of their buttons.
struct CartSound {
    samples: std::sync::Arc<[f32]>,
    channels: usize,
    rate: u32,
}

/// Decoded bank audio kept in memory, in samples (about 96 MB of f32).
const CART_CACHE_SAMPLES: usize = 24_000_000;

/// Records kept for the caller to collect; older ones are dropped beyond this.
const MAX_PLAY_RECORDS: usize = 256;

fn wall_clock() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// What the control side knows about the track handed to the audio thread.
struct LoadedTrack {
    name: String,
    duration: Duration,
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// How long before the end of a track (or of the crossfade into the next one) the next
/// track is decoded and queued.
const PRELOAD_MS: u64 = 5000;

pub struct AudioEngine {
    state: PlaybackState,
    volume: f32,
    mixer: Mixer,
    current_track: Option<LoadedTrack>,
    playlist: Playlist,
    /// Set while the caller of `poll` decodes the next track.
    advancing: bool,
    /// Bumped on every playlist change (items, order, current item, durations), so
    /// UIs can skip fetching an unchanged playlist.
    playlist_revision: u64,
    /// Bumped on every seek, so a jump is saved at once (see `session_key`).
    seek_count: u64,
    dead_air: DeadAirConfig,
    /// The current silence was already reported (one incident per silence).
    dead_air_active: bool,
    dead_air_count: u64,
    /// The next item comes from a dead-air skip: start it at once, do not queue it.
    skip_now: bool,
    /// Nothing after the current track could be decoded; stop when it ends.
    advance_blocked: bool,
    /// Next playlist item, already handed to the audio thread. Kept as the item, not
    /// its index: the playlist may be edited while it waits.
    queued: Option<(PlaylistItem, LoadedTrack)>,
    /// Playlist item on air and when it started.
    on_air: Option<(PlaylistItem, chrono::NaiveDateTime)>,
    /// Items that came off air, for the caller to log (`take_play_records`).
    play_records: Vec<PlayRecord>,
    /// Why the next start ends the current item, when it is not obvious.
    end_reason: Option<EndReason>,
    /// Revision bumps from `append_generated`, which need no session save (the
    /// generator makes them again after a restart).
    generated_bumps: u64,
    normalization: NormalizationConfig,
    loudness: Option<LoudnessLookup>,
    /// Imaging attached to tracks handed to the audio thread, waiting to start, with
    /// the id of the host item.
    overlays_pending: Vec<(String, PlaylistItem)>,
    /// Imaging playing over the current track, and when it started.
    overlay_on_air: Option<(PlaylistItem, chrono::NaiveDateTime)>,
    overlays_started_seen: u64,
    overlays_ended_seen: u64,
    /// Last value of `Shared::switches` seen, to notice the audio thread moving on.
    switches_seen: u64,
    /// `None` when no sound card is available (e.g. a headless server); the engine
    /// then keeps track of state without producing sound.
    output: Option<AudioOutput>,
    crossfade: CrossfadeConfig,
    sample_rate: u32,
    play_mode: PlayMode,
    /// Stopped at the end of an item (manual mode or "stop after"); Play starts this
    /// item (by id) next.
    held: Option<String>,
    mic: Option<MicInput>,
    mic_config: MicConfig,
    mic_error: Option<String>,
    /// Bumped when the play mode or microphone settings change, for the session save.
    live_settings_rev: u64,
    cart_serial: u64,
    carts_air: Vec<CartAir>,
    cart_cache: std::collections::HashMap<String, CartSound>,
    /// Insertion order of `cart_cache`, oldest first, and its size in samples.
    cart_cache_order: std::collections::VecDeque<String>,
    cart_cache_samples: usize,
}

impl AudioEngine {
    /// Play through the default sound card, or the internal clock if there is none.
    pub fn new() -> Result<Self, String> {
        Self::with_output(true)
    }

    /// Never touch a sound card: playback runs on the internal clock, for servers that
    /// only stream.
    pub fn new_headless() -> Result<Self, String> {
        Self::with_output(false)
    }

    /// Play on this sound card (by name), or the default one if it is missing.
    pub fn with_device(device: &str) -> Result<Self, String> {
        Self::with_choice(super::output::DeviceChoice::Named(device.to_string()))
    }

    fn with_output(use_device: bool) -> Result<Self, String> {
        Self::with_choice(if use_device { super::output::DeviceChoice::Default } else { super::output::DeviceChoice::Clock })
    }

    fn with_choice(choice: super::output::DeviceChoice) -> Result<Self, String> {
        let sample_rate = 44100;
        let volume = 0.8;
        let output = match AudioOutput::open_on(volume, choice) {
            Ok(output) => Some(output),
            Err(e) => {
                log::warn!("No audio output, running silent: {e}");
                None
            }
        };
        let crossfade = CrossfadeConfig::default();
        if let Some(output) = &output {
            output.shared.set_crossfade(&crossfade);
        }
        if let Some(output) = &output {
            output.shared.set_silence_level(db_to_linear(DeadAirConfig::default().threshold_db));
        }
        let mut mixer = Mixer::new(sample_rate);
        mixer.set_master_volume(volume);
        Ok(Self {
            state: PlaybackState::Stopped,
            volume,
            mixer,
            current_track: None,
            playlist: Playlist::new("On air".to_string()),
            advancing: false,
            playlist_revision: 0,
            seek_count: 0,
            dead_air: DeadAirConfig::default(),
            dead_air_active: false,
            dead_air_count: 0,
            skip_now: false,
            advance_blocked: false,
            queued: None,
            on_air: None,
            play_records: Vec::new(),
            end_reason: None,
            generated_bumps: 0,
            normalization: NormalizationConfig::default(),
            loudness: None,
            overlays_pending: Vec::new(),
            overlay_on_air: None,
            overlays_started_seen: 0,
            overlays_ended_seen: 0,
            switches_seen: 0,
            output,
            crossfade,
            sample_rate,
            play_mode: PlayMode::Automatic,
            held: None,
            mic: None,
            mic_config: MicConfig::default(),
            mic_error: None,
            live_settings_rev: 0,
            cart_serial: 0,
            carts_air: Vec::new(),
            cart_cache: std::collections::HashMap::new(),
            cart_cache_order: std::collections::VecDeque::new(),
            cart_cache_samples: 0,
        })
    }

    /// Jump to `position` in the current track (clamped to its length).
    pub fn seek(&mut self, position: Duration) -> Result<(), String> {
        let track = self.current_track.as_ref().ok_or("No track loaded")?;
        let position = position.min(track.duration);
        if let Some(output) = &self.output {
            output.shared.position_ms.store(position.as_millis() as u64, Ordering::Relaxed);
            output.send(Command::Seek(position));
        }
        self.seek_count += 1;
        Ok(())
    }

    /// Output peaks since the previous call (silence without an output).
    pub fn take_levels(&self) -> Levels {
        self.output.as_ref().map(|o| o.shared.take_levels()).unwrap_or_default()
    }

    /// The audio sent to the sound card, for streaming. Can be taken once.
    pub fn take_stream_tap(&mut self) -> Option<StreamTap> {
        self.output.as_mut().and_then(|o| o.take_tap())
    }

    /// Name of the sound card in use, if any.
    pub fn output_device(&self) -> Option<&str> {
        self.output.as_ref().map(|o| o.device_name())
    }

    pub fn play(&mut self) {
        if self.current_track.is_none() {
            return;
        }
        if let Some(output) = &self.output {
            // Pressing play after the track ended starts it again.
            if output.shared.finished.load(Ordering::Relaxed) {
                output.send(Command::Seek(Duration::ZERO));
            }
            output.shared.playing.store(true, Ordering::Relaxed);
        }
        if self.on_air.is_none() {
            if let Some(item) = self.playlist.current_item() {
                self.on_air = Some((item.clone(), wall_clock()));
            }
        }
        self.state = PlaybackState::Playing;
        log::info!("Playback started");
    }

    pub fn pause(&mut self) {
        if self.playback_state() == PlaybackState::Playing {
            if let Some(output) = &self.output {
                output.shared.playing.store(false, Ordering::Relaxed);
            }
            self.state = PlaybackState::Paused;
            log::info!("Playback paused");
        }
    }

    pub fn stop(&mut self) {
        let finished = self.output.as_ref().is_some_and(|o| o.shared.finished.load(Ordering::Relaxed));
        self.end_on_air(if finished { EndReason::Finished } else { EndReason::Stopped });
        if let Some(output) = &self.output {
            output.shared.playing.store(false, Ordering::Relaxed);
            output.send(Command::Seek(Duration::ZERO));
        }
        self.state = PlaybackState::Stopped;
        log::info!("Playback stopped");
    }

    /// Load and play the next playlist item. Stops when the playlist has nothing left.
    /// Decodes while holding `&mut self`; UIs that must stay responsive should use
    /// `next_index` + `TrackPlayer::from_file` + `start_index` instead.
    pub fn next_track(&mut self) -> Result<(), String> {
        match self.playlist.next_index() {
            Some(index) => {
                log::info!("Skipping to next track");
                self.play_index(index)
            }
            None => {
                self.stop();
                Ok(())
            }
        }
    }

    /// Load and play the playlist item at `index`.
    pub fn play_index(&mut self, index: usize) -> Result<(), String> {
        let path = self.item_path(index)?;
        let player = TrackPlayer::from_file(Path::new(&path), self.sample_rate)?;
        self.start_index(index, player);
        Ok(())
    }

    /// Id of the playlist item at `index`.
    pub fn item_id(&self, index: usize) -> Result<String, String> {
        self.playlist
            .items
            .get(index)
            .map(|item| item.id.clone())
            .ok_or_else(|| format!("No playlist item at {index}"))
    }

    /// Close the record of the item on air.
    fn end_on_air(&mut self, reason: EndReason) {
        let reason = self.end_reason.take().unwrap_or(reason);
        if let Some((item, started_at)) = self.on_air.take() {
            if self.play_records.len() >= MAX_PLAY_RECORDS {
                self.play_records.remove(0);
            }
            self.play_records.push(PlayRecord { item, started_at, ended_at: wall_clock(), reason });
        }
    }

    /// Items that came off air since the previous call, oldest first.
    pub fn take_play_records(&mut self) -> Vec<PlayRecord> {
        std::mem::take(&mut self.play_records)
    }

    pub fn has_play_records(&self) -> bool {
        !self.play_records.is_empty()
    }

    /// Playlist item on air and when it started.
    pub fn on_air(&self) -> Option<&(PlaylistItem, chrono::NaiveDateTime)> {
        self.on_air.as_ref()
    }

    /// Give up an advance started by `poll` without blocking it: the item went away
    /// while it was being decoded, and the next `poll` picks another.
    pub fn cancel_advance(&mut self) {
        self.advancing = false;
    }

    /// Append items from the automation generator and drop played items beyond
    /// `keep_played`. Not a change worth a session save on its own.
    pub fn append_generated(&mut self, items: Vec<PlaylistItem>, keep_played: usize) {
        let before = self.playlist_revision;
        if !items.is_empty() {
            self.playlist.items.extend(items);
            self.playlist_revision += 1;
            // A blocked advance (nothing left to play) can go on now.
            self.advance_blocked = false;
        }
        if self.playlist.prune_played(keep_played) > 0 {
            self.playlist_revision += 1;
        }
        self.generated_bumps += self.playlist_revision - before;
    }

    /// Path of the playlist item at `index`.
    pub fn item_path(&self, index: usize) -> Result<String, String> {
        self.playlist
            .items
            .get(index)
            .map(|item| item.track_path.clone())
            .ok_or_else(|| format!("No playlist item at {index}"))
    }

    pub fn next_index(&self) -> Option<usize> {
        self.playlist.next_index()
    }

    /// Make an already decoded playlist item current and play it. Fast: no decoding.
    pub fn start_index(&mut self, index: usize, mut player: TrackPlayer) {
        let finished = self.output.as_ref().is_some_and(|o| o.shared.finished.load(Ordering::Relaxed));
        self.end_on_air(if finished { EndReason::Finished } else { EndReason::Skipped });
        if let Some(item) = self.playlist.items.get(index) {
            player.set_no_overlap(item.no_overlap);
            self.on_air = Some((item.clone(), wall_clock()));
        }
        self.advancing = false;
        self.dead_air_active = false;
        self.advance_blocked = false;
        self.queued = None;
        self.held = None;
        if index < self.playlist.len() {
            self.playlist.current_index = index;
        }
        self.playlist_revision += 1;
        self.load_decoded(player);
        self.play();
    }

    /// Hand the next playlist item, decoded after `poll`, to the audio thread. While the
    /// current track is still playing it is queued and starts the instant this one ends.
    pub fn queue_or_start(&mut self, index: usize, mut player: TrackPlayer) {
        self.advancing = false;
        let still_playing = self.playback_state() == PlaybackState::Playing;
        if !still_playing || self.output.is_none() || std::mem::take(&mut self.skip_now) {
            self.start_index(index, player);
            return;
        }
        self.fill_duration(&player);
        let Some(item) = self.playlist.items.get(index).cloned() else {
            self.start_index(index, player);
            return;
        };
        player.set_no_overlap(item.no_overlap);
        self.queued = Some((
            item,
            LoadedTrack {
                name: player.file_name().to_string(),
                duration: player.duration(),
            },
        ));
        if let Some(output) = &self.output {
            log::info!("Queued next track: {}", player.path().display());
            output.send(Command::Queue(Box::new(player)));
        }
    }

    /// Call periodically from the control side (every ~250 ms). Returns a playlist index
    /// to decode, once per track: shortly before the current track ends (to queue it
    /// gaplessly) or after it ended. The caller decodes it and calls `queue_or_start`,
    /// or `end_advance` if nothing could be played.
    pub fn poll(&mut self) -> Option<usize> {
        let (switches, finished, position_ms) = match &self.output {
            Some(o) => (
                o.shared.switches.load(Ordering::Relaxed),
                o.shared.finished.load(Ordering::Relaxed),
                o.shared.position_ms.load(Ordering::Relaxed),
            ),
            None => return None,
        };
        if let Some(output) = &self.output {
            output.collect_garbage();
        }
        let (overlays_started, overlays_ended) = match &self.output {
            Some(o) => (o.shared.overlays_started.load(Ordering::Relaxed), o.shared.overlays_ended.load(Ordering::Relaxed)),
            None => (self.overlays_started_seen, self.overlays_ended_seen),
        };
        // The audio thread switched to the queued track: catch up.
        if switches != self.switches_seen {
            self.switches_seen = switches;
            if let Some((item, track)) = self.queued.take() {
                match self.playlist.index_of(&item.id) {
                    Some(index) => self.playlist.current_index = index,
                    None => log::warn!("Queued item {} left the playlist before it started", item.title),
                }
                self.end_on_air(EndReason::Finished);
                self.on_air = Some((item, wall_clock()));
                self.playlist_revision += 1;
                log::info!("Now playing: {}", track.name);
                self.current_track = Some(track);
            }
        }

        self.follow_overlays(overlays_started, overlays_ended);
        self.follow_carts();
        self.check_mic();
        if self.state != PlaybackState::Playing || self.advancing {
            return None;
        }
        if let Some(index) = self.check_dead_air() {
            return Some(index);
        }
        if self.queued.is_some() {
            return None;
        }
        let remaining_ms = self
            .current_track
            .as_ref()
            .map(|t| t.duration.as_millis() as u64)
            .unwrap_or(0)
            .saturating_sub(position_ms);
        let fade_ms = if self.crossfade.enabled { self.crossfade.duration.as_millis() as u64 } else { 0 };
        if !finished && remaining_ms > PRELOAD_MS + fade_ms {
            return None;
        }
        if self.holds_after_current() {
            if finished {
                self.hold();
            }
            return None;
        }
        if self.advance_blocked {
            if finished {
                self.stop();
            }
            return None;
        }
        match self.playlist.next_index() {
            Some(index) => {
                log::debug!("Preparing playlist item {index}");
                self.advancing = true;
                Some(index)
            }
            None => {
                if finished {
                    self.stop();
                }
                None
            }
        }
    }

    /// Report dead air once per silence; with the Skip action, return the next item
    /// to start at once.
    fn check_dead_air(&mut self) -> Option<usize> {
        let output = self.output.as_ref()?;
        let rate = output.shared.out_rate.load(Ordering::Relaxed).max(1) as u64;
        let silent_secs = output.shared.silent_frames.load(Ordering::Relaxed) / rate;
        if !self.dead_air.enabled || silent_secs < self.dead_air.seconds as u64 {
            self.dead_air_active = false;
            return None;
        }
        if self.dead_air_active {
            return None;
        }
        self.dead_air_active = true;
        self.dead_air_count += 1;
        let track = self.current_track.as_ref().map(|t| t.name.as_str()).unwrap_or("?");
        log::warn!("Dead air: {silent_secs} s of silence in {track}");
        if self.dead_air.action != DeadAirAction::Skip {
            return None;
        }
        let next = self.playlist.next_index()?;
        // Drop a queued crossfade partner: we jump now.
        self.queued = None;
        self.end_reason = Some(EndReason::DeadAir);
        self.skip_now = true;
        self.advancing = true;
        Some(next)
    }

    pub fn dead_air_config(&self) -> &DeadAirConfig {
        &self.dead_air
    }

    pub fn set_dead_air(&mut self, config: DeadAirConfig) {
        if let Some(output) = &self.output {
            let level = if config.enabled { db_to_linear(config.threshold_db) } else { 0.0 };
            output.shared.set_silence_level(level);
        }
        self.dead_air_active = false;
        self.dead_air = config;
    }

    /// Give up an advance started by `poll`: nothing after the current track is playable.
    /// The current track plays to its end, then playback stops.
    pub fn end_advance(&mut self) {
        self.advancing = false;
        self.advance_blocked = true;
        if self
            .output
            .as_ref()
            .map_or(true, |o| o.shared.finished.load(Ordering::Relaxed))
        {
            self.stop();
        }
    }

    /// "Artist - Title" for the track playing now, for stream metadata.
    pub fn now_playing_title(&self) -> Option<String> {
        let track = self.current_track.as_ref()?;
        let item = self
            .playlist
            .current_item()
            .filter(|item| item.track_path.ends_with(&track.name));
        Some(match item {
            Some(item) if !item.artist.is_empty() => format!("{} - {}", item.artist, item.title),
            Some(item) => item.title.clone(),
            None => track.name.clone(),
        })
    }

    pub fn playlist(&self) -> &Playlist {
        &self.playlist
    }

    /// Replace the on-air playlist (e.g. with a saved one). Playback is not touched:
    /// the current track plays on, and auto-advance continues in the new list.
    pub fn set_playlist(&mut self, mut playlist: Playlist) {
        playlist.current_index = 0;
        self.playlist = playlist;
        self.playlist_revision += 1;
        self.queued = None;
        self.advancing = false;
        self.advance_blocked = false;
    }

    /// Put `incoming` on air, as from another playlist tab, without touching the audio:
    /// the track playing goes on to its end, then `incoming` follows. The track playing
    /// (and the next one, if already handed to the audio thread for a gapless start or
    /// a crossfade) move to the top of the new list. Returns the list leaving the air,
    /// cut down to what it had left to play, so putting it back resumes where it was.
    pub fn take_over(&mut self, mut incoming: Playlist) -> Playlist {
        let playing = self.playback_state() != PlaybackState::Stopped && self.on_air.is_some();
        let current = self.playlist.current_index;
        let queued_id = self.queued.as_ref().map(|(item, _)| item.id.clone());
        let mut head = Vec::new();
        let mut rest = Vec::new();
        for (index, item) in self.playlist.items.iter().enumerate() {
            if playing && (index == current || Some(&item.id) == queued_id.as_ref()) {
                head.push(item.clone());
            } else if index > current || (!playing && index == current) {
                rest.push(item.clone());
            }
        }
        let mut leaving = std::mem::replace(&mut self.playlist, Playlist::new(String::new()));
        leaving.items = rest;
        leaving.current_index = 0;
        // Items keep their ids. A decode under way for an item of the old list finds it
        // gone and is dropped (`play_index_unlocked`), unless it was carried over.
        let carried: std::collections::HashSet<String> = head.iter().map(|i| i.id.clone()).collect();
        incoming.items.retain(|item| !carried.contains(&item.id));
        head.append(&mut incoming.items);
        incoming.items = head;
        incoming.current_index = 0;
        self.playlist = incoming;
        self.advance_blocked = false;
        self.playlist_revision += 1;
        leaving
    }

    /// What to save so a restart can carry on (the stream is added by the caller).
    pub fn snapshot(&self) -> SessionState {
        SessionState {
            playlist: Some(self.playlist.clone()),
            position_ms: self.state_info().position_ms,
            playing: self.playback_state() == PlaybackState::Playing,
            volume: Some(self.volume),
            crossfade: Some(self.crossfade.clone()),
            stream: None,
            dead_air: Some(self.dead_air.clone()),
            normalization: Some(self.normalization),
            play_mode: Some(self.play_mode),
            mic: Some(self.mic_config.clone()),
        }
    }

    /// Changes worth saving at once (not the position, which moves all the time).
    pub fn session_key(&self) -> (u64, u32, u64, bool, bool) {
        let normalization = (self.normalization.enabled as u64) << 62 ^ (self.normalization.target_lufs.to_bits() as u64) << 20;
        let dead_air = (self.dead_air.enabled as u64) << 63
            ^ (self.dead_air.seconds as u64) << 40
            ^ (self.dead_air.threshold_db.to_bits() as u64) << 8
            ^ self.dead_air.action as u64;
        (
            self.playlist_revision
                .wrapping_sub(self.generated_bumps)
                .wrapping_add(self.seek_count << 32)
                .wrapping_add(self.live_settings_rev << 48)
                ^ dead_air
                ^ normalization,
            self.volume.to_bits(),
            self.crossfade.duration.as_millis() as u64,
            self.crossfade.enabled,
            self.playback_state() == PlaybackState::Playing,
        )
    }

    /// Put back a saved playlist (keeping its current item), volume and crossfade.
    /// Playback is resumed by `session::resume`.
    pub fn restore(&mut self, state: &SessionState) {
        if let Some(normalization) = state.normalization {
            self.normalization = normalization;
        }
        if let Some(mode) = state.play_mode {
            self.play_mode = mode;
        }
        if let Some(mic) = &state.mic {
            if mic.validate().is_ok() {
                self.mic_config = mic.clone();
                if let Some(output) = &self.output {
                    output.shared.mic.apply(mic);
                }
            }
        }
        if let Some(volume) = state.volume {
            self.set_volume(volume);
        }
        if let Some(crossfade) = &state.crossfade {
            self.set_crossfade(crossfade.clone());
        }
        if let Some(dead_air) = &state.dead_air {
            self.set_dead_air(dead_air.clone());
        }
        if let Some(playlist) = &state.playlist {
            self.playlist = playlist.clone();
            if self.playlist.current_index >= self.playlist.len() {
                self.playlist.current_index = 0;
            }
            self.playlist_revision += 1;
        }
    }

    pub fn playlist_revision(&self) -> u64 {
        self.playlist_revision
    }

    pub fn playlist_mut(&mut self) -> &mut Playlist {
        self.playlist_revision += 1;
        &mut self.playlist
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.mixer.set_master_volume(self.volume);
        if let Some(output) = &self.output {
            output.shared.set_volume(self.mixer.master_volume());
        }
    }

    /// Glide the volume to `level` over `duration_ms` (smoothly, in the audio thread).
    pub fn fade_to_volume(&mut self, level: f32, duration_ms: u64) {
        let level = level.clamp(0.0, 1.0);
        let delta = (level - self.volume).abs();
        if duration_ms == 0 || delta < 0.001 {
            self.set_volume(level);
            return;
        }
        self.volume = level;
        self.mixer.set_master_volume(level);
        if let Some(output) = &self.output {
            let ms_per_unit = (duration_ms as f32 / delta).min(u32::MAX as f32) as u32;
            output.shared.ramp_volume(self.mixer.master_volume(), ms_per_unit.max(1));
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// The rate tracks are decoded at for this engine (see [`Self::load_decoded`]).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn load_track(&mut self, path: &str) -> Result<(), String> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(format!("File not found: {}", path.display()));
        }
        // Decoding allocates and reads the file, so it happens here, never on the audio thread.
        let player = TrackPlayer::from_file(path, self.sample_rate)?;
        self.load_decoded(player);
        Ok(())
    }

    /// The library scan may not know a duration; take it from the decoded track.
    fn fill_duration(&mut self, player: &TrackPlayer) {
        let path_str = player.path().to_string_lossy();
        let duration_ms = player.duration().as_millis() as u64;
        for item in self.playlist.items.iter_mut().filter(|i| i.track_path == path_str) {
            if item.duration_ms != duration_ms {
                item.duration_ms = duration_ms;
                self.playlist_revision += 1;
            }
        }
    }

    /// Hand an already decoded track to the audio thread.
    pub fn load_decoded(&mut self, player: TrackPlayer) {
        let path = player.path().to_path_buf();
        log::info!("Loaded track: {}", path.display());
        self.fill_duration(&player);
        self.current_track = Some(LoadedTrack {
            name: player.file_name().to_string(),
            duration: player.duration(),
        });
        if let Some(output) = &self.output {
            output.shared.position_ms.store(0, Ordering::Relaxed);
            // Clear it here too, so `poll` does not see the old track's end before the
            // audio thread picks up the new one.
            output.shared.finished.store(false, Ordering::Relaxed);
            output.send(Command::Load(Box::new(player)));
        }
    }

    pub fn state_info(&self) -> EngineStateInfo {
        let (current_track, duration_ms) = match &self.current_track {
            Some(track) => (Some(track.name.clone()), track.duration.as_millis() as u64),
            None => (None, 0),
        };
        let position_ms = self
            .output
            .as_ref()
            .map(|o| o.shared.position_ms.load(Ordering::Relaxed).min(duration_ms))
            .unwrap_or(0);

        EngineStateInfo {
            dead_air: self.dead_air_active,
            dead_air_count: self.dead_air_count,
            state: self.playback_state(),
            volume: self.volume,
            current_track,
            position_ms,
            duration_ms,
        }
    }

    pub fn playback_state(&self) -> PlaybackState {
        // The audio thread stops by itself at the end of a track.
        match &self.output {
            Some(output)
                if self.state == PlaybackState::Playing
                    && output.shared.finished.load(Ordering::Relaxed) =>
            {
                PlaybackState::Stopped
            }
            _ => self.state,
        }
    }

    pub fn crossfade_config(&self) -> &CrossfadeConfig {
        &self.crossfade
    }

    pub fn normalization_config(&self) -> NormalizationConfig {
        self.normalization
    }

    pub fn set_normalization(&mut self, config: NormalizationConfig) {
        self.normalization = config;
    }

    /// Where the engine reads each file's loudness for normalisation.
    pub fn set_loudness_lookup(&mut self, lookup: LoudnessLookup) {
        self.loudness = Some(lookup);
    }

    /// Imaging attached to the player of item `host_id`: its proof of play starts when
    /// the audio thread starts it.
    fn note_overlay(&mut self, host_id: &str, overlay: PlaylistItem) {
        self.overlays_pending.retain(|(host, _)| host != host_id);
        self.overlays_pending.push((host_id.to_string(), overlay));
    }

    /// Follow imaging started and ended by the audio thread (from `poll`).
    fn follow_overlays(&mut self, started: u64, ended: u64) {
        // An end first: the audio thread retires the old overlay before starting a new one.
        if ended != self.overlays_ended_seen {
            self.overlays_ended_seen = ended;
            if let Some((item, started_at)) = self.overlay_on_air.take() {
                if self.play_records.len() >= MAX_PLAY_RECORDS {
                    self.play_records.remove(0);
                }
                self.play_records.push(PlayRecord { item, started_at, ended_at: wall_clock(), reason: EndReason::Finished });
            }
        }
        if started != self.overlays_started_seen {
            self.overlays_started_seen = started;
            let host = self.on_air.as_ref().map(|(item, _)| item.id.clone());
            if let Some(pos) = self.overlays_pending.iter().position(|(h, _)| Some(h) == host.as_ref()) {
                let (_, item) = self.overlays_pending.remove(pos);
                self.overlay_on_air = Some((item, wall_clock()));
            }
        }
        // Imaging of items that left the air without reaching it never plays.
        let keep = [self.on_air.as_ref().map(|(i, _)| i.id.clone()), self.queued.as_ref().map(|(i, _)| i.id.clone())];
        self.overlays_pending.retain(|(host, _)| keep.iter().any(|k| k.as_deref() == Some(host.as_str())));
    }

    // --- Play modes ---------------------------------------------------------------

    pub fn play_mode(&self) -> PlayMode {
        self.play_mode
    }

    pub fn set_play_mode(&mut self, mode: PlayMode) {
        if mode != self.play_mode {
            log::info!("Play mode: {mode:?}");
            self.play_mode = mode;
            self.live_settings_rev += 1;
        }
    }

    /// The item on air must not run into the next one.
    fn holds_after_current(&self) -> bool {
        self.play_mode == PlayMode::Manual || self.playlist.current_item().is_some_and(|i| i.stop_after)
    }

    /// The item on air ended and the list waits: stop, with the next item ready for Play.
    fn hold(&mut self) {
        self.end_on_air(EndReason::Finished);
        if let Some(output) = &self.output {
            output.shared.playing.store(false, Ordering::Relaxed);
        }
        self.state = PlaybackState::Stopped;
        self.held = self.playlist.next_index().and_then(|i| self.playlist.items.get(i)).map(|i| i.id.clone());
        log::info!("Holding after the item on air; Play or Next starts the next one");
    }

    /// Index Play should start after a hold, if the list is holding.
    pub fn held_index(&self) -> Option<usize> {
        self.held.as_ref().and_then(|id| self.playlist.index_of(id))
    }

    // --- Microphone -----------------------------------------------------------------

    pub fn mic_config(&self) -> &MicConfig {
        &self.mic_config
    }

    /// Change the microphone settings; a new input card is opened at once if the
    /// microphone was open.
    pub fn set_mic_config(&mut self, config: MicConfig) -> Result<(), String> {
        config.validate()?;
        let reopen = self.mic.is_some() && config.device != self.mic_config.device;
        self.mic_config = config;
        self.live_settings_rev += 1;
        if let Some(output) = &self.output {
            output.shared.mic.apply(&self.mic_config);
        }
        if reopen {
            let on = self.output.as_ref().is_some_and(|o| o.shared.mic.on.load(Ordering::Relaxed));
            self.close_mic();
            self.open_mic()?;
            if on {
                self.set_mic_on(true, false)?;
            }
        }
        Ok(())
    }

    /// Open the input card (closed: off air). Returns its name.
    pub fn open_mic(&mut self) -> Result<String, String> {
        let Some(output) = &self.output else {
            return Err("No audio output: the microphone has nowhere to go".to_string());
        };
        if let Some(mic) = &self.mic {
            return Ok(mic.device_name.clone());
        }
        match MicInput::open(self.mic_config.device.as_deref()) {
            Ok((input, feed)) => {
                output.send(Command::Mic(Some(Box::new(feed))));
                let name = input.device_name.clone();
                self.mic = Some(input);
                self.mic_error = None;
                Ok(name)
            }
            Err(e) => {
                self.mic_error = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Take the microphone off air and close the input card.
    pub fn close_mic(&mut self) {
        if let Some(output) = &self.output {
            output.shared.mic.on.store(false, Ordering::Relaxed);
            output.send(Command::Mic(None));
        }
        self.mic = None;
    }

    /// Put the microphone on or off air. `remote` requests (API, agents) are refused
    /// unless the settings allow them.
    pub fn set_mic_on(&mut self, on: bool, remote: bool) -> Result<(), String> {
        if on && remote && !self.mic_config.allow_remote {
            return Err("Opening the microphone remotely is disabled in the microphone settings".to_string());
        }
        if on && self.mic.is_none() {
            self.open_mic()?;
        }
        if let Some(output) = &self.output {
            output.shared.mic.on.store(on, Ordering::Relaxed);
        }
        log::info!("Microphone {}", if on { "on air" } else { "off" });
        Ok(())
    }

    pub fn mic_status(&self) -> MicStatus {
        let Some(output) = &self.output else {
            return MicStatus { error: self.mic_error.clone(), ..MicStatus::default() };
        };
        let controls = &output.shared.mic;
        MicStatus {
            available: self.mic.is_some(),
            on: controls.on.load(Ordering::Relaxed),
            device: self.mic.as_ref().map(|m| m.device_name.clone()),
            level: controls.take_peak(),
            speaking: controls.speaking.load(Ordering::Relaxed),
            error: self.mic_error.clone(),
        }
    }

    /// A microphone whose card failed or went away falls silent and is closed.
    fn check_mic(&mut self) {
        if self.mic.as_ref().is_some_and(|m| !m.alive.load(Ordering::Relaxed)) {
            log::warn!("The microphone input stopped; closing it");
            self.mic_error = Some("The microphone input stopped (card unplugged or in use)".to_string());
            self.close_mic();
        }
    }

    // --- Sound bank -----------------------------------------------------------------

    /// A player over the cached audio of `path`, if it is decoded already. Instant.
    fn cached_cart(&self, path: &str) -> Option<TrackPlayer> {
        self.cart_cache
            .get(path)
            .map(|s| TrackPlayer::from_shared(path.into(), std::sync::Arc::clone(&s.samples), s.channels, s.rate))
    }

    /// Keep decoded bank audio for the next presses, forgetting the oldest beyond the
    /// cache size.
    fn cache_cart(&mut self, path: &str, player: &TrackPlayer) {
        let Some((samples, channels, rate)) = player.shared_samples() else { return };
        if self.cart_cache.contains_key(path) || samples.len() > CART_CACHE_SAMPLES {
            return;
        }
        while self.cart_cache_samples + samples.len() > CART_CACHE_SAMPLES {
            let Some(old) = self.cart_cache_order.pop_front() else { break };
            if let Some(sound) = self.cart_cache.remove(&old) {
                self.cart_cache_samples -= sound.samples.len();
            }
        }
        self.cart_cache_samples += samples.len();
        self.cart_cache_order.push_back(path.to_string());
        self.cart_cache.insert(path.to_string(), CartSound { samples, channels, rate });
    }

    /// Whether a voice of this button is playing.
    pub fn is_cart_playing(&self, button: u32) -> bool {
        self.carts_playing().iter().any(|c| c.button == button)
    }

    pub fn carts_playing(&self) -> Vec<CartPlaying> {
        let Some(output) = &self.output else { return Vec::new() };
        let status = &output.shared.carts;
        (0..MAX_CART_VOICES)
            .filter(|&i| status.serial[i].load(Ordering::Relaxed) != 0)
            .map(|i| {
                let left = status.left_ms[i].load(Ordering::Relaxed);
                CartPlaying { button: status.button[i].load(Ordering::Relaxed), left_ms: (left != u32::MAX).then_some(left) }
            })
            .collect()
    }

    /// Hand a decoded bank sound to the audio thread. A Fire button playing already
    /// starts again from the top.
    fn start_cart(&mut self, button: &CartButton, player: TrackPlayer, gain: f32) {
        let Some(output) = &self.output else { return };
        let id = button.id();
        if self.is_cart_playing(id) {
            output.send(Command::CartStop(Some(id)));
        }
        self.cart_serial += 1;
        let mut item = PlaylistItem::new(button.file_path.clone(), button.label.clone(), String::new(), player.duration().as_millis() as u64);
        item.kind = crate::playlist::playlist::ItemKind::Jingle;
        item.track_id = button.track_id;
        self.carts_air.push(CartAir {
            serial: self.cart_serial,
            item,
            started: wall_clock(),
            fired: std::time::Instant::now(),
            seen: false,
        });
        output.send(Command::CartPlay(CartVoice {
            player: Box::new(player),
            button: id,
            serial: self.cart_serial,
            looping: button.mode == CartMode::Loop,
            duck: button.duck,
            gain,
        }));
    }

    /// Fade out the bank sounds of `button`, or all of them.
    pub fn stop_carts(&mut self, button: Option<u32>) {
        if let Some(output) = &self.output {
            output.send(Command::CartStop(button));
        }
    }

    /// Bank sounds that came off air go to the proof-of-play records.
    fn follow_carts(&mut self) {
        if self.carts_air.is_empty() {
            return;
        }
        let Some(output) = &self.output else { return };
        let playing: Vec<u64> = output.shared.carts.serial.iter().map(|s| s.load(Ordering::Relaxed)).collect();
        let mut ended = Vec::new();
        self.carts_air.retain_mut(|air| {
            if playing.contains(&air.serial) {
                air.seen = true;
                return true;
            }
            // Not playing: over, or never seen because it was shorter than a poll.
            if air.seen || air.fired.elapsed() > Duration::from_secs(1) {
                ended.push(PlayRecord {
                    item: air.item.clone(),
                    started_at: air.started,
                    ended_at: wall_clock(),
                    reason: EndReason::Finished,
                });
                return false;
            }
            true
        });
        for record in ended {
            if self.play_records.len() >= MAX_PLAY_RECORDS {
                self.play_records.remove(0);
            }
            self.play_records.push(record);
        }
    }

    pub fn set_crossfade(&mut self, config: CrossfadeConfig) {
        if let Some(output) = &self.output {
            output.shared.set_crossfade(&config);
        }
        self.crossfade = config;
    }
}

/// Decode the playlist item at `index` without holding the engine lock, then start it.
/// With `skip_failures` (auto-advance), unplayable items are skipped and the engine
/// stops if none can be played. Blocking: call it from a worker thread.
pub fn play_index_unlocked(
    engine: &Mutex<AudioEngine>,
    index: usize,
    skip_failures: bool,
) -> Result<(), String> {
    let lock = || engine.lock().map_err(|e| e.to_string());
    let mut index = index;
    // Separate statement: a guard in the `for` expression would live for the whole
    // loop and deadlock on the next `lock()`.
    let attempts = lock()?.playlist().len().max(1);
    for _ in 0..attempts {
        let (path, id, item, normalization, loudness) = {
            let engine = lock()?;
            let item = engine.playlist.items.get(index).cloned();
            (engine.item_path(index)?, engine.item_id(index)?, item, engine.normalization, engine.loudness.clone())
        };
        match TrackPlayer::from_file(Path::new(&path), 44100) {
            Ok(mut player) => {
                // Gain and imaging are set up here, with no lock held: the loudness
                // lookup reads the library.
                let gain_of = |path: &str| {
                    let (lufs, peak) = loudness.as_ref().and_then(|l| l(path)).map_or((None, None), |(l, p)| (Some(l), p));
                    db_to_gain(normalization.gain_db(lufs, peak))
                };
                player.set_gain(gain_of(&path));
                let overlay = item.as_ref().and_then(|i| i.overlay.as_deref()).and_then(|spec| {
                    match TrackPlayer::from_file(Path::new(&spec.item.track_path), 44100) {
                        Ok(mut over) => {
                            over.set_gain(gain_of(&spec.item.track_path));
                            let start_frame = spec.start_ms as f64 * player.sample_rate() as f64 / 1000.0;
                            player.set_overlay(Overlay { player: Box::new(over), start_frame, duck: db_to_gain(-spec.duck_db.abs()) });
                            Some(spec.item.clone())
                        }
                        Err(e) => {
                            log::warn!("Cannot load imaging {}: {e}; the track plays without it", spec.item.track_path);
                            None
                        }
                    }
                });
                let mut engine = lock()?;
                if let Some(overlay) = overlay {
                    engine.note_overlay(&id, overlay);
                }
                // The playlist may have changed while decoding: find the item again.
                let Some(index) = engine.playlist().index_of(&id) else {
                    engine.cancel_advance();
                    if skip_failures {
                        log::info!("{path} left the playlist while loading; not playing it");
                        return Ok(());
                    }
                    return Err(format!("{path} left the playlist while loading"));
                };
                if skip_failures {
                    engine.queue_or_start(index, player);
                } else {
                    engine.start_index(index, player);
                }
                return Ok(());
            }
            Err(e) if skip_failures => {
                log::warn!("Skipping unplayable playlist item {path}: {e}");
                let mut engine = lock()?;
                engine.playlist_mut().current_index = index;
                match engine.next_index() {
                    Some(next) => index = next,
                    None => break,
                }
            }
            Err(e) => return Err(e),
        }
    }
    lock()?.end_advance();
    Ok(())
}

/// Play: after a hold (manual mode or "stop after"), the next item; otherwise resume
/// what is loaded. Decodes with no lock held.
pub fn play_unlocked(engine: &Mutex<AudioEngine>) -> Result<(), String> {
    let held = engine.lock().map_err(|e| e.to_string())?.held_index();
    match held {
        Some(index) => play_index_unlocked(engine, index, false),
        None => {
            engine.lock().map_err(|e| e.to_string())?.play();
            Ok(())
        }
    }
}

/// Press a sound-bank button: start its sound, or stop it when it is a Toggle or Loop
/// button that is playing. Returns whether it started. Decodes with no lock held; the
/// decoded audio is kept, so the next press is instant.
pub fn fire_cart_unlocked(engine: &Mutex<AudioEngine>, button: &CartButton) -> Result<bool, String> {
    let lock = || engine.lock().map_err(|e| e.to_string());
    let (cached, normalization, loudness) = {
        let mut engine = lock()?;
        if button.mode != CartMode::Fire && engine.is_cart_playing(button.id()) {
            engine.stop_carts(Some(button.id()));
            return Ok(false);
        }
        (engine.cached_cart(&button.file_path), engine.normalization, engine.loudness.clone())
    };
    let player = match cached {
        Some(player) => player,
        None => {
            let player = TrackPlayer::from_file(Path::new(&button.file_path), 44100)?;
            lock()?.cache_cart(&button.file_path, &player);
            player
        }
    };
    let (lufs, peak) = loudness
        .as_ref()
        .and_then(|l| l(&button.file_path))
        .map_or((None, None), |(l, p)| (Some(l), p));
    let gain = db_to_gain(button.gain_db) * db_to_gain(normalization.gain_db(lufs, peak));
    lock()?.start_cart(button, player, gain);
    Ok(true)
}

/// Decode the short sounds of a bank page ahead, so their first press is instant.
/// Long ones (streamed from disk) and missing files are skipped.
pub fn preload_carts_unlocked(engine: &Mutex<AudioEngine>, paths: &[String]) {
    for path in paths {
        let cached = engine.lock().map(|e| e.cart_cache.contains_key(path)).unwrap_or(true);
        if cached {
            continue;
        }
        if let Ok(player) = TrackPlayer::from_file(Path::new(path), 44100) {
            if let Ok(mut engine) = engine.lock() {
                engine.cache_cart(path, &player);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A 0.2 s silent in-memory track.
    fn short() -> TrackPlayer {
        TrackPlayer::from_samples(PathBuf::from("s.wav"), vec![0.0; 8820], 1, 44_100)
    }

    fn playlist(engine: &mut AudioEngine, n: usize) {
        for i in 0..n {
            engine.playlist_mut().add_item(format!("{i}.wav"), format!("t{i}"), String::new(), 200);
        }
    }

    /// Poll until the item on air ends (the internal clock plays in real time).
    fn run_out(engine: &mut AudioEngine) -> Option<usize> {
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(20));
            let finished = engine.output.as_ref().is_some_and(|o| o.shared.finished.load(Ordering::Relaxed));
            let next = engine.poll();
            if finished || next.is_some() {
                return next;
            }
        }
        None
    }

    #[test]
    fn manual_mode_and_stop_after_hold_the_next_item() {
        let mut engine = AudioEngine::new_headless().unwrap();
        playlist(&mut engine, 3);
        engine.set_play_mode(PlayMode::Manual);
        engine.start_index(0, short());
        assert_eq!(run_out(&mut engine), None, "manual: nothing is prepared");
        assert_eq!(engine.playback_state(), PlaybackState::Stopped);
        assert_eq!(engine.held_index(), Some(1));
        assert_eq!(engine.take_play_records().len(), 1);

        // Automatic, but item 1 says stop after it.
        engine.set_play_mode(PlayMode::Automatic);
        engine.playlist_mut().items[1].stop_after = true;
        engine.start_index(1, short());
        assert_eq!(engine.held_index(), None, "starting clears the hold");
        assert_eq!(run_out(&mut engine), None);
        assert_eq!(engine.held_index(), Some(2));

        // Without a hold, the next item is prepared as usual.
        engine.start_index(0, short());
        assert_eq!(run_out(&mut engine), Some(1));
    }

    #[test]
    fn take_over_keeps_the_track_on_air_and_returns_what_was_left() {
        let mut engine = AudioEngine::new_headless().unwrap();
        playlist(&mut engine, 4);
        let main_id = engine.playlist().id.clone();
        let mut programme = Playlist::new("Programme".into());
        programme.add_item("p0.wav".into(), "p0".into(), String::new(), 200);
        programme.add_item("p1.wav".into(), "p1".into(), String::new(), 200);
        let programme_id = programme.id.clone();

        // Stopped on item 1: nothing is carried over, item 1 is still to play.
        engine.playlist_mut().current_index = 1;
        let left = engine.take_over(programme.clone());
        assert_eq!(left.id, main_id);
        assert_eq!(left.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["t1", "t2", "t3"]);
        assert_eq!(engine.playlist().id, programme_id);
        assert_eq!(engine.playlist().items.len(), 2);

        // Playing item 0 of the main list again: it stays on air at the top of the
        // programme, which follows it; the main list keeps t1, t2.
        engine.take_over(left);
        engine.start_index(0, short());
        let left = engine.take_over(programme);
        assert_eq!(left.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["t2", "t3"]);
        let titles: Vec<_> = engine.playlist().items.iter().map(|i| i.title.clone()).collect();
        assert_eq!(titles, ["t1", "p0", "p1"]);
        assert_eq!(engine.playlist().current_index, 0);
        assert_eq!(engine.on_air().map(|(item, _)| item.title.as_str()), Some("t1"));
        assert_eq!(run_out(&mut engine), Some(1), "the programme follows the track on air");
    }

    #[test]
    fn remote_microphone_needs_permission_and_settings_are_checked() {
        let mut engine = AudioEngine::new_headless().unwrap();
        assert!(engine.set_mic_on(true, true).unwrap_err().contains("disabled"));
        let bad = MicConfig { gain_db: 99.0, ..MicConfig::default() };
        assert!(engine.set_mic_config(bad).is_err());
        let before = engine.session_key();
        engine.set_play_mode(PlayMode::Assisted);
        assert_ne!(engine.session_key(), before, "the play mode is saved");
        assert_eq!(engine.snapshot().play_mode, Some(PlayMode::Assisted));
    }
}
