use color_eyre::eyre::Result;
use enigo::{Enigo, Keyboard, Settings};
use tokio::sync::mpsc;

use crate::runtime;

pub fn spawn_virtual_keyboard(mut virtual_keyboard_receiver: mpsc::Receiver<String>) -> Result<()> {
    runtime().spawn(async move {
        while let Some(line) = virtual_keyboard_receiver.recv().await {
            // Don't ask why we do this each time. Sometimes the wayland connection
            // breaks and this allows us to be more robust.
            let mut enigo = Enigo::new(&Settings::default()).unwrap();
            if let Err(e) = enigo.text(&line) {
                eprintln!("Failed to type text: {e}")
            }
        }
    });

    Ok(())
}
