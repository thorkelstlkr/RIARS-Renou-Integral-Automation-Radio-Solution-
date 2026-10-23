use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use riars_core::EngineStateInfo;

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
        .route("/player/load", post(load_track))
        .route("/library/tracks", get(library_tracks))
        .route("/library/search", post(library_search))
        .route("/library/scan", post(library_scan))
}

async fn info() -> Json<ApiInfo> {
    Json(ApiInfo {
        name: "RIARS — Renou Integral Automation Radio Solution",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn status(State(state): State<Arc<AppState>>) -> Result<Json<StatusResponse>, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    let library = state.library.lock().map_err(|e| e.to_string())?;
    let scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;

    Ok(Json(StatusResponse {
        engine: engine.state_info(),
        library_tracks: library.track_count().unwrap_or(0),
        scheduled_events: scheduler.events().len(),
    }))
}

async fn play(State(state): State<Arc<AppState>>) -> Result<&'static str, String> {
    state.engine.lock().map_err(|e| e.to_string())?.play();
    Ok("ok")
}

async fn pause(State(state): State<Arc<AppState>>) -> Result<&'static str, String> {
    state.engine.lock().map_err(|e| e.to_string())?.pause();
    Ok("ok")
}

async fn stop(State(state): State<Arc<AppState>>) -> Result<&'static str, String> {
    state.engine.lock().map_err(|e| e.to_string())?.stop();
    Ok("ok")
}

async fn next(State(state): State<Arc<AppState>>) -> Result<&'static str, String> {
    state.engine.lock().map_err(|e| e.to_string())?.next_track()?;
    Ok("ok")
}

async fn set_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VolumeRequest>,
) -> Result<&'static str, String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .set_volume(req.level);
    Ok("ok")
}

async fn load_track(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoadTrackRequest>,
) -> Result<&'static str, String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .load_track(&req.path)?;
    Ok("ok")
}

async fn library_tracks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<riars_core::Track>>, String> {
    let tracks = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .get_all_tracks()?;
    Ok(Json(tracks))
}

async fn library_search(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<riars_core::Track>>, String> {
    let tracks = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .search(&req.query)?;
    Ok(Json(tracks))
}

async fn library_scan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<CountResponse>, String> {
    let count = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .scan_folder(&req.path)?;
    Ok(Json(CountResponse { count }))
}
