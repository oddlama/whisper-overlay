use color_eyre::eyre::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::StreamExt;
use gdk::glib::ExitCode;
use gdk_wayland::{prelude::*, WaylandSurface};
use gtk::cairo::{RectangleInt, Region};
use gtk::gdk::Display;
use gtk::{glib, Application, ApplicationWindow, Label};
use gtk::{prelude::*, CssProvider};
use gtk_layer_shell::{Layer, LayerShell};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::codec::LengthDelimitedCodec;

use crate::cli::{Command, ConnectionOpts};
use crate::hotkeys::HotkeyEvent;
use crate::keyboard::spawn_virtual_keyboard;
use crate::runtime;
use crate::util::{recv_message, send_audio_data, send_message};

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
pub struct Word {
    #[allow(unused)]
    begin: f32,
    #[allow(unused)]
    end: f32,
    word: String,
    probability: f32,
}

#[derive(Debug, Deserialize)]
pub struct Segment {
    words: Vec<Word>,
}

#[derive(Debug, Deserialize)]
pub struct ModelResult {
    kind: String,
    #[allow(unused)]
    text: String,
    segments: Vec<Segment>,
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

async fn set_disconnect_status(ui_sender: &mpsc::Sender<UiAction>, message: &serde_json::Value) {
    let text = if let Some(status) = message.get("status").and_then(|x| x.as_str()) {
        status.to_string()
    } else {
        message.to_string()
    };
    ui_sender
        .send(UiAction::Disconnected(Some(text)))
        .await
        .unwrap();
}

async fn handle_connection(
    mut connection_receiver: watch::Receiver<ConnectionState>,
    ui_sender: mpsc::Sender<UiAction>,
    connection_opts: ConnectionOpts,
) {
    ui_sender.send(UiAction::Disconnected(None)).await.unwrap();

    let bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
    let bytes_2 = bytes.clone();
    let (audio_tx, mut audio_rx) = watch::channel(());
    let (audio_shutdown_tx, audio_shutdown_rx) = std::sync::mpsc::channel();
    let audio_active = Arc::new(Mutex::new(false));
    let audio_active_2 = audio_active.clone();

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
                &config,
                move |data: &[i16], _: &_| {
                    if !*audio_active_2.lock().expect("Could not lock audio stop") {
                        // BUG: https://github.com/RustAudio/cpal/issues/771
                        return;
                    }
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
        {
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
            let (mut socket_read, mut socket_write) = match connect_whisper(&connection_opts).await
            {
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
                        eprintln!("error: received unexpected message: {}", message);
                        set_disconnect_status(&ui_sender, &message).await;
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
                        eprintln!("error: received unexpected message: {}", message);
                        set_disconnect_status(&ui_sender, &message).await;
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

            let (shutdown_tx, mut shutdown_rx) = watch::channel(());
            // Start audio thread
            *audio_active.lock().expect("Could not lock audio stop") = true;

            let mut shutdown_timer: Option<JoinHandle<()>> = None;
            let mut read_message_frame = LengthDelimitedCodec::builder()
                .length_field_offset(0) // default value
                .length_field_length(4)
                .length_adjustment(0) // default value
                .num_skip(4) // skip the first 4 bytes
                .new_read(socket_read);

            loop {
                tokio::select! {
                    message = read_message_frame.next() => {
                        let Some(message) = message else {
                            continue;
                        };

                        let message: Result<serde_json::Value> = message.wrap_err("Failed to read next message")
                            .and_then(|x| String::from_utf8(x.to_vec()).wrap_err("Failed to convert message to utf8"))
                            .and_then(|x| serde_json::from_str(&x).wrap_err("Failed to parse json"));

                        match message {
                            Ok(message) => {
                                if message.get("segments").is_some() {
                                    if message.get("kind") != Some(&json!("result")) {
                                        // If this is a result message, and we have a running shutdown timer
                                        // (i.e. we want to disconnect), we use this as the final result.
                                        if let Some(ref timer) = shutdown_timer {
                                            timer.abort();
                                            shutdown_timer = None;
                                            let _ = shutdown_tx.send(());
                                            println!("Received final result for this session in time, signalling shutdown");
                                        }
                                    }
                                    ui_sender.send(UiAction::ModelResult(message)).await.unwrap();
                                } else {
                                    eprintln!("ignoring unsolicited message: {}", message.to_string());
                                }
                            },
                            Err(e) => {
                                eprintln!("could not receive message from socket: {:#}", e);
                                ui_sender
                                    .send(UiAction::Disconnected(Some(e.to_string())))
                                    .await
                                    .unwrap();
                                break;
                            },
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        println!("Ready to disconnect.");
                        // Processing is finished
                        shutdown_rx.mark_unchanged(); // Mark state seen
                        ui_sender.send(UiAction::Disconnected(None)).await.unwrap();
                        break;
                    }
                    _ = audio_rx.changed() => {
                        audio_rx.mark_unchanged(); // Mark state seen
                        let data = std::mem::take(&mut *bytes.lock().expect("Could not lock mutex to read audio data"));

                        if let Err(e) = send_audio_data(&mut socket_write, &data).await {
                            eprintln!("could not write audio data to socket: {}", e);
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
                            println!("Done, notifying server to finish...");
                            // Pause audio thread
                            *audio_active.lock().expect("Could not lock audio stop") = false;

                            // Don't disconnect immediately, instead instruct the server to flush
                            if let Err(e) = send_message(&mut socket_write, json!({"action": "flush"})).await {
                                eprintln!("could not send flush action to socket: {}", e);
                                ui_sender
                                    .send(UiAction::Disconnected(Some(e.to_string())))
                                    .await
                                    .unwrap();
                                break;
                            }

                            // If the server fails to respond within a short timeframe, we will force-kill.
                            let shutdown_tx_2 = shutdown_tx.clone();
                            let timer = runtime().spawn(async move {
                                tokio::time::sleep(Duration::from_millis(2000)).await;
                                println!("Server has not responded to flush, forcing disconnect now.");
                                let _ = shutdown_tx_2.send(());
                            });
                            shutdown_timer = Some(timer);
                        } else {
                            // If the client wants to reconnect, cancel any running disconnect timers
                            if let Some(ref timer) = shutdown_timer {
                                timer.abort();
                                shutdown_timer = None;
                            }
                            // Restart audio thread
                            *audio_active.lock().expect("Could not lock audio stop") = true;
                            println!("Staying connected due to user request...");
                        }
                    }
                };
            }

            println!("Disconnecting.");
        }

        // Keep the window open for another 4 seconds if no other event takes priority
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(4000)) => {
                ui_sender.send(UiAction::HideWindow).await.unwrap();
            },
            _ = connection_receiver.changed() => {
                connection_receiver.mark_changed();
                // Early break so the request can be prioritized
            }
        };

        println!("Waiting for next connection request");
    }

    // Stop and join audio thread
    let _ = audio_shutdown_tx.send(());
    audio_thread.join().expect("Could not join audio_thread");
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

    let Command::Overlay { style, .. } = opts.clone() else {
        bail!("got invalid command options");
    };

    // Connect to signals
    app.connect_startup(move |_| load_css(style.clone()));
    app.connect_activate(move |app| build_ui(app, opts.clone()));

    // Run the application
    let exit_code = app.run_with_args::<&str>(&[]);
    if exit_code != ExitCode::SUCCESS {
        bail!("Could not launch gtk application: {:?}", exit_code);
    };

    Ok(())
}

fn load_css(style: Option<PathBuf>) {
    // Load the CSS file and add it to the provider
    let provider = CssProvider::new();
    if let Some(path) = style {
        provider.load_from_path(path);
    } else {
        provider.load_from_string(include_str!("style.css"));
    }

    // Add the provider to the default screen
    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &Application, opts: Command) {
    let Command::Overlay {
        connection_opts,
        hotkey,
        ..
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

    let live_text = Label::builder()
        .wrap(true)
        .wrap_mode(gdk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Left)
        .halign(gtk::Align::Start)
        .can_target(false)
        .can_focus(false)
        .focus_on_click(false)
        .build();
    live_text.add_css_class("live-text");
    live_text.add_tick_callback(move |widget, _| {
        widget.queue_draw();
        glib::ControlFlow::Continue
    });
    main_box.append(&live_text);

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
        .title("Whisper Overlay")
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
        window.set_visible(false);
    });

    window.present();

    let (ui_sender, mut ui_receiver) = mpsc::channel(64);
    let (connection_sender, connection_receiver) = watch::channel(ConnectionState::Disconnected);
    let (hotkey_sender, hotkey_receiver) = mpsc::channel(64);
    let (virtual_keyboard_sender, virtual_keyboard_receiver) = mpsc::channel(64);

    // Spawn connection manager
    runtime().spawn(
        glib::clone!(@strong connection_receiver, @strong ui_sender => async move {
            handle_connection(connection_receiver, ui_sender, connection_opts.clone()).await;
        }),
    );

    // Spawn hotkey detector
    runtime().spawn(glib::clone!(@strong hotkey_sender => async move {
        crate::hotkeys::register(hotkey_sender, hotkey).await;
    }));

    // Spawn hotkey processor
    runtime().spawn(glib::clone!(@strong connection_sender => async move {
        handle_hotkey(hotkey_receiver, connection_sender).await;
    }));

    spawn_virtual_keyboard(virtual_keyboard_receiver).expect("Failed to spawn virutal keyboard");

    // Ui updater
    glib::spawn_future_local(async move {
        let keep_duration = Duration::from_millis(6000);
        let mut line_history: Vec<(SystemTime, String)> = vec![];

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
                    match serde_json::from_value::<ModelResult>(value) {
                        Ok(res) => {
                            let now = SystemTime::now();

                            // Expire old history
                            line_history.retain(|&(time, _)| {
                                now.duration_since(time)
                                    .map_or(false, |x| x <= keep_duration)
                            });

                            let mut to_type = "".to_string();
                            let mut line_markup = "".to_string();
                            let mut markup = line_history
                                .iter()
                                .map(|(_, markup)| markup)
                                .cloned()
                                .collect::<Vec<String>>()
                                .join("\n");

                            for (si, segment) in res.segments.iter().enumerate() {
                                if si != 0 {
                                    line_markup += "\n";
                                }

                                for (wi, word) in segment.words.iter().enumerate() {
                                    let color = gradient.at(word.probability.into());
                                    let word = if wi == 0 {
                                        word.word.trim_start()
                                    } else {
                                        &word.word
                                    };

                                    //let rgba = color.to_rgba8();
                                    //print!("{}", word.color(Rgb(rgba[0], rgba[1], rgba[2])));
                                    //let _ = std::io::stdout().flush();

                                    to_type += &word;
                                    line_markup += &format!(
                                        "<span color=\"{fg}\">{text}</span>",
                                        fg = color.to_hex_string(),
                                        text = glib::markup_escape_text(word)
                                    );
                                }

                                to_type = to_type.trim_end().to_string() + "\n";
                            }

                            if !markup.is_empty() {
                                markup += "\n";
                            }
                            markup += &line_markup;
                            live_text.set_markup(&markup);

                            // Add line to history if we have a result
                            if res.kind == "result" {
                                if !to_type.is_empty() {
                                    let _ = virtual_keyboard_sender.send(to_type).await;
                                }
                                line_history.push((now, line_markup))
                            }
                        }
                        Err(e) => eprintln!("error: ignoring invalid model result data: {e}"),
                    }
                }
                UiAction::HideWindow => {
                    window.set_visible(false);
                    window.queue_draw();
                    live_text.set_markup("");
                }
                UiAction::ShowWindow => {
                    // Just don't ask, this is not an oversight!
                    // If the window is not toggled, on, off, on, it won't show the first time.
                    // This is somehow related to hiding the window in connect_realize.
                    window.set_visible(true);
                    window.set_visible(false);
                    window.set_visible(true);
                    window.queue_draw();
                    status_label.queue_draw();
                }
                UiAction::Disconnected(reason) => {
                    let mut message = "<span color='gray'></span> Disconnected".to_string();
                    if let Some(reason) = reason {
                        message += &format!(" <span color='gray'>{}</span>", reason);
                    }
                    status_label.set_markup(&message);
                    status_label.queue_draw();
                }
                UiAction::Connecting => {
                    status_label.set_markup("<span color='yellow'></span> Connecting");
                    status_label.queue_draw();
                }
                UiAction::Locking => {
                    status_label.set_markup("<span color='orange'></span> Waiting for model lock");
                    status_label.queue_draw();
                }
                UiAction::Connected => {
                    status_label.set_markup("<span color='#4ab0fa'></span> Connected");
                    status_label.queue_draw();
                }
            }
        }
    });
}
