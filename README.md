[Installation and Usage](#-installation-and-usage)

## 💬 whisper-overlay

A wayland overlay providing speech-to-text functionality for any application via a global push-to-talk hotkey.
Anything you are saying while holding the hotkey will be transcribed in real-time and shown on-screen.
The live transcriptions use a faster but less accurate model but as soon as you pause speaking or release
the hotkey, the transcription will be updated using a second, more accurate model.
This resulting text will then be tryped into the window that is currently focused.

- On-screen, realtime live transcriptions via CUDA and faster-whisper
- The server-client based architecture allows you to host the model on another machine
- Native waybar integration for status display
- Utilizes `layer-shell` and `virtual-keyboard-v1` to support most wayland compositors

This makes use of the [RealtimeSTT](https://github.com/KoljaB/RealtimeSTT) python library to provide
live transcriptions, which in turn uses [faster-whisper](https://github.com/SYSTRAN/faster-whisper)
for both the actual realtime and high-fidelity transcription model.

Requirements:

- A wayland compositor (sway, hyprland, ...)
- A GPU with CUDA support is highly recommended, otherwise translation will have a significantly latency even
  on a modern CPU (1 second latency for live transcription and ~5 seconds for the result)

## 🚀 Quick Start

- Clone the repository
  ```
  git clone https://github.com/oddlama/whisper-overlay
  cd whisper-overlay
  ```

- Run the realtime-stt-server using docker
  ```
  docker-compose up
  ```

- Install and run whisper-overlay
  ```
  cargo install whisper-overlay
  whisper-overlay overlay
  # Or alternatively select a hotkey:
  #whisper-overlay overlay --hotkey KEY_F12
  ```

Now press and hold <kbd>Right Ctrl</kbd> to transcribe. For a permanent installation
I recommend starting the server as a systemd service and adding the `whisper-overlay overlay`
as a startup command to your desktop environment / compositor.

## ⚙️ Usage

In principle you just need to start `./realtime-stt-server.py` and it will be listening for requests on `localhost:7007`.
You can then start `whisper-overlay overlay` to transcribe text. The default hotkey is <kbd>Right Ctrl</kbd>,
but you can change this by specifying any name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html),
for example `KEY_F12` for <kbd>F12</kbd>. Beware that the hotkey is only observed and will still be passed to the application that is focused.

#### Server (realtime-stt-server)

If you want to change the server settings, it comes with the following options:

```bash
> realtime-stt-server.py --help
usage: realtime-stt-server.py [-h] [--host HOST] [--port PORT] [--device DEVICE] [--model MODEL]
                              [--model-realtime MODEL_REALTIME] [--language LANGUAGE] [--debug]

options:
  -h, --help            show this help message and exit
  --host HOST           The host to listen on [default: 'localhost']
  --port PORT           The port to listen on [default: 7007]
  --device DEVICE       Device to run the models on, defaults to cuda if available, else cpu [default: 'cuda']
  --model MODEL         Main model used to generate the final transcription [default: 'large-v3']
  --model-realtime MODEL_REALTIME
                        Faster model used to generate live transcriptions [default: 'base']
  --language LANGUAGE   Set the spoken language. Leave empty to auto-detect. [default: '']
  --debug               Enable debug log output [default: unset]
```

#### Client (whisper-overlay)

The actual overlay can also be customized, for example by providing your own gtk style
(refer to [the builtin style.css](./src/style.css) as a reference), or by changing the hotkey.
It has the following options:

```bash
> whisper-overlay overlay --help
Usage: whisper-overlay overlay [OPTIONS]

Options:
  -a, --address <ADDRESS>  The address of the the whisper streaming instance (host:port) [default: localhost:7007]
  -s, --style <STYLE>      An optional stylesheet for the overlay, which replaces the internal style
      --hotkey <HOTKEY>    Specifies the hotkey to activate voice input. You can use any key or button name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html) [default: KEY_RIGHTCTRL]
  -h, --help               Print help
```

## 📦 Installation

<details>
<summary>

### ❄️ 🐳 Docker & cargo
</summary>

For a quick and simple install, you can run the server using docker and
install the overlay directly via cargo:

```bash
git clone https://github.com/oddlama/whisper-overlay
cd whisper-overlay

# Start realtime-stt-server
docker-compose up

# Install and run overlay
cargo install whisper-overlay
whisper-overlay overlay
```

</details>
<details>
<summary>

### ❄️ NixOS
</summary>

This application comes with a NixOS module and overlay so you can easily access the relevant packages
and host the realtime-stt-server. First, add this flake as an input:

```nix
{
  inputs = {
    # ...
    whisper-overlay.url = "github:oddlama/whisper-overlay";
    whisper-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Then add the nixos module exposed by this flake,
and enable the realtime-stt-server in your `configuration.nix`. Also add the relevant package to your system or user,
so you can start it later.

```nix
{
  imports = [
    inputs.whisper-overlay.nixosModules.default
  ];

  # Also make sure to enable cuda support in nixpkgs, otherwise transcription will
  # be painfully slow. But be prepared to let your computer build packages for 2-3 hours.
  nixpkgs.config.cudaSupport = true;

  services.realtime-stt-server.enable = true;
  environment.systemPackages = [pkgs.whisper-overlay];
}
```

The server will now be started automatically with your system,
and you can run `whisper-overlay overlay` as your user.
You might want to add this.

</details>
<details>
<summary>

### 🧰 Manually
</summary>

First, install and start the server:

```bash
# Create virtualenv
python -m venv venv
source venv/bin/activate

# Install RealtimeSTT (fork)
# Follow this for GPU support:
# https://github.com/KoljaB/RealtimeSTT?tab=readme-ov-file#gpu-support-with-cuda-recommended
git clone https://github.com/oddlama/RealtimeSTT
cd RealtimeSTT
pip install -r requirements.txt
cd ..

# Run server script
git clone https://github.com/oddlama/whisper-overlay
python ./realtime-stt-server.py
```

Second, start the overlay by tunning the client from source:

```bash
# Clone repository (or reuse the previous checkout)
git clone https://github.com/oddlama/whisper-overlay
cargo build --release
./target/release/whisper-overlay overlay
```

</details>

## 🌟 Waybar integration

The whisper-overlay natively supports a waybar status command to
display the server status in your waybar.

Add this to your waybar config:

```jsonc
"custom/whisper_overlay": {
    "escape": true,
    "exec": "/path/to/whisper-overlay waybar-status",
    "format": "{icon} {}",
    "format-icons": {
        "disconnected": "<span foreground='gray'></span>",
        "connected": "<span foreground='#4ab0fa'></span>",
        "connected-active": "<span foreground='red'></span>"
    },
    "return-type": "json",
    "tooltip": true
},
```

And instanciate the module somewhere:

```jsonc
"modules-left": [
    // ...
    "custom/whisper_overlay"
    // ...
],
```

## ❌ Limitations

#### Requires RealtimeSTT fork

Currently, you need to use my fork of [RealtimeSTT](https://github.com/oddlama/RealtimeSTT) which allows the client
to read token probabilities and fixes some shutdown issues. Already requested this to be upstreamed,
so hopefully this won't be required for long.

#### Single active client

The provided `realtime-stt-server` implementation allows you to host the server either locally on your machine, or on another machine
in your network. Our end of the implementation is techincally ready for multiple clients, but due to the way `RealtimeSTT` works, it cannot process
multiple requests simultaneously at this point in time. So you will have to wait for other clients to disconnect before your transcription can begin.

#### Wayland only

Currently, this project _requires_ the use of a wayland compositor that supports the layer-shell and virtual-keyboard-v1 protocol extensions.
Thus it should work out-of-the-box on any wlroots based compositor (sway, ...) and on hyprland. X11 support is currently not planned.
There is a branch with a partial implementation for X11, but getting GTK4 to create a reliable overlay window has proven to be hard and
auto-type doesn't work properly with enigo (the rust library in use for virtual input). But I'm of course happy to accept contributions
in that regard if someone knows how to address the remaining issues.

#### Global hotkeys via evdev

The global hotkey is detected using `evdev`, since I didn't manage to get the GlobalShortcuts desktop portal
to work with windows using the layer-shell protocol ([related issue](https://github.com/bilelmoussaoui/ashpd/issues/213)).
In the future this might change, but for now your user must be in the `input` group for this to work.

## 📜 License

Licensed under the MIT license ([LICENSE](LICENSE) or <https://opensource.org/licenses/MIT>).
Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project by you, shall be licensed as above, without any additional terms or conditions.
