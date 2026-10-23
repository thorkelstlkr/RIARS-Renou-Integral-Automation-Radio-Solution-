use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use riars_core::audio::crossfade::{CrossfadeConfig, FadeCurve};
use riars_core::{play_index_unlocked, EngineStateInfo, IcecastConfig, Playlist};
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
        .route("/player/crossfade", get(get_crossfade).post(set_crossfade))
        .route("/player/load", post(load_track))
        .route("/library/tracks", get(library_tracks))
        .route("/library/search", post(library_search))
        .route("/library/scan", post(library_scan))
        .route("/stream", get(stream_get))
        .route("/stream/start", post(stream_start))
        .route("/stream/stop", post(stream_stop))
        .route("/playlist", get(playlist_get))
        .route("/playlist/add", post(playlist_add))
        .route("/playlist/remove", post(playlist_remove))
        .route("/playlist/move", post(playlist_move))
        .route("/playlist/clear", post(playlist_clear))
        .route("/playlist/play", post(playlist_play))
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

/// Error body for every endpoint: `{"error": "..."}` with a 4xx/5xx status, so clients
/// and agents can tell failures from success.
pub struct ApiError(StatusCode, String);

impl ApiError {
    fn internal(e: impl std::fmt::Display) -> Self {
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
    state.engine.lock().map_err(ApiError::internal)?.play();
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
    let count = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .scan_folder(&req.path)?;
    Ok(Json(CountResponse { count }))
}
