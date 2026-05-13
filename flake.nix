{
  description = "ttlcd-rs — Rust driver for XinWeiYe/Thermaltake USB LCD panels";

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
      };

      # ── package derivation ───────────────────────────────────────
      ttlcd = pkgs.rustPlatform.buildRustPackage {
        pname = "ttlcd";
        version = "0.9.0";
        src = ./.;

        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [pkg-config];
        buildInputs = with pkgs; [libusb1];

        # The binary needs libusb at runtime
        postInstall = ''
          wrapProgram $out/bin/ttlcd \
            --prefix LD_LIBRARY_PATH : "${pkgs.libusb1}/lib"
        '';

        meta = {
          description = "USB LCD panel driver and display suite (XinWeiYe / Thermaltake)";
          homepage = "https://github.com/marauder/ttlcd-rs";
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
          # Useful for testing the web UI without a device
          pkgs.websocat
          pkgs.curl
        ];
        env.RUST_BACKTRACE = "1";
        env.RUST_LOG = "info";
      };

      # ── NixOS module ─────────────────────────────────────────────
      # Import as:  imports = [ ttlcd-rs.nixosModules.default ];
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
            description = "Initial display mode on startup.";
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
            description = "Bind address for the HTTP server (use 0.0.0.0 to listen on all interfaces).";
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

          user = lib.mkOption {
            type = lib.types.str;
            default = "marauder";
            description = "User to run the daemon as.";
          };

          package = lib.mkOption {
            type = lib.types.package;
            default = ttlcd;
            description = "ttlcd package to use.";
          };
        };

        config = lib.mkIf cfg.enable {
          # Udev rule: grant seat-local access to the USB panel without root.
          # TAG+="uaccess" handles loginctl seat sessions; MODE/GROUP covers
          # non-seat services running as cfg.user.
          services.udev.extraRules = ''
            # XinWeiYe/Thermaltake 480x480 panel (0x264a:0x2343)
            SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2343", TAG+="uaccess", MODE="0660", GROUP="users"
            # Sibling SKU 128-height panel (0x264a:0x233d)
            SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="233d", TAG+="uaccess", MODE="0660", GROUP="users"
            # Alternate Thermaltake SKU (0x264a:0x2428)
            SUBSYSTEM=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2428", TAG+="uaccess", MODE="0660", GROUP="users"
          '';

          systemd.services.ttlcd = {
            description = "ttlcd USB LCD panel daemon";
            wantedBy = ["multi-user.target"];
            after = ["udev.service"];
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
              ];
              User = cfg.user;
              Restart = "on-failure";
              RestartSec = 5;
              # Allow USB device access
              SupplementaryGroups = ["users"];
              # Nice level — rendering is CPU-light
              Nice = 10;
            };
            environment = {
              RUST_LOG = "info";
            };
          };
        };
      };
    });
}
