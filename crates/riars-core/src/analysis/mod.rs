//! Offline audio analysis (never on the audio thread): where a track really starts and
//! ends, and where its vocals start and stop, so imaging can play over the intro or
//! outro without covering a voice.

pub mod batch;
pub mod cues;
pub mod loudness;
pub mod model;
pub mod store;

pub use cues::{analyze_file, analyze_samples, CueAnalysis};
pub use loudness::LoudnessMeter;
pub use model::{AnalysisStats, CueSource, TrackCues};
pub use batch::{analyze_track, BatchAnalyzer, BatchScope, BatchStatus, CueDetector, LibraryAccess};
