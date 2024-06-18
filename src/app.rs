use color_eyre::eyre::bail;
use color_eyre::eyre::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gdk::glib::ExitCode;
use gdk_wayland::prelude::*;
use gdk_wayland::WaylandSurface;
use gtk::cairo::{RectangleInt, Region};
use gtk::gdk::Display;
use gtk::{glib, Application, ApplicationWindow, Label};
use gtk::{prelude::*, CssProvider};
use gtk_layer_shell::{Layer, LayerShell};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::watch;

use crate::cli::Command;
use crate::cli::ConnectionOpts;
use crate::hotkeys::HotkeyEvent;
use crate::runtime;
use crate::util::recv_message;
use crate::util::send_message;

const APP_ID: &str = "org.oddlama.whisper-overlay";

#[derive(Debug)]
pub enum UiAction {
    ModelResult(serde_json::Value),
    Disconnected(Option<String>),
    Connecting,
    Connected,
    Locking,
    HideWindow,
    ShowWindow,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ConnectionState {
    Connected,
    Disconnected,
}

#[derive(Debug, Deserialize)]
pub struct Token {
    #[allow(unused)]
    begin: f32,
    #[allow(unused)]
    end: f32,
    word: String,
    probability: f32,
}

#[derive(Debug, Deserialize)]
pub struct TokenResult {
    tokens: Vec<Token>,
}

async fn connect_whisper(
    connection_opts: &ConnectionOpts,
) -> Result<(OwnedReadHalf, OwnedWriteHalf)> {
    println!("Connecting to {}", connection_opts.address);
    let (socket_read, mut socket_write) = TcpStream::connect(&connection_opts.address)
        .await?
        .into_split();
    println!("Connected to {}", connection_opts.address);

    send_message(&mut socket_write, json!({"mode": "stream"})).await?;
    Ok((socket_read, socket_write))
}

async fn handle_connection(
    mut connection_receiver: watch::Receiver<ConnectionState>,
    ui_sender: mpsc::Sender<UiAction>,
    connection_opts: ConnectionOpts,
) {
    ui_sender.send(UiAction::Disconnected(None)).await.unwrap();
    loop {
        if connection_receiver.changed().await.is_err() {
            break;
        }

        // Wait until we should connect
        let desired_state = *connection_receiver.borrow_and_update();
        match desired_state {
            ConnectionState::Connected => {
                ui_sender.send(UiAction::ShowWindow).await.unwrap();
            }
            ConnectionState::Disconnected => {
                ui_sender.send(UiAction::HideWindow).await.unwrap();
                continue;
            }
        }

        ui_sender.send(UiAction::Connecting).await.unwrap();
        let (mut socket_read, mut socket_write) = match connect_whisper(&connection_opts).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to connect to {}: {}", connection_opts.address, e);
                ui_sender
                    .send(UiAction::Disconnected(Some(e.to_string())))
                    .await
                    .unwrap();
                continue;
            }
        };

        match recv_message(&mut socket_read).await {
            Ok(message) => {
                if message.get("status") != Some(&json!("waiting for lock")) {
                    eprintln!(
                        "error: received unexpected message: {}",
                        message.to_string()
                    );
                    ui_sender
                        .send(UiAction::Disconnected(Some(message.to_string())))
                        .await
                        .unwrap();
                    continue;
                }
            }
            Err(e) => {
                eprintln!("could not receive message from socket: {}", e);
                ui_sender
                    .send(UiAction::Disconnected(Some(e.to_string())))
                    .await
                    .unwrap();
                continue;
            }
        }

        ui_sender.send(UiAction::Locking).await.unwrap();

        match recv_message(&mut socket_read).await {
            Ok(message) => {
                if message.get("status") != Some(&json!("lock acquired")) {
                    eprintln!(
                        "error: received unexpected message: {}",
                        message.to_string()
                    );
                    ui_sender
                        .send(UiAction::Disconnected(Some(message.to_string())))
                        .await
                        .unwrap();
                    continue;
                }
            }
            Err(e) => {
                eprintln!("could not receive message from socket: {}", e);
                ui_sender
                    .send(UiAction::Disconnected(Some(e.to_string())))
                    .await
                    .unwrap();
                continue;
            }
        }

        ui_sender.send(UiAction::Connected).await.unwrap();

        let (audio_tx, mut audio_rx) = watch::channel(());
        let (audio_shutdown_tx, audio_shutdown_rx) = std::sync::mpsc::channel();

        let bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
        let bytes_2 = bytes.clone();

        let audio_thread = std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .expect("No input device available");

            println!("Input device: {}", device.name().unwrap());

            let config = cpal::StreamConfig {
                channels: 1,
                sample_rate: cpal::SampleRate(16000),
                buffer_size: cpal::BufferSize::Default,
            };

            let err_fn = move |err| {
                eprintln!("an error occurred on the audio stream: {}", err);
            };

            let stream = device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        bytes_2
                            .lock()
                            .expect("Could not lock mutex to write audio data")
                            .extend_from_slice(bytemuck::cast_slice(data));
                        let _ = audio_tx.send(());
                    },
                    err_fn,
                    None,
                )
                .expect("Failed to build audio input stream");

            stream.play().expect("Failed to start audio stream");
            let _ = audio_shutdown_rx.recv();
        });

        loop {
            tokio::select! {
                message = recv_message(&mut socket_read) => {
                    match message {
                        Ok(message) => {
                            println!("{}", message.to_string());
                            ui_sender.send(UiAction::ModelResult(message)).await.unwrap();
                        },
                        Err(e) => {
                            eprintln!("could not receive message from socket: {}", e);
                            ui_sender
                                .send(UiAction::Disconnected(Some(e.to_string())))
                                .await
                                .unwrap();
                            break;
                        },
                    }
                }
                _ = audio_rx.changed() => {
                    audio_rx.mark_unchanged();
                    let data = std::mem::take(&mut *bytes.lock().expect("Could not lock mutex to read audio data"));
                    if let Err(e) = socket_write.write_all(&data).await {
                        eprintln!("could not write audio to socket: {}", e);
                        ui_sender
                            .send(UiAction::Disconnected(Some(e.to_string())))
                            .await
                            .unwrap();
                        break;
                    }
                }
                _ = connection_receiver.changed() => {
                    // Wait until we should disconnect
                    if *connection_receiver.borrow_and_update() == ConnectionState::Disconnected {
                        ui_sender.send(UiAction::HideWindow).await.unwrap();
                        break;
                    }
                }
            };
        }

        let _ = audio_shutdown_tx.send(());
        audio_thread.join().expect("Could not join audio_thread");

        ui_sender.send(UiAction::Disconnected(None)).await.unwrap();
    }
}

async fn handle_hotkey(
    mut hotkey_receiver: mpsc::Receiver<HotkeyEvent>,
    connection_sender: watch::Sender<ConnectionState>,
) {
    while let Some(event) = hotkey_receiver.recv().await {
        match event {
            HotkeyEvent::Pressed => {
                let _ = connection_sender.send(ConnectionState::Connected);
                // window will be hidden as soon as connection task is ready
            }
            HotkeyEvent::Released => {
                let _ = connection_sender.send(ConnectionState::Disconnected);
                // window will be hidden as soon as transcription task is finished
            }
        }
    }
}

pub fn launch_app(opts: Command) -> Result<()> {
    // Create a new application
    let app = Application::builder().application_id(APP_ID).build();

    // Connect to signals
    app.connect_startup(|_| load_css());
    app.connect_activate(move |app| build_ui(app, opts.clone()));

    // Run the application
    let exit_code = app.run_with_args::<&str>(&[]);
    if exit_code != ExitCode::SUCCESS {
        bail!("Could not launch gtk application: {:?}", exit_code);
    };

    Ok(())
}

fn load_css() {
    // Load the CSS file and add it to the provider
    let provider = CssProvider::new();
    provider.load_from_string(include_str!("style.css"));

    // Add the provider to the default screen
    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &Application, opts: Command) {
    let Command::Overlay {
        connection_opts, ..
    } = opts
    else {
        panic!("build_ui() got invalid command options");
    };

    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(5)
        .can_target(false)
        .can_focus(false)
        .focus_on_click(false)
        .build();
    main_box.add_css_class("main-box");

    let lines_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(5)
        .can_target(false)
        .can_focus(false)
        .focus_on_click(false)
        .build();
    main_box.append(&lines_box);

    let status_label = Label::builder()
        .halign(gtk::Align::Start)
        .can_target(false)
        .can_focus(false)
        .focus_on_click(false)
        .build();
    status_label.add_css_class("connection-status");
    main_box.append(&status_label);

    // Create a new window and present it
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Whisper Streaming Overlay")
        .decorated(false)
        .focus_on_click(false)
        .resizable(false)
        .can_target(false)
        .focusable(false)
        .default_width(1600)
        .default_height(0)
        .child(&main_box)
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(gtk_layer_shell::KeyboardMode::None);
    window.set_anchor(gtk_layer_shell::Edge::Bottom, true);
    window.set_margin(gtk_layer_shell::Edge::Bottom, 200);
    window.set_namespace("whisper-overlay");

    window.connect_realize(|window| {
        let wayland_surface = window.surface().and_downcast::<WaylandSurface>().unwrap();
        wayland_surface.set_input_region(&Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0)));

        // FIXME: disabled temporarily
        // window.set_visible(false);
    });

    //window.set_monitor();
    window.present();

    let (ui_sender, mut ui_receiver) = mpsc::channel(64);
    let (connection_sender, connection_receiver) = watch::channel(ConnectionState::Disconnected);
    let (hotkey_sender, hotkey_receiver) = mpsc::channel(64);

    // Spawn connection manager
    runtime().spawn(
        glib::clone!(@strong connection_receiver, @strong ui_sender => async move {
            handle_connection(connection_receiver, ui_sender, connection_opts.clone()).await;
        }),
    );

    // Spawn hotkey detector
    runtime().spawn(glib::clone!(@strong hotkey_sender => async move {
        crate::hotkeys::register(hotkey_sender).await;
    }));

    // Spawn hotkey processor
    runtime().spawn(glib::clone!(@strong connection_sender => async move {
        handle_hotkey(hotkey_receiver, connection_sender).await;
    }));

    // Ui updater
    glib::spawn_future_local(async move {
        let mut current_line = None;
        let mut current_markup = "".to_string();

        let gradient = colorgrad::CustomGradient::new()
            .html_colors(&[
                "#fe0000", "#fb3209", "#f74811", "#f35918", "#ef671e", "#ea7423", "#e67f28",
                "#e18a2c", "#dc9430", "#d79e34", "#d1a738", "#cbb03b", "#c4b93d", "#bcc23e",
                "#b2cc3d", "#a6d53a", "#97df36", "#82e92e", "#62f321", "#00ff00",
            ])
            .build()
            .expect("Could not build color gradient");

        while let Some(ui_action) = ui_receiver.recv().await {
            match ui_action {
                UiAction::ModelResult(value) => {
                    match serde_json::from_value::<TokenResult>(value) {
                        Ok(res) => {
                            for token in res.tokens {
                                if current_line.is_none() {
                                    let label = Label::builder()
                                        .wrap(true)
                                        .wrap_mode(gdk::pango::WrapMode::WordChar)
                                        .justify(gtk::Justification::Left)
                                        .halign(gtk::Align::Start)
                                        .can_target(false)
                                        .can_focus(false)
                                        .focus_on_click(false)
                                        .build();
                                    label.add_css_class("transcribed-text");
                                    lines_box.append(&label);

                                    current_line = Some(label.clone());
                                };

                                let markup = format!(
                                    "{current_markup}<span color=\"{fg}\">{text}</span>",
                                    fg = gradient.at(token.probability.into()).to_hex_string(),
                                    text = glib::markup_escape_text(if current_markup.is_empty() {
                                        &token.word.trim()
                                    } else {
                                        &token.word
                                    })
                                );

                                current_line.as_ref().unwrap().set_markup(&markup);
                                current_markup = markup;

                                if token.word.ends_with('.') {
                                    let label_2 = std::mem::take(&mut current_line.unwrap());
                                    let lines_box_2 = lines_box.clone();
                                    gtk::glib::timeout_add_local_once(
                                        Duration::from_millis(6000),
                                        move || {
                                            lines_box_2.remove(&label_2);
                                        },
                                    );

                                    current_line = None;
                                    current_markup = "".to_string();
                                }
                            }
                        }
                        Err(e) => eprintln!("error: ignoring invalid model result data: {e}"),
                    }
                }
                UiAction::HideWindow => {
                    //window.set_visible(false);
                    while let Some(c) = lines_box.last_child() {
                        lines_box.remove(&c);
                    }
                }
                UiAction::ShowWindow => {
                    //window.set_visible(true);
                }
                UiAction::Disconnected(reason) => {
                    let mut message = "<span color='gray'></span> Disconnected".to_string();
                    if let Some(reason) = reason {
                        message += &format!(" <span color='gray'>{}</span>", reason);
                    }
                    status_label.set_markup(&message);
                }
                UiAction::Connecting => {
                    status_label.set_markup("<span color='yellow'></span> Connecting");
                }
                UiAction::Locking => {
                    status_label.set_markup("<span color='orange'></span> Waiting for model lock");
                }
                UiAction::Connected => {
                    status_label.set_markup("<span color='#4ab0fa'></span> Connected");
                }
            }
        }
    });
}
