//! RDS (see `riars_rds`): read and change the settings, external encoders and the
//! built-in FM stereo encoder, and put a RadioText of the agent's own on air for a while.

use riars_core::rds::RdsConfig;
use serde_json::{json, Value};

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &["get_rds", "set_rds_config", "set_rds_text", "clear_rds_text", "list_mpx_devices"];

pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_rds",
            description: "Get the RDS settings (PI, PS, PTY, TP, music flag, RadioText template and texts, encoder outputs, built-in FM stereo encoder) and status: the PS and RadioText being sent, with RT+ tags, the manual text if any, each output's state and last error, and the FM encoder's state (sound card, sample rate, underruns, last error).",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "set_rds_config",
            description: "Replace the RDS settings. Read them with get_rds first, change what you need and send the whole object back. pi: 4 hex digits from the licence; ps: up to 8 characters, fixed; pty 0-31; rt_template uses {artist} and {title}; station_text is shown for jingles, spots and silence; live_text while the microphone is on. outputs: [{name, enabled, transport: {type: Tcp|Udp, host, port} | {type: Serial, port, baud} | {type: File, path (absolute .txt)}, protocol: {type: Uecp, site, encoder} | {type: Ascii, template, charset: Rds|Ascii|Utf8}}]. mpx is the built-in FM stereo encoder, for a transmitter's MPX input: {enabled, device (sound card name from list_mpx_devices, empty for the default), pre_emphasis: off|50us|75us (50us in Europe, 75us in the Americas), stereo, rds, pilot_percent 6-12 (9 usual), rds_percent 1-10 (4 usual), output_db -40..0}. Ask the operator before changing PI, PS, the outputs or mpx: they are what goes on air.",
            input_schema: json!({
                "type": "object",
                "properties": { "config": { "type": "object", "description": "The whole RDS settings, as get_rds returns them" } },
                "required": ["config"]
            }),
        },
        ToolDef {
            name: "set_rds_text",
            description: "Show a RadioText of your own (up to 64 characters) instead of the song's, for a number of minutes or until clear_rds_text. For announcements: a phone number, the next programme, a contest.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "maxLength": 64 },
                    "minutes": { "type": "integer", "minimum": 1, "maximum": 1440, "description": "How long it stays; leave out to keep it until cleared" }
                },
                "required": ["text"]
            }),
        },
        ToolDef {
            name: "list_mpx_devices",
            description: "List the sound cards that can play the FM stereo (MPX) signal: they must run at four times the on-air rate (192 kHz for 48 kHz audio).",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "clear_rds_text",
            description: "Go back to the automatic RadioText (the song on air, or the station text).",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub fn handle(name: &str, args: &Value, state: &McpState) -> Option<Result<Value, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    Some(dispatch(name, args, state))
}

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    let rds = &state.rds;
    match name {
        "get_rds" => {}
        "set_rds_config" => {
            let config: RdsConfig =
                serde_json::from_value(args.get("config").cloned().unwrap_or(Value::Null)).map_err(|e| format!("Bad config: {e}"))?;
            rds.set_config(config)?;
        }
        "set_rds_text" => {
            let text = args["text"].as_str().ok_or("Missing text")?;
            let minutes = match args.get("minutes") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().and_then(|m| u32::try_from(m).ok()).ok_or("minutes must be a whole number")?),
            };
            rds.set_text(Some(text), minutes)?;
        }
        "clear_rds_text" => rds.set_text(None, None)?,
        "list_mpx_devices" => return Ok(text_result(&json!(rds.mpx_devices()).to_string())),
        _ => return Err(format!("Unknown RDS tool: {name}")),
    }
    let body = json!({ "config": rds.config(), "status": rds.status() });
    Ok(text_result(&body.to_string()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn test_state() -> McpState {
        let base = std::env::temp_dir().join(format!("riars-mcp-rds-{}", std::process::id()));
        McpState {
            engine: Arc::new(Mutex::new(AudioEngine::new_headless().unwrap())),
            library: Arc::new(Mutex::new(MediaLibrary::open(":memory:").unwrap())),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            stream: None,
            rds: riars_rds::RdsService::new(None),
            schedule_path: base.with_extension("schedule.json"),
            session: Mutex::new(SessionSaver::new(base.with_extension("session.json"))),
            automation: Arc::new(Mutex::new(Automation::new())),
        }
    }

    fn call(state: &McpState, name: &str, args: Value) -> Result<Value, String> {
        let result = handle(name, &args, state).unwrap()?;
        Ok(serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap())
    }

    #[test]
    fn an_agent_reads_changes_and_announces() {
        let state = test_state();
        let mut body = call(&state, "get_rds", json!({})).unwrap();
        assert_eq!(body["config"]["ps"], "RIARS");
        body["config"]["ps"] = json!("RENOU");
        body["config"]["station_text"] = json!("Radio Renou 98.5");
        let body = call(&state, "set_rds_config", json!({ "config": body["config"] })).unwrap();
        assert_eq!(body["status"]["texts"]["ps"], "RENOU   ");
        assert_eq!(body["status"]["texts"]["rt"], "Radio Renou 98.5");

        let body = call(&state, "set_rds_text", json!({ "text": "Concurso: llama al 900", "minutes": 15 })).unwrap();
        assert_eq!(body["status"]["texts"]["rt"], "Concurso: llama al 900");
        assert!(body["status"]["manual_until"].is_u64());
        let body = call(&state, "clear_rds_text", json!({})).unwrap();
        assert_eq!(body["status"]["texts"]["rt"], "Radio Renou 98.5");

        assert!(call(&state, "set_rds_config", json!({ "config": { "pi": "XYZ" } })).is_err());
        assert!(call(&state, "set_rds_text", json!({ "text": "hola", "minutes": -1 })).is_err());
        assert!(handle("get_status", &json!({}), &state).is_none());
    }
}
