pub mod analysis_tools;
pub mod automation_tools;
pub mod folder_tools;
pub mod live_tools;
pub mod metadata_tools;
pub mod tools;

use std::sync::{Arc, Mutex};

use riars_core::{AudioEngine, MediaLibrary, Scheduler};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use tools::handle_tool_call;

pub struct McpState {
    pub engine: Arc<Mutex<AudioEngine>>,
    pub library: Arc<Mutex<MediaLibrary>>,
    pub scheduler: Arc<Mutex<Scheduler>>,
    /// Icecast streaming; `None` without a sound card.
    pub stream: Option<riars_stream::StreamService>,
    /// Where the schedule is saved after every change.
    pub schedule_path: std::path::PathBuf,
    /// Saves playlist, position, volume, crossfade and streaming for restarts.
    pub session: Mutex<riars_core::session::SessionSaver>,
    /// Long-term automation: playlist generator and proof-of-play log.
    pub automation: Arc<Mutex<riars_core::automation::Automation>>,
    #[cfg(not(test))]
    pub analysis: riars_core::analysis::BatchAnalyzer,
}

impl McpState {
    pub fn analysis(&self) -> &riars_core::analysis::BatchAnalyzer {
        #[cfg(not(test))]
        {
            &self.analysis
        }
        #[cfg(test)]
        {
            test_analyzer()
        }
    }
}

#[cfg(test)]
pub(crate) fn test_analyzer() -> &'static riars_core::analysis::BatchAnalyzer {
    static ANALYZER: std::sync::OnceLock<riars_core::analysis::BatchAnalyzer> =
        std::sync::OnceLock::new();
    ANALYZER.get_or_init(riars_core::analysis::BatchAnalyzer::new)
}

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Serialize)]
struct ServerInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: &'static str,
    capabilities: Capabilities,
    #[serde(rename = "serverInfo")]
    server_info: ServerInfo,
}

#[derive(Serialize)]
struct Capabilities {
    tools: ToolsCapability,
}

#[derive(Serialize)]
struct ToolsCapability {
    #[serde(rename = "listChanged")]
    list_changed: bool,
}

impl JsonRpcResponse {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

pub async fn run_stdio(state: McpState) -> Result<(), Box<dyn std::error::Error>> {
    tokio::spawn(auto_advance(Arc::clone(&state.engine), Arc::clone(&state.library), Arc::clone(&state.automation)));
    let state = Arc::new(state);
    tokio::spawn(follow_title(Arc::clone(&state)));
    let scheduler_state = Arc::clone(&state);
    std::thread::Builder::new().name("riars-scheduler".into()).spawn(move || {
        let state = scheduler_state;
        let mut stream = |command| {
            use riars_core::automation::StreamCommand;
            match (&state.stream, command) {
                (Some(s), StreamCommand::Start) => s.resume(),
                (Some(s), StreamCommand::Stop) => {
                    s.stop();
                    Ok(())
                }
                (None, _) => Err("Streaming needs an audio output".to_string()),
            }
        };
        riars_core::automation::scheduler_loop(&state.engine, &state.library, &state.scheduler, &state.schedule_path, &mut stream)
    })?;
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    log::info!("RIARS MCP server started (stdio transport)");

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {e}"),
                );
                let json = serde_json::to_string(&resp)?;
                stdout.write_all(json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
                continue;
            }
        };

        let _ = &request.jsonrpc;
        let id = request.id.unwrap_or(serde_json::Value::Null);

        let response = match request.method.as_str() {
            "initialize" => {
                let result = InitializeResult {
                    protocol_version: "2024-11-05",
                    capabilities: Capabilities {
                        tools: ToolsCapability {
                            list_changed: false,
                        },
                    },
                    server_info: ServerInfo {
                        name: "riars-mcp",
                        version: env!("CARGO_PKG_VERSION"),
                    },
                };
                JsonRpcResponse::success(id, serde_json::to_value(result)?)
            }
            "notifications/initialized" => continue,
            "tools/list" => {
                let tools = tools::list_tools();
                JsonRpcResponse::success(id, serde_json::to_value(tools)?)
            }
            "tools/call" => {
                let tool_name = request.params["name"].as_str().unwrap_or("");
                let arguments = &request.params["arguments"];
                match handle_tool_call(tool_name, arguments, &state) {
                    Ok(result) => JsonRpcResponse::success(id, result),
                    Err(e) if e.starts_with(&riars_core::t("unknown_tool")) => {
                        JsonRpcResponse::error(id, -32602, e)
                    }
                    // Tool failures are results with isError, per the MCP spec, so the
                    // agent sees the reason and can recover.
                    Err(e) => JsonRpcResponse::success(
                        id,
                        serde_json::json!({
                            "content": [{ "type": "text", "text": e }],
                            "isError": true
                        }),
                    ),
                }
            }
            _ => JsonRpcResponse::error(id, -32601, format!("Unknown method: {}", request.method)),
        };

        let json = serde_json::to_string(&response)?;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

/// Move to the next playlist item when the current track ends, and run the
/// automation (same as riars-api).
async fn auto_advance(
    engine: Arc<Mutex<AudioEngine>>,
    library: Arc<Mutex<MediaLibrary>>,
    automation: Arc<Mutex<riars_core::automation::Automation>>,
) {
    let mut tick = tokio::time::interval(tokio::time::Duration::from_millis(250));
    loop {
        tick.tick().await;
        // Advance first: the automation must never delay the next track.
        let next = engine.lock().ok().and_then(|mut e| e.poll());
        if let Some(index) = next {
            let engine = Arc::clone(&engine);
            let result = tokio::task::spawn_blocking(move || {
                riars_core::play_index_unlocked(&engine, index, true)
            })
            .await;
            if let Ok(Err(e)) = result {
                log::error!("Auto-advance failed: {e}");
            }
        }
        // Engine first, released before the automation lock (see `automation::runner`).
        let pending = engine.lock().ok().map(|e| (e.playlist_revision(), e.has_play_records()));
        if let Some((revision, records)) = pending {
            if automation.lock().is_ok_and(|a| a.wants_tick(revision, records)) {
                let (engine, library, automation) = (Arc::clone(&engine), Arc::clone(&library), Arc::clone(&automation));
                let now = chrono::Local::now().naive_local();
                let result = tokio::task::spawn_blocking(move || {
                    riars_core::automation::tick(&engine, &library, &automation, now)
                })
                .await;
                if let Ok(Err(e)) = result {
                    log::warn!("Automation: {e}");
                }
            }
        }
    }
}

/// Keep the stream's song title in step with what is playing.
async fn follow_title(state: Arc<McpState>) {
    let mut tick = tokio::time::interval(tokio::time::Duration::from_millis(500));
    let mut last = String::new();
    loop {
        tick.tick().await;
        let config = state.stream.as_ref().and_then(|s| s.config_for_session());
        let due = match (state.session.lock(), state.engine.lock()) {
            (Ok(mut saver), Ok(engine)) => saver.due(&engine, config),
            _ => None,
        };
        if let Some(session) = due {
            if let Ok(saver) = state.session.lock() {
                saver.write(&session);
            }
        }
        let Some(stream) = &state.stream else { continue };
        let title = state.engine.lock().ok().and_then(|e| e.now_playing_title());
        if let Some(title) = title.filter(|t| *t != last) {
            stream.set_title(&title);
            last = title;
        }
    }
}
