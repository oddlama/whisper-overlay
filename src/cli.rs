use clap::{Parser, Subcommand, Args};
#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}


#[derive(Debug, Subcommand)]
pub enum Command {
    WaybarStatus {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Overlay {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Load {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Unload {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Stream {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
}

#[derive(Debug, Args)]
pub struct ConnectionOpts {
    /// The address of the the whisper streaming instance (host:port)
    #[clap(long)]
    pub address: String,
}
