use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use riars_core::analysis::{
    analyze_track_detailed, BatchScope, CueDetector, CueSource, TrackCues,
};
use riars_core::audio::normalize::NormalizationConfig;
use riars_core::MediaLibrary;

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const DETECTOR_KEY: &str = "cue_detector";

const TOOL_NAMES: &[&str] = &[
    "get_track_cues",
    "set_track_cues",
    "analyze_track",
    "list_tracks_needing_review",
    "start_analysis",
    "analysis_status",
    "analysis_stats",
    "get_normalization",
    "set_normalization",
];

pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        def(
            "get_track_cues",
            "Get the cue points, loudness and who set them for a library track",
            json!({
                "type": "object",
                "properties": { "track_id": { "type": "integer" } },
                "required": ["track_id"]
            }),
        ),
        def(
            "set_track_cues",
            "Set a track's cue points from an analysis or your own listening. Marks are stored with source Agent, keeping the given confidence (default 0.9). Times are milliseconds from the start. intro_end_ms is where the voice starts and outro_start_ms where it ends; leave them null when there is no voice or it is unknown. cue_in_ms and cue_out_ms are the first and last audible moments. Cannot replace marks set by hand (source Manual); the operator must clear them first. Loudness and peak are saved when given.",
            json!({
                "type": "object",
                "properties": { "cues": cues_schema() },
                "required": ["cues"]
            }),
        ),
        def(
            "analyze_track",
            "Analyse a library track now without saving anything. Returns duration_s, cue_in_s, cue_out_s, intro_end_s (where the voice starts), outro_start_s (where it ends), confidence, loudness_lufs and vocal_timeline: one digit per second from the start, 0 = no voice and 9 = clearly voice. Use the timeline to find the first and last sung second, then call set_track_cues with intro_end_ms at the start of the voice and outro_start_ms where it ends (in milliseconds). If there is no voice, leave both null.",
            json!({
                "type": "object",
                "properties": { "track_id": { "type": "integer" } },
                "required": ["track_id"]
            }),
        ),
        def(
            "list_tracks_needing_review",
            "List automatically analysed tracks whose confidence is below the threshold, lowest first, for an agent to check with analyze_track and fix with set_track_cues. min_confidence defaults to 0.35 and limit to 20 (maximum 200).",
            json!({
                "type": "object",
                "properties": {
                    "min_confidence": { "type": "number", "minimum": 0, "maximum": 1, "description": "Default 0.35" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Default 20" }
                }
            }),
        ),
        def(
            "start_analysis",
            "Start analysing the library, a category or some tracks in the background (cue points and loudness). scope is \"Library\" (default), {\"Category\":{\"category_id\":1}} or {\"Tracks\":{\"track_ids\":[1,2]}}. reanalyze also redoes tracks already analysed; hand-set marks are always kept.",
            json!({
                "type": "object",
                "properties": {
                    "scope": {
                        "description": "\"Library\", {\"Category\":{\"category_id\":1}} or {\"Tracks\":{\"track_ids\":[1,2]}}; default Library",
                        "oneOf": [
                            { "type": "string", "enum": ["Library"] },
                            {
                                "type": "object",
                                "properties": {
                                    "Category": {
                                        "type": "object",
                                        "properties": { "category_id": { "type": "integer" } },
                                        "required": ["category_id"]
                                    }
                                },
                                "required": ["Category"]
                            },
                            {
                                "type": "object",
                                "properties": {
                                    "Tracks": {
                                        "type": "object",
                                        "properties": {
                                            "track_ids": { "type": "array", "items": { "type": "integer" } }
                                        },
                                        "required": ["track_ids"]
                                    }
                                },
                                "required": ["Tracks"]
                            }
                        ]
                    },
                    "reanalyze": { "type": "boolean", "description": "Redo tracks already analysed; default false" }
                }
            }),
        ),
        def(
            "analysis_status",
            "Get the background analysis progress: running, done, total, failed, current file and the tracks whose hand-set marks were kept",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "analysis_stats",
            "Get how the library stands: tracks, analysed, hand-set, with intro, with outro and low-confidence",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "get_normalization",
            "Get the loudness normalisation settings (enabled, target LUFS, gain limits and peak ceiling)",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "set_normalization",
            "Replace the loudness normalisation settings. Omitted fields keep their defaults: target_lufs -30 to -5, max_gain_db 0 to 20, max_cut_db 0 to 30, peak_ceiling_dbfs -6 to 0.",
            json!({
                "type": "object",
                "properties": { "config": normalization_schema() },
                "required": ["config"]
            }),
        ),
    ]
}

fn def(name: &'static str, description: &'static str, input_schema: Value) -> ToolDef {
    ToolDef {
        name,
        description,
        input_schema,
    }
}

fn cues_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "track_id": { "type": "integer" },
            "duration_ms": { "type": "integer", "minimum": 0, "description": "Length of the track in ms; 0 if unknown" },
            "cue_in_ms": { "type": "integer", "minimum": 0, "description": "First audible moment" },
            "cue_out_ms": { "type": "integer", "minimum": 0, "description": "Last audible moment" },
            "intro_end_ms": { "type": ["integer", "null"], "description": "Where the voice starts; null if unknown or no voice" },
            "outro_start_ms": { "type": ["integer", "null"], "description": "Where the voice ends; null as above" },
            "confidence": { "type": "number", "minimum": 0, "maximum": 1, "description": "0 to 1, default 0.9" },
            "loudness_lufs": { "type": ["number", "null"] },
            "peak_dbfs": { "type": ["number", "null"] }
        },
        "required": ["track_id", "cue_in_ms", "cue_out_ms"]
    })
}

fn normalization_schema() -> Value {
    json!({
        "type": "object",
        "description": "Full loudness normalisation settings; omitted fields take their defaults",
        "properties": {
            "enabled": { "type": "boolean" },
            "target_lufs": { "type": "number", "minimum": -30, "maximum": -5, "description": "Default -16 LUFS" },
            "max_gain_db": { "type": "number", "minimum": 0, "maximum": 20, "description": "Default 10 dB" },
            "max_cut_db": { "type": "number", "minimum": 0, "maximum": 30, "description": "Default 12 dB" },
            "peak_ceiling_dbfs": { "type": "number", "minimum": -6, "maximum": 0, "description": "Default -1 dBFS" }
        }
    })
}

fn detector_from_library(library: &MediaLibrary) -> Result<CueDetector, String> {
    let stored = library.kv_get(DETECTOR_KEY)?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub fn handle(name: &str, args: &Value, state: &McpState) -> Option<Result<Value, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    Some(dispatch(name, args, state))
}

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    match name {
        "get_track_cues" => {
            let track_id = args["track_id"].as_i64().ok_or("Missing track_id")?;
            let cues = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .track_cues(track_id)?;
            match cues {
                Some(cues) => json_compact(&cues),
                None => Err(format!("No cues for track {track_id}")),
            }
        }
        "set_track_cues" => {
            let mut value = args
                .get("cues")
                .cloned()
                .filter(Value::is_object)
                .ok_or("Missing cues object")?;
            if value.get("source").is_none() {
                value["source"] = json!("Agent");
            }
            if value.get("confidence").is_none() {
                value["confidence"] = json!(0.9);
            }
            if value.get("duration_ms").is_none() {
                value["duration_ms"] = json!(0);
            }
            let mut cues: TrackCues =
                serde_json::from_value(value).map_err(|e| format!("Invalid cues: {e}"))?;
            cues.source = CueSource::Agent;
            if !(0.0..=1.0).contains(&cues.confidence) {
                return Err("confidence must be between 0 and 1".to_string());
            }
            let track_id = cues.track_id;
            let stored = {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                library.set_track_cues(&cues)?;
                library.track_cues(track_id)?
            };
            match stored {
                Some(cues) => json_compact(&cues),
                None => Ok(text_result(&format!("Cues saved for track {track_id}"))),
            }
        }
        "analyze_track" => {
            let track_id = args["track_id"].as_i64().ok_or("Missing track_id")?;
            let (path, detector) = {
                let library = state.library.lock().map_err(|e| e.to_string())?;
                let track = library
                    .get_track(track_id)?
                    .ok_or_else(|| format!("No track {track_id}"))?;
                (track.file_path, detector_from_library(&library)?)
            };
            let path = std::path::Path::new(&path);
            let (analysis, _cues) = analyze_track_detailed(
                track_id,
                path,
                0,
                &detector,
                &AtomicBool::new(false),
            )?;
            let vocal_timeline: String = analysis
                .vocal_timeline
                .iter()
                .map(|value| char::from(b'0' + (*value).min(9)))
                .collect();
            json_compact(&json!({
                "duration_s": analysis.duration_ms as f32 / 1000.0,
                "cue_in_s": analysis.cue_in_ms as f32 / 1000.0,
                "cue_out_s": analysis.cue_out_ms as f32 / 1000.0,
                "intro_end_s": analysis.intro_ms.map(|ms| ms as f32 / 1000.0),
                "outro_start_s": analysis.outro_ms.map(|ms| ms as f32 / 1000.0),
                "confidence": analysis.confidence,
                "loudness_lufs": analysis.loudness_lufs,
                "vocal_timeline": vocal_timeline,
            }))
        }
        "list_tracks_needing_review" => {
            let min_confidence = args
                .get("min_confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.35) as f32;
            if !(0.0..=1.0).contains(&min_confidence) {
                return Err("min_confidence must be between 0 and 1".to_string());
            }
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20);
            if limit == 0 || limit > 200 {
                return Err("limit must be between 1 and 200".to_string());
            }
            let cues = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .tracks_needing_review(min_confidence, limit as usize)?;
            json_compact(&cues)
        }
        "start_analysis" => {
            let scope: BatchScope = match args.get("scope").filter(|value| !value.is_null()) {
                Some(value) => serde_json::from_value(value.clone())
                    .map_err(|e| format!("Invalid scope: {e}"))?,
                None => BatchScope::Library,
            };
            let reanalyze = args
                .get("reanalyze")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let detector = {
                let library = state.library.lock().map_err(|e| e.to_string())?;
                detector_from_library(&library)?
            };
            state.analysis().start(
                Arc::new(Arc::clone(&state.library)),
                scope,
                reanalyze,
                detector,
            )?;
            Ok(text_result("Analysis started"))
        }
        "analysis_status" => json_compact(&state.analysis().status()),
        "analysis_stats" => {
            let stats = {
                let library = state.library.lock().map_err(|e| e.to_string())?;
                let min_confidence = library.automation_config()?.talk_over.min_confidence;
                library.analysis_stats(min_confidence)?
            };
            json_compact(&stats)
        }
        "get_normalization" => {
            let config = state
                .engine
                .lock()
                .map_err(|e| e.to_string())?
                .normalization_config();
            json_compact(&config)
        }
        "set_normalization" => {
            let config: NormalizationConfig = serde_json::from_value(
                args.get("config")
                    .cloned()
                    .filter(Value::is_object)
                    .ok_or("Missing config object")?,
            )
            .map_err(|e| format!("Invalid config: {e}"))?;
            config.validate()?;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            engine.set_normalization(config);
            json_compact(&engine.normalization_config())
        }
        _ => Err(format!("Unknown analysis tool: {name}")),
    }
}

fn json_compact<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use riars_core::automation::Automation;
    use riars_core::library::db::MediaLibrary;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, Scheduler};

    use super::*;

    fn remove_db_files(path: &Path) {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let _ = std::fs::remove_file(PathBuf::from(shm));
    }

    fn test_state(name: &str) -> (McpState, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "riars-mcp-analysis-{}-{name}.db",
            std::process::id()
        ));
        remove_db_files(&path);
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let state = McpState {
            engine: Arc::new(Mutex::new(AudioEngine::new_headless().unwrap())),
            library: Arc::new(Mutex::new(library)),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            stream: None,
            schedule_path: path.with_extension("schedule.json"),
            session: Mutex::new(SessionSaver::new(path.with_extension("session.json"))),
            automation: Arc::new(Mutex::new(Automation::new())),
        };
        (state, path)
    }

    fn cleanup(state: McpState, path: &Path) {
        drop(state);
        remove_db_files(path);
        let _ = std::fs::remove_file(path.with_extension("schedule.json"));
        let _ = std::fs::remove_file(path.with_extension("session.json"));
    }

    fn call(state: &McpState, name: &str, args: Value) -> Result<Value, String> {
        handle(name, &args, state).unwrap_or_else(|| panic!("{name} is not an analysis tool"))
    }

    fn text_of(result: &Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn parse_text(result: &Value) -> Value {
        serde_json::from_str(&text_of(result)).unwrap()
    }

    fn insert_track(state: &McpState, dir: &Path, name: &str) -> i64 {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), b"").unwrap();
        let mut library = state.library.lock().unwrap();
        library.scan_folder(&dir.to_string_lossy()).unwrap();
        library
            .get_all_tracks()
            .unwrap()
            .into_iter()
            .find(|track| track.file_path.ends_with(name))
            .unwrap()
            .id
    }

    fn write_wav(path: &Path, samples: &[f32], rate: u32, channels: u16) {
        let data_size = (samples.len() * 2) as u32;
        let mut f = std::fs::File::create(path).expect("create wav");
        let mut w = |b: &[u8]| f.write_all(b).expect("write wav");
        w(b"RIFF");
        w(&(36 + data_size).to_le_bytes());
        w(b"WAVEfmt ");
        w(&16u32.to_le_bytes());
        w(&1u16.to_le_bytes());
        w(&channels.to_le_bytes());
        w(&rate.to_le_bytes());
        w(&(rate * channels as u32 * 2).to_le_bytes());
        w(&(channels * 2).to_le_bytes());
        w(&16u16.to_le_bytes());
        w(b"data");
        w(&data_size.to_le_bytes());
        for s in samples {
            w(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
    }

    #[test]
    fn tool_names_are_unique_and_do_not_clash() {
        let defs = tool_defs();
        let mut own = HashSet::new();
        for tool in &defs {
            assert!(own.insert(tool.name), "duplicate analysis tool {}", tool.name);
        }

        let all = serde_json::to_value(crate::tools::list_tools()).unwrap();
        let tools = all["tools"].as_array().unwrap();
        let mut names = HashSet::new();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(names.insert(name.to_string()), "duplicate tool {name}");
        }
        for name in own {
            assert!(names.contains(name), "{name} is missing from list_tools");
        }
    }

    #[test]
    fn set_track_cues_stores_agent_source_and_confidence() {
        let (state, path) = test_state("cues");
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-analysis-{}-cues-audio",
            std::process::id()
        ));
        let track = insert_track(&state, &dir, "a.mp3");

        let saved = call(
            &state,
            "set_track_cues",
            json!({
                "cues": {
                    "track_id": track,
                    "cue_in_ms": 500,
                    "cue_out_ms": 195_000,
                    "intro_end_ms": 12_500,
                    "outro_start_ms": 180_000,
                    "confidence": 0.7
                }
            }),
        )
        .unwrap();
        let saved = parse_text(&saved);
        assert_eq!(saved["source"], "Agent");
        assert_eq!(saved["confidence"].as_f64().unwrap(), 0.7);

        let stored = state
            .library
            .lock()
            .unwrap()
            .track_cues(track)
            .unwrap()
            .unwrap();
        assert_eq!(stored.source, CueSource::Agent);
        assert_eq!(stored.confidence, 0.7);
        assert_eq!(stored.intro_end_ms, Some(12_500));

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_track_cues_cannot_replace_manual_marks() {
        let (state, path) = test_state("manual");
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-analysis-{}-manual-audio",
            std::process::id()
        ));
        let track = insert_track(&state, &dir, "a.mp3");
        {
            let mut library = state.library.lock().unwrap();
            let manual = TrackCues {
                track_id: track,
                duration_ms: 0,
                cue_in_ms: 500,
                cue_out_ms: 195_000,
                intro_end_ms: None,
                outro_start_ms: None,
                source: CueSource::Manual,
                confidence: 1.0,
                loudness_lufs: None,
                peak_dbfs: None,
                analyzed_at: None,
            };
            library.set_track_cues(&manual).unwrap();
        }

        let err = call(
            &state,
            "set_track_cues",
            json!({
                "cues": {
                    "track_id": track,
                    "cue_in_ms": 1_000,
                    "cue_out_ms": 100_000
                }
            }),
        )
        .unwrap_err();
        assert!(err.contains("hand-set"));

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_track_cues_rejects_marks_past_the_stored_duration() {
        let (state, path) = test_state("past-duration");
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-analysis-{}-past-duration-audio",
            std::process::id()
        ));
        let track = insert_track(&state, &dir, "a.mp3");
        {
            let mut library = state.library.lock().unwrap();
            let analyzed = TrackCues {
                track_id: track,
                duration_ms: 200_000,
                cue_in_ms: 0,
                cue_out_ms: 200_000,
                intro_end_ms: None,
                outro_start_ms: None,
                source: CueSource::Auto,
                confidence: 0.5,
                loudness_lufs: None,
                peak_dbfs: None,
                analyzed_at: None,
            };
            library.save_analysis(&analyzed).unwrap();
        }

        let err = call(
            &state,
            "set_track_cues",
            json!({
                "cues": {
                    "track_id": track,
                    "cue_in_ms": 0,
                    "cue_out_ms": 999_999_999
                }
            }),
        )
        .unwrap_err();
        assert!(!err.is_empty());

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_tracks_needing_review_rejects_bad_limit() {
        let (state, path) = test_state("review-limit");
        let err = call(
            &state,
            "list_tracks_needing_review",
            json!({ "limit": 500 }),
        )
        .unwrap_err();
        assert!(err.contains("limit"));

        cleanup(state, &path);
    }

    #[test]
    fn analyze_track_returns_a_vocal_timeline() {
        let (state, path) = test_state("analyze");
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-analysis-{}-analyze-audio",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 44_100;
        let samples: Vec<f32> = (0..rate * 3)
            .map(|i| 0.3 * (i as f32 * 0.0628).sin())
            .collect();
        write_wav(&dir.join("tone.wav"), &samples, rate, 1);
        let track = {
            let mut library = state.library.lock().unwrap();
            library.scan_folder(&dir.to_string_lossy()).unwrap();
            library.get_all_tracks().unwrap()[0].id
        };

        let result = call(&state, "analyze_track", json!({ "track_id": track })).unwrap();
        let value = parse_text(&result);
        assert!(value["duration_s"].as_f64().unwrap() > 2.0);
        let timeline = value["vocal_timeline"].as_str().unwrap();
        assert!(!timeline.is_empty());
        assert!(timeline.chars().all(|c| c.is_ascii_digit()));
        assert!(timeline.len() >= 3);
        assert!(value["confidence"].is_number());
        assert!(value["cue_in_s"].is_number());

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
