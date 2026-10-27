//! What the library keeps about a track's cue points and loudness.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::cues::CueAnalysis;

/// Who set a track's marks. Manual marks are never overwritten by an analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CueSource {
    /// The built-in analysis (or an external detector command).
    Auto,
    /// Set by hand in the editor.
    Manual,
    /// Set by an AI agent (MCP), after looking at the analysis.
    Agent,
}

/// Cue points and loudness of a library track. Times are from the start of the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackCues {
    pub track_id: i64,
    pub duration_ms: u64,
    /// First and last audible moments.
    pub cue_in_ms: u64,
    pub cue_out_ms: u64,
    /// Where the voice starts (end of the instrumental intro). `None`: unknown, or no
    /// voice found: nothing may be played over the intro.
    pub intro_end_ms: Option<u64>,
    /// Where the voice ends (start of the instrumental outro). `None`: as above.
    pub outro_start_ms: Option<u64>,
    pub source: CueSource,
    /// 0 to 1, for automatic marks; 1 for manual ones.
    pub confidence: f32,
    pub loudness_lufs: Option<f32>,
    pub peak_dbfs: Option<f32>,
    pub analyzed_at: Option<NaiveDateTime>,
}

impl TrackCues {
    pub fn from_analysis(track_id: i64, a: &CueAnalysis, at: NaiveDateTime) -> Self {
        Self {
            track_id,
            duration_ms: a.duration_ms,
            cue_in_ms: a.cue_in_ms,
            cue_out_ms: a.cue_out_ms,
            intro_end_ms: a.intro_ms,
            outro_start_ms: a.outro_ms,
            source: CueSource::Auto,
            confidence: a.confidence,
            loudness_lufs: a.loudness_lufs,
            peak_dbfs: Some(a.peak_dbfs),
            analyzed_at: Some(at),
        }
    }

    /// Instrumental time at the start that can be talked over (0 when unknown).
    pub fn intro_room_ms(&self) -> u64 {
        self.intro_end_ms.map_or(0, |end| end.saturating_sub(self.cue_in_ms))
    }

    /// Instrumental time at the end that can be talked over (0 when unknown).
    pub fn outro_room_ms(&self) -> u64 {
        self.outro_start_ms.map_or(0, |start| self.cue_out_ms.saturating_sub(start))
    }

    /// Marks in order: cue in ≤ intro end ≤ outro start ≤ cue out ≤ duration.
    pub fn validate(&self) -> Result<(), String> {
        let intro = self.intro_end_ms.unwrap_or(self.cue_in_ms);
        let outro = self.outro_start_ms.unwrap_or(self.cue_out_ms);
        let ordered = self.cue_in_ms <= intro && intro <= outro && outro <= self.cue_out_ms;
        if !ordered {
            return Err("Marks must be in order: start ≤ intro end ≤ outro start ≤ end".to_string());
        }
        if self.duration_ms > 0 && self.cue_out_ms > self.duration_ms {
            return Err(format!("The end mark is past the end of the track ({} ms)", self.duration_ms));
        }
        Ok(())
    }
}

/// How the whole library stands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalysisStats {
    pub tracks: usize,
    pub analyzed: usize,
    pub manual: usize,
    pub with_intro: usize,
    pub with_outro: usize,
    /// Automatic marks below the confidence the generator trusts.
    pub low_confidence: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cues(cue_in: u64, intro: Option<u64>, outro: Option<u64>, cue_out: u64) -> TrackCues {
        TrackCues {
            track_id: 1,
            duration_ms: 200_000,
            cue_in_ms: cue_in,
            cue_out_ms: cue_out,
            intro_end_ms: intro,
            outro_start_ms: outro,
            source: CueSource::Manual,
            confidence: 1.0,
            loudness_lufs: None,
            peak_dbfs: None,
            analyzed_at: None,
        }
    }

    #[test]
    fn rooms_and_order() {
        let c = cues(500, Some(12_500), Some(180_000), 195_000);
        assert_eq!((c.intro_room_ms(), c.outro_room_ms()), (12_000, 15_000));
        assert!(c.validate().is_ok());
        assert_eq!(cues(0, None, None, 190_000).intro_room_ms(), 0);
        assert!(cues(5_000, Some(4_000), None, 190_000).validate().is_err());
        assert!(cues(0, None, None, 250_000).validate().is_err());
    }
}
