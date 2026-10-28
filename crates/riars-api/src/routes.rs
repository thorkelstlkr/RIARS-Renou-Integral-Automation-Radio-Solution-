use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use riars_core::audio::crossfade::{CrossfadeConfig, FadeCurve};
use riars_core::playlist::scheduler::{EventAction, EventTrigger, ScheduledEvent};
use riars_core::{DeadAirConfig, play_index_unlocked, EngineStateInfo, IcecastConfig, Playlist, SavedPlaylist};
use riars_stream::{StreamService, StreamStatus};

use crate::AppState;

#[derive(Serialize)]
struct ApiInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct StatusResponse {
    engine: EngineStateInfo,
    library_tracks: usize,
    scheduled_events: usize,
}

#[derive(Deserialize)]
pub struct VolumeRequest {
    pub level: f32,
}

#[derive(Deserialize)]
pub struct LoadTrackRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct ScanRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

#[derive(Serialize)]
struct CountResponse {
    count: usize,
}

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(info))
        .route("/status", get(status))
        .route("/player/play", post(play))
        .route("/player/pause", post(pause))
        .route("/player/stop", post(stop))
        .route("/player/next", post(next))
        .route("/player/volume", post(set_volume))
        .route("/player/levels", get(levels))
        .route("/player/dead-air", get(get_dead_air).post(set_dead_air))
        .route("/player/seek", post(seek))
        .route("/player/crossfade", get(get_crossfade).post(set_crossfade))
        .route("/player/load", post(load_track))
        .route("/library/tracks", get(library_tracks))
        .route("/library/search", post(library_search))
        .route("/library/scan", post(library_scan))
        .route("/library/query", get(library_query))
        .route("/stream", get(stream_get))
        .route("/stream/start", post(stream_start))
        .route("/stream/stop", post(stream_stop))
        .route("/schedule", get(schedule_get))
        .route("/schedule/add", post(schedule_add))
        .route("/schedule/remove", post(schedule_remove))
        .route("/schedule/toggle", post(schedule_toggle))
        .route("/playlists", get(playlists_list))
        .route("/playlists/save", post(playlists_save))
        .route("/playlists/load", post(playlists_load))
        .route("/playlists/delete", post(playlists_delete))
        .route("/playlist", get(playlist_get))
        .route("/playlist/add", post(playlist_add))
        .route("/playlist/remove", post(playlist_remove))
        .route("/playlist/move", post(playlist_move))
        .route("/playlist/clear", post(playlist_clear))
        .route("/playlist/play", post(playlist_play))
        .merge(crate::analysis_routes::analysis_routes())
        .merge(crate::automation_routes::automation_routes())
        .merge(crate::folder_routes::router())
        .merge(crate::live_routes::router())
}

#[derive(Deserialize)]
struct PlaylistAddRequest {
    path: String,
    title: Option<String>,
    artist: Option<String>,
}

#[derive(Deserialize)]
struct IndexRequest {
    index: usize,
}

#[derive(Deserialize)]
struct MoveRequest {
    from: usize,
    to: usize,
}

async fn playlist_get(State(state): State<Arc<AppState>>) -> Result<Json<Playlist>, ApiError> {
    let playlist = state.engine.lock().map_err(ApiError::internal)?.playlist().clone();
    Ok(Json(playlist))
}

async fn playlist_add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PlaylistAddRequest>,
) -> Result<Json<Playlist>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    let title = req.title.unwrap_or_else(|| {
        std::path::Path::new(&req.path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let artist = req.artist.unwrap_or_default();
    engine.playlist_mut().add_item(req.path, title, artist, 0);
    Ok(Json(engine.playlist().clone()))
}

async fn playlist_remove(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IndexRequest>,
) -> Result<Json<Playlist>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine
        .playlist_mut()
        .remove_item(req.index)
        .ok_or_else(|| format!("No playlist item at {}", req.index))?;
    Ok(Json(engine.playlist().clone()))
}

async fn playlist_move(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<Playlist>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.playlist_mut().move_item(req.from, req.to);
    Ok(Json(engine.playlist().clone()))
}

async fn playlist_clear(State(state): State<Arc<AppState>>) -> Result<Json<Playlist>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.playlist_mut().clear();
    Ok(Json(engine.playlist().clone()))
}

async fn playlist_play(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IndexRequest>,
) -> Result<&'static str, ApiError> {
    play_unlocked(state, req.index).await
}


#[derive(Serialize, Deserialize)]
pub struct CrossfadeSettings {
    pub enabled: bool,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

impl From<&CrossfadeConfig> for CrossfadeSettings {
    fn from(config: &CrossfadeConfig) -> Self {
        CrossfadeSettings {
            enabled: config.enabled,
            duration_ms: config.duration.as_millis() as u64,
            curve: config.curve,
        }
    }
}

impl From<CrossfadeSettings> for CrossfadeConfig {
    fn from(settings: CrossfadeSettings) -> Self {
        CrossfadeConfig {
            duration: std::time::Duration::from_millis(settings.duration_ms),
            curve: settings.curve,
            enabled: settings.enabled,
        }
    }
}

async fn get_crossfade(State(state): State<Arc<AppState>>) -> Result<Json<CrossfadeSettings>, ApiError> {
    let engine = state.engine.lock().map_err(ApiError::internal)?;
    Ok(Json(CrossfadeSettings::from(engine.crossfade_config())))
}

async fn set_crossfade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CrossfadeSettings>,
) -> Result<Json<CrossfadeSettings>, ApiError> {
    if req.duration_ms > 30000 {
        return Err(ApiError::from("duration_ms must be at most 30000".to_string()));
    }
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.set_crossfade(req.into());
    Ok(Json(CrossfadeSettings::from(engine.crossfade_config())))
}

#[derive(Serialize)]
struct StreamResponse {
    status: StreamStatus,
    config: Option<IcecastConfig>,
}

fn stream_service(state: &AppState) -> Result<&StreamService, ApiError> {
    state.stream.as_ref().ok_or(ApiError::from("Streaming needs an audio output device".to_string()))
}

async fn stream_get(State(state): State<Arc<AppState>>) -> Result<Json<StreamResponse>, ApiError> {
    let service = stream_service(&state)?;
    let response = StreamResponse {
        status: service.status(),
        config: service.config(),
    };
    Ok(Json(response))
}

async fn stream_start(State(state): State<Arc<AppState>>, Json(config): Json<IcecastConfig>) -> Result<Json<StreamResponse>, ApiError> {
    let service = stream_service(&state)?;
    service.start(config)?;
    let response = StreamResponse {
        status: service.status(),
        config: service.config(),
    };
    Ok(Json(response))
}

async fn stream_stop(State(state): State<Arc<AppState>>) -> Result<Json<StreamResponse>, ApiError> {
    let service = stream_service(&state)?;
    service.stop();
    let response = StreamResponse {
        status: service.status(),
        config: service.config(),
    };
    Ok(Json(response))
}

#[derive(Deserialize)]
struct AddEventRequest {
    name: String,
    trigger: EventTrigger,
    action: EventAction,
}

#[derive(Deserialize)]
struct EventIdRequest {
    id: String,
}

async fn schedule_get(State(state): State<Arc<AppState>>) -> Result<Json<Vec<ScheduledEvent>>, ApiError> {
    Ok(Json(state.scheduler.lock().map_err(ApiError::internal)?.events().to_vec()))
}

async fn schedule_add(State(state): State<Arc<AppState>>, Json(req): Json<AddEventRequest>) -> Result<Json<ScheduledEvent>, ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::from("name is required".to_string()));
    }
    let mut scheduler = state.scheduler.lock().map_err(ApiError::internal)?;
    let event = ScheduledEvent::new(req.name, req.trigger, req.action);
    scheduler.add_event(event.clone());
    scheduler.save(&state.schedule_path)?;
    Ok(Json(event))
}

async fn schedule_remove(State(state): State<Arc<AppState>>, Json(req): Json<EventIdRequest>) -> Result<Json<Vec<ScheduledEvent>>, ApiError> {
    let mut scheduler = state.scheduler.lock().map_err(ApiError::internal)?;
    if !scheduler.remove_event(&req.id) {
        return Err(ApiError::from(format!("No scheduled event {}", req.id)));
    }
    scheduler.save(&state.schedule_path)?;
    Ok(Json(scheduler.events().to_vec()))
}

async fn schedule_toggle(State(state): State<Arc<AppState>>, Json(req): Json<EventIdRequest>) -> Result<Json<Vec<ScheduledEvent>>, ApiError> {
    let mut scheduler = state.scheduler.lock().map_err(ApiError::internal)?;
    if scheduler.toggle_event(&req.id).is_some() {
        scheduler.save(&state.schedule_path)?;
        Ok(Json(scheduler.events().to_vec()))
    } else {
        Err(ApiError::from(format!("No scheduled event {}", req.id)))
    }
}

#[derive(Deserialize)]
struct SavePlaylistRequest { name: String }

#[derive(Deserialize)]
struct LoadPlaylistRequest { id: String, #[serde(default)] play: bool }

async fn playlists_list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<SavedPlaylist>>, ApiError> {
    let playlists = state.library.lock().map_err(ApiError::internal)?.list_playlists()?;
    Ok(Json(playlists))
}

async fn playlists_save(State(state): State<Arc<AppState>>, Json(req): Json<SavePlaylistRequest>) -> Result<Json<Vec<SavedPlaylist>>, ApiError> {
    let playlist = state.engine.lock().map_err(ApiError::internal)?.playlist().clone();
    state.library.lock().map_err(ApiError::internal)?.save_playlist(&req.name, &playlist)?;
    playlists_list(State(state)).await
}

async fn playlists_load(State(state): State<Arc<AppState>>, Json(req): Json<LoadPlaylistRequest>) -> Result<Json<Playlist>, ApiError> {
    let playlist = state.library.lock().map_err(ApiError::internal)?.load_playlist(&req.id)?;
    state.engine.lock().map_err(ApiError::internal)?.set_playlist(playlist.clone());
    if req.play {
        play_unlocked(Arc::clone(&state), 0).await?;
    }
    Ok(Json(playlist))
}

async fn playlists_delete(State(state): State<Arc<AppState>>, Json(req): Json<EventIdRequest>) -> Result<Json<Vec<SavedPlaylist>>, ApiError> {
    state.library.lock().map_err(ApiError::internal)?.delete_playlist(&req.id)?;
    playlists_list(State(state)).await
}

/// Error body for every endpoint: `{"error": "..."}` with a 4xx/5xx status, so clients
/// and agents can tell failures from success.
pub struct ApiError(pub(crate) StatusCode, pub(crate) String);

impl ApiError {
    pub(crate) fn internal(e: impl std::fmt::Display) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

/// Output peaks since the previous call (each read resets them).
async fn levels(State(state): State<Arc<AppState>>) -> Result<Json<riars_core::audio::Levels>, ApiError> {
    Ok(Json(state.engine.lock().map_err(ApiError::internal)?.take_levels()))
}

#[derive(Deserialize)]
struct SeekRequest {
    position_ms: u64,
}

async fn seek(State(state): State<Arc<AppState>>, Json(req): Json<SeekRequest>) -> Result<&'static str, ApiError> {
    state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .seek(std::time::Duration::from_millis(req.position_ms))?;
    Ok("ok")
}

#[derive(Deserialize)]
struct LibraryQuery {
    #[serde(default)]
    search: String,
    #[serde(default = "default_page")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_page() -> usize {
    100
}

/// One page of the library (GET /library/query?search=&limit=&offset=).
async fn library_query(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<LibraryQuery>,
) -> Result<Json<riars_core::TrackPage>, ApiError> {
    let library = state.library.lock().map_err(ApiError::internal)?;
    Ok(Json(library.query(&q.search, q.limit, q.offset)?))
}

async fn get_dead_air(State(state): State<Arc<AppState>>) -> Result<Json<DeadAirConfig>, ApiError> {
    let config = state.engine.lock().map_err(ApiError::internal)?.dead_air_config().clone();
    Ok(Json(config))
}

async fn set_dead_air(State(state): State<Arc<AppState>>, Json(req): Json<DeadAirConfig>) -> Result<Json<DeadAirConfig>, ApiError> {
    req.validate()?;
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.set_dead_air(req);
    Ok(Json(engine.dead_air_config().clone()))
}

async fn info() -> Json<ApiInfo> {
    Json(ApiInfo {
        name: "RIARS — Renou Integral Automation Radio Solution",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn status(State(state): State<Arc<AppState>>) -> Result<Json<StatusResponse>, ApiError> {
    let engine = state.engine.lock().map_err(ApiError::internal)?;
    let library = state.library.lock().map_err(ApiError::internal)?;
    let scheduler = state.scheduler.lock().map_err(ApiError::internal)?;

    Ok(Json(StatusResponse {
        engine: engine.state_info(),
        library_tracks: library.track_count().unwrap_or(0),
        scheduled_events: scheduler.events().len(),
    }))
}

async fn play(State(state): State<Arc<AppState>>) -> Result<&'static str, ApiError> {
    tokio::task::spawn_blocking(move || riars_core::play_unlocked(&state.engine))
        .await
        .map_err(ApiError::internal)??;
    Ok("ok")
}

async fn pause(State(state): State<Arc<AppState>>) -> Result<&'static str, ApiError> {
    state.engine.lock().map_err(ApiError::internal)?.pause();
    Ok("ok")
}

async fn stop(State(state): State<Arc<AppState>>) -> Result<&'static str, ApiError> {
    state.engine.lock().map_err(ApiError::internal)?.stop();
    Ok("ok")
}

async fn next(State(state): State<Arc<AppState>>) -> Result<&'static str, ApiError> {
    let next = state.engine.lock().map_err(ApiError::internal)?.next_index();
    match next {
        Some(index) => play_unlocked(state, index).await,
        None => {
            state.engine.lock().map_err(ApiError::internal)?.stop();
            Ok("ok")
        }
    }
}

/// Decode on a blocking thread without holding the engine lock, so /status stays fast.
async fn play_unlocked(state: Arc<AppState>, index: usize) -> Result<&'static str, ApiError> {
    tokio::task::spawn_blocking(move || play_index_unlocked(&state.engine, index, false))
        .await
        .map_err(ApiError::internal)??;
    Ok("ok")
}

async fn set_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VolumeRequest>,
) -> Result<&'static str, ApiError> {
    state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .set_volume(req.level);
    Ok("ok")
}

async fn load_track(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoadTrackRequest>,
) -> Result<&'static str, ApiError> {
    state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .load_track(&req.path)?;
    Ok("ok")
}

async fn library_tracks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<riars_core::Track>>, ApiError> {
    let tracks = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .get_all_tracks()?;
    Ok(Json(tracks))
}

async fn library_search(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<riars_core::Track>>, ApiError> {
    let tracks = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .search(&req.query)?;
    Ok(Json(tracks))
}

async fn library_scan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<CountResponse>, ApiError> {
    let stored = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .add_folder(&req.path)?;
    let worker = Arc::clone(&state);
    let count = tokio::task::spawn_blocking(move || {
        riars_core::library::folders::scan_into(&worker.library, &stored)
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(Json(CountResponse { count }))
}
