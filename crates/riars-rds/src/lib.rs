//! RDS output: sends the PS, RadioText and RT+ made by `riars_core::rds` to external
//! encoders, on its own thread, whenever what is on air changes and every 30 s (so an
//! encoder that restarted gets the texts back). A failed output is retried with
//! backoff; the others carry on.

pub mod ascii;
mod transport;
pub mod uecp;

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use riars_core::rds::{self, charset, OnAirInfo, RdsConfig, RdsInput, RdsProtocol, RdsTexts, RT_LEN};
use serde::{Deserialize, Serialize};

use transport::Link;

/// Texts are sent again this often even when nothing changed.
const REFRESH: Duration = Duration::from_secs(30);
const FIRST_RETRY: Duration = Duration::from_secs(5);
const LAST_RETRY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputState {
    /// RDS or this output is turned off.
    Off,
    /// Not sent yet.
    Waiting,
    /// The last send worked.
    Ok,
    /// The last send failed; retrying.
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputStatus {
    pub name: String,
    pub state: OutputState,
    /// Unix time (s) of the last successful send.
    pub last_sent: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RdsStatus {
    pub enabled: bool,
    /// What is being sent now (or would be, with RDS off).
    pub texts: RdsTexts,
    /// RadioText set by hand, while it lasts.
    pub manual: Option<String>,
    /// Unix time (s) when the manual text ends; `None` keeps it until cleared.
    pub manual_until: Option<u64>,
    pub outputs: Vec<OutputStatus>,
}

struct Manual {
    text: String,
    until: Option<SystemTime>,
}

struct Shared {
    config: RdsConfig,
    /// Bumped on every new config: the worker reopens its links.
    revision: u64,
    on_air: Option<OnAirInfo>,
    mic_on: bool,
    manual: Option<Manual>,
    /// Something changed since the worker last looked.
    dirty: bool,
    shutdown: bool,
}

struct Inner {
    shared: Mutex<Shared>,
    wake: Condvar,
    status: Mutex<RdsStatus>,
    /// Where the settings are saved; `None` keeps them in memory (tests).
    path: Option<PathBuf>,
}

/// Owns the RDS thread. Update it from the control loops; set it from any thread.
pub struct RdsService {
    inner: Arc<Inner>,
    _thread: Option<JoinHandle<()>>,
}

fn unix(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl RdsService {
    /// Start with the settings saved at `path` (defaults when there are none).
    pub fn new(path: Option<PathBuf>) -> Self {
        let config = path.as_deref().map_or_else(RdsConfig::default, |p| {
            rds::load(p).unwrap_or_else(|e| {
                log::warn!("{e}; RDS starts with default settings");
                RdsConfig::default()
            })
        });
        if config.enabled {
            log::info!("RDS: {} outputs", config.outputs.iter().filter(|o| o.enabled).count());
        }
        let status = RdsStatus {
            enabled: config.enabled,
            texts: rds::compose(&config, &RdsInput::default()),
            manual: None,
            manual_until: None,
            outputs: Vec::new(),
        };
        let inner = Arc::new(Inner {
            shared: Mutex::new(Shared {
                config,
                revision: 0,
                on_air: None,
                mic_on: false,
                manual: None,
                dirty: true,
                shutdown: false,
            }),
            wake: Condvar::new(),
            status: Mutex::new(status),
            path,
        });
        let worker = Arc::clone(&inner);
        let thread = std::thread::Builder::new()
            .name("riars-rds".into())
            .spawn(move || run(&worker))
            .map_err(|e| log::error!("Cannot start the RDS thread: {e}"))
            .ok();
        Self { inner, _thread: thread }
    }

    pub fn config(&self) -> RdsConfig {
        self.lock().config.clone()
    }

    /// Check, save and apply new settings.
    pub fn set_config(&self, config: RdsConfig) -> Result<(), String> {
        config.validate()?;
        if let Some(path) = &self.inner.path {
            rds::save(&config, path)?;
        }
        let mut shared = self.lock();
        shared.config = config;
        shared.revision += 1;
        self.wake(shared);
        Ok(())
    }

    /// What is on air now. Cheap when nothing changed: call it on every tick.
    pub fn update(&self, on_air: Option<OnAirInfo>, mic_on: bool) {
        let mut shared = self.lock();
        if shared.on_air != on_air || shared.mic_on != mic_on {
            shared.on_air = on_air;
            shared.mic_on = mic_on;
            self.wake(shared);
        }
    }

    /// Show `text` as RadioText instead of the song, for `minutes` (until cleared when
    /// `None`). An empty or missing text goes back to the automatic texts.
    pub fn set_text(&self, text: Option<&str>, minutes: Option<u32>) -> Result<(), String> {
        let text = text.map(str::trim).filter(|t| !t.is_empty());
        if let Some(text) = text {
            if text.chars().count() > RT_LEN {
                return Err(format!("RadioText has {RT_LEN} characters at most"));
            }
        }
        if minutes.is_some_and(|m| m == 0 || m > rds::MAX_MANUAL_MINUTES) {
            return Err(format!("minutes must be between 1 and {}", rds::MAX_MANUAL_MINUTES));
        }
        let mut shared = self.lock();
        shared.manual = text.map(|text| Manual {
            text: text.to_string(),
            until: minutes.map(|m| SystemTime::now() + Duration::from_secs(m as u64 * 60)),
        });
        self.wake(shared);
        Ok(())
    }

    /// The status as of the worker's last pass, with the texts as they are now.
    pub fn status(&self) -> RdsStatus {
        let mut status = self.inner.status.lock().map(|s| s.clone()).unwrap_or_else(|e| e.into_inner().clone());
        let shared = self.lock();
        status.enabled = shared.config.enabled;
        status.texts = rds::compose(&shared.config, &input(&shared));
        status.manual = shared.manual.as_ref().map(|m| m.text.clone());
        status.manual_until = shared.manual.as_ref().and_then(|m| m.until.map(unix));
        status
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.inner.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wake(&self, mut shared: std::sync::MutexGuard<'_, Shared>) {
        shared.dirty = true;
        drop(shared);
        self.inner.wake.notify_one();
    }
}

impl Drop for RdsService {
    fn drop(&mut self) {
        self.lock().shutdown = true;
        self.inner.wake.notify_one();
    }
}

fn input(shared: &Shared) -> RdsInput {
    RdsInput {
        on_air: shared.on_air.clone(),
        mic_on: shared.mic_on,
        manual: shared.manual.as_ref().map(|m| m.text.clone()),
    }
}

/// One output as the worker sees it.
struct Sender {
    link: Option<Link>,
    /// Texts the encoder has, after the last successful send.
    sent: Option<RdsTexts>,
    sent_at: Option<Instant>,
    retry_at: Option<Instant>,
    backoff: Duration,
    status: OutputStatus,
}

impl Sender {
    fn new(name: &str) -> Self {
        Self {
            link: None,
            sent: None,
            sent_at: None,
            retry_at: None,
            backoff: FIRST_RETRY,
            status: OutputStatus { name: name.to_string(), state: OutputState::Waiting, last_sent: None, last_error: None },
        }
    }

    fn off(&mut self) {
        self.link = None;
        self.sent = None;
        self.status.state = OutputState::Off;
    }
}

fn run(inner: &Inner) {
    let mut senders: Vec<Sender> = Vec::new();
    let mut revision = u64::MAX;
    loop {
        // Copy what is needed and let go of the lock before any I/O.
        let (config, input, new_revision) = {
            let mut shared = inner.shared.lock().unwrap_or_else(|e| e.into_inner());
            while !shared.dirty && !shared.shutdown {
                let (guard, wait) = inner.wake.wait_timeout(shared, Duration::from_secs(1)).unwrap_or_else(|e| e.into_inner());
                shared = guard;
                if wait.timed_out() {
                    break;
                }
            }
            if shared.shutdown {
                return;
            }
            shared.dirty = false;
            if shared.manual.as_ref().is_some_and(|m| m.until.is_some_and(|t| SystemTime::now() >= t)) {
                shared.manual = None;
            }
            (shared.config.clone(), input(&shared), shared.revision)
        };
        if new_revision != revision {
            senders = config.outputs.iter().map(|o| Sender::new(&o.name)).collect();
            revision = new_revision;
        }
        let texts = rds::compose(&config, &input);
        for (sender, output) in senders.iter_mut().zip(&config.outputs) {
            if !config.enabled || !output.enabled {
                sender.off();
                continue;
            }
            let now = Instant::now();
            let changed = sender.sent.as_ref() != Some(&texts);
            let stale = sender.sent_at.is_none_or(|at| now.duration_since(at) >= REFRESH);
            if !(changed || stale) || sender.retry_at.is_some_and(|at| now < at) {
                continue;
            }
            let toggle = sender.sent.as_ref().is_none_or(|sent| sent.rt != texts.rt);
            let payload = payload(&output.protocol, &config, &texts, toggle);
            let result = match &mut sender.link {
                Some(link) => link.send(&payload),
                None => Link::open(&output.transport).and_then(|mut link| {
                    let sent = link.send(&payload);
                    sender.link = Some(link);
                    sent
                }),
            };
            match result {
                Ok(()) => {
                    sender.sent = Some(texts.clone());
                    sender.sent_at = Some(now);
                    sender.retry_at = None;
                    sender.backoff = FIRST_RETRY;
                    sender.status.state = OutputState::Ok;
                    sender.status.last_sent = Some(unix(SystemTime::now()));
                    sender.status.last_error = None;
                }
                Err(e) => {
                    if sender.status.state != OutputState::Error {
                        log::warn!("RDS output \"{}\": {e}", output.name);
                    }
                    sender.link = None;
                    sender.sent = None;
                    sender.retry_at = Some(now + sender.backoff);
                    sender.backoff = (sender.backoff * 2).min(LAST_RETRY);
                    sender.status.state = OutputState::Error;
                    sender.status.last_error = Some(e.to_string());
                }
            }
        }
        if let Ok(mut status) = inner.status.lock() {
            status.outputs = senders.iter().map(|s| s.status.clone()).collect();
        }
    }
}

fn payload(protocol: &RdsProtocol, config: &RdsConfig, texts: &RdsTexts, toggle: bool) -> Vec<u8> {
    match protocol {
        RdsProtocol::Uecp { site, encoder } => {
            let mut elements = uecp::pi(config.pi_code());
            elements.extend(uecp::ps(&charset::encode(&texts.ps)));
            elements.extend(uecp::pty(config.pty));
            elements.extend(uecp::ta_tp(config.tp, false));
            elements.extend(uecp::ms(config.music));
            elements.extend(uecp::rt(&charset::encode(&texts.rt), toggle));
            uecp::frame(*site, *encoder, &elements)
        }
        RdsProtocol::Ascii { template, charset } => ascii::render(template, *charset, config, texts),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::{TcpListener, UdpSocket};

    use riars_core::playlist::playlist::ItemKind;
    use riars_core::rds::{RdsCharset, RdsOutput, RdsTransport, DEFAULT_ASCII_TEMPLATE};

    use super::*;

    fn song(title: &str) -> Option<OnAirInfo> {
        Some(OnAirInfo { artist: "Rosalía".into(), title: title.into(), kind: ItemKind::Music })
    }

    fn config(transport: RdsTransport, protocol: RdsProtocol) -> RdsConfig {
        RdsConfig {
            enabled: true,
            pi: "C201".into(),
            ps: "RENOU".into(),
            outputs: vec![RdsOutput { name: "Encoder".into(), enabled: true, transport, protocol }],
            ..RdsConfig::default()
        }
    }

    /// Read until `needle` shows up (or fail after a few seconds).
    fn read_until(stream: &mut impl Read, needle: &[u8]) -> Vec<u8> {
        let mut got = Vec::new();
        let mut buf = [0u8; 512];
        let deadline = Instant::now() + Duration::from_secs(10);
        while !got.windows(needle.len()).any(|w| w == needle) {
            assert!(Instant::now() < deadline, "never got {:?}; got {got:02X?}", String::from_utf8_lossy(needle));
            match stream.read(&mut buf) {
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => panic!("{e}"),
            }
        }
        got
    }

    #[test]
    fn uecp_over_tcp_follows_the_songs() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let service = RdsService::new(None);
        service.update(song("Malamente"), false);
        service
            .set_config(config(RdsTransport::Tcp { host: "127.0.0.1".into(), port }, RdsProtocol::Uecp { site: 0, encoder: 0 }))
            .unwrap();
        let (mut encoder, _) = listener.accept().unwrap();
        encoder.set_read_timeout(Some(Duration::from_millis(200))).unwrap();

        let first = read_until(&mut encoder, &charset::encode("Rosalía - Malamente"));
        assert_eq!(first[0], 0xFE);
        assert!(first.windows(5).any(|w| w == [0x01, 0, 0, 0xC2, 0x01]), "PI C201");
        assert!(first.windows(11).any(|w| w == b"\x02\x00\x00RENOU   "), "PS");

        service.update(song("Pienso en tu mirá"), false);
        read_until(&mut encoder, &charset::encode("Rosalía - Pienso en tu mirá"));

        service.set_text(Some("Llámanos: 900 100 100"), Some(5)).unwrap();
        read_until(&mut encoder, &charset::encode("Llámanos: 900 100 100"));
        let status = service.status();
        assert_eq!(status.manual.as_deref(), Some("Llámanos: 900 100 100"));
        assert!(status.manual_until.is_some());
        assert_eq!(status.outputs[0].state, OutputState::Ok);
    }

    #[test]
    fn ascii_over_udp() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let port = socket.local_addr().unwrap().port();
        let service = RdsService::new(None);
        service.update(song("Malamente"), false);
        service
            .set_config(config(
                RdsTransport::Udp { host: "127.0.0.1".into(), port },
                RdsProtocol::Ascii { template: DEFAULT_ASCII_TEMPLATE.into(), charset: RdsCharset::Ascii },
            ))
            .unwrap();
        let mut buf = [0u8; 512];
        let n = socket.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"PS=RENOU   \r\nRT=Rosalia - Malamente\r\n");
    }

    #[test]
    fn file_output_and_a_dead_encoder() {
        let path = std::env::temp_dir().join(format!("riars-rds-test-{}.txt", std::process::id()));
        let mut config = config(
            RdsTransport::File { path: path.to_string_lossy().into_owned() },
            RdsProtocol::Ascii { template: "{rt}".into(), charset: RdsCharset::Utf8 },
        );
        // Nothing listens on this port: the output fails, the file still gets written.
        let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        config.outputs.push(RdsOutput {
            name: "Dead".into(),
            enabled: true,
            transport: RdsTransport::Tcp { host: "127.0.0.1".into(), port: closed },
            protocol: RdsProtocol::Uecp { site: 0, encoder: 0 },
        });
        let service = RdsService::new(None);
        service.update(song("Malamente"), false);
        service.set_config(config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = service.status();
            if status.outputs.len() == 2 && status.outputs[0].state == OutputState::Ok && status.outputs[1].state == OutputState::Error {
                assert!(status.outputs[1].last_error.is_some());
                break;
            }
            assert!(Instant::now() < deadline, "{status:?}");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Rosalía - Malamente");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bad_settings_and_texts_are_refused() {
        let service = RdsService::new(None);
        assert!(service.set_config(RdsConfig { pi: "nope".into(), ..RdsConfig::default() }).is_err());
        assert!(service.set_text(Some(&"x".repeat(65)), None).is_err());
        assert!(service.set_text(Some("ok"), Some(0)).is_err());
        assert!(service.set_text(Some("ok"), None).is_ok());
        assert_eq!(service.status().texts.rt, "ok");
        service.set_text(None, None).unwrap();
        assert_eq!(service.status().texts.rt, "RIARS");
        assert!(!service.status().enabled);
    }
}
