use std::path::PathBuf;

use evdev::{Device, InputEventKind, Key};
use gtk::glib;

use crate::runtime;

#[derive(Debug)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

pub async fn evdev_listen_device(
    sender: async_channel::Sender<HotkeyEvent>,
    path: PathBuf,
    device: Device,
) {
    let name = device.name().unwrap_or("Unnamed device");
    let name = format!("{} ({})", name, path.display());

    println!("listening for events on {}", name);
    let mut events = match device.into_event_stream() {
        Ok(events) => events,
        Err(e) => {
            eprintln!("Error while starting event stream on {}: {}", name, e);
            return;
        }
    };
    loop {
        let ev = match events.next_event().await {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("Error while processing events on {}: {}", name, e);
                return;
            }
        };

        if let InputEventKind::Key(Key::KEY_RIGHTCTRL) = ev.kind() {
            if ev.value() == 0 {
                let _ = sender.send(HotkeyEvent::Released).await;
            } else if ev.value() == 1 {
                let _ = sender.send(HotkeyEvent::Pressed).await;
            }
        }
    }
}

pub async fn register(sender: async_channel::Sender<HotkeyEvent>) {
    evdev::enumerate()
        .filter(|(_, device)| {
            device
                .supported_keys()
                .map_or(false, |keys| keys.contains(Key::KEY_RIGHTCTRL))
        })
        .for_each(|(path, device)| {
            runtime().spawn(glib::clone!(@strong sender => async move {
                evdev_listen_device(sender, path, device).await;
            }));
        });
}
