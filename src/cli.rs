use clap::{Parser, Subcommand, Args};
#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(subcommand)]
    command: Command,
}


#[derive(Debug, Subcommand)]
pub enum Command {
    Status {
    },
    Load {
    },
    Unload {
    },
    Stream {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
}

#[derive(Debug, Args)]
pub struct ConnectionOpts {
    /// The host of the whisper streaming instance
    #[clap(long)]
    pub host: String,

    /// The port of the whisper streaming instance
    #[clap(long)]
    pub port: u16,
}
