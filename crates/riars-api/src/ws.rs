use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn ws_routes() -> Router<Arc<AppState>> {
    Router::new().route("/events", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Serialize the engine state, releasing the lock before the caller awaits on the socket.
fn state_json(state: &AppState) -> Option<String> {
    let engine = state.engine.lock().ok()?;
    serde_json::to_string(&engine.state_info()).ok()
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    log::info!("WebSocket client connected");

    // The state on connect, then only changes (while playing, that is the position).
    let mut last = state_json(&state);
    if let Some(json) = &last {
        let _ = socket.send(Message::Text(json.clone().into())).await;
    }

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let json = state_json(&state);
                if json.is_some() && json != last {
                    if socket.send(Message::Text(json.clone().unwrap_or_default().into())).await.is_err() {
                        break;
                    }
                    last = json;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        log::debug!("WS received: {text}");
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    log::info!("WebSocket client disconnected");
}
