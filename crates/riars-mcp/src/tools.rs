use serde::Serialize;
use serde_json::{json, Value};

use riars_core::audio::crossfade::FadeCurve;
use riars_core::streaming::icecast::{IcecastConfig, StreamFormat};
use riars_core::playlist::scheduler::{EventAction, EventTrigger, ScheduledEvent};
use riars_core::DeadAirAction;
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
                description: "Search the music library by title, artist, album, or genre (every word must match; case and accents ignored). Returns one page and the total.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (matches title, artist, album, or genre)"
                        },
                        "limit": { "type": "integer", "description": "Tracks per page (default 50, max 500)" },
                        "offset": { "type": "integer", "description": "Tracks to skip, for the next pages" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDef {
                name: "list_tracks",
                description: "List the music library, sorted by artist and title, one page at a time (with the total)",
                input_schema: json!({ "type": "object", "properties": { "limit": { "type": "integer", "description": "Tracks per page (default 50, max 500)" },
                        "offset": { "type": "integer", "description": "Tracks to skip, for the next pages" } } }),
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
            ToolDef {
                name: "set_crossfade",
                description: "Set crossfade settings",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "duration_ms": { "type": "integer", "minimum": 0, "maximum": 30000 },
                        "curve": { "type": "string", "enum": ["Linear", "EqualPower", "SCurve"] }
                    },
                    "required": []
                }),
            },
            ToolDef {
                name: "stream_start",
                description: "Start streaming the radio to an Icecast server",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "host": { "type": "string", "description": "Icecast server host", "required": true },
                        "port": { "type": "integer", "description": "Icecast server port", "required": true },
                        "mount": { "type": "string", "description": "Mount point", "required": true },
                        "password": { "type": "string", "description": "Icecast server password", "required": true },
                        "username": { "type": "string", "description": "Icecast username", "default": "source" },
                        "bitrate": { "type": "integer", "description": "Bitrate", "default": 128 },
                        "station_name": { "type": "string", "description": "Station name" },
                        "station_genre": { "type": "string", "description": "Station genre" }
                    },
                    "required": ["host", "port", "mount", "password"]
                }),
            },
            ToolDef {
                name: "stream_stop",
                description: "Stop streaming",
                input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
            },
            ToolDef {
                name: "stream_status",
                description: "Get the streaming state, bytes sent, title and last error",
                input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
            },
            ToolDef {
                name: "add_scheduled_event",
                description: "Schedule an automation event.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Name of the event" },
                        "trigger": { "type": "object", "description": "Trigger for the event" },
                        "action": { "type": "object", "description": "Action to perform" }
                    },
                    "required": ["name", "trigger", "action"]
                }),
            },
            ToolDef {
                name: "remove_scheduled_event",
                description: "Remove a scheduled event.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "ID of the event" }
                    },
                    "required": ["id"]
                }),
            },
            ToolDef {
                name: "toggle_scheduled_event",
                description: "Toggle a scheduled event.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "ID of the event" }
                    },
                    "required": ["id"]
                }),
            },
            ToolDef {
                name: "list_saved_playlists",
                description: "List all saved playlists",
                input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
            },
            ToolDef {
                name: "save_playlist",
                description: "Save the on-air playlist under a name (replaces a saved playlist with the same name)",
                input_schema: json!({ "type": "object", "properties": { "name": { "type": "string", "description": "Name of the playlist", "required": true } }, "required": ["name"] }),
            },
            ToolDef {
                name: "load_playlist",
                description: "Put a saved playlist on air, by id or name",
                input_schema: json!({ "type": "object", "properties": { "id": { "type": "string", "description": "ID or name of the playlist", "required": true }, "play": { "type": "boolean", "description": "Play the playlist immediately", "default": false } }, "required": ["id"] }),
            },
            ToolDef {
                name: "delete_saved_playlist",
                description: "Delete a saved playlist",
                input_schema: json!({ "type": "object", "properties": { "id": { "type": "string", "description": "ID of the playlist", "required": true } }, "required": ["id"] }),
            },
            ToolDef {
                name: "seek",
                description: "Jump to a position in the current track",
                input_schema: json!({ "type": "object", "properties": { "position_ms": { "type": "integer", "minimum": 0, "description": "Position from the start, in milliseconds" } }, "required": ["position_ms"] }),
            },
            ToolDef {
                name: "set_dead_air",
                description: "Configure dead-air (silence) detection: how long silence may last and what to do. Fields left out keep their value.",
                input_schema: json!({ "type": "object", "properties": {
                    "enabled": { "type": "boolean" },
                    "threshold_db": { "type": "number", "description": "Silence level, dBFS before volume (-90 to 0, default -50)" },
                    "seconds": { "type": "integer", "description": "How long silence may last (1 to 600, default 10)" },
                    "action": { "type": "string", "enum": ["Skip", "Alert"], "description": "Skip to the next item, or only raise the alarm" }
                } }),
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
        "seek" => {
            let ms = args["position_ms"].as_u64().ok_or("Missing position_ms")?;
            state
                .engine
                .lock()
                .map_err(|e| e.to_string())?
                .seek(std::time::Duration::from_millis(ms))?;
            Ok(text_result(&format!("Position {}:{:02}", ms / 60_000, ms / 1000 % 60)))
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
        // Both return one page plus the total: a big library would flood an agent's context.
        "search_library" | "list_tracks" => {
            if name == "search_library" && args["query"].as_str().is_none() {
                return Err(t("missing_query"));
            }
            let search = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(50).min(500) as usize;
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            let page = state.library.lock().map_err(|e| e.to_string())?.query(search, limit, offset)?;
            Ok(text_result(&serde_json::to_string_pretty(&page).unwrap_or_default()))
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
        "set_crossfade" => {
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            let mut config = engine.crossfade_config().clone();
            if let Some(enabled) = args["enabled"].as_bool() {
                config.enabled = enabled;
            }
            if let Some(ms) = args["duration_ms"].as_u64() {
                if ms > 30000 {
                    return Err("duration_ms must be at most 30000".to_string());
                }
                config.duration = std::time::Duration::from_millis(ms);
            }
            if let Some(curve) = args["curve"].as_str() {
                config.curve = match curve {
                    "Linear" => FadeCurve::Linear,
                    "EqualPower" => FadeCurve::EqualPower,
                    "SCurve" => FadeCurve::SCurve,
                    other => return Err(format!("Unknown curve: {other}")),
                };
            }
            engine.set_crossfade(config);
            let c = engine.crossfade_config();
            Ok(text_result(&format!(
                "Crossfade: {}, {} ms, {:?}",
                if c.enabled { "on" } else { "off" },
                c.duration.as_millis(),
                c.curve
            )))
        }
        "stream_start" => {
            let host = args["host"].as_str().ok_or("Missing host")?;
            let port = args["port"]
                .as_u64()
                .and_then(|p| u16::try_from(p).ok())
                .ok_or("Missing or invalid port")?;
            let mount = args["mount"].as_str().ok_or("Missing mount")?;
            let password = args["password"].as_str().ok_or("Missing password")?;
            let username = args.get("username").and_then(|v| v.as_str()).unwrap_or("source");
            let bitrate = args.get("bitrate").and_then(|v| v.as_u64()).map_or(128, |v| v as u32);
            let station_name = args.get("station_name").and_then(|v| v.as_str());
            let station_genre = args.get("station_genre").and_then(|v| v.as_str());

            let config = IcecastConfig {
                host: host.to_string(),
                port,
                mount: mount.to_string(),
                password: password.to_string(),
                username: username.to_string(),
                bitrate,
                format: StreamFormat::Mp3,
                station_name: station_name.map_or_else(|| "".to_string(), |s| s.to_string()),
                station_description: "".to_string(),
                station_genre: station_genre.map_or_else(|| "".to_string(), |s| s.to_string()),
                station_url: "".to_string(),
                encoder_quality: Default::default(),
            };

            let stream = state.stream.as_ref().ok_or("Streaming needs an audio output device")?;
            stream.start(config)?;

            let status = stream.status();
            Ok(text_result(&serde_json::to_string_pretty(&status).unwrap_or_default()))
        },
        "stream_stop" => {
            let stream = state.stream.as_ref().ok_or("Streaming needs an audio output device")?;
            stream.stop();
            let status = stream.status();
            Ok(text_result(&serde_json::to_string_pretty(&status).unwrap_or_default()))
        },
        "stream_status" => {
            let stream = state.stream.as_ref().ok_or("Streaming needs an audio output device")?;
            let status = stream.status();
            let config = stream.config();
            Ok(text_result(&serde_json::to_string_pretty(&json!({
                "status": status,
                "config": config
            })).unwrap_or_default()))
        }
        "add_scheduled_event" => {
            let name = args["name"].as_str().ok_or("Missing name".to_string())?.trim().to_string();
            if name.is_empty() {
                return Err("Missing name".to_string());
            }
            let trigger: EventTrigger = serde_json::from_value(args["trigger"].clone()).map_err(|e| format!("Invalid trigger: {e}"))?;
            let action: EventAction = serde_json::from_value(args["action"].clone()).map_err(|e| format!("Invalid action: {e}"))?;
            let event = ScheduledEvent::new(name, trigger, action);
            let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
            scheduler.add_event(event.clone());
            scheduler.save(&state.schedule_path)?;
            Ok(text_result(&serde_json::to_string_pretty(&event).unwrap_or_default()))
        },
        "remove_scheduled_event" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
            if !scheduler.remove_event(id) {
                return Err(format!("No scheduled event {}", id));
            }
            scheduler.save(&state.schedule_path)?;
            Ok(text_result("Removed"))
        },
        "toggle_scheduled_event" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let mut scheduler = state.scheduler.lock().map_err(|e| e.to_string())?;
            if let Some(enabled) = scheduler.toggle_event(id) {
                scheduler.save(&state.schedule_path)?;
                Ok(text_result(if enabled { "Enabled" } else { "Disabled" }))
            } else {
                Err(format!("No scheduled event {}", id))
            }
        }
        "list_saved_playlists" => {
            let list = state.library.lock().map_err(|e| e.to_string())?.list_playlists()?;
            Ok(text_result(&serde_json::to_string_pretty(&list).unwrap_or_default()))
        },
        "save_playlist" => {
            let name = args["name"].as_str().ok_or("Missing name")?;
            let playlist = state.engine.lock().map_err(|e| e.to_string())?.playlist().clone();
            state.library.lock().map_err(|e| e.to_string())?.save_playlist(name, &playlist)?;
            Ok(text_result(&format!("Saved as {name}")))
        },
        "load_playlist" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let play = args.get("play").and_then(|v| v.as_bool()).unwrap_or(false);
            let playlist = state.library.lock().map_err(|e| e.to_string())?.load_playlist(id)?;
            state.engine.lock().map_err(|e| e.to_string())?.set_playlist(playlist.clone());
            if play {
                riars_core::play_index_unlocked(&state.engine, 0, false)?;
            }
            Ok(text_result(&format!("Loaded {} ({} tracks)", playlist.name, playlist.items.len())))
        },
        "delete_saved_playlist" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            state.library.lock().map_err(|e| e.to_string())?.delete_playlist(id)?;
            Ok(text_result("Deleted"))
        }
        "set_dead_air" => {
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            let mut config = engine.dead_air_config().clone();
            if let Some(enabled) = args["enabled"].as_bool() {
                config.enabled = enabled;
            }
            if let Some(db) = args["threshold_db"].as_f64() {
                config.threshold_db = db as f32;
            }
            if let Some(seconds) = args["seconds"].as_u64() {
                config.seconds = seconds.min(u32::MAX as u64) as u32;
            }
            if let Some(action) = args["action"].as_str() {
                config.action = match action {
                    "Skip" => DeadAirAction::Skip,
                    "Alert" => DeadAirAction::Alert,
                    other => return Err(format!("Unknown action: {other}")),
                };
            }
            config.validate()?;
            engine.set_dead_air(config);
            let c = engine.dead_air_config();
            Ok(text_result(&format!(
                "Dead air: {}, {} s under {} dBFS, {:?}",
                if c.enabled { "on" } else { "off" },
                c.seconds,
                c.threshold_db,
                c.action
            )))
        }
        _ => Err(format!("{}: {name}", t("unknown_tool"))),
    }
}

fn text_result(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}
