use std::path::{Path, PathBuf};
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct TrackPlayer {
    file_path: PathBuf,
    position: Duration,
    duration: Duration,
    volume: f32,
    sample_rate: u32,
    samples: Vec<f32>,
    sample_pos: usize,
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
        let duration_secs = if sample_rate > 0 {
            n_frames as f64 / sample_rate as f64
        } else {
            0.0
        };

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

            let decoded = decoder
                .decode(&packet)
                .map_err(|e| format!("Decode error: {e}"))?;
            let spec = *decoded.spec();
            let num_samples = decoded.capacity();
            let mut sample_buf = SampleBuffer::<f32>::new(num_samples as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buf.samples());
        }

        Ok(Self {
            file_path: path.to_path_buf(),
            position: Duration::ZERO,
            duration: Duration::from_secs_f64(duration_secs),
            volume: 1.0,
            sample_rate,
            samples,
            sample_pos: 0,
            channels,
        })
    }

    pub fn file_name(&self) -> &str {
        self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    pub fn position(&self) -> Duration {
        self.position
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn seek(&mut self, position: Duration) {
        let sample_offset =
            (position.as_secs_f64() * self.sample_rate as f64) as usize * self.channels;
        self.sample_pos = sample_offset.min(self.samples.len());
        self.position = position;
    }

    pub fn read_samples(&mut self, output: &mut [f32]) -> usize {
        let available = self.samples.len() - self.sample_pos;
        let to_copy = output.len().min(available);

        for (i, sample) in output.iter_mut().enumerate().take(to_copy) {
            *sample = self.samples[self.sample_pos + i] * self.volume;
        }

        self.sample_pos += to_copy;
        let elapsed_frames = self.sample_pos / self.channels;
        self.position = Duration::from_secs_f64(elapsed_frames as f64 / self.sample_rate as f64);

        to_copy
    }

    pub fn is_finished(&self) -> bool {
        self.sample_pos >= self.samples.len()
    }
}
