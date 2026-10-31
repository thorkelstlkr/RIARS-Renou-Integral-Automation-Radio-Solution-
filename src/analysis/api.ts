// Cue points, loudness, pre-listening and sound cards. Types mirror the Rust ones in
// crates/riars-core/src/analysis and src/audio (serde JSON). Times are milliseconds
// from the start of the file.
import { invoke } from "@tauri-apps/api/core";

export type CueSource = "Auto" | "Manual" | "Agent";

export interface TrackCues {
  track_id: number;
  duration_ms: number;
  cue_in_ms: number;
  cue_out_ms: number;
  /** Where the voice starts; null: unknown or no voice (nothing goes over the intro). */
  intro_end_ms: number | null;
  /** Where the voice ends; null as above. */
  outro_start_ms: number | null;
  source: CueSource;
  confidence: number;
  loudness_lufs: number | null;
  peak_dbfs: number | null;
  analyzed_at: string | null;
}

/** A fresh analysis, with the waveform for the editor. */
export interface CueAnalysis {
  duration_ms: number;
  cue_in_ms: number;
  cue_out_ms: number;
  intro_ms: number | null;
  outro_ms: number | null;
  confidence: number;
  loudness_lufs: number | null;
  peak_dbfs: number;
  /** Peak level per bucket, 0-255. */
  peaks: number[];
  /** How voice-like each second is, 0-9. */
  vocal_timeline: number[];
}

export type BatchScope =
  | "Library"
  | { Category: { category_id: number } }
  | { Tracks: { track_ids: number[] } };

export interface BatchStatus {
  running: boolean;
  done: number;
  total: number;
  failed: number;
  current: string | null;
  last_error: string | null;
  kept_manual: number;
}

export interface AnalysisStats {
  tracks: number;
  analyzed: number;
  manual: number;
  with_intro: number;
  with_outro: number;
  low_confidence: number;
}

export type CueDetector = "Builtin" | { Command: { program: string; args: string[] } };

export type FadeCurve = "Linear" | "EqualPower" | "SCurve";

export interface NormalizationConfig {
  enabled: boolean;
  target_lufs: number;
  max_gain_db: number;
  max_cut_db: number;
  peak_ceiling_dbfs: number;
}

export interface AudioDevices {
  /** Sound cards of this machine. */
  devices: string[];
  default: string | null;
  /** On-air card chosen in the settings (null: the system default). Takes effect after a restart. */
  main: string | null;
  /** Card currently in use on air. */
  main_in_use: string | null;
  /** Pre-listen card (null: pre-listening off). */
  preview: string | null;
}

export interface PreviewState {
  device: string | null;
  playing: boolean;
  file: string | null;
  position_ms: number;
  duration_ms: number;
}

/** Tauri commands (src-tauri/src/analysis_cmds.rs). Errors reject with a message. */
export const analysis = {
  trackCues: (trackId: number) => invoke<TrackCues | null>("analysis_track_cues", { trackId }),
  /** Cues of several tracks (unknown or never analysed ones are left out), at most 1000. */
  tracksCues: (trackIds: number[]) => invoke<TrackCues[]>("analysis_tracks_cues", { trackIds }),
  /** Cues by file path, for playlist items without a track id; at most 1000. */
  pathsCues: (paths: string[]) => invoke<{ path: string; cues: TrackCues }[]>("analysis_paths_cues", { paths }),
  /** Save hand-set marks (source Manual); resolves with what was stored. */
  setTrackCues: (cues: TrackCues) => invoke<TrackCues>("analysis_set_track_cues", { cues }),
  clearTrackCues: (trackId: number) => invoke<void>("analysis_clear_track_cues", { trackId }),
  /** Analyse now (seconds on a slow machine). With `save`, store the result (hand-set marks are kept). */
  analyzeTrack: (trackId: number, save: boolean) => invoke<CueAnalysis>("analysis_analyze_track", { trackId, save }),
  batchStart: (scope: BatchScope, reanalyze: boolean) => invoke<void>("analysis_batch_start", { scope, reanalyze }),
  batchCancel: () => invoke<void>("analysis_batch_cancel"),
  batchStatus: () => invoke<BatchStatus>("analysis_batch_status"),
  stats: () => invoke<AnalysisStats>("analysis_stats"),
  getDetector: () => invoke<CueDetector>("analysis_get_detector"),
  setDetector: (detector: CueDetector) => invoke<CueDetector>("analysis_set_detector", { detector }),
};

export const audio = {
  devices: () => invoke<AudioDevices>("audio_devices"),
  /** Save the on-air card (null: system default); used from the next start. */
  setMainDevice: (name: string | null) => invoke<AudioDevices>("audio_set_main_device", { name }),
  setPreviewDevice: (name: string | null) => invoke<AudioDevices>("audio_set_preview_device", { name }),
  getNormalization: () => invoke<NormalizationConfig>("normalization_get"),
  setNormalization: (config: NormalizationConfig) => invoke<NormalizationConfig>("normalization_set", { config }),
};

export const preview = {
  /** Pre-listen a library track from `fromMs`, at its normalised level. */
  play: (trackId: number, fromMs = 0) => invoke<PreviewState>("preview_play", { trackId, fromMs }),
  playPath: (path: string, fromMs: number) => invoke<PreviewState>("preview_play_path", { path, fromMs }),
  stop: () => invoke<PreviewState>("preview_stop"),
  state: () => invoke<PreviewState>("preview_state"),
};

