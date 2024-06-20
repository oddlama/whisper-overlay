use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use color_eyre::eyre::Result;
use evdev::{Device, InputEventKind, Key};
use gtk::glib;
use notify::{event::CreateKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::{self, channel};

use crate::runtime;

#[derive(Debug)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

pub async fn evdev_listen_device(
    sender: mpsc::Sender<HotkeyEvent>,
    path: PathBuf,
    device: Device,
    key: Key,
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
                eprintln!(
                    "Error while processing events on {} (device disconnected?): {}",
                    name, e
                );
                return;
            }
        };

        if let InputEventKind::Key(k) = ev.kind() {
            if k == key {
                if ev.value() == 0 {
                    let _ = sender.send(HotkeyEvent::Released).await;
                } else if ev.value() == 1 {
                    let _ = sender.send(HotkeyEvent::Pressed).await;
                }
            }
        }
    }
}

pub async fn register_and_watch(sender: mpsc::Sender<HotkeyEvent>, hotkey: String) {
    let key = Key::from_str(&hotkey).expect(&format!("Could not find key with name {hotkey}"));
    evdev::enumerate()
        .filter(|(_, device)| {
            device
                .supported_keys()
                .map_or(false, |keys| keys.contains(key))
        })
        .for_each(|(path, device)| {
            runtime().spawn(glib::clone!(@strong sender => async move {
                evdev_listen_device(sender, path, device, key).await;
            }));
        });

    // Watch for new devices in /dev/input
    let (tx, mut rx) = channel(1);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            tx.blocking_send(res).unwrap();
        },
        notify::Config::default(),
    )
    .expect("Failed to setup /dev/input watcher");

    watcher
        .watch(Path::new("/dev/input"), RecursiveMode::NonRecursive)
        .expect("Failed to watch /dev/input");

    let mut wait_for_permissions = HashMap::new();
    let try_spawn_listener = |path: PathBuf| -> Result<()> {
        let device = Device::open(&path)?;
        runtime().spawn(glib::clone!(@strong sender => async move {
            evdev_listen_device(sender, path, device, key).await;
        }));

        Ok(())
    };

    loop {
        let timeout = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timeout);
        tokio::select! {
            res = rx.recv() => {
                if let Some(res) = res {
                    match res {
                        Ok(event) => match event.kind {
                            EventKind::Create(CreateKind::File) => {
                                for path in event.paths {
                                    if let Err(_) = try_spawn_listener(path.clone()) {
                                        // Udev might take some time to modify the file permissions
                                        // so we can actually access the device. This also prevents
                                        // us from installing a file watcher, so instead we will
                                        // recheck up to 5 times, with a 1 second delay.
                                        wait_for_permissions.insert(path, 5);
                                    }
                                }
                            }
                            _ => {}
                        },
                        Err(e) => eprintln!("watch error: {:?}", e),
                    }
                } else {
                    break;
                }
            }
            _ = &mut timeout => {
                wait_for_permissions.retain(|path, remaining_tries| {
                    if *remaining_tries == 0 {
                        return false;
                    }

                    if try_spawn_listener(path.clone()).is_ok() {
                        // We are now listening, so don't spawn new listeners
                        return false;
                    }

                    *remaining_tries -= 1;
                    return true;
                });
            }
        }
    }
}
