use std::sync::OnceLock;

use evdev::InputEventKind;
use gdk_wayland::prelude::*;
use gdk_wayland::WaylandSurface;
use gtk::cairo::{RectangleInt, Region};
use gtk::gdk::Display;
use gtk::{glib, Application, ApplicationWindow, Label};
use gtk::{prelude::*, CssProvider};
use gtk_layer_shell::{Layer, LayerShell};
use tokio::runtime::Runtime;

const APP_ID: &str = "org.oddlama.whisper-streaming-overlay";

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
}

#[derive(Debug)]
enum Message {
    AddText(String),
}

async fn evdev_listen(sender: async_channel::Sender<Message>) {
    evdev::enumerate()
        .map(|t| t.1)
        .filter(|device| {
            device
                .supported_keys()
                .map_or(false, |keys| keys.contains(evdev::Key::KEY_RIGHTCTRL))
        })
        .for_each(|device| {
            runtime().spawn(glib::clone!(@strong sender => async move {
                println!("listening for events on {}", device.name().unwrap_or("Unnamed device"));
                let mut events = device.into_event_stream().expect(&format!("Cannot get event stream for device"));
                loop {
                    let ev = events.next_event().await.expect("");
                    if let InputEventKind::Key(evdev::Key::KEY_RIGHTCTRL) = ev.kind() {
                        if ev.value() == 0 {
                            sender
                                .send(Message::AddText("Hi!".to_string()))
                                .await
                                .expect("The channel needs to be open.");
                        }
                    }
                }
            }));
        });
}

fn main() -> glib::ExitCode {
    // Create a new application
    let app = Application::builder().application_id(APP_ID).build();

    // Connect to signals
    app.connect_startup(|_| load_css());
    app.connect_activate(build_ui);

    // Run the application
    app.run()
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
        .label("...")
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

// TODO: --monitor
// TODO: --style
