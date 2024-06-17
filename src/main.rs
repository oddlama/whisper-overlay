use crate::cli::ConnectionOpts;
use crate::util::{recv_message, send_message};
use clap::Parser;
use color_eyre::eyre::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::json;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

mod app;
mod cli;
mod hotkeys;
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

async fn main_stream(connection_opts: &ConnectionOpts) -> Result<()> {
    let (mut socket_read, mut socket_write) = TcpStream::connect(&connection_opts.address)
        .await?
        .into_split();
    println!("Connected to {}", connection_opts.address);

    send_message(&mut socket_write, json!({"mode": "stream"})).await?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("No input device available"); // FIXME: AAAAAA
    println!("Input device: {}", device.name()?);

    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(16000),
        buffer_size: cpal::BufferSize::Default,
    };

    let err_fn = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(4096);
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[i16], _: &_| {
            let tx = tx.clone();
            runtime().block_on(async move {
                tx.send(bytemuck::cast_slice(data).to_vec()).await.unwrap();
            });
        },
        err_fn,
        None,
    )?;

    stream.play()?;

    runtime().spawn(async move {
        while let Some(data) = rx.recv().await {
            if let Err(_) = socket_write.write_all(&data).await {
                return;
            }
        }
    });

    loop {
        let message = recv_message(&mut socket_read).await?;
        println!("{}", message.to_string());
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = cli::Cli::parse();

    match args.command {
        cli::Command::WaybarStatus { connection_opts } => {
            runtime()
                .block_on(async move { waybar::main_waybar_status(&connection_opts).await })?;
        }
        cli::Command::Overlay {
            connection_opts: _,
            style,
            monitor,
            input,
            hotkey,
        } => {
            app::launch_app()?;
        }
        cli::Command::Load { connection_opts } => {
            runtime().block_on(async move { main_action("load", &connection_opts).await })?;
        }
        cli::Command::Unload { connection_opts } => {
            runtime().block_on(async move { main_action("unload", &connection_opts).await })?;
        }
        cli::Command::Stream { connection_opts } => {
            runtime().block_on(async move { main_stream(&connection_opts).await })?;
        }
    }

    Ok(())
}
