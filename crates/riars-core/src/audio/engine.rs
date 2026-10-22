use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;

use super::crossfade::CrossfadeConfig;
use super::mixer::Mixer;
use super::output::{AudioOutput, Command};
use super::player::TrackPlayer;

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

    pub fn next_track(&mut self) {
        log::info!("Skipping to next track");
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
        log::info!("Loaded track: {}", path.display());
        self.current_track = Some(LoadedTrack {
            name: player.file_name().to_string(),
            duration: player.duration(),
        });
        if let Some(output) = &self.output {
            output.shared.position_ms.store(0, Ordering::Relaxed);
            output.send(Command::Load(Box::new(player)));
        }
        Ok(())
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
