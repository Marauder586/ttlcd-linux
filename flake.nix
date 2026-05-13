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
        version = "0.2.0";
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
            description = "Initial display mode.";
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
          systemd.services.ttlcd = {
            description = "ttlcd USB LCD panel daemon";
            wantedBy = ["multi-user.target"];
            after = ["udev.service"];
            serviceConfig = {
              ExecStart = "${cfg.package}/bin/ttlcd daemon --port ${toString cfg.port} --mode ${cfg.mode}";
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
