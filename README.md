# TTLCD Linux

A Linux driver, daemon and web UI for the Thermaltake / XinWeiYe 3.95" square
USB LCD (480×480, USB id `264a:2343`).

- **Driver** (`src/lib.rs`, `src/proto.rs`): talks to the panel over USB.
- **Daemon** (`ttlcd daemon`): renders one of about 155 display modes to the
  panel (clocks, system monitors, screensavers, a pet cat) and serves an HTTP
  API.
- **Web UI** (`web/`): written in Rust, compiled to WebAssembly and embedded
  in the daemon. Open <http://127.0.0.1:7070> to pick a mode and change its
  settings.

The daemon only listens on loopback addresses.

Full disclaimer: this is an AI generated project (which should be obvious).

This project has only been tested with the MAGForce 3.95 inch square LCD.

## Build

### With Nix

```bash
nix build .#          # result/bin/ttlcd
nix develop           # shell with everything below installed
```

### Without Nix

You need:

- Rust (stable) with the `wasm32-unknown-unknown` target
- `wasm-bindgen-cli` version 0.2.117 (it must match the `wasm-bindgen` crate
  in `Cargo.lock`)
- `libusb-1.0` development files and `pkg-config`

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.117

scripts/build-web.sh        # builds the web UI into src/web/pkg/
cargo build --release       # target/release/ttlcd
```

The web UI must be built first; the daemon embeds it.

## Install as a daemon

### NixOS

Add this repository as a flake input and import its module:

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    ttlcd.url = "github:Marauder586/ttlcd-linux";
  };

  outputs = { nixpkgs, ttlcd, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        ttlcd.nixosModules.default
      ];
    };
  };
}
```

Then enable the service:

```nix
# configuration.nix
{
  services.ttlcd = {
    enable = true;
    mode = "clock_sysinfo";  # mode shown at start-up
  };
}
```

The module installs a udev rule that lets the logged-in user open the panel,
and a systemd **user** service. The daemon starts when you log in and runs as
you, so there is no user to configure. After `nixos-rebuild switch`, replug
the panel and log in again (or reboot).

```bash
systemctl --user status ttlcd
journalctl --user -u ttlcd -f
```

To start it at boot, before anyone logs in, enable lingering for your user:
`users.users.alice.linger = true;`.

Other options, all optional:

| Option | Default | Meaning |
|---|---|---|
| `port` | `7070` | HTTP port |
| `bind` | `"127.0.0.1"` | Bind address; must be loopback |
| `quality` | `85` | JPEG quality, 1–100 |
| `rotate` | `0` | Rotate output by 0, 90, 180 or 270 degrees |
| `rotationModes` | `[]` | Modes to cycle through |
| `rotationInterval` | `0` | Seconds between rotation steps; 0 disables |
| `headless` | `false` | Run without the USB panel (web UI only) |
| `package` | this flake's build | The `ttlcd` package to run |

### Other distributions

1. Build as above and install the binary:

   ```bash
   sudo install -m 755 target/release/ttlcd /usr/local/bin/ttlcd
   ```

2. Let your user open the panel without root. Create
   `/etc/udev/rules.d/70-ttlcd.rules`:

   ```
   SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2343", TAG+="uaccess"
   ```

   Then reload and replug the panel:

   ```bash
   sudo udevadm control --reload
   ```

3. Run it as a systemd user service. Create
   `~/.config/systemd/user/ttlcd.service`:

   ```ini
   [Unit]
   Description=ttlcd USB LCD panel daemon

   [Service]
   ExecStart=/usr/local/bin/ttlcd daemon
   Restart=on-failure
   RestartSec=5

   [Install]
   WantedBy=default.target
   ```

   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now ttlcd
   ```

`ttlcd daemon --help` lists the flags (port, start-up mode, rotation and so
on); add them to `ExecStart`. Settings changed in the web UI are saved under
`~/.config/ttlcd/`.
