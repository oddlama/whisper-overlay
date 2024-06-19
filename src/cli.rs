use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Command {
    WaybarStatus {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Overlay {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,

        /// An optional stylesheet for the overlay, which replaces the internal style.
        #[arg(short, short, long, default_value=None)]
        style: Option<PathBuf>,

        /// Specifies the hotkey to activate voice input. You can use any
        /// key or button name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html)
        #[arg(long, default_value="KEY_RIGHTCTRL")]
        hotkey: String,
    },
    Load {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Unload {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ConnectionOpts {
    /// The address of the the whisper streaming instance (host:port)
    #[clap(short, long)]
    pub address: String,
}
