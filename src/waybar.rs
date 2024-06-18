use crate::cli::ConnectionOpts;
use crate::util::{recv_message, send_message};
use color_eyre::eyre::Result;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpStream;

#[derive(Debug, Deserialize)]
struct StatusMessage {
    state: String,
    clients: u32,
    waiting: u32,
}

pub async fn main_waybar_status(connection_opts: &ConnectionOpts) -> Result<()> {
    let status_offline = json!({
        "text": "",
        "alt": "none",
        "tooltip": format!("Server offline ({})", connection_opts.address),
        "class": "disconnected",
        "state": "unloaded",
        "clients": 0,
        "waiting": 0,
    });

    let mut last_status = json!({});
    let mut update_status = |s: serde_json::Value| {
        if last_status != s {
            println!("{}", s);
            last_status = s;
        }
    };

    'outer: loop {
        let mut socket = match TcpStream::connect(&connection_opts.address).await {
            Ok(socket) => socket,
            Err(e) => {
                eprintln!("error: {e}");
                update_status(status_offline.clone());
                // FIXME: make this a setting
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Err(e) = send_message(&mut socket, json!({"mode": "status"})).await {
            eprintln!("error: {e}");
            update_status(status_offline.clone());
            // FIXME: make this a setting
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        loop {
            let message = match recv_message(&mut socket).await {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("error: {e}");
                    update_status(status_offline.clone());
                    // FIXME: make this a setting
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue 'outer;
                }
            };

            let message: StatusMessage = match serde_json::from_value(message) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("error: {e}");
                    update_status(status_offline.clone());
                    // FIXME: make this a setting
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue 'outer;
                }
            };

            let status = json!({
                "text": "",
                "alt": "none",
                "tooltip": format!("Connected ({})", connection_opts.address),
                "class": format!("connected-{}{}", message.state, (if message.waiting < message.clients { "-active" } else { "" })),
                "state": message.state,
                "clients": message.clients,
                "waiting": message.waiting,
            });

            update_status(status);
        }
    }
}
