//! Icecast streaming: reads the mixed audio from the engine's `StreamTap`, encodes it to
//! MP3 and sends it to an Icecast server as a source client, reconnecting on failure.

mod util;

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushNoGap, InterleavedPcm, Quality};
use riars_core::audio::StreamTap;
use riars_core::streaming::icecast::{EncoderQuality, IcecastConfig, StreamFormat};
use serde::Serialize;

use util::{base64_encode, url_encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StreamState {
    Disconnected,
    Connecting,
    Connected,
    /// Failed; retrying with backoff while the stream stays enabled.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamStatus {
    pub state: StreamState,
    /// `http://host:port/mount` listeners connect to.
    pub url: Option<String>,
    pub bytes_sent: u64,
    /// Unix time (s) of the current connection.
    pub connected_since: Option<u64>,
    pub last_error: Option<String>,
    pub title: Option<String>,
}

#[derive(Default)]
struct Control {
    /// `Some` while streaming is wanted.
    config: Option<IcecastConfig>,
    /// Bumped on every start/stop so the worker drops its connection.
    generation: u64,
    title: Option<String>,
    /// Settings of the last start, for `resume` (scheduled starts).
    last_config: Option<IcecastConfig>,
}

struct Inner {
    control: Mutex<Control>,
    status: Mutex<StreamStatus>,
}

/// Owns the streaming thread. Start, stop and retitle it from any thread.
pub struct StreamService {
    inner: Arc<Inner>,
    _thread: JoinHandle<()>,
}

impl StreamService {
    pub fn new(tap: StreamTap) -> Self {
        let inner = Arc::new(Inner {
            control: Mutex::new(Control::default()),
            status: Mutex::new(StreamStatus {
                state: StreamState::Disconnected,
                url: None,
                bytes_sent: 0,
                connected_since: None,
                last_error: None,
                title: None,
            }),
        });
        let worker = Arc::clone(&inner);
        let thread = std::thread::Builder::new()
            .name("riars-icecast".into())
            .spawn(move || run(worker, tap))
            .expect("cannot start streaming thread");
        Self { inner, _thread: thread }
    }

    /// Start streaming (or restart with a new config).
    pub fn start(&self, config: IcecastConfig) -> Result<(), String> {
        if config.format != StreamFormat::Mp3 {
            return Err("Only MP3 streaming is supported for now".to_string());
        }
        bitrate(config.bitrate)?;
        if !config.mount.starts_with('/') {
            return Err("mount must start with '/'".to_string());
        }
        let mut control = self.inner.control.lock().map_err(|e| e.to_string())?;
        control.last_config = Some(config.clone());
        control.config = Some(config);
        control.generation += 1;
        Ok(())
    }

    /// Start again with the settings of the last `start`.
    pub fn resume(&self) -> Result<(), String> {
        let last = self.inner.control.lock().map_err(|e| e.to_string())?.last_config.clone();
        match last {
            Some(config) => self.start(config),
            None => Err("No stream settings yet: start the stream once first".to_string()),
        }
    }

    pub fn stop(&self) {
        if let Ok(mut control) = self.inner.control.lock() {
            control.config = None;
            control.generation += 1;
        }
        // The worker closes the connection within a few ms; report it now so callers
        // do not read a stale "Connected" right after stopping.
        set_status(&self.inner, |s| {
            s.state = StreamState::Disconnected;
            s.connected_since = None;
        });
    }

    /// Song title shown to listeners ("Artist - Title").
    pub fn set_title(&self, title: &str) {
        if let Ok(mut control) = self.inner.control.lock() {
            control.title = Some(title.to_string());
        }
    }

    pub fn status(&self) -> StreamStatus {
        self.inner.status.lock().map(|s| s.clone()).unwrap_or_else(|e| e.into_inner().clone())
    }

    /// The active config, with the password blanked.
    pub fn config(&self) -> Option<IcecastConfig> {
        let control = self.inner.control.lock().ok()?;
        control.config.clone().map(|mut c| {
            c.password = String::new();
            c
        })
    }
}

fn bitrate(kbps: u32) -> Result<Bitrate, String> {
    Ok(match kbps {
        64 => Bitrate::Kbps64,
        96 => Bitrate::Kbps96,
        128 => Bitrate::Kbps128,
        160 => Bitrate::Kbps160,
        192 => Bitrate::Kbps192,
        256 => Bitrate::Kbps256,
        320 => Bitrate::Kbps320,
        other => return Err(format!("Unsupported bitrate {other}; use 64, 96, 128, 160, 192, 256 or 320")),
    })
}

fn set_status(inner: &Inner, f: impl FnOnce(&mut StreamStatus)) {
    if let Ok(mut status) = inner.status.lock() {
        f(&mut status);
    }
}

/// Worker loop: idle (draining the tap) while disabled, otherwise connect and stream,
/// retrying with backoff on errors.
fn run(inner: Arc<Inner>, mut tap: StreamTap) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let (config, generation) = match inner.control.lock() {
            Ok(c) => (c.config.clone(), c.generation),
            Err(_) => return,
        };
        let Some(config) = config else {
            set_status(&inner, |s| {
                s.state = StreamState::Disconnected;
                s.connected_since = None;
                s.url = None;
            });
            drain(&mut tap);
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };

        set_status(&inner, |s| {
            s.state = StreamState::Connecting;
            s.url = Some(format!("http://{}:{}{}", config.host, config.port, config.mount));
        });
        let result = connect(&config).and_then(|socket| {
            backoff = Duration::from_secs(1);
            stream(&inner, &mut tap, socket, &config, generation)
        });
        match result {
            Ok(()) => {} // stopped or reconfigured
            Err(e) => {
                log::warn!("Icecast stream error: {e}");
                set_status(&inner, |s| {
                    s.state = StreamState::Error;
                    s.connected_since = None;
                    s.last_error = Some(e);
                });
                // Wait before retrying, unless the stream is stopped or reconfigured.
                let until = Instant::now() + backoff;
                while Instant::now() < until && current_generation(&inner) == generation {
                    drain(&mut tap);
                    std::thread::sleep(Duration::from_millis(50));
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

fn current_generation(inner: &Inner) -> u64 {
    inner.control.lock().map(|c| c.generation).unwrap_or(0)
}

fn drain(tap: &mut StreamTap) {
    let available = tap.consumer.slots();
    if let Ok(chunk) = tap.consumer.read_chunk(available) {
        chunk.commit_all();
    }
}

/// Header values come from user config: strip CR/LF so they cannot inject headers.
fn header_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

fn connect(config: &IcecastConfig) -> Result<TcpStream, String> {
    let addr = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|e| format!("Cannot resolve {}: {e}", config.host))?
        .next()
        .ok_or_else(|| format!("Cannot resolve {}", config.host))?;
    let mut socket = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| format!("Cannot connect to {addr}: {e}"))?;
    socket.set_write_timeout(Some(Duration::from_secs(10))).ok();
    socket.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let auth = base64_encode(format!("{}:{}", config.username, config.password).as_bytes());
    let request = format!(
        "PUT {mount} HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Authorization: Basic {auth}\r\n\
         User-Agent: RIARS/{version}\r\n\
         Content-Type: audio/mpeg\r\n\
         Ice-Public: 0\r\n\
         Ice-Name: {name}\r\n\
         Ice-Description: {description}\r\n\
         Ice-Genre: {genre}\r\n\
         Ice-Url: {url}\r\n\
         Ice-Audio-Info: ice-bitrate={bitrate}\r\n\
         Expect: 100-continue\r\n\r\n",
        mount = header_value(&config.mount),
        host = header_value(&config.host),
        port = config.port,
        version = env!("CARGO_PKG_VERSION"),
        name = header_value(&config.station_name),
        description = header_value(&config.station_description),
        genre = header_value(&config.station_genre),
        url = header_value(&config.station_url),
        bitrate = config.bitrate,
    );
    socket
        .write_all(request.as_bytes())
        .map_err(|e| format!("Cannot send request: {e}"))?;

    let status_line = read_status_line(&mut socket)?;
    let code = status_line.split_whitespace().nth(1).unwrap_or("");
    match code {
        "100" | "200" => Ok(socket),
        "401" => Err("Icecast rejected the username or password".to_string()),
        "403" => Err(format!("Icecast refused the mount (already in use?): {status_line}")),
        _ => Err(format!("Unexpected Icecast reply: {status_line}")),
    }
}

/// Read the response head and return its first line.
fn read_status_line(socket: &mut TcpStream) -> Result<String, String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match socket.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => head.push(byte[0]),
            Err(e) => return Err(format!("No reply from Icecast: {e}")),
        }
        if head.len() > 8192 {
            return Err("Icecast reply too long".to_string());
        }
    }
    let text = String::from_utf8_lossy(&head);
    text.lines()
        .next()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Empty reply from Icecast".to_string())
}

fn new_encoder(sample_rate: u32, kbps: u32, quality: EncoderQuality) -> Result<Encoder, String> {
    let mut builder = Builder::new().ok_or("Cannot create MP3 encoder")?;
    builder.set_num_channels(2).map_err(|e| e.to_string())?;
    builder.set_sample_rate(sample_rate).map_err(|e| e.to_string())?;
    builder.set_brate(bitrate(kbps)?).map_err(|e| e.to_string())?;
    let quality = match quality {
        EncoderQuality::Fast => Quality::Ok,
        EncoderQuality::Balanced => Quality::Good,
        EncoderQuality::High => Quality::NearBest,
    };
    builder.set_quality(quality).map_err(|e| e.to_string())?;
    builder.build().map_err(|e| e.to_string())
}

/// Encode and send until the stream is stopped/reconfigured (Ok) or fails (Err).
fn stream(
    inner: &Inner,
    tap: &mut StreamTap,
    mut socket: TcpStream,
    config: &IcecastConfig,
    generation: u64,
) -> Result<(), String> {
    // Start from live audio, not from what piled up while connecting.
    drain(tap);
    let mut encoder = new_encoder(tap.sample_rate, config.bitrate, config.encoder_quality)?;
    let channels = tap.channels.max(1) as usize;
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok();
    set_status(inner, |s| {
        s.state = StreamState::Connected;
        s.connected_since = since;
        s.last_error = None;
    });
    log::info!("Streaming to http://{}:{}{}", config.host, config.port, config.mount);

    let mut pcm: Vec<i16> = Vec::new();
    let mut mp3: Vec<u8> = Vec::new();
    let mut sent_title: Option<String> = None;
    let mut encoded_any = false;
    loop {
        let (wanted, title) = match inner.control.lock() {
            Ok(c) => (c.generation == generation, c.title.clone()),
            Err(_) => (false, None),
        };
        if !wanted {
            // Flush LAME's last frames, if it got any audio at all: flushing an unused
            // encoder fails (and, with no buffer room, brought the process down).
            if encoded_any {
                mp3.clear();
                mp3.reserve(16 * 1024);
                if encoder.flush_to_vec::<FlushNoGap>(&mut mp3).is_ok() {
                    let _ = socket.write_all(&mp3);
                }
            }
            log::info!("Stopped streaming");
            return Ok(());
        }
        if title.is_some() && title != sent_title {
            sent_title = title.clone();
            send_metadata(config, title.unwrap_or_default());
            set_status(inner, |s| s.title = sent_title.clone());
        }

        // Encode about 100 ms at a time: fewer wakeups, and listeners buffer seconds anyway.
        let frames = tap.consumer.slots() / channels;
        if frames < tap.sample_rate as usize / 10 {
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }
        let chunk = tap
            .consumer
            .read_chunk(frames * channels)
            .map_err(|e| e.to_string())?;
        pcm.clear();
        let (first, second) = chunk.as_slices();
        let samples = first.iter().chain(second.iter());
        // Stereo out: mono is doubled, extra channels are dropped.
        let mut frame = Vec::with_capacity(channels);
        for &sample in samples {
            frame.push(sample);
            if frame.len() == channels {
                let left = frame[0];
                let right = if channels > 1 { frame[1] } else { frame[0] };
                pcm.push((left.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                pcm.push((right.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                frame.clear();
            }
        }
        chunk.commit_all();

        mp3.clear();
        mp3.reserve(mp3lame_encoder::max_required_buffer_size(pcm.len() / 2));
        encoder
            .encode_to_vec(InterleavedPcm(&pcm), &mut mp3)
            .map_err(|e| format!("MP3 encoding failed: {e}"))?;
        encoded_any = true;
        socket
            .write_all(&mp3)
            .map_err(|e| format!("Connection to Icecast lost: {e}"))?;
        let sent = mp3.len() as u64;
        set_status(inner, |s| s.bytes_sent += sent);
    }
}

/// Update the song title through Icecast's admin endpoint, off the streaming thread.
fn send_metadata(config: &IcecastConfig, title: String) {
    let config = config.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let addr = (config.host.as_str(), config.port)
                .to_socket_addrs()
                .map_err(|e| e.to_string())?
                .next()
                .ok_or("cannot resolve host")?;
            let mut socket = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            socket.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let auth = base64_encode(format!("{}:{}", config.username, config.password).as_bytes());
            let request = format!(
                "GET /admin/metadata?mount={}&mode=updinfo&song={} HTTP/1.0\r\n\
                 Host: {}:{}\r\nAuthorization: Basic {auth}\r\nUser-Agent: RIARS/{}\r\n\r\n",
                url_encode(&config.mount),
                url_encode(&title),
                header_value(&config.host),
                config.port,
                env!("CARGO_PKG_VERSION"),
            );
            socket.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
            read_status_line(&mut socket)
        })();
        match result {
            Ok(line) if line.contains(" 200") => log::info!("Stream title: {title}"),
            Ok(line) => log::warn!("Icecast refused the title update: {line}"),
            Err(e) => log::warn!("Cannot update the stream title: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_values_cannot_inject_lines() {
        assert_eq!(header_value("Radio\r\nX-Evil: 1"), "RadioX-Evil: 1");
    }

    #[test]
    fn only_lame_bitrates() {
        assert!(bitrate(128).is_ok());
        assert!(bitrate(127).is_err());
    }
}
