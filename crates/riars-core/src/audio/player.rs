use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Tracks at least this long are decoded progressively instead of all at once. Shorter
/// ones (jingles, idents) load whole: tiny, and ready instantly.
const STREAM_MIN_SECS: u64 = 30;
/// Audio the decoder thread keeps ready ahead of playback.
const STREAM_BUFFER_SECS: usize = 5;
/// Audio decoded before `from_file` returns, so playback starts without a stall.
const STREAM_PREFILL_SECS: f64 = 0.5;
/// Streaming supports up to this many channels (more are decoded in memory).
const MAX_CHANNELS: usize = 8;
/// Samples copied from the ring at a time, so reading costs a couple of atomic
/// operations per block instead of per sample.
const CACHE_SAMPLES: usize = 1024;

/// How to open a file: decode it all, stream it, or pick by length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Auto,
    Memory,
    Stream,
}

/// A track ready to play. Reading methods (`sample`, `advance`, `seek`) are real-time
/// safe: long tracks are decoded by their own thread into a ring buffer.
pub struct TrackPlayer {
    file_path: PathBuf,
    duration: Duration,
    volume: f32,
    sample_rate: u32,
    /// Read position in source frames; fractional so playback can resample.
    frame_pos: f64,
    channels: usize,
    source: Source,
    /// Never crossfade into or out of this track (e.g. spots).
    no_overlap: bool,
    /// Linear gain applied when mixing (loudness normalisation); may exceed 1.
    gain: f32,
    /// A track to play over this one (imaging over the intro or outro), started when
    /// this one reaches `Overlay::start_frame`.
    overlay: Option<Overlay>,
}

/// Imaging attached to a track: plays over it from a given frame, ducking it.
pub struct Overlay {
    pub player: Box<TrackPlayer>,
    /// In the host track's source frames.
    pub start_frame: f64,
    /// Linear gain of the host track while the overlay plays (1 = no ducking).
    pub duck: f32,
}

enum Source {
    /// Decoded audio, shareable: sound-bank buttons play the same samples many times.
    Memory(Arc<[f32]>),
    Stream(Box<StreamSource>),
}

/// The audio-thread side of a progressively decoded track.
struct StreamSource {
    consumer: rtrb::Consumer<f32>,
    /// Samples already taken from the ring, read from `cache_pos` to `cache_len`.
    cache: [f32; CACHE_SAMPLES],
    cache_pos: usize,
    cache_len: usize,
    control: Arc<DecodeControl>,
    /// Frames `win_index` and `win_index + 1`, for interpolation.
    window: [[f32; MAX_CHANNELS]; 2],
    win_index: usize,
    /// How many of the two window slots hold data.
    win_valid: usize,
    /// Frames according to the container; the real end is when the decoder runs out.
    total_frames: usize,
    /// A seek waiting for the decoder: its generation.
    waiting_seek: Option<u32>,
    ended: bool,
    _thread: std::thread::JoinHandle<()>,
}

/// Shared between the audio thread and the decoder thread.
struct DecodeControl {
    eof: AtomicBool,
    seek_frame: AtomicU64,
    seek_requested: AtomicU32,
    /// The decoder sought to `seek_frame` and waits for the ring to be flushed.
    seek_done: AtomicU32,
    /// The player dropped the stale audio; the decoder may fill the ring again.
    seek_flushed: AtomicU32,
}

impl TrackPlayer {
    pub fn from_file(path: &Path, target_sample_rate: u32) -> Result<Self, String> {
        Self::open(path, target_sample_rate, LoadMode::Auto)
    }

    pub fn open(path: &Path, target_sample_rate: u32, mode: LoadMode) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| format!("Unsupported format: {e}"))?;

        let format_reader = probed.format;
        let track = format_reader.default_track().ok_or("No audio track found")?;
        let codec_params = track.codec_params.clone();
        let track_id = track.id;
        let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
        let sample_rate = codec_params.sample_rate.unwrap_or(target_sample_rate);
        let n_frames = codec_params.n_frames.unwrap_or(0);

        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Unsupported codec: {e}"))?;

        // Without a known length the duration would be a guess: decode it all.
        let long = sample_rate > 0 && n_frames / sample_rate as u64 >= STREAM_MIN_SECS;
        let stream = match mode {
            LoadMode::Memory => false,
            LoadMode::Stream => n_frames > 0,
            LoadMode::Auto => long,
        } && (1..=MAX_CHANNELS).contains(&channels);

        if stream {
            return Ok(Self::streamed(path, format_reader, decoder, track_id, channels, sample_rate, n_frames));
        }
        let samples = decode_all(path, format_reader, decoder, track_id, n_frames as usize * channels)?;
        Ok(Self::from_samples(path.to_path_buf(), samples, channels, sample_rate))
    }

    fn streamed(
        path: &Path,
        format_reader: Box<dyn FormatReader>,
        decoder: Box<dyn Decoder>,
        track_id: u32,
        channels: usize,
        sample_rate: u32,
        n_frames: u64,
    ) -> Self {
        let capacity = sample_rate as usize * channels * STREAM_BUFFER_SECS;
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let control = Arc::new(DecodeControl {
            eof: AtomicBool::new(false),
            seek_frame: AtomicU64::new(0),
            seek_requested: AtomicU32::new(0),
            seek_done: AtomicU32::new(0),
            seek_flushed: AtomicU32::new(0),
        });
        let thread_control = Arc::clone(&control);
        let name = path.display().to_string();
        let thread = std::thread::Builder::new()
            .name("riars-decoder".into())
            .spawn(move || decode_loop(format_reader, decoder, track_id, producer, thread_control, name))
            .expect("cannot start decoder thread");

        // Wait for the first half second (or the end), so playback starts at once.
        let prefill = (STREAM_PREFILL_SECS * sample_rate as f64) as usize * channels;
        let started = std::time::Instant::now();
        while consumer.slots() < prefill.min(capacity)
            && !control.eof.load(Ordering::Acquire)
            && started.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(2));
        }

        let mut source = StreamSource {
            consumer,
            cache: [0.0; CACHE_SAMPLES],
            cache_pos: 0,
            cache_len: 0,
            control,
            window: [[0.0; MAX_CHANNELS]; 2],
            win_index: 0,
            win_valid: 0,
            total_frames: n_frames as usize,
            waiting_seek: None,
            ended: false,
            _thread: thread,
        };
        source.fill(channels);
        Self {
            file_path: path.to_path_buf(),
            duration: Duration::from_secs_f64(n_frames as f64 / sample_rate as f64),
            volume: 1.0,
            sample_rate,
            frame_pos: 0.0,
            no_overlap: false,
            gain: 1.0,
            overlay: None,
            channels,
            source: Source::Stream(Box::new(source)),
        }
    }

    /// Build a player from interleaved samples already in memory.
    pub fn set_no_overlap(&mut self, no_overlap: bool) {
        self.no_overlap = no_overlap;
    }

    pub fn no_overlap(&self) -> bool {
        self.no_overlap
    }

    /// Mixing gain (linear, 0 to 8): loudness normalisation.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 8.0);
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Attach imaging to play over this track from `start_frame` (source frames).
    pub fn set_overlay(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    /// The attached overlay once this track has reached its start. Real-time safe.
    pub fn take_due_overlay(&mut self) -> Option<Overlay> {
        if self.overlay.as_ref().is_some_and(|o| self.frame_pos >= o.start_frame) {
            self.overlay.take()
        } else {
            None
        }
    }

    pub fn from_samples(file_path: PathBuf, samples: Vec<f32>, channels: usize, sample_rate: u32) -> Self {
        Self::from_shared(file_path, samples.into(), channels, sample_rate)
    }

    /// A player over decoded audio another player may be using too; no copy. Dropping
    /// it can free the audio, so retire it off the audio thread like any player.
    pub fn from_shared(file_path: PathBuf, samples: Arc<[f32]>, channels: usize, sample_rate: u32) -> Self {
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
            frame_pos: 0.0,
            no_overlap: false,
            gain: 1.0,
            overlay: None,
            channels,
            source: Source::Memory(samples),
        }
    }

    /// The decoded audio, channels and rate of an in-memory player, to share with
    /// `from_shared`; `None` for a streamed one.
    pub fn shared_samples(&self) -> Option<(Arc<[f32]>, usize, u32)> {
        match &self.source {
            Source::Memory(samples) => Some((Arc::clone(samples), self.channels, self.sample_rate)),
            Source::Stream(_) => None,
        }
    }

    /// Whether the track is decoded progressively.
    pub fn is_streamed(&self) -> bool {
        matches!(self.source, Source::Stream(_))
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
        Duration::from_secs_f64(self.frame_pos.min(self.total_frames() as f64) / self.sample_rate as f64)
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
        let frame = (position.as_secs_f64() * self.sample_rate as f64).min(self.total_frames() as f64);
        self.frame_pos = frame;
        if let Source::Stream(s) = &mut self.source {
            // Ask the decoder thread; until it answers, play silence.
            let generation = s.control.seek_requested.load(Ordering::Relaxed).wrapping_add(1);
            s.control.seek_frame.store(frame as u64, Ordering::Relaxed);
            s.control.seek_requested.store(generation, Ordering::Release);
            s.waiting_seek = Some(generation);
            s.win_index = frame as usize;
            s.win_valid = 0;
            s.ended = false;
        }
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
        let index = self.frame_pos as usize;
        let frac = (self.frame_pos - index as f64) as f32;

        let (a, b) = match &self.source {
            Source::Memory(samples) => {
                let frames = samples.len() / self.channels;
                if index >= frames {
                    return 0.0;
                }
                let a = samples[index * self.channels + src_channel];
                let b = if index + 1 < frames { samples[(index + 1) * self.channels + src_channel] } else { a };
                (a, b)
            }
            Source::Stream(s) => {
                if s.waiting_seek.is_some() || s.win_valid == 0 || s.win_index != index {
                    return 0.0; // seeking or starved
                }
                let a = s.window[0][src_channel];
                let b = if s.win_valid == 2 { s.window[1][src_channel] } else { a };
                (a, b)
            }
        };
        (a + (b - a) * frac) * self.volume
    }

    /// Move the read position forward by `step` source frames (source rate / output rate).
    /// A streamed track that has not been decoded that far yet waits instead.
    pub fn advance(&mut self, step: f64) {
        let channels = self.channels;
        let s = match &mut self.source {
            Source::Memory(_) => {
                self.frame_pos += step;
                return;
            }
            Source::Stream(s) => s,
        };
        if let Some(generation) = s.waiting_seek {
            if s.control.seek_done.load(Ordering::Acquire) != generation {
                return;
            }
            // The decoder sought and is paused: everything in the ring is stale.
            let stale = s.consumer.slots();
            if let Ok(chunk) = s.consumer.read_chunk(stale) {
                chunk.commit_all();
            }
            s.cache_pos = 0;
            s.cache_len = 0;
            s.control.seek_flushed.store(generation, Ordering::Release);
            s.waiting_seek = None;
            // This output frame was silence; start moving on the next one.
            return;
        }

        if s.win_valid == 0 || s.win_index != self.frame_pos as usize {
            s.fill(channels);
            if s.win_valid == 0 && s.control.eof.load(Ordering::Acquire) && s.is_drained() {
                s.ended = true;
                s.total_frames = s.win_index;
            }
            return; // this frame was not decoded yet when it was played: wait
        }

        let target = self.frame_pos + step;
        while target as usize > s.win_index {
            if s.win_valid == 2 {
                s.window[0] = s.window[1];
                s.win_valid = 1;
                s.win_index += 1;
                s.fill(channels);
            } else {
                s.fill(channels);
                if s.win_valid < 2 {
                    if s.control.eof.load(Ordering::Acquire) && s.is_drained() {
                        // Past the last decoded frame.
                        s.ended = true;
                        s.total_frames = s.win_index + s.win_valid;
                        self.frame_pos = s.total_frames as f64;
                    }
                    return; // otherwise the decoder is behind: wait
                }
            }
        }
        self.frame_pos = target;
    }

    pub fn is_finished(&self) -> bool {
        match &self.source {
            Source::Memory(_) => self.frame_pos as usize >= self.frames(),
            Source::Stream(s) => s.ended,
        }
    }

    fn frames(&self) -> usize {
        match &self.source {
            Source::Memory(samples) => samples.len() / self.channels,
            Source::Stream(s) => s.total_frames,
        }
    }

    pub fn total_frames(&self) -> usize {
        self.frames()
    }

    /// Source frames left to play.
    pub fn remaining_frames(&self) -> f64 {
        (self.frames() as f64 - self.frame_pos).max(0.0)
    }
}

impl StreamSource {
    /// Pull frames until both window slots are full or the audio runs dry.
    fn fill(&mut self, channels: usize) {
        while self.win_valid < 2 {
            if self.cache_len - self.cache_pos < channels && !self.refill(channels) {
                return;
            }
            let frame = &self.cache[self.cache_pos..self.cache_pos + channels];
            self.window[self.win_valid][..channels].copy_from_slice(frame);
            self.cache_pos += channels;
            self.win_valid += 1;
        }
    }

    /// Move whole frames from the ring into the cache. False if not even one frame
    /// is available.
    fn refill(&mut self, channels: usize) -> bool {
        // Keep a partial frame left in the cache (none in practice: we copy whole frames).
        let left = self.cache_len - self.cache_pos;
        self.cache.copy_within(self.cache_pos..self.cache_len, 0);
        self.cache_pos = 0;
        self.cache_len = left;
        let room = (CACHE_SAMPLES - left) / channels * channels;
        let take = (self.consumer.slots() / channels * channels).min(room);
        if take > 0 {
            if let Ok(chunk) = self.consumer.read_chunk(take) {
                let (first, second) = chunk.as_slices();
                self.cache[left..left + first.len()].copy_from_slice(first);
                self.cache[left + first.len()..left + take].copy_from_slice(second);
                chunk.commit_all();
                self.cache_len = left + take;
            }
        }
        self.cache_len - self.cache_pos >= channels
    }

    fn is_drained(&self) -> bool {
        self.consumer.is_empty() && self.cache_pos == self.cache_len
    }
}

fn decode_all(
    path: &Path,
    mut format_reader: Box<dyn FormatReader>,
    mut decoder: Box<dyn Decoder>,
    track_id: u32,
    capacity: usize,
) -> Result<Vec<f32>, String> {
    let mut samples = Vec::with_capacity(capacity);
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
        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(sample_buf.samples());
    }
    Ok(samples)
}

/// Decoder thread of a streamed track: keeps the ring full, answers seeks, and exits
/// once the player is dropped.
fn decode_loop(
    mut format_reader: Box<dyn FormatReader>,
    mut decoder: Box<dyn Decoder>,
    track_id: u32,
    mut producer: rtrb::Producer<f32>,
    control: Arc<DecodeControl>,
    name: String,
) {
    let mut pending: Vec<f32> = Vec::new();
    let mut offset = 0;
    let sample_rate = decoder.codec_params().sample_rate.unwrap_or(44_100).max(1);
    let channels = decoder.codec_params().channels.map(|c| c.count()).unwrap_or(2).max(1);
    let time_base = decoder.codec_params().time_base;
    // Samples to drop after a seek, from the packet start to the requested frame.
    let mut skip = 0usize;
    while !producer.is_abandoned() {
        let requested = control.seek_requested.load(Ordering::Acquire);
        if requested != control.seek_done.load(Ordering::Relaxed) {
            let frame = control.seek_frame.load(Ordering::Relaxed);
            let time = Time::from(frame as f64 / sample_rate as f64);
            match format_reader.seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(track_id) }) {
                Ok(seeked) => {
                    let ticks = seeked.required_ts.saturating_sub(seeked.actual_ts);
                    let frames = match time_base {
                        Some(tb) => (ticks as f64 * tb.numer as f64 / tb.denom as f64 * sample_rate as f64).round() as usize,
                        None => ticks as usize,
                    };
                    skip = frames * channels;
                }
                Err(e) => log::warn!("Cannot seek in {name}: {e}"),
            }
            decoder.reset();
            pending.clear();
            offset = 0;
            control.eof.store(false, Ordering::Release);
            control.seek_done.store(requested, Ordering::Release);
        }
        // After a seek, push nothing until the player has dropped the stale audio.
        if control.seek_flushed.load(Ordering::Acquire) != control.seek_done.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        if offset == pending.len() && !control.eof.load(Ordering::Relaxed) {
            pending.clear();
            offset = 0;
            match format_reader.next_packet() {
                Ok(packet) if packet.track_id() == track_id => match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                        buf.copy_interleaved_ref(decoded);
                        let samples = buf.samples();
                        let dropped = skip.min(samples.len());
                        skip -= dropped;
                        pending.extend_from_slice(&samples[dropped..]);
                    }
                    Err(symphonia::core::errors::Error::DecodeError(e)) => {
                        log::warn!("Skipping undecodable packet in {name}: {e}");
                    }
                    Err(e) => {
                        log::warn!("Decode error in {name}: {e}");
                        control.eof.store(true, Ordering::Release);
                    }
                },
                Ok(_) => {}
                Err(_) => control.eof.store(true, Ordering::Release), // end of file
            }
            continue;
        }

        // Push what fits. When the ring is full, sleep until about half a second has
        // drained, instead of waking up every few milliseconds for nothing.
        if producer.slots() == 0 {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let free = producer.slots().min(pending.len() - offset);
        if free > 0 {
            if let Ok(mut chunk) = producer.write_chunk_uninit(free) {
                let (first, second) = chunk.as_mut_slices();
                let split = first.len();
                for (slot, value) in first.iter_mut().zip(&pending[offset..offset + split]) {
                    slot.write(*value);
                }
                for (slot, value) in second.iter_mut().zip(&pending[offset + split..offset + free]) {
                    slot.write(*value);
                }
                // SAFETY: all `free` slots were written just above.
                unsafe { chunk.commit_all() };
                offset += free;
            }
        } else {
            // Nothing pending: the file is done; wait for a seek or the player to go.
            std::thread::sleep(Duration::from_millis(50));
        }
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

    /// A 16-bit stereo WAV where frame i holds (i % 1000, (i % 1000) + 1000) / 32768.
    fn write_wav(path: &Path, frames: u32, rate: u32) {
        let mut data = Vec::with_capacity(frames as usize * 4);
        for i in 0..frames {
            for v in [(i % 1000) as i16, (i % 1000 + 1000) as i16] {
                data.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&2u16.to_le_bytes()); // stereo
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }

    /// Read `count` output samples of channel 0 and 1, waiting out decoder stalls.
    fn read(p: &mut TrackPlayer, count: usize, step: f64) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        let mut spins = 0;
        while out.len() < count && !p.is_finished() {
            // When the decoder is behind and holds only the current frame, the player
            // plays it without interpolating towards the next one: inaudible, but the
            // exact comparisons in these tests would see it. Let the decoder catch up.
            let channels = p.channels;
            if let Source::Stream(s) = &mut p.source {
                let mut waits = 0;
                while s.win_valid == 1 && !(s.control.eof.load(Ordering::Acquire) && s.is_drained()) && waits < 10_000 {
                    s.fill(channels);
                    if s.win_valid == 1 {
                        waits += 1;
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }
            let before = p.frame_pos;
            let pair = (p.sample(0), p.sample(1));
            p.advance(step);
            if p.frame_pos == before && !p.is_finished() {
                spins += 1;
                assert!(spins < 10_000, "decoder never caught up");
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            out.push(pair);
        }
        out
    }

    fn temp_wav(name: &str, frames: u32, rate: u32) -> PathBuf {
        let path = std::env::temp_dir().join(format!("riars-{name}-{}.wav", std::process::id()));
        write_wav(&path, frames, rate);
        path
    }

    #[test]
    fn streamed_plays_the_same_samples_as_memory() {
        // 3 s at 8 kHz: bigger than the prefill, so reading overlaps decoding.
        let path = temp_wav("stream-same", 24_000, 8000);
        for step in [1.0, 0.7, 1.3] {
            let mut memory = TrackPlayer::open(&path, 8000, LoadMode::Memory).unwrap();
            let mut streamed = TrackPlayer::open(&path, 8000, LoadMode::Stream).unwrap();
            assert!(streamed.is_streamed() && !memory.is_streamed());
            assert_eq!(streamed.duration(), memory.duration());
            let a = read(&mut memory, 50_000, step);
            let b = read(&mut streamed, 50_000, step);
            assert_eq!(a.len(), b.len(), "step {step}");
            for (i, (x, y)) in a.iter().zip(&b).enumerate() {
                assert!((x.0 - y.0).abs() < 1e-6 && (x.1 - y.1).abs() < 1e-6, "step {step}, sample {i}: {x:?} vs {y:?}");
            }
            assert!(streamed.is_finished());
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn streamed_seek() {
        let path = temp_wav("stream-seek", 24_000, 8000);
        let mut p = TrackPlayer::open(&path, 8000, LoadMode::Stream).unwrap();
        read(&mut p, 100, 1.0);
        p.seek(Duration::from_millis(2000)); // frame 16000 -> value 0
        assert_eq!(p.sample(0), 0.0); // silent until the decoder answers
        let after = read(&mut p, 3, 1.0);
        let expect = |frame: u32| (frame % 1000) as f32 / 32768.0;
        assert!((after[0].0 - expect(16_000)).abs() < 1e-6, "{after:?}");
        assert!((after[2].0 - expect(16_002)).abs() < 1e-6, "{after:?}");
        assert_eq!(p.position(), Duration::from_secs_f64(16_003.0 / 8000.0));
        // Back to the start, then play to the end.
        p.seek(Duration::ZERO);
        let all = read(&mut p, 100_000, 1.0);
        assert_eq!(all.len(), 24_000);
        assert!(p.is_finished());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn auto_mode_streams_only_long_tracks() {
        let path = temp_wav("stream-auto", 8000, 8000);
        assert!(!TrackPlayer::from_file(&path, 8000).unwrap().is_streamed());
        std::fs::remove_file(&path).unwrap();
    }
}
