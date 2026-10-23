use serde::Serialize;
use serde_json::{json, Value};

use riars_core::t;

use crate::McpState;

#[derive(Serialize)]
pub struct ToolDef {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Serialize)]
pub struct ToolList {
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
            ToolDef {
                name: "get_playlist",
                description: "Get the current playlist",
                input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
            },
            ToolDef {
                name: "playlist_add",
                description: "Add a track to the playlist",
                input_schema: json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Absolute path to the audio file" }, "title": { "type": "string", "description": "Optional title of the track", "default": "" } }, "required": ["path"] }),
            },
            ToolDef {
                name: "playlist_remove",
                description: "Remove a track from the playlist by index",
                input_schema: json!({ "type": "object", "properties": { "index": { "type": "integer", "description": "Index of the track to remove" } }, "required": ["index"] }),
            },
            ToolDef {
                name: "playlist_move",
                description: "Move a track in the playlist",
                input_schema: json!({ "type": "object", "properties": { "from": { "type": "integer", "description": "Index of the track to move" }, "to": { "type": "integer", "description": "New index for the track" } }, "required": ["from", "to"] }),
            },
            ToolDef {
                name: "playlist_clear",
                description: "Clear the playlist",
                input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
            },
            ToolDef {
                name: "playlist_play",
                description: "Play a track by index",
                input_schema: json!({ "type": "object", "properties": { "index": { "type": "integer", "description": "Index of the track to play" } }, "required": ["index"] }),
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
            state.engine.lock().map_err(|e| e.to_string())?.next_track()?;
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
        "get_playlist" => {
            let engine = state.engine.lock().map_err(|e| e.to_string())?;
            Ok(text_result(&serde_json::to_string_pretty(engine.playlist()).unwrap_or_default()))
        },
        "playlist_add" => {
            let path = args["path"].as_str().ok_or(t("missing_path"))?;
            let title = args.get("title").and_then(|v| v.as_str()).unwrap_or_else(|| std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or_default());
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            engine.playlist_mut().add_item(path.to_string(), title.to_string(), "".to_string(), 0);
            Ok(text_result(&serde_json::to_string_pretty(engine.playlist()).unwrap_or_default()))
        },
        "playlist_remove" => {
            let index = args["index"].as_u64().ok_or("Missing index")? as usize;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            if engine.playlist_mut().remove_item(index).is_none() {
                return Err(format!("No playlist item at {}", index));
            }
            Ok(text_result(&serde_json::to_string_pretty(engine.playlist()).unwrap_or_default()))
        },
        "playlist_move" => {
            let from = args["from"].as_u64().ok_or("Missing from")? as usize;
            let to = args["to"].as_u64().ok_or("Missing to")? as usize;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            engine.playlist_mut().move_item(from, to);
            Ok(text_result(&serde_json::to_string_pretty(engine.playlist()).unwrap_or_default()))
        },
        "playlist_clear" => {
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            engine.playlist_mut().clear();
            Ok(text_result(&serde_json::to_string_pretty(engine.playlist()).unwrap_or_default()))
        },
        "playlist_play" => {
            let index = args["index"].as_u64().ok_or("Missing index")? as usize;
            riars_core::play_index_unlocked(&state.engine, index, false)?;
            Ok(text_result("Playing"))
        },
        _ => Err(format!("{}: {name}", t("unknown_tool"))),
    }
}

fn text_result(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}
