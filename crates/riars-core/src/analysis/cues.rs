//! Cue points of a track: audible start and end, end of the intro (vocals start) and
//! start of the outro (vocals end).
//!
//! Vocals are told apart from the music around them by three cues, each measured
//! against the same track (not against fixed levels, so it works for quiet and loud
//! masters alike):
//! - energy in the voice band (250-3500 Hz) against the whole spectrum;
//! - syllabic modulation: the voice band's loudness moves at 3-7 Hz with syllables,
//!   while pads, guitars and most drum patterns do not;
//! - harmonicity: a voice is a pitched, peaky spectrum, not a flat one.
//!
//! It is a heuristic: it proposes, the operator confirms or corrects. A track whose
//! sections do not differ enough (instrumentals, spoken word) gets no intro/outro and
//! a low confidence.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::loudness::LoudnessMeter;

/// Analysis sample rate: voice-band features need no more than this.
const RATE: u32 = 11_025;
const FFT: usize = 512;
const HOP: usize = 256;
/// Frames per second of the feature tracks.
const FPS: f32 = RATE as f32 / HOP as f32;
/// Below this (dBFS) a frame counts as silence for the audible start and end.
const SILENCE_DB: f32 = -45.0;
/// Vocals must last this long to count (a shout or a sample does not start a verse).
const MIN_VOCAL_SECS: f32 = 2.0;
/// Intros shorter than this are not worth talking over.
const MIN_INTRO_SECS: f32 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueAnalysis {
    pub duration_ms: u64,
    /// First audible moment.
    pub cue_in_ms: u64,
    /// Last audible moment.
    pub cue_out_ms: u64,
    /// Where the vocals start (end of the instrumental intro), if found.
    pub intro_ms: Option<u64>,
    /// Where the vocals end (start of the instrumental outro), if found.
    pub outro_ms: Option<u64>,
    /// 0 to 1: how clearly the vocal sections stand out.
    pub confidence: f32,
    /// Integrated loudness (EBU R128) of the file as decoded, all channels.
    pub loudness_lufs: Option<f32>,
    /// Highest sample level (dBFS).
    pub peak_dbfs: f32,
    /// Waveform for editors: peak level per bucket, 0-255.
    pub peaks: Vec<u8>,
}

/// Decode `path` (mono, downsampled) and analyse it. Blocking: seconds on a Raspberry
/// Pi for a song; run it on a worker thread, never on the audio thread.
pub fn analyze_file(path: &Path, buckets: usize) -> Result<CueAnalysis, String> {
    let (mono, rate, meter) = decode_mono(path)?;
    let samples = resample(&mono, rate, RATE);
    let mut analysis = analyze_samples(&samples, RATE, buckets);
    // Loudness from the file itself (full band, every channel), not the analysis copy.
    if let Some(meter) = meter {
        analysis.loudness_lufs = meter.integrated();
        analysis.peak_dbfs = meter.peak_dbfs();
    }
    Ok(analysis)
}

/// Mono samples at the file's rate, and the loudness measured while decoding.
fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32, Option<LoudnessMeter>), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Unsupported audio {}: {e}", path.display()))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("No audio track")?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.ok_or("Unknown sample rate")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Cannot decode {}: {e}", path.display()))?;
    let mut mono = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    let mut meter: Option<LoudnessMeter> = None;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let buf = buffer.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if buf.capacity() < decoded.capacity() * channels {
            *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        buf.copy_interleaved_ref(decoded);
        meter.get_or_insert_with(|| LoudnessMeter::new(rate, channels)).push(buf.samples());
        mono.extend(buf.samples().chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
    }
    Ok((mono, rate, meter))
}

/// Box-filter decimation (plus linear stepping for non-integer ratios): good enough
/// for features under 4 kHz.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = from as f64 / to as f64;
    let width = step.ceil().max(1.0) as usize;
    let n = (input.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let start = (i as f64 * step) as usize;
            let end = (start + width).min(input.len());
            input[start..end].iter().sum::<f32>() / (end - start).max(1) as f32
        })
        .collect()
}

/// In-place iterative radix-2 FFT (`re.len()` a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

struct Frame {
    db: f32,
    /// Voice-band share of the energy, 0-1.
    voice: f32,
    /// Voice-band energy (linear), for the modulation track.
    voice_energy: f32,
    /// Peakiness of the voice band: 1 - spectral flatness.
    harmonic: f32,
}

fn frames(samples: &[f32], rate: u32) -> Vec<Frame> {
    let window: Vec<f32> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT as f32).cos())
        .collect();
    let bin_hz = rate as f32 / FFT as f32;
    let (lo, hi) = ((250.0 / bin_hz) as usize, (3500.0 / bin_hz) as usize);
    let mut re = vec![0.0; FFT];
    let mut im = vec![0.0; FFT];
    let mut out = Vec::with_capacity(samples.len() / HOP + 1);
    let mut start = 0;
    while start + FFT <= samples.len() {
        let chunk = &samples[start..start + FFT];
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / FFT as f32).sqrt();
        for i in 0..FFT {
            re[i] = chunk[i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let power: Vec<f32> = (1..FFT / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        let total: f32 = power.iter().sum::<f32>() + 1e-12;
        let band = &power[lo.saturating_sub(1)..hi.min(power.len())];
        let voice_energy: f32 = band.iter().sum();
        let mean = voice_energy / band.len().max(1) as f32 + 1e-12;
        let log_mean = band.iter().map(|p| (p + 1e-12).ln()).sum::<f32>() / band.len().max(1) as f32;
        let flatness = (log_mean.exp() / mean).clamp(0.0, 1.0);
        out.push(Frame {
            db: 20.0 * (rms + 1e-9).log10(),
            voice: voice_energy / total,
            voice_energy,
            harmonic: 1.0 - flatness,
        });
        start += HOP;
    }
    out
}

/// Share of the voice-band loudness movement at syllable rate (3-7 Hz) over a window
/// around each frame.
fn syllabic(frames: &[Frame]) -> Vec<f32> {
    let half = (0.75 * FPS) as usize;
    let env: Vec<f32> = frames.iter().map(|f| (f.voice_energy + 1e-9).ln()).collect();
    (0..frames.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half).min(env.len()));
            let seg = &env[a..b];
            if seg.len() < 16 {
                return 0.0;
            }
            let mean = seg.iter().sum::<f32>() / seg.len() as f32;
            let power_at = |hz: f32| {
                let w = 2.0 * std::f32::consts::PI * hz / FPS;
                let (mut c, mut s) = (0.0f32, 0.0f32);
                for (n, v) in seg.iter().enumerate() {
                    let x = v - mean;
                    c += x * (w * n as f32).cos();
                    s += x * (w * n as f32).sin();
                }
                c * c + s * s
            };
            let syllable: f32 = [3.0, 4.0, 5.0, 6.0, 7.0].iter().map(|&hz| power_at(hz)).sum();
            let all: f32 = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 9.0, 12.0].iter().map(|&hz| power_at(hz)).sum();
            if all <= 1e-9 { 0.0 } else { syllable / all }
        })
        .collect()
}

fn percentile(values: &[f32], p: f32) -> f32 {
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(((v.len() as f32 - 1.0) * p).round() as usize).copied().unwrap_or(0.0)
}

/// Z-score of `x` against the audible frames of the track.
fn standardize(x: &[f32], audible: &[bool]) -> Vec<f32> {
    let vals: Vec<f32> = x.iter().zip(audible).filter(|(_, a)| **a).map(|(v, _)| *v).collect();
    if vals.is_empty() {
        return vec![0.0; x.len()];
    }
    let mean = vals.iter().sum::<f32>() / vals.len() as f32;
    let sd = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32).sqrt().max(1e-6);
    x.iter().map(|v| (v - mean) / sd).collect()
}

fn smooth(x: &[f32], half: usize) -> Vec<f32> {
    (0..x.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(x.len()));
            x[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// Analyse mono samples at `rate` (ideally 11 025 Hz; other rates work, slower).
pub fn analyze_samples(samples: &[f32], rate: u32, buckets: usize) -> CueAnalysis {
    let duration_ms = samples.len() as u64 * 1000 / rate.max(1) as u64;
    let peaks = waveform(samples, buckets);
    let mut meter = LoudnessMeter::new(rate, 1);
    meter.push(samples);
    let (loudness_lufs, peak_dbfs) = (meter.integrated(), meter.peak_dbfs());
    let frames = frames(samples, rate);
    let fps = rate as f32 / HOP as f32;
    let to_ms = |frame: usize| ((frame * HOP) as f32 * 1000.0 / rate as f32) as u64;
    let audible: Vec<bool> = frames.iter().map(|f| f.db > SILENCE_DB).collect();
    let first = audible.iter().position(|a| *a);
    let last = audible.iter().rposition(|a| *a);
    let (Some(first), Some(last)) = (first, last) else {
        return CueAnalysis { duration_ms, cue_in_ms: 0, cue_out_ms: duration_ms, intro_ms: None, outro_ms: None, confidence: 0.0, loudness_lufs, peak_dbfs, peaks };
    };
    let cue_in_ms = to_ms(first);
    let cue_out_ms = (to_ms(last) + (FFT as u64 * 1000 / rate as u64)).min(duration_ms);

    // Vocal score: the three cues, each standardised within the track, smoothed.
    let voice = standardize(&frames.iter().map(|f| f.voice).collect::<Vec<_>>(), &audible);
    let harmonic = standardize(&frames.iter().map(|f| f.harmonic).collect::<Vec<_>>(), &audible);
    let syll = standardize(&syllabic(&frames), &audible);
    let raw: Vec<f32> = (0..frames.len())
        .map(|i| if audible[i] { 0.4 * voice[i] + 0.4 * syll[i] + 0.2 * harmonic[i] } else { -3.0 })
        .collect();
    let score = smooth(&raw, (0.5 * fps) as usize);

    // Threshold halfway between the quiet and busy parts of this track.
    let active: Vec<f32> = score[first..=last].to_vec();
    let (low, high) = (percentile(&active, 0.2), percentile(&active, 0.8));
    let contrast = high - low;
    let confidence = ((contrast - 0.3) / 1.2).clamp(0.0, 1.0);
    if confidence < 0.15 {
        return CueAnalysis { duration_ms, cue_in_ms, cue_out_ms, intro_ms: None, outro_ms: None, confidence, loudness_lufs, peak_dbfs, peaks };
    }
    let threshold = low + 0.5 * contrast;
    let need = (MIN_VOCAL_SECS * fps) as usize;
    let vocal: Vec<bool> = score.iter().map(|s| *s > threshold).collect();
    // First / last run of vocal frames at least `need` long.
    let runs = {
        let mut runs = Vec::new();
        let mut i = first;
        while i <= last {
            if vocal[i] {
                let start = i;
                while i <= last && vocal[i] {
                    i += 1;
                }
                if i - start >= need {
                    runs.push((start, i));
                }
            } else {
                i += 1;
            }
        }
        runs
    };
    let (Some(&(vocal_start, _)), Some(&(_, vocal_end))) = (runs.first(), runs.last()) else {
        return CueAnalysis { duration_ms, cue_in_ms, cue_out_ms, intro_ms: None, outro_ms: None, confidence: confidence * 0.5, loudness_lufs, peak_dbfs, peaks };
    };
    // The smoothing spreads the edges by about half a window: pull them in so the
    // marks fall before the voice starts and after it ends, never on it.
    let pad = (0.5 * fps) as usize;
    let intro = to_ms(vocal_start.saturating_sub(pad)).max(cue_in_ms);
    let outro = to_ms((vocal_end + pad).min(last)).min(cue_out_ms);
    let min_intro = (MIN_INTRO_SECS * 1000.0) as u64;
    CueAnalysis {
        duration_ms,
        cue_in_ms,
        cue_out_ms,
        intro_ms: Some(if intro.saturating_sub(cue_in_ms) < min_intro { cue_in_ms } else { intro }),
        outro_ms: Some(if cue_out_ms.saturating_sub(outro) < min_intro { cue_out_ms } else { outro }),
        confidence,
        loudness_lufs,
        peak_dbfs,
        peaks,
    }
}

fn waveform(samples: &[f32], buckets: usize) -> Vec<u8> {
    if buckets == 0 || samples.is_empty() {
        return Vec::new();
    }
    let per = samples.len().div_ceil(buckets);
    samples
        .chunks(per.max(1))
        .map(|c| (c.iter().fold(0.0f32, |m, s| m.max(s.abs())).min(1.0) * 255.0) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: u32 = RATE;

    /// Steady instrumental bed: a chord pad and a kick at 120 bpm.
    fn bed(t: f32) -> f32 {
        let pad: f32 = [110.0f32, 165.0, 220.0, 277.0].iter().map(|f| (2.0 * std::f32::consts::PI * f * t).sin()).sum::<f32>() * 0.06;
        let beat = t % 0.5;
        let kick = if beat < 0.08 { (2.0 * std::f32::consts::PI * 55.0 * beat).sin() * (1.0 - beat / 0.08) * 0.5 } else { 0.0 };
        pad + kick
    }

    /// A sung line: a harmonic voice around 220 Hz with formants, gliding pitch and
    /// syllables at about 4.5 per second.
    fn voice(t: f32) -> f32 {
        let f0 = 220.0 * (1.0 + 0.06 * (2.0 * std::f32::consts::PI * 0.7 * t).sin());
        let syllable = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 4.5 * t).sin()).powf(2.0);
        let mut v = 0.0;
        for h in 1..=14 {
            let f = f0 * h as f32;
            // Formant-ish envelope: emphasis around 700 Hz and 1200 Hz.
            let gain = (-((f - 700.0) / 350.0).powi(2)).exp() + 0.7 * (-((f - 1200.0) / 400.0).powi(2)).exp() + 0.05;
            v += gain * (2.0 * std::f32::consts::PI * f * t).sin();
        }
        v * 0.12 * syllable
    }

    fn song(intro: f32, vocals_end: f32, total: f32, lead_silence: f32) -> Vec<f32> {
        let n = (total * R as f32) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / R as f32;
                if t < lead_silence {
                    return 0.0;
                }
                let mut s = bed(t);
                if t >= intro && t < vocals_end {
                    s += voice(t);
                }
                s
            })
            .collect()
    }

    #[test]
    fn finds_intro_and_outro_of_a_song() {
        let a = analyze_samples(&song(12.0, 50.0, 60.0, 0.0), R, 100);
        let intro = a.intro_ms.unwrap() as f32 / 1000.0;
        let outro = a.outro_ms.unwrap() as f32 / 1000.0;
        // The marks must not fall on the voice: intro at most at 12 s, outro at least at 50 s.
        assert!((10.0..=12.2).contains(&intro), "intro {intro}");
        assert!((49.8..=52.5).contains(&outro), "outro {outro}");
        assert!(a.confidence > 0.3, "{}", a.confidence);
        assert_eq!(a.peaks.len(), 100);
    }

    #[test]
    fn audible_start_and_end_skip_silence() {
        let mut s = song(10.0, 30.0, 40.0, 1.5);
        s.extend(std::iter::repeat_n(0.0, R as usize * 3)); // 3 s of trailing silence
        let a = analyze_samples(&s, R, 0);
        assert!((1400..=1600).contains(&a.cue_in_ms), "{}", a.cue_in_ms);
        assert!((39_800..=40_200).contains(&a.cue_out_ms), "{}", a.cue_out_ms);
        assert_eq!(a.duration_ms, 43_000);
    }

    #[test]
    fn vocals_from_the_start_leave_no_intro() {
        let a = analyze_samples(&song(0.0, 30.0, 40.0, 0.0), R, 0);
        assert_eq!(a.intro_ms, Some(a.cue_in_ms));
    }

    #[test]
    fn instrumental_gets_no_marks() {
        let n = (40.0 * R as f32) as usize;
        let s: Vec<f32> = (0..n).map(|i| bed(i as f32 / R as f32)).collect();
        let a = analyze_samples(&s, R, 0);
        assert!(a.intro_ms.is_none() && a.outro_ms.is_none(), "{a:?}");
        assert!(a.confidence < 0.3);
    }

    #[test]
    fn silence_is_handled() {
        let a = analyze_samples(&vec![0.0; R as usize * 5], R, 10);
        assert_eq!((a.cue_in_ms, a.intro_ms), (0, None));
    }

    #[test]
    fn fft_of_a_sine_peaks_at_its_bin() {
        let mut re: Vec<f32> = (0..64).map(|i| (2.0 * std::f32::consts::PI * 8.0 * i as f32 / 64.0).sin()).collect();
        let mut im = vec![0.0; 64];
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..32).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        let max = mags.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(max, 8);
    }
}

/// Analyse every MP3/FLAC/WAV under `RIARS_ANALYZE_DIR` and print the marks (run with
/// `cargo test --release -p riars-core analyze_folder -- --ignored --nocapture`).
#[cfg(test)]
#[test]
#[ignore]
fn analyze_folder() {
    let Some(dir) = std::env::var_os("RIARS_ANALYZE_DIR") else { return };
    let mut files = Vec::new();
    let mut pending = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                pending.push(p);
            } else if matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(), Some("mp3" | "flac" | "wav")) {
                files.push(p);
            }
        }
    }
    files.sort();
    let s = |ms: Option<u64>| ms.map_or("—".to_string(), |ms| format!("{:.1}", ms as f32 / 1000.0));
    println!("file\tlength\tcue_in\tintro\toutro\tcue_out\tconf\tLUFS\tpeak\tms");
    for f in files {
        let started = std::time::Instant::now();
        match analyze_file(&f, 0) {
            Ok(a) => println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{:.1}\t{}",
                f.file_name().unwrap().to_string_lossy(),
                s(Some(a.duration_ms)), s(Some(a.cue_in_ms)), s(a.intro_ms), s(a.outro_ms), s(Some(a.cue_out_ms)),
                a.confidence, a.loudness_lufs.map_or("—".into(), |l| format!("{l:.1}")), a.peak_dbfs, started.elapsed().as_millis()
            ),
            Err(e) => println!("{}\tERROR {e}", f.display()),
        }
    }
}
