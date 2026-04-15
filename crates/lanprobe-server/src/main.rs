//! `lanprobe-server` — mode serveur headless de LanProbe.
//!
//! Binaire standalone : parse les args CLI et délègue à `lanprobe_server::start`.
//! La logique complète (routes, auth, WS, TLS) vit dans la lib pour que le
//! client desktop puisse aussi l'embarquer via un toggle Settings.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use lanprobe_server::{default_config_dir, start, StartConfig};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "lanprobe-server", version, about = "LanProbe headless server")]
struct Cli {
    /// Adresse d'écoute
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    /// Port HTTPS
    #[arg(long, default_value_t = 8443)]
    port: u16,
    /// Dossier de config (users.json + cert TLS). Défaut : ~/.config/lanprobe
    #[arg(long)]
    config_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lanprobe_server=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let config_dir = cli.config_dir.unwrap_or_else(default_config_dir);
    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;

    info!("config dir: {}", config_dir.display());
    info!("listening on https://{addr}");

    let handle = start(StartConfig { addr, config_dir, shared_state: None }).await?;

    if handle.addr.ip().is_unspecified() {
        warn!("bound to 0.0.0.0 — accessible from the whole LAN");
    }

    // Bloque jusqu'à Ctrl-C. Le lib gère déjà le graceful shutdown via un
    // oneshot interne ; ici on déclenche la coupure proprement.
    tokio::signal::ctrl_c().await?;
    info!("shutdown requested");
    handle.shutdown().await?;
    Ok(())
}
