//! Tauri commands for cue points, analysis, loudness normalisation, sound cards and
//! pre-listening. They mirror the `analysis`, `audio` and `preview` objects in
//! `src/analysis/api.ts`.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use riars_core::analysis::{
    analyze_track_detailed, AnalysisStats, BatchScope, BatchStatus, CueAnalysis, CueDetector,
    CueSource, LibraryAccess, TrackCues,
};
use riars_core::audio::normalize::NormalizationConfig;
use riars_core::audio::preview::{PreviewPlayer, PreviewState};
use riars_core::audio::{default_output_device, output_devices};
use riars_core::MediaLibrary;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// `automation_kv` key holding the cue detector as JSON.
const DETECTOR_KEY: &str = "cue_detector";

/// Sound cards chosen in the settings, next to the library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub main_device: Option<String>,
    pub preview_device: Option<String>,
}

impl AudioSettings {
    /// The saved settings, or the defaults when the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write through a temporary file and rename it into place.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

/// Sound cards of this machine and the ones chosen in the settings.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevices {
    devices: Vec<String>,
    default: Option<String>,
    main: Option<String>,
    main_in_use: Option<String>,
    preview: Option<String>,
}

fn audio_devices_from(state: &AppState) -> AudioDevices {
    let settings = AudioSettings::load(&state.audio_settings_path);
    let main_in_use = state
        .engine
        .lock()
        .ok()
        .and_then(|engine| engine.output_device().map(str::to_string));
    AudioDevices {
        devices: output_devices(),
        default: default_output_device(),
        main: settings.main_device,
        main_in_use,
        preview: settings.preview_device,
    }
}

/// The stored detector, Builtin when missing or unreadable.
fn detector_from_library(library: &MediaLibrary) -> Result<CueDetector, String> {
    let stored = library.kv_get(DETECTOR_KEY)?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

fn validate_detector(detector: &CueDetector) -> Result<(), String> {
    if let CueDetector::Command { program, args } = detector {
        if program.trim().is_empty() {
            return Err("The detector program is empty".to_string());
        }
        if args.len() > 20 {
            return Err("A detector command has at most 20 arguments".to_string());
        }
    }
    Ok(())
}

/// The library as the background analyser reaches it.
pub(crate) struct TauriLibrary(pub(crate) AppHandle);

impl LibraryAccess for TauriLibrary {
    fn with_library(
        &self,
        f: &mut dyn FnMut(&mut MediaLibrary) -> Result<(), String>,
    ) -> Result<(), String> {
        let state = self.0.state::<AppState>();
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        f(&mut library)
    }
}

// --- Cues -------------------------------------------------------------------------

#[tauri::command]
pub fn analysis_track_cues(
    track_id: i64,
    state: State<AppState>,
) -> Result<Option<TrackCues>, String> {
    state.library.lock().map_err(|e| e.to_string())?.track_cues(track_id)
}

#[tauri::command]
pub fn analysis_tracks_cues(
    track_ids: Vec<i64>,
    state: State<AppState>,
) -> Result<Vec<TrackCues>, String> {
    if track_ids.len() > 1000 {
        return Err("At most 1000 track ids".to_string());
    }
    let library = state.library.lock().map_err(|e| e.to_string())?;
    let mut cues = Vec::with_capacity(track_ids.len());
    for id in track_ids {
        if let Some(found) = library.track_cues(id)? {
            cues.push(found);
        }
    }
    Ok(cues)
}

/// Cues of a playlist item, found by its file path.
#[derive(Debug, Clone, Serialize)]
pub struct PathCues {
    path: String,
    cues: TrackCues,
}

/// Cues of the tracks at these paths (unknown or never analysed ones are left out).
#[tauri::command]
pub async fn analysis_paths_cues(app: AppHandle, paths: Vec<String>) -> Result<Vec<PathCues>, String> {
    if paths.len() > 1000 {
        return Err("At most 1000 paths".to_string());
    }
    blocking(app, move |state| {
        let library = state.library.lock().map_err(|e| e.to_string())?;
        let mut found = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(cues) = library.track_cues_by_path(&path)? {
                found.push(PathCues { path, cues });
            }
        }
        Ok(found)
    })
    .await
}

#[tauri::command]
pub fn analysis_set_track_cues(
    mut cues: TrackCues,
    state: State<AppState>,
) -> Result<TrackCues, String> {
    cues.source = CueSource::Manual;
    cues.confidence = 1.0;
    let mut library = state.library.lock().map_err(|e| e.to_string())?;
    library.set_track_cues(&cues)?;
    library
        .track_cues(cues.track_id)?
        .ok_or_else(|| format!("No track {}", cues.track_id))
}

#[tauri::command]
pub fn analysis_clear_track_cues(track_id: i64, state: State<AppState>) -> Result<(), String> {
    state.library.lock().map_err(|e| e.to_string())?.clear_track_cues(track_id)?;
    Ok(())
}

// --- Analysis ---------------------------------------------------------------------

#[tauri::command]
pub async fn analysis_analyze_track(
    app: AppHandle,
    track_id: i64,
    save: bool,
) -> Result<CueAnalysis, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (path, detector) = {
            let state = app.state::<AppState>();
            let library = state.library.lock().map_err(|e| e.to_string())?;
            let track = library
                .get_track(track_id)?
                .ok_or_else(|| format!("No track {track_id}"))?;
            let detector = detector_from_library(&library)?;
            (track.file_path, detector)
        };
        let (analysis, cues) =
            analyze_track_detailed(track_id, Path::new(&path), 1200, &detector, &AtomicBool::new(false))?;
        if save {
            let state = app.state::<AppState>();
            let mut library = state.library.lock().map_err(|e| e.to_string())?;
            library.save_analysis(&cues)?;
        }
        Ok(analysis)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Run `f` on a worker thread: Tauri runs plain commands on the UI thread, and listing
/// sound cards or a big library would freeze the window.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn analysis_batch_start(
    app: AppHandle,
    scope: BatchScope,
    reanalyze: bool,
) -> Result<(), String> {
    let library = Arc::new(TauriLibrary(app.clone()));
    blocking(app, move |state| {
        let detector = {
            let library = state.library.lock().map_err(|e| e.to_string())?;
            detector_from_library(&library)?
        };
        state.analysis.start(library, scope, reanalyze, detector)
    })
    .await
}

#[tauri::command]
pub fn analysis_batch_cancel(state: State<AppState>) -> Result<(), String> {
    state.analysis.cancel();
    Ok(())
}

#[tauri::command]
pub fn analysis_batch_status(state: State<AppState>) -> Result<BatchStatus, String> {
    Ok(state.analysis.status())
}

#[tauri::command]
pub async fn analysis_stats(app: AppHandle) -> Result<AnalysisStats, String> {
    blocking(app, |state| {
        let library = state.library.lock().map_err(|e| e.to_string())?;
        let min_confidence = library.automation_config()?.talk_over.min_confidence;
        library.analysis_stats(min_confidence)
    })
    .await
}

#[tauri::command]
pub fn analysis_get_detector(state: State<AppState>) -> Result<CueDetector, String> {
    let library = state.library.lock().map_err(|e| e.to_string())?;
    detector_from_library(&library)
}

#[tauri::command]
pub fn analysis_set_detector(
    detector: CueDetector,
    state: State<AppState>,
) -> Result<CueDetector, String> {
    validate_detector(&detector)?;
    let json = serde_json::to_string(&detector).map_err(|e| e.to_string())?;
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .kv_set(DETECTOR_KEY, &json)?;
    Ok(detector)
}

// --- Sound cards ------------------------------------------------------------------

#[tauri::command]
pub async fn audio_devices(app: AppHandle) -> Result<AudioDevices, String> {
    blocking(app, |state| Ok(audio_devices_from(state))).await
}

#[tauri::command]
pub async fn audio_set_main_device(app: AppHandle, name: Option<String>) -> Result<AudioDevices, String> {
    blocking(app, move |state| set_main_device(name, state)).await
}

fn set_main_device(name: Option<String>, state: &AppState) -> Result<AudioDevices, String> {
    let mut settings = AudioSettings::load(&state.audio_settings_path);
    settings.main_device = name.filter(|name| !name.trim().is_empty());
    settings.save(&state.audio_settings_path)?;
    Ok(audio_devices_from(state))
}

#[tauri::command]
pub async fn audio_set_preview_device(app: AppHandle, name: Option<String>) -> Result<AudioDevices, String> {
    blocking(app, move |state| set_preview_device(name, state)).await
}

fn set_preview_device(name: Option<String>, state: &AppState) -> Result<AudioDevices, String> {
    let name = name.filter(|name| !name.trim().is_empty());
    let mut settings = AudioSettings::load(&state.audio_settings_path);
    let previous = settings.preview_device.clone();
    {
        let mut preview = state.preview.lock().map_err(|e| e.to_string())?;
        if let Err(e) = preview.set_device(name.clone()) {
            // Never keep a missing card: reopen the previous one, if it still exists.
            let _ = preview.set_device(previous);
            return Err(e);
        }
    }
    settings.preview_device = name;
    settings.save(&state.audio_settings_path)?;
    Ok(audio_devices_from(state))
}

// --- Normalisation ----------------------------------------------------------------

#[tauri::command]
pub fn normalization_get(state: State<AppState>) -> Result<NormalizationConfig, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.normalization_config())
}

#[tauri::command]
pub fn normalization_set(
    config: NormalizationConfig,
    state: State<AppState>,
) -> Result<NormalizationConfig, String> {
    config.validate()?;
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.set_normalization(config);
    Ok(engine.normalization_config())
}

// --- Pre-listening ----------------------------------------------------------------

#[tauri::command]
pub async fn preview_play(
    app: AppHandle,
    track_id: i64,
    from_ms: u64,
) -> Result<PreviewState, String> {
    blocking(app, move |state| {
        let path = state
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .get_track(track_id)?
            .ok_or_else(|| format!("No track {track_id}"))?
            .file_path;
        play_path(state, &path, from_ms)
    })
    .await
}

/// Pre-listen a file by path, for playlist items that are not in the library.
#[tauri::command]
pub async fn preview_play_path(app: AppHandle, path: String, from_ms: u64) -> Result<PreviewState, String> {
    blocking(app, move |state| play_path(state, &path, from_ms)).await
}

/// Pre-listen `path` at the gain it would get on air.
fn play_path(state: &AppState, path: &str, from_ms: u64) -> Result<PreviewState, String> {
    let config = state.engine.lock().map_err(|e| e.to_string())?.normalization_config();
    let (loudness, peak) = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .loudness_by_path(path)?
        .map_or((None, None), |(loudness, peak)| (Some(loudness), peak));
    let gain = config.gain_db(loudness, peak);
    if state.preview.lock().map_err(|e| e.to_string())?.device().is_none() {
        return Err("No pre-listen sound card chosen".to_string());
    }
    // Decode with no lock held, so polling the state and Stop stay responsive.
    let player = PreviewPlayer::load(Path::new(path), gain)?;
    let mut preview = state.preview.lock().map_err(|e| e.to_string())?;
    preview.play_loaded(player, Path::new(path), from_ms)?;
    Ok(preview.state())
}

#[tauri::command]
pub fn preview_stop(state: State<AppState>) -> Result<PreviewState, String> {
    let mut preview = state.preview.lock().map_err(|e| e.to_string())?;
    preview.stop();
    Ok(preview.state())
}

#[tauri::command]
pub fn preview_state(state: State<AppState>) -> Result<PreviewState, String> {
    Ok(state.preview.lock().map_err(|e| e.to_string())?.state())
}
