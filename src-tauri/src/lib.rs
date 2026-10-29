use std::sync::{Arc, Mutex};

use riars_core::{AudioEngine, EngineStateInfo, MediaLibrary, Playlist, Track};
use std::path::PathBuf;

use riars_core::automation::StreamCommand;
use riars_core::playlist::scheduler::{EventAction, EventTrigger, ScheduledEvent};
use riars_core::{DeadAirConfig, IcecastConfig, SavedPlaylist, Scheduler};
use riars_stream::{StreamService, StreamState, StreamStatus};
use tauri::{AppHandle, Emitter, Manager, State};

mod analysis_cmds;
mod automation_cmds;
mod folder_cmds;
mod live_cmds;
mod metadata_cmds;

#[tauri::command]
fn set_locale(lang: String) -> Result<(), String> {
    riars_core::set_language(&lang);
    Ok(())
}

#[tauri::command]
fn get_supported_languages() -> Vec<&'static str> {
    riars_core::SUPPORTED_LANGUAGES.to_vec()
}

pub struct AppState {
    pub engine: Mutex<AudioEngine>,
    pub library: Mutex<MediaLibrary>,
    /// Icecast streaming; `None` without a sound card.
    pub stream: Option<StreamService>,
    /// Last title sent to the stream, to send it only when the track changes.
    pub stream_title: Mutex<String>,
    pub scheduler: Mutex<Scheduler>,
    /// Where the schedule is saved after every change.
    pub schedule_path: PathBuf,
    /// Saves playlist, position, volume, crossfade and streaming for restarts.
    pub session: Mutex<riars_core::session::SessionSaver>,
    /// Long-term automation: playlist generator and proof-of-play log.
    pub automation: Mutex<riars_core::automation::Automation>,
    /// Background cue-point and loudness analysis.
    pub analysis: riars_core::analysis::BatchAnalyzer,
    /// Pre-listening on its own sound card.
    pub preview: Mutex<riars_core::audio::preview::PreviewPlayer>,
    /// Sound cards chosen in the settings (`riars_audio.json`).
    pub audio_settings_path: PathBuf,
}

#[tauri::command]
fn schedule_get(state: State<AppState>) -> Result<Vec<ScheduledEvent>, String> {
    let scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
    Ok(scheduler.events().to_vec())
}

#[tauri::command]
fn schedule_add(name: String, trigger: EventTrigger, action: EventAction, state: State<AppState>) -> Result<Vec<ScheduledEvent>, String> {
    if name.trim().is_empty() {
        return Err("Name is required".to_string());
    }
    let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
    scheduler.add_event(ScheduledEvent::new(name.trim().to_string(), trigger, action));
    scheduler.save(&state.schedule_path)?;
    Ok(scheduler.events().to_vec())
}

#[tauri::command]
fn schedule_remove(id: String, state: State<AppState>) -> Result<Vec<ScheduledEvent>, String> {
    let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
    if !scheduler.remove_event(&id) {
        return Err(format!("No scheduled event {}", id));
    }
    scheduler.save(&state.schedule_path)?;
    Ok(scheduler.events().to_vec())
}

#[tauri::command]
fn schedule_toggle(id: String, state: State<AppState>) -> Result<Vec<ScheduledEvent>, String> {
    let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
    if let Some(toggled) = scheduler.toggle_event(&id) {
        if !toggled {
            return Err(format!("No scheduled event {}", id));
        }
    }
    scheduler.save(&state.schedule_path)?;
    Ok(scheduler.events().to_vec())
}

#[derive(serde::Serialize)]
struct StreamInfo {
    available: bool,
    status: Option<StreamStatus>,
    config: Option<IcecastConfig>,
}

fn stream_info(state: &AppState) -> StreamInfo {
    StreamInfo {
        available: state.stream.is_some(),
        status: state.stream.as_ref().map(|s| s.status()),
        config: state.stream.as_ref().and_then(|s| s.config()),
    }
}

#[tauri::command]
fn stream_get(state: State<AppState>) -> StreamInfo {
    stream_info(&state)
}

#[tauri::command]
fn stream_start(config: IcecastConfig, state: State<AppState>) -> Result<StreamInfo, String> {
    let stream = state.stream.as_ref().ok_or("Streaming needs an audio output device")?;
    stream.start(config)?;
    // Send the current title again on the new connection.
    if let Ok(mut title) = state.stream_title.lock() {
        title.clear();
    }
    Ok(stream_info(&state))
}

#[tauri::command]
fn stream_stop(state: State<AppState>) -> StreamInfo {
    if let Some(stream) = &state.stream {
        stream.stop();
    }
    stream_info(&state)
}

#[tauri::command]
async fn engine_play(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        riars_core::play_unlocked(&app.state::<AppState>().engine)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn engine_pause(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.pause();
    Ok(())
}

#[tauri::command]
fn engine_stop(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.stop();
    Ok(())
}

#[tauri::command]
async fn engine_next(app: AppHandle) -> Result<(), String> {
    let next = app.state::<AppState>().engine.lock().map_err(|e| e.to_string())?.next_index();
    match next {
        Some(index) => decode_and_start(app, index, false).await,
        None => {
            app.state::<AppState>().engine.lock().map_err(|e| e.to_string())?.stop();
            Ok(())
        }
    }
}

/// Runs `riars_core::play_index_unlocked` off the async runtime.
async fn decode_and_start(app: AppHandle, index: usize, skip_failures: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        riars_core::play_index_unlocked(&app.state::<AppState>().engine, index, skip_failures)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn engine_set_volume(volume: f32, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .set_volume(volume.clamp(0.0, 1.0));
    Ok(())
}

#[derive(serde::Serialize)]
struct PlaylistChange {
    revision: u64,
    playlist: Playlist,
}

/// The playlist if it changed since `since`, else null: the panel polls this cheaply.
#[tauri::command]
fn playlist_changes(since: Option<u64>, state: State<AppState>) -> Result<Option<PlaylistChange>, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    let revision = engine.playlist_revision();
    if since == Some(revision) {
        return Ok(None);
    }
    Ok(Some(PlaylistChange { revision, playlist: engine.playlist().clone() }))
}

#[tauri::command]
fn dead_air_get(state: State<AppState>) -> Result<DeadAirConfig, String> {
    let config = state.engine.lock().map_err(|e| e.to_string())?.dead_air_config().clone();
    Ok(config)
}

#[tauri::command]
fn dead_air_set(config: DeadAirConfig, state: State<AppState>) -> Result<DeadAirConfig, String> {
    config.validate()?;
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.set_dead_air(config);
    Ok(engine.dead_air_config().clone())
}

/// Crossfade as the UI edits it (milliseconds instead of a Duration).
#[derive(serde::Serialize, serde::Deserialize)]
struct CrossfadeSettings {
    enabled: bool,
    duration_ms: u64,
    curve: riars_core::audio::crossfade::FadeCurve,
}

#[tauri::command]
fn crossfade_get(state: State<AppState>) -> Result<CrossfadeSettings, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    let c = engine.crossfade_config();
    Ok(CrossfadeSettings { enabled: c.enabled, duration_ms: c.duration.as_millis() as u64, curve: c.curve })
}

#[tauri::command]
fn crossfade_set(config: CrossfadeSettings, state: State<AppState>) -> Result<CrossfadeSettings, String> {
    if config.duration_ms > 30_000 {
        return Err("duration_ms must be at most 30000".to_string());
    }
    state.engine.lock().map_err(|e| e.to_string())?.set_crossfade(riars_core::audio::crossfade::CrossfadeConfig {
        enabled: config.enabled,
        duration: std::time::Duration::from_millis(config.duration_ms),
        curve: config.curve,
    });
    crossfade_get(state)
}

#[tauri::command]
fn engine_seek(position_ms: u64, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .seek(std::time::Duration::from_millis(position_ms))
}

/// Output peaks since the last call; the VU meter polls it.
#[tauri::command]
fn engine_levels(state: State<AppState>) -> Result<riars_core::audio::Levels, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.take_levels())
}

#[tauri::command]
fn engine_get_state(state: State<AppState>) -> Result<EngineStateInfo, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.state_info())
}

/// Auto-advance, stream title, session saving and UI events, on a backend thread:
/// playback must go on while the window is minimised, and the UI is told about
/// changes instead of polling for them.
fn run_engine_loop(app: AppHandle) {
    let mut last_state: Option<EngineStateInfo> = None;
    let mut last_revision = u64::MAX;
    let mut last_stream: Option<String> = None;
    let mut ticks: u64 = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        ticks += 1;
        let state = app.state::<AppState>();
        let config = state.stream.as_ref().and_then(|s| s.config_for_session());
        let due = match (state.session.lock(), state.engine.lock()) {
            (Ok(mut saver), Ok(engine)) => saver.due(&engine, config),
            _ => None,
        };
        if let (Some(session), Ok(saver)) = (due, state.session.lock()) {
            saver.write(&session); // engine unlocked: slow disks do not stall it
        }
        let (info, revision) = {
            let Ok(mut engine) = state.engine.lock() else { continue };
            if let Some(index) = engine.poll() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = decode_and_start(app, index, true).await {
                        log::error!("Auto-advance failed: {e}");
                    }
                });
            }
            if let (Some(stream), Some(title)) = (&state.stream, engine.now_playing_title()) {
                if let Ok(mut last) = state.stream_title.lock() {
                    if *last != title {
                        stream.set_title(&title);
                        *last = title;
                    }
                }
            }
            (engine.state_info(), engine.playlist_revision())
        };

        // Automation: log what aired, top up the playlist (engine unlocked first).
        let wants = state.automation.lock().is_ok_and(|a| {
            a.wants_tick(revision, state.engine.lock().is_ok_and(|e| e.has_play_records()))
        });
        if wants {
            let now = chrono::Local::now().naive_local();
            match riars_core::automation::tick(&state.engine, &state.library, &state.automation, now) {
                Ok(report) => {
                    if report.generated > 0 {
                        log::info!("Automation: {} items added", report.generated);
                    }
                    if report.generated > 0
                        || report.logged > 0
                        || report.cut_to.is_some()
                        || !report.warnings.is_empty()
                    {
                        let _ = app.emit(
                            "automation",
                            serde_json::json!({
                                "type": "automation",
                                "generated": report.generated,
                                "logged": report.logged,
                                "cut_to": report.cut_to,
                                "warnings": report.warnings,
                            }),
                        );
                    }
                }
                Err(e) => log::warn!("Automation: {e}"),
            }
        }

        // Engine state: on any change, which while playing is the position every tick.
        if last_state.as_ref() != Some(&info) {
            let _ = app.emit("engine-state", &info);
            last_state = Some(info);
        }
        if revision != last_revision {
            let _ = app.emit("playlist-revision", revision);
            last_revision = revision;
        }
        // Stream: on state/title/error changes, and the byte count once a second.
        let stream = stream_info(&state);
        let key = stream.status.as_ref().map(|s| format!("{:?}|{:?}|{:?}", s.state, s.title, s.last_error));
        let connected = stream.status.as_ref().is_some_and(|s| s.state == StreamState::Connected);
        if key != last_stream || (connected && ticks % 4 == 0) {
            let _ = app.emit("stream-status", &stream);
            last_stream = key;
        }
    }
}

/// VU meter levels at 20 Hz, only while there is something to show and the window can
/// be seen: a stopped or minimised station costs nothing here.
fn run_meter_loop(app: AppHandle) {
    let mut silent = true;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let visible = app
            .get_webview_window("main")
            .map(|w| w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false))
            .unwrap_or(false);
        let state = app.state::<AppState>();
        let Ok(engine) = state.engine.lock() else { continue };
        let levels = engine.take_levels();
        drop(engine);
        let quiet = levels.peak_left == 0.0 && levels.peak_right == 0.0 && levels.clipped == 0;
        // One last zero reading lets the meter fall; then nothing until sound returns.
        if visible && !(quiet && silent) {
            let _ = app.emit("levels", &levels);
        }
        silent = quiet;
    }
}

#[tauri::command]
fn engine_load_track(path: String, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .load_track(&path)
}

/// Add `path` as a library folder and scan it now; returns the new tracks.
/// The lock is held only to store them, so playback and the UI carry on meanwhile.
#[tauri::command]
async fn library_scan_folder(path: String, app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.library.lock().map_err(|e| e.to_string())?.add_folder(&path)?;
        folder_cmds::scan_now(&app, &folder)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One page of the library: the UI loads pages as it scrolls instead of everything.
#[tauri::command]
fn library_query(search: String, limit: usize, offset: usize, state: State<AppState>) -> Result<riars_core::TrackPage, String> {
    state.library.lock().map_err(|e| e.to_string())?.query(&search, limit, offset)
}

#[tauri::command]
fn library_get_tracks(state: State<AppState>) -> Result<Vec<Track>, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .get_all_tracks()
}

#[tauri::command]
fn library_search(query: String, state: State<AppState>) -> Result<Vec<Track>, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .search(&query)
}

#[tauri::command]
fn playlist_create(name: String) -> Result<Playlist, String> {
    Ok(Playlist::new(name))
}

#[tauri::command]
fn playlist_get(state: State<AppState>) -> Result<Playlist, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.playlist().clone())
}

#[tauri::command]
fn playlist_add(
    path: String,
    title: String,
    artist: String,
    duration_ms: u64,
    state: State<AppState>,
) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().add_item(path, title, artist, duration_ms);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_remove(index: usize, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().remove_item(index);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_move(from: usize, to: usize, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().move_item(from, to);
    Ok(engine.playlist().clone())
}

/// A library track to put in the playlist.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewItem {
    path: String,
    title: String,
    artist: String,
    duration_ms: u64,
}

#[derive(serde::Deserialize)]
enum InsertPosition {
    End,
    /// Right after the item on air.
    Next,
}

#[tauri::command]
fn playlist_insert(tracks: Vec<NewItem>, position: InsertPosition, state: State<AppState>) -> Result<Playlist, String> {
    if tracks.len() > 10_000 {
        return Err("At most 10000 tracks at once".to_string());
    }
    let items = tracks
        .into_iter()
        .map(|t| riars_core::playlist::playlist::PlaylistItem::new(t.path, t.title, t.artist, t.duration_ms))
        .collect();
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    let playlist = engine.playlist_mut();
    let at = match position {
        InsertPosition::End => playlist.len(),
        InsertPosition::Next => playlist.next_position(),
    };
    playlist.insert_items(at, items);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_remove_many(ids: Vec<String>, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().remove_ids(&ids);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_remove_played(state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().prune_played(0);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_play_next(id: String, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    let playlist = engine.playlist_mut();
    if let Some(index) = playlist.index_of(&id) {
        playlist.move_to_next(index);
    }
    Ok(engine.playlist().clone())
}

/// Remove tracks from the library (the files stay on disk). Returns how many went.
#[tauri::command]
fn library_remove_tracks(ids: Vec<i64>, state: State<AppState>) -> Result<usize, String> {
    let mut library = state.library.lock().map_err(|e| e.to_string())?;
    let mut removed = 0;
    for id in ids {
        if library.delete_track(id)? {
            removed += 1;
        }
    }
    Ok(removed)
}

#[tauri::command]
fn library_track_by_path(path: String, state: State<AppState>) -> Result<Option<Track>, String> {
    state.library.lock().map_err(|e| e.to_string())?.track_by_path(&path)
}

/// Show `path` selected in the system file manager.
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    let file = std::path::Path::new(&path);
    if !file.is_file() {
        return Err(format!("The file is not there: {path}"));
    }
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(format!("/select,{}", file.display())).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg("-R").arg(file).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(file.parent().unwrap_or(file)).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn playlist_clear(state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().clear();
    Ok(engine.playlist().clone())
}

#[tauri::command]
async fn playlist_play(index: usize, app: AppHandle) -> Result<(), String> {
    decode_and_start(app, index, false).await
}

#[tauri::command]
fn playlists_list(state: State<AppState>) -> Result<Vec<SavedPlaylist>, String> {
    let playlists = state.library.lock().map_err(|e| e.to_string())?.list_playlists()?;
    Ok(playlists)
}

#[tauri::command]
fn playlists_save(name: String, state: State<AppState>) -> Result<Vec<SavedPlaylist>, String> {
    let playlist = state.engine.lock().map_err(|e| e.to_string())?.playlist().clone();
    state.library.lock().map_err(|e| e.to_string())?.save_playlist(&name, &playlist)?;
    playlists_list(state)
}

#[tauri::command]
fn playlists_delete(id: String, state: State<AppState>) -> Result<Vec<SavedPlaylist>, String> {
    state.library.lock().map_err(|e| e.to_string())?.delete_playlist(&id)?;
    playlists_list(state)
}

#[tauri::command]
async fn playlists_load(id: String, play: bool, app: AppHandle) -> Result<Playlist, String> {
    let state = app.state::<AppState>();
    let playlist = state.library.lock().map_err(|e| e.to_string())?.load_playlist(&id)?;
    state.engine.lock().map_err(|e| e.to_string())?.set_playlist(playlist.clone());
    if play {
        decode_and_start(app.clone(), 0, false).await?;
    }
    Ok(playlist)
}

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Keep the library in the per-user app data folder, not the working directory.
            // RIARS_DATA_DIR points at another data folder (portable installs, a
            // second station on the same machine, tests).
            let data_dir = match std::env::var_os("RIARS_DATA_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("riars_library.db");
            let library = MediaLibrary::open(&db_path.to_string_lossy())?;
            let schedule_path = data_dir.join("riars_schedule.json");
            let session_path = data_dir.join("riars_session.json");
            let audio_settings_path = data_dir.join("riars_audio.json");
            let scheduler = Scheduler::load(&schedule_path).unwrap_or_else(|e| {
                log::warn!("{e}; starting with an empty schedule");
                Scheduler::new()
            });
            log::info!("Media library: {}", db_path.display());

            // Sound cards chosen last time: the on-air one before the engine opens.
            let audio_settings = analysis_cmds::AudioSettings::load(&audio_settings_path);
            let mut engine = match &audio_settings.main_device {
                Some(name) => AudioEngine::with_device(name),
                None => AudioEngine::new(),
            }
            .expect("Failed to initialize audio engine");
            let stream = engine.take_stream_tap().map(StreamService::new);

            app.manage(AppState {
                stream,
                stream_title: Mutex::new(String::new()),
                scheduler: Mutex::new(scheduler),
                schedule_path,
                session: Mutex::new(riars_core::session::SessionSaver::new(session_path.clone())),
                engine: Mutex::new(engine),
                library: Mutex::new(library),
                automation: Mutex::new(riars_core::automation::Automation::new()),
                analysis: riars_core::analysis::BatchAnalyzer::new(),
                preview: Mutex::new(riars_core::audio::preview::PreviewPlayer::new()),
                audio_settings_path,
            });

            // Normalisation reads each file's loudness from the library; the closure
            // never runs with the engine locked (see `play_index_unlocked`).
            let lookup_app = app.handle().clone();
            {
                let state = app.state::<AppState>();
                match state.engine.lock() {
                    Ok(mut engine) => engine.set_loudness_lookup(Arc::new(move |path| {
                        lookup_app
                            .state::<AppState>()
                            .library
                            .lock()
                            .ok()?
                            .loudness_by_path(path)
                            .ok()
                            .flatten()
                    })),
                    Err(e) => log::warn!("Cannot set the loudness lookup: {e}"),
                };
            }
            if let Some(name) = audio_settings.preview_device {
                let opened = app
                    .state::<AppState>()
                    .preview
                    .lock()
                    .map_err(|e| e.to_string())
                    .and_then(|mut preview| preview.set_device(Some(name)));
                if let Err(e) = opened {
                    log::warn!("Cannot open the pre-listen sound card: {e}");
                }
            }
            // Pick up files added to the library folders since the last run.
            folder_cmds::spawn_rescan(app.handle().clone());
            // Carry on where the last run stopped (decodes a track: off the main thread).
            let resume_app = app.handle().clone();
            std::thread::Builder::new().name("riars-resume".into()).spawn(move || {
                let state = resume_app.state::<AppState>();
                let config = riars_core::session::resume_from_file(&state.engine, &session_path);
                if let (Some(config), Some(stream)) = (config, &state.stream) {
                    if let Err(e) = stream.start(config) {
                        log::warn!("Cannot resume streaming: {e}");
                    }
                }
            })?;
            let engine_app = app.handle().clone();
            std::thread::Builder::new()
                .name("riars-engine-loop".into())
                .spawn(move || run_engine_loop(engine_app))?;
            let meter_app = app.handle().clone();
            std::thread::Builder::new()
                .name("riars-meter-loop".into())
                .spawn(move || run_meter_loop(meter_app))?;
            // Run the schedule on its own thread (an action may decode a track).
            let handle = app.handle().clone();
            std::thread::Builder::new().name("riars-scheduler".into()).spawn(move || {
                let state = handle.state::<AppState>();
                let mut stream = |command| match (&state.stream, command) {
                    (Some(s), StreamCommand::Start) => s.resume(),
                    (Some(s), StreamCommand::Stop) => {
                        s.stop();
                        Ok(())
                    }
                    (None, _) => Err("Streaming needs an audio output".to_string()),
                };
                riars_core::automation::scheduler_loop(
                    &state.engine,
                    &state.library,
                    &state.scheduler,
                    &state.schedule_path,
                    &mut stream,
                )
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_play,
            engine_pause,
            engine_stop,
            engine_next,
            engine_set_volume,
            engine_get_state,
            engine_levels,
            engine_seek,
            dead_air_get,
            dead_air_set,
            crossfade_get,
            crossfade_set,
            playlist_changes,
            schedule_get,
            playlists_list,
            playlists_save,
            playlists_load,
            playlists_delete,
            schedule_add,
            schedule_remove,
            schedule_toggle,
            stream_get,
            stream_start,
            stream_stop,
            engine_load_track,
            library_scan_folder,
            folder_cmds::fs_roots,
            folder_cmds::fs_list_dir,
            folder_cmds::library_folders,
            folder_cmds::library_add_folders,
            folder_cmds::library_remove_folder,
            folder_cmds::library_rescan,
            folder_cmds::library_track_for_file,
            metadata_cmds::metadata_lookup_start,
            metadata_cmds::metadata_lookup_status,
            metadata_cmds::metadata_lookup_cancel,
            metadata_cmds::metadata_apply,
            metadata_cmds::metadata_settings_get,
            metadata_cmds::metadata_settings_set,
            metadata_cmds::metadata_missing_count,
            metadata_cmds::metadata_for_paths,
            folder_cmds::playlist_drop_files,
            library_get_tracks,
            library_query,
            library_search,
            playlist_create,
            playlist_get,
            playlist_add,
            playlist_remove,
            playlist_move,
            playlist_clear,
            playlist_play,
            set_locale,
            get_supported_languages,
            automation_cmds::automation_status,
            automation_cmds::automation_get_config,
            automation_cmds::automation_save_config,
            automation_cmds::automation_set_enabled,
            automation_cmds::automation_categories,
            automation_cmds::automation_save_category,
            automation_cmds::automation_delete_category,
            automation_cmds::automation_category_tracks,
            automation_cmds::automation_track_categories,
            automation_cmds::automation_set_track_codes,
            automation_cmds::automation_set_track_enabled,
            automation_cmds::automation_codes,
            automation_cmds::automation_category_members,
            automation_cmds::automation_tracks,
            automation_cmds::automation_rules,
            automation_cmds::automation_save_rule,
            automation_cmds::automation_delete_rule,
            automation_cmds::automation_apply_rules,
            automation_cmds::automation_clocks,
            automation_cmds::automation_save_clock,
            automation_cmds::automation_delete_clock,
            automation_cmds::automation_grid,
            automation_cmds::automation_save_grid_rule,
            automation_cmds::automation_delete_grid_rule,
            automation_cmds::automation_advertisers,
            automation_cmds::automation_save_advertiser,
            automation_cmds::automation_delete_advertiser,
            automation_cmds::automation_campaigns,
            automation_cmds::automation_save_campaign,
            automation_cmds::automation_delete_campaign,
            automation_cmds::automation_preview,
            automation_cmds::automation_log,
            automation_cmds::automation_delivery,
            automation_cmds::automation_export_csv,
            analysis_cmds::analysis_track_cues,
            analysis_cmds::analysis_tracks_cues,
            analysis_cmds::analysis_set_track_cues,
            analysis_cmds::analysis_clear_track_cues,
            analysis_cmds::analysis_analyze_track,
            analysis_cmds::analysis_paths_cues,
            analysis_cmds::preview_play_path,
            playlist_insert,
            playlist_remove_many,
            playlist_remove_played,
            playlist_play_next,
            library_remove_tracks,
            library_track_by_path,
            reveal_in_folder,
            analysis_cmds::analysis_batch_start,
            analysis_cmds::analysis_batch_cancel,
            analysis_cmds::analysis_batch_status,
            analysis_cmds::analysis_stats,
            analysis_cmds::analysis_get_detector,
            analysis_cmds::analysis_set_detector,
            analysis_cmds::audio_devices,
            analysis_cmds::audio_set_main_device,
            analysis_cmds::audio_set_preview_device,
            analysis_cmds::normalization_get,
            analysis_cmds::normalization_set,
            analysis_cmds::preview_play,
            analysis_cmds::preview_stop,
            analysis_cmds::preview_state,
            live_cmds::live_status,
            live_cmds::live_set_mode,
            live_cmds::live_mic_open,
            live_cmds::live_mic_close,
            live_cmds::live_set_mic,
            live_cmds::live_mic_config,
            live_cmds::live_set_mic_config,
            live_cmds::live_input_devices,
            live_cmds::live_banks,
            live_cmds::live_bank,
            live_cmds::live_save_bank,
            live_cmds::live_delete_bank,
            live_cmds::live_active_bank,
            live_cmds::live_set_active_bank,
            live_cmds::live_set_button,
            live_cmds::live_clear_button,
            live_cmds::live_preload,
            live_cmds::live_fire,
            live_cmds::live_stop,
            live_cmds::playlist_set_stop_after,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RIARS");
}
