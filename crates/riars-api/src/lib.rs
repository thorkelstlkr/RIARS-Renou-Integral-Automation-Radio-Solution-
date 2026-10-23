pub mod routes;
pub mod ws;

use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use riars_core::{AudioEngine, MediaLibrary, Scheduler};
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub engine: Mutex<AudioEngine>,
    pub library: Mutex<MediaLibrary>,
    pub scheduler: Mutex<Scheduler>,
    /// Every request must carry it as `Authorization: Bearer <token>`
    /// (or `?token=` on the WebSocket, where browsers cannot set headers).
    pub api_token: String,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", routes::api_routes())
        .nest("/ws", ws::ws_routes())
        .layer(middleware::from_fn_with_state(Arc::clone(&state), require_token))
        // Outermost, so CORS preflight requests are answered without a token.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn require_token(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    // Query tokens end up in logs and history, so only the WebSocket accepts them.
    let query = if req.uri().path().starts_with("/ws") {
        req.uri()
            .query()
            .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")))
    } else {
        None
    };
    let valid = bearer
        .or(query)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), state.api_token.as_bytes()));
    if valid {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing or invalid API token" })),
        )
            .into_response()
    }
}

/// Compare without exiting early, so response time does not reveal how much matched.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn serve(state: Arc<AppState>, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(Arc::clone(&state));
    let addr = format!("0.0.0.0:{port}");
    log::info!("RIARS API server listening on {addr}");
    tokio::spawn(auto_advance(Arc::clone(&state)));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Move to the next playlist item when the current track ends. The desktop app does this
/// from its UI polling; the headless server needs its own loop.
async fn auto_advance(state: Arc<AppState>) {
    let mut tick = tokio::time::interval(tokio::time::Duration::from_millis(250));
    loop {
        tick.tick().await;
        let next = state.engine.lock().ok().and_then(|mut engine| engine.poll());
        if let Some(index) = next {
            let state = Arc::clone(&state);
            let result = tokio::task::spawn_blocking(move || {
                riars_core::play_index_unlocked(&state.engine, index, true)
            })
            .await;
            if let Ok(Err(e)) = result {
                log::error!("Auto-advance failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn token_comparison() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
        assert!(!constant_time_eq(b"abc123", b"abc124"));
        assert!(!constant_time_eq(b"abc", b"abc123"));
        assert!(!constant_time_eq(b"", b"abc"));
    }
}
