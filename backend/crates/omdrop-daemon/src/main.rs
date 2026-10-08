use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "omdropd", about = "Unprivileged OmarchyDrop session daemon")]
struct Args {
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Development-only simulated adapter. Never upgrades real hardware.
    #[arg(long, hide = true)]
    mock_hardware: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let args = Args::parse();
    let socket = args
        .socket
        .unwrap_or_else(omdrop_platform_linux::default_socket_path);
    omdrop_daemon::run(&socket, args.mock_hardware).await
}
