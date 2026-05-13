//! `ttlcd` — drive XinWeiYe / Thermaltake USB LCD panels from the command line.
//!
//! Run `ttlcd --help` for a full command list.
//! Run `ttlcd daemon` to start the HTTP control server (default port 7070).
//! Run `ttlcd daemon --headless` to try the web UI without the USB device.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use image::{DynamicImage, imageops};
use ttlcd::{
    display::{
        load_slideshow_frames,
        render_clock, render_clock_sysinfo, render_cpu_detail,
        render_disk_usage, render_network, render_process_top,
        render_custom_text, render_sysinfo, render_solid, render_test_card,
    CustomTextPayload,
    },
    encode_for_panel, letterbox_for_panel, Panel, DEFAULT_JPEG_QUALITY, PANEL_HEIGHT, PANEL_WIDTH,
};

// ─── CLI definition ───────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    version,
    about = "Drive XinWeiYe / Thermaltake USB LCD panels (480×480)",
    long_about = None,
)]
struct Cli {
    /// USB vendor ID (hex). Default matches Thermaltake / XinWeiYe.
    #[arg(long, value_parser = parse_hex_u16, default_value = "264a")]
    vid: u16,

    /// USB product ID (hex). Default = 3.95" square TFT, 480×480.
    #[arg(long, value_parser = parse_hex_u16, default_value = "2343")]
    pid: u16,

    /// USB bus reset the device before opening (recovers from abrupt termination).
    #[arg(long)]
    reset: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open device, run init handshake, print device info, then exit.
    Init,

    /// Encode and display a single image file.
    Image {
        path: PathBuf,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        /// Letterbox (keep aspect ratio) instead of stretching.
        #[arg(long)]
        letterbox: bool,
        #[arg(long)]
        no_init: bool,
        /// Rotate the image 0/90/180/270 degrees before sending.
        #[arg(long, default_value_t = 0, value_parser = parse_rotation)]
        rotate: u16,
    },

    /// Display a built-in gradient + diagonal test pattern.
    Test {
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Display SMPTE-style colour bars.
    Testcard {
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Display a solid colour.
    Solid {
        #[arg(default_value_t = 255)]
        r: u8,
        #[arg(default_value_t = 0)]
        g: u8,
        #[arg(default_value_t = 0)]
        b: u8,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Live system-stats dashboard. Refreshes every N seconds.
    Sysinfo {
        #[arg(long, default_value_t = 2)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Large digital clock. Refreshes every second.
    Clock {
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Loop through images in a directory as a slideshow.
    Slideshow {
        dir: PathBuf,
        #[arg(long, default_value_t = 5)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
        #[arg(long)]
        letterbox: bool,
    },

    /// Start an HTTP control server (default port 7070).
    Daemon {
        #[arg(long, default_value_t = 7070)]
        port: u16,
        /// Initial display mode.
        #[arg(long, default_value = "clock_sysinfo", value_parser = parse_mode)]
        mode: String,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        /// Run web server without USB device (renders frames but doesn't send them).
        /// The last rendered frame is served at /api/preview for debugging.
        #[arg(long)]
        headless: bool,
        /// Rotate all output 0/90/180/270 degrees (for panels mounted sideways).
        #[arg(long, default_value_t = 0, value_parser = parse_rotation)]
        rotate: u16,
    },

    /// Live network I/O rates. Refreshes every ~3 seconds (1s for sampling).
    Network {
        #[arg(long, default_value_t = 3)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Per-core CPU usage grid. Refreshes every 2 seconds.
    CpuDetail {
        #[arg(long, default_value_t = 2)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Combined clock + sysinfo (split screen). Refreshes every 2 seconds.
    ClockSysinfo {
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Disk partition usage bars. Refreshes every 10 seconds.
    DiskUsage {
        #[arg(long, default_value_t = 10)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Top processes by CPU usage. Refreshes every ~3 seconds.
    ProcessTop {
        #[arg(long, default_value_t = 3)]
        interval: u64,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_init: bool,
    },

    /// Loop forever sending keepalive packets (prevents screen blank).
    Keepalive,

    /// Protocol debug sweep.
    Debug {
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        no_prelude: bool,
        #[arg(long)]
        no_init_ack: bool,
        #[arg(long, default_value_t = 0)]
        packet_delay_ms: u64,
        #[arg(long, default_value_t = 0)]
        pings: u32,
        #[arg(long, value_parser = parse_hex_u16, default_value = "82")]
        ping_opcode: u16,
        #[arg(long, value_delimiter = ',')]
        solid: Option<Vec<u8>>,
    },

    /// Encode an image to JPEG and write to file (no device I/O).
    Encode {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        #[arg(long)]
        letterbox: bool,
        #[arg(long, default_value_t = 0, value_parser = parse_rotation)]
        rotate: u16,
    },
}

// ─── Argument parsers ─────────────────────────────────────────────────────────

fn parse_hex_u16(s: &str) -> Result<u16, std::num::ParseIntError> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u16::from_str_radix(s, 16)
}

fn parse_mode(s: &str) -> Result<String, String> {
    DisplayMode::from_str_name(s)
        .map(|_| s.to_string())
        .ok_or_else(|| format!(
            "unknown mode '{}'; choose: {}",
            s, DisplayMode::ALL_NAMES.join(", ")
        ))
}

fn parse_rotation(s: &str) -> Result<u16, String> {
    match s {
        "0" | "90" | "180" | "270" => Ok(s.parse().unwrap()),
        _ => Err(format!("rotation must be 0, 90, 180, or 270; got '{s}'")),
    }
}

// ─── Image rotation helper ────────────────────────────────────────────────────

fn apply_rotation(img: DynamicImage, deg: u16) -> DynamicImage {
    match deg {
        90  => DynamicImage::ImageRgb8(imageops::rotate90(&img.into_rgb8())),
        180 => DynamicImage::ImageRgb8(imageops::rotate180(&img.into_rgb8())),
        270 => DynamicImage::ImageRgb8(imageops::rotate270(&img.into_rgb8())),
        _   => img,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn open_panel(vid: u16, pid: u16, reset: bool) -> anyhow::Result<Panel> {
    if reset {
        Panel::open_with_reset(vid, pid)
    } else {
        Panel::open(vid, pid)
    }
}


// ─── Config file ──────────────────────────────────────────────────────────────

/// Persistent settings loaded from `~/.config/ttlcd/config.toml`.
/// All fields are optional; CLI args override these values.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Config {
    device:  ConfigDevice,
    daemon:  ConfigDaemon,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ConfigDevice {
    vid: Option<String>,  // hex, e.g. "264a"
    pid: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ConfigDaemon {
    port:       Option<u16>,
    mode:       Option<String>,
    quality:    Option<u8>,
    rotate:     Option<u16>,
    headless:   Option<bool>,
}

fn load_config() -> Config {
    let path = dirs_config_path();
    let Some(p) = path else { return Config::default() };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(_) => return Config::default(),
    };
    match toml::from_str::<Config>(&text) {
        Ok(c) => {
            log::info!("loaded config from {}", p.display());
            c
        }
        Err(e) => {
            log::warn!("config parse error ({}): {e}", p.display());
            Config::default()
        }
    }
}

fn dirs_config_path() -> Option<std::path::PathBuf> {
    // XDG_CONFIG_HOME or ~/.config
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".config")
        });
    if base.as_os_str().is_empty() { return None; }
    Some(base.join("ttlcd").join("config.toml"))
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let cfg = load_config();
    // Apply config defaults where CLI kept the default value.
    // (This is a simple approach: if CLI has same value as clap default, use config.)
    let _ = &cfg; // used in Daemon arm below

    match cli.cmd {
        Cmd::Init => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            panel.init()?;
            println!("serial   : {}", panel.info.serial);
            println!("firmware : 0x{:02x}", panel.info.firmware_byte);
            println!("status   : 0x{:02x}", panel.info.status_byte);
        }

        Cmd::Image { path, quality, letterbox, no_init, rotate } => {
            let img = image::open(&path)
                .with_context(|| format!("open {}", path.display()))?;
            let img = apply_rotation(img, rotate);
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            if letterbox {
                panel.send_jpeg(&letterbox_for_panel(&img, quality)?)?;
            } else {
                panel.send_image(&img, quality)?;
            }
            log::info!("frame sent");
        }

        Cmd::Test { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            panel.send_image(&test_pattern(), quality)?;
        }

        Cmd::Testcard { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            panel.send_image(&render_test_card(), quality)?;
        }

        Cmd::Solid { r, g, b, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            panel.send_image(&render_solid(r, g, b), quality)?;
        }

        Cmd::Sysinfo { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_sysinfo(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::Clock { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(1) {
                    thread::sleep(Duration::from_secs(1) - elapsed);
                }
            }
        }

        Cmd::Slideshow { dir, interval, quality, no_init, letterbox } => {
            let frames = load_slideshow_frames(&dir);
            if frames.is_empty() {
                anyhow::bail!("No supported images in {}", dir.display());
            }
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                for img in &frames {
                    if letterbox {
                        panel.send_jpeg(&letterbox_for_panel(img, quality)?)?;
                    } else {
                        panel.send_image(img, quality)?;
                    }
                    thread::sleep(Duration::from_secs(interval));
                }
            }
        }

        Cmd::Network { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_network(), quality)?;
                if interval > 1 { thread::sleep(Duration::from_secs(interval - 1)); }
            }
        }

        Cmd::CpuDetail { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_cpu_detail(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::ClockSysinfo { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock_sysinfo(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(2) {
                    thread::sleep(Duration::from_secs(2) - elapsed);
                }
            }
        }

        Cmd::DiskUsage { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_disk_usage(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::ProcessTop { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_process_top(), quality)?;
                if interval > 0 { thread::sleep(Duration::from_secs(interval)); }
            }
        }

        Cmd::Daemon { port, mode, quality, headless, rotate } => {
            // Config file provides defaults; CLI args still override.
            let eff_port     = cfg.daemon.port.unwrap_or(port);
            let eff_mode     = cfg.daemon.mode.clone().unwrap_or(mode);
            let eff_quality  = cfg.daemon.quality.unwrap_or(quality);
            let eff_headless = cfg.daemon.headless.unwrap_or(headless);
            let eff_rotate   = cfg.daemon.rotate.unwrap_or(rotate);
            let eff_vid = cfg.device.vid.as_deref()
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(cli.vid);
            let eff_pid = cfg.device.pid.as_deref()
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(cli.pid);
            run_daemon(eff_vid, eff_pid, eff_port, eff_mode, eff_quality, eff_headless, eff_rotate)?;
        }

        Cmd::Keepalive => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            panel.init()?;
            loop {
                panel.keepalive()?;
                thread::sleep(Duration::from_secs(2));
            }
        }

        Cmd::Debug {
            quality, no_prelude: _, no_init_ack,
            packet_delay_ms, pings, ping_opcode, solid,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if no_init_ack { panel.init_no_ack()?; } else { panel.init()?; }
            for _ in 0..pings {
                panel.send_ctrl_opcode(ping_opcode as u8)?;
                thread::sleep(Duration::from_millis(200));
            }
            let img = if let Some(rgb) = solid {
                render_solid(
                    rgb.first().copied().unwrap_or(255),
                    rgb.get(1).copied().unwrap_or(0),
                    rgb.get(2).copied().unwrap_or(0),
                )
            } else {
                test_pattern()
            };
            let jpeg = encode_for_panel(&img, quality)?;
            panel.send_jpeg_with_delay(&jpeg, packet_delay_ms)?;
            for _ in 0..pings {
                panel.send_ctrl_opcode(ping_opcode as u8)?;
                thread::sleep(Duration::from_millis(200));
            }
        }

        Cmd::Encode { input, output, quality, letterbox, rotate } => {
            let img = image::open(&input)
                .with_context(|| format!("open {}", input.display()))?;
            let img = apply_rotation(img, rotate);
            let bytes = if letterbox {
                letterbox_for_panel(&img, quality)?
            } else {
                encode_for_panel(&img, quality)?
            };
            std::fs::write(&output, &bytes)
                .with_context(|| format!("write {}", output.display()))?;
            log::info!("wrote {} bytes to {}", bytes.len(), output.display());
        }
    }

    Ok(())
}

// ─── Display modes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum DisplayMode {
    Clock,
    ClockSysinfo,
    Sysinfo,
    CpuDetail,
    Network,
    DiskUsage,
    ProcessTop,
    Idle,
    CustomImage,
    /// Custom text + progress bar (driven by POST /api/text).
    CustomText,
}

impl DisplayMode {
    const ALL_NAMES: &'static [&'static str] = &[
        "clock", "clock_sysinfo", "sysinfo", "cpu_detail",
        "network", "disk_usage", "process_top", "idle",
        // custom_text and image are transient; not listed here
    ];

    fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "clock"         => Some(DisplayMode::Clock),
            "clock_sysinfo" => Some(DisplayMode::ClockSysinfo),
            "sysinfo"       => Some(DisplayMode::Sysinfo),
            "cpu_detail"    => Some(DisplayMode::CpuDetail),
            "network"       => Some(DisplayMode::Network),
            "disk_usage"    => Some(DisplayMode::DiskUsage),
            "process_top"   => Some(DisplayMode::ProcessTop),
            "idle"          => Some(DisplayMode::Idle),
            "custom_text"   => Some(DisplayMode::CustomText),
            _               => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            DisplayMode::Clock        => "clock",
            DisplayMode::ClockSysinfo => "clock_sysinfo",
            DisplayMode::Sysinfo      => "sysinfo",
            DisplayMode::CpuDetail    => "cpu_detail",
            DisplayMode::Network      => "network",
            DisplayMode::DiskUsage    => "disk_usage",
            DisplayMode::ProcessTop   => "process_top",
            DisplayMode::Idle         => "idle",
            DisplayMode::CustomImage  => "image",
            DisplayMode::CustomText   => "custom_text",
        }
    }
}

impl std::str::FromStr for DisplayMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_name(s).ok_or_else(|| format!("unknown mode '{}'", s))
    }
}

// ─── Daemon ───────────────────────────────────────────────────────────────────

/// Shared daemon state accessible from both the USB thread and HTTP handlers.
#[derive(Clone)]
struct DaemonState {
    mode:         Arc<Mutex<DisplayMode>>,
    /// Uploaded JPEG waiting to be shown (set by POST /api/image).
    pending_jpeg: Arc<Mutex<Option<Vec<u8>>>>,
    /// Last rendered JPEG (for /api/preview in headless mode).
    last_frame:   Arc<Mutex<Option<Vec<u8>>>>,
    /// Custom text payload (set by POST /api/text).
    pending_text: Arc<Mutex<Option<CustomTextPayload>>>,
    quality:      u8,
    vid:          u16,
    pid:          u16,
    headless:     bool,
    rotate_deg:   u16,
}

#[tokio::main]
async fn run_daemon(
    vid: u16, pid: u16, port: u16,
    mode: String, quality: u8,
    headless: bool, rotate_deg: u16,
) -> Result<()> {
    use actix_web::{web, App, HttpServer};

    let initial_mode: DisplayMode = mode.parse().unwrap_or(DisplayMode::ClockSysinfo);
    let state = DaemonState {
        mode:         Arc::new(Mutex::new(initial_mode)),
        pending_jpeg: Arc::new(Mutex::new(None)),
        last_frame:   Arc::new(Mutex::new(None)),
        pending_text: Arc::new(Mutex::new(None)),
        quality,
        vid,
        pid,
        headless,
        rotate_deg,
    };

    // Open the panel unless headless
    let panel: Arc<Mutex<Option<Panel>>> = if headless {
        log::info!("daemon: headless mode — USB device not opened");
        Arc::new(Mutex::new(None))
    } else {
        let mut p = Panel::open(vid, pid)?;
        p.init()?;
        log::info!(
            "daemon: panel ready (serial={}); mode={}; rotate={}°; listening on :{port}",
            p.info.serial,
            state.mode.lock().unwrap().as_str(),
            rotate_deg
        );
        Arc::new(Mutex::new(Some(p)))
    };

    if headless {
        log::info!(
            "daemon: headless; mode={}; rotate={}°; preview at http://localhost:{port}/api/preview",
            state.mode.lock().unwrap().as_str(),
            rotate_deg
        );
    }

    // ── USB display thread ─────────────────────────────────────────────────
    {
        let panel  = Arc::clone(&panel);
        let state2 = state.clone();
        thread::spawn(move || display_loop(panel, state2));
    }

    // ── HTTP server ────────────────────────────────────────────────────────
    let web_state = web::Data::new(state.clone());

    HttpServer::new(move || {
        App::new()
            .app_data(web_state.clone())
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::PayloadConfig::default().limit(16 * 1024 * 1024))
            // API
            .route("/api/status",   web::get().to(api_status))
            .route("/api/mode",     web::post().to(api_set_mode))
            .route("/api/image",    web::post().to(api_upload_image))
            .route("/api/sysinfo",  web::get().to(api_sysinfo_json))
            .route("/api/preview",  web::get().to(api_preview_frame))
            .route("/api/text",     web::post().to(api_text))
            // Web UI
            .route("/",                  web::get().to(web_ui_index))
            .route("/static/app.css",    web::get().to(web_ui_css))
            .route("/static/app.js",     web::get().to(web_ui_js))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
    .context("HTTP server error")?;

    Ok(())
}

/// Background loop: renders a frame, (optionally rotates it), stores it as the
/// last preview, and sends it to the USB panel if not headless.
fn display_loop(panel: Arc<Mutex<Option<Panel>>>, state: DaemonState) {
    loop {
        let mode    = state.mode.lock().unwrap().clone();
        let quality = state.quality;

        // Renders the next DynamicImage based on the current mode.
        // Returns None for Idle (no frame to send, just keepalive) or
        // CustomImage (handled separately).
        let result: anyhow::Result<()> = (|| {
            match mode {
                DisplayMode::Idle => {
                    // Keepalive only — no rendering
                    let mut guard = panel.lock().unwrap();
                    if let Some(p) = guard.as_mut() {
                        p.keepalive()?;
                    }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomImage => {
                    let jpeg = state.pending_jpeg.lock().unwrap().take();
                    if let Some(j) = jpeg {
                        *state.last_frame.lock().unwrap() = Some(j.clone());
                        let mut guard = panel.lock().unwrap();
                        if let Some(p) = guard.as_mut() {
                            p.send_jpeg(&j)?;
                        }
                    } else {
                        // Image already shown; keepalive
                        let mut guard = panel.lock().unwrap();
                        if let Some(p) = guard.as_mut() {
                            p.keepalive()?;
                        }
                    }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomText => {
                    let payload = state.pending_text.lock().unwrap().clone();
                    if let Some(p) = payload {
                        let img  = render_custom_text(&p);
                        let img  = apply_rotation(img, state.rotate_deg);
                        let jpeg = encode_for_panel(&img, quality)?;
                        *state.last_frame.lock().unwrap() = Some(jpeg.clone());
                        let mut guard = panel.lock().unwrap();
                        if let Some(panel) = guard.as_mut() {
                            panel.send_jpeg(&jpeg)?;
                        }
                    } else {
                        let mut guard = panel.lock().unwrap();
                        if let Some(p) = guard.as_mut() {
                            p.keepalive()?;
                        }
                    }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                _ => {}
            }

            // For all other modes: render → rotate → encode → store → send
            let (img, sleep_secs): (DynamicImage, u64) = match mode {
                DisplayMode::Clock        => (render_clock(),        1),
                DisplayMode::ClockSysinfo => (render_clock_sysinfo(), 2),
                DisplayMode::Sysinfo      => (render_sysinfo(),       2),
                DisplayMode::CpuDetail    => (render_cpu_detail(),    2),
                DisplayMode::Network      => (render_network(),       2), // 1s internal sleep
                DisplayMode::DiskUsage    => (render_disk_usage(),    8),
                DisplayMode::ProcessTop   => (render_process_top(),   3), // ~200ms internal sleep
                // Handled above
                DisplayMode::Idle | DisplayMode::CustomImage | DisplayMode::CustomText => unreachable!(),
            };

            let img = apply_rotation(img, state.rotate_deg);
            let jpeg = encode_for_panel(&img, quality)?;

            *state.last_frame.lock().unwrap() = Some(jpeg.clone());

            {
                let mut guard = panel.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    p.send_jpeg(&jpeg)?;
                }
            }

            thread::sleep(Duration::from_secs(sleep_secs));
            Ok(())
        })();

        if let Err(e) = result {
            log::error!("display loop error: {e:#}");
            log::info!("will retry in 5 s…");
            thread::sleep(Duration::from_secs(5));

            if !state.headless {
                match Panel::open(state.vid, state.pid) {
                    Ok(mut new_panel) => match new_panel.init() {
                        Ok(()) => {
                            log::info!("reconnected to panel (serial={})", new_panel.info.serial);
                            *panel.lock().unwrap() = Some(new_panel);
                        }
                        Err(e) => log::warn!("reconnect init failed: {e}"),
                    },
                    Err(e) => log::warn!("reconnect open failed: {e}"),
                }
            }
        }
    }
}

// ─── HTTP handlers ────────────────────────────────────────────────────────────

use actix_web::{web, HttpResponse, Responder};
use actix_multipart::Multipart;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct StatusResponse {
    mode:            String,
    version:         &'static str,
    available_modes: &'static [&'static str],
    headless:        bool,
    rotate_deg:      u16,
}

async fn api_status(state: web::Data<DaemonState>) -> impl Responder {
    let mode = state.mode.lock().unwrap().as_str().to_string();
    HttpResponse::Ok().json(StatusResponse {
        mode,
        version:         env!("CARGO_PKG_VERSION"),
        available_modes: DisplayMode::ALL_NAMES,
        headless:        state.headless,
        rotate_deg:      state.rotate_deg,
    })
}

#[derive(Deserialize)]
struct SetModeRequest {
    mode: String,
}

async fn api_set_mode(
    state: web::Data<DaemonState>,
    body:  web::Json<SetModeRequest>,
) -> impl Responder {
    match body.mode.parse() {
        Ok(m) => {
            *state.mode.lock().unwrap() = m;
            HttpResponse::Ok().body("ok")
        }
        Err(e) => HttpResponse::BadRequest().body(e),
    }
}

async fn api_upload_image(
    state:  web::Data<DaemonState>,
    mut mp: Multipart,
) -> impl Responder {
    let mut raw_bytes: Vec<u8> = Vec::new();

    while let Some(field) = mp.next().await {
        let mut field = match field {
            Ok(f)  => f,
            Err(e) => return HttpResponse::BadRequest().body(format!("multipart error: {e}")),
        };
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(c)  => raw_bytes.extend_from_slice(&c),
                Err(e) => return HttpResponse::BadRequest().body(format!("chunk error: {e}")),
            }
        }
    }

    if raw_bytes.is_empty() {
        return HttpResponse::BadRequest().body("no image data");
    }

    let quality    = state.quality;
    let rotate_deg = state.rotate_deg;

    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = image::load_from_memory(&raw_bytes)?;
        let img = apply_rotation(img, rotate_deg);
        letterbox_for_panel(&img, quality)
    })() {
        Ok(j)  => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("decode error: {e}")),
    };

    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;

    HttpResponse::Ok().body("image queued")
}

/// Push custom text + optional progress bar to the display.
///
/// The display mode is switched to `custom_text` and the payload is cached
/// until a new `POST /api/text` or `POST /api/mode` request arrives.
///
/// Example body:
/// ```json
/// {"title":"Build","lines":["Running tests…","17 / 20 pass"],"progress":0.85,"color":"green"}
/// ```
async fn api_text(
    state: web::Data<DaemonState>,
    body:  web::Json<CustomTextPayload>,
) -> impl Responder {
    *state.pending_text.lock().unwrap() = Some(body.into_inner());
    *state.mode.lock().unwrap()         = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

/// Serves the last rendered frame as JPEG (great for headless testing).
async fn api_preview_frame(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => HttpResponse::Ok()
            .content_type("image/jpeg")
            .body(jpeg.clone()),
        None => HttpResponse::NoContent().finish(),
    }
}

/// Live system snapshot as JSON.
#[derive(Serialize)]
struct SysinfoJson {
    cpu_pct:   f32,
    mem_used:  u64,
    mem_total: u64,
    uptime:    u64,
    load1:     f64,
    load5:     f64,
    load15:    f64,
    hostname:  String,
}

async fn api_sysinfo_json() -> impl Responder {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let load = System::load_average();
    HttpResponse::Ok().json(SysinfoJson {
        cpu_pct:   sys.global_cpu_usage(),
        mem_used:  sys.used_memory(),
        mem_total: sys.total_memory(),
        uptime:    System::uptime(),
        load1:     load.one,
        load5:     load.five,
        load15:    load.fifteen,
        hostname:  System::host_name().unwrap_or_default(),
    })
}

// ─── Embedded web UI ─────────────────────────────────────────────────────────

async fn web_ui_index() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../src/web/index.html"))
}

async fn web_ui_css() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/css")
        .body(include_str!("../../src/web/app.css"))
}

async fn web_ui_js() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/javascript")
        .body(include_str!("../../src/web/app.js"))
}

// ─── Gradient test pattern ────────────────────────────────────────────────────

fn test_pattern() -> DynamicImage {
    let mut img = image::RgbImage::new(PANEL_WIDTH, PANEL_HEIGHT);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let r = (x * 255 / (PANEL_WIDTH - 1)) as u8;
        let g = (y * 255 / (PANEL_HEIGHT - 1)) as u8;
        let b = (((x + y) * 255) / (PANEL_WIDTH + PANEL_HEIGHT - 2)) as u8;
        let on_diag = x == y || x + y == PANEL_WIDTH - 1;
        let on_edge = x < 4 || y < 4 || x >= PANEL_WIDTH - 4 || y >= PANEL_HEIGHT - 4;
        *px = if on_diag || on_edge {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([r, g, b])
        };
    }
    DynamicImage::ImageRgb8(img)
}
