use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    Mp3,
    Ogg,
    Aac,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcecastConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    pub password: String,
    pub bitrate: u32,
    pub format: StreamFormat,
    pub station_name: String,
    pub station_description: String,
    pub station_genre: String,
    pub station_url: String,
}

impl Default for IcecastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream".to_string(),
            username: "source".to_string(),
            password: String::new(),
            bitrate: 128,
            format: StreamFormat::Mp3,
            station_name: "RIARS Station".to_string(),
            station_description: String::new(),
            station_genre: String::new(),
            station_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StreamerState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

pub struct IcecastStreamer {
    config: IcecastConfig,
    state: StreamerState,
}

impl IcecastStreamer {
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            state: StreamerState::Disconnected,
        }
    }

    pub fn connect(&mut self) -> Result<(), String> {
        log::info!(
            "Connecting to Icecast: {}:{}{}",
            self.config.host,
            self.config.port,
            self.config.mount
        );
        self.state = StreamerState::Connecting;
        // TODO: open TCP socket, send HTTP SOURCE headers, authenticate
        self.state = StreamerState::Connected;
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.state = StreamerState::Disconnected;
        log::info!("Disconnected from Icecast");
    }

    pub fn send_audio(&mut self, _samples: &[f32]) -> Result<(), String> {
        if self.state != StreamerState::Connected {
            return Err("Not connected to Icecast server".to_string());
        }
        // TODO: encode samples and send over socket
        Ok(())
    }

    pub fn state(&self) -> StreamerState {
        self.state
    }

    pub fn config(&self) -> &IcecastConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: IcecastConfig) {
        self.config = config;
    }
}
