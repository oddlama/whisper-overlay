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
    AddText(String),
    Disconnected,
    Connected,
    Connecting,
    HideWindow,
    ShowWindow,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
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
    loop {
        ui_sender.send(UiAction::Disconnected).await.unwrap();
        if connection_receiver.changed().await.is_err() {
            break;
        }

        // Wait until we should connect
        if *connection_receiver.borrow_and_update() != ConnectionState::Connected {
            continue;
        }

        ui_sender.send(UiAction::Connecting).await.unwrap();
        let (mut socket_read, mut socket_write) = match connect_whisper(&connection_opts).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to connect to {}: {}", connection_opts.address, e);
                continue;
            }
        };
        ui_sender.send(UiAction::Connected).await.unwrap();

        let (audio_tx, mut audio_rx) = watch::channel(());
        let (audio_shutdown_tx, audio_shutdown_rx) = std::sync::mpsc::channel();

        let bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
        let bytes_2 = bytes.clone();

        let audio_thread = std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .expect("No input device available"); // FIXME: AAAAAA
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
                            .expect("Could not audio read bytes mutex")
                            .extend_from_slice(bytemuck::cast_slice(data));
                        let _ = audio_tx.send(());
                    },
                    err_fn,
                    None,
                )
                .unwrap();

            stream.play().unwrap();
            let _ = audio_shutdown_rx.recv();
        });

        loop {
            tokio::select! {
                message = recv_message(&mut socket_read) => {
                    match message {
                        Ok(message) => {
                            println!("{}", message.to_string());
                        },
                        Err(e) => {
                            eprintln!("could not receive message from socket: {}", e);
                            break;
                        },
                    }
                }
                _ = audio_rx.changed() => {
                    audio_rx.mark_unchanged();
                    let data = std::mem::take(&mut *bytes.lock().expect("Could not audio read bytes mutex"));
                    if let Err(e) = socket_write.write_all(&data).await {
                        eprintln!("could not write audio to socket: {}", e);
                        break;
                    }
                }
                _ = connection_receiver.changed() => {
                    // Wait until we should disconnect
                    if *connection_receiver.borrow_and_update() == ConnectionState::Disconnected {
                        break;
                    }
                }
            };
        }

        let _ = audio_shutdown_tx.send(());
        audio_thread.join().expect("Could not join audio_thread");
    }
}

async fn handle_hotkey(
    mut hotkey_receiver: mpsc::Receiver<HotkeyEvent>,
    ui_sender: mpsc::Sender<UiAction>,
    connection_sender: watch::Sender<ConnectionState>,
) {
    while let Some(event) = hotkey_receiver.recv().await {
        match event {
            HotkeyEvent::Pressed => {
                ui_sender.send(UiAction::ShowWindow).await.unwrap();
                let _ = connection_sender.send(ConnectionState::Connected);
            }
            HotkeyEvent::Released => {
                ui_sender.send(UiAction::HideWindow).await.unwrap();
                let _ = connection_sender.send(ConnectionState::Disconnected);
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
    runtime().spawn(
        glib::clone!(@strong ui_sender, @strong connection_sender => async move {
            handle_hotkey(hotkey_receiver, ui_sender, connection_sender).await;
        }),
    );

    // Ui updater
    glib::spawn_future_local(async move {
        while let Some(ui_action) = ui_receiver.recv().await {
            match ui_action {
                UiAction::AddText(text) => {} //label.set_text(&format!("{}{}", label.text(), text)),
                UiAction::HideWindow => {
                    //window.set_visible(false);
                }
                UiAction::ShowWindow => {
                    //window.set_visible(true);

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
                    label.set_markup(
                        "<span color='red'>Hell o aegjro ijaoirgjoi jaroigja oirjgoairj oiajrgoiajrgoij oairgj aorgi rgi gir igrigri rgayrgairg iargi arigai rgargarg</span> <span color='green'>world</span>",
                    );

                    main_box.append(&label);

                    let label = label.clone();
                    let main_box = main_box.clone();
                    gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                        main_box.remove(&label);
                    });
                }
                UiAction::Disconnected => {}
                UiAction::Connected => {}
                UiAction::Connecting => {}
            }
        }
    });
}
