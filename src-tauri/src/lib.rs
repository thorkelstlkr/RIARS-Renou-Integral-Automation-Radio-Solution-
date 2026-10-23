use std::sync::Mutex;

use riars_core::{AudioEngine, EngineStateInfo, MediaLibrary, Playlist, Track};
use std::path::PathBuf;

use riars_core::automation::StreamCommand;
use riars_core::playlist::scheduler::{EventAction, EventTrigger, ScheduledEvent};
use riars_core::{IcecastConfig, SavedPlaylist, Scheduler};
use riars_stream::{StreamService, StreamStatus};
use tauri::{AppHandle, Manager, State};

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
fn engine_play(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.play();
    Ok(())
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

/// Output peaks since the last call; the VU meter polls it.
#[tauri::command]
fn engine_levels(state: State<AppState>) -> Result<riars_core::audio::Levels, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.take_levels())
}

#[tauri::command]
fn engine_get_state(app: AppHandle, state: State<AppState>) -> Result<EngineStateInfo, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    // The UI polls this every 250 ms, which also drives auto-advance in the background.
    if let Some(index) = engine.poll() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = decode_and_start(app, index, true).await {
                log::error!("Auto-advance failed: {e}");
            }
        });
    }
    // Keep the stream's song title in step with what is playing.
    if let (Some(stream), Some(title)) = (&state.stream, engine.now_playing_title()) {
        if let Ok(mut last) = state.stream_title.lock() {
            if *last != title {
                stream.set_title(&title);
                *last = title;
            }
        }
    }
    Ok(engine.state_info())
}

#[tauri::command]
fn engine_load_track(path: String, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .load_track(&path)
}

#[tauri::command]
fn library_scan_folder(path: String, state: State<AppState>) -> Result<usize, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .scan_folder(&path)
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

    let mut engine = AudioEngine::new().expect("Failed to initialize audio engine");
    let stream = engine.take_stream_tap().map(StreamService::new);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Keep the library in the per-user app data folder, not the working directory.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("riars_library.db");
            let library = MediaLibrary::open(&db_path.to_string_lossy())?;
            let schedule_path = data_dir.join("riars_schedule.json");
            let scheduler = Scheduler::load(&schedule_path).unwrap_or_else(|e| {
                log::warn!("{e}; starting with an empty schedule");
                Scheduler::new()
            });
            log::info!("Media library: {}", db_path.display());

            app.manage(AppState {
                stream,
                stream_title: Mutex::new(String::new()),
                scheduler: Mutex::new(scheduler),
                schedule_path,
                engine: Mutex::new(engine),
                library: Mutex::new(library),
            });
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
            library_get_tracks,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running RIARS");
}
