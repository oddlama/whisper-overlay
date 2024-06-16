use clap::Parser;
use color_eyre::eyre::Result;

mod app;
mod cli;
mod shortcuts;

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = cli::Cli::parse();

    app::launch_app()?;
    Ok(())
}
