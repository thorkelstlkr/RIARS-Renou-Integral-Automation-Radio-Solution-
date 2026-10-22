use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::crossfade::CrossfadeConfig;
use super::mixer::Mixer;
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

pub struct AudioEngine {
    state: PlaybackState,
    volume: f32,
    mixer: Mixer,
    current_player: Option<TrackPlayer>,
    crossfade: CrossfadeConfig,
    sample_rate: u32,
}

impl AudioEngine {
    pub fn new() -> Result<Self, String> {
        let sample_rate = 44100;
        Ok(Self {
            state: PlaybackState::Stopped,
            volume: 0.8,
            mixer: Mixer::new(sample_rate),
            current_player: None,
            crossfade: CrossfadeConfig::default(),
            sample_rate,
        })
    }

    pub fn play(&mut self) {
        if self.current_player.is_some() {
            self.state = PlaybackState::Playing;
            log::info!("Playback started");
        }
    }

    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
            log::info!("Playback paused");
        }
    }

    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        if let Some(player) = &mut self.current_player {
            player.seek(Duration::ZERO);
        }
        log::info!("Playback stopped");
    }

    pub fn next_track(&mut self) {
        log::info!("Skipping to next track");
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.mixer.set_master_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn load_track(&mut self, path: &str) -> Result<(), String> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(format!("File not found: {}", path.display()));
        }
        let player = TrackPlayer::from_file(path, self.sample_rate)?;
        log::info!("Loaded track: {}", path.display());
        self.current_player = Some(player);
        Ok(())
    }

    pub fn state_info(&self) -> EngineStateInfo {
        let (current_track, position_ms, duration_ms) = match &self.current_player {
            Some(player) => (
                Some(player.file_name().to_string()),
                player.position().as_millis() as u64,
                player.duration().as_millis() as u64,
            ),
            None => (None, 0, 0),
        };

        EngineStateInfo {
            state: self.state,
            volume: self.volume,
            current_track,
            position_ms,
            duration_ms,
        }
    }

    pub fn playback_state(&self) -> PlaybackState {
        self.state
    }

    pub fn crossfade_config(&self) -> &CrossfadeConfig {
        &self.crossfade
    }

    pub fn set_crossfade(&mut self, config: CrossfadeConfig) {
        self.crossfade = config;
    }
}
