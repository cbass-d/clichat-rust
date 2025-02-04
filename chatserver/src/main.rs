mod room;
mod server;

use anyhow::{anyhow, Result};
use clap::Parser;
use log::{error, info};

use server::Server;

#[derive(Parser, Debug)]
struct ServerConfig {
    // Port to listening on
    #[arg(short, long)]
    port: u64,
}

// Set RUST_LOG if not already set
fn init_logging() {
    match std::env::var("RUST_LOG") {
        Ok(_) => {}
        Err(_) => {
            std::env::set_var("RUST_LOG", "info");
        }
    };
    env_logger::init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    // Get server config from cli arguments
    let config = ServerConfig::parse();

    info!("[*] Starting server");
    let mut server = Server::new(config.port);

    match server.start().await {
        Ok(()) => {}
        Err(e) => {
            let e = e.to_string();
            error!("[-] Server error: {e}");
            return Err(anyhow!("Server error: {e}"));
        }
    }

    server.close_server().await;

    info!("[+] Server shut down");

    Ok(())
}
