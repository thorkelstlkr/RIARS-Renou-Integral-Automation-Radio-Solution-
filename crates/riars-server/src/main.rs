use std::sync::{Arc, Mutex};

use clap::Parser;
use riars_api::AppState;
use riars_core::{set_language, t, AudioEngine, MediaLibrary, Scheduler, SUPPORTED_LANGUAGES};

#[derive(Parser)]
#[command(name = "riars-server")]
#[command(about = "RIARS — Renou Integral Automation Radio Solution (headless server)")]
struct Cli {
    /// Port for the REST API and WebSocket server
    #[arg(long, default_value_t = 3100)]
    port: u16,

    /// Address the API listens on. The default only accepts this machine; use
    /// 0.0.0.0 for the whole network, ideally behind a TLS proxy (the API is plain HTTP).
    #[arg(long, default_value = "127.0.0.1")]
    bind: std::net::IpAddr,

    /// Folder API and MCP clients may play, add, scan and tag files from (repeatable).
    /// Library folders and --scan always count; paths elsewhere are refused.
    #[arg(long = "media-root", value_name = "FOLDER")]
    media_roots: Vec<String>,

    /// Icecast server streaming may connect to, as host or host:port (repeatable).
    /// Without it, any server is allowed.
    #[arg(long = "stream-host", value_name = "HOST[:PORT]")]
    stream_hosts: Vec<String>,

    /// Browser origin allowed to call the API from another site (repeatable), e.g.
    /// https://panel.example.com. The built-in panel needs none.
    #[arg(long = "cors-origin", value_name = "ORIGIN")]
    cors_origins: Vec<String>,

    /// Run as MCP server over stdio (for AI agent integration)
    #[arg(long)]
    mcp_stdio: bool,

    /// Path to the media library database
    #[arg(long, default_value = "riars_library.db")]
    db_path: String,

    /// Token required by the REST API and WebSocket (`Authorization: Bearer <token>`).
    /// Defaults to the RIARS_API_TOKEN environment variable; if neither is set, a
    /// random token is generated once and kept in riars_api_token next to the database.
    #[arg(long)]
    api_token: Option<String>,

    /// Do not use a sound card: play on an internal clock (for servers that only
    /// stream to Icecast). Without a working sound card this happens anyway.
    #[arg(long)]
    no_sound_card: bool,

    /// On-air sound card by name (ignored with --no-sound-card)
    #[arg(long)]
    device: Option<String>,

    /// List the sound cards and exit
    #[arg(long)]
    list_devices: bool,

    /// Start fresh instead of resuming the last session (playlist, position,
    /// volume, crossfade, streaming).
    #[arg(long)]
    no_resume: bool,

    /// Folder to auto-scan on startup
    #[arg(long)]
    scan: Option<String>,

    /// Language for user-facing messages (en, es, fr, de, pt, ca)
    #[arg(long, default_value = "en", value_parser = parse_lang)]
    lang: String,
}

fn parse_lang(s: &str) -> Result<String, String> {
    if SUPPORTED_LANGUAGES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Unsupported language '{}'. Available: {}",
            s,
            SUPPORTED_LANGUAGES.join(", ")
        ))
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    set_language(&cli.lang);

    if cli.list_devices {
        let default = riars_core::audio::default_output_device();
        let devices = riars_core::audio::output_devices();
        if devices.is_empty() {
            println!("No output devices");
        } else {
            for device in devices {
                if default.as_deref() == Some(device.as_str()) {
                    println!("{device} (default)");
                } else {
                    println!("{device}");
                }
            }
        }
        return Ok(());
    }

    log::info!("{} v{}", t("server_starting"), env!("CARGO_PKG_VERSION"));

    let mut engine = if cli.no_sound_card {
        AudioEngine::new_headless()
    } else if let Some(device) = cli.device.as_deref() {
        AudioEngine::with_device(device)
    } else {
        AudioEngine::new()
    }
    .expect(&t("engine_init_failed"));
    log::info!("Audio output: {}", engine.output_device().unwrap_or("none"));
    let stream = engine.take_stream_tap().map(riars_stream::StreamService::new);
    let mut library = MediaLibrary::open(&cli.db_path).expect(&t("library_init_failed"));
    // The schedule lives next to the library database.
    let schedule_path = std::path::Path::new(&cli.db_path).with_file_name("riars_schedule.json");
    let scheduler = Scheduler::load(&schedule_path).unwrap_or_else(|e| {
        log::warn!("{e}; starting with an empty schedule");
        Scheduler::new()
    });
    log::info!("Schedule: {} ({} events)", schedule_path.display(), scheduler.events().len());
    let session_path = std::path::Path::new(&cli.db_path).with_file_name("riars_session.json");
    let session = Mutex::new(riars_core::session::SessionSaver::new(session_path.clone()));

    // --scan adds a library folder; every library folder is rescanned in the
    // background once the server is up, without holding the library lock.
    if let Some(ref folder) = cli.scan {
        if let Err(e) = library.add_folder(folder) {
            log::warn!("{} {folder}: {e}", t("scan_failed"));
        }
    }
    // Remote clients stay inside the media roots and the library folders.
    match library.set_media_roots(&cli.media_roots) {
        Ok(roots) if roots.is_empty() => log::info!("Media roots: the library folders only (see --media-root)"),
        Ok(roots) => log::info!("Media roots: {} and the library folders", roots.join(", ")),
        Err(e) => return Err(format!("--media-root: {e}").into()),
    }
    if let Some(stream) = &stream {
        stream.set_allowed_hosts(cli.stream_hosts.clone());
    }

    if cli.mcp_stdio {
        log::info!("{}", t("mcp_started"));
        let mcp_state = riars_mcp::McpState {
            engine: Arc::new(Mutex::new(engine)),
            library: Arc::new(Mutex::new(library)),
            scheduler: Arc::new(Mutex::new(scheduler)),
            stream,
            schedule_path,
            session,
            automation: Arc::new(Mutex::new(riars_core::automation::Automation::new())),
            analysis: riars_core::analysis::BatchAnalyzer::new(),
        };
        {
            let lookup_library = Arc::clone(&mcp_state.library);
            mcp_state.engine.lock().map_err(|e| e.to_string())?.set_loudness_lookup(Arc::new(
                move |path| {
                    lookup_library
                        .lock()
                        .ok()?
                        .loudness_by_path(path)
                        .ok()
                        .flatten()
                },
            ));
        }
        let scan_library = Arc::clone(&mcp_state.library);
        tokio::task::spawn_blocking(move || rescan_library(&scan_library));
        if !cli.no_resume {
            let engine = Arc::clone(&mcp_state.engine);
            let path = session_path.clone();
            let resumed =
                tokio::task::spawn_blocking(move || riars_core::session::resume_from_file(&engine, &path)).await?;
            if let (Some(config), Some(stream)) = (resumed, &mcp_state.stream) {
                if let Err(e) = stream.start(config) {
                    log::warn!("Cannot resume streaming: {e}");
                }
            }
        }
        riars_mcp::run_stdio(mcp_state).await?;
    } else {
        let addr = std::net::SocketAddr::new(cli.bind, cli.port);
        log::info!("{} {addr}", t("api_listening"));
        if !cli.bind.is_loopback() {
            log::warn!("The API is open to the network over plain HTTP: put a TLS proxy in front of it");
        }
        let api_token = match cli.api_token.clone().or_else(|| std::env::var("RIARS_API_TOKEN").ok()) {
            Some(token) if !token.trim().is_empty() => token,
            _ => stored_token(&std::path::Path::new(&cli.db_path).with_file_name("riars_api_token"))?,
        };
        let state = Arc::new(AppState {
            api_token,
            stream,
            schedule_path,
            session,
            engine: Mutex::new(engine),
            library: Mutex::new(library),
            scheduler: Mutex::new(scheduler),
            automation: Mutex::new(riars_core::automation::Automation::new()),
            automation_events: tokio::sync::broadcast::channel(64).0,
            analysis: riars_core::analysis::BatchAnalyzer::new(),
        });
        {
            let lookup_state = Arc::downgrade(&state);
            state.engine.lock().map_err(|e| e.to_string())?.set_loudness_lookup(Arc::new(
                move |path| {
                    let state = lookup_state.upgrade()?;
                    let library = state.library.lock().ok()?;
                    let loudness = library.loudness_by_path(path).ok().flatten();
                    loudness
                },
            ));
        }
        let scan_state = Arc::clone(&state);
        tokio::task::spawn_blocking(move || rescan_library(&scan_state.library));
        if !cli.no_resume {
            let resume_state = Arc::clone(&state);
            let path = session_path.clone();
            let resumed = tokio::task::spawn_blocking(move || {
                riars_core::session::resume_from_file(&resume_state.engine, &path)
            })
            .await?;
            if let (Some(config), Some(stream)) = (resumed, &state.stream) {
                if let Err(e) = stream.start(config) {
                    log::warn!("Cannot resume streaming: {e}");
                }
            }
        }
        riars_api::serve(state, addr, &cli.cors_origins).await?;
    }

    Ok(())
}

/// The token kept in `path`, generated there (readable by its owner only) the first
/// time. Its path is logged, never the token itself.
fn stored_token(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(token) = std::fs::read_to_string(path) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            log::info!("API token: read from {}", path.display());
            return Ok(token);
        }
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    riars_core::session::write_private(path, token.as_bytes())
        .map_err(|e| format!("Cannot save the API token in {}: {e}", path.display()))?;
    log::warn!("No --api-token given; generated one and saved it in {}", path.display());
    Ok(token)
}

/// Pick up files added to the library folders since the last run.
fn rescan_library(library: &Mutex<MediaLibrary>) {
    match riars_core::library::folders::rescan_all(library) {
        Ok(count) => log::info!("{}: {count} {}", t("auto_scanned"), t("tracks_added")),
        Err(e) => log::warn!("{}: {e}", t("scan_failed")),
    }
}
