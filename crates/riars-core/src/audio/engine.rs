use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use super::crossfade::CrossfadeConfig;
use super::mixer::Mixer;
use super::output::{AudioOutput, Command, Levels, StreamTap};
use super::player::TrackPlayer;
use crate::playlist::playlist::Playlist;
use crate::session::SessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineStateInfo {
    pub state: PlaybackState,
    pub volume: f32,
    pub current_track: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
}

/// What the control side knows about the track handed to the audio thread.
struct LoadedTrack {
    name: String,
    duration: Duration,
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
    /// Nothing after the current track could be decoded; stop when it ends.
    advance_blocked: bool,
    /// Next playlist item, already handed to the audio thread.
    queued: Option<(usize, LoadedTrack)>,
    /// Last value of `Shared::switches` seen, to notice the audio thread moving on.
    switches_seen: u64,
    /// `None` when no sound card is available (e.g. a headless server); the engine
    /// then keeps track of state without producing sound.
    output: Option<AudioOutput>,
    crossfade: CrossfadeConfig,
    sample_rate: u32,
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

    fn with_output(use_device: bool) -> Result<Self, String> {
        let sample_rate = 44100;
        let volume = 0.8;
        let output = match AudioOutput::open(volume, use_device) {
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
            advance_blocked: false,
            queued: None,
            switches_seen: 0,
            output,
            crossfade,
            sample_rate,
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
    pub fn start_index(&mut self, index: usize, player: TrackPlayer) {
        self.advancing = false;
        self.advance_blocked = false;
        self.queued = None;
        if index < self.playlist.len() {
            self.playlist.current_index = index;
        }
        self.playlist_revision += 1;
        self.load_decoded(player);
        self.play();
    }

    /// Hand the next playlist item, decoded after `poll`, to the audio thread. While the
    /// current track is still playing it is queued and starts the instant this one ends.
    pub fn queue_or_start(&mut self, index: usize, player: TrackPlayer) {
        self.advancing = false;
        let still_playing = self.playback_state() == PlaybackState::Playing;
        if !still_playing || self.output.is_none() {
            self.start_index(index, player);
            return;
        }
        self.fill_duration(&player);
        self.queued = Some((
            index,
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
        // The audio thread switched to the queued track: catch up.
        if switches != self.switches_seen {
            self.switches_seen = switches;
            if let Some((index, track)) = self.queued.take() {
                self.playlist.current_index = index;
                self.playlist_revision += 1;
                log::info!("Now playing: {}", track.name);
                self.current_track = Some(track);
            }
        }

        if self.state != PlaybackState::Playing || self.advancing || self.queued.is_some() {
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

    /// What to save so a restart can carry on (the stream is added by the caller).
    pub fn snapshot(&self) -> SessionState {
        SessionState {
            playlist: Some(self.playlist.clone()),
            position_ms: self.state_info().position_ms,
            playing: self.playback_state() == PlaybackState::Playing,
            volume: Some(self.volume),
            crossfade: Some(self.crossfade.clone()),
            stream: None,
        }
    }

    /// Changes worth saving at once (not the position, which moves all the time).
    pub fn session_key(&self) -> (u64, u32, u64, bool, bool) {
        (
            self.playlist_revision.wrapping_add(self.seek_count << 32),
            self.volume.to_bits(),
            self.crossfade.duration.as_millis() as u64,
            self.crossfade.enabled,
            self.playback_state() == PlaybackState::Playing,
        )
    }

    /// Put back a saved playlist (keeping its current item), volume and crossfade.
    /// Playback is resumed by `session::resume`.
    pub fn restore(&mut self, state: &SessionState) {
        if let Some(volume) = state.volume {
            self.set_volume(volume);
        }
        if let Some(crossfade) = &state.crossfade {
            self.set_crossfade(crossfade.clone());
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
        let path = lock()?.item_path(index)?;
        match TrackPlayer::from_file(Path::new(&path), 44100) {
            Ok(player) => {
                let mut engine = lock()?;
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
