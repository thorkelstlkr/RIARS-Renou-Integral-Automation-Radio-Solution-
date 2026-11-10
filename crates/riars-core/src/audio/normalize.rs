//! Loudness normalisation: one fixed gain per track, from its measured loudness, so
//! songs, jingles and spots from different masters play at the same level.

use serde::{Deserialize, Serialize};

use crate::playlist::playlist::ItemKind;

/// Loudness a ReplayGain 2 gain of 0 dB stands for (LUFS).
const REPLAYGAIN_REFERENCE_LUFS: f32 = -18.0;

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
    /// Own target for spots (LUFS); `None` uses `target_lufs`.
    pub spot_target_lufs: Option<f32>,
    /// Own target for imaging: jingles, bumpers, station ids, sweepers and promos.
    pub imaging_target_lufs: Option<f32>,
    /// A track not analysed yet takes its loudness from its ReplayGain tags, if any.
    pub use_replaygain: bool,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            target_lufs: -16.0,
            max_gain_db: 10.0,
            max_cut_db: 12.0,
            peak_ceiling_dbfs: -1.0,
            spot_target_lufs: None,
            imaging_target_lufs: None,
            use_replaygain: true,
        }
    }
}

impl NormalizationConfig {
    pub fn validate(&self) -> Result<(), String> {
        let targets = [
            ("target_lufs", Some(self.target_lufs)),
            ("spot_target_lufs", self.spot_target_lufs),
            ("imaging_target_lufs", self.imaging_target_lufs),
        ];
        for (name, target) in targets {
            if target.is_some_and(|t| !(-30.0..=-5.0).contains(&t)) {
                return Err(format!("{name} must be -30 to -5"));
            }
        }
        if !(0.0..=20.0).contains(&self.max_gain_db) || !(0.0..=30.0).contains(&self.max_cut_db) {
            return Err("max_gain_db must be 0 to 20 and max_cut_db 0 to 30".to_string());
        }
        if !(-6.0..=0.0).contains(&self.peak_ceiling_dbfs) {
            return Err("peak_ceiling_dbfs must be -6 to 0".to_string());
        }
        Ok(())
    }

    /// The loudness items of this kind are brought to.
    pub fn target_for(&self, kind: ItemKind) -> f32 {
        let own = match kind {
            ItemKind::Spot => self.spot_target_lufs,
            ItemKind::Jingle | ItemKind::Bumper | ItemKind::StationId | ItemKind::Sweeper | ItemKind::Promo => {
                self.imaging_target_lufs
            }
            ItemKind::Music | ItemKind::Program | ItemKind::Bed | ItemKind::Other => None,
        };
        own.unwrap_or(self.target_lufs)
    }

    /// Gain (dB) for a track of this loudness and peak; 0 when off or not measured.
    pub fn gain_db(&self, loudness_lufs: Option<f32>, peak_dbfs: Option<f32>) -> f32 {
        self.gain_db_for(ItemKind::Music, loudness_lufs, peak_dbfs)
    }

    /// `gain_db` towards the target of this kind of item.
    pub fn gain_db_for(&self, kind: ItemKind, loudness_lufs: Option<f32>, peak_dbfs: Option<f32>) -> f32 {
        let Some(loudness) = loudness_lufs.filter(|_| self.enabled) else { return 0.0 };
        let mut gain = (self.target_for(kind) - loudness).clamp(-self.max_cut_db, self.max_gain_db);
        if gain > 0.0 {
            if let Some(peak) = peak_dbfs {
                // Raise only as far as the peak allows.
                gain = gain.min((self.peak_ceiling_dbfs - peak).max(0.0));
            }
        }
        gain
    }
}

/// Loudness (LUFS) and peak (dBFS) from ReplayGain tag values such as `-6.54 dB` and
/// `0.988`. `None` when the gain does not parse.
pub fn replaygain_loudness(gain: &str, peak: Option<&str>) -> Option<(f32, Option<f32>)> {
    let number = |text: &str| {
        let text = text.trim();
        let text = if text.len() >= 2 && text[text.len() - 2..].eq_ignore_ascii_case("db") { &text[..text.len() - 2] } else { text };
        text.trim().parse::<f32>().ok().filter(|v| v.is_finite())
    };
    let gain = number(gain).filter(|g| (-60.0..=60.0).contains(g))?;
    let peak = peak.and_then(number).filter(|p| *p > 0.0).map(|p| 20.0 * p.log10());
    Some((REPLAYGAIN_REFERENCE_LUFS - gain, peak))
}

/// The ReplayGain track tags of a file as loudness and peak. Reads the file: never
/// call it with the engine locked.
pub fn replaygain_of_file(path: &std::path::Path) -> Option<(f32, Option<f32>)> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(path).ok()?;
    tagged.tags().iter().find_map(|tag| {
        let gain = tag.get_string(&ItemKey::ReplayGainTrackGain)?;
        replaygain_loudness(gain, tag.get_string(&ItemKey::ReplayGainTrackPeak))
    })
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

    #[test]
    fn spots_and_imaging_take_their_own_target() {
        let n = NormalizationConfig { spot_target_lufs: Some(-14.0), ..NormalizationConfig::default() };
        assert_eq!(n.gain_db_for(ItemKind::Spot, Some(-20.0), None), 6.0);
        assert_eq!(n.gain_db_for(ItemKind::Music, Some(-20.0), None), 4.0);
        // No own target for imaging: the station target.
        assert_eq!(n.gain_db_for(ItemKind::Jingle, Some(-20.0), None), 4.0);
        let n = NormalizationConfig { imaging_target_lufs: Some(-15.0), ..n };
        assert_eq!(n.gain_db_for(ItemKind::StationId, Some(-20.0), None), 5.0);
        assert!(NormalizationConfig { spot_target_lufs: Some(-2.0), ..n }.validate().is_err());
        assert!(n.validate().is_ok());
    }

    #[test]
    fn replaygain_tags_become_loudness_and_peak() {
        assert_eq!(replaygain_loudness("-6.00 dB", Some("1.0")), Some((-12.0, Some(0.0))));
        assert_eq!(replaygain_loudness("+2.5 dB", None), Some((-20.5, None)));
        let (_, peak) = replaygain_loudness("0", Some("0.5")).unwrap();
        assert!((peak.unwrap() + 6.02).abs() < 0.01);
        assert_eq!(replaygain_loudness("loud", None), None);
        assert_eq!(replaygain_loudness("-3 dB", Some("0")), Some((-15.0, None)));
    }
}
