use serde::Serialize;
use serde_json::{json, Value};

use riars_core::t;

use crate::McpState;

#[derive(Serialize)]
struct ToolDef {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Serialize)]
struct ToolList {
    tools: Vec<ToolDef>,
}

pub fn list_tools() -> ToolList {
    ToolList {
        tools: vec![
            ToolDef {
                name: "get_status",
                description: "Get the current state of the radio: playback status, current track, volume, library size, and scheduled events count",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "play",
                description: "Start or resume audio playback",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "pause",
                description: "Pause audio playback",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "stop",
                description: "Stop audio playback and reset position to the beginning",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "next_track",
                description: "Skip to the next track in the playlist",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "set_volume",
                description: "Set the master volume level",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "level": {
                            "type": "number",
                            "description": "Volume level from 0.0 (muted) to 1.0 (maximum)",
                            "minimum": 0.0,
                            "maximum": 1.0
                        }
                    },
                    "required": ["level"]
                }),
            },
            ToolDef {
                name: "load_track",
                description: "Load an audio file for playback by its file path",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the audio file"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolDef {
                name: "search_library",
                description: "Search the music library by title, artist, album, or genre",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (matches title, artist, album, or genre)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDef {
                name: "list_tracks",
                description: "List all tracks in the music library",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "scan_folder",
                description: "Scan a folder for audio files and add them to the library",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the folder to scan for audio files"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolDef {
                name: "list_scheduled_events",
                description: "List all scheduled automation events (programming schedule)",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
        ],
    }
}

pub fn handle_tool_call(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    match name {
        "get_status" => {
            let engine = state.engine.lock().map_err(|e| e.to_string())?;
            let library = state.library.lock().map_err(|e| e.to_string())?;
            let scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "engine": engine.state_info(),
                        "library_tracks": library.track_count().unwrap_or(0),
                        "scheduled_events": scheduler.events().len()
                    })).unwrap_or_default()
                }]
            }))
        }
        "play" => {
            state.engine.lock().map_err(|e| e.to_string())?.play();
            Ok(text_result(&t("playback_started")))
        }
        "pause" => {
            state.engine.lock().map_err(|e| e.to_string())?.pause();
            Ok(text_result(&t("playback_paused")))
        }
        "stop" => {
            state.engine.lock().map_err(|e| e.to_string())?.stop();
            Ok(text_result(&t("playback_stopped")))
        }
        "next_track" => {
            state.engine.lock().map_err(|e| e.to_string())?.next_track();
            Ok(text_result(&t("skipped_next")))
        }
        "set_volume" => {
            let level = args["level"].as_f64().ok_or(t("missing_level"))? as f32;
            state
                .engine
                .lock()
                .map_err(|e| e.to_string())?
                .set_volume(level);
            Ok(text_result(&format!("{} {:.0}%", t("volume_set"), level * 100.0)))
        }
        "load_track" => {
            let path = args["path"].as_str().ok_or(t("missing_path"))?;
            state
                .engine
                .lock()
                .map_err(|e| e.to_string())?
                .load_track(path)?;
            Ok(text_result(&format!("{}: {path}", t("loaded_track"))))
        }
        "search_library" => {
            let query = args["query"].as_str().ok_or(t("missing_query"))?;
            let tracks = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .search(query)?;
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&tracks).unwrap_or_default()
                }]
            }))
        }
        "list_tracks" => {
            let tracks = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .get_all_tracks()?;
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&tracks).unwrap_or_default()
                }]
            }))
        }
        "scan_folder" => {
            let path = args["path"].as_str().ok_or(t("missing_path"))?;
            let count = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .scan_folder(path)?;
            Ok(text_result(&format!(
                "{}: {count} {}",
                t("scanned_folder"),
                t("tracks_added")
            )))
        }
        "list_scheduled_events" => {
            let scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
            let events = scheduler.events();
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&events).unwrap_or_default()
                }]
            }))
        }
        _ => Err(format!("{}: {name}", t("unknown_tool"))),
    }
}

fn text_result(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}
