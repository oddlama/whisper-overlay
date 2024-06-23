use clap::Parser;
use color_eyre::eyre::Result;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

mod app;
mod cli;
mod hotkeys;
mod keyboard;
mod util;
mod waybar;
mod x;

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
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
    }

    Ok(())
}
