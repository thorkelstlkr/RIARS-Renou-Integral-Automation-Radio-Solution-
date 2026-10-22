use std::sync::{Arc, Mutex};

use clap::Parser;
use riars_api::AppState;
use riars_core::{AudioEngine, MediaLibrary, Scheduler};

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

    /// Folder to auto-scan on startup
    #[arg(long)]
    scan: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    log::info!("RIARS v{} starting...", env!("CARGO_PKG_VERSION"));

    let engine = AudioEngine::new().expect("Failed to initialize audio engine");
    let mut library =
        MediaLibrary::open(&cli.db_path).expect("Failed to initialize media library");
    let scheduler = Scheduler::new();

    if let Some(ref folder) = cli.scan {
        match library.scan_folder(folder) {
            Ok(count) => log::info!("Auto-scanned {folder}: {count} tracks added"),
            Err(e) => log::warn!("Failed to scan {folder}: {e}"),
        }
    }

    if cli.mcp_stdio {
        log::info!("Starting MCP server (stdio transport)");
        let mcp_state = riars_mcp::McpState {
            engine: Arc::new(Mutex::new(engine)),
            library: Arc::new(Mutex::new(library)),
            scheduler: Arc::new(Mutex::new(scheduler)),
        };
        riars_mcp::run_stdio(mcp_state).await?;
    } else {
        log::info!("Starting REST API on port {}", cli.port);
        let state = Arc::new(AppState {
            engine: Mutex::new(engine),
            library: Mutex::new(library),
            scheduler: Mutex::new(scheduler),
        });
        riars_api::serve(state, cli.port).await?;
    }

    Ok(())
}
