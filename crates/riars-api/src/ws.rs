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

/// The microphone (`"type": "mic"`) and the bank sounds on air (`"type": "cart"`), as
/// two messages. The microphone level is left out: it changes on every read, and
/// `GET /live/status` has it.
fn live_json(state: &AppState) -> Option<(String, String)> {
    let engine = state.engine.lock().ok()?;
    let mic = engine.mic_state();
    let carts: Vec<u32> = engine.carts_playing().iter().map(|c| c.button).collect();
    Some((
        serde_json::json!({
            "type": "mic",
            "available": mic.available,
            "on": mic.on,
            "speaking": mic.speaking,
            "device": mic.device,
        })
        .to_string(),
        serde_json::json!({ "type": "cart", "playing": carts }).to_string(),
    ))
}

/// A client gets this long to take a message; one that stops reading is dropped
/// instead of holding its task (and the state) forever.
const SEND_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(5);

async fn send(socket: &mut WebSocket, text: String) -> bool {
    matches!(tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Text(text.into()))).await, Ok(Ok(())))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    log::info!("WebSocket client connected");

    // The state on connect, then only changes (while playing, that is the position).
    let mut last = state_json(&state);
    if let Some(json) = &last {
        let _ = send(&mut socket, json.clone()).await;
    }

    // Live events only on changes: nothing is sent until the microphone or the bank
    // differ from how they were on connect.
    let mut last_live = live_json(&state);

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
    let mut automation = state.automation_events.subscribe();

    loop {
        tokio::select! {
            event = automation.recv() => {
                match event {
                    Ok(json) => {
                        if !send(&mut socket, json).await {
                            break;
                        }
                    }
                    // A slow client missed some events: carry on with the next ones.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = interval.tick() => {
                let json = state_json(&state);
                if json.is_some() && json != last {
                    if !send(&mut socket, json.clone().unwrap_or_default()).await {
                        break;
                    }
                    last = json;
                }
                let live = live_json(&state);
                if let Some((mic, cart)) = &live {
                    let (old_mic, old_cart) = match &last_live {
                        Some((m, c)) => (Some(m), Some(c)),
                        None => (None, None),
                    };
                    if old_mic != Some(mic) && !send(&mut socket, mic.clone()).await {
                        break;
                    }
                    if old_cart != Some(cart) && !send(&mut socket, cart.clone()).await {
                        break;
                    }
                    last_live = live;
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
