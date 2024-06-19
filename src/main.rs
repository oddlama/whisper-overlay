use crate::cli::ConnectionOpts;
use crate::util::send_message;
use clap::Parser;
use color_eyre::eyre::Result;
use serde_json::json;
use std::sync::OnceLock;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

mod app;
mod cli;
mod hotkeys;
mod keyboard;
mod util;
mod waybar;

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
}

async fn main_action(action: &str, connection_opts: &ConnectionOpts) -> Result<()> {
    let mut socket = TcpStream::connect(&connection_opts.address).await?;
    println!("Connected to {}", connection_opts.address);

    send_message(&mut socket, json!({"mode": action})).await?;
    println!("Executed action {}", action);
    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = cli::Cli::parse();

    match args.command {
        cli::Command::WaybarStatus { connection_opts } => {
            runtime()
                .block_on(async move { waybar::main_waybar_status(&connection_opts).await })?;
        }
        command @ cli::Command::Overlay { .. } => {
            app::launch_app(command)?;
        }
        cli::Command::Load { connection_opts } => {
            runtime().block_on(async move { main_action("load", &connection_opts).await })?;
        }
        cli::Command::Unload { connection_opts } => {
            runtime().block_on(async move { main_action("unload", &connection_opts).await })?;
        }
    }

    Ok(())
}
