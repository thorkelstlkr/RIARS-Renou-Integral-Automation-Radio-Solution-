use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use super::crossfade::CrossfadeConfig;
use super::mixer::Mixer;
use super::output::{AudioOutput, Command};
use super::player::TrackPlayer;
use crate::playlist::playlist::Playlist;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
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

pub struct AudioEngine {
    state: PlaybackState,
    volume: f32,
    mixer: Mixer,
    current_track: Option<LoadedTrack>,
    playlist: Playlist,
    /// Set while the caller of `poll` decodes the next track.
    advancing: bool,
    /// `None` when no sound card is available (e.g. a headless server); the engine
    /// then keeps track of state without producing sound.
    output: Option<AudioOutput>,
    crossfade: CrossfadeConfig,
    sample_rate: u32,
}

impl AudioEngine {
    pub fn new() -> Result<Self, String> {
        let sample_rate = 44100;
        let volume = 0.8;
        let output = match AudioOutput::open(volume) {
            Ok(output) => Some(output),
            Err(e) => {
                log::warn!("No audio output, running silent: {e}");
                None
            }
        };
        let mut mixer = Mixer::new(sample_rate);
        mixer.set_master_volume(volume);
        Ok(Self {
            state: PlaybackState::Stopped,
            volume,
            mixer,
            current_track: None,
            playlist: Playlist::new("On air".to_string()),
            advancing: false,
            output,
            crossfade: CrossfadeConfig::default(),
            sample_rate,
        })
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
        if index < self.playlist.len() {
            self.playlist.current_index = index;
        }
        self.load_decoded(player);
        self.play();
    }

    /// Call periodically from the control side. When the current track has finished,
    /// returns the playlist index to play next, once; the caller decodes it and calls
    /// `start_index`, or `end_advance` if nothing could be played.
    pub fn poll(&mut self) -> Option<usize> {
        let finished = self.state == PlaybackState::Playing
            && self
                .output
                .as_ref()
                .is_some_and(|o| o.shared.finished.load(Ordering::Relaxed));
        if !finished || self.advancing {
            return None;
        }
        match self.playlist.next_index() {
            Some(index) => {
                self.advancing = true;
                Some(index)
            }
            None => {
                self.stop();
                None
            }
        }
    }

    /// Give up an advance started by `poll`.
    pub fn end_advance(&mut self) {
        self.advancing = false;
        self.stop();
    }

    pub fn playlist(&self) -> &Playlist {
        &self.playlist
    }

    pub fn playlist_mut(&mut self) -> &mut Playlist {
        &mut self.playlist
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.mixer.set_master_volume(self.volume);
        if let Some(output) = &self.output {
            output.shared.set_volume(self.mixer.master_volume());
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

    /// Hand an already decoded track to the audio thread.
    pub fn load_decoded(&mut self, player: TrackPlayer) {
        let path = player.path().to_path_buf();
        log::info!("Loaded track: {}", path.display());
        // The library scan does not read durations, so fill them in once decoded.
        let path_str = path.to_string_lossy();
        let duration_ms = player.duration().as_millis() as u64;
        for item in self.playlist.items.iter_mut().filter(|i| i.track_path == path_str) {
            item.duration_ms = duration_ms;
        }
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
                lock()?.start_index(index, player);
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
