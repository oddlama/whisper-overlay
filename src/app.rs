use std::sync::OnceLock;

use color_eyre::eyre::bail;
use color_eyre::eyre::Result;
use gdk::glib::ExitCode;
use gdk_wayland::prelude::*;
use gdk_wayland::WaylandSurface;
use gtk::cairo::{RectangleInt, Region};
use gtk::gdk::Display;
use gtk::{glib, Application, ApplicationWindow, Label};
use gtk::{prelude::*, CssProvider};
use gtk_layer_shell::{Layer, LayerShell};
use tokio::runtime::Runtime;

use crate::shortcuts::evdev_listen;

const APP_ID: &str = "org.oddlama.whisper-streaming-overlay";

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
}

#[derive(Debug)]
pub enum Message {
    AddText(String),
}

pub fn launch_app() -> Result<()> {
    // Create a new application
    let app = Application::builder().application_id(APP_ID).build();

    // Connect to signals
    app.connect_startup(|_| load_css());
    app.connect_activate(build_ui);

    // Run the application
    let exit_code = app.run_with_args::<&str>(&[]);
    if exit_code != ExitCode::SUCCESS {
        bail!(
            "Could not launch gtk application: {:?}",
            exit_code
        );
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

fn build_ui(app: &Application) {
    let label = Label::builder()
        .label("Initial: ...")
        .can_target(false)
        .can_focus(false)
        .focus_on_click(false)
        .build();
    label.add_css_class("live-text");

    // Create a new window and present it
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Whisper Streaming Overlay")
        .decorated(false)
        .focus_on_click(false)
        .resizable(false)
        .can_target(false)
        .focusable(false)
        .child(&label)
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(gtk_layer_shell::KeyboardMode::None);
    window.set_namespace("whisper-streaming-overlay");

    window.connect_realize(|window| {
        let wayland_surface = window.surface().and_downcast::<WaylandSurface>().unwrap();
        wayland_surface.set_input_region(&Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0)));
    });

    // Initialize tokio runtime and spawn evdev thread
    let (sender, receiver) = async_channel::bounded(64);
    runtime().spawn(glib::clone!(@strong sender => async move { evdev_listen(sender).await; }));

    glib::spawn_future_local(async move {
        while let Ok(response) = receiver.recv().await {
            println!("Got message {:#?}", response);
            match response {
                Message::AddText(text) => label.set_text(&format!("{}{}", label.text(), text)),
            }
        }
    });

    //window.set_monitor();
    window.present();
}
