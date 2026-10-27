use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::audio::engine::{play_index_unlocked, AudioEngine};
use serde::{Deserialize, Serialize};
use crate::audio::crossfade::CrossfadeConfig;
use crate::playlist::playlist::Playlist;
use crate::streaming::icecast::IcecastConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub playlist: Option<Playlist>,
    pub position_ms: u64,
    pub playing: bool,
    pub volume: Option<f32>,
    pub crossfade: Option<CrossfadeConfig>,
    pub stream: Option<IcecastConfig>,
    pub dead_air: Option<crate::audio::engine::DeadAirConfig>,
    pub normalization: Option<crate::audio::normalize::NormalizationConfig>,
    #[serde(default)]
    pub play_mode: Option<crate::audio::engine::PlayMode>,
    #[serde(default)]
    pub mic: Option<crate::audio::live::MicConfig>,
}

impl SessionState {
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(json) => {
                let scheduler: Self = serde_json::from_str(&json)
                    .map_err(|e| format!("Invalid session file {}: {e}", path.display()))?;
                Ok(Some(scheduler))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    }
}

/// Restore `state` into the engine and, if it was playing, carry on from the same
/// item and position. Blocking (decodes a track): call it from a worker thread.
pub fn resume(engine: &Mutex<AudioEngine>, state: &SessionState) -> Result<(), String> {
    let lock = || engine.lock().map_err(|e| e.to_string());
    let index = {
        let mut engine = lock()?;
        engine.restore(state);
        engine.playlist().current_index
    };
    let has_items = lock()?.playlist().len() > 0;
    if !state.playing || !has_items {
        return Ok(());
    }
    play_index_unlocked(engine, index, true)?;
    if state.position_ms > 0 {
        lock()?.seek(Duration::from_millis(state.position_ms))?;
    }
    log::info!("Resumed playback where it stopped ({} ms in)", state.position_ms);
    Ok(())
}

/// Load the session file, if any, and resume it; returns the stream config to
/// restart (the caller owns streaming). Blocking.
pub fn resume_from_file(engine: &Mutex<AudioEngine>, path: &Path) -> Option<IcecastConfig> {
    match SessionState::load(path) {
        Ok(Some(state)) => {
            if let Err(e) = resume(engine, &state) {
                log::warn!("Cannot resume the last session: {e}");
            }
            state.stream
        }
        Ok(None) => None,
        Err(e) => {
            log::warn!("{e}; starting fresh");
            None
        }
    }
}

/// How often the position is saved while playing. Other changes are saved at once.
/// Kept low-frequency to spare SD cards on small machines.
const POSITION_SAVE_SECS: u64 = 30;

/// Writes the session file only when something worth keeping changed.
pub struct SessionSaver {
    path: PathBuf,
    last_key: Option<(u64, u32, u64, bool, bool, bool)>,
    last_save: Instant,
}

impl SessionSaver {
    pub fn new(path: PathBuf) -> Self {
        Self { path, last_key: None, last_save: Instant::now() }
    }

    /// Call periodically with the engine locked: returns the state to write if
    /// something changed. `stream` is the active stream config (with its password), so
    /// a restart can resume streaming too. Write it with `write`, after unlocking.
    pub fn due(&mut self, engine: &AudioEngine, stream: Option<IcecastConfig>) -> Option<SessionState> {
        let (revision, volume, fade_ms, fade_on, playing) = engine.session_key();
        let key = (revision, volume, fade_ms, fade_on, playing, stream.is_some());
        let position_due = playing && self.last_save.elapsed() >= Duration::from_secs(POSITION_SAVE_SECS);
        if self.last_key == Some(key) && !position_due {
            return None;
        }
        self.last_key = Some(key);
        self.last_save = Instant::now();
        let mut state = engine.snapshot();
        state.stream = stream;
        Some(state)
    }

    /// Write a state returned by `due` (file I/O: keep the engine unlocked).
    pub fn write(&self, state: &SessionState) {
        if let Err(e) = state.save(&self.path) {
            log::warn!("{e}");
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::process::id;

    #[test]
    fn save_then_load() {
        let path = temp_dir().join(format!("session_{}.json", id()));
        let state = SessionState {
            position_ms: 12345,
            playing: true,
            volume: Some(0.5),
            ..Default::default()
        };
        state.save(&path).unwrap();
        let loaded_state = SessionState::load(&path).unwrap().unwrap();
        assert_eq!(loaded_state.position_ms, 12345);
        assert_eq!(loaded_state.playing, true);
        assert_eq!(loaded_state.volume, Some(0.5));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn load_missing_file() {
        let path = temp_dir().join(format!("missing_session_{}.json", id()));
        let loaded_state = SessionState::load(&path).unwrap();
        assert!(loaded_state.is_none());
    }

    #[test]
    fn load_empty_file() {
        let path = temp_dir().join(format!("empty_session_{}.json", id()));
        std::fs::write(&path, "{}").unwrap();
        let loaded_state = SessionState::load(&path).unwrap().unwrap();
        assert!(loaded_state.playlist.is_none() && loaded_state.stream.is_none());
        assert_eq!((loaded_state.position_ms, loaded_state.playing, loaded_state.volume), (0, false, None));
        std::fs::remove_file(&path).unwrap();
    }
}
