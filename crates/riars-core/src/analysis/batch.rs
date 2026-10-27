//! Analysing the library in the background: cue points and loudness of every track,
//! one at a time on its own thread, pausing between tracks so playback and the rest of
//! the machine keep their CPU (it matters on a Raspberry Pi).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::cues::{analyze_file, CueAnalysis};
use super::model::TrackCues;
use crate::library::db::MediaLibrary;

/// Pause between two tracks.
const PAUSE: Duration = Duration::from_millis(200);

/// Which tracks to analyse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BatchScope {
    Library,
    Category { category_id: i64 },
    Tracks { track_ids: Vec<i64> },
}

/// Where the marks come from. The loudness is always measured by RIARS.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum CueDetector {
    #[default]
    Builtin,
    /// An external program (e.g. a voice-separation model) run as
    /// `program args... <file>`, printing `{"intro_ms": .., "outro_ms": .., "confidence": ..}`
    /// (each optional) on standard output. Its marks replace the built-in ones.
    Command { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BatchStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
    /// File being analysed now.
    pub current: Option<String>,
    pub last_error: Option<String>,
    /// Tracks whose hand-set marks were kept (only their loudness was updated).
    pub kept_manual: usize,
}

/// The library as the analyser reaches it, a moment at a time.
pub trait LibraryAccess: Send + Sync + 'static {
    fn with_library(&self, f: &mut dyn FnMut(&mut MediaLibrary) -> Result<(), String>) -> Result<(), String>;
}

impl LibraryAccess for Arc<Mutex<MediaLibrary>> {
    fn with_library(&self, f: &mut dyn FnMut(&mut MediaLibrary) -> Result<(), String>) -> Result<(), String> {
        f(&mut *self.lock().map_err(|e| e.to_string())?)
    }
}

#[derive(Default)]
pub struct BatchAnalyzer {
    status: Arc<Mutex<BatchStatus>>,
    cancel: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct CommandMarks {
    intro_ms: Option<u64>,
    outro_ms: Option<u64>,
    confidence: Option<f32>,
}

/// Longest an external detector may take on one file.
const DETECTOR_TIMEOUT: Duration = Duration::from_secs(120);

/// Run the external detector on `path`; its marks, or why not. It is killed when it
/// takes longer than [`DETECTOR_TIMEOUT`] or when `cancel` is set.
fn run_command(program: &str, args: &[String], path: &Path, cancel: &AtomicBool) -> Result<CommandMarks, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = std::process::Command::new(program)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run {program}: {e}"))?;
    // Read both pipes on their own threads so a chatty program never blocks on a full pipe.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        let why = if cancel.load(Ordering::Relaxed) {
            Some("cancelled".to_string())
        } else if started.elapsed() > DETECTOR_TIMEOUT {
            Some(format!("took more than {} s", DETECTOR_TIMEOUT.as_secs()))
        } else {
            None
        };
        if let Some(why) = why {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{program} {why}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("{program} failed: {}", String::from_utf8_lossy(&stderr).trim()));
    }
    serde_json::from_slice(&stdout).map_err(|e| format!("{program} printed no valid JSON: {e}"))
}

/// Analyse one file: the built-in analysis, with the detector's marks if it has one.
pub fn analyze_track(track_id: i64, path: &Path, detector: &CueDetector) -> Result<TrackCues, String> {
    analyze_track_detailed(track_id, path, 0, detector, &AtomicBool::new(false)).map(|(_, cues)| cues)
}

/// [`analyze_track`] that also returns the analysis itself, with a waveform of `buckets`
/// columns, decoding the file once. `cancel` stops an external detector.
pub fn analyze_track_detailed(
    track_id: i64,
    path: &Path,
    buckets: usize,
    detector: &CueDetector,
    cancel: &AtomicBool,
) -> Result<(CueAnalysis, TrackCues), String> {
    let mut analysis = analyze_file(path, buckets)?;
    let mut cues = TrackCues::from_analysis(track_id, &analysis, chrono::Local::now().naive_local());
    if let CueDetector::Command { program, args } = detector {
        let marks = run_command(program, args, path, cancel)?;
        cues.intro_end_ms = marks.intro_ms.map(|ms| ms.clamp(cues.cue_in_ms, cues.cue_out_ms));
        cues.outro_start_ms = marks.outro_ms.map(|ms| ms.clamp(cues.intro_end_ms.unwrap_or(cues.cue_in_ms), cues.cue_out_ms));
        cues.confidence = marks.confidence.unwrap_or(0.9).clamp(0.0, 1.0);
        analysis.intro_ms = cues.intro_end_ms;
        analysis.outro_ms = cues.outro_start_ms;
        analysis.confidence = cues.confidence;
    }
    cues.validate()?;
    Ok((analysis, cues))
}

impl BatchAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> BatchStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Stop after the track being analysed now.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Start analysing `scope` on a background thread. `reanalyze` also redoes tracks
    /// analysed before (hand-set marks are kept anyway). Fails if a run is going on.
    pub fn start(&self, library: Arc<dyn LibraryAccess>, scope: BatchScope, reanalyze: bool, detector: CueDetector) -> Result<(), String> {
        {
            let mut status = self.status.lock().map_err(|e| e.to_string())?;
            if status.running {
                return Err("An analysis is already running".to_string());
            }
            *status = BatchStatus { running: true, ..Default::default() };
        }
        self.cancel.store(false, Ordering::Relaxed);
        let mut tracks = Vec::new();
        let listed = library.with_library(&mut |lib| {
            tracks = match &scope {
                BatchScope::Library => lib.tracks_to_analyze(None, reanalyze, usize::MAX)?,
                BatchScope::Category { category_id } => lib.tracks_to_analyze(Some(*category_id), reanalyze, usize::MAX)?,
                BatchScope::Tracks { track_ids } => {
                    let mut out = Vec::new();
                    for id in track_ids.iter().take(10_000) {
                        if let Some(t) = lib.get_track(*id)? {
                            out.push((t.id, t.file_path));
                        }
                    }
                    out
                }
            };
            Ok(())
        });
        if let Err(e) = listed {
            if let Ok(mut status) = self.status.lock() {
                status.running = false;
                status.last_error = Some(e.clone());
            }
            return Err(e);
        }
        if let Ok(mut status) = self.status.lock() {
            status.total = tracks.len();
        }
        let (status, cancel) = (Arc::clone(&self.status), Arc::clone(&self.cancel));
        let spawned = std::thread::Builder::new()
            .name("riars-analysis".into())
            .spawn(move || {
                for (n, (id, path)) in tracks.into_iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if n > 0 {
                        std::thread::sleep(PAUSE);
                    }
                    if let Ok(mut s) = status.lock() {
                        s.current = Some(path.clone());
                    }
                    // Decoding and analysis without any lock; the library only to save.
                    let result = analyze_track_detailed(id, Path::new(&path), 0, &detector, &cancel).and_then(|(_, cues)| {
                        let mut wrote = true;
                        library.with_library(&mut |lib| {
                            wrote = lib.save_analysis(&cues)?;
                            Ok(())
                        })?;
                        Ok(wrote)
                    });
                    if let Ok(mut s) = status.lock() {
                        s.done += 1;
                        match result {
                            Ok(true) => {}
                            Ok(false) => s.kept_manual += 1,
                            Err(e) => {
                                log::warn!("Analysis of {path} failed: {e}");
                                s.failed += 1;
                                s.last_error = Some(format!("{path}: {e}"));
                            }
                        }
                    }
                }
                if let Ok(mut s) = status.lock() {
                    s.running = false;
                    s.current = None;
                }
            });
        if let Err(e) = spawned {
            if let Ok(mut s) = self.status.lock() {
                s.running = false;
                s.last_error = Some(e.to_string());
            }
            return Err(e.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 16-bit PCM WAV (drafted by the local model; header field sizes fixed by Claude).
    fn write_wav(path: &Path, samples: &[f32], rate: u32, channels: u16) {
        let data_size = (samples.len() * 2) as u32;
        let mut f = std::fs::File::create(path).expect("create wav");
        let mut w = |b: &[u8]| f.write_all(b).expect("write wav");
        w(b"RIFF");
        w(&(36 + data_size).to_le_bytes());
        w(b"WAVEfmt ");
        w(&16u32.to_le_bytes());
        w(&1u16.to_le_bytes()); // PCM
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
    fn analyses_the_library_and_keeps_manual_marks() {
        let dir = std::env::temp_dir().join(format!("riars-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lib.db");
        let mut lib = MediaLibrary::open(&db.to_string_lossy()).unwrap();
        for (n, amp) in [0.05f32, 0.4, 0.2].iter().enumerate() {
            let samples: Vec<f32> = (0..44_100 * 4).map(|i| amp * (i as f32 * 0.0628).sin()).collect();
            write_wav(&dir.join(format!("t{n}.wav")), &samples, 44_100, 1);
        }
        lib.scan_folder(&dir.to_string_lossy()).unwrap();
        // Track 1 already has hand-set marks.
        let first = lib.tracks_to_analyze(None, true, 10).unwrap()[0].0;
        let manual = TrackCues {
            track_id: first,
            duration_ms: 4_000,
            cue_in_ms: 100,
            cue_out_ms: 3_900,
            intro_end_ms: Some(1_000),
            outro_start_ms: Some(3_000),
            source: crate::analysis::CueSource::Manual,
            confidence: 1.0,
            loudness_lufs: None,
            peak_dbfs: None,
            analyzed_at: None,
        };
        lib.set_track_cues(&manual).unwrap();
        let lib = Arc::new(Mutex::new(lib));
        let batch = BatchAnalyzer::new();
        batch.start(Arc::new(Arc::clone(&lib)), BatchScope::Library, true, CueDetector::Builtin).unwrap();
        assert!(batch.start(Arc::new(Arc::clone(&lib)), BatchScope::Library, true, CueDetector::Builtin).is_err());
        let mut status = batch.status();
        for _ in 0..200 {
            if !status.running {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
            status = batch.status();
        }
        assert_eq!((status.running, status.done, status.total, status.failed, status.kept_manual), (false, 3, 3, 0, 1), "{status:?}");
        let lib = lib.lock().unwrap();
        let kept = lib.track_cues(first).unwrap().unwrap();
        assert_eq!((kept.intro_end_ms, kept.source), (Some(1_000), crate::analysis::CueSource::Manual));
        assert!(kept.loudness_lufs.is_some(), "loudness still measured");
        // The three sine levels give three loudness values, louder for the louder file.
        let mut loud: Vec<f32> = lib
            .tracks_to_analyze(None, true, 10)
            .unwrap()
            .iter()
            .map(|(id, _)| lib.track_cues(*id).unwrap().map(|c| c.loudness_lufs.unwrap()).unwrap_or(f32::NAN))
            .collect();
        loud.sort_by(|a, b| a.total_cmp(b));
        assert!(loud[2] - loud[0] > 15.0, "{loud:?}");
        drop(lib);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_detector_is_killed() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("powershell", vec!["-NoProfile".into(), "-Command".into(), "Start-Sleep 30;".into()])
        } else {
            ("sleep", vec!["30".into()])
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        let result = run_command(program, &args, Path::new("x.wav"), &cancel);
        stopper.join().unwrap();
        assert!(result.err().is_some_and(|e| e.contains("cancelled")));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
