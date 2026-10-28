//! Loudness normalisation: one fixed gain per track, from its measured loudness, so
//! songs, jingles and spots from different masters play at the same level.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NormalizationConfig {
    pub enabled: bool,
    /// Loudness every track is brought to (LUFS).
    pub target_lufs: f32,
    /// Most a quiet track is raised (dB).
    pub max_gain_db: f32,
    /// Most a loud track is lowered (dB).
    pub max_cut_db: f32,
    /// A raised track's peak stays under this (dBFS); the output limiter catches the rest.
    pub peak_ceiling_dbfs: f32,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        Self { enabled: true, target_lufs: -16.0, max_gain_db: 10.0, max_cut_db: 12.0, peak_ceiling_dbfs: -1.0 }
    }
}

impl NormalizationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(-30.0..=-5.0).contains(&self.target_lufs) {
            return Err("target_lufs must be -30 to -5".to_string());
        }
        if !(0.0..=20.0).contains(&self.max_gain_db) || !(0.0..=30.0).contains(&self.max_cut_db) {
            return Err("max_gain_db must be 0 to 20 and max_cut_db 0 to 30".to_string());
        }
        if !(-6.0..=0.0).contains(&self.peak_ceiling_dbfs) {
            return Err("peak_ceiling_dbfs must be -6 to 0".to_string());
        }
        Ok(())
    }

    /// Gain (dB) for a track of this loudness and peak; 0 when off or not measured.
    pub fn gain_db(&self, loudness_lufs: Option<f32>, peak_dbfs: Option<f32>) -> f32 {
        let Some(loudness) = loudness_lufs.filter(|_| self.enabled) else { return 0.0 };
        let mut gain = (self.target_lufs - loudness).clamp(-self.max_cut_db, self.max_gain_db);
        if gain > 0.0 {
            if let Some(peak) = peak_dbfs {
                // Raise only as far as the peak allows.
                gain = gain.min((self.peak_ceiling_dbfs - peak).max(0.0));
            }
        }
        gain
    }
}

/// dB to a linear factor.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gains() {
        let n = NormalizationConfig::default();
        assert_eq!(n.gain_db(Some(-10.0), Some(0.0)), -6.0); // loud master: lowered
        assert_eq!(n.gain_db(Some(-22.0), Some(-12.0)), 6.0); // quiet: raised
        assert_eq!(n.gain_db(Some(-22.0), Some(-3.0)), 2.0); // raised only to the ceiling
        assert_eq!(n.gain_db(Some(-40.0), Some(-30.0)), 10.0); // capped
        assert_eq!(n.gain_db(Some(0.0), Some(0.0)), -12.0);
        assert_eq!(n.gain_db(None, None), 0.0); // not measured
        assert_eq!(NormalizationConfig { enabled: false, ..n }.gain_db(Some(-10.0), None), 0.0);
        assert!((db_to_gain(-6.0) - 0.501).abs() < 0.001);
    }
}
