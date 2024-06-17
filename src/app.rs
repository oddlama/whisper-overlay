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

use crate::hotkeys::HotkeyEvent;
use crate::runtime;

const APP_ID: &str = "org.oddlama.whisper-overlay";

#[derive(Debug)]
pub enum UiAction {
    AddText(String),
    HideWindow,
    ShowWindow,
}

async fn handle_hotkey(
    hotkey_receiver: async_channel::Receiver<HotkeyEvent>,
    ui_sender: async_channel::Sender<UiAction>,
) {
    while let Ok(event) = hotkey_receiver.recv().await {
        match event {
            HotkeyEvent::Pressed => {
                ui_sender.send(UiAction::ShowWindow).await.unwrap();
            }
            HotkeyEvent::Released => {
                ui_sender.send(UiAction::HideWindow).await.unwrap();
            }
        }
    }
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
    window.set_namespace("whisper-overlay");

    window.connect_realize(|window| {
        let wayland_surface = window.surface().and_downcast::<WaylandSurface>().unwrap();
        wayland_surface.set_input_region(&Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0)));
    });

    //window.set_monitor();
    window.present();

    // Spawn hotkey detector
    let (hotkey_sender, hotkey_receiver) = async_channel::bounded(64);
    runtime().spawn(glib::clone!(@strong hotkey_sender => async move {
        crate::hotkeys::register(hotkey_sender).await;
    }));

    // Spawn hotkey processor
    let (ui_sender, ui_receiver) = async_channel::bounded(64);
    runtime().spawn(
        glib::clone!(@strong hotkey_receiver, @strong ui_sender => async move {
            handle_hotkey(hotkey_receiver, ui_sender).await;
        }),
    );

    glib::spawn_future_local(async move {
        while let Ok(ui_action) = ui_receiver.recv().await {
            println!("Got ui action {:#?}", ui_action);
            match ui_action {
                UiAction::AddText(text) => label.set_text(&format!("{}{}", label.text(), text)),
                UiAction::HideWindow => {}
                UiAction::ShowWindow => {}
            }
        }
    });
}
