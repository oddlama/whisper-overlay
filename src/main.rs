use clap::Parser;
use cli::ConnectionOpts;
use color_eyre::eyre::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::json;
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

mod app;
mod cli;
mod shortcuts;

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
}

async fn send_message<S: AsyncWriteExt + std::marker::Unpin>(
    socket: &mut S,
    value: serde_json::Value,
) -> Result<()> {
    let json_str = value.to_string();
    let data = json_str.as_bytes();
    let message_length = (data.len() as u32).to_be_bytes();

    socket.write_all(&message_length).await?;
    socket.write_all(data).await?;
    socket.flush().await?;

    Ok(())
}

async fn recv_message<S: AsyncReadExt + std::marker::Unpin>(
    socket: &mut S,
) -> Result<serde_json::Value> {
    let mut length_buf = [0u8; 4];
    socket.read_exact(&mut length_buf).await?;

    let message_length = u32::from_be_bytes(length_buf) as usize;
    let mut data_buf = vec![0u8; message_length];
    socket.read_exact(&mut data_buf).await?;

    let json_str = String::from_utf8(data_buf)?;
    let value = serde_json::from_str(&json_str)?;
    Ok(value)
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
            socket_write.write_all(&data).await.unwrap();
        }
    });

    loop {
        let message = recv_message(&mut socket_read).await?;
        println!("{:?}", message);
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = cli::Cli::parse();

    match args.command {
        cli::Command::WaybarStatus { connection_opts: _ } => {}
        cli::Command::Overlay { connection_opts: _ } => {
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
