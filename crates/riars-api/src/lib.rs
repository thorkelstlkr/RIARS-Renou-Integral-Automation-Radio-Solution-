mod automation_routes;
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
    /// Where the schedule is saved after every change.
    pub schedule_path: std::path::PathBuf,
    /// Every request must carry it as `Authorization: Bearer <token>`
    /// (or `?token=` on the WebSocket, where browsers cannot set headers).
    pub api_token: String,
    /// Icecast streaming; `None` without a sound card (nothing to stream).
    pub stream: Option<riars_stream::StreamService>,
    /// Saves playlist, position, volume, crossfade and streaming for restarts.
    pub session: Mutex<riars_core::session::SessionSaver>,
    /// Long-term automation: playlist generator and proof-of-play log.
    pub automation: Mutex<riars_core::automation::Automation>,
    /// Automation events (JSON with `"type": "automation"`) for WebSocket clients.
    pub automation_events: tokio::sync::broadcast::Sender<String>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", routes::api_routes())
        .nest("/ws", ws::ws_routes())
        .layer(middleware::from_fn_with_state(Arc::clone(&state), require_token))
        // Added after the auth layer: the page itself is public, everything it does
        // goes through the API with the token.
        .route("/", axum::routing::get(panel))
        // Outermost, so CORS preflight requests are answered without a token.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// The web control panel for headless servers: one static page, no dependencies.
const PANEL_HTML: &str = include_str!("panel.html");

async fn panel() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self' ws: wss:; frame-ancestors 'none'"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        PANEL_HTML,
    )
        .into_response()
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
    let scheduler_state = Arc::clone(&state);
    std::thread::Builder::new()
        .name("riars-scheduler".into())
        .spawn(move || run_scheduler(&scheduler_state))?;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Move to the next playlist item when the current track ends. The desktop app does this
/// from its UI polling; the headless server needs its own loop.
async fn auto_advance(state: Arc<AppState>) {
    let mut tick = tokio::time::interval(tokio::time::Duration::from_millis(250));
    let mut last_title = String::new();
    loop {
        tick.tick().await;
        // A step stuck this long stalls playback: say which one, to find out why.
        let watch = Watchdog::new();
        save_session(&state).await;
        watch.step("session save");
        // Keep the stream's song title in step with what is playing.
        if let Some(stream) = &state.stream {
            let title = state.engine.lock().ok().and_then(|e| e.now_playing_title());
            if let Some(title) = title.filter(|t| *t != last_title) {
                stream.set_title(&title);
                last_title = title;
            }
        }
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
            watch.step("loading the next track");
        }
        run_automation(&state).await;
        watch.step("automation");
    }
}

/// Warns when a step of the control loop takes too long.
struct Watchdog(std::cell::Cell<std::time::Instant>);

impl Watchdog {
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

    fn new() -> Self {
        Self(std::cell::Cell::new(std::time::Instant::now()))
    }

    fn step(&self, name: &str) {
        let took = self.0.get().elapsed();
        if took > Self::LIMIT {
            log::warn!("Control loop: {name} took {took:?}");
        }
        self.0.set(std::time::Instant::now());
    }
}

/// Log what aired and keep the on-air playlist topped up, when there is work.
async fn run_automation(state: &Arc<AppState>) {
    // Engine first, released before the automation lock (see `automation::runner`).
    let Ok((revision, records)) = state.engine.lock().map(|e| (e.playlist_revision(), e.has_play_records())) else {
        return;
    };
    if !state.automation.lock().is_ok_and(|a| a.wants_tick(revision, records)) {
        return;
    }
    let worker = Arc::clone(state);
    let now = chrono::Local::now().naive_local();
    let result = tokio::task::spawn_blocking(move || {
        riars_core::automation::tick(&worker.engine, &worker.library, &worker.automation, now)
    })
    .await;
    match result {
        Ok(Ok(report)) => {
            if report.generated > 0 {
                log::info!("Automation: {} items added", report.generated);
            }
            if report.generated > 0 || report.logged > 0 || report.cut_to.is_some() || !report.warnings.is_empty() {
                let event = serde_json::json!({
                    "type": "automation",
                    "generated": report.generated,
                    "logged": report.logged,
                    "cut_to": report.cut_to,
                    "warnings": report.warnings,
                });
                // No subscribers is fine.
                let _ = state.automation_events.send(event.to_string());
            }
        }
        Ok(Err(e)) => log::warn!("Automation: {e}"),
        Err(_) => {}
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

/// Scheduled events, on a blocking thread because an action may decode a track.
fn run_scheduler(state: &AppState) {
    let mut stream = |command| stream_command(state.stream.as_ref(), command);
    riars_core::automation::scheduler_loop(&state.engine, &state.library, &state.scheduler, &state.schedule_path, &mut stream)
}

pub fn stream_command(
    stream: Option<&riars_stream::StreamService>,
    command: riars_core::automation::StreamCommand,
) -> Result<(), String> {
    use riars_core::automation::StreamCommand;
    match (stream, command) {
        (Some(s), StreamCommand::Start) => s.resume(),
        (Some(s), StreamCommand::Stop) => {
            s.stop();
            Ok(())
        }
        (None, _) => Err("Streaming needs an audio output".to_string()),
    }
}

/// Write the session file when something changed, off the async workers.
async fn save_session(state: &Arc<AppState>) {
    let stream = state.stream.as_ref().and_then(|s| s.config_for_session());
    let due = match (state.session.lock(), state.engine.lock()) {
        (Ok(mut saver), Ok(engine)) => saver.due(&engine, stream),
        _ => None,
    };
    if let Some(session) = due {
        let state = Arc::clone(state);
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(saver) = state.session.lock() {
                saver.write(&session);
            }
        })
        .await;
    }
}
