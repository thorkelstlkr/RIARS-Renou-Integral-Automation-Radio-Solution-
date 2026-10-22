pub mod routes;
pub mod ws;

use std::sync::{Arc, Mutex};

use axum::Router;
use riars_core::{AudioEngine, MediaLibrary, Scheduler};
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub engine: Mutex<AudioEngine>,
    pub library: Mutex<MediaLibrary>,
    pub scheduler: Mutex<Scheduler>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", routes::api_routes())
        .nest("/ws", ws::ws_routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(state: Arc<AppState>, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(state);
    let addr = format!("0.0.0.0:{port}");
    log::info!("RIARS API server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
