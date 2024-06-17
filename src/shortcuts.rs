use std::path::PathBuf;

use evdev::{Device, InputEventKind, Key};
use gtk::glib;

use crate::{app::Message, runtime};

pub async fn evdev_listen_device(
    sender: async_channel::Sender<Message>,
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
                sender
                    .send(Message::AddText("Hi!".to_string()))
                    .await
                    .expect("The channel needs to be open.");
            }
        }
    }
}

pub async fn evdev_listen(sender: async_channel::Sender<Message>) {
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
