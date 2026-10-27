//! Integrated loudness after ITU-R BS.1770 / EBU R128: K-weighting, 400 ms blocks
//! every 100 ms, absolute gate at -70 LUFS and relative gate 10 LU below.
//! Streaming: feed interleaved samples at the file's own rate as they are decoded.

/// One biquad section (direct form I).
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn process(&mut self, x0: f64) -> f64 {
        let y0 = self.b[0] * x0 + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x0, self.x[0]];
        self.y = [y0, self.y[0]];
        y0
    }
}

/// The two K-weighting stages for `rate`, from their analogue prototypes (the BS.1770
/// tables only give 48 kHz coefficients).
fn k_weighting(rate: f64) -> [Biquad; 2] {
    let normalise = |b: [f64; 3], a: [f64; 3]| Biquad {
        b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]],
        a: [a[1] / a[0], a[2] / a[0]],
        ..Default::default()
    };
    // Stage 1: high shelf, +4 dB above about 1.5 kHz (the head's effect). These RBJ
    // cookbook parameters match the BS.1770 48 kHz coefficients (as in pyloudnorm).
    let (gain_db, q, fc) = (4.0, std::f64::consts::FRAC_1_SQRT_2, 1_500.0);
    let a_ = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / rate;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let s = 2.0 * a_.sqrt() * alpha;
    let shelf = normalise(
        [
            a_ * ((a_ + 1.0) + (a_ - 1.0) * cos + s),
            -2.0 * a_ * ((a_ - 1.0) + (a_ + 1.0) * cos),
            a_ * ((a_ + 1.0) + (a_ - 1.0) * cos - s),
        ],
        [(a_ + 1.0) - (a_ - 1.0) * cos + s, 2.0 * ((a_ - 1.0) - (a_ + 1.0) * cos), (a_ + 1.0) - (a_ - 1.0) * cos - s],
    );
    // Stage 2: high pass at about 38 Hz (RLB weighting).
    let (q, fc) = (0.5, 38.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / rate;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let high_pass = normalise([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]);
    [shelf, high_pass]
}

pub struct LoudnessMeter {
    filters: Vec<[Biquad; 2]>,
    /// Weighted squares of the current 100 ms step, summed over channels.
    step_sum: f64,
    step_len: usize,
    step_frames: usize,
    /// Mean square of each 100 ms step, for 400 ms blocks with 75 % overlap.
    steps: Vec<f64>,
    peak: f32,
}

impl LoudnessMeter {
    pub fn new(rate: u32, channels: usize) -> Self {
        Self {
            filters: vec![k_weighting(rate as f64); channels.max(1)],
            step_sum: 0.0,
            step_len: (rate as usize / 10).max(1),
            step_frames: 0,
            steps: Vec::new(),
            peak: 0.0,
        }
    }

    /// Interleaved samples, `channels` per frame (the count given to `new`).
    pub fn push(&mut self, interleaved: &[f32]) {
        let channels = self.filters.len();
        for frame in interleaved.chunks_exact(channels) {
            for (c, &x) in frame.iter().enumerate() {
                self.peak = self.peak.max(x.abs());
                let [shelf, high_pass] = &mut self.filters[c];
                let y = high_pass.process(shelf.process(x as f64));
                self.step_sum += y * y;
            }
            self.step_frames += 1;
            if self.step_frames == self.step_len {
                self.steps.push(self.step_sum / self.step_len as f64);
                self.step_sum = 0.0;
                self.step_frames = 0;
            }
        }
    }

    /// Integrated loudness in LUFS, or `None` for silence or under 400 ms of audio.
    pub fn integrated(&self) -> Option<f32> {
        let blocks: Vec<f64> = self.steps.windows(4).map(|w| w.iter().sum::<f64>() / 4.0).collect();
        let lufs = |power: f64| -0.691 + 10.0 * power.log10();
        let mean_lufs = |powers: &[f64]| lufs(powers.iter().sum::<f64>() / powers.len() as f64);
        let absolute: Vec<f64> = blocks.into_iter().filter(|&p| p > 0.0 && lufs(p) > -70.0).collect();
        if absolute.is_empty() {
            return None;
        }
        let relative_gate = mean_lufs(&absolute) - 10.0;
        let gated: Vec<f64> = absolute.iter().copied().filter(|&p| lufs(p) > relative_gate).collect();
        Some(mean_lufs(if gated.is_empty() { &absolute } else { &gated }) as f32)
    }

    /// Highest sample level in dBFS (sample peak, not oversampled true peak).
    pub fn peak_dbfs(&self) -> f32 {
        20.0 * self.peak.max(1e-9).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, channels: usize, hz: f32, amplitude: f32, secs: f32) -> Vec<f32> {
        let n = (secs * rate as f32) as usize;
        (0..n)
            .flat_map(|i| {
                let v = amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
                std::iter::repeat_n(v, channels)
            })
            .collect()
    }

    fn measure(samples: &[f32], rate: u32, channels: usize) -> f32 {
        let mut meter = LoudnessMeter::new(rate, channels);
        // Fed in uneven chunks, as a decoder would.
        for chunk in samples.chunks(1234 * channels) {
            meter.push(chunk);
        }
        meter.integrated().unwrap()
    }

    #[test]
    fn ebu_tech_3341_stereo_sine_reads_its_level() {
        // EBU Tech 3341 case 1/2: stereo 1 kHz sine at -23 dBFS (and -33) reads -23 (-33) LUFS.
        for rate in [44_100, 48_000] {
            for level in [-23.0f32, -33.0] {
                let a = 10f32.powf(level / 20.0);
                let got = measure(&sine(rate, 2, 1000.0, a, 20.0), rate, 2);
                assert!((got - level).abs() < 0.1, "{rate} Hz, {level} dBFS: {got} LUFS");
            }
        }
    }

    #[test]
    fn relative_gate_ignores_quiet_passages() {
        // EBU Tech 3341 case 3 idea: loud and very quiet parts; the quiet part is gated out.
        let rate = 48_000;
        let mut s = sine(rate, 2, 1000.0, 10f32.powf(-23.0 / 20.0), 10.0);
        s.extend(sine(rate, 2, 1000.0, 10f32.powf(-60.0 / 20.0), 10.0));
        let got = measure(&s, rate, 2);
        assert!((got + 23.0).abs() < 0.2, "{got}");
    }

    #[test]
    fn low_frequencies_weigh_less() {
        let rate = 44_100;
        let a = 10f32.powf(-20.0 / 20.0);
        let low = measure(&sine(rate, 1, 30.0, a, 5.0), rate, 1);
        let mid = measure(&sine(rate, 1, 1000.0, a, 5.0), rate, 1);
        assert!(mid - low > 3.0, "30 Hz {low}, 1 kHz {mid}");
    }

    #[test]
    fn silence_and_peak() {
        let mut meter = LoudnessMeter::new(44_100, 2);
        meter.push(&vec![0.0; 44_100 * 2]);
        assert_eq!(meter.integrated(), None);
        meter.push(&[0.5, -0.25]);
        assert!((meter.peak_dbfs() + 6.02).abs() < 0.01);
    }
}
