use std::path::{Path, PathBuf};
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// A fully decoded track. Reading methods (`sample`, `advance`) are real-time safe.
pub struct TrackPlayer {
    file_path: PathBuf,
    duration: Duration,
    volume: f32,
    sample_rate: u32,
    samples: Vec<f32>,
    /// Read position in source frames; fractional so playback can resample.
    frame_pos: f64,
    channels: usize,
}

impl TrackPlayer {
    pub fn from_file(path: &Path, target_sample_rate: u32) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("Unsupported format: {e}"))?;

        let mut format_reader = probed.format;
        let track = format_reader
            .default_track()
            .ok_or("No audio track found")?;

        let codec_params = track.codec_params.clone();
        let track_id = track.id;
        let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
        let sample_rate = codec_params.sample_rate.unwrap_or(target_sample_rate);
        let n_frames = codec_params.n_frames.unwrap_or(0);

        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Unsupported codec: {e}"))?;

        let mut samples = Vec::with_capacity(n_frames as usize * channels);

        loop {
            let packet = match format_reader.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(e) => return Err(format!("Decode error: {e}")),
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                // A corrupt packet should not make the whole track unplayable.
                Err(symphonia::core::errors::Error::DecodeError(e)) => {
                    log::warn!("Skipping undecodable packet in {}: {e}", path.display());
                    continue;
                }
                Err(e) => return Err(format!("Decode error: {e}")),
            };
            let spec = *decoded.spec();
            let num_samples = decoded.capacity();
            let mut sample_buf = SampleBuffer::<f32>::new(num_samples as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buf.samples());
        }

        Ok(Self::from_samples(path.to_path_buf(), samples, channels, sample_rate))
    }

    /// Build a player from interleaved samples already in memory.
    pub fn from_samples(file_path: PathBuf, samples: Vec<f32>, channels: usize, sample_rate: u32) -> Self {
        let channels = channels.max(1);
        let frames = samples.len() / channels;
        let duration = if sample_rate > 0 {
            Duration::from_secs_f64(frames as f64 / sample_rate as f64)
        } else {
            Duration::ZERO
        };
        Self {
            file_path,
            duration,
            volume: 1.0,
            sample_rate,
            samples,
            frame_pos: 0.0,
            channels,
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn file_name(&self) -> &str {
        self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    pub fn position(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.frame_pos.min(self.frames() as f64) / self.sample_rate as f64)
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn seek(&mut self, position: Duration) {
        let frame = position.as_secs_f64() * self.sample_rate as f64;
        self.frame_pos = frame.min(self.frames() as f64);
    }

    /// Linearly interpolated sample for output channel `out_channel` at the read position.
    /// Mono sources feed every output channel; extra output channels get silence.
    pub fn sample(&self, out_channel: usize) -> f32 {
        let src_channel = if self.channels == 1 {
            0
        } else if out_channel < self.channels {
            out_channel
        } else {
            return 0.0;
        };

        let frames = self.frames();
        let index = self.frame_pos as usize;
        if index >= frames {
            return 0.0;
        }
        let frac = (self.frame_pos - index as f64) as f32;
        let a = self.samples[index * self.channels + src_channel];
        let b = if index + 1 < frames {
            self.samples[(index + 1) * self.channels + src_channel]
        } else {
            a
        };
        (a + (b - a) * frac) * self.volume
    }

    /// Move the read position forward by `step` source frames (source rate / output rate).
    pub fn advance(&mut self, step: f64) {
        self.frame_pos += step;
    }

    pub fn is_finished(&self) -> bool {
        self.frame_pos as usize >= self.frames()
    }

    fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_ramp() -> TrackPlayer {
        // Frames: (0, 10), (1, 11), (2, 12), (3, 13) at 4 Hz -> 1 second.
        let samples = vec![0.0, 10.0, 1.0, 11.0, 2.0, 12.0, 3.0, 13.0];
        TrackPlayer::from_samples(PathBuf::from("ramp.wav"), samples, 2, 4)
    }

    #[test]
    fn duration_from_samples() {
        assert_eq!(stereo_ramp().duration(), Duration::from_secs(1));
    }

    #[test]
    fn interpolates_between_frames() {
        let mut p = stereo_ramp();
        p.advance(1.5);
        assert!((p.sample(0) - 1.5).abs() < 1e-6);
        assert!((p.sample(1) - 11.5).abs() < 1e-6);
        assert_eq!(p.sample(2), 0.0);
    }

    #[test]
    fn mono_feeds_all_outputs() {
        let p = TrackPlayer::from_samples(PathBuf::from("m.wav"), vec![0.5, 0.25], 1, 2);
        assert_eq!(p.sample(0), 0.5);
        assert_eq!(p.sample(1), 0.5);
    }

    #[test]
    fn finishes_and_seeks() {
        let mut p = stereo_ramp();
        p.advance(4.0);
        assert!(p.is_finished());
        assert_eq!(p.sample(0), 0.0);
        p.seek(Duration::from_millis(500));
        assert!(!p.is_finished());
        assert_eq!(p.position(), Duration::from_millis(500));
    }
}
