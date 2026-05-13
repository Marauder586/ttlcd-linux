//! `ttlcd` — drive XinWeiYe / Thermaltake USB LCD panels from the command line.
//!
//! Run `ttlcd --help` for a full command list.
//! Run `ttlcd daemon` to start the HTTP control server (default port 7070).
//! Run `ttlcd daemon --headless` to try the web UI without the USB device.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
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
        render_temperature, render_memory_detail, render_countdown,
        render_cpu_graph, push_cpu_history,
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
        #[arg(long)]
        letterbox: bool,
        #[arg(long)]
        no_init: bool,
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
        #[arg(long, default_value = "clock_sysinfo", value_parser = parse_mode)]
        mode: String,
        #[arg(long, default_value_t = DEFAULT_JPEG_QUALITY)]
        quality: u8,
        /// Run without USB device (renders frames but doesn't send them).
        #[arg(long)]
        headless: bool,
        /// Rotate all output 0/90/180/270 degrees.
        #[arg(long, default_value_t = 0, value_parser = parse_rotation)]
        rotate: u16,
        /// Comma-separated list of modes to auto-rotate (e.g. clock,sysinfo,cpu_detail).
        #[arg(long, value_delimiter = ',')]
        rotation_modes: Vec<String>,
        /// Seconds between auto-rotation advances (0 = disabled).
        #[arg(long, default_value_t = 0)]
        rotation_interval: u64,
    },

    /// Live network I/O rates. Refreshes every ~3 seconds.
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

// ─── Misc helpers ─────────────────────────────────────────────────────────────

fn open_panel(vid: u16, pid: u16, reset: bool) -> anyhow::Result<Panel> {
    if reset { Panel::open_with_reset(vid, pid) } else { Panel::open(vid, pid) }
}

/// Collect sorted image file paths from a directory.
fn collect_image_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.flatten())
        .map(|e| e.path())
        .filter(|p| matches!(
            p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "bmp" | "gif"
        ))
        .collect();
    paths.sort();
    paths
}

// ─── Config file ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Config {
    device: ConfigDevice,
    daemon: ConfigDaemon,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ConfigDevice {
    vid: Option<String>,
    pid: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ConfigDaemon {
    port:              Option<u16>,
    mode:              Option<String>,
    quality:           Option<u8>,
    rotate:            Option<u16>,
    headless:          Option<bool>,
    /// Comma-separated or array of mode names to cycle through.
    rotation_modes:    Option<Vec<String>>,
    /// Seconds between auto-rotation advances (0 = disabled).
    rotation_interval: Option<u64>,
}

fn load_config() -> Config {
    let Some(p) = dirs_config_path() else { return Config::default() };
    match std::fs::read_to_string(&p) {
        Err(_) => Config::default(),
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(c)  => { log::info!("loaded config from {}", p.display()); c }
            Err(e) => { log::warn!("config parse error ({}): {e}", p.display()); Config::default() }
        },
    }
}

fn dirs_config_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    if base.as_os_str().is_empty() { return None; }
    Some(base.join("ttlcd").join("config.toml"))
}

// ─── Preset persistence ───────────────────────────────────────────────────────

fn presets_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("presets.json"))
}

fn load_presets_from_disk() -> HashMap<String, CustomTextPayload> {
    (|| -> Option<HashMap<String, CustomTextPayload>> {
        let path = presets_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

fn save_presets_to_disk(presets: &HashMap<String, CustomTextPayload>) {
    let path = match presets_path() {
        Some(p) => p,
        None    => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(presets) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, &text) {
                log::warn!("could not save presets to {}: {e}", path.display());
            }
        }
        Err(e) => log::warn!("could not serialise presets: {e}"),
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let cfg = load_config();

    match cli.cmd {
        Cmd::Init => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            panel.init()?;
            println!("serial   : {}", panel.info.serial);
            println!("firmware : 0x{:02x}", panel.info.firmware_byte);
            println!("status   : 0x{:02x}", panel.info.status_byte);
        }

        Cmd::Image { path, quality, letterbox, no_init, rotate } => {
            let img = image::open(&path).with_context(|| format!("open {}", path.display()))?;
            let img = apply_rotation(img, rotate);
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            if letterbox { panel.send_jpeg(&letterbox_for_panel(&img, quality)?)?; }
            else         { panel.send_image(&img, quality)?; }
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
            loop { panel.send_image(&render_sysinfo(), quality)?; thread::sleep(Duration::from_secs(interval)); }
        }

        Cmd::Clock { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(1) { thread::sleep(Duration::from_secs(1) - elapsed); }
            }
        }

        Cmd::Slideshow { dir, interval, quality, no_init, letterbox } => {
            let frames = load_slideshow_frames(&dir);
            if frames.is_empty() { anyhow::bail!("No supported images in {}", dir.display()); }
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                for img in &frames {
                    if letterbox { panel.send_jpeg(&letterbox_for_panel(img, quality)?)?; }
                    else         { panel.send_image(img, quality)?; }
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
            loop { panel.send_image(&render_cpu_detail(), quality)?; thread::sleep(Duration::from_secs(interval)); }
        }

        Cmd::ClockSysinfo { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock_sysinfo(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(2) { thread::sleep(Duration::from_secs(2) - elapsed); }
            }
        }

        Cmd::DiskUsage { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop { panel.send_image(&render_disk_usage(), quality)?; thread::sleep(Duration::from_secs(interval)); }
        }

        Cmd::ProcessTop { interval, quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init { panel.init()?; }
            loop {
                panel.send_image(&render_process_top(), quality)?;
                if interval > 0 { thread::sleep(Duration::from_secs(interval)); }
            }
        }

        Cmd::Daemon { port, mode, quality, headless, rotate,
                      rotation_modes, rotation_interval } => {
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
            // Rotation: CLI takes precedence over config
            let eff_rot_modes = if !rotation_modes.is_empty() {
                rotation_modes
            } else {
                cfg.daemon.rotation_modes.unwrap_or_default()
            };
            let eff_rot_interval = if rotation_interval > 0 {
                rotation_interval
            } else {
                cfg.daemon.rotation_interval.unwrap_or(0)
            };
            run_daemon(
                eff_vid, eff_pid, eff_port, eff_mode, eff_quality,
                eff_headless, eff_rotate, eff_rot_modes, eff_rot_interval,
            )?;
        }

        Cmd::Keepalive => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            panel.init()?;
            loop { panel.keepalive()?; thread::sleep(Duration::from_secs(2)); }
        }

        Cmd::Debug { quality, no_prelude: _, no_init_ack,
                     packet_delay_ms, pings, ping_opcode, solid } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if no_init_ack { panel.init_no_ack()?; } else { panel.init()?; }
            for _ in 0..pings {
                panel.send_ctrl_opcode(ping_opcode as u8)?;
                thread::sleep(Duration::from_millis(200));
            }
            let img = if let Some(rgb) = solid {
                render_solid(rgb.first().copied().unwrap_or(255),
                             rgb.get(1).copied().unwrap_or(0),
                             rgb.get(2).copied().unwrap_or(0))
            } else { test_pattern() };
            panel.send_jpeg_with_delay(&encode_for_panel(&img, quality)?, packet_delay_ms)?;
            for _ in 0..pings {
                panel.send_ctrl_opcode(ping_opcode as u8)?;
                thread::sleep(Duration::from_millis(200));
            }
        }

        Cmd::Encode { input, output, quality, letterbox, rotate } => {
            let img = image::open(&input).with_context(|| format!("open {}", input.display()))?;
            let img = apply_rotation(img, rotate);
            let bytes = if letterbox { letterbox_for_panel(&img, quality)? } else { encode_for_panel(&img, quality)? };
            std::fs::write(&output, &bytes).with_context(|| format!("write {}", output.display()))?;
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
    Temperature,
    MemoryDetail,
    Countdown,
    CpuGraph,
    Idle,
    CustomImage,
    CustomText,
    Slideshow,
}

impl DisplayMode {
    const ALL_NAMES: &'static [&'static str] = &[
        "clock", "clock_sysinfo", "sysinfo", "cpu_detail",
        "network", "disk_usage", "process_top", "temperature", "memory_detail", "countdown", "cpu_graph", "idle",
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
            "temperature"   => Some(DisplayMode::Temperature),
            "memory_detail" => Some(DisplayMode::MemoryDetail),
            "countdown"    => Some(DisplayMode::Countdown),
            "cpu_graph"    => Some(DisplayMode::CpuGraph),
            "idle"          => Some(DisplayMode::Idle),
            "custom_text"   => Some(DisplayMode::CustomText),
            "slideshow"     => Some(DisplayMode::Slideshow),
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
            DisplayMode::Temperature  => "temperature",
            DisplayMode::MemoryDetail => "memory_detail",
            DisplayMode::Countdown   => "countdown",
            DisplayMode::CpuGraph    => "cpu_graph",
            DisplayMode::Idle         => "idle",
            DisplayMode::CustomImage  => "image",
            DisplayMode::CustomText   => "custom_text",
            DisplayMode::Slideshow    => "slideshow",
        }
    }

    /// True for modes where content was pushed by the user and they may want
    /// to "return" to the previous live stats mode.
    fn is_transient(&self) -> bool {
        matches!(self, DisplayMode::CustomText | DisplayMode::CustomImage | DisplayMode::Slideshow | DisplayMode::Countdown)
    }
}

impl std::str::FromStr for DisplayMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_name(s).ok_or_else(|| format!("unknown mode '{}'", s))
    }
}

// ─── Daemon ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct DaemonState {
    mode:         Arc<Mutex<DisplayMode>>,
    /// Mode before transitioning to a transient mode.
    prev_mode:    Arc<Mutex<DisplayMode>>,
    pending_jpeg: Arc<Mutex<Option<Vec<u8>>>>,
    last_frame:   Arc<Mutex<Option<Vec<u8>>>>,
    /// Monotonically-increasing counter; incremented with each new stored frame.
    frame_seq:    Arc<Mutex<u64>>,
    pending_text: Arc<Mutex<Option<CustomTextPayload>>>,
    /// Named presets (in-memory; persisted to ~/.config/ttlcd/presets.json).
    presets:      Arc<Mutex<HashMap<String, CustomTextPayload>>>,
    slideshow_paths:    Arc<Mutex<Vec<PathBuf>>>,
    slideshow_idx:      Arc<Mutex<usize>>,
    slideshow_interval: Arc<Mutex<u64>>,
    /// Ordered list of modes for auto-rotation (empty = disabled).
    rotation_modes:    Arc<Mutex<Vec<DisplayMode>>>,
    /// Seconds between auto-rotation advances (0 = disabled).
    rotation_interval: Arc<Mutex<u64>>,
    quality:    Arc<Mutex<u8>>,
    vid:        u16,
    pid:        u16,
    headless:   bool,
    rotate_deg: Arc<Mutex<u16>>,
    /// Ring-buffer of the last 50 text/alert notifications sent to the panel.
    history:    Arc<Mutex<VecDeque<HistoryEntry>>>,
    /// Countdown target (Unix seconds) + total duration + label.
    countdown:  Arc<Mutex<Option<CountdownState>>>,
    /// Auto-clear: if set, CustomText/CustomImage reverts to prev_mode at this Unix time.
    auto_clear_at: Arc<Mutex<Option<u64>>>,
    /// Count of pending scheduled tasks (for status display).
    pending_scheduled: Arc<Mutex<u32>>,
}

#[tokio::main]
#[allow(clippy::too_many_arguments)]
async fn run_daemon(
    vid: u16, pid: u16, port: u16,
    mode: String, quality: u8,
    headless: bool, rotate_deg: u16,
    rotation_mode_names: Vec<String>,
    rotation_interval: u64,
) -> Result<()> {
    use actix_web::{web, App, HttpServer};

    let initial_mode: DisplayMode = mode.parse().unwrap_or(DisplayMode::ClockSysinfo);

    // Parse rotation modes (ignore unknown and transient ones)
    let rotation_modes: Vec<DisplayMode> = rotation_mode_names.iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .filter(|m| !m.is_transient())
        .collect();

    if rotation_interval > 0 && !rotation_modes.is_empty() {
        log::info!(
            "auto-rotation enabled: {} modes, {}s interval",
            rotation_modes.len(), rotation_interval
        );
    }

    // Load persisted presets
    let saved_presets = load_presets_from_disk();
    if !saved_presets.is_empty() {
        log::info!("loaded {} preset(s) from disk", saved_presets.len());
    }

    let state = DaemonState {
        prev_mode:          Arc::new(Mutex::new(initial_mode.clone())),
        mode:               Arc::new(Mutex::new(initial_mode)),
        pending_jpeg:       Arc::new(Mutex::new(None)),
        last_frame:         Arc::new(Mutex::new(None)),
        frame_seq:          Arc::new(Mutex::new(0)),
        pending_text:       Arc::new(Mutex::new(None)),
        presets:            Arc::new(Mutex::new(saved_presets)),
        slideshow_paths:    Arc::new(Mutex::new(Vec::new())),
        slideshow_idx:      Arc::new(Mutex::new(0)),
        slideshow_interval: Arc::new(Mutex::new(5)),
        rotation_modes:     Arc::new(Mutex::new(rotation_modes)),
        rotation_interval:  Arc::new(Mutex::new(rotation_interval)),
        quality:    Arc::new(Mutex::new(quality)),
        vid,
        pid,
        headless,
        rotate_deg: Arc::new(Mutex::new(rotate_deg)),
        history:    Arc::new(Mutex::new(VecDeque::with_capacity(50))),
        countdown:  Arc::new(Mutex::new(None)),
        auto_clear_at:     Arc::new(Mutex::new(None)),
        pending_scheduled: Arc::new(Mutex::new(0)),
    };

    let panel: Arc<Mutex<Option<Panel>>> = if headless {
        log::info!("daemon: headless mode — USB device not opened");
        Arc::new(Mutex::new(None))
    } else {
        let mut p = Panel::open(vid, pid)?;
        p.init()?;
        log::info!(
            "daemon: panel ready (serial={}); mode={}; rotate={}°; port={}",
            p.info.serial, state.mode.lock().unwrap().as_str(), rotate_deg, port
        );
        Arc::new(Mutex::new(Some(p)))
    };

    if headless {
        log::info!(
            "daemon: headless; mode={}; web UI → http://localhost:{port}/",
            state.mode.lock().unwrap().as_str()
        );
    }

    // ── Display thread ────────────────────────────────────────────────────
    {
        let panel2 = Arc::clone(&panel);
        let state2 = state.clone();
        thread::spawn(move || display_loop(panel2, state2));
    }

    // ── Auto-rotation thread ──────────────────────────────────────────────
    {
        let state3 = state.clone();
        thread::spawn(move || rotation_loop(state3));
    }

    // ── HTTP server ────────────────────────────────────────────────────────
    let web_state = web::Data::new(state.clone());

    HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::DefaultHeaders::new()
                .add(("Access-Control-Allow-Origin",  "*"))
                .add(("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS"))
                .add(("Access-Control-Allow-Headers", "Content-Type")))
            .app_data(web_state.clone())
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::PayloadConfig::default().limit(16 * 1024 * 1024))
            // Core API
            .route("/api/status",         web::get().to(api_status))
            .route("/api/mode",           web::post().to(api_set_mode))
            .route("/api/image",          web::post().to(api_upload_image))
            .route("/api/image/path",     web::post().to(api_image_path))
            .route("/api/render",         web::post().to(api_render))
            .route("/api/sysinfo",        web::get().to(api_sysinfo_json))
            .route("/api/preview",        web::get().to(api_preview_frame))
            .route("/api/text",           web::post().to(api_text))
            .route("/api/clear",          web::post().to(api_clear))
            // Presets API (full CRUD)
            .route("/api/presets",          web::get().to(api_presets_list))
            .route("/api/presets",          web::post().to(api_presets_save))
            .route("/api/presets/{name}",   web::get().to(api_presets_get))
            .route("/api/presets/{name}",   web::post().to(api_presets_apply))
            .route("/api/presets/{name}",   web::delete().to(api_presets_delete))
            // Alert shorthand & screenshot download
            .route("/api/alert",          web::post().to(api_alert))
            .route("/api/screenshot",     web::get().to(api_screenshot))
            // Mode listing and navigation
            .route("/api/modes",          web::get().to(api_modes_list))
            .route("/api/mode/next",      web::post().to(api_mode_next))
            .route("/api/mode/prev",      web::post().to(api_mode_prev))
            // Notification history
            .route("/api/history",        web::get().to(api_history))
            .route("/api/history",        web::delete().to(api_history_clear))
            // Runtime quality and orientation
            .route("/api/quality",        web::post().to(api_set_quality))
            .route("/api/orient",         web::post().to(api_set_orient))
            // Health check, blank, countdown
            .route("/health",             web::get().to(api_health))
            .route("/api/blank",          web::post().to(api_blank))
            .route("/api/countdown",      web::post().to(api_countdown))
            // Prometheus metrics
            .route("/metrics",            web::get().to(api_metrics))
            // Clear all presets
            .route("/api/presets",        web::delete().to(api_presets_clear))
            // Slideshow & rotation
            .route("/api/slideshow",      web::post().to(api_slideshow))
            .route("/api/slideshow/next", web::post().to(api_slideshow_next))
            .route("/api/slideshow/prev", web::post().to(api_slideshow_prev))
            .route("/api/rotation",       web::post().to(api_rotation))
            // Convenience endpoints
            .route("/api/notify",         web::post().to(api_notify))
            .route("/api/webhook",        web::post().to(api_webhook))
            .route("/api/config",         web::get().to(api_get_config))
            .route("/api/schedule",       web::post().to(api_schedule))
            // Server-Sent Events
            .route("/api/events",         web::get().to(api_events))
            // MJPEG live stream
            .route("/api/stream",         web::get().to(api_mjpeg_stream))
            // Web UI
            .route("/",                   web::get().to(web_ui_index))
            .route("/static/app.css",     web::get().to(web_ui_css))
            .route("/static/app.js",      web::get().to(web_ui_js))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
    .context("HTTP server error")?;

    Ok(())
}

// ─── Store a new frame ────────────────────────────────────────────────────────

fn store_frame(state: &DaemonState, jpeg: Vec<u8>) -> Vec<u8> {
    *state.last_frame.lock().unwrap() = Some(jpeg.clone());
    *state.frame_seq.lock().unwrap() += 1;
    jpeg
}

// ─── Display loop ─────────────────────────────────────────────────────────────

fn display_loop(panel: Arc<Mutex<Option<Panel>>>, state: DaemonState) {
    loop {
        let mode       = state.mode.lock().unwrap().clone();
        let quality    = *state.quality.lock().unwrap();
        let rotate_deg = *state.rotate_deg.lock().unwrap();

        let result: anyhow::Result<()> = (|| {
            match mode {
                DisplayMode::Idle => {
                    let mut g = panel.lock().unwrap();
                    if let Some(p) = g.as_mut() { p.keepalive()?; }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomImage => {
                    let jpeg = state.pending_jpeg.lock().unwrap().take();
                    if let Some(j) = jpeg {
                        let j = store_frame(&state, j);
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() { p.send_jpeg(&j)?; }
                    } else {
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() { p.keepalive()?; }
                    }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomText => {
                    // Auto-expiry check
                    {
                        let exp = *state.auto_clear_at.lock().unwrap();
                        if let Some(t) = exp {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs()).unwrap_or(0);
                            if now >= t {
                                *state.auto_clear_at.lock().unwrap() = None;
                                *state.pending_text.lock().unwrap() = None;
                                let prev = state.prev_mode.lock().unwrap().clone();
                                *state.mode.lock().unwrap() = prev;
                                thread::sleep(Duration::from_millis(100));
                                return Ok(());
                            }
                        }
                    }
                    let payload = state.pending_text.lock().unwrap().clone();
                    if let Some(p) = payload {
                        let img  = render_custom_text(&p);
                        let img  = apply_rotation(img, rotate_deg);
                        let jpeg = encode_for_panel(&img, quality)?;
                        let jpeg = store_frame(&state, jpeg);
                        let mut g = panel.lock().unwrap();
                        if let Some(panel) = g.as_mut() { panel.send_jpeg(&jpeg)?; }
                    } else {
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() { p.keepalive()?; }
                    }
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::Slideshow => {
                    let paths = state.slideshow_paths.lock().unwrap().clone();
                    if paths.is_empty() {
                        let placeholder = render_custom_text(&CustomTextPayload {
                            title:    Some("Slideshow".into()),
                            subtitle: Some("No images loaded".into()),
                            ..Default::default()
                        });
                        let placeholder = apply_rotation(placeholder, rotate_deg);
                        let jpeg = store_frame(&state, encode_for_panel(&placeholder, quality)?);
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() { p.send_jpeg(&jpeg)?; }
                        thread::sleep(Duration::from_secs(2));
                        return Ok(());
                    }
                    let idx = {
                        let mut g = state.slideshow_idx.lock().unwrap();
                        let i = *g;
                        *g = (i + 1) % paths.len();
                        i
                    };
                    let interval = *state.slideshow_interval.lock().unwrap();
                    let img  = image::open(&paths[idx])?;
                    let img  = apply_rotation(img, rotate_deg);
                    let jpeg = letterbox_for_panel(&img, quality)?;
                    let jpeg = store_frame(&state, jpeg);
                    { let mut g = panel.lock().unwrap(); if let Some(p) = g.as_mut() { p.send_jpeg(&jpeg)?; } }
                    thread::sleep(Duration::from_secs(interval));
                    return Ok(());
                }
                _ => {}
            }

            let (img, sleep_secs): (DynamicImage, u64) = match mode {
                DisplayMode::Clock        => (render_clock(),         1),
                DisplayMode::ClockSysinfo => (render_clock_sysinfo(), 2),
                DisplayMode::Sysinfo      => (render_sysinfo(),        2),
                DisplayMode::CpuDetail    => (render_cpu_detail(),     2),
                DisplayMode::Network      => (render_network(),        2),
                DisplayMode::DiskUsage    => (render_disk_usage(),     8),
                DisplayMode::ProcessTop   => (render_process_top(),    3),
                DisplayMode::Temperature  => (render_temperature(),    5),
                DisplayMode::MemoryDetail => (render_memory_detail(),  4),
                DisplayMode::CpuGraph     => (render_cpu_graph(),      1),
                DisplayMode::Countdown => {
                    let cd_opt = state.countdown.lock().unwrap().clone();
                    if let Some(cd) = cd_opt {
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let remaining = (cd.target_unix as i64) - (now_secs as i64);
                        (render_countdown(remaining, cd.total_secs as i64, &cd.label), 1)
                    } else {
                        (render_custom_text(&CustomTextPayload {
                            title: Some("No countdown set".into()),
                            ..Default::default()
                        }), 5)
                    }
                },
                DisplayMode::Idle
                | DisplayMode::CustomImage
                | DisplayMode::CustomText
                | DisplayMode::Slideshow => unreachable!("handled above"),
            };

            // Update CPU history for the graph mode (on any live frame)
            if matches!(mode, DisplayMode::Clock | DisplayMode::ClockSysinfo | DisplayMode::Sysinfo
                | DisplayMode::CpuDetail | DisplayMode::Network | DisplayMode::DiskUsage
                | DisplayMode::ProcessTop | DisplayMode::Temperature | DisplayMode::MemoryDetail) {
                use sysinfo::System;
                let mut s = System::new();
                s.refresh_cpu_usage();
                push_cpu_history(s.global_cpu_usage());
            }
            let img  = apply_rotation(img, rotate_deg);
            let jpeg = store_frame(&state, encode_for_panel(&img, quality)?);
            { let mut g = panel.lock().unwrap(); if let Some(p) = g.as_mut() { p.send_jpeg(&jpeg)?; } }
            thread::sleep(Duration::from_secs(sleep_secs));
            Ok(())
        })();

        if let Err(e) = result {
            log::error!("display loop error: {e:#}");
            thread::sleep(Duration::from_secs(5));
            if !state.headless {
                match Panel::open(state.vid, state.pid) {
                    Ok(mut p) => match p.init() {
                        Ok(()) => { log::info!("reconnected (serial={})", p.info.serial); *panel.lock().unwrap() = Some(p); }
                        Err(e) => log::warn!("reconnect init failed: {e}"),
                    },
                    Err(e) => log::warn!("reconnect open failed: {e}"),
                }
            }
        }
    }
}

// ─── Auto-rotation loop ───────────────────────────────────────────────────────

fn rotation_loop(state: DaemonState) {
    loop {
        let interval = *state.rotation_interval.lock().unwrap();
        if interval == 0 {
            // Rotation is disabled; check again after a short sleep.
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        thread::sleep(Duration::from_secs(interval));

        // Only advance if current mode is not transient (user-pushed content).
        let cur = state.mode.lock().unwrap().clone();
        if cur.is_transient() { continue; }

        let modes = state.rotation_modes.lock().unwrap().clone();
        if modes.is_empty() { continue; }

        // Find current mode's position in the rotation list; advance from there.
        let cur_pos = modes.iter().position(|m| m == &cur).unwrap_or(0);
        let next    = modes[(cur_pos + 1) % modes.len()].clone();
        log::info!("auto-rotation: {} → {}", cur.as_str(), next.as_str());
        *state.prev_mode.lock().unwrap() = next.clone();
        *state.mode.lock().unwrap()      = next;
    }
}

// ─── HTTP handlers ────────────────────────────────────────────────────────────

use actix_web::{web, HttpResponse, Responder};
use actix_multipart::Multipart;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct StatusResponse {
    mode:                  String,
    version:               &'static str,
    available_modes:       &'static [&'static str],
    headless:              bool,
    rotate_deg:            u16,
    quality:               u8,
    frame_seq:             u64,
    can_clear:             bool,
    rotation_interval:     u64,
    rotation_modes:        Vec<String>,
    slideshow_image_count: usize,
    slideshow_interval:    u64,
    slideshow_current_idx: usize,
    pending_scheduled:    u32,
}

async fn api_status(state: web::Data<DaemonState>) -> impl Responder {
    let mode      = state.mode.lock().unwrap().as_str().to_string();
    let frame_seq = *state.frame_seq.lock().unwrap();
    let can_clear = state.mode.lock().unwrap().is_transient();
    let rot_interval = *state.rotation_interval.lock().unwrap();
    let rot_modes: Vec<String> = state.rotation_modes.lock().unwrap()
        .iter().map(|m| m.as_str().to_string()).collect();
    let ss_count = state.slideshow_paths.lock().unwrap().len();
    let ss_interval = *state.slideshow_interval.lock().unwrap();
    let ss_idx = *state.slideshow_idx.lock().unwrap();
    HttpResponse::Ok().json(StatusResponse {
        mode,
        version:               env!("CARGO_PKG_VERSION"),
        available_modes:       DisplayMode::ALL_NAMES,
        headless:              state.headless,
        rotate_deg:            *state.rotate_deg.lock().unwrap(),
        quality:               *state.quality.lock().unwrap(),
        frame_seq,
        can_clear,
        rotation_interval:     rot_interval,
        rotation_modes:        rot_modes,
        slideshow_image_count: ss_count,
        slideshow_interval:    ss_interval,
        slideshow_current_idx: ss_idx,
        pending_scheduled:     *state.pending_scheduled.lock().unwrap(),
    })
}

#[derive(Deserialize)]
struct SetModeRequest { mode: String }

async fn api_set_mode(state: web::Data<DaemonState>, body: web::Json<SetModeRequest>) -> impl Responder {
    match body.mode.parse::<DisplayMode>() {
        Ok(new_mode) => {
            if !new_mode.is_transient() {
                *state.pending_text.lock().unwrap() = None;
                *state.pending_jpeg.lock().unwrap() = None;
                *state.prev_mode.lock().unwrap()    = new_mode.clone();
            }
            *state.mode.lock().unwrap() = new_mode;
            HttpResponse::Ok().body("ok")
        }
        Err(e) => HttpResponse::BadRequest().body(e),
    }
}

async fn api_upload_image(state: web::Data<DaemonState>, mut mp: Multipart) -> impl Responder {
    let mut raw: Vec<u8> = Vec::new();
    while let Some(field) = mp.next().await {
        let mut field = match field {
            Ok(f)  => f,
            Err(e) => return HttpResponse::BadRequest().body(format!("multipart error: {e}")),
        };
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(c)  => raw.extend_from_slice(&c),
                Err(e) => return HttpResponse::BadRequest().body(format!("chunk error: {e}")),
            }
        }
    }
    if raw.is_empty() { return HttpResponse::BadRequest().body("no image data"); }

    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = image::load_from_memory(&raw)?;
        letterbox_for_panel(&apply_rotation(img, *state.rotate_deg.lock().unwrap()), *state.quality.lock().unwrap())
    })() {
        Ok(j)  => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("decode error: {e}")),
    };

    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;
    HttpResponse::Ok().body("image queued")
}

async fn api_text(state: web::Data<DaemonState>, body: web::Json<CustomTextPayload>) -> impl Responder {
    let payload = body.into_inner();
    let summary = payload.title.clone().unwrap_or_else(|| "(no title)".into());
    // Set auto-expiry if duration_secs was provided
    if let Some(dur) = payload.duration_secs {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        *state.auto_clear_at.lock().unwrap() = Some(now + dur);
    } else {
        *state.auto_clear_at.lock().unwrap() = None;
    }
    push_history(&state, HistoryKind::Text, summary);
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap()         = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

async fn api_clear(state: web::Data<DaemonState>) -> impl Responder {
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap()         = prev;
    *state.pending_text.lock().unwrap() = None;
    *state.pending_jpeg.lock().unwrap() = None;
    *state.auto_clear_at.lock().unwrap() = None;
    HttpResponse::Ok().body("ok")
}

// ─── Presets ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct PresetEntry { name: String, payload: CustomTextPayload }

async fn api_presets_list(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.presets.lock().unwrap();
    let mut list: Vec<PresetEntry> = guard.iter()
        .map(|(k, v)| PresetEntry { name: k.clone(), payload: v.clone() })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    HttpResponse::Ok().json(list)
}

/// GET /api/presets/{name} — return a single preset's payload as JSON.
async fn api_presets_get(state: web::Data<DaemonState>, name: web::Path<String>) -> impl Responder {
    let payload = state.presets.lock().unwrap().get(name.as_str()).cloned();
    match payload {
        Some(p) => HttpResponse::Ok().json(p),
        None    => HttpResponse::NotFound().body(format!("preset '{}' not found", name)),
    }
}

#[derive(Deserialize)]
struct SavePresetRequest { name: String, payload: CustomTextPayload }

async fn api_presets_save(state: web::Data<DaemonState>, body: web::Json<SavePresetRequest>) -> impl Responder {
    let req = body.into_inner();
    if req.name.trim().is_empty() { return HttpResponse::BadRequest().body("name must not be empty"); }
    {
        let mut guard = state.presets.lock().unwrap();
        guard.insert(req.name.clone(), req.payload);
        save_presets_to_disk(&guard);
    }
    HttpResponse::Ok().body(format!("saved '{}'", req.name))
}

async fn api_presets_apply(state: web::Data<DaemonState>, name: web::Path<String>) -> impl Responder {
    let payload = state.presets.lock().unwrap().get(name.as_str()).cloned();
    match payload {
        Some(p) => {
            { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
            *state.pending_text.lock().unwrap() = Some(p);
            *state.mode.lock().unwrap()         = DisplayMode::CustomText;
            HttpResponse::Ok().body("ok")
        }
        None => HttpResponse::NotFound().body(format!("preset '{}' not found", name)),
    }
}

async fn api_presets_delete(state: web::Data<DaemonState>, name: web::Path<String>) -> impl Responder {
    let removed = {
        let mut guard = state.presets.lock().unwrap();
        let r = guard.remove(name.as_str()).is_some();
        if r { save_presets_to_disk(&guard); }
        r
    };
    if removed { HttpResponse::Ok().body("deleted") }
    else        { HttpResponse::NotFound().body(format!("'{}' not found", name)) }
}

// ─── Mode listing ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ModeInfo {
    name:        &'static str,
    description: &'static str,
}

async fn api_modes_list() -> impl Responder {
    HttpResponse::Ok().json(&[
        ModeInfo { name: "clock",         description: "Large digital clock with date and uptime" },
        ModeInfo { name: "clock_sysinfo", description: "Split-screen: clock above, quick stats below" },
        ModeInfo { name: "sysinfo",       description: "CPU, RAM, temperature, load average, uptime" },
        ModeInfo { name: "cpu_detail",    description: "Per-core CPU usage grid (up to 32 cores)" },
        ModeInfo { name: "network",       description: "Per-interface Rx/Tx rates and totals" },
        ModeInfo { name: "disk_usage",    description: "Disk partition usage bars" },
        ModeInfo { name: "process_top",   description: "Top processes sorted by CPU usage" },
        ModeInfo { name: "temperature",   description: "Thermal sensor readings with color-coded bars" },
        ModeInfo { name: "memory_detail", description: "Detailed RAM breakdown: used, cached, free, swap" },
        ModeInfo { name: "countdown",    description: "Countdown timer with colour transitions and progress bar" },
        ModeInfo { name: "cpu_graph",    description: "CPU usage history bar chart (last 2 minutes)" },
        ModeInfo { name: "idle",          description: "Keepalive — no display update, prevents blank" },
    ])
}

/// POST /api/mode/next — advance to the next live mode (wraps around).
async fn api_mode_next(state: web::Data<DaemonState>) -> impl Responder {
    cycle_mode(&state, 1)
}

/// POST /api/mode/prev — go to the previous live mode (wraps around).
async fn api_mode_prev(state: web::Data<DaemonState>) -> impl Responder {
    cycle_mode(&state, DisplayMode::ALL_NAMES.len() - 1)
}

fn cycle_mode(state: &DaemonState, step: usize) -> HttpResponse {
    let all: Vec<DisplayMode> = DisplayMode::ALL_NAMES.iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .collect();
    let cur = state.mode.lock().unwrap().clone();
    let cur_pos = all.iter().position(|m| m == &cur).unwrap_or(0);
    let next = all[(cur_pos + step) % all.len()].clone();
    *state.pending_text.lock().unwrap() = None;
    *state.pending_jpeg.lock().unwrap() = None;
    *state.prev_mode.lock().unwrap()    = next.clone();
    *state.mode.lock().unwrap()         = next.clone();
    HttpResponse::Ok().body(next.as_str())
}

// ─── Alert shorthand ─────────────────────────────────────────────────────────

/// POST /api/alert — send a severity-keyed notification without building a full
/// CustomTextPayload. Useful for CI pipelines and scripts.
///
/// Body (JSON):
///   { "level": "error"|"warning"|"info"|"success"|"critical",
///     "message": "human-readable line",
///     "detail":  "optional second line"  }
#[derive(Deserialize)]
struct AlertRequest {
    level:   AlertLevel,
    message: String,
    #[serde(default)]
    detail:  Option<String>,
    /// Optional progress bar value 0.0–1.0.
    #[serde(default)]
    progress: Option<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum AlertLevel { Error, Warning, Info, Success, Critical }

async fn api_alert(state: web::Data<DaemonState>, body: web::Json<AlertRequest>) -> impl Responder {
    use ttlcd::display::NamedColor;
    let req = body.into_inner();
    let (title, color): (&str, NamedColor) = match req.level {
        AlertLevel::Error    => ("ERROR",    NamedColor::Red),
        AlertLevel::Warning  => ("WARNING",  NamedColor::Yellow),
        AlertLevel::Info     => ("INFO",     NamedColor::Blue),
        AlertLevel::Success  => ("SUCCESS",  NamedColor::Green),
        AlertLevel::Critical => ("CRITICAL", NamedColor::Red),
    };
    let msg_summary = req.message.clone();
    let payload = CustomTextPayload {
        title:     Some(title.into()),
        subtitle:  Some(req.message),
        lines:     req.detail.into_iter().collect(),
        progress:  req.progress,
        color,
        show_time: true,
        ..Default::default()
    };
    push_history(&state, HistoryKind::Alert, format!("{} — {}", title, msg_summary));
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap()         = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

// ─── Screenshot download ──────────────────────────────────────────────────────

/// GET /api/screenshot — like /api/preview but forces a file download.
async fn api_screenshot(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .insert_header(("Content-Disposition", "attachment; filename=panel.jpg"))
            .content_type("image/jpeg")
            .body(jpeg.clone()),
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
    }
}

// ─── Countdown timer ─────────────────────────────────────────────────────────

/// POST /api/countdown — start a countdown timer on the panel.
///
/// Body (JSON):
///   { "seconds": 300 }                         ← count down from N seconds
///   { "until": "2026-05-13T21:00:00+00:00" }   ← count down to a UTC datetime
///   { "label": "Build cooldown" }              ← optional label
///
/// The label is optional.  Supply `seconds` OR `until`, not both.
#[derive(Deserialize)]
struct CountdownRequest {
    /// Total seconds to count down from.
    #[serde(default)]
    seconds: Option<u64>,
    /// ISO-8601 datetime string to count down to.
    #[serde(default)]
    until:   Option<String>,
    #[serde(default)]
    label:   Option<String>,
}

async fn api_countdown(state: web::Data<DaemonState>, body: web::Json<CountdownRequest>) -> impl Responder {
    let req = body.into_inner();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (target_unix, total_secs) = if let Some(secs) = req.seconds {
        if secs == 0 { return HttpResponse::BadRequest().body("seconds must be > 0"); }
        (now_secs + secs, secs)
    } else if let Some(ref until_str) = req.until {
        // Parse ISO-8601 with chrono
        match chrono::DateTime::parse_from_rfc3339(until_str) {
            Ok(dt) => {
                let target = dt.timestamp() as u64;
                if target <= now_secs { return HttpResponse::BadRequest().body("'until' is in the past"); }
                (target, target - now_secs)
            }
            Err(e) => return HttpResponse::BadRequest().body(format!("invalid datetime: {e}")),
        }
    } else {
        return HttpResponse::BadRequest().body("supply 'seconds' or 'until'");
    };

    let label = req.label.unwrap_or_default();
    *state.countdown.lock().unwrap() = Some(CountdownState { target_unix, total_secs, label: label.clone() });
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.mode.lock().unwrap() = DisplayMode::Countdown;
    push_history(&state, HistoryKind::Alert, format!("Countdown: {}s — {}", total_secs, label));
    HttpResponse::Ok().body(format!("countdown started: {}s", total_secs))
}

// ─── Health check ─────────────────────────────────────────────────────────────

/// GET /health — lightweight liveness probe.
/// Returns 200 with a small JSON body even in headless mode.
async fn api_health(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct Health {
        ok:         bool,
        version:    &'static str,
        mode:       String,
        headless:   bool,
        frame_seq:  u64,
    }
    HttpResponse::Ok().json(Health {
        ok:        true,
        version:   env!("CARGO_PKG_VERSION"),
        mode:      state.mode.lock().unwrap().as_str().to_string(),
        headless:  state.headless,
        frame_seq: *state.frame_seq.lock().unwrap(),
    })
}

// ─── Blank frame ──────────────────────────────────────────────────────────────

/// POST /api/blank — push a solid background-colour frame (effectively
/// "clears" the panel).  The panel stays on CustomImage mode until /api/clear
/// is called.
async fn api_blank(state: web::Data<DaemonState>) -> impl Responder {
    let quality = *state.quality.lock().unwrap();
    let rotate  = *state.rotate_deg.lock().unwrap();
    let blank_img = render_solid(30, 30, 46);
    let blank_img = apply_rotation(blank_img, rotate);
    let jpeg = match encode_for_panel(&blank_img, quality) {
        Ok(j)  => j,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;
    HttpResponse::Ok().body("blanked")
}

// ─── Text → JPEG preview renderer ───────────────────────────────────────────

/// POST /api/render — render a CustomTextPayload to JPEG and return it as
/// image/jpeg.  Does NOT affect panel or daemon state.  Useful for previewing
/// a notification layout in the web UI before sending it to the device.
async fn api_render(state: web::Data<DaemonState>, body: web::Json<CustomTextPayload>) -> impl Responder {
    let payload = body.into_inner();
    let img     = render_custom_text(&payload);
    let img     = apply_rotation(img, *state.rotate_deg.lock().unwrap());
    let quality = *state.quality.lock().unwrap();
    match encode_for_panel(&img, quality) {
        Ok(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .content_type("image/jpeg")
            .body(jpeg),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// ─── Server-side image load ───────────────────────────────────────────────────

/// POST /api/image/path — load an image from an absolute server-side path and
/// send it to the panel.  Avoids multipart upload overhead for files already on
/// the same machine.
///
/// Body (JSON):
///   { "path": "/absolute/path/to/image.jpg", "letterbox": true }
#[derive(Deserialize)]
struct ImagePathRequest {
    path: String,
    #[serde(default)]
    letterbox: bool,
}

async fn api_image_path(state: web::Data<DaemonState>, body: web::Json<ImagePathRequest>) -> impl Responder {
    let req  = body.into_inner();
    let path = PathBuf::from(&req.path);
    if !path.is_absolute() {
        return HttpResponse::BadRequest().body("path must be absolute");
    }
    if !path.exists() {
        return HttpResponse::NotFound().body(format!("'{}' not found", req.path));
    }
    let quality    = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();
    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = image::open(&path)?;
        let img = apply_rotation(img, rotate_deg);
        if req.letterbox { letterbox_for_panel(&img, quality) } else { encode_for_panel(&img, quality) }
    })() {
        Ok(j)  => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("image error: {e}")),
    };
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;
    HttpResponse::Ok().body("image queued")
}

// ─── Countdown timer state ───────────────────────────────────────────────────

/// Persistent state for the countdown display mode.
#[derive(Clone)]
struct CountdownState {
    /// Unix timestamp (seconds) when the countdown expires.
    target_unix:  u64,
    /// Original total duration in seconds (for progress bar).
    total_secs:   u64,
    /// Optional label shown below the clock.
    label:        String,
}

// ─── Notification history ────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum HistoryKind { Text, Alert }

#[derive(Clone, Serialize)]
struct HistoryEntry {
    kind:      HistoryKind,
    summary:   String,
    /// Unix timestamp (seconds).
    timestamp: u64,
}

/// Push an entry to the ring-buffer (capped at 50 items).
fn push_history(state: &DaemonState, kind: HistoryKind, summary: String) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = HistoryEntry { kind, summary, timestamp: ts };
    let mut h = state.history.lock().unwrap();
    if h.len() >= 50 { h.pop_front(); }
    h.push_back(entry);
}

/// GET /api/history?limit=N — return the last N notifications (default 50).
async fn api_history(state: web::Data<DaemonState>, query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let limit: usize = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50).min(50);
    let h = state.history.lock().unwrap();
    let items: Vec<&HistoryEntry> = h.iter().rev().take(limit).collect();
    HttpResponse::Ok().json(items)
}

/// DELETE /api/history — clear the notification ring-buffer.
async fn api_history_clear(state: web::Data<DaemonState>) -> impl Responder {
    let mut h = state.history.lock().unwrap();
    let count = h.len();
    h.clear();
    log::info!("notification history cleared ({} entries removed)", count);
    HttpResponse::Ok().body(format!("cleared {} notification(s)", count))
}

// ─── Runtime quality / orient controls ───────────────────────────────────────

#[derive(Deserialize)]
struct QualityRequest {
    /// New JPEG quality (1–100).
    quality: u8,
}

/// POST /api/quality — change JPEG quality at runtime (1–100).
async fn api_set_quality(state: web::Data<DaemonState>, body: web::Json<QualityRequest>) -> impl Responder {
    let q = body.quality.clamp(1, 100);
    *state.quality.lock().unwrap() = q;
    log::info!("quality set to {}", q);
    HttpResponse::Ok().body(format!("{}", q))
}

#[derive(Deserialize)]
struct OrientRequest {
    /// Rotation in degrees: 0, 90, 180, or 270.
    rotate: u16,
}

/// POST /api/orient — rotate all output at runtime (0/90/180/270).
async fn api_set_orient(state: web::Data<DaemonState>, body: web::Json<OrientRequest>) -> impl Responder {
    let deg = match body.rotate {
        0 | 90 | 180 | 270 => body.rotate,
        other => return HttpResponse::BadRequest().body(format!("invalid rotation {other}; use 0, 90, 180, or 270")),
    };
    *state.rotate_deg.lock().unwrap() = deg;
    log::info!("rotation set to {}°", deg);
    HttpResponse::Ok().body(format!("{}°", deg))
}

// ─── Prometheus metrics ───────────────────────────────────────────────────────

/// GET /metrics — expose key system and daemon metrics in Prometheus text format.
async fn api_metrics(state: web::Data<DaemonState>) -> impl Responder {
    use sysinfo::{Components, System};
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let load = System::load_average();
    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);
    let cpu_temp: Option<f32> = comps.iter()
        .filter(|c| { let l = c.label().to_lowercase(); l.contains("cpu") || l.contains("core") || l.contains("package") })
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);

    let mode    = state.mode.lock().unwrap().as_str().to_string();
    let quality = *state.quality.lock().unwrap();
    let rotate  = *state.rotate_deg.lock().unwrap();
    let history_len = state.history.lock().unwrap().len();

    let mut out = String::with_capacity(1024);
    let push = |s: &mut String, line: String| { s.push_str(&line); s.push('\n'); };

    push(&mut out, "# HELP ttlcd_cpu_usage_percent System CPU usage percentage".into());
    push(&mut out, "# TYPE ttlcd_cpu_usage_percent gauge".into());
    push(&mut out, format!("ttlcd_cpu_usage_percent {:.2}", sys.global_cpu_usage()));

    push(&mut out, "# HELP ttlcd_memory_used_bytes RAM used in bytes".into());
    push(&mut out, "# TYPE ttlcd_memory_used_bytes gauge".into());
    push(&mut out, format!("ttlcd_memory_used_bytes {}", sys.used_memory()));

    push(&mut out, "# HELP ttlcd_memory_total_bytes Total RAM in bytes".into());
    push(&mut out, "# TYPE ttlcd_memory_total_bytes gauge".into());
    push(&mut out, format!("ttlcd_memory_total_bytes {}", sys.total_memory()));

    push(&mut out, "# HELP ttlcd_load1 1-minute load average".into());
    push(&mut out, "# TYPE ttlcd_load1 gauge".into());
    push(&mut out, format!("ttlcd_load1 {:.4}", load.one));

    push(&mut out, "# HELP ttlcd_load5 5-minute load average".into());
    push(&mut out, "# TYPE ttlcd_load5 gauge".into());
    push(&mut out, format!("ttlcd_load5 {:.4}", load.five));

    push(&mut out, "# HELP ttlcd_load15 15-minute load average".into());
    push(&mut out, "# TYPE ttlcd_load15 gauge".into());
    push(&mut out, format!("ttlcd_load15 {:.4}", load.fifteen));

    if let Some(t) = cpu_temp {
        push(&mut out, "# HELP ttlcd_cpu_temperature_celsius CPU temperature".into());
        push(&mut out, "# TYPE ttlcd_cpu_temperature_celsius gauge".into());
        push(&mut out, format!("ttlcd_cpu_temperature_celsius {:.1}", t));
    }

    push(&mut out, "# HELP ttlcd_uptime_seconds System uptime in seconds".into());
    push(&mut out, "# TYPE ttlcd_uptime_seconds counter".into());
    push(&mut out, format!("ttlcd_uptime_seconds {}", System::uptime()));

    push(&mut out, "# HELP ttlcd_frame_seq Monotonic frame counter".into());
    push(&mut out, "# TYPE ttlcd_frame_seq counter".into());
    push(&mut out, format!("ttlcd_frame_seq {}", *state.frame_seq.lock().unwrap()));

    push(&mut out, "# HELP ttlcd_quality_percent Current JPEG quality setting".into());
    push(&mut out, "# TYPE ttlcd_quality_percent gauge".into());
    push(&mut out, format!("ttlcd_quality_percent {}", quality));

    push(&mut out, "# HELP ttlcd_rotate_deg Current output rotation (degrees)".into());
    push(&mut out, "# TYPE ttlcd_rotate_deg gauge".into());
    push(&mut out, format!("ttlcd_rotate_deg {}", rotate));

    push(&mut out, "# HELP ttlcd_display_mode Current display mode (label only)".into());
    push(&mut out, "# TYPE ttlcd_display_mode gauge".into());
    push(&mut out, format!("ttlcd_display_mode{{mode=\"{}\"}} 1", mode));

    push(&mut out, "# HELP ttlcd_history_total Total notification events pushed".into());
    push(&mut out, "# TYPE ttlcd_history_total counter".into());
    push(&mut out, format!("ttlcd_history_total {}", history_len));

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(out)
}

// ─── Clear all presets ────────────────────────────────────────────────────────

/// DELETE /api/presets — remove ALL saved presets (irreversible).
async fn api_presets_clear(state: web::Data<DaemonState>) -> impl Responder {
    let count = {
        let mut guard = state.presets.lock().unwrap();
        let n = guard.len();
        guard.clear();
        save_presets_to_disk(&guard);
        n
    };
    log::info!("all {} preset(s) cleared", count);
    HttpResponse::Ok().body(format!("cleared {} preset(s)", count))
}

// ─── Slideshow ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SlideshowRequest {
    dir:      String,
    #[serde(default = "default_ss_interval")]
    interval: u64,
}
fn default_ss_interval() -> u64 { 5 }

async fn api_slideshow(state: web::Data<DaemonState>, body: web::Json<SlideshowRequest>) -> impl Responder {
    let req = body.into_inner();
    let dir = PathBuf::from(&req.dir);
    if !dir.is_dir() { return HttpResponse::BadRequest().body(format!("'{}' is not a directory", req.dir)); }
    let paths = collect_image_paths(&dir);
    if paths.is_empty() { return HttpResponse::BadRequest().body(format!("no supported images in '{}'", req.dir)); }
    let count    = paths.len();
    let interval = req.interval.max(1);
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.slideshow_paths.lock().unwrap()    = paths;
    *state.slideshow_idx.lock().unwrap()      = 0;
    *state.slideshow_interval.lock().unwrap() = interval;
    *state.mode.lock().unwrap()               = DisplayMode::Slideshow;
    HttpResponse::Ok().body(format!("slideshow started: {} images, {}s/frame", count, interval))
}

// ─── Auto-rotation API ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RotationRequest {
    modes:    Vec<String>,
    /// Seconds between advances. 0 = stop rotation.
    interval: u64,
}

async fn api_rotation(state: web::Data<DaemonState>, body: web::Json<RotationRequest>) -> impl Responder {
    let req = body.into_inner();
    let modes: Vec<DisplayMode> = req.modes.iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .filter(|m| !m.is_transient())
        .collect();

    if req.interval > 0 && modes.is_empty() {
        return HttpResponse::BadRequest().body("no valid (non-transient) modes specified");
    }

    *state.rotation_modes.lock().unwrap()    = modes;
    *state.rotation_interval.lock().unwrap() = req.interval;

    if req.interval == 0 {
        HttpResponse::Ok().body("rotation stopped")
    } else {
        let count = state.rotation_modes.lock().unwrap().len();
        HttpResponse::Ok().body(format!("rotation started: {} modes, {}s interval", count, req.interval))
    }
}

// ─── Preview & sysinfo ────────────────────────────────────────────────────────

async fn api_preview_frame(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .content_type("image/jpeg")
            .body(jpeg.clone()),
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
    }
}

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
    cpu_temp:  Option<f32>,
}

async fn api_sysinfo_json() -> impl Responder {
    use sysinfo::{Components, System};
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let load = System::load_average();
    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);
    let cpu_temp: Option<f32> = comps.iter()
        .filter(|c| { let l = c.label().to_lowercase(); l.contains("cpu") || l.contains("core") || l.contains("package") })
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);
    HttpResponse::Ok().json(SysinfoJson {
        cpu_pct:   sys.global_cpu_usage(),
        mem_used:  sys.used_memory(),
        mem_total: sys.total_memory(),
        uptime:    System::uptime(),
        load1:     load.one,
        load5:     load.five,
        load15:    load.fifteen,
        hostname:  System::host_name().unwrap_or_default(),
        cpu_temp,
    })
}

// ─── Server-Sent Events stream ────────────────────────────────────────────────

/// GET /api/events — Server-Sent Events stream for real-time frame updates.
///
/// Sends `event: frame\ndata: {seq}\n\n` whenever a new panel frame is stored.
/// Sends a `: heartbeat` comment every 15 s to keep the TCP connection alive.
/// The browser can use this to trigger a fresh `/api/preview` fetch without
/// polling on a fixed timer.
async fn api_events(state: web::Data<DaemonState>) -> impl Responder {
    let frame_seq = Arc::clone(&state.frame_seq);

    let stream = futures_util::stream::unfold(
        (frame_seq, u64::MAX, std::time::Instant::now()),
        |(seq, last_seq, last_hb)| async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let cur = *seq.lock().unwrap();
                let now = std::time::Instant::now();

                if cur != last_seq {
                    let event = format!("event: frame\ndata: {cur}\n\n");
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (seq, cur, last_hb),
                    ));
                }

                if now.duration_since(last_hb).as_secs() >= 15 {
                    return Some((
                        Ok::<_, actix_web::Error>(
                            actix_web::web::Bytes::from_static(b": heartbeat\n\n")
                        ),
                        (seq, last_seq, now),
                    ));
                }
            }
        },
    );

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

// ─── /api/notify — simple single-call push ────────────────────────────────────

/// POST /api/notify — convenience endpoint for quick panel notifications.
///
/// Simpler than `/api/text`: accepts a flat JSON with optional fields.
/// Designed for one-liners in shell scripts and CI pipelines.
///
/// ```json
/// { "title": "Build OK", "body": "All 42 tests passed", "color": "green", "progress": 1.0 }
/// ```
#[derive(Deserialize)]
struct NotifyRequest {
    #[serde(default)]
    title:    Option<String>,
    /// Single-line body text (shown as `lines[0]`).
    #[serde(default)]
    body:     Option<String>,
    #[serde(default = "default_notify_color")]
    color:    ttlcd::display::NamedColor,
    #[serde(default)]
    progress: Option<f32>,
    #[serde(default = "default_true")]
    show_time: bool,
}
fn default_notify_color() -> ttlcd::display::NamedColor { ttlcd::display::NamedColor::Blue }
fn default_true() -> bool { true }

async fn api_notify(state: web::Data<DaemonState>, body: web::Json<NotifyRequest>) -> impl Responder {
    let req = body.into_inner();
    if req.title.is_none() && req.body.is_none() {
        return HttpResponse::BadRequest().body("supply at least 'title' or 'body'");
    }
    let summary = req.title.clone().unwrap_or_else(|| req.body.clone().unwrap_or_default());
    let payload = CustomTextPayload {
        title:    req.title,
        lines:    req.body.into_iter().collect(),
        progress: req.progress,
        color:    req.color,
        show_time: req.show_time,
        ..Default::default()
    };
    push_history(&state, HistoryKind::Text, summary);
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap()         = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

// ─── /api/webhook — generic CI/CD webhook ────────────────────────────────────

/// POST /api/webhook — accept JSON from CI/CD systems and render a panel alert.
///
/// Understands:
///   • GitHub Actions-style: `{ "action": "...", "workflow_run": { "conclusion": "success|failure", "name": "..." } }`
///   • Simple flat: `{ "event": "...", "status": "success|failure|...", "message": "..." }`
///   • Anything else: renders a generic "Webhook" notification.
///
/// Query parameter `?secret=TOKEN` is compared against the `TTLCD_WEBHOOK_SECRET`
/// environment variable when that variable is set.
async fn api_webhook(
    state: web::Data<DaemonState>,
    query: web::Query<std::collections::HashMap<String, String>>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    // Optional shared-secret check
    if let Ok(expected) = std::env::var("TTLCD_WEBHOOK_SECRET") {
        let provided = query.get("secret").map(String::as_str).unwrap_or("");
        if provided != expected {
            return HttpResponse::Unauthorized().body("invalid webhook secret");
        }
    }

    let v = body.into_inner();

    // ── GitHub Actions workflow_run ──────────────────────────────────────
    if let Some(wr) = v.get("workflow_run") {
        let conclusion = wr.get("conclusion").and_then(|c| c.as_str()).unwrap_or("unknown");
        let name       = wr.get("name").and_then(|n| n.as_str()).unwrap_or("Workflow");
        let (color, emoji) = match conclusion {
            "success"   => (ttlcd::display::NamedColor::Green,  "✓"),
            "failure"   => (ttlcd::display::NamedColor::Red,    "✗"),
            "cancelled" => (ttlcd::display::NamedColor::Yellow, "⊘"),
            _           => (ttlcd::display::NamedColor::Blue,   "·"),
        };
        let payload = CustomTextPayload {
            title:    Some(format!("{emoji} {}", name.chars().take(18).collect::<String>())),
            subtitle: Some(conclusion.to_uppercase()),
            progress: if conclusion == "success" { Some(1.0) } else if conclusion == "failure" { Some(0.0) } else { None },
            color,
            show_time: true,
            ..Default::default()
        };
        let summary = format!("GitHub: {} — {}", name, conclusion);
        push_history(&state, HistoryKind::Alert, summary);
        { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
        *state.pending_text.lock().unwrap() = Some(payload);
        *state.mode.lock().unwrap()         = DisplayMode::CustomText;
        return HttpResponse::Ok().body("ok");
    }

    // ── Simple flat { "status": "...", "message": "..." } ────────────────
    let status  = v.get("status").or_else(|| v.get("event"))
                   .and_then(|s| s.as_str()).unwrap_or("info");
    let message = v.get("message").or_else(|| v.get("text"))
                   .and_then(|m| m.as_str())
                   .unwrap_or("Webhook received");
    let (color, title) = match status.to_lowercase().as_str() {
        "success" | "passed" | "ok"    => (ttlcd::display::NamedColor::Green,  "✓ SUCCESS"),
        "failure" | "failed" | "error" => (ttlcd::display::NamedColor::Red,    "✗ FAILED"),
        "warning" | "warn"             => (ttlcd::display::NamedColor::Yellow, "⚠ WARNING"),
        "running" | "pending"          => (ttlcd::display::NamedColor::Blue,   "⟳ RUNNING"),
        _                              => (ttlcd::display::NamedColor::Teal,   "· WEBHOOK"),
    };
    let payload = CustomTextPayload {
        title:    Some(title.into()),
        subtitle: Some(message.chars().take(38).collect()),
        color,
        show_time: true,
        ..Default::default()
    };
    push_history(&state, HistoryKind::Alert, format!("{title} — {message}"));
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap()         = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

// ─── Runtime config snapshot ─────────────────────────────────────────────────

/// GET /api/config — return the current daemon runtime configuration as JSON.
/// Useful for debugging, dashboards, and config-as-code verification.
async fn api_get_config(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct RuntimeConfig {
        version:           &'static str,
        mode:              String,
        quality:           u8,
        rotate_deg:        u16,
        headless:          bool,
        rotation_modes:    Vec<String>,
        rotation_interval: u64,
        preset_count:      usize,
        history_count:     usize,
        slideshow_images:  usize,
        slideshow_interval: u64,
        frame_seq:         u64,
    }
    let rotation_modes: Vec<String> = state.rotation_modes.lock().unwrap()
        .iter().map(|m| m.as_str().to_string()).collect();
    HttpResponse::Ok().json(RuntimeConfig {
        version:           env!("CARGO_PKG_VERSION"),
        mode:              state.mode.lock().unwrap().as_str().to_string(),
        quality:           *state.quality.lock().unwrap(),
        rotate_deg:        *state.rotate_deg.lock().unwrap(),
        headless:          state.headless,
        rotation_modes,
        rotation_interval: *state.rotation_interval.lock().unwrap(),
        preset_count:      state.presets.lock().unwrap().len(),
        history_count:     state.history.lock().unwrap().len(),
        slideshow_images:  state.slideshow_paths.lock().unwrap().len(),
        slideshow_interval: *state.slideshow_interval.lock().unwrap(),
        frame_seq:         *state.frame_seq.lock().unwrap(),
    })
}

// ─── Scheduled notifications ─────────────────────────────────────────────────

/// POST /api/schedule — schedule a panel notification for a future time.
///
/// The daemon spawns a background task that sleeps until the requested time,
/// then pushes the notification exactly like `POST /api/text`.
///
/// Body (JSON):
/// ```json
/// { "at": "ISO8601 datetime",   // e.g. "2026-05-13T22:00:00Z"
///   "in_secs": 300,             // alternative: fire after this many seconds
///   "payload": { ... },         // CustomTextPayload fields
///   "label": "human label"      // optional; used in log output
/// }
/// ```
/// Supply exactly one of `at` or `in_secs`.
#[derive(Deserialize)]
struct ScheduleRequest {
    #[serde(default)]
    at:      Option<String>,
    #[serde(default)]
    in_secs: Option<u64>,
    payload: CustomTextPayload,
    #[serde(default)]
    label:   Option<String>,
}

async fn api_schedule(state: web::Data<DaemonState>, body: web::Json<ScheduleRequest>) -> impl Responder {
    let req = body.into_inner();

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);

    let delay_secs: u64 = if let Some(ref at_str) = req.at {
        match chrono::DateTime::parse_from_rfc3339(at_str) {
            Ok(dt) => {
                let target = dt.timestamp() as u64;
                if target <= now_secs {
                    return HttpResponse::BadRequest().body("'at' must be in the future");
                }
                target - now_secs
            }
            Err(e) => return HttpResponse::BadRequest().body(format!("invalid datetime: {e}")),
        }
    } else if let Some(s) = req.in_secs {
        if s == 0 { return HttpResponse::BadRequest().body("in_secs must be > 0"); }
        s
    } else {
        return HttpResponse::BadRequest().body("supply 'at' (ISO-8601) or 'in_secs'");
    };

    let label = req.label.unwrap_or_else(|| {
        req.payload.title.clone().unwrap_or_else(|| "(no title)".into())
    });
    let fire_at_str = if delay_secs >= 60 {
        format!("{:.0}m {:.0}s", delay_secs / 60, delay_secs % 60)
    } else {
        format!("{delay_secs}s")
    };

    // Increment pending counter
    *state.pending_scheduled.lock().unwrap() += 1;
    let counter = Arc::clone(&state.pending_scheduled);
    let payload_clone = req.payload;
    let state_clone   = state.get_ref().clone();
    let label_clone   = label.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;

        log::info!("scheduled notification fired: {}", label_clone);

        if let Some(dur) = payload_clone.duration_secs {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            *state_clone.auto_clear_at.lock().unwrap() = Some(now + dur);
        } else {
            *state_clone.auto_clear_at.lock().unwrap() = None;
        }

        let summary = payload_clone.title.clone()
            .unwrap_or_else(|| label_clone.clone());
        push_history(&state_clone, HistoryKind::Text, summary);
        {
            let cur = state_clone.mode.lock().unwrap().clone();
            if !cur.is_transient() { *state_clone.prev_mode.lock().unwrap() = cur; }
        }
        *state_clone.pending_text.lock().unwrap() = Some(payload_clone);
        *state_clone.mode.lock().unwrap()         = DisplayMode::CustomText;

        // Decrement pending counter
        let mut c = counter.lock().unwrap();
        *c = c.saturating_sub(1);
    });

    log::info!("scheduled notification in {}s: {}", delay_secs, label);
    HttpResponse::Ok().body(format!(
        "scheduled in {fire_at_str}: \"{label}\""
    ))
}

// ─── MJPEG live stream ────────────────────────────────────────────────────────

/// GET /api/stream — MJPEG live stream of the panel.
///
/// Standard Motion JPEG format: browsers and VLC can display it directly.
/// Stream a new JPEG whenever frame_seq changes, otherwise wait.
/// Sends at most ~4fps to limit CPU/bandwidth usage.
///
/// Usage in HTML: `<img src="/api/stream" style="width:480px">`
async fn api_mjpeg_stream(state: web::Data<DaemonState>) -> impl Responder {
    let frame_seq_lock = Arc::clone(&state.frame_seq);
    let last_frame_lock = Arc::clone(&state.last_frame);

    let stream = futures_util::stream::unfold(
        (frame_seq_lock, last_frame_lock, u64::MAX),
        |(seq_lock, frame_lock, last_seq)| async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let cur = *seq_lock.lock().unwrap();
                if cur != last_seq {
                    let maybe_jpeg = frame_lock.lock().unwrap().clone();
                    if let Some(jpeg) = maybe_jpeg {
                        let header = format!(
                            "--frame
Content-Type: image/jpeg
Content-Length: {}

",
                            jpeg.len()
                        );
                        let mut part = header.into_bytes();
                        part.extend_from_slice(&jpeg);
                        part.extend_from_slice(b"
");
                        return Some((
                            Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(part)),
                            (seq_lock, frame_lock, cur),
                        ));
                    }
                }
            }
        },
    );

    HttpResponse::Ok()
        .content_type("multipart/x-mixed-replace; boundary=frame")
        .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

// ─── Slideshow navigation ─────────────────────────────────────────────────────

/// POST /api/slideshow/next — advance the slideshow to the next image immediately.
async fn api_slideshow_next(state: web::Data<DaemonState>) -> impl Responder {
    if *state.mode.lock().unwrap() != DisplayMode::Slideshow {
        return HttpResponse::Conflict().body("not in slideshow mode");
    }
    let count = state.slideshow_paths.lock().unwrap().len();
    if count == 0 { return HttpResponse::BadRequest().body("no slideshow loaded"); }
    {
        let mut idx = state.slideshow_idx.lock().unwrap();
        *idx = (*idx + 1) % count;
    }
    HttpResponse::Ok().body(format!("advanced to image {}", *state.slideshow_idx.lock().unwrap()))
}

/// POST /api/slideshow/prev — go back to the previous slideshow image.
async fn api_slideshow_prev(state: web::Data<DaemonState>) -> impl Responder {
    if *state.mode.lock().unwrap() != DisplayMode::Slideshow {
        return HttpResponse::Conflict().body("not in slideshow mode");
    }
    let count = state.slideshow_paths.lock().unwrap().len();
    if count == 0 { return HttpResponse::BadRequest().body("no slideshow loaded"); }
    {
        let mut idx = state.slideshow_idx.lock().unwrap();
        *idx = idx.checked_sub(1).unwrap_or(count - 1);
    }
    HttpResponse::Ok().body(format!("went back to image {}", *state.slideshow_idx.lock().unwrap()))
}

// ─── Embedded web UI ─────────────────────────────────────────────────────────

async fn web_ui_index() -> HttpResponse {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../src/web/index.html"))
}
async fn web_ui_css() -> HttpResponse {
    HttpResponse::Ok().content_type("text/css").body(include_str!("../../src/web/app.css"))
}
async fn web_ui_js() -> HttpResponse {
    HttpResponse::Ok().content_type("application/javascript").body(include_str!("../../src/web/app.js"))
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
        *px = if on_diag || on_edge { image::Rgb([255, 255, 255]) } else { image::Rgb([r, g, b]) };
    }
    DynamicImage::ImageRgb8(img)
}
