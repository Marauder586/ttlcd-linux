{
  description = "ttlcd-linux — Rust driver for XinWeiYe/Thermaltake USB LCD panels";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {inherit system overlays;};
      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = ["rust-src" "rust-analyzer" "clippy" "rustfmt"];
        # The web UI (web/) is compiled to WebAssembly.
        targets = ["wasm32-unknown-unknown"];
      };

      # ── package derivation ───────────────────────────────────────
      # Build with the toolchain above so the wasm32 target is available.
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rust;
        rustc = rust;
      };

      ttlcd = rustPlatform.buildRustPackage {
        pname = "ttlcd";
        version = "0.22.0";
        src = ./.;

        cargoLock.lockFile = ./Cargo.lock;

        # wasm-bindgen-cli must be the same version as the wasm-bindgen crate
        # pinned in web/Cargo.toml.
        nativeBuildInputs = with pkgs; [pkg-config makeWrapper wasm-bindgen-cli];
        buildInputs = with pkgs; [libusb1];

        # The daemon embeds the web UI, so build that first.
        preBuild = ''
          bash scripts/build-web.sh --offline
        '';

        # The binary needs libusb at runtime
        postInstall = ''
          wrapProgram $out/bin/ttlcd \
            --prefix LD_LIBRARY_PATH : "${pkgs.libusb1}/lib"
        '';

        meta = {
          description = "USB LCD panel driver and display suite (XinWeiYe / Thermaltake)";
          homepage = "https://github.com/marauder/ttlcd-linux";
          license = pkgs.lib.licenses.mit;
          maintainers = [];
          mainProgram = "ttlcd";
        };
      };
    in {
      # ── packages ────────────────────────────────────────────────
      packages = {
        default = ttlcd;
        inherit ttlcd;
      };

      # ── dev shell ────────────────────────────────────────────────
      devShells.default = pkgs.mkShell {
        packages = [
          rust
          pkgs.libusb1
          pkgs.pkg-config
          pkgs.usbutils
          # Generates the JS loader for the web UI (scripts/build-web.sh)
          pkgs.wasm-bindgen-cli
          # Useful for testing the web UI without a device
          pkgs.websocat
          pkgs.curl
        ];
        env.RUST_BACKTRACE = "1";
        env.RUST_LOG = "info";
      };
    })
    // {
      # Not per-system: NixOS expects modules at `nixosModules.<name>`.
      # ── NixOS module ─────────────────────────────────────────────
      # Import as:  imports = [ ttlcd-linux.nixosModules.default ];
      nixosModules.default = {
        config,
        lib,
        pkgs,
        ...
      }: let
        cfg = config.services.ttlcd;

        # Build the ExecStart argument list
        bindArgs = " --bind ${cfg.bind}";
        modeArgs =
          lib.optionalString (cfg.rotationModes != [])
          " --rotation-modes ${lib.concatStringsSep "," cfg.rotationModes}";
        rotIntervalArgs =
          lib.optionalString (cfg.rotationInterval > 0)
          " --rotation-interval ${toString cfg.rotationInterval}";
        headlessFlag = lib.optionalString cfg.headless " --headless";
        rotateArgs =
          lib.optionalString (cfg.rotate != 0)
          " --rotate ${toString cfg.rotate}";
        qualityArgs =
          lib.optionalString (cfg.quality != 85)
          " --quality ${toString cfg.quality}";
        petArgs =
          lib.optionalString (cfg.pet.coat != null) " --pet-coat ${cfg.pet.coat}"
          + lib.optionalString (cfg.pet.environment != null)
          " --pet-environment ${cfg.pet.environment}";
        lightsArgs =
          lib.optionalString cfg.lights.enable " --lights"
          + lib.optionalString (cfg.lights.openrgbAddress != "127.0.0.1:6742")
          " --openrgb ${cfg.lights.openrgbAddress}";
      in {
        options.services.ttlcd = {
          enable = lib.mkEnableOption "ttlcd LCD panel daemon";

          port = lib.mkOption {
            type = lib.types.port;
            default = 7070;
            description = "HTTP port for the web UI / API.";
          };

          mode = lib.mkOption {
            type = lib.types.str;
            default = "clock_sysinfo";
            description = "Display mode on first start. After that the daemon restores the mode it was last showing.";
          };

          quality = lib.mkOption {
            type = lib.types.ints.between 1 100;
            default = 85;
            description = "JPEG quality sent to panel (1–100).";
          };

          rotate = lib.mkOption {
            type = lib.types.enum [0 90 180 270];
            default = 0;
            description = "Rotate all output by this many degrees (0/90/180/270).";
          };

          headless = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Start without opening the USB device (web UI only).";
          };

          bind = lib.mkOption {
            type = lib.types.str;
            default = "127.0.0.1";
            description = "Bind address for the HTTP server. Must be a loopback address; the daemon refuses anything else.";
          };

          rotationModes = lib.mkOption {
            type = lib.types.listOf lib.types.str;
            default = [];
            example = ["clock" "sysinfo" "cpu_detail" "network"];
            description = "Ordered list of modes for auto-rotation (empty = disabled).";
          };

          rotationInterval = lib.mkOption {
            type = lib.types.ints.unsigned;
            default = 0;
            description = "Seconds between auto-rotation advances (0 = disabled).";
          };

          pet.coat = lib.mkOption {
            type = lib.types.nullOr (lib.types.enum ["tuxedo" "black" "orange"]);
            default = null;
            description = "Pet cat coat. If set, it replaces the choice saved from the web UI at every start.";
          };

          pet.environment = lib.mkOption {
            type = lib.types.nullOr (lib.types.enum ["solid" "forest" "living_room" "night_forest" "dungeon" "library"]);
            default = null;
            description = "Pet cat environment. If set, it replaces the choice saved from the web UI at every start.";
          };

          lights.enable = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Match the computer's RGB lighting to the panel through OpenRGB at every start. When false, the choice saved from the web UI is used. Needs an OpenRGB server, e.g. services.hardware.openrgb.";
          };

          lights.openrgbAddress = lib.mkOption {
            type = lib.types.str;
            default = "127.0.0.1:6742";
            description = "Address of the OpenRGB SDK server.";
          };

          package = lib.mkOption {
            type = lib.types.package;
            default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
            defaultText = lib.literalExpression "ttlcd.packages.\${pkgs.stdenv.hostPlatform.system}.default";
            description = "ttlcd package to use.";
          };
        };

        # The daemon used to be a system service running as a named user.
        imports = [
          (lib.mkRemovedOptionModule ["services" "ttlcd" "user"] ''
            ttlcd now runs as a systemd user service for whoever is logged
            in, so there is no user to configure.
          '')
        ];

        config = lib.mkIf cfg.enable {
          # Let the user at the local seat open the panel without root.
          # TAG+="uaccess" only takes effect in a rules file ordered before
          # systemd's 73-seat-late.rules, which `services.udev.extraRules`
          # (99-local.rules) is not, so ship a rules file of our own.
          # MODE/GROUP additionally covers sessions without a seat (SSH,
          # lingering users).
          services.udev.packages = [
            (pkgs.writeTextFile {
              name = "ttlcd-udev-rules";
              destination = "/etc/udev/rules.d/70-ttlcd.rules";
              text = ''
                # XinWeiYe/Thermaltake 480x480 panel (0x264a:0x2343)
                SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2343", TAG+="uaccess", MODE="0660", GROUP="users"
                # Sibling SKU 128-height panel (0x264a:0x233d)
                SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="233d", TAG+="uaccess", MODE="0660", GROUP="users"
                # Alternate Thermaltake SKU (0x264a:0x2428)
                SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2428", TAG+="uaccess", MODE="0660", GROUP="users"
              '';
            })
          ];

          # A user service: it starts when a user logs in and runs as them,
          # so saved settings live in their ~/.config/ttlcd and the shell,
          # log-tail and notification features act with their permissions.
          systemd.user.services.ttlcd = {
            description = "ttlcd USB LCD panel daemon";
            wantedBy = ["default.target"];
            serviceConfig = {
              ExecStart = lib.concatStrings [
                "${cfg.package}/bin/ttlcd daemon"
                " --port ${toString cfg.port}"
                bindArgs
                " --mode ${cfg.mode}"
                qualityArgs
                rotateArgs
                headlessFlag
                modeArgs
                rotIntervalArgs
                petArgs
                lightsArgs
              ];
              Restart = "on-failure";
              RestartSec = 5;
              Nice = 10;
            };
            environment = {
              RUST_LOG = "info";
            };
          };
        };
      };
    };
}
