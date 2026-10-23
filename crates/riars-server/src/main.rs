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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    set_language(&cli.lang);

    log::info!("{} v{}", t("server_starting"), env!("CARGO_PKG_VERSION"));

    let engine = AudioEngine::new().expect(&t("engine_init_failed"));
    let mut library = MediaLibrary::open(&cli.db_path).expect(&t("library_init_failed"));
    let scheduler = Scheduler::new();

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
        };
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
            engine: Mutex::new(engine),
            library: Mutex::new(library),
            scheduler: Mutex::new(scheduler),
        });
        riars_api::serve(state, cli.port).await?;
    }

    Ok(())
}
