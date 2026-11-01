//! RDS for an external encoder: the station's settings, and the texts (PS, RadioText,
//! RT+) made from what is on air. Sending them to the encoder (UECP or ASCII commands
//! over TCP, UDP, a serial port or a file) lives in `riars-rds`; this part does no I/O
//! besides loading and saving the settings.

pub mod charset;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::playlist::playlist::ItemKind;

/// Longest PS, in characters.
pub const PS_LEN: usize = 8;
/// Longest RadioText, in characters.
pub const RT_LEN: usize = 64;
/// Most encoders a station sends to at once.
pub const MAX_OUTPUTS: usize = 8;
/// Longest manual RadioText, in minutes (one day).
pub const MAX_MANUAL_MINUTES: u32 = 24 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RdsConfig {
    /// Send to the outputs. Off, nothing is sent and connections are closed.
    pub enabled: bool,
    /// Programme Identification: 4 hex digits, assigned with the licence.
    pub pi: String,
    /// Programme Service name, up to 8 characters, sent as is (no scrolling: many
    /// countries forbid a changing PS).
    pub ps: String,
    /// Programme Type, 0–31.
    pub pty: u8,
    /// Traffic Programme: the station carries traffic announcements.
    pub tp: bool,
    /// Music/Speech flag: music when true.
    pub music: bool,
    /// RadioText for songs; `{artist}` and `{title}` are replaced.
    pub rt_template: String,
    /// RadioText for everything that is not a song (jingles, spots...) and while
    /// nothing plays. Empty sends the PS.
    pub station_text: String,
    /// RadioText while the microphone is on air. Empty keeps the song's text.
    pub live_text: String,
    pub outputs: Vec<RdsOutput>,
}

impl Default for RdsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pi: "C000".to_string(),
            ps: "RIARS".to_string(),
            pty: 0,
            tp: false,
            music: true,
            rt_template: "{artist} - {title}".to_string(),
            station_text: String::new(),
            live_text: String::new(),
            outputs: Vec::new(),
        }
    }
}

/// One encoder (or file) the texts go to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RdsOutput {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub transport: RdsTransport,
    pub protocol: RdsProtocol,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RdsTransport {
    /// A connection kept open to the encoder, reopened when it drops.
    Tcp { host: String, port: u16 },
    Udp { host: String, port: u16 },
    /// A serial port: `COM3`, `/dev/ttyUSB0`... 8 data bits, no parity, 1 stop bit.
    Serial { port: String, baud: u32 },
    /// A text file rewritten on every change, for encoders or software that read one.
    File { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RdsProtocol {
    /// EBU UECP (SPB 490): PI, PS, PTY, TP, MS and RadioText in one frame. Site and
    /// encoder addresses 0 reach every encoder on the line.
    Uecp {
        #[serde(default)]
        site: u16,
        #[serde(default)]
        encoder: u8,
    },
    /// Text commands from a template, for encoders with their own command set. See
    /// `riars_rds::ascii` for the placeholders.
    Ascii { template: String, charset: RdsCharset },
}

/// How the characters of an ASCII command are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdsCharset {
    /// The RDS table, one byte per character (accents as the radio shows them).
    Rds,
    /// Plain ASCII, accents taken off.
    Ascii,
    /// UTF-8, for files and software rather than encoders.
    Utf8,
}

/// The ASCII template most encoders understand: `PS=` and `RT=` lines.
pub const DEFAULT_ASCII_TEMPLATE: &str = "PS={ps}\\r\\nRT={rt}\\r\\n";

impl RdsConfig {
    /// The PI as a number; the settings were checked by `validate`.
    pub fn pi_code(&self) -> u16 {
        u16::from_str_radix(&self.pi, 16).unwrap_or(0)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.pi.len() != 4 || u16::from_str_radix(&self.pi, 16).is_err() {
            return Err(format!("PI must be 4 hex digits, like C201 (got \"{}\")", self.pi));
        }
        let ps = self.ps.trim().chars().count();
        if ps == 0 || ps > PS_LEN {
            return Err(format!("PS must have 1 to {PS_LEN} characters"));
        }
        if self.pty > 31 {
            return Err("PTY must be between 0 and 31".to_string());
        }
        for (name, text) in [("rt_template", &self.rt_template), ("station_text", &self.station_text), ("live_text", &self.live_text)] {
            if text.chars().count() > 256 {
                return Err(format!("{name} is too long (256 characters at most)"));
            }
        }
        if self.outputs.len() > MAX_OUTPUTS {
            return Err(format!("At most {MAX_OUTPUTS} RDS outputs"));
        }
        for output in &self.outputs {
            output.validate().map_err(|e| format!("Output \"{}\": {e}", output.name))?;
        }
        Ok(())
    }
}

impl RdsOutput {
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 64 {
            return Err("the name must have 1 to 64 characters".to_string());
        }
        match &self.transport {
            RdsTransport::Tcp { host, port } | RdsTransport::Udp { host, port } => {
                if host.trim().is_empty() || host.len() > 253 || host.contains(char::is_whitespace) {
                    return Err("bad host".to_string());
                }
                if *port == 0 {
                    return Err("the port cannot be 0".to_string());
                }
            }
            RdsTransport::Serial { port, baud } => {
                if !is_serial_port(port) {
                    return Err("give the serial port, like COM3 or /dev/ttyUSB0".to_string());
                }
                if !(1200..=230_400).contains(baud) {
                    return Err("the speed must be between 1200 and 230400 baud".to_string());
                }
            }
            RdsTransport::File { path } => validate_file(path)?,
        }
        match &self.protocol {
            RdsProtocol::Uecp { site, encoder } => {
                if *site > 1023 || *encoder > 63 {
                    return Err("UECP site goes up to 1023 and encoder up to 63".to_string());
                }
                if matches!(self.transport, RdsTransport::File { .. }) {
                    return Err("a file takes ASCII text, not UECP".to_string());
                }
            }
            RdsProtocol::Ascii { template, .. } => {
                if template.is_empty() || template.len() > 1024 {
                    return Err("the template must have 1 to 1024 characters".to_string());
                }
            }
        }
        Ok(())
    }
}

/// Serial port names only (`COM3`, `\\.\COM12`, `/dev/ttyUSB0`, `/dev/serial/by-id/...`):
/// the name comes from the API, and opening any other device is not wanted.
fn is_serial_port(port: &str) -> bool {
    let com = port.strip_prefix(r"\\.\").unwrap_or(port);
    // `get`, not slicing: the name may have characters of more than one byte.
    let windows = com.len() > 3
        && com.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("COM"))
        && com.get(3..).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()));
    let unix = ["/dev/tty", "/dev/cu.", "/dev/serial/", "/dev/rfcomm"].iter().any(|p| port.starts_with(p))
        && !port.contains("..")
        && port.len() <= 128;
    windows || unix
}

/// Anyone with the API token can set the path, so keep it to a text file and never
/// follow a link to somewhere else.
fn validate_file(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("the file path must be absolute".to_string());
    }
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
        return Err("the file must end in .txt".to_string());
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("the file's folder does not exist".to_string());
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => Err("the path is not a plain file".to_string()),
        _ => Ok(()),
    }
}

/// Settings saved by `save`; defaults when there is no file yet.
pub fn load(path: &Path) -> Result<RdsConfig, String> {
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).map_err(|e| format!("Invalid RDS settings {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RdsConfig::default()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}

pub fn save(config: &RdsConfig, path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

/// The item on air, as the RDS texts need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnAirInfo {
    pub artist: String,
    pub title: String,
    pub kind: ItemKind,
}

/// What the texts are made from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RdsInput {
    pub on_air: Option<OnAirInfo>,
    pub mic_on: bool,
    /// RadioText typed by the operator (or an agent); it wins over everything else.
    pub manual: Option<String>,
}

/// RT+ content types used here (RT+ specification, table of content types).
pub const RTPLUS_TITLE: u8 = 1;
pub const RTPLUS_ARTIST: u8 = 4;

/// An RT+ tag: where in the RadioText a piece (title, artist) is, in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtPlusTag {
    pub content_type: u8,
    pub start: u8,
    pub len: u8,
}

/// The texts to send, in Unicode; outputs convert them to their character set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdsTexts {
    /// Always 8 characters, padded with spaces.
    pub ps: String,
    /// Up to 64 characters.
    pub rt: String,
    pub rt_plus: Vec<RtPlusTag>,
}

pub fn compose(config: &RdsConfig, input: &RdsInput) -> RdsTexts {
    let ps: String = config.ps.trim().chars().take(PS_LEN).collect();
    let ps = format!("{ps:<PS_LEN$}");
    let station = || {
        let text = config.station_text.trim();
        RdsTexts { ps: ps.clone(), rt: clip(if text.is_empty() { ps.trim() } else { text }), rt_plus: Vec::new() }
    };
    if let Some(text) = input.manual.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        return RdsTexts { ps: ps.clone(), rt: clip(text), rt_plus: Vec::new() };
    }
    if input.mic_on && !config.live_text.trim().is_empty() {
        return RdsTexts { ps: ps.clone(), rt: clip(config.live_text.trim()), rt_plus: Vec::new() };
    }
    let Some(on_air) = &input.on_air else { return station() };
    if !matches!(on_air.kind, ItemKind::Music | ItemKind::Program) {
        return station();
    }
    let (artist, title) = (on_air.artist.trim(), on_air.title.trim());
    if title.is_empty() {
        return station();
    }
    if artist.is_empty() {
        let rt = clip(title);
        let tag = RtPlusTag { content_type: RTPLUS_TITLE, start: 0, len: rt.chars().count() as u8 };
        return RdsTexts { ps, rt, rt_plus: vec![tag] };
    }
    let (rt, rt_plus) = fill(&config.rt_template, artist, title);
    if rt.trim().is_empty() {
        return station();
    }
    RdsTexts { ps, rt, rt_plus }
}

fn clip(text: &str) -> String {
    text.chars().take(RT_LEN).collect()
}

/// Fill the template, noting where the artist and the title land, then cut to 64.
fn fill(template: &str, artist: &str, title: &str) -> (String, Vec<RtPlusTag>) {
    let mut out = String::new();
    let mut len = 0usize;
    let mut tags = Vec::new();
    let mut rest = template;
    while !rest.is_empty() {
        let found = [("{artist}", artist, RTPLUS_ARTIST), ("{title}", title, RTPLUS_TITLE)]
            .into_iter()
            .find(|(key, _, _)| rest.starts_with(key));
        if let Some((key, value, content_type)) = found {
            let count = value.chars().count();
            tags.push((content_type, len, count));
            out.push_str(value);
            len += count;
            rest = &rest[key.len()..];
        } else {
            let c = rest.chars().next().unwrap_or(' ');
            out.push(c);
            len += 1;
            rest = &rest[c.len_utf8()..];
        }
    }
    let rt = clip(out.trim_end());
    let total = rt.chars().count();
    let tags = tags
        .into_iter()
        .filter(|&(_, start, count)| start < total && count > 0)
        .map(|(content_type, start, count)| RtPlusTag {
            content_type,
            start: start as u8,
            len: count.min(total - start) as u8,
        })
        .collect();
    (rt, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(artist: &str, title: &str) -> RdsInput {
        RdsInput {
            on_air: Some(OnAirInfo { artist: artist.into(), title: title.into(), kind: ItemKind::Music }),
            ..RdsInput::default()
        }
    }

    fn config() -> RdsConfig {
        RdsConfig { ps: "RENOU".into(), station_text: "Radio Renou 98.5".into(), ..RdsConfig::default() }
    }

    #[test]
    fn a_song_fills_the_template_with_rt_plus() {
        let texts = compose(&config(), &song("Rosalía", "Malamente"));
        assert_eq!(texts.ps, "RENOU   ");
        assert_eq!(texts.rt, "Rosalía - Malamente");
        assert_eq!(texts.rt_plus, vec![
            RtPlusTag { content_type: RTPLUS_ARTIST, start: 0, len: 7 },
            RtPlusTag { content_type: RTPLUS_TITLE, start: 10, len: 9 },
        ]);
    }

    #[test]
    fn long_texts_are_cut_and_their_tags_clipped() {
        let title = "x".repeat(80);
        let texts = compose(&config(), &song("Artist", &title));
        assert_eq!(texts.rt.chars().count(), RT_LEN);
        assert_eq!(texts.rt_plus[1], RtPlusTag { content_type: RTPLUS_TITLE, start: 9, len: 55 });
    }

    #[test]
    fn jingles_and_silence_show_the_station_text() {
        let mut input = song("", "ID 1");
        input.on_air.as_mut().unwrap().kind = ItemKind::Jingle;
        assert_eq!(compose(&config(), &input).rt, "Radio Renou 98.5");
        assert_eq!(compose(&config(), &RdsInput::default()).rt, "Radio Renou 98.5");
        let bare = RdsConfig { station_text: String::new(), ..config() };
        assert_eq!(compose(&bare, &RdsInput::default()).rt, "RENOU");
    }

    #[test]
    fn manual_text_wins_then_the_microphone() {
        let config = RdsConfig { live_text: "En directo".into(), ..config() };
        let mut input = song("A", "B");
        input.mic_on = true;
        assert_eq!(compose(&config, &input).rt, "En directo");
        input.manual = Some("Llámanos al 900 100 100".into());
        assert_eq!(compose(&config, &input).rt, "Llámanos al 900 100 100");
        assert!(compose(&config, &input).rt_plus.is_empty());
    }

    #[test]
    fn a_song_without_artist_is_tagged_as_title() {
        let texts = compose(&config(), &song("", "Programa de la mañana"));
        assert_eq!(texts.rt, "Programa de la mañana");
        assert_eq!(texts.rt_plus, vec![RtPlusTag { content_type: RTPLUS_TITLE, start: 0, len: 21 }]);
    }

    #[test]
    fn validation() {
        assert!(config().validate().is_ok());
        assert!(RdsConfig { pi: "C2G1".into(), ..config() }.validate().is_err());
        assert!(RdsConfig { ps: "TOO LONG!".into(), ..config() }.validate().is_err());
        assert!(RdsConfig { pty: 32, ..config() }.validate().is_err());
        let output = |transport, protocol| RdsConfig {
            outputs: vec![RdsOutput { name: "Encoder".into(), enabled: true, transport, protocol }],
            ..config()
        };
        let uecp = RdsProtocol::Uecp { site: 0, encoder: 0 };
        assert!(output(RdsTransport::Tcp { host: "192.168.1.50".into(), port: 7000 }, uecp.clone()).validate().is_ok());
        assert!(output(RdsTransport::Tcp { host: "".into(), port: 7000 }, uecp.clone()).validate().is_err());
        assert!(output(RdsTransport::Serial { port: "COM3".into(), baud: 9600 }, RdsProtocol::Uecp { site: 1024, encoder: 0 }).validate().is_err());
        for good in ["COM3", "com12", r"\\.\COM10", "/dev/ttyUSB0", "/dev/serial/by-id/usb-FTDI-if00"] {
            assert!(output(RdsTransport::Serial { port: good.into(), baud: 9600 }, uecp.clone()).validate().is_ok(), "{good}");
        }
        for bad in ["", "COM", "COMx", "Cñ1", "/dev/sda", "/dev/tty/../sda", "/etc/passwd"] {
            assert!(output(RdsTransport::Serial { port: bad.into(), baud: 9600 }, uecp.clone()).validate().is_err(), "{bad}");
        }
        let dir = std::env::temp_dir();
        let file = |name: &str| RdsTransport::File { path: dir.join(name).to_string_lossy().into_owned() };
        let ascii = RdsProtocol::Ascii { template: DEFAULT_ASCII_TEMPLATE.into(), charset: RdsCharset::Utf8 };
        assert!(output(file("riars-rds.txt"), ascii.clone()).validate().is_ok());
        assert!(output(file("riars-rds.exe"), ascii.clone()).validate().is_err());
        assert!(output(file("riars-rds.txt"), uecp).validate().is_err());
        assert!(output(RdsTransport::File { path: "relative.txt".into() }, ascii).validate().is_err());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let config = RdsConfig {
            outputs: vec![RdsOutput {
                name: "Encoder".into(),
                enabled: true,
                transport: RdsTransport::Udp { host: "10.0.0.2".into(), port: 5000 },
                protocol: RdsProtocol::Ascii { template: DEFAULT_ASCII_TEMPLATE.into(), charset: RdsCharset::Rds },
            }],
            ..config()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<RdsConfig>(&json).unwrap(), config);
        let path = std::env::temp_dir().join(format!("riars-rds-{}.json", std::process::id()));
        save(&config, &path).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path).unwrap(), RdsConfig::default());
    }
}
