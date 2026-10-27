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

    /// Run as MCP server over stdio (for AI agent integration)
    #[arg(long)]
    mcp_stdio: bool,

    /// Path to the media library database
    #[arg(long, default_value = "riars_library.db")]
    db_path: String,

    /// Token required by the REST API and WebSocket (`Authorization: Bearer <token>`).
    /// Defaults to the RIARS_API_TOKEN environment variable; if neither is set, a
    /// random token is generated and printed at startup.
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

    if let Some(ref folder) = cli.scan {
        match library.scan_folder(folder) {
            Ok(count) => log::info!("{} {folder}: {count} {}", t("auto_scanned"), t("tracks_added")),
            Err(e) => log::warn!("{} {folder}: {e}", t("scan_failed")),
        }
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
        log::info!("{} 0.0.0.0:{}", t("api_listening"), cli.port);
        let api_token = match cli.api_token.or_else(|| std::env::var("RIARS_API_TOKEN").ok()) {
            Some(token) if !token.trim().is_empty() => token,
            _ => {
                let token = uuid::Uuid::new_v4().simple().to_string();
                log::warn!("No --api-token given; generated one for this run: {token}");
                token
            }
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
        riars_api::serve(state, cli.port).await?;
    }

    Ok(())
}
