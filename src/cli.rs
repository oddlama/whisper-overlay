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

        /// Forces the overlay to open on the specified monitor. Otherwise
        /// the monitor will be determined by your compositor.
        #[arg(short, long, default_value=None)]
        monitor: Option<String>,

        /// Use the specified sound input device. Uses the default device if not given.
        #[arg(short, long, default_value=None)]
        input: Option<String>,

        /// Specifies the hotkey to activate voice input.
        #[arg(long, default_value=None)]
        hotkey: Option<String>,
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
