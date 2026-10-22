use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeCurve {
    Linear,
    EqualPower,
    SCurve,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossfadeConfig {
    pub duration: Duration,
    pub curve: FadeCurve,
    pub enabled: bool,
}

impl Default for CrossfadeConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(3),
            curve: FadeCurve::EqualPower,
            enabled: true,
        }
    }
}

impl CrossfadeConfig {
    pub fn gain_at(&self, progress: f32) -> (f32, f32) {
        let t = progress.clamp(0.0, 1.0);
        match self.curve {
            FadeCurve::Linear => (1.0 - t, t),
            FadeCurve::EqualPower => {
                let angle = t * std::f32::consts::FRAC_PI_2;
                (angle.cos(), angle.sin())
            }
            FadeCurve::SCurve => {
                let s = t * t * (3.0 - 2.0 * t);
                (1.0 - s, s)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_boundaries() {
        let config = CrossfadeConfig {
            curve: FadeCurve::Linear,
            ..Default::default()
        };
        let (a, b) = config.gain_at(0.0);
        assert!((a - 1.0).abs() < 1e-6);
        assert!(b.abs() < 1e-6);

        let (a, b) = config.gain_at(1.0);
        assert!(a.abs() < 1e-6);
        assert!((b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn equal_power_preserves_energy() {
        let config = CrossfadeConfig::default();
        let (a, b) = config.gain_at(0.5);
        assert!((a - b).abs() < 1e-5);
        assert!((a * a + b * b - 1.0).abs() < 1e-5);
    }
}
