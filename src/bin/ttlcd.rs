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
use reqwest;
use chrono::{Local, Timelike};
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
        render_net_graph,
        render_recent_alerts,
        render_process_memory,
        render_disk_io,
        render_calendar,
        render_big_text_banner,
        render_boot_splash,
        render_stopwatch,
        render_metrics,
        render_weather,
        render_notes,
        render_big_clock,
        render_qr_code,
        render_pomodoro,
        render_battery,
        render_world_clock,
        render_ticker,
        render_http_status,
        render_shell_cmd,
        render_ping_monitor,
        render_dashboard,
        render_cpu_freq,
        render_log_tail,
        render_hostname_ip,
        render_rss_feed,
        render_service_status,
        render_net_connections,
        render_uptime,
        render_os_info,
        render_docker_status,
        render_logged_in_users,
        render_load_graph,
        render_mem_graph,
        render_progress,
        render_matrix_rain,
        ProgressState,
        push_load_history,
        push_mem_history,
        push_temp_history,
        render_temp_graph,
        push_disk_io_history,
        render_disk_io_graph,
        render_graphs_overview,
        render_net_speed_graph,
        render_cpu_core_bars,
        render_analog_clock,
        render_ping_graph,
        push_ping_history,
        render_status_overview,
        render_game_of_life,
        render_led_clock,
        render_bounce_ball,
        render_speedometer_gauge,
        render_disk_pie,
        render_thermal_dashboard,
        render_plasma,
        render_fireworks,
        render_cpu_heatmap,
        push_cpu_core_history,
        read_docker_containers,
        read_user_sessions,
        HttpMonitorResult,
        PingResult,
        Metric,
        WeatherData,
        CustomTextPayload,
    },
    encode_for_panel, letterbox_for_panel, Panel, DEFAULT_JPEG_QUALITY, PANEL_HEIGHT, PANEL_WIDTH,
};
use ttlcd::display::{get_cpu_history, get_net_history};

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
        /// Bind address. Use 127.0.0.1 to restrict to localhost only.
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
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

    /// List all available display modes and their descriptions.
    Modes,

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

/// Darken all pixels by `factor` (1.0 = full brightness, 0.0 = black).
/// Returns the image unchanged when factor ≥ 0.99 to avoid a costly scan.
fn apply_brightness(img: DynamicImage, factor: f32) -> DynamicImage {
    if factor >= 0.99 { return img; }
    let factor = factor.clamp(0.0, 1.0);
    let mut rgb = img.into_rgb8();
    for px in rgb.pixels_mut() {
        px[0] = (px[0] as f32 * factor) as u8;
        px[1] = (px[1] as f32 * factor) as u8;
        px[2] = (px[2] as f32 * factor) as u8;
    }
    DynamicImage::ImageRgb8(rgb)
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
    bind:              Option<String>,
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

// ─── Persistent daemon state ────────────────────────────────────────────────

/// Path to ~/.config/ttlcd/daemon_state.json
fn daemon_state_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("daemon_state.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PersistedState {
    /// Last active non-transient display mode.
    mode:       Option<String>,
    /// JPEG quality.
    quality:    Option<u8>,
    /// Rotation degrees (0 / 90 / 180 / 270).
    rotate_deg: Option<u16>,
    /// Brightness factor 0.0–1.0.
    brightness: Option<f32>,
}

fn load_persisted_state() -> PersistedState {
    (|| -> Option<PersistedState> {
        let path = daemon_state_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

fn save_persisted_state(ps: &PersistedState) {
    let path = match daemon_state_path() {
        Some(p) => p,
        None    => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(ps) {
        Ok(text) => { let _ = std::fs::write(&path, &text); }
        Err(_)   => {}
    }
}

/// Snapshot the current daemon settings to disk (called on any settings change).
fn persist_state(state: &DaemonState) {
    let ps = PersistedState {
        mode:       Some(state.mode.lock().unwrap().as_str().to_string()),
        quality:    Some(*state.quality.lock().unwrap()),
        rotate_deg: Some(*state.rotate_deg.lock().unwrap()),
        brightness: Some(*state.dim_factor.lock().unwrap()),
    };
    save_persisted_state(&ps);
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let cfg = load_config();

    match cli.cmd {
        Cmd::Modes => {
            println!("{:<18}  {}", "NAME", "DESCRIPTION");
            println!("{}", "-".repeat(70));
            for info in &[
                ("clock",          "Large digital clock with date and uptime"),
                ("big_clock",      "Full-screen giant HH:MM with seconds and progress bar"),
                ("clock_sysinfo",  "Split: clock + quick stats"),
                ("sysinfo",        "CPU, RAM, temperature, load, uptime"),
                ("cpu_detail",     "Per-core CPU usage grid (up to 32 cores)"),
                ("network",        "Per-interface Rx/Tx rates and totals"),
                ("disk_usage",     "Disk partition usage bars"),
                ("process_top",    "Top processes by CPU usage"),
                ("temperature",    "Thermal sensor readings"),
                ("memory_detail",  "Detailed RAM/swap breakdown"),
                ("countdown",      "Countdown timer with colour transitions"),
                ("cpu_graph",      "CPU usage history bar chart (2 min)"),
                ("net_graph",      "Network I/O history chart (60 s)"),
                ("recent_alerts",  "Last 5 notifications as on-panel log"),
                ("process_memory", "Top processes ranked by RAM usage"),
                ("disk_io",        "Disk read/write throughput per device"),
                ("calendar",       "Full-month calendar with today highlighted"),
                ("idle",           "Keepalive — no display update"),
            ] {
                println!("{:<18}  {}", info.0, info.1);
            }
        }

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

        Cmd::Daemon { port, bind, mode, quality, headless, rotate,
                      rotation_modes, rotation_interval } => {
            let eff_port     = cfg.daemon.port.unwrap_or(port);
            let eff_bind     = cfg.daemon.bind.clone().unwrap_or(bind);
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
                eff_vid, eff_pid, eff_port, eff_bind, eff_mode, eff_quality,
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
#[allow(dead_code)] // variants constructed via from_str_name string matching
enum DisplayMode {
    Clock,
    BigClock,
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
    NetGraph,
    RecentAlerts,
    ProcessMemory,
    DiskIo,
    Calendar,
    Stopwatch,
    Metrics,
    Weather,
    Notes,
    QrCode,
    Pomodoro,
    Battery,
    WorldClock,
    Ticker,
    HttpStatus,
    ShellCmd,
    PingMonitor,
    Dashboard,
    LogTail,
    CpuFreq,
    HostnameIp,
    RssFeed,
    ServiceStatus,
    NetConnections,
    UptimeFancy,
    OsInfo,
    DockerStatus,
    LoggedInUsers,
    LoadGraph,
    MemGraph,
    Progress,
    MatrixRain,
    TempGraph,
    DiskIoGraph,
    GraphsOverview,
    AnalogClock,
    PingGraph,
    StatusOverview,
    GameOfLife,
    LedClock,
    BounceBall,
    NetSpeedGraph,
    CpuCoreBars,
    SpeedometerGauge,
    DiskPie,
    ThermalDashboard,
    Plasma,
    Fireworks,
    CpuHeatmap,
    Idle,
    CustomImage,
    CustomText,
    Slideshow,
}

impl DisplayMode {
    const ALL_NAMES: &'static [&'static str] = &[
        "clock", "big_clock", "clock_sysinfo", "sysinfo", "cpu_detail",
        "network", "disk_usage", "process_top", "temperature", "memory_detail", "countdown", "cpu_graph", "net_graph", "recent_alerts", "process_memory", "disk_io", "calendar", "stopwatch", "metrics", "weather", "notes", "qr_code", "pomodoro", "battery", "world_clock", "ticker", "http_status", "shell_cmd", "ping_monitor", "dashboard", "log_tail", "cpu_freq", "hostname_ip", "rss_feed", "service_status", "net_connections", "uptime_fancy", "os_info", "docker_status", "logged_in_users", "load_graph", "mem_graph", "progress", "matrix_rain", "temp_graph", "disk_io_graph", "graphs_overview", "analog_clock", "ping_graph", "status_overview", "game_of_life", "led_clock", "bounce_ball", "net_speed_graph", "cpu_core_bars", "speedometer_gauge", "disk_pie", "thermal_dashboard", "plasma", "fireworks", "cpu_heatmap", "idle",
    ];

    fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "clock"         => Some(DisplayMode::Clock),
            "big_clock"     => Some(DisplayMode::BigClock),
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
            "net_graph"    => Some(DisplayMode::NetGraph),
            "recent_alerts"   => Some(DisplayMode::RecentAlerts),
            "process_memory"  => Some(DisplayMode::ProcessMemory),
            "disk_io"         => Some(DisplayMode::DiskIo),
            "calendar"        => Some(DisplayMode::Calendar),
            "stopwatch"       => Some(DisplayMode::Stopwatch),
            "metrics"         => Some(DisplayMode::Metrics),
            "weather"         => Some(DisplayMode::Weather),
            "notes"           => Some(DisplayMode::Notes),
            "qr_code"         => Some(DisplayMode::QrCode),
            "pomodoro"        => Some(DisplayMode::Pomodoro),
            "battery"         => Some(DisplayMode::Battery),
            "world_clock"     => Some(DisplayMode::WorldClock),
            "ticker"          => Some(DisplayMode::Ticker),
            "http_status"     => Some(DisplayMode::HttpStatus),
            "shell_cmd"       => Some(DisplayMode::ShellCmd),
            "ping_monitor"    => Some(DisplayMode::PingMonitor),
            "dashboard"       => Some(DisplayMode::Dashboard),
            "log_tail"        => Some(DisplayMode::LogTail),
            "cpu_freq"        => Some(DisplayMode::CpuFreq),
            "hostname_ip"      => Some(DisplayMode::HostnameIp),
            "rss_feed"         => Some(DisplayMode::RssFeed),
            "service_status"   => Some(DisplayMode::ServiceStatus),
            "net_connections"  => Some(DisplayMode::NetConnections),
            "uptime_fancy"     => Some(DisplayMode::UptimeFancy),
            "os_info"          => Some(DisplayMode::OsInfo),
            "docker_status"    => Some(DisplayMode::DockerStatus),
            "logged_in_users"  => Some(DisplayMode::LoggedInUsers),
            "load_graph"       => Some(DisplayMode::LoadGraph),
            "mem_graph"        => Some(DisplayMode::MemGraph),
            "progress"         => Some(DisplayMode::Progress),
            "matrix_rain"      => Some(DisplayMode::MatrixRain),
            "analog_clock"      => Some(DisplayMode::AnalogClock),
            "ping_graph"        => Some(DisplayMode::PingGraph),
            "status_overview"   => Some(DisplayMode::StatusOverview),
            "game_of_life"      => Some(DisplayMode::GameOfLife),
            "led_clock"         => Some(DisplayMode::LedClock),
            "bounce_ball"       => Some(DisplayMode::BounceBall),
            "net_speed_graph"   => Some(DisplayMode::NetSpeedGraph),
            "cpu_core_bars"        => Some(DisplayMode::CpuCoreBars),
            "speedometer_gauge"   => Some(DisplayMode::SpeedometerGauge),
            "disk_pie"            => Some(DisplayMode::DiskPie),
            "thermal_dashboard"   => Some(DisplayMode::ThermalDashboard),
            "plasma"              => Some(DisplayMode::Plasma),
            "fireworks"           => Some(DisplayMode::Fireworks),
            "cpu_heatmap"         => Some(DisplayMode::CpuHeatmap),
            "idle"             => Some(DisplayMode::Idle),
            "custom_text"   => Some(DisplayMode::CustomText),
            "slideshow"     => Some(DisplayMode::Slideshow),
            _               => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            DisplayMode::Clock        => "clock",
            DisplayMode::BigClock     => "big_clock",
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
            DisplayMode::NetGraph    => "net_graph",
            DisplayMode::RecentAlerts  => "recent_alerts",
            DisplayMode::ProcessMemory => "process_memory",
            DisplayMode::DiskIo        => "disk_io",
            DisplayMode::Calendar      => "calendar",
            DisplayMode::Stopwatch     => "stopwatch",
            DisplayMode::Metrics      => "metrics",
            DisplayMode::Weather       => "weather",
            DisplayMode::Notes         => "notes",
            DisplayMode::QrCode        => "qr_code",
            DisplayMode::Pomodoro      => "pomodoro",
            DisplayMode::Battery       => "battery",
            DisplayMode::WorldClock    => "world_clock",
            DisplayMode::Ticker        => "ticker",
            DisplayMode::HttpStatus    => "http_status",
            DisplayMode::ShellCmd      => "shell_cmd",
            DisplayMode::PingMonitor   => "ping_monitor",
            DisplayMode::Dashboard     => "dashboard",
            DisplayMode::LogTail       => "log_tail",
            DisplayMode::CpuFreq       => "cpu_freq",
            DisplayMode::HostnameIp     => "hostname_ip",
            DisplayMode::RssFeed        => "rss_feed",
            DisplayMode::ServiceStatus  => "service_status",
            DisplayMode::NetConnections => "net_connections",
            DisplayMode::UptimeFancy    => "uptime_fancy",
            DisplayMode::OsInfo         => "os_info",
            DisplayMode::DockerStatus   => "docker_status",
            DisplayMode::LoggedInUsers  => "logged_in_users",
            DisplayMode::LoadGraph      => "load_graph",
            DisplayMode::MemGraph       => "mem_graph",
            DisplayMode::Progress       => "progress",
            DisplayMode::MatrixRain     => "matrix_rain",
            DisplayMode::TempGraph      => "temp_graph",
            DisplayMode::DiskIoGraph    => "disk_io_graph",
            DisplayMode::GraphsOverview => "graphs_overview",
    DisplayMode::AnalogClock    => "analog_clock",
    DisplayMode::PingGraph      => "ping_graph",
    DisplayMode::StatusOverview  => "status_overview",
    DisplayMode::GameOfLife      => "game_of_life",
    DisplayMode::LedClock        => "led_clock",
    DisplayMode::BounceBall      => "bounce_ball",
    DisplayMode::NetSpeedGraph  => "net_speed_graph",
    DisplayMode::CpuCoreBars        => "cpu_core_bars",
    DisplayMode::SpeedometerGauge => "speedometer_gauge",
    DisplayMode::DiskPie          => "disk_pie",
    DisplayMode::ThermalDashboard => "thermal_dashboard",
    DisplayMode::Plasma        => "plasma",
    DisplayMode::Fireworks     => "fireworks",
    DisplayMode::CpuHeatmap    => "cpu_heatmap",
            DisplayMode::Idle           => "idle",
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

// ─── Stopwatch state ─────────────────────────────────────────────────────────

/// Stopwatch state kept in DaemonState.
#[derive(Clone, Debug, serde::Serialize)]
struct StopwatchState {
    /// Whether the stopwatch is currently counting up.
    running: bool,
    /// Accumulated milliseconds that were counted before the last pause.
    elapsed_ms: u64,
    /// Unix millisecond timestamp when the stopwatch was last started
    /// (0 when stopped / never started).
    started_at_unix_ms: u64,
    /// Recorded lap times (accumulated ms at the moment each lap was tapped).
    laps: Vec<u64>,
}

impl StopwatchState {
    fn new() -> Self {
        Self { running: false, elapsed_ms: 0, started_at_unix_ms: 0, laps: Vec::new() }
    }

    /// Current total elapsed milliseconds (may be running).
    fn current_elapsed_ms(&self) -> u64 {
        if self.running && self.started_at_unix_ms > 0 {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.elapsed_ms + now_ms.saturating_sub(self.started_at_unix_ms)
        } else {
            self.elapsed_ms
        }
    }
}

// ─── Alert threshold state ────────────────────────────────────────────────────

/// Configurable thresholds for automatic panel alerts.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AlertThresholds {
    /// Fire an alert when global CPU usage exceeds this percentage (0 = disabled).
    cpu_pct:  f32,
    /// Fire an alert when any temperature component exceeds this Celsius (0 = disabled).
    temp_c:   f32,
    /// Minimum seconds between consecutive threshold alerts (rate-limit).
    cooldown_secs: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self { cpu_pct: 95.0, temp_c: 85.0, cooldown_secs: 120 }
    }
}

// ─── Night-dim schedule ───────────────────────────────────────────────────────

/// Auto-dim the panel between `start_hour` and `end_hour` (24-h, local time).
/// Outside the window, restores the user's configured brightness.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct DimSchedule {
    /// Hour of day (0–23) when dimming begins.
    start_hour: u8,
    /// Hour of day (0–23) when normal brightness is restored.
    end_hour:   u8,
    /// Brightness level to apply during the dim window (0.0–1.0).
    dim_level:  f32,
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
    /// Brightness multiplier 0.0–1.0 applied to every rendered frame (1.0 = full).
    dim_factor: Arc<Mutex<f32>>,
    /// Exponential-moving-average of frames rendered per second.
    render_fps: Arc<Mutex<f32>>,
    /// Timestamp of the last rendered frame (for FPS calc); None before first frame.
    render_fps_last: Arc<Mutex<Option<std::time::Instant>>>,
    /// Hardware panel info (serial, firmware) — None in headless mode.
    panel_info: Arc<Mutex<Option<ttlcd::DeviceInfo>>>,
    /// Unix timestamp (seconds) when this daemon instance started.
    daemon_start_secs: u64,
    /// Stopwatch state (start/stop/lap/reset).
    stopwatch: Arc<Mutex<StopwatchState>>,
    /// If true, the display thread skips rendering (panel shows the last frame).
    frozen: Arc<Mutex<bool>>,
    /// Thresholds for automatic CPU/temperature alerts.
    alert_thresholds: Arc<Mutex<AlertThresholds>>,
    /// Optional night-dim schedule.
    dim_schedule: Arc<Mutex<Option<DimSchedule>>>,
    /// Last Unix-second timestamp when a threshold alert fired (for rate-limiting).
    last_threshold_alert_at: Arc<Mutex<u64>>,
    /// Custom metrics for the Metrics display mode.
    metrics_title: Arc<Mutex<String>>,
    metrics_data:  Arc<Mutex<Vec<Metric>>>,
    /// Cached weather snapshot for the Weather display mode.
    weather_data:  Arc<Mutex<WeatherData>>,
    /// wttr.in location for auto-fetch (empty = disabled).
    weather_location:  Arc<Mutex<String>>,
    /// Auto-fetch interval in seconds (0 = disabled).
    weather_auto_secs: Arc<Mutex<u64>>,
    /// Notes/bulletin board title and list for the Notes display mode.
    notes_title: Arc<Mutex<String>>,
    notes_items: Arc<Mutex<Vec<String>>>,
    /// Ring-buffer of recent render FPS values (last 60 samples) for the FPS sparkline.
    fps_history: Arc<Mutex<VecDeque<f32>>>,
    /// Webhook URL override — if set, outbound webhook notifications are forwarded here.
    webhook_forward_url: Arc<Mutex<Option<String>>>,
    /// QR code mode: payload (text/URL) and optional title.
    qr_payload: Arc<Mutex<String>>,
    qr_title:   Arc<Mutex<String>>,
    /// Mode schedule: sorted list of (hour, minute, mode_name) entries.
    /// The scheduler thread checks every 30 s and switches mode when a time slot matches.
    mode_schedule: Arc<Mutex<Vec<ModeScheduleEntry>>>,
    /// Pomodoro timer state.
    pomodoro: Arc<Mutex<PomodoroState>>,
    /// Scrolling ticker text and color.
    ticker_text:  Arc<Mutex<String>>,
    ticker_color: Arc<Mutex<String>>,
    /// Current scroll offset in pixels (advances each render tick).
    ticker_pos:   Arc<Mutex<i32>>,
    /// World clock: list of (city_name, utc_offset_minutes) pairs.
    world_clock_cities: Arc<Mutex<Vec<(String, i32)>>>,
    /// Monotonically-increasing counter; incremented on every push_history(Alert) call.
    /// Used by the SSE stream to detect and push alert events to clients.
    alert_seq: Arc<Mutex<u64>>,
    /// Summary of the latest alert (seq, summary). None before first alert.
    last_alert_event: Arc<Mutex<Option<(u64, String)>>>,
    /// HTTP status monitor: list of configured monitors (label + URL).
    http_monitors: Arc<Mutex<Vec<HttpMonitorEntry>>>,
    /// Latest HTTP check results.
    http_results: Arc<Mutex<Vec<HttpMonitorResult>>>,
    /// Interval between HTTP health checks (seconds).
    http_check_interval_secs: Arc<Mutex<u64>>,
    /// Shell command for the ShellCmd display mode.
    shell_cmd_text: Arc<Mutex<String>>,
    /// Captured stdout of the last shell command run.
    shell_cmd_output: Arc<Mutex<String>>,
    /// True if the last shell command exited with code 0.
    shell_cmd_exit_ok: Arc<Mutex<bool>>,
    /// Unix-second timestamp of the last shell command run (0 = never).
    shell_cmd_run_at: Arc<Mutex<u64>>,
    /// Refresh interval for the shell command (seconds; 0 = manual only).
    shell_cmd_interval_secs: Arc<Mutex<u64>>,
    /// Ping monitor: list of configured hosts (label + host).
    ping_hosts:   Arc<Mutex<Vec<PingHostEntry>>>,
    /// Latest ping results.
    ping_results: Arc<Mutex<Vec<PingResult>>>,
    /// Interval between ping checks (seconds; minimum 5).
    ping_interval_secs: Arc<Mutex<u64>>,
    /// Optional HTTP JSON metrics source (auto-fetches and updates the metrics display).
    metrics_source: Arc<Mutex<Option<MetricsSource>>>,
    /// Path of the file being tailed.
    log_tail_path:     Arc<Mutex<String>>,
    /// Cached tail lines (most-recent last).
    log_tail_lines:    Arc<Mutex<Vec<String>>>,
    /// Number of tail lines to keep.
    log_tail_n:        Arc<Mutex<usize>>,
    /// Seconds between re-reads.
    log_tail_interval: Arc<Mutex<u64>>,
    /// Whether the last read succeeded.
    log_tail_ok:       Arc<Mutex<bool>>,
    /// RSS feed: title and items (title, meta).
    rss_feed_title:    Arc<Mutex<String>>,
    rss_feed_items:    Arc<Mutex<Vec<(String, String)>>>,
    /// Whether the last RSS fetch succeeded.
    rss_feed_ok:       Arc<Mutex<bool>>,
    /// RSS feed URL (empty = not configured).
    rss_feed_url:      Arc<Mutex<String>>,
    /// Seconds between RSS re-fetches (0 = manual only).
    rss_feed_interval: Arc<Mutex<u64>>,
    /// Systemd units to watch (service names).
    service_units:     Arc<Mutex<Vec<String>>>,
    /// Cached service status snapshots.
    service_statuses:  Arc<Mutex<Vec<ttlcd::display::ServiceStatus>>>,
    /// Seconds between service status polls (minimum 5).
    service_poll_interval: Arc<Mutex<u64>>,
    /// Optional API key for authentication. When set, all requests must include
    /// `Authorization: Bearer <key>` (except `/static/`, `/api/stream`, and health-check).
    api_key: Arc<Mutex<Option<String>>>,
    /// ntfy.sh topic URL for push notifications. When set, every Alert-kind history entry
    /// will be POSTed to this URL as a plain-text notification.
    /// Example: "https://ntfy.sh/my-ttlcd-topic"
    ntfy_url: Arc<Mutex<Option<String>>>,
    /// Progress bar display state for the Progress mode.
    progress_state: Arc<Mutex<ttlcd::display::ProgressState>>,
    /// Unix-second timestamp of the last non-static API activity.
    /// Updated by the activity middleware on every API request.
    last_activity_secs: Arc<Mutex<u64>>,
    /// If > 0, auto-switch to `screensaver_mode_name` after this many seconds of inactivity.
    screensaver_timeout_secs: Arc<Mutex<u64>>,
    /// Which display mode to use as screensaver (default: "matrix_rain").
    screensaver_mode_name: Arc<Mutex<String>>,
    /// True while the screensaver mode is currently active due to inactivity.
    screensaver_active: Arc<Mutex<bool>>,
    /// True while the USB panel is believed to be connected (False after send errors).
    panel_connected: Arc<Mutex<bool>>,
}

/// A single entry in the display-mode schedule.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ModeScheduleEntry {
    /// Hour of day (0–23).
    hour: u8,
    /// Minute of hour (0–59).
    minute: u8,
    /// Display mode name (must be a valid DisplayMode string).
    mode: String,
    /// Optional human-readable label.
    #[serde(default)]
    label: String,
}


// ─── HTTP monitor state ───────────────────────────────────────────────────────

/// A configured HTTP health-check endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct HttpMonitorEntry {
    label: String,
    url:   String,
}

/// A configured ping host.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PingHostEntry {
    label: String,
    host:  String,
}

/// A single field extracted from an HTTP JSON metrics source.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct MetricsSourceField {
    /// Label shown on the panel.
    label: String,
    /// Dot-notation JSON path (e.g. "cpu.usage_pct", "data.0.temp").
    path:  String,
    /// Optional display format. "{}" = raw string value.
    #[serde(default = "default_ms_format")]
    format: String,
    /// Optional metric color name.
    #[serde(default = "default_ms_color")]
    color:  String,
}
fn default_ms_format() -> String { "{}".to_string() }
fn default_ms_color()  -> String { "blue".to_string() }

/// A configured HTTP JSON metrics source for automatic metric polling.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct MetricsSource {
    /// URL to GET (must return JSON).
    url:          String,
    /// Panel title shown above the metrics grid.
    #[serde(default)]
    title:        String,
    /// Fields to extract and display.
    fields:       Vec<MetricsSourceField>,
    /// Auto-refresh interval in seconds (min 5; 0 = disabled).
    #[serde(default = "default_ms_interval")]
    interval_secs: u64,
}
fn default_ms_interval() -> u64 { 30 }

fn ping_hosts_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("ping_hosts.json"))
}

fn load_ping_hosts() -> Vec<PingHostEntry> {
    (|| -> Option<Vec<PingHostEntry>> {
        let path = ping_hosts_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })().unwrap_or_default()
}

fn save_ping_hosts(hosts: &[PingHostEntry]) {
    let path = match ping_hosts_path() {
        Some(p) => p,
        None    => return,
    };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(text) = serde_json::to_string_pretty(hosts) {
        let _ = std::fs::write(&path, text);
    }
}

fn http_monitors_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("http_monitors.json"))
}

fn load_http_monitors() -> Vec<HttpMonitorEntry> {
    (|| -> Option<Vec<HttpMonitorEntry>> {
        let path = http_monitors_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

fn save_http_monitors(monitors: &[HttpMonitorEntry]) {
    let path = match http_monitors_path() {
        Some(p) => p,
        None    => return,
    };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(text) = serde_json::to_string_pretty(monitors) {
        let _ = std::fs::write(&path, text);
    }
}
// ─── New settings persistence ────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct PersistedNewSettings {
    weather_location:  String,
    weather_auto_secs: u64,
    log_tail_path:     String,
    log_tail_n:        usize,
    log_tail_interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metrics_source:    Option<MetricsSource>,
    #[serde(default)]
    rss_feed_url:      String,
    #[serde(default)]
    rss_feed_interval: u64,
    #[serde(default)]
    service_units:     Vec<String>,
    #[serde(default)]
    service_poll_interval: u64,
    #[serde(default)]
    shell_cmd:          String,
    #[serde(default)]
    shell_cmd_interval: u64,
    #[serde(default)]
    http_monitors:      Vec<HttpMonitorEntry>,
    #[serde(default)]
    ping_hosts:         Vec<PingHostEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ntfy_url:           Option<String>,
    /// Screensaver timeout in seconds (0 = disabled).
    #[serde(default)]
    screensaver_timeout_secs: u64,
    /// Screensaver mode name (e.g. "matrix_rain").
    #[serde(default)]
    screensaver_mode_name: String,
    /// CPU alert threshold percent (0 = disabled).
    #[serde(default)]
    alert_cpu_pct: f32,
    /// Temperature alert threshold Celsius (0 = disabled).
    #[serde(default)]
    alert_temp_c: f32,
    /// Cooldown between threshold alerts (seconds).
    #[serde(default)]
    alert_cooldown_secs: u64,
}

fn new_settings_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("new_settings.json"))
}

fn load_new_settings() -> PersistedNewSettings {
    (|| -> Option<PersistedNewSettings> {
        let path = new_settings_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

fn save_new_settings(state: &DaemonState) {
    let Some(path) = new_settings_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let s = PersistedNewSettings {
        weather_location:  state.weather_location.lock().unwrap().clone(),
        weather_auto_secs: *state.weather_auto_secs.lock().unwrap(),
        log_tail_path:     state.log_tail_path.lock().unwrap().clone(),
        log_tail_n:        *state.log_tail_n.lock().unwrap(),
        log_tail_interval: *state.log_tail_interval.lock().unwrap(),
        metrics_source:    state.metrics_source.lock().unwrap().clone(),
        rss_feed_url:      state.rss_feed_url.lock().unwrap().clone(),
        rss_feed_interval: *state.rss_feed_interval.lock().unwrap(),
        service_units:     state.service_units.lock().unwrap().clone(),
        service_poll_interval: *state.service_poll_interval.lock().unwrap(),
        shell_cmd:          state.shell_cmd_text.lock().unwrap().clone(),
        shell_cmd_interval: *state.shell_cmd_interval_secs.lock().unwrap(),
        http_monitors:      state.http_monitors.lock().unwrap().clone(),
        ping_hosts:         state.ping_hosts.lock().unwrap().clone(),
        ntfy_url:           state.ntfy_url.lock().unwrap().clone(),
        screensaver_timeout_secs: *state.screensaver_timeout_secs.lock().unwrap(),
        screensaver_mode_name:    state.screensaver_mode_name.lock().unwrap().clone(),
        alert_cpu_pct:    state.alert_thresholds.lock().unwrap().cpu_pct,
        alert_temp_c:     state.alert_thresholds.lock().unwrap().temp_c,
        alert_cooldown_secs: state.alert_thresholds.lock().unwrap().cooldown_secs,
    };
    if let Ok(text) = serde_json::to_string_pretty(&s) {
        let _ = std::fs::write(&path, text);
    }
}



// ─── Pomodoro timer state ─────────────────────────────────────────────────────

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Which phase the Pomodoro timer is currently in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    fn as_str(&self) -> &'static str {
        match self {
            PomodoroPhase::Work       => "work",
            PomodoroPhase::ShortBreak => "short_break",
            PomodoroPhase::LongBreak  => "long_break",
        }
    }
}

/// Runtime state for the Pomodoro mode.
#[derive(Clone, Debug, Serialize)]
struct PomodoroState {
    phase:              PomodoroPhase,
    /// Unix-ms timestamp when the current phase run was last started (0 = paused).
    phase_started_unix_ms: u64,
    /// Accumulated elapsed ms for the current phase (excluding the running portion).
    phase_elapsed_ms:   u64,
    running:            bool,
    /// Total completed work sessions (ever).
    completed_sessions: u32,
    // ── Configurable durations ────
    work_secs:           u64,
    short_break_secs:    u64,
    long_break_secs:     u64,
    /// Number of work sessions between long breaks (default 4).
    long_break_interval: u32,
}

impl PomodoroState {
    fn new() -> Self {
        Self {
            phase:                 PomodoroPhase::Work,
            phase_started_unix_ms: 0,
            phase_elapsed_ms:      0,
            running:               false,
            completed_sessions:    0,
            work_secs:             25 * 60,
            short_break_secs:      5 * 60,
            long_break_secs:       15 * 60,
            long_break_interval:   4,
        }
    }

    fn phase_total_secs(&self) -> u64 {
        match self.phase {
            PomodoroPhase::Work       => self.work_secs,
            PomodoroPhase::ShortBreak => self.short_break_secs,
            PomodoroPhase::LongBreak  => self.long_break_secs,
        }
    }

    fn phase_elapsed_ms_now(&self) -> u64 {
        if self.running && self.phase_started_unix_ms > 0 {
            let now = now_unix_ms();
            self.phase_elapsed_ms + now.saturating_sub(self.phase_started_unix_ms)
        } else {
            self.phase_elapsed_ms
        }
    }

    fn remaining_secs(&self) -> i64 {
        let total_ms  = self.phase_total_secs() as i64 * 1000;
        let elapsed   = self.phase_elapsed_ms_now() as i64;
        // ceiling so we show "0:01" for the last partial second
        (total_ms - elapsed + 999) / 1000
    }

    /// Advance to the next phase and auto-start it.
    fn advance_phase(&mut self) {
        self.phase_elapsed_ms      = 0;
        self.phase_started_unix_ms = 0;

        match self.phase {
            PomodoroPhase::Work => {
                self.completed_sessions += 1;
                if self.completed_sessions % self.long_break_interval == 0 {
                    self.phase = PomodoroPhase::LongBreak;
                } else {
                    self.phase = PomodoroPhase::ShortBreak;
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                self.phase = PomodoroPhase::Work;
            }
        }

        // Auto-start the new phase
        self.running               = true;
        self.phase_started_unix_ms = now_unix_ms();
    }
}
#[tokio::main]
#[allow(clippy::too_many_arguments)]
async fn run_daemon(
    vid: u16, pid: u16, port: u16,
    bind: String,
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

    // Load persistent daemon state (last mode, quality, rotate, brightness).
    // CLI / config arguments already take precedence — only apply if those
    // fields still hold their defaults (i.e. the user did not override them).
    let persisted = load_persisted_state();
    let initial_mode = if let Some(ref saved_mode) = persisted.mode {
        // Only restore if the caller passed the compiled default "clock_sysinfo"
        // (i.e. the user did not explicitly choose a mode on the command line).
        if mode == "clock_sysinfo" {
            DisplayMode::from_str_name(saved_mode).unwrap_or(initial_mode)
        } else {
            initial_mode
        }
    } else {
        initial_mode
    };
    let quality     = persisted.quality.unwrap_or(quality);
    let rotate_deg  = persisted.rotate_deg.unwrap_or(rotate_deg);
    let init_brightness = persisted.brightness.unwrap_or(1.0);
    if persisted.mode.is_some() {
        log::info!("restored persisted state: mode={} quality={} rotate={}°",
            initial_mode.as_str(), quality, rotate_deg);
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
        dim_factor:        Arc::new(Mutex::new(init_brightness)),
        render_fps:        Arc::new(Mutex::new(0.0)),
        render_fps_last:   Arc::new(Mutex::new(None)),
        panel_info:        Arc::new(Mutex::new(None)),
        daemon_start_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        stopwatch:               Arc::new(Mutex::new(StopwatchState::new())),
        frozen:                  Arc::new(Mutex::new(false)),
        alert_thresholds:        Arc::new(Mutex::new(AlertThresholds::default())),
        dim_schedule:            Arc::new(Mutex::new(None)),
        last_threshold_alert_at: Arc::new(Mutex::new(0)),
        metrics_title: Arc::new(Mutex::new(String::new())),
        metrics_data:  Arc::new(Mutex::new(Vec::new())),
        weather_data:  Arc::new(Mutex::new(WeatherData::default())),
        weather_location:  Arc::new(Mutex::new(String::new())),
        weather_auto_secs: Arc::new(Mutex::new(0)),
        notes_title:  Arc::new(Mutex::new(String::new())),
        notes_items:  Arc::new(Mutex::new(Vec::new())),
        fps_history:  Arc::new(Mutex::new(VecDeque::with_capacity(60))),
        webhook_forward_url: Arc::new(Mutex::new(None)),
        qr_payload: Arc::new(Mutex::new(String::new())),
        qr_title:   Arc::new(Mutex::new("QR Code".to_string())),
        mode_schedule: Arc::new(Mutex::new(Vec::new())),
        pomodoro:           Arc::new(Mutex::new(PomodoroState::new())),
        ticker_text:        Arc::new(Mutex::new(String::new())),
        ticker_color:       Arc::new(Mutex::new("blue".to_string())),
        ticker_pos:         Arc::new(Mutex::new(0i32)),
        world_clock_cities:   Arc::new(Mutex::new(Vec::new())),
        alert_seq:            Arc::new(Mutex::new(0)),
        last_alert_event:     Arc::new(Mutex::new(None)),
        http_monitors:           Arc::new(Mutex::new(Vec::new())),
        http_results:            Arc::new(Mutex::new(Vec::new())),
        http_check_interval_secs: Arc::new(Mutex::new(30)),
        shell_cmd_text:          Arc::new(Mutex::new(String::new())),
        shell_cmd_output:        Arc::new(Mutex::new(String::new())),
        shell_cmd_exit_ok:       Arc::new(Mutex::new(true)),
        shell_cmd_run_at:        Arc::new(Mutex::new(0u64)),
        shell_cmd_interval_secs: Arc::new(Mutex::new(10)),
        ping_hosts:          Arc::new(Mutex::new(Vec::new())),
        ping_results:        Arc::new(Mutex::new(Vec::new())),
        ping_interval_secs:  Arc::new(Mutex::new(10)),
        metrics_source:      Arc::new(Mutex::new(None)),
        log_tail_path:     Arc::new(Mutex::new(String::new())),
        log_tail_lines:    Arc::new(Mutex::new(Vec::new())),
        log_tail_n:        Arc::new(Mutex::new(16)),
        log_tail_interval: Arc::new(Mutex::new(3)),
        log_tail_ok:       Arc::new(Mutex::new(false)),
        rss_feed_title:    Arc::new(Mutex::new(String::new())),
        rss_feed_items:    Arc::new(Mutex::new(Vec::new())),
        rss_feed_ok:       Arc::new(Mutex::new(false)),
        rss_feed_url:      Arc::new(Mutex::new(String::new())),
        rss_feed_interval: Arc::new(Mutex::new(300)),
        service_units:         Arc::new(Mutex::new(Vec::new())),
        service_statuses:      Arc::new(Mutex::new(Vec::new())),
        service_poll_interval: Arc::new(Mutex::new(10)),
        api_key:               Arc::new(Mutex::new(None)),
        ntfy_url:              Arc::new(Mutex::new(None)),
        progress_state:        Arc::new(Mutex::new(ttlcd::display::ProgressState::default())),
        last_activity_secs:    Arc::new(Mutex::new(0u64)),
        screensaver_timeout_secs: Arc::new(Mutex::new(0u64)),
        screensaver_mode_name:    Arc::new(Mutex::new("matrix_rain".to_string())),
        screensaver_active:       Arc::new(Mutex::new(false)),
        panel_connected:          Arc::new(Mutex::new(!headless)),
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
        // Cache hardware info in state so web handlers can read it
        *state.panel_info.lock().unwrap() = Some(p.info.clone());
        // Send boot splash as the very first frame
        let splash = render_boot_splash(env!("CARGO_PKG_VERSION"));
        if let Ok(jpeg) = encode_for_panel(&splash, quality) {
            let _ = p.send_jpeg(&jpeg);
        }
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

    // ── Alert threshold monitor thread ───────────────────────────────────
    {
        let state4 = state.clone();
        thread::spawn(move || threshold_monitor_loop(state4));
    }

    // ── Dim schedule thread ───────────────────────────────────────────────
    {
        let state5 = state.clone();
        thread::spawn(move || dim_schedule_loop(state5));
    }

    // ── Mode schedule thread ──────────────────────────────────────────────
    {
        // Load persisted schedule entries from disk before starting the loop
        *state.mode_schedule.lock().unwrap() = load_mode_schedule();
        let state6 = state.clone();
        thread::spawn(move || mode_schedule_loop(state6));
    }

    // ── World clock persistence ────────────────────────────────────────────
    {
        let loaded = load_world_clock();
        if !loaded.is_empty() {
            *state.world_clock_cities.lock().unwrap() = loaded;
        }
    }

    // ── HTTP monitors persistence ──────────────────────────────────────────
    {
        let monitors = load_http_monitors();
        if !monitors.is_empty() {
            *state.http_monitors.lock().unwrap() = monitors;
        }
    }

    // ── Ping hosts persistence ────────────────────────────────────────────
    {
        let hosts = load_ping_hosts();
        if !hosts.is_empty() {
            *state.ping_hosts.lock().unwrap() = hosts;
        }
    }

    // ── Load new settings (weather, log_tail, metrics_source) ────────────
    {
        let ns = load_new_settings();
        if !ns.weather_location.is_empty() {
            *state.weather_location.lock().unwrap()  = ns.weather_location;
            *state.weather_auto_secs.lock().unwrap() = ns.weather_auto_secs;
        }
        if !ns.log_tail_path.is_empty() {
            *state.log_tail_path.lock().unwrap()     = ns.log_tail_path;
            *state.log_tail_n.lock().unwrap()        = ns.log_tail_n.max(1);
            *state.log_tail_interval.lock().unwrap() = ns.log_tail_interval.max(1);
        }
        if let Some(src) = ns.metrics_source {
            *state.metrics_source.lock().unwrap() = Some(src);
        }
        if !ns.rss_feed_url.is_empty() {
            *state.rss_feed_url.lock().unwrap()      = ns.rss_feed_url;
            *state.rss_feed_interval.lock().unwrap() = ns.rss_feed_interval.max(30);
        }
        if !ns.service_units.is_empty() {
            *state.service_units.lock().unwrap()         = ns.service_units;
            *state.service_poll_interval.lock().unwrap() = ns.service_poll_interval.max(5);
        }
        if !ns.shell_cmd.is_empty() {
            *state.shell_cmd_text.lock().unwrap()          = ns.shell_cmd;
            *state.shell_cmd_interval_secs.lock().unwrap() = ns.shell_cmd_interval.max(1);
        }
        if !ns.http_monitors.is_empty() {
            *state.http_monitors.lock().unwrap() = ns.http_monitors;
        }
        if !ns.ping_hosts.is_empty() {
            *state.ping_hosts.lock().unwrap() = ns.ping_hosts;
        }
        if let Some(ntfy) = ns.ntfy_url {
            *state.ntfy_url.lock().unwrap() = Some(ntfy);
        }
        if ns.screensaver_timeout_secs > 0 {
            *state.screensaver_timeout_secs.lock().unwrap() = ns.screensaver_timeout_secs;
        }
        if !ns.screensaver_mode_name.is_empty() {
            *state.screensaver_mode_name.lock().unwrap() = ns.screensaver_mode_name;
        }
        if ns.alert_cpu_pct > 0.0 || ns.alert_temp_c > 0.0 {
            let mut t = state.alert_thresholds.lock().unwrap();
            if ns.alert_cpu_pct  >= 0.0 { t.cpu_pct  = ns.alert_cpu_pct; }
            if ns.alert_temp_c   >= 0.0 { t.temp_c   = ns.alert_temp_c; }
            if ns.alert_cooldown_secs > 0 { t.cooldown_secs = ns.alert_cooldown_secs; }
        }
    }

    // ── HTTP health-check background thread ───────────────────────────────
    {
        let state_hc = state.clone();
        tokio::spawn(async move { http_check_loop(state_hc).await });
    }

    // ── Shell-command background thread ───────────────────────────────────
    {
        let state_sc = state.clone();
        tokio::spawn(async move { shell_cmd_loop(state_sc).await });
    }

    // ── Ping monitor background thread ────────────────────────────────────
    {
        let state_pm = state.clone();
        tokio::spawn(async move { ping_monitor_loop(state_pm).await });
    }

    // ── Metrics source auto-fetch background thread ─────────────────────
    {
        let state_ms = state.clone();
        tokio::spawn(async move { metrics_source_loop(state_ms).await });
    }

    // ── Weather auto-fetch background thread ─────────────────────────────
    {
        let state_wf = state.clone();
        tokio::spawn(async move { weather_auto_loop(state_wf).await });
    }

    // ── Log tail background thread ────────────────────────────────────────
    {
        let state_lt = state.clone();
        tokio::spawn(async move { log_tail_loop(state_lt).await });
    }

    // ── RSS feed background thread ────────────────────────────────────────
    {
        let state_rss = state.clone();
        tokio::spawn(async move { rss_feed_loop(state_rss).await });
    }

    // ── Service status background thread ─────────────────────────────────
    {
        let state_svc = state.clone();
        tokio::spawn(async move { service_status_loop(state_svc).await });
    }

    // ── Screensaver idle-detection thread ────────────────────────────────
    {
        // Seed last_activity_secs so screensaver doesn't fire immediately on startup
        *state.last_activity_secs.lock().unwrap() = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let state_ss = state.clone();
        thread::spawn(move || screensaver_loop(state_ss));
    }

    // ── HTTP server ────────────────────────────────────────────────────────
    let web_state = web::Data::new(state.clone());

    let server = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::from_fn(auth_middleware))
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
            .route("/api/image/url",      web::post().to(api_image_url))
            .route("/api/render",         web::post().to(api_render))
            .route("/api/sysinfo",        web::get().to(api_sysinfo_json))
            .route("/api/processes",      web::get().to(api_processes_json))
            .route("/api/disks",          web::get().to(api_disks_json))
            .route("/api/preview",        web::get().to(api_preview_frame))
            .route("/api/preview.png",    web::get().to(api_preview_frame_png))
            .route("/api/render/{mode}",  web::get().to(api_render_mode_preview))
            .route("/api/text",           web::post().to(api_text))
            .route("/api/clear",          web::post().to(api_clear))
            // Presets API (full CRUD)
            .route("/api/presets",          web::get().to(api_presets_list))
            .route("/api/presets",          web::post().to(api_presets_save))
            .route("/api/presets/{name}",   web::get().to(api_presets_get))
            .route("/api/presets/{name}",   web::post().to(api_presets_apply))
            .route("/api/presets/{name}",   web::delete().to(api_presets_delete))
            .route("/api/presets/export",   web::get().to(api_presets_export))
            .route("/api/presets/import",   web::post().to(api_presets_import))
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
            .route("/ping",               web::get().to(api_ping))
            .route("/api/ping",           web::get().to(api_ping))
            .route("/api/blank",          web::post().to(api_blank))
            .route("/api/display/reset",  web::post().to(api_display_reset))
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
            .route("/api/info",           web::get().to(api_info))
            // Server-Sent Events
            .route("/api/events",         web::get().to(api_events))
            // MJPEG live stream
            .route("/api/stream",         web::get().to(api_mjpeg_stream))
            // Test pattern
            .route("/api/test",           web::post().to(api_test_pattern))
            // Slideshow extra controls
            .route("/api/slideshow/images",  web::get().to(api_slideshow_images))
            .route("/api/slideshow/shuffle", web::post().to(api_slideshow_shuffle))
            // Brightness control
            .route("/api/brightness",     web::get().to(api_get_brightness))
            .route("/api/brightness",     web::post().to(api_set_brightness))
            // Countdown cancel
            .route("/api/countdown",      web::delete().to(api_cancel_countdown))
            // Network + temperature JSON
            .route("/api/network",        web::get().to(api_network_json))
            .route("/api/temperatures",   web::get().to(api_temperatures_json))
            // History ring-buffer snapshots
            .route("/api/cpu/history",    web::get().to(api_cpu_history))
            .route("/api/net/history",    web::get().to(api_net_history))
            // PNG screenshot
            .route("/api/screenshot.png", web::get().to(api_screenshot_png))
            // Rotation GET
            .route("/api/rotation",       web::get().to(api_rotation_get))
            // SVG status badge
            .route("/api/status.svg",     web::get().to(api_status_badge))
            // Version and uptime
            .route("/api/version",        web::get().to(api_version))
            .route("/api/uptime",         web::get().to(api_uptime))
            // Panel hardware info
            .route("/api/panel/info",       web::get().to(api_panel_info))
            .route("/api/panel/reconnect", web::post().to(api_panel_reconnect))
            // Large text banner (POST to display, GET to fetch last banner)
            .route("/api/banner",         web::post().to(api_banner))
            // Modes with descriptions
            .route("/api/modes/info",     web::get().to(api_modes_info))
            // Reload persisted daemon state from disk
            .route("/api/state/reload",   web::post().to(api_state_reload))
            // Stopwatch
            .route("/api/stopwatch",       web::get().to(api_stopwatch_get))
            .route("/api/stopwatch/start", web::post().to(api_stopwatch_start))
            .route("/api/stopwatch/stop",  web::post().to(api_stopwatch_stop))
            .route("/api/stopwatch/lap",   web::post().to(api_stopwatch_lap))
            .route("/api/stopwatch/reset", web::post().to(api_stopwatch_reset))
            // Display freeze / unfreeze
            .route("/api/display/freeze",   web::post().to(api_display_freeze))
            .route("/api/display/unfreeze", web::post().to(api_display_unfreeze))
            .route("/api/display/frozen",   web::get().to(api_display_frozen))
            // Alert thresholds
            .route("/api/alerts/thresholds", web::get().to(api_get_thresholds))
            .route("/api/alerts/thresholds", web::post().to(api_set_thresholds))
            // Night-dim schedule
            .route("/api/dim/schedule",   web::get().to(api_get_dim_schedule))
            .route("/api/dim/schedule",   web::post().to(api_set_dim_schedule))
            .route("/api/dim/schedule",   web::delete().to(api_clear_dim_schedule))
            // Custom metrics dashboard
            .route("/api/metrics",        web::get().to(api_get_metrics))
            .route("/api/metrics",        web::post().to(api_push_metrics))
            .route("/api/metrics",        web::delete().to(api_clear_metrics))
            // Display profiles
            .route("/api/profiles",             web::get().to(api_profiles_list))
            .route("/api/profiles",             web::post().to(api_profiles_save))
            .route("/api/profiles/{name}/apply", web::post().to(api_profiles_apply))
            .route("/api/profiles/{name}",       web::delete().to(api_profiles_delete))
            // Comprehensive system snapshot
            .route("/api/sys/snapshot",          web::get().to(api_sys_snapshot))
            .route("/api/sys/os",                web::get().to(api_sys_os))
            .route("/api/sys/net",               web::get().to(api_sys_net))
            // Weather display
            .route("/api/weather",        web::get().to(api_get_weather))
            .route("/api/weather",        web::post().to(api_update_weather))
            .route("/api/weather",        web::delete().to(api_clear_weather))
            .route("/api/weather/fetch",  web::post().to(api_weather_fetch_now))
            .route("/api/weather/auto",   web::put().to(api_weather_auto))
            .route("/api/weather/auto",   web::delete().to(api_weather_auto_stop))
            // Notes / bulletin board
            .route("/api/notes",          web::get().to(api_get_notes))
            .route("/api/notes",          web::post().to(api_push_notes))
            .route("/api/notes",          web::delete().to(api_clear_notes))
            // QR Code mode
            .route("/api/qr",             web::get().to(api_get_qr))
            .route("/api/qr",             web::post().to(api_set_qr))
            .route("/api/qr",             web::delete().to(api_clear_qr))
            // Mode schedule
            .route("/api/schedule/mode",        web::get().to(api_get_mode_schedule))
            .route("/api/schedule/mode",        web::post().to(api_add_mode_schedule))
            .route("/api/schedule/mode",        web::delete().to(api_clear_mode_schedule))
            .route("/api/schedule/mode/{index}", web::delete().to(api_remove_mode_schedule_entry))
            // Settings export/import
            .route("/api/settings/export", web::get().to(api_settings_export))
            .route("/api/settings/import", web::post().to(api_settings_import))
            // FPS history sparkline
            .route("/api/fps/history",    web::get().to(api_fps_history))
            // Webhook config (GET current URL, POST to set, DELETE to clear)
            .route("/api/webhook/config", web::get().to(api_get_webhook_config))
            .route("/api/webhook/config", web::post().to(api_set_webhook_config))
            .route("/api/webhook/config", web::delete().to(api_clear_webhook_config))
            // Pomodoro timer
            .route("/api/pomodoro",         web::get().to(api_get_pomodoro))
            .route("/api/pomodoro/start",   web::post().to(api_pomodoro_start))
            .route("/api/pomodoro/pause",   web::post().to(api_pomodoro_pause))
            .route("/api/pomodoro/skip",    web::post().to(api_pomodoro_skip))
            .route("/api/pomodoro/reset",   web::post().to(api_pomodoro_reset))
            .route("/api/pomodoro/stop",    web::post().to(api_pomodoro_stop))
            .route("/api/pomodoro/config",  web::post().to(api_pomodoro_config))
            // Scrolling ticker
            .route("/api/ticker",         web::get().to(api_get_ticker))
            .route("/api/ticker",         web::post().to(api_set_ticker))
            .route("/api/ticker",         web::delete().to(api_clear_ticker))
            // World clock
            .route("/api/world_clock",  web::get().to(api_get_world_clock))
            .route("/api/world_clock",  web::post().to(api_set_world_clock))
            .route("/api/world_clock",  web::delete().to(api_clear_world_clock))
            // HTTP JSON metrics source
            .route("/api/metrics/source",         web::get().to(api_get_metrics_source))
            .route("/api/metrics/source",         web::post().to(api_set_metrics_source))
            .route("/api/metrics/source",         web::delete().to(api_clear_metrics_source))
            .route("/api/metrics/source/fetch",   web::post().to(api_fetch_metrics_source_now))
            // Battery info
            .route("/api/battery",        web::get().to(api_battery_json))
            // HTTP status monitor
            .route("/api/http_status",              web::get().to(api_get_http_status))
            .route("/api/http_status",              web::post().to(api_add_http_monitor))
            .route("/api/http_status/{index}",      web::delete().to(api_delete_http_monitor))
            .route("/api/http_status/interval",     web::put().to(api_set_http_interval))
            // Shell command
            .route("/api/shell_cmd",                web::get().to(api_get_shell_cmd))
            .route("/api/shell_cmd",                web::post().to(api_set_shell_cmd))
            .route("/api/shell_cmd",                web::delete().to(api_clear_shell_cmd))
            .route("/api/shell_cmd/run",            web::post().to(api_run_shell_cmd_now))
            // Ping monitor
            .route("/api/ping_monitor",             web::get().to(api_get_ping))
            .route("/api/ping_monitor",             web::post().to(api_add_ping_host))
            .route("/api/ping_monitor/{index}",     web::delete().to(api_delete_ping_host))
            .route("/api/ping_monitor/interval",    web::put().to(api_set_ping_interval))
            // Log tail viewer
            .route("/api/log_tail",                 web::get().to(api_get_log_tail))
            .route("/api/log_tail",                 web::post().to(api_set_log_tail))
            .route("/api/log_tail",                 web::delete().to(api_clear_log_tail))
            // RSS feed
            .route("/api/rss",                      web::get().to(api_get_rss))
            .route("/api/rss",                      web::post().to(api_set_rss))
            .route("/api/rss",                      web::delete().to(api_clear_rss))
            .route("/api/rss/fetch",                web::post().to(api_fetch_rss))
            // Service status monitor
            .route("/api/services",                 web::get().to(api_get_services))
            .route("/api/services",                 web::post().to(api_set_services))
            .route("/api/services",                 web::delete().to(api_clear_services))
            // Docker containers
            .route("/api/docker",                   web::get().to(api_get_docker))
            // Logged-in user sessions
            .route("/api/sessions",                 web::get().to(api_get_sessions))
            // API key authentication management
            .route("/api/auth/key",                 web::post().to(api_set_auth_key))
            .route("/api/auth/key",                 web::delete().to(api_clear_auth_key))
            .route("/api/auth/status",              web::get().to(api_get_auth_status))
            // Progress bar display
            .route("/api/progress",                 web::get().to(api_get_progress))
            .route("/api/progress",                 web::post().to(api_set_progress))
            .route("/api/progress",                 web::delete().to(api_clear_progress))
            // ntfy.sh push notification config
            .route("/api/ntfy/config",              web::get().to(api_get_ntfy_config))
            .route("/api/ntfy/config",              web::post().to(api_set_ntfy_config))
            .route("/api/ntfy/config",              web::delete().to(api_clear_ntfy_config))
            .route("/api/ntfy/test",                web::post().to(api_ntfy_test))
            // Batch API
            .route("/api/batch",             web::post().to(api_batch))
            // Screensaver
            .route("/api/screensaver",       web::get().to(api_get_screensaver))
            .route("/api/screensaver",       web::post().to(api_set_screensaver))
            .route("/api/screensaver",       web::delete().to(api_clear_screensaver))
            .route("/api/screensaver/wake",  web::post().to(api_screensaver_wake))
            // API docs
            .route("/api/docs",           web::get().to(api_docs))
            // Web UI
            .route("/",                   web::get().to(web_ui_index))
            .route("/static/app.css",     web::get().to(web_ui_css))
            .route("/static/app.js",      web::get().to(web_ui_js))
    })
    .bind((bind.as_str(), port))?
    .run();

    let handle = server.handle();
    // ── Graceful shutdown handler ─────────────────────────────────────────────
    // On SIGTERM or Ctrl-C: blank the LCD panel, then stop the HTTP server.
    {
        let panel_sd = Arc::clone(&panel);
        let quality_sd = *state.quality.lock().unwrap();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            log::info!("ttlcd: shutdown signal — blanking panel");
            {
                let mut guard = panel_sd.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    // Render a clean "shutdown" frame
                    let goodbye = render_boot_splash(&format!("v{} — shutting down", env!("CARGO_PKG_VERSION")));
                    if let Ok(jpeg) = encode_for_panel(&goodbye, quality_sd) {
                        let _ = p.send_jpeg(&jpeg);
                        // Brief pause so the user sees the goodbye frame
                        std::thread::sleep(std::time::Duration::from_millis(800));
                    }
                    // Then blank the panel (all-black)
                    let blank_img = image::DynamicImage::ImageRgb8(image::RgbImage::new(480, 480));
                    if let Ok(jpeg) = encode_for_panel(&blank_img, 50) {
                        let _ = p.send_jpeg(&jpeg);
                    }
                }
            }
            handle.stop(true).await;
        });
    }

    server.await.context("HTTP server error")?;

    Ok(())
}

// ─── Store a new frame ────────────────────────────────────────────────────────

fn store_frame(state: &DaemonState, jpeg: Vec<u8>) -> Vec<u8> {
    *state.last_frame.lock().unwrap() = Some(jpeg.clone());
    *state.frame_seq.lock().unwrap() += 1;
    jpeg
}

/// Update the EMA render-FPS counter.
/// Uses an `Arc<Mutex<Option<Instant>>>` stored in `DaemonState` so there are
/// no thread-local initialization quirks.
fn update_fps(state: &DaemonState) {
    let now = std::time::Instant::now();
    let mut last_opt = state.render_fps_last.lock().unwrap();
    if let Some(last) = *last_opt {
        let elapsed = now.duration_since(last).as_secs_f32();
        // Sanity bounds: only accept 0.01 s – 60 s intervals
        if elapsed >= 0.01 && elapsed <= 60.0 {
            let new_fps = 1.0 / elapsed;
            let mut fps = state.render_fps.lock().unwrap();
            *fps = if *fps == 0.0 { new_fps } else { 0.2 * new_fps + 0.8 * (*fps) };
            // Append to FPS history ring-buffer (capped at 60 samples)
            let mut fh = state.fps_history.lock().unwrap();
            if fh.len() >= 60 { fh.pop_front(); }
            fh.push_back(*fps);
        }
    }
    *last_opt = Some(now);
}

// ─── Display loop ─────────────────────────────────────────────────────────────

fn display_loop(panel: Arc<Mutex<Option<Panel>>>, state: DaemonState) {
    let mut consecutive_errors: u32 = 0;
    loop {
        let mode       = state.mode.lock().unwrap().clone();
        let quality    = *state.quality.lock().unwrap();
        let rotate_deg = *state.rotate_deg.lock().unwrap();

        let result: anyhow::Result<()> = (|| {
            // ── Freeze: skip rendering, just keepalive ─────────────────────
            if *state.frozen.lock().unwrap() {
                let mut g = panel.lock().unwrap();
                if let Some(p) = g.as_mut() { p.keepalive()?; }
                thread::sleep(Duration::from_secs(2));
                return Ok(());
            }

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
                        let img  = apply_brightness(img, *state.dim_factor.lock().unwrap());
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
                    let img  = apply_brightness(img, *state.dim_factor.lock().unwrap());
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
                DisplayMode::BigClock     => (render_big_clock(),     1),
                DisplayMode::ClockSysinfo => (render_clock_sysinfo(), 2),
                DisplayMode::Sysinfo      => (render_sysinfo(),        2),
                DisplayMode::CpuDetail    => (render_cpu_detail(),     2),
                DisplayMode::Network      => (render_network(),        2),
                DisplayMode::DiskUsage    => (render_disk_usage(),     8),
                DisplayMode::ProcessTop   => (render_process_top(),    3),
                DisplayMode::Temperature  => (render_temperature(),    5),
                DisplayMode::MemoryDetail => (render_memory_detail(),  4),
                DisplayMode::CpuGraph     => (render_cpu_graph(),      1),
                DisplayMode::NetGraph     => (render_net_graph(),      2),
                DisplayMode::ProcessMemory => (render_process_memory(), 3),
                DisplayMode::DiskIo        => (render_disk_io(),        3),
                DisplayMode::Calendar      => (render_calendar(),       30),
                DisplayMode::Stopwatch    => {
                    let (elapsed_ms, laps, running) = {
                        let sw = state.stopwatch.lock().unwrap();
                        (sw.current_elapsed_ms(), sw.laps.clone(), sw.running)
                    };
                    (render_stopwatch(elapsed_ms, &laps, running), if running { 1 } else { 2 })
                },
                DisplayMode::Metrics => {
                    let title = state.metrics_title.lock().unwrap().clone();
                    let data  = state.metrics_data.lock().unwrap().clone();
                    (render_metrics(&title, &data), 5)
                },
                DisplayMode::Weather => {
                    let wd = state.weather_data.lock().unwrap().clone();
                    (render_weather(&wd), 60)
                },
                DisplayMode::Notes => {
                    let title = state.notes_title.lock().unwrap().clone();
                    let items = state.notes_items.lock().unwrap().clone();
                    (render_notes(&title, &items), 10)
                },
                DisplayMode::QrCode => {
                    let payload = state.qr_payload.lock().unwrap().clone();
                    let title   = state.qr_title.lock().unwrap().clone();
                    // QR content never changes, so refresh every 60 s (just keepalive)
                    (render_qr_code(&payload, &title), 60)
                },
                DisplayMode::RecentAlerts => {
                    let entries: Vec<(u64, bool, String)> = {
                        let h = state.history.lock().unwrap();
                        h.iter().rev().take(5).map(|e| (
                            e.timestamp,
                            matches!(e.kind, HistoryKind::Alert),
                            e.summary.clone(),
                        )).collect()
                    };
                    let refs: Vec<(u64, bool, &str)> = entries.iter().map(|(ts, a, s)| (*ts, *a, s.as_str())).collect();
                    (render_recent_alerts(&refs), 5)
                },
                DisplayMode::Countdown => {
                    let cd_opt = state.countdown.lock().unwrap().clone();
                    if let Some(cd) = cd_opt {
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let remaining = (cd.target_unix as i64) - (now_secs as i64);
                        // Auto-expiry: when countdown just hits zero, record history and
                        // return to the previous mode after showing "TIME'S UP" for 10 s.
                        if remaining <= 0 && !cd.expired_notified {
                            let label = cd.label.clone();
                            {
                                let mut cd_lock = state.countdown.lock().unwrap();
                                if let Some(ref mut c) = *cd_lock { c.expired_notified = true; }
                            }
                            push_history(&state, HistoryKind::Alert,
                                format!("⏰ Countdown expired: {}", if label.is_empty() { "Timer" } else { &label }));
                            log::info!("countdown expired: {label}");
                        }
                        // After 10 s in expired state, return to previous mode
                        if remaining < -10 {
                            let prev = state.prev_mode.lock().unwrap().clone();
                            *state.mode.lock().unwrap() = prev;
                            *state.countdown.lock().unwrap() = None;
                        }
                        (render_countdown(remaining, cd.total_secs as i64, &cd.label), 1)
                    } else {
                        (render_custom_text(&CustomTextPayload {
                            title: Some("No countdown set".into()),
                            ..Default::default()
                        }), 5)
                    }
                },
                DisplayMode::Pomodoro => {
                    // Check for auto-advance (phase expired while running)
                    let (check_running, check_remaining, check_phase) = {
                        let p = state.pomodoro.lock().unwrap();
                        (p.running, p.remaining_secs(), p.phase.as_str().to_string())
                    };
                    if check_running && check_remaining <= 0 {
                        let next_phase = {
                            let mut p = state.pomodoro.lock().unwrap();
                            p.advance_phase();
                            p.phase.as_str().to_string()
                        };
                        let msg = format!("Pomodoro: {} complete → {}", check_phase, next_phase);
                        push_history(&state, HistoryKind::Alert, msg);
                        // Audio notification (best-effort, fire-and-forget)
                        thread::spawn(move || {
                            // Try paplay (PulseAudio), then aplay (ALSA)
                            let played = std::process::Command::new("paplay")
                                .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false);
                            if !played {
                                let _ = std::process::Command::new("aplay")
                                    .arg("/usr/share/sounds/alsa/Front_Center.wav")
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .status();
                            }
                        });
                    }
                    let (phase_str, remaining, total, completed, running) = {
                        let p = state.pomodoro.lock().unwrap();
                        (p.phase.as_str().to_string(), p.remaining_secs(), p.phase_total_secs(),
                         (p.completed_sessions % p.long_break_interval) as u8, p.running)
                    };
                    let interval = if running { 1 } else { 2 };
                    (render_pomodoro(&phase_str, remaining, total, completed, running), interval)
                },
                DisplayMode::Ticker => {
                    let text  = state.ticker_text.lock().unwrap().clone();
                    let color = state.ticker_color.lock().unwrap().clone();
                    let pos   = *state.ticker_pos.lock().unwrap();
                    let img   = render_ticker(&text, pos, &color);
                    // Advance ~80 px per 1-second tick (good ticker speed)
                    let text_w = text.len() as i32 * 26 + 480;
                    {
                        let mut p = state.ticker_pos.lock().unwrap();
                        *p = (*p + 80) % text_w.max(1);
                    }
                    (img, 1)
                },
                DisplayMode::Battery => (render_battery(), 10),
                DisplayMode::WorldClock => {
                    let cities = state.world_clock_cities.lock().unwrap().clone();
                    (render_world_clock(&cities), 1)
                },
                DisplayMode::HttpStatus => {
                    let results = state.http_results.lock().unwrap().clone();
                    (render_http_status(&results), 5)
                },
                DisplayMode::ShellCmd => {
                    let cmd     = state.shell_cmd_text.lock().unwrap().clone();
                    let output  = state.shell_cmd_output.lock().unwrap().clone();
                    let exit_ok = *state.shell_cmd_exit_ok.lock().unwrap();
                    let run_at  = *state.shell_cmd_run_at.lock().unwrap();
                    (render_shell_cmd(&cmd, &output, exit_ok, run_at), 5)
                },
                DisplayMode::PingMonitor => {
                    let results = state.ping_results.lock().unwrap().clone();
                    (render_ping_monitor(&results), 5)
                },
                DisplayMode::Dashboard => (render_dashboard(), 2),
                DisplayMode::LogTail => {
                    let path  = state.log_tail_path.lock().unwrap().clone();
                    let lines = state.log_tail_lines.lock().unwrap().clone();
                    let ok    = *state.log_tail_ok.lock().unwrap();
                    (render_log_tail(&path, &lines, ok), 3)
                },
                DisplayMode::CpuFreq => (render_cpu_freq(), 2),
                DisplayMode::HostnameIp => (render_hostname_ip(), 10),
                DisplayMode::RssFeed => {
                    let title = state.rss_feed_title.lock().unwrap().clone();
                    let items = state.rss_feed_items.lock().unwrap().clone();
                    let ok    = *state.rss_feed_ok.lock().unwrap();
                    (render_rss_feed(&title, &items, ok), 30)
                },
                DisplayMode::ServiceStatus => {
                    let units = state.service_statuses.lock().unwrap().clone();
                    (render_service_status(&units), 5)
                },
                DisplayMode::NetConnections => (render_net_connections(), 5),
                DisplayMode::UptimeFancy    => (render_uptime(), 2),
                DisplayMode::OsInfo         => (render_os_info(), 10),
                DisplayMode::DockerStatus   => {
                    let containers = tokio::task::block_in_place(read_docker_containers);
                    (render_docker_status(&containers), 10)
                },
                DisplayMode::LoggedInUsers  => {
                    let sessions = tokio::task::block_in_place(read_user_sessions);
                    (render_logged_in_users(&sessions), 10)
                },
                DisplayMode::LoadGraph      => (render_load_graph(), 5),
                DisplayMode::MemGraph       => (render_mem_graph(), 5),
                DisplayMode::Progress => {
                    let ps = state.progress_state.lock().unwrap().clone();
                    (render_progress(&ps), 2)
                },
                DisplayMode::TempGraph    => (render_temp_graph(), 5),
                DisplayMode::DiskIoGraph    => (render_disk_io_graph(), 5),
                DisplayMode::GraphsOverview => (render_graphs_overview(), 5),
                DisplayMode::AnalogClock    => (render_analog_clock(), 1),
                DisplayMode::StatusOverview => (render_status_overview(), 3),
                DisplayMode::GameOfLife      => (render_game_of_life(), 0),
                DisplayMode::LedClock        => (render_led_clock(), 1),
                DisplayMode::BounceBall      => (render_bounce_ball(), 0),
            DisplayMode::PingGraph      => {
                    let results = state.ping_results.lock().unwrap().clone();
                    (render_ping_graph(&results), 5)
                },
                DisplayMode::NetSpeedGraph  => (render_net_speed_graph(), 5),
                DisplayMode::CpuCoreBars       => (render_cpu_core_bars(), 5),
                DisplayMode::SpeedometerGauge => (render_speedometer_gauge(), 2),
                DisplayMode::DiskPie          => (render_disk_pie(), 10),
                DisplayMode::ThermalDashboard => (render_thermal_dashboard(), 5),
                DisplayMode::Plasma        => (render_plasma(), 0),
                DisplayMode::Fireworks     => (render_fireworks(), 0),
                DisplayMode::CpuHeatmap    => (render_cpu_heatmap(), 2),
                DisplayMode::MatrixRain => (render_matrix_rain(), 0),
                DisplayMode::Idle
                | DisplayMode::CustomImage
                | DisplayMode::CustomText
                | DisplayMode::Slideshow => unreachable!("handled above"),
            };

            // Update CPU / load / mem history on every live frame
            if matches!(mode, DisplayMode::Clock | DisplayMode::BigClock | DisplayMode::ClockSysinfo | DisplayMode::Sysinfo
                | DisplayMode::CpuDetail | DisplayMode::Network | DisplayMode::DiskUsage
                | DisplayMode::ProcessTop | DisplayMode::Temperature | DisplayMode::MemoryDetail
                | DisplayMode::NetGraph | DisplayMode::LoadGraph | DisplayMode::MemGraph | DisplayMode::TempGraph
                | DisplayMode::DiskIoGraph
                | DisplayMode::NetSpeedGraph
                | DisplayMode::CpuCoreBars) {
                use sysinfo::System;
                let mut s = System::new_all();
                s.refresh_cpu_usage();
                s.refresh_memory();
                push_cpu_history(s.global_cpu_usage());
                let load = System::load_average();
                push_load_history(load.one as f32);
                let total = s.total_memory();
                if total > 0 {
                    push_mem_history(s.used_memory() as f32 / total as f32 * 100.0);
                }
                // Temperature history (used by TempGraph mode)
                {
                    use sysinfo::Components;
                    let comps = Components::new_with_refreshed_list();
                    let hot = comps.iter()
                        .filter_map(|c| c.temperature())
                        .filter(|t| t.is_finite() && *t > 0.0)
                        .fold(0.0_f32, f32::max);
                    if hot > 0.0 { push_temp_history(hot); }
                }
                // Disk I/O history (used by DiskIoGraph mode) - lightweight sample
                {
                    use sysinfo::{DiskRefreshKind, Disks};
                    let mut disks = Disks::new_with_refreshed_list();
                    disks.refresh_specifics(false, DiskRefreshKind::everything());
                    let mut r: u64 = 0;
                    let mut w: u64 = 0;
                    for d in disks.iter() {
                        if d.total_space() > 0 {
                            let io = d.usage();
                            r += io.read_bytes;
                            w += io.written_bytes;
                        }
                    }
                    push_disk_io_history(r as f32, w as f32);
                }
            }
            let img  = apply_rotation(img, rotate_deg);
            let img  = apply_brightness(img, *state.dim_factor.lock().unwrap());
            let jpeg = store_frame(&state, encode_for_panel(&img, quality)?);
            { let mut g = panel.lock().unwrap(); if let Some(p) = g.as_mut() { p.send_jpeg(&jpeg)?; } }
            update_fps(&state);
            consecutive_errors = 0;
            if !state.headless { *state.panel_connected.lock().unwrap() = true; }
            if sleep_secs > 0 {
                thread::sleep(Duration::from_secs(sleep_secs));
            } else {
                // Continuous modes (sleep_secs==0): yield briefly so other threads run.
                thread::sleep(Duration::from_millis(16));
            }
            Ok(())
        })();

        if let Err(e) = result {
            consecutive_errors += 1;
            *state.panel_connected.lock().unwrap() = false;
            log::error!("display loop error #{}: {:#}", consecutive_errors, e);
            push_history(&state, HistoryKind::System,
                format!("USB error #{}: {}", consecutive_errors, e));

            // Exponential backoff: 5 → 10 → 20 → 40 → 60s (capped)
            let backoff_secs: u64 = (5u64 << (consecutive_errors.saturating_sub(1).min(3))) .min(60);
            log::info!("reconnect backoff: {}s (error #{})", backoff_secs, consecutive_errors);
            thread::sleep(Duration::from_secs(backoff_secs));

            if !state.headless {
                // After 3+ consecutive errors use open_with_reset to clear stale
                // kernel driver state left by the previous abrupt disconnect.
                let use_reset = consecutive_errors >= 3;
                let open_result = if use_reset {
                    log::info!("reconnect: using open_with_reset after {} consecutive errors", consecutive_errors);
                    Panel::open_with_reset(state.vid, state.pid)
                } else {
                    Panel::open(state.vid, state.pid)
                };
                match open_result {
                    Ok(mut p) => match p.init() {
                        Ok(()) => {
                            log::info!("reconnected (serial={}, after {} errors)", p.info.serial, consecutive_errors);
                            push_history(&state, HistoryKind::System,
                                format!("USB reconnected (serial={})", p.info.serial));
                            *state.panel_info.lock().unwrap() = Some(p.info.clone());
                            *state.panel_connected.lock().unwrap() = true;
                            *panel.lock().unwrap() = Some(p);
                            consecutive_errors = 0;
                        }
                        Err(init_err) => log::warn!("reconnect init failed: {}", init_err),
                    },
                    Err(open_err) => log::warn!("reconnect open failed: {}", open_err),
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

// ─── Threshold monitoring loop ───────────────────────────────────────────────

/// Background thread: poll CPU usage and temperatures every 15 s;
/// fire an alert notification if any threshold is exceeded.
fn threshold_monitor_loop(state: DaemonState) {
    use sysinfo::{System, Components};
    loop {
        thread::sleep(Duration::from_secs(15));

        let thresholds = state.alert_thresholds.lock().unwrap().clone();

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);

        // Rate-limit
        let last_alert = *state.last_threshold_alert_at.lock().unwrap();
        if now_secs.saturating_sub(last_alert) < thresholds.cooldown_secs {
            continue;
        }

        // ── CPU check ──────────────────────────────────────────────────────
        if thresholds.cpu_pct > 0.0 {
            let mut sys = System::new();
            sys.refresh_cpu_usage();
            thread::sleep(Duration::from_millis(200));
            sys.refresh_cpu_usage();
            let cpu = sys.global_cpu_usage();
            if cpu >= thresholds.cpu_pct {
                log::warn!("threshold alert: CPU {:.0}% ≥ {:.0}%", cpu, thresholds.cpu_pct);
                push_threshold_alert(
                    &state,
                    format!("CPU {:.0}%", cpu),
                    format!("Threshold: {:.0}%", thresholds.cpu_pct),
                    ttlcd::display::NamedColor::Red,
                    now_secs,
                );
                continue;
            }
        }

        // ── Temperature check ──────────────────────────────────────────────
        if thresholds.temp_c > 0.0 {
            let components = Components::new_with_refreshed_list();
            for c in components.iter() {
                if let Some(temp) = c.temperature() {
                    if temp >= thresholds.temp_c {
                        let label = c.label().to_string();
                        log::warn!("threshold alert: {} = {:.0}°C ≥ {:.0}°C", label, temp, thresholds.temp_c);
                        push_threshold_alert(
                            &state,
                            format!("{:.0}°C — {}", temp, label),
                            format!("Threshold: {:.0}°C", thresholds.temp_c),
                            ttlcd::display::NamedColor::Red,
                            now_secs,
                        );
                        break;
                    }
                }
            }
        }
    }
}

/// Push a threshold alert into the history ring buffer and switch to
/// `RecentAlerts` mode briefly (5 s) then return.
fn push_threshold_alert(
    state: &DaemonState,
    title: String,
    subtitle: String,
    color: ttlcd::display::NamedColor,
    now_secs: u64,
) {
    *state.last_threshold_alert_at.lock().unwrap() = now_secs;

    let payload = CustomTextPayload {
        title:     Some(title.clone()),
        subtitle:  Some(subtitle),
        color,
        show_time: true,
        ..Default::default()
    };
    let summary = format!("THRESHOLD — {}", title);
    push_history(state, HistoryKind::Alert, summary);
    *state.pending_text.lock().unwrap() = Some(payload);
    let prev = state.mode.lock().unwrap().clone();
    if !prev.is_transient() {
        *state.prev_mode.lock().unwrap() = prev;
    }
    *state.mode.lock().unwrap() = DisplayMode::CustomText;
    *state.auto_clear_at.lock().unwrap() = Some(now_secs + 10);
}

// ─── Dim schedule loop ────────────────────────────────────────────────────────

/// Background thread: check the dim schedule every 60 s and apply/lift the dim.
fn dim_schedule_loop(state: DaemonState) {
    loop {
        thread::sleep(Duration::from_secs(60));

        let schedule_opt = state.dim_schedule.lock().unwrap().clone();
        let Some(sched) = schedule_opt else { continue };

        let now_hour = Local::now().hour() as u8;
        let in_window = if sched.start_hour <= sched.end_hour {
            now_hour >= sched.start_hour && now_hour < sched.end_hour
        } else {
            // Overnight window e.g. 22:00 – 06:00
            now_hour >= sched.start_hour || now_hour < sched.end_hour
        };

        let current = *state.dim_factor.lock().unwrap();
        let target   = if in_window { sched.dim_level } else { 1.0_f32 };
        if (current - target).abs() > 0.01 {
            log::info!("dim schedule: {} → brightness {:.0}%", if in_window { "dim" } else { "restore" }, target * 100.0);
            *state.dim_factor.lock().unwrap() = target;
        }
    }
}


// ─── Mode schedule loop ───────────────────────────────────────────────────────

/// Path to ~/.config/ttlcd/mode_schedule.json
fn mode_schedule_path() -> Option<std::path::PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("mode_schedule.json"))
}

fn load_mode_schedule() -> Vec<ModeScheduleEntry> {
    (|| -> Option<Vec<ModeScheduleEntry>> {
        let path = mode_schedule_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })().unwrap_or_default()
}

fn save_mode_schedule(entries: &[ModeScheduleEntry]) {
    let Some(path) = mode_schedule_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(text) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(&path, &text);
    }
}


// ─── World clock persistence ──────────────────────────────────────────────────

fn world_clock_path() -> Option<std::path::PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("world_clock.json"))
}

fn load_world_clock() -> Vec<(String, i32)> {
    (|| -> Option<Vec<(String, i32)>> {
        let path = world_clock_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })().unwrap_or_default()
}

fn save_world_clock(cities: &[(String, i32)]) {
    let Some(path) = world_clock_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(text) = serde_json::to_string_pretty(cities) {
        let _ = std::fs::write(&path, &text);
    }
}

// ─── HTTP health-check loop ───────────────────────────────────────────────────

/// Async background loop: periodically checks all configured HTTP monitors.
async fn http_check_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.http_check_interval_secs.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let monitors: Vec<HttpMonitorEntry> = state.http_monitors.lock().unwrap().clone();
        if monitors.is_empty() { continue; }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("ttlcd-rs/0.1 health-checker")
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let mut results: Vec<HttpMonitorResult> = Vec::with_capacity(monitors.len());
        for m in &monitors {
            let start = std::time::Instant::now();
            let resp = client.get(&m.url).send().await;
            let latency_ms = start.elapsed().as_millis() as u64;
            let (status_code, ok) = match resp {
                Ok(r) => {
                    let code = r.status().as_u16();
                    (Some(code), code < 400)
                }
                Err(_) => (None, false),
            };
            results.push(HttpMonitorResult {
                label:       m.label.clone(),
                url:         m.url.clone(),
                status_code,
                latency_ms:  if ok || status_code.is_some() { Some(latency_ms) } else { None },
                ok,
            });
        }
        *state.http_results.lock().unwrap() = results;
    }
}

// ─── Shell command loop ───────────────────────────────────────────────────────

/// Async background loop: periodically runs the configured shell command.
async fn shell_cmd_loop(state: DaemonState) {
    let mut last_run_at: u64 = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let cmd_text = state.shell_cmd_text.lock().unwrap().clone();
        if cmd_text.is_empty() { continue; }

        let interval_secs = *state.shell_cmd_interval_secs.lock().unwrap();
        if interval_secs == 0 { continue; } // manual-only mode

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if now_secs < last_run_at + interval_secs { continue; }
        last_run_at = now_secs;

        run_shell_cmd_once(&state, &cmd_text).await;
    }
}

/// Execute `cmd_text` via `/bin/sh -c` and store output + exit status.
async fn run_shell_cmd_once(state: &DaemonState, cmd_text: &str) {
    use tokio::process::Command;
    let result = Command::new("/bin/sh")
        .args(["-c", cmd_text])
        .output()
        .await;

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                format!("{}
--- stderr ---
{}", stdout.trim_end(), stderr.trim_end())
            };
            *state.shell_cmd_output.lock().unwrap()  = combined;
            *state.shell_cmd_exit_ok.lock().unwrap()  = out.status.success();
            *state.shell_cmd_run_at.lock().unwrap()   = now_secs;
        }
        Err(e) => {
            *state.shell_cmd_output.lock().unwrap()  = format!("exec error: {e}");
            *state.shell_cmd_exit_ok.lock().unwrap()  = false;
            *state.shell_cmd_run_at.lock().unwrap()   = now_secs;
        }
    }
}

// ─── Ping monitor loop ───────────────────────────────────────────────────────

/// Async background loop: periodically pings all configured hosts using the
/// system `ping` command, and stores results in `state.ping_results`.
async fn ping_monitor_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.ping_interval_secs.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let hosts: Vec<PingHostEntry> = state.ping_hosts.lock().unwrap().clone();
        if hosts.is_empty() { continue; }

        let mut results: Vec<PingResult> = Vec::with_capacity(hosts.len());
        for h in &hosts {
            let result = run_ping_once(&h.host).await;
            results.push(PingResult {
                host:       h.host.clone(),
                label:      h.label.clone(),
                latency_ms: result.0,
                loss_pct:   result.1,
            });
        }
        push_ping_history(&results);
        *state.ping_results.lock().unwrap() = results;
    }
}

/// Execute `ping -c 3 -W 1 <host>` and parse the summary.
/// Returns `(latency_ms, loss_pct)`.
async fn run_ping_once(host: &str) -> (Option<f64>, f32) {
    use tokio::process::Command;
    // -c 3 = send 3 packets; -W 1 = 1s timeout per packet
    let out = Command::new("ping")
        .args(["-c", "3", "-W", "1", host])
        .output()
        .await;

    match out {
        Err(_) => (None, 100.0),
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            // Parse "rtt min/avg/max/mdev = X/Y/Z/W ms"
            let avg_ms = stdout.lines()
                .find(|l| l.contains("rtt min/avg"))
                .and_then(|l| l.split('/').nth(4))
                .and_then(|s| s.trim().split(' ').next())
                .and_then(|s| s.parse::<f64>().ok());
            // Parse "N packets transmitted, M received, X% packet loss"
            let loss_pct = stdout.lines()
                .find(|l| l.contains("packet loss"))
                .and_then(|l| {
                    l.split(',')
                        .find(|s| s.contains("packet loss"))
                        .and_then(|s| s.trim().split_whitespace().next())
                        .and_then(|s| s.trim_end_matches('%').parse::<f32>().ok())
                })
                .unwrap_or(100.0);
            (avg_ms, loss_pct)
        }
    }
}




/// Background loop: periodically fetch the configured HTTP JSON metrics source and update metrics_data.
async fn metrics_source_loop(state: DaemonState) {
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    loop {
        let source = state.metrics_source.lock().unwrap().clone();
        let Some(src) = source else {
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        };
        let interval = src.interval_secs.max(5);

        if let Err(e) = fetch_metrics_source(&state, &src).await {
            log::warn!("metrics_source fetch '{}': {e}", src.url);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

/// Fetch the metrics source URL, extract fields, and update metrics_data + metrics_title.
async fn fetch_metrics_source(state: &DaemonState, src: &MetricsSource) -> anyhow::Result<()> {
    use reqwest::Client;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .user_agent("ttlcd/0.13.0")
        .build()?;
    let resp = client.get(&src.url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;

    let mut metrics: Vec<Metric> = Vec::with_capacity(src.fields.len());
    for field in &src.fields {
        let val = json_path_get(&body, &field.path);
        let val_str = match val {
            Some(v) => {
                let raw = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                // Apply simple format: replace "{}" with the value
                if field.format == "{}" || field.format.is_empty() {
                    raw
                } else {
                    field.format.replacen("{}", &raw, 1)
                }
            }
            None => "N/A".to_string(),
        };
        metrics.push(Metric {
            label: field.label.clone(),
            value: val_str,
            color: field.color.clone(),
        });
    }

    if !src.title.is_empty() {
        *state.metrics_title.lock().unwrap() = src.title.clone();
    }
    *state.metrics_data.lock().unwrap() = metrics;
    log::debug!("metrics_source updated from '{}'", src.url);
    Ok(())
}

/// Walk a JSON value using a dot-separated path string.
/// Array indices are supported: "data.0.value" navigates `data[0].value`.
fn json_path_get<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = root;
    for segment in path.split('.') {
        cur = if let Ok(idx) = segment.parse::<usize>() {
            cur.get(idx)?
        } else {
            cur.get(segment)?
        };
    }
    Some(cur)
}

/// Background loop: periodically fetch weather from wttr.in.
/// Only runs when `weather_location` is non-empty and `weather_auto_secs > 0`.
async fn weather_auto_loop(state: DaemonState) {
    // Stagger start a bit so it doesn't hammer the network at startup
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    loop {
        let location = state.weather_location.lock().unwrap().clone();
        let interval = *state.weather_auto_secs.lock().unwrap();

        if location.is_empty() || interval == 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            continue;
        }

        if let Err(e) = fetch_wttr(&state, &location).await {
            log::warn!("weather auto-fetch for '{}': {e}", location);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval.max(60))).await;
    }
}

/// Fetch current conditions from wttr.in JSON API and update weather_data.
async fn fetch_wttr(state: &DaemonState, location: &str) -> anyhow::Result<()> {
    use reqwest::Client;
    let url = format!("https://wttr.in/{}?format=j1", urlencoding_encode(location));
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("ttlcd/0.13.0")
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("wttr.in HTTP {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;

    let cc = body["current_condition"].get(0).cloned().unwrap_or_default();
    let w0 = body["weather"].get(0).cloned().unwrap_or_default();

    let temp_c: Option<f32>      = cc["temp_C"].as_str().and_then(|s| s.parse().ok());
    let feels_c: Option<f32>     = cc["FeelsLikeC"].as_str().and_then(|s| s.parse().ok());
    let humidity: Option<u8>     = cc["humidity"].as_str().and_then(|s| s.parse().ok());
    let wind_kmh: Option<f32>    = cc["windspeedKmph"].as_str().and_then(|s| s.parse().ok());
    let wind_dir: String         = cc["winddir16Point"].as_str().unwrap_or("").to_string();
    let condition: String        = cc["weatherDesc"].get(0)
        .and_then(|d| d["value"].as_str())
        .unwrap_or("Unknown")
        .to_string();
    let visibility: Option<f32>  = cc["visibility"].as_str().and_then(|s| s.parse().ok());
    let max_c: Option<f32>       = w0["maxtempC"].as_str().and_then(|s| s.parse().ok());
    let min_c: Option<f32>       = w0["mintempC"].as_str().and_then(|s| s.parse().ok());

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);

    let wd = WeatherData {
        city:           location.to_string(),
        temp_c,
        feels_like_c:   feels_c,
        condition,
        humidity_pct:   humidity,
        wind_kmh,
        wind_dir,
        high_c:         max_c,
        low_c:          min_c,
        visibility_km:  visibility,
        updated_at:     Some(now_secs),
    };
    *state.weather_data.lock().unwrap() = wd;
    log::info!("weather updated for '{}'", location);
    Ok(())
}

/// Percent-encode a location string for use in a URL path component.
/// Only encodes characters that are not safe in a URL path.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '+' => out.push(c),
            ' ' => out.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    out.push('%');
                    out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0').to_ascii_uppercase());
                    out.push(char::from_digit((byte & 0xf) as u32, 16).unwrap_or('0').to_ascii_uppercase());
                }
            }
        }
    }
    out
}

/// Background loop: periodically re-reads the tail of the configured log file.
async fn log_tail_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.log_tail_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(1))).await;

        let path = state.log_tail_path.lock().unwrap().clone();
        if path.is_empty() { continue; }

        let n = *state.log_tail_n.lock().unwrap();
        match read_tail_lines(&path, n) {
            Ok(lines) => {
                *state.log_tail_lines.lock().unwrap() = lines;
                *state.log_tail_ok.lock().unwrap()    = true;
            }
            Err(e) => {
                log::warn!("log_tail: cannot read '{}': {e}", path);
                *state.log_tail_ok.lock().unwrap() = false;
            }
        }
    }
}

/// Read the last `n` lines from a file efficiently.
fn read_tail_lines(path: &str, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    use std::fs::File;

    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    if file_size == 0 || n == 0 {
        return Ok(Vec::new());
    }

    // Walk backwards in 8 KB chunks counting newlines
    const CHUNK: u64 = 8192;
    let mut reader = BufReader::new(file);
    let mut pos = file_size;
    let mut newline_count: usize = 0;
    let mut start: u64 = 0;

    'outer: while pos > 0 {
        let read_from = if pos >= CHUNK { pos - CHUNK } else { 0 };
        let read_len  = (pos - read_from) as usize;
        let mut buf = vec![0u8; read_len];
        reader.seek(SeekFrom::Start(read_from))?;
        reader.read_exact(&mut buf)?;

        for (i, &b) in buf.iter().enumerate().rev() {
            if b == b'\n' {
                newline_count += 1;
                // We want n complete lines — that means n+1 newlines (last char of file may be \n)
                if newline_count > n {
                    start = read_from + i as u64 + 1;
                    break 'outer;
                }
            }
        }
        pos = read_from;
    }

    reader.seek(SeekFrom::Start(start))?;
    let lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(if lines.len() > n { lines[lines.len() - n..].to_vec() } else { lines })
}


// ─── RSS feed background loop ─────────────────────────────────────────────────

/// Parse a simple RSS 2.0 or Atom feed from XML text.
/// Returns (feed_title, Vec<(item_title, item_date_or_author)>)
fn parse_rss_xml(xml: &str) -> (String, Vec<(String, String)>) {
    // Very lightweight XML-free parser using string scanning
    fn extract_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
        let open  = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        let start = text.find(&open)? + open.len();
        let end   = text[start..].find(&close)? + start;
        Some(&text[start..end])
    }
    fn extract_cdata(s: &str) -> &str {
        if let Some(inner) = s.strip_prefix("<![CDATA[") {
            inner.strip_suffix("]]>").unwrap_or(inner)
        } else { s }
    }
    fn decode_entities(s: &str) -> String {
        s.replace("&amp;", "&")
         .replace("&lt;", "<")
         .replace("&gt;", ">")
         .replace("&quot;", "\"")
         .replace("&apos;", "'")
         .replace("&#39;", "'")
         .replace("&#x27;", "'")
    }

    let is_atom = xml.contains("xmlns=\"http://www.w3.org/2005/Atom\"");
    let mut feed_title = String::new();
    let mut items: Vec<(String, String)> = Vec::new();

    if is_atom {
        // Atom: <feed><title>...</title> and <entry><title/><updated/>...
        if let Some(raw) = extract_tag(xml, "title") {
            feed_title = decode_entities(extract_cdata(raw));
        }
        // Parse <entry> blocks
        let mut rest = xml;
        while let Some(entry_start) = rest.find("<entry>") {
            let after = &rest[entry_start + 7..];
            let entry_end = after.find("</entry>").unwrap_or(after.len());
            let block = &after[..entry_end];

            let title = extract_tag(block, "title")
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            let meta = extract_tag(block, "updated")
                .or_else(|| extract_tag(block, "published"))
                .or_else(|| extract_tag(block, "author"))
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            // Shorten ISO date to date only
            let meta = meta.split('T').next().unwrap_or(&meta).to_string();
            if !title.is_empty() { items.push((title, meta)); }
            rest = &after[entry_end..];
            if rest.len() < 8 { break; }
        }
    } else {
        // RSS 2.0: <channel><title>...</title> and <item>...</item>
        // Find channel title (first <title> that is not inside an item)
        if let Some(ch_start) = xml.find("<channel>") {
            let ch = &xml[ch_start..];
            if let Some(raw) = extract_tag(ch, "title") {
                feed_title = decode_entities(extract_cdata(raw));
            }
        }
        let mut rest = xml;
        while let Some(item_start) = rest.find("<item>") {
            let after = &rest[item_start + 6..];
            let item_end = after.find("</item>").unwrap_or(after.len());
            let block = &after[..item_end];

            let title = extract_tag(block, "title")
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            let meta = extract_tag(block, "pubDate")
                .or_else(|| extract_tag(block, "dc:date"))
                .or_else(|| extract_tag(block, "author"))
                .map(|s| {
                    let s = decode_entities(extract_cdata(s));
                    // Shorten RFC-2822 date: "Wed, 14 May 2026 12:00:00 +0000" -> "14 May 2026 12:00"
                    // Strip leading weekday
                    let s2 = if s.contains(',') {
                        s.splitn(2, ',').nth(1).unwrap_or(&s).trim().to_string()
                    } else { s.clone() };
                    // Take date + time but drop seconds and timezone: "14 May 2026 12:00"
                    let parts: Vec<&str> = s2.split_whitespace().collect();
                    if parts.len() >= 5 {
                        // parts: ["14","May","2026","12:00:00","+0000"]
                        let time_short = parts[3].splitn(3, ':').take(2).collect::<Vec<_>>().join(":");
                        format!("{} {} {} {}", parts[0], parts[1], parts[2], time_short)
                    } else {
                        s2.chars().take(20).collect::<String>().trim().to_string()
                    }
                })
                .unwrap_or_default();
            if !title.is_empty() { items.push((title, meta)); }
            rest = &after[item_end..];
            if rest.len() < 6 { break; }
        }
    }

    (feed_title, items)
}

/// Fetch the RSS/Atom feed URL and update the cache.
async fn fetch_rss_feed(state: &DaemonState, url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("ttlcd/0.13 rss-reader")
        .build()?;
    let text = client.get(url).send().await?.text().await?;
    let (feed_title, items) = parse_rss_xml(&text);
    *state.rss_feed_title.lock().unwrap() = if feed_title.is_empty() { "RSS FEED".to_string() } else { feed_title };
    *state.rss_feed_items.lock().unwrap() = items;
    *state.rss_feed_ok.lock().unwrap()    = true;
    Ok(())
}

/// Background loop: re-fetches the RSS feed at the configured interval.
async fn rss_feed_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.rss_feed_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(30))).await;

        let url = state.rss_feed_url.lock().unwrap().clone();
        if url.is_empty() { continue; }

        if let Err(e) = fetch_rss_feed(&state, &url).await {
            log::warn!("rss_feed: fetch error for '{}': {e:#}", url);
            *state.rss_feed_ok.lock().unwrap() = false;
        }
    }
}

// ─── Service status background loop ──────────────────────────────────────────

/// Query systemd service states via `systemctl show`.
fn query_service_statuses(units: &[String]) -> Vec<ttlcd::display::ServiceStatus> {
    units.iter().map(|name| {
        let out = std::process::Command::new("systemctl")
            .args(["show", name, "--property=ActiveState,SubState", "--no-pager"])
            .output();
        let (state, sub) = if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            let mut st  = String::from("unknown");
            let mut sub = String::from("unknown");
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("ActiveState=") { st  = v.to_string(); }
                if let Some(v) = line.strip_prefix("SubState=")    { sub = v.to_string(); }
            }
            (st, sub)
        } else {
            ("unknown".to_string(), "unknown".to_string())
        };
        ttlcd::display::ServiceStatus { name: name.clone(), state, sub }
    }).collect()
}

/// Background loop: periodically polls systemd unit states.
async fn service_status_loop(state: DaemonState) {
    // prev_states: service name → last known ActiveState string
    let mut prev_states: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    loop {
        let interval_secs = *state.service_poll_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let units: Vec<String> = state.service_units.lock().unwrap().clone();
        if units.is_empty() { prev_states.clear(); continue; }

        let statuses = tokio::task::spawn_blocking(move || query_service_statuses(&units))
            .await
            .unwrap_or_default();

        // ── State-change alerting ──────────────────────────────────────────
        for svc in &statuses {
            let prev = prev_states.get(&svc.name).map(|s| s.as_str()).unwrap_or("unknown");
            let cur  = svc.state.as_str();
            if prev != cur {
                // Always log transitions
                log::info!("service state change: {} {} → {}", svc.name, prev, cur);

                // Alert on failure or unexpected stops
                let is_bad   = cur == "failed" || (cur == "inactive" && prev == "active");
                let is_recov = cur == "active"  && (prev == "failed" || prev == "inactive");
                if is_bad || is_recov {
                    let emoji   = if is_bad { "✗" } else { "✓" };
                    let verdict = if is_bad { "FAILED" } else { "RECOVERED" };
                    let summary = format!("{emoji} Service {}: {} ({} → {})",
                        verdict, svc.name, prev, cur);

                    // Show on display briefly
                    let color = if is_bad {
                        ttlcd::display::NamedColor::Red
                    } else {
                        ttlcd::display::NamedColor::Green
                    };
                    let now_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    push_threshold_alert(
                        &state,
                        format!("{emoji} {}", svc.name.chars().take(20).collect::<String>()),
                        format!("{} ({} → {})", verdict, prev, cur),
                        color,
                        now_secs,
                    );

                    // Fire outbound webhook if configured
                    let fwd_url_opt = state.webhook_forward_url.lock().unwrap().clone();
                    if let Some(fwd_url) = fwd_url_opt {
                        let payload = serde_json::json!({
                            "event":   "service_state_change",
                            "service": svc.name,
                            "state":   cur,
                            "prev":    prev,
                            "verdict": verdict,
                            "summary": summary,
                        });
                        tokio::spawn(async move {
                            match reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(10))
                                .build()
                            {
                                Ok(client) => match client.post(&fwd_url).json(&payload).send().await {
                                    Ok(r)  => log::info!("service alert webhook → {} status {}", fwd_url, r.status()),
                                    Err(e) => log::warn!("service alert webhook → {} failed: {}", fwd_url, e),
                                },
                                Err(e) => log::warn!("service alert: could not build client: {}", e),
                            }
                        });
                    }
                }
            }
        }

        // Update prev_states map for next iteration
        for svc in &statuses {
            prev_states.insert(svc.name.clone(), svc.state.clone());
        }

        *state.service_statuses.lock().unwrap() = statuses;
    }
}

/// Background loop: every 30 s, check if a mode schedule entry matches the current time.
/// Uses "most recent past slot" logic — finds the latest entry whose (hour, minute) <= now.
fn mode_schedule_loop(state: DaemonState) {
    use chrono::Timelike;
    // Track which slot we last activated to avoid repeated switches
    let mut last_activated: Option<(u8, u8)> = None;

    loop {
        thread::sleep(Duration::from_secs(30));

        let entries = state.mode_schedule.lock().unwrap().clone();
        if entries.is_empty() { last_activated = None; continue; }

        let now = Local::now();
        let now_h = now.hour() as u8;
        let now_m = now.minute() as u8;

        // Find the "active" entry: the latest one whose (hour, minute) <= (now_h, now_m).
        // Wrap-around: if no past entry exists, use the last entry of the previous day.
        let active = entries.iter()
            .filter(|e| (e.hour, e.minute) <= (now_h, now_m))
            .max_by_key(|e| (e.hour, e.minute));
        let active = active.or_else(|| entries.iter().max_by_key(|e| (e.hour, e.minute)));

        if let Some(entry) = active {
            let slot = (entry.hour, entry.minute);
            if last_activated != Some(slot) {
                if let Ok(mode) = entry.mode.parse::<DisplayMode>() {
                    let cur = state.mode.lock().unwrap().clone();
                    if cur != mode {
                        log::info!("mode schedule: switching to {} (slot {:02}:{:02})", entry.mode, entry.hour, entry.minute);
                        if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
                        *state.mode.lock().unwrap() = mode;
                        push_history(&state, HistoryKind::System,
                            format!("Scheduled mode switch → {} at {:02}:{:02}", entry.mode, entry.hour, entry.minute));
                    }
                    last_activated = Some(slot);
                }
            }
        }
    }
}

// ─── Auth middleware ─────────────────────────────────────────────────────────

/// Actix middleware that enforces Bearer-token authentication when an API key
/// has been configured via POST /api/auth/key.
///
/// The following paths are always exempt (no key required):
///   - `/`  (web UI)
///   - `/static/*`
///   - `/health`, `/ping`
///   - `/api/events`, `/api/stream`  (SSE and MJPEG — often used by dashboards)
///   - `/api/auth/*`  (so you can set/check the key without already having it)
///
/// All other paths require `Authorization: Bearer <key>`.
async fn auth_middleware(
    req: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, actix_web::Error> {
    let state_opt = req.app_data::<web::Data<DaemonState>>();
    let key_opt: Option<String> = state_opt
        .and_then(|s| s.api_key.lock().ok())
        .and_then(|g| g.clone());

    if let Some(expected) = key_opt {
        let path = req.path();
        let skip = path == "/"
            || path.starts_with("/static/")
            || path == "/health"
            || path == "/ping"
            || path.starts_with("/api/events")
            || path.starts_with("/api/stream")
            || path.starts_with("/api/auth/");

        if !skip {
            let auth_ok = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|t| t.trim() == expected.as_str())
                .unwrap_or(false);

            if !auth_ok {
                return Err(actix_web::error::ErrorUnauthorized(
                    r#"{"error":"Unauthorized","message":"Bearer token required. Set via POST /api/auth/key"}"#,
                ));
            }
        }
    }

    // Update last-activity timestamp for screensaver idle detection.
    // Skip updates for pure-static or SSE/stream paths.
    {
        let path = req.path();
        let is_static = path == "/" || path.starts_with("/static/")
            || path.starts_with("/api/events") || path.starts_with("/api/stream")
            || path == "/api/stream.mjpeg" || path == "/metrics";
        if !is_static {
            if let Some(s) = req.app_data::<web::Data<DaemonState>>() {
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                *s.last_activity_secs.lock().unwrap() = now_secs;
                // If screensaver was active, deactivate it and restore the previous mode
                let was_active = {
                    let mut a = s.screensaver_active.lock().unwrap();
                    if *a { *a = false; true } else { false }
                };
                if was_active {
                    let prev = s.prev_mode.lock().unwrap().clone();
                    *s.mode.lock().unwrap() = prev;
                }
            }
        }
    }

    next.call(req).await
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
    /// Seconds remaining in countdown (None if not in countdown mode).
    countdown_remaining_secs: Option<i64>,
    /// Countdown label (None if not in countdown mode).
    countdown_label: Option<String>,
    /// Display brightness 0.0–1.0 (1.0 = full brightness).
    brightness: f32,
    /// Exponential-moving-average of frames rendered per second.
    render_fps: f32,
    /// Seconds since the daemon started.
    daemon_uptime_secs: u64,
    /// Whether the display is currently frozen (updates paused).
    frozen: bool,
    /// Whether the USB panel is currently connected (always false in headless mode).
    panel_connected: bool,
    /// Whether the stopwatch is currently running.
    stopwatch_running: bool,
    /// Current stopwatch elapsed milliseconds.
    stopwatch_elapsed_ms: u64,
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
    // Countdown remaining
    let (cd_remaining, cd_label) = {
        let cd = state.countdown.lock().unwrap().clone();
        if let Some(c) = cd {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            let rem = (c.target_unix as i64) - (now as i64);
            (Some(rem), Some(c.label))
        } else {
            (None, None)
        }
    };

    // Extract stopwatch fields before building the response to avoid
    // holding two MutexGuard temporaries on the same mutex in one expression.
    let (sw_running, sw_elapsed_ms) = {
        let sw = state.stopwatch.lock().unwrap();
        (sw.running, sw.current_elapsed_ms())
    };
    let rotate_deg        = *state.rotate_deg.lock().unwrap();
    let quality           = *state.quality.lock().unwrap();
    let pending_scheduled = *state.pending_scheduled.lock().unwrap();
    let brightness        = *state.dim_factor.lock().unwrap();
    let render_fps        = *state.render_fps.lock().unwrap();
    let frozen            = *state.frozen.lock().unwrap();
    let daemon_uptime_secs = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        now.saturating_sub(state.daemon_start_secs)
    };

    HttpResponse::Ok().json(StatusResponse {
        mode,
        version:               env!("CARGO_PKG_VERSION"),
        available_modes:       DisplayMode::ALL_NAMES,
        headless:              state.headless,
        rotate_deg,
        quality,
        frame_seq,
        can_clear,
        rotation_interval:     rot_interval,
        rotation_modes:        rot_modes,
        slideshow_image_count: ss_count,
        slideshow_interval:    ss_interval,
        slideshow_current_idx: ss_idx,
        pending_scheduled,
        countdown_remaining_secs: cd_remaining,
        countdown_label:          cd_label,
        brightness,
        render_fps,
        daemon_uptime_secs,
        frozen,
        panel_connected: *state.panel_connected.lock().unwrap(),
        stopwatch_running:    sw_running,
        stopwatch_elapsed_ms: sw_elapsed_ms,
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
            persist_state(&state);
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
    push_history(&state, HistoryKind::Image, format!("Image uploaded ({} bytes)", raw.len()));
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
    let modes: Vec<ModeInfo> = vec![
        ModeInfo { name: "clock",          description: "Large digital clock with date and uptime" },
        ModeInfo { name: "big_clock",      description: "Full-screen giant HH:MM with large seconds and progress bar" },
        ModeInfo { name: "clock_sysinfo",  description: "Split-screen: clock above, quick stats below" },
        ModeInfo { name: "sysinfo",        description: "CPU, RAM, temperature, load average, uptime" },
        ModeInfo { name: "cpu_detail",     description: "Per-core CPU usage grid (up to 32 cores)" },
        ModeInfo { name: "network",        description: "Per-interface Rx/Tx rates and totals" },
        ModeInfo { name: "disk_usage",     description: "Disk partition usage bars" },
        ModeInfo { name: "process_top",    description: "Top processes sorted by CPU usage" },
        ModeInfo { name: "temperature",    description: "Thermal sensor readings with color-coded bars" },
        ModeInfo { name: "memory_detail",  description: "Detailed RAM breakdown: used, cached, free, swap" },
        ModeInfo { name: "countdown",      description: "Countdown timer with colour transitions and progress bar" },
        ModeInfo { name: "cpu_graph",      description: "CPU usage history bar chart (last 2 minutes)" },
        ModeInfo { name: "net_graph",      description: "Network Rx/Tx I/O history chart (last 60 seconds)" },
        ModeInfo { name: "recent_alerts",  description: "Last 5 notifications as an on-panel log" },
        ModeInfo { name: "process_memory", description: "Top 8 processes sorted by RAM usage" },
        ModeInfo { name: "disk_io",        description: "Disk read/write throughput per block device" },
        ModeInfo { name: "calendar",       description: "Full-month calendar with today highlighted" },
        ModeInfo { name: "stopwatch",      description: "Running stopwatch with lap times" },
        ModeInfo { name: "metrics",        description: "Custom key/value metrics dashboard (up to 8 items)" },
        ModeInfo { name: "weather",        description: "Weather snapshot — push via /api/weather or web UI" },
        ModeInfo { name: "notes",          description: "Bulletin board — push text items via /api/notes" },
        ModeInfo { name: "qr_code",         description: "Display a scannable QR code — set payload via /api/qr" },
        ModeInfo { name: "pomodoro",        description: "Pomodoro focus timer: 25 min work / 5 min break cycles with visual progress ring" },
        ModeInfo { name: "battery",        description: "Battery level, charging status, voltage and estimated time remaining (Linux laptops)" },
        ModeInfo { name: "world_clock",    description: "World clock showing up to 4 cities with configurable UTC offsets and day/night coloring" },
        ModeInfo { name: "ticker",         description: "Scrolling text ticker — push text via POST /api/ticker" },
        ModeInfo { name: "http_status",    description: "HTTP health monitor dashboard — configure endpoints via /api/http_status" },
        ModeInfo { name: "shell_cmd",      description: "Shell command output viewer — set command via POST /api/shell_cmd" },
        ModeInfo { name: "ping_monitor",   description: "ICMP ping latency dashboard — configure hosts via /api/ping_monitor" },
        ModeInfo { name: "dashboard",      description: "Four-quadrant overview: clock, CPU/RAM, network, temperature" },
        ModeInfo { name: "log_tail",       description: "Live log tail viewer — configure file via POST /api/log_tail" },
        ModeInfo { name: "cpu_freq",        description: "Per-core CPU clock frequency grid (reads /proc/cpuinfo)" },
        ModeInfo { name: "hostname_ip",    description: "Machine identification: hostname and all network IP addresses" },
        ModeInfo { name: "rss_feed",       description: "RSS/Atom headline feed — configure URL via POST /api/rss" },
        ModeInfo { name: "service_status", description: "Systemd unit state monitor — configure units via POST /api/services" },
        ModeInfo { name: "net_connections",description: "Active TCP/UDP socket viewer: listening ports and established connections" },
        ModeInfo { name: "uptime_fancy",   description: "Full-screen uptime display: big counter, boot time, load averages, memory" },
        ModeInfo { name: "os_info",        description: "OS and hardware info: distro, kernel, CPU model, memory, uptime" },
        ModeInfo { name: "docker_status",  description: "Docker containers panel: running/stopped containers with image and status" },
        ModeInfo { name: "logged_in_users",description: "Active user sessions: who is logged in, from where, and idle time" },
        ModeInfo { name: "load_graph",     description: "Load average history graph: 1m/5m/15m load avg as a filled line chart" },
        ModeInfo { name: "mem_graph",      description: "Memory usage history graph: RAM % over time as a filled line chart" },
        ModeInfo { name: "progress",       description: "Full-screen progress bar — configure via POST /api/progress with value/total/label" },
        ModeInfo { name: "matrix_rain",    description: "Matrix digital rain screensaver — animated falling characters" },
        ModeInfo { name: "bounce_ball",      description: "Bouncing ball screensaver — Catppuccin colour cycling on wall bounces" },
        ModeInfo { name: "speedometer_gauge", description: "Analog speedometer gauges for CPU % and RAM %" },
        ModeInfo { name: "disk_pie",          description: "Disk usage as pie charts for each mounted filesystem" },
        ModeInfo { name: "thermal_dashboard", description: "Temperature sensor bar chart dashboard — all CPU/GPU/SSD sensors" },
        ModeInfo { name: "led_clock",        description: "7-segment LED-style clock with date, load average, and hostname — flashing colons" },
        ModeInfo { name: "game_of_life",     description: "Conway's Game of Life — animated cellular automaton, re-seeds when population stagnates" },
        ModeInfo { name: "status_overview",  description: "4-quadrant live overview: clock, CPU/RAM, network Rx/Tx, and temperature/uptime" },
        ModeInfo { name: "ping_graph",      description: "Ping latency history graph — overlaid line charts for up to 4 monitored hosts" },
        ModeInfo { name: "analog_clock",    description: "Analog clock face with hour/minute/second hands and date/load display" },
        ModeInfo { name: "net_speed_graph", description: "Network speed history: overlaid Rx/Tx line chart with per-interface breakdown" },
        ModeInfo { name: "cpu_core_bars",   description: "Per-core CPU utilisation as horizontal bars — good for high core-count systems" },
        ModeInfo { name: "temp_graph",     description: "CPU/component temperature history graph with thermal thresholds" },
        ModeInfo { name: "disk_io_graph",  description: "Disk I/O history graph: total read and write throughput over time" },
        ModeInfo { name: "graphs_overview",description: "4-panel mini-charts: CPU, Load, Memory, Temperature at a glance" },
        ModeInfo { name: "idle",           description: "Keepalive — no display update, prevents blank" },
    ];
    HttpResponse::Ok().json(modes)
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
    *state.countdown.lock().unwrap() = Some(CountdownState { target_unix, total_secs, label: label.clone(), expired_notified: false });
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

// ─── Ping ───────────────────────────────────────────────────────────────────────

/// GET /ping or GET /api/ping — returns "pong" instantly.
/// Useful for latency checks and uptime monitoring.
async fn api_ping() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body("pong")
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


// ─── Full reset ───────────────────────────────────────────────────────────────

/// POST /api/display/reset — reset daemon to defaults.
///
/// - Stops rotation (sets interval = 0)
/// - Cancels countdown and screensaver
/// - Returns to `clock` mode
/// - Clears pending JPEG
///
/// Does NOT clear presets, history, thresholds, or persisted settings.
async fn api_display_reset(state: web::Data<DaemonState>) -> impl Responder {
    *state.rotation_interval.lock().unwrap() = 0;
    *state.rotation_modes.lock().unwrap()    = vec![];
    *state.countdown.lock().unwrap()         = None;
    *state.screensaver_active.lock().unwrap()= false;
    *state.pending_jpeg.lock().unwrap()      = None;
    *state.mode.lock().unwrap()              = DisplayMode::Clock;
    *state.prev_mode.lock().unwrap()         = DisplayMode::Clock;
    log::info!("display reset to defaults");
    push_history(&state, HistoryKind::System, "display reset".to_string());
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "mode": "clock",
        "message": "display reset to defaults"
    }))
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

#[derive(Deserialize)]
struct ImageUrlRequest {
    /// HTTP/HTTPS URL of the image to fetch and display.
    url: String,
    /// If true, letterbox (fit with black bars) instead of cropping. Defaults to true.
    #[serde(default = "bool_true")]
    letterbox: bool,
}

/// POST /api/image/url — fetch an image from an HTTP/HTTPS URL and display it on the panel.
///
/// Useful for dashboards that want to push live chart images, webcam frames,
/// or any image hosted on the network.
///
/// Body: `{ "url": "https://example.com/chart.png", "letterbox": true }`
async fn api_image_url(state: web::Data<DaemonState>, body: web::Json<ImageUrlRequest>) -> impl Responder {
    let req = body.into_inner();
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return HttpResponse::BadRequest().body("url must start with http:// or https://");
    }

    // Fetch image bytes from URL
    let image_bytes = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => match client.get(&url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return HttpResponse::BadGateway()
                        .body(format!("remote returned {}", resp.status()));
                }
                match resp.bytes().await {
                    Ok(b) => b.to_vec(),
                    Err(e) => return HttpResponse::BadGateway().body(format!("read error: {e}")),
                }
            }
            Err(e) => return HttpResponse::BadGateway().body(format!("fetch error: {e}")),
        },
        Err(e) => return HttpResponse::InternalServerError().body(format!("client error: {e}")),
    };

    let quality    = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();
    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = image::load_from_memory(&image_bytes)?;
        let img = apply_rotation(img, rotate_deg);
        if req.letterbox { letterbox_for_panel(&img, quality) } else { encode_for_panel(&img, quality) }
    })() {
        Ok(j)  => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("image decode error: {e}")),
    };
    push_history(&state, HistoryKind::Image, format!("Image from URL: {}", url.chars().take(80).collect::<String>()));
    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;
    HttpResponse::Ok().body("image queued from URL")
}

// ─── Countdown timer state ───────────────────────────────────────────────────

/// Persistent state for the countdown display mode.
#[derive(Clone, Default)]
struct CountdownState {
    /// Unix timestamp (seconds) when the countdown expires.
    target_unix:  u64,
    /// Original total duration in seconds (for progress bar).
    total_secs:   u64,
    /// Optional label shown below the clock.
    label:        String,
    /// True once the expiry notification has been pushed to history (prevents duplicate pushes).
    expired_notified: bool,
}

// ─── Notification history ────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum HistoryKind { Text, Alert, Image, System }

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

    // Fire desktop notification for Alert-kind events (if notify-send is available)
    if matches!(kind, HistoryKind::Alert) {
        let sum2 = summary.clone();
        thread::spawn(move || {
            let _ = std::process::Command::new("notify-send")
                .args(["ttlcd Alert", &sum2, "--icon", "dialog-warning", "--expire-time", "5000"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }

    let entry = HistoryEntry { kind: kind.clone(), summary: summary.clone(), timestamp: ts };
    {
        let mut h = state.history.lock().unwrap();
        if h.len() >= 200 { h.pop_front(); }
        h.push_back(entry);
    }
    // Publish alert events for the SSE stream
    if matches!(kind, HistoryKind::Alert) {
        let mut seq = state.alert_seq.lock().unwrap();
        *seq += 1;
        let new_seq = *seq;
        drop(seq);
        *state.last_alert_event.lock().unwrap() = Some((new_seq, summary.clone()));

        // Fire ntfy.sh notification (fire-and-forget)
        let ntfy_opt = state.ntfy_url.lock().unwrap().clone();
        if let Some(ntfy_url) = ntfy_opt {
            let body = summary.clone();
            tokio::spawn(async move {
                match reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                {
                    Ok(client) => {
                        match client.post(&ntfy_url)
                            .header("Title", "ttlcd Alert")
                            .header("Priority", "high")
                            .header("Tags", "warning")
                            .body(body)
                            .send()
                            .await
                        {
                            Ok(r)  => log::info!("ntfy notification sent → {} ({})", ntfy_url, r.status()),
                            Err(e) => log::warn!("ntfy notification failed: {}", e),
                        }
                    }
                    Err(e) => log::warn!("ntfy: could not build HTTP client: {}", e),
                }
            });
        }
    }
}

/// GET /api/history — return notifications.
///
/// Query params:
///   `limit=N`    — max entries (default 50, max 50)
///   `kind=alert|text|image|system` — filter by entry kind
///   `format=csv` — CSV output (columns: timestamp_unix,kind,summary)
async fn api_history(state: web::Data<DaemonState>, query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let limit: usize = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50).min(200);
    let fmt         = query.get("format").map(|s| s.as_str()).unwrap_or("json").to_string();
    let kind_filter = query.get("kind").cloned();

    fn kind_str(k: &HistoryKind) -> &'static str {
        match k {
            HistoryKind::Alert  => "alert",
            HistoryKind::Text   => "text",
            HistoryKind::Image  => "image",
            HistoryKind::System => "system",
        }
    }

    let h = state.history.lock().unwrap();
    let items: Vec<&HistoryEntry> = h.iter().rev()
        .filter(|e| {
            kind_filter.as_ref()
                .map(|kf| kind_str(&e.kind) == kf.as_str())
                .unwrap_or(true)
        })
        .take(limit)
        .collect();

    if fmt == "csv" {
        let mut csv = String::from("timestamp_unix,kind,summary\n");
        for e in &items {
            let safe = e.summary.replace('"', "\"\"");
            csv.push_str(&format!("{},{},\"{}\"\n", e.timestamp, kind_str(&e.kind), safe));
        }
        return HttpResponse::Ok()
            .content_type("text/csv; charset=utf-8")
            .insert_header(("Content-Disposition", "attachment; filename=\"ttlcd-history.csv\""))
            .body(csv);
    }
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

// ─── Runtime setting persistence ─────────────────────────────────────────────

/// Write a single runtime setting to the config file.
/// This is a best-effort helper: failures are logged but not propagated.
fn persist_setting(key: &str, value: &str) {
    let path = match dirs_config_path() {
        Some(p) => p,
        None    => return,
    };
    // Read existing TOML, or start fresh
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    // Use a simple line-based replace/append strategy
    let key_eq = format!("{key} = ");
    let new_line = format!("{key} = {value}
");
    let updated = if current.contains(&key_eq) {
        current.lines()
            .map(|l| if l.starts_with(&key_eq) { new_line.trim_end().to_string() } else { l.to_string() })
            .collect::<Vec<_>>()
            .join("
")
            + "
"
    } else {
        // Append under [daemon] section or at EOF
        if current.contains("[daemon]") {
            current + &new_line
        } else {
            current + "[daemon]
" + &new_line
        }
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, &updated) {
        log::debug!("could not persist setting {key}={value}: {e}");
    }
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
    persist_state(&state);
    // Persist to config file in background
    tokio::spawn(async move {
        persist_setting("quality", &q.to_string());
    });
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
    persist_state(&state);
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

    push(&mut out, "# HELP ttlcd_brightness Current display brightness 0.0–1.0".into());
    push(&mut out, "# TYPE ttlcd_brightness gauge".into());
    push(&mut out, format!("ttlcd_brightness {:.3}", *state.dim_factor.lock().unwrap()));

    push(&mut out, "# HELP ttlcd_pending_scheduled Scheduled notifications not yet fired".into());
    push(&mut out, "# TYPE ttlcd_pending_scheduled gauge".into());
    push(&mut out, format!("ttlcd_pending_scheduled {}", *state.pending_scheduled.lock().unwrap()));

    push(&mut out, "# HELP ttlcd_swap_used_bytes Swap space used in bytes".into());
    push(&mut out, "# TYPE ttlcd_swap_used_bytes gauge".into());
    push(&mut out, format!("ttlcd_swap_used_bytes {}", sys.used_swap()));

    push(&mut out, "# HELP ttlcd_swap_total_bytes Total swap space in bytes".into());
    push(&mut out, "# TYPE ttlcd_swap_total_bytes gauge".into());
    push(&mut out, format!("ttlcd_swap_total_bytes {}", sys.total_swap()));

    // Per-disk space metrics
    {
        use sysinfo::Disks;
        let disks = Disks::new_with_refreshed_list();
        push(&mut out, "# HELP ttlcd_disk_used_bytes Disk partition used bytes".into());
        push(&mut out, "# TYPE ttlcd_disk_used_bytes gauge".into());
        push(&mut out, "# HELP ttlcd_disk_total_bytes Disk partition total bytes".into());
        push(&mut out, "# TYPE ttlcd_disk_total_bytes gauge".into());
        for d in disks.iter() {
            if d.total_space() == 0 { continue; }
            let mount = d.mount_point().to_string_lossy().replace('"', "");
            let used  = d.total_space() - d.available_space();
            push(&mut out, format!("ttlcd_disk_used_bytes{{mount=\"{mount}\"}} {used}"));
            push(&mut out, format!("ttlcd_disk_total_bytes{{mount=\"{mount}\"}} {}", d.total_space()));
        }
    }

    // All temperature sensors
    {
        push(&mut out, "# HELP ttlcd_sensor_temperature_celsius Temperature sensor reading in Celsius".into());
        push(&mut out, "# TYPE ttlcd_sensor_temperature_celsius gauge".into());
        for c in comps.iter() {
            if let Some(t) = c.temperature() {
                if t.is_nan() || t <= 0.0 { continue; }
                let label = c.label().replace('"', "").replace(' ', "_");
                push(&mut out, format!("ttlcd_sensor_temperature_celsius{{sensor=\"{label}\"}} {:.1}", t));
            }
        }
    }

    // Render FPS
    push(&mut out, "# HELP ttlcd_render_fps Exponential-moving-average render frames per second".into());
    push(&mut out, "# TYPE ttlcd_render_fps gauge".into());
    push(&mut out, format!("ttlcd_render_fps {:.3}", *state.render_fps.lock().unwrap()));

    // Daemon uptime
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    push(&mut out, "# HELP ttlcd_daemon_uptime_seconds Seconds since daemon started".into());
    push(&mut out, "# TYPE ttlcd_daemon_uptime_seconds counter".into());
    push(&mut out, format!("ttlcd_daemon_uptime_seconds {}", now_s.saturating_sub(state.daemon_start_secs)));

    // Frozen state
    push(&mut out, "# HELP ttlcd_display_frozen Whether the display is frozen (1 = yes, 0 = no)".into());
    push(&mut out, "# TYPE ttlcd_display_frozen gauge".into());
    push(&mut out, format!("ttlcd_display_frozen {}", if *state.frozen.lock().unwrap() { 1 } else { 0 }));

    // Stopwatch
    {
        let sw = state.stopwatch.lock().unwrap();
        push(&mut out, "# HELP ttlcd_stopwatch_running Whether the stopwatch is running (1 = yes, 0 = no)".into());
        push(&mut out, "# TYPE ttlcd_stopwatch_running gauge".into());
        push(&mut out, format!("ttlcd_stopwatch_running {}", if sw.running { 1 } else { 0 }));
        push(&mut out, "# HELP ttlcd_stopwatch_elapsed_ms Stopwatch elapsed milliseconds".into());
        push(&mut out, "# TYPE ttlcd_stopwatch_elapsed_ms counter".into());
        push(&mut out, format!("ttlcd_stopwatch_elapsed_ms {}", sw.current_elapsed_ms()));
        push(&mut out, "# HELP ttlcd_stopwatch_lap_count Number of recorded laps".into());
        push(&mut out, "# TYPE ttlcd_stopwatch_lap_count counter".into());
        push(&mut out, format!("ttlcd_stopwatch_lap_count {}", sw.laps.len()));
    }

    // Custom metrics count
    push(&mut out, "# HELP ttlcd_metrics_count Number of custom metric items currently set".into());
    push(&mut out, "# TYPE ttlcd_metrics_count gauge".into());
    push(&mut out, format!("ttlcd_metrics_count {}", state.metrics_data.lock().unwrap().len()));

    // Alert threshold
    {
        let t = state.alert_thresholds.lock().unwrap();
        push(&mut out, "# HELP ttlcd_threshold_cpu_pct CPU alert threshold percent (0 = disabled)".into());
        push(&mut out, "# TYPE ttlcd_threshold_cpu_pct gauge".into());
        push(&mut out, format!("ttlcd_threshold_cpu_pct {}", t.cpu_pct));
        push(&mut out, "# HELP ttlcd_threshold_temp_c Temperature alert threshold Celsius (0 = disabled)".into());
        push(&mut out, "# TYPE ttlcd_threshold_temp_c gauge".into());
        push(&mut out, format!("ttlcd_threshold_temp_c {}", t.temp_c));
    }

    // HTTP monitor metrics
    {
        let results = state.http_results.lock().unwrap().clone();
        if !results.is_empty() {
            push(&mut out, "# HELP ttlcd_http_monitor_up HTTP monitor endpoint up (1=up, 0=down)".into());
            push(&mut out, "# TYPE ttlcd_http_monitor_up gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let url   = r.url.replace('"', "'");
                push(&mut out, format!("ttlcd_http_monitor_up{{label=\"{}\",url=\"{}\"}} {}", label, url, if r.ok { 1 } else { 0 }));
            }
            push(&mut out, "# HELP ttlcd_http_monitor_latency_ms HTTP monitor last response latency milliseconds".into());
            push(&mut out, "# TYPE ttlcd_http_monitor_latency_ms gauge".into());
            for r in &results {
                if let Some(ms) = r.latency_ms {
                    let label = r.label.replace('"', "'");
                    let url   = r.url.replace('"', "'");
                    push(&mut out, format!("ttlcd_http_monitor_latency_ms{{label=\"{}\",url=\"{}\"}} {}", label, url, ms));
                }
            }
            push(&mut out, "# HELP ttlcd_http_monitor_status_code HTTP monitor last HTTP status code (0=no response)".into());
            push(&mut out, "# TYPE ttlcd_http_monitor_status_code gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let url   = r.url.replace('"', "'");
                push(&mut out, format!("ttlcd_http_monitor_status_code{{label=\"{}\",url=\"{}\"}} {}", label, url, r.status_code.unwrap_or(0)));
            }
        }
    }

    // Ping monitor metrics
    {
        let results = state.ping_results.lock().unwrap().clone();
        if !results.is_empty() {
            push(&mut out, "# HELP ttlcd_ping_latency_ms Ping round-trip latency milliseconds (-1=unreachable)".into());
            push(&mut out, "# TYPE ttlcd_ping_latency_ms gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let host  = r.host.replace('"', "'");
                let val   = r.latency_ms.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-1".into());
                push(&mut out, format!("ttlcd_ping_latency_ms{{label=\"{}\",host=\"{}\"}} {}", label, host, val));
            }
            push(&mut out, "# HELP ttlcd_ping_loss_percent Ping packet loss percent".into());
            push(&mut out, "# TYPE ttlcd_ping_loss_percent gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let host  = r.host.replace('"', "'");
                push(&mut out, format!("ttlcd_ping_loss_percent{{label=\"{}\",host=\"{}\"}} {:.1}", label, host, r.loss_pct));
            }
        }
    }

    // Weather metrics
    {
        let wd = state.weather_data.lock().unwrap().clone();
        if let Some(temp) = wd.temp_c {
            push(&mut out, "# HELP ttlcd_weather_temp_c Last fetched weather temperature Celsius".into());
            push(&mut out, "# TYPE ttlcd_weather_temp_c gauge".into());
            let city = wd.city.replace('"', "'");
            push(&mut out, format!("ttlcd_weather_temp_c{{city=\"{}\"}} {:.1}", city, temp));
            if let Some(humidity) = wd.humidity_pct {
                push(&mut out, "# HELP ttlcd_weather_humidity_pct Last fetched weather humidity percent".into());
                push(&mut out, "# TYPE ttlcd_weather_humidity_pct gauge".into());
                push(&mut out, format!("ttlcd_weather_humidity_pct{{city=\"{}\"}} {}", city, humidity));
            }
            if let Some(wind) = wd.wind_kmh {
                push(&mut out, "# HELP ttlcd_weather_wind_kmh Last fetched weather wind speed km/h".into());
                push(&mut out, "# TYPE ttlcd_weather_wind_kmh gauge".into());
                push(&mut out, format!("ttlcd_weather_wind_kmh{{city=\"{}\"}} {:.1}", city, wind));
            }
        }
    }

    // Custom metrics source values (if populated)
    {
        let metrics = state.metrics_data.lock().unwrap().clone();
        if !metrics.is_empty() && state.metrics_source.lock().unwrap().is_some() {
            push(&mut out, "# HELP ttlcd_metrics_source_value Custom metric fetched from HTTP JSON source".into());
            push(&mut out, "# TYPE ttlcd_metrics_source_value untyped".into());
            for m in &metrics {
                let label_safe = m.label.replace('"', "'");
                // Try to parse value as a number for Prometheus
                let raw_val = m.value.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-');
                if let Ok(v) = raw_val.parse::<f64>() {
                    push(&mut out, format!("ttlcd_metrics_source_value{{label=\"{}\"}} {}", label_safe, v));
                }
            }
        }
    }


    // RSS feed availability
    {
        let ok  = *state.rss_feed_ok.lock().unwrap();
        let url = state.rss_feed_url.lock().unwrap().clone();
        if !url.is_empty() {
            let url_safe = url.replace('"', "'");
            push(&mut out, "# HELP ttlcd_rss_feed_up 1 if last RSS feed fetch succeeded".into());
            push(&mut out, "# TYPE ttlcd_rss_feed_up gauge".into());
            push(&mut out, format!("ttlcd_rss_feed_up{{url=\"{}\"}} {}", url_safe, if ok { 1 } else { 0 }));
            push(&mut out, "# HELP ttlcd_rss_feed_items_total Number of RSS items in cache".into());
            push(&mut out, "# TYPE ttlcd_rss_feed_items_total gauge".into());
            push(&mut out, format!("ttlcd_rss_feed_items_total{{url=\"{}\"}} {}", url_safe,
                state.rss_feed_items.lock().unwrap().len()));
        }
    }

    // Service statuses
    {
        let statuses = state.service_statuses.lock().unwrap().clone();
        if !statuses.is_empty() {
            push(&mut out, "# HELP ttlcd_service_active 1 if systemd unit is active".into());
            push(&mut out, "# TYPE ttlcd_service_active gauge".into());
            for svc in &statuses {
                let name_safe = svc.name.replace('"', "'");
                push(&mut out, format!("ttlcd_service_active{{name=\"{}\"}} {}", name_safe, if svc.state == "active" { 1 } else { 0 }));
                if svc.state == "failed" {
                    push(&mut out, format!("ttlcd_service_failed{{name=\"{}\"}} 1", name_safe));
                }
            }
        }
    }

    // ── Docker container metrics ──────────────────────────────────────────────
    {
        let containers = tokio::task::block_in_place(read_docker_containers);
        if !containers.is_empty() {
            push(&mut out, "# HELP ttlcd_docker_running 1 if container is running, 0 if stopped".into());
            push(&mut out, "# TYPE ttlcd_docker_running gauge".into());
            for c in &containers {
                let name_safe  = c.name.trim_start_matches('/').replace('"', "'");
                let image_safe = c.image.replace('"', "'");
                push(&mut out, format!(
                    "ttlcd_docker_running{{name=\"{}\",image=\"{}\"}} {}",
                    name_safe, image_safe, if c.running { 1 } else { 0 }
                ));
            }
            let running_count = containers.iter().filter(|c| c.running).count();
            push(&mut out, "# HELP ttlcd_docker_running_total Number of running containers".into());
            push(&mut out, "# TYPE ttlcd_docker_running_total gauge".into());
            push(&mut out, format!("ttlcd_docker_running_total {}", running_count));
            push(&mut out, "# HELP ttlcd_docker_total_total Total number of containers".into());
            push(&mut out, "# TYPE ttlcd_docker_total_total gauge".into());
            push(&mut out, format!("ttlcd_docker_total_total {}", containers.len()));
        }
    }

    // Screensaver metrics
    {
        let timeout = *state.screensaver_timeout_secs.lock().unwrap();
        let active  = *state.screensaver_active.lock().unwrap();
        let last    = *state.last_activity_secs.lock().unwrap();
        let now_s2  = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        push(&mut out, "# HELP ttlcd_screensaver_active 1 if screensaver is currently active".into());
        push(&mut out, "# TYPE ttlcd_screensaver_active gauge".into());
        push(&mut out, format!("ttlcd_screensaver_active {}", if active { 1 } else { 0 }));
        push(&mut out, "# HELP ttlcd_screensaver_timeout_secs Configured screensaver idle timeout (0=disabled)".into());
        push(&mut out, "# TYPE ttlcd_screensaver_timeout_secs gauge".into());
        push(&mut out, format!("ttlcd_screensaver_timeout_secs {}", timeout));
        push(&mut out, "# HELP ttlcd_seconds_since_activity Seconds since last API activity".into());
        push(&mut out, "# TYPE ttlcd_seconds_since_activity gauge".into());
        push(&mut out, format!("ttlcd_seconds_since_activity {}", now_s2.saturating_sub(last)));
    }

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

/// GET /api/preview.png — current panel frame as lossless PNG.
///
/// Decodes the stored JPEG and re-encodes as PNG. Useful for high-quality
/// screenshots, image processing, and clients that prefer lossless format.
async fn api_preview_frame_png(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => {
            // Decode JPEG → re-encode as PNG
            match image::load_from_memory(jpeg) {
                Ok(img) => {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    match img.write_to(&mut buf, image::ImageFormat::Png) {
                        Ok(()) => HttpResponse::Ok()
                            .insert_header(("Cache-Control", "no-store"))
                            .content_type("image/png")
                            .body(buf.into_inner()),
                        Err(e) => HttpResponse::InternalServerError()
                            .body(format!("PNG encode error: {e}")),
                    }
                }
                Err(e) => HttpResponse::InternalServerError()
                    .body(format!("JPEG decode error: {e}")),
            }
        }
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
    let frame_seq       = Arc::clone(&state.frame_seq);
    let mode_arc        = Arc::clone(&state.mode);
    let frozen_arc      = Arc::clone(&state.frozen);
    let alert_seq_arc   = Arc::clone(&state.alert_seq);
    let last_alert_arc  = Arc::clone(&state.last_alert_event);

    let init_mode      = mode_arc.lock().unwrap().as_str().to_string();
    let init_alert_seq = *alert_seq_arc.lock().unwrap();

    // State tuple: (frame_seq, last_frame_seq, last_heartbeat, last_mode,
    //               mode_arc, frozen_arc, alert_seq_arc, last_alert_arc, last_alert_seq)
    let stream = futures_util::stream::unfold(
        (frame_seq, u64::MAX, std::time::Instant::now(), init_mode,
         mode_arc, frozen_arc, alert_seq_arc, last_alert_arc, init_alert_seq),
        |(seq, last_seq, last_hb, last_mode, mode_arc, frozen_arc,
          alert_seq_arc, last_alert_arc, last_alert_seq)| async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let cur       = *seq.lock().unwrap();
                let mode      = mode_arc.lock().unwrap().as_str().to_string();
                let alert_seq = *alert_seq_arc.lock().unwrap();
                let now       = std::time::Instant::now();

                // Alert event (new alert pushed to history)
                if alert_seq != last_alert_seq {
                    let summary = last_alert_arc.lock().unwrap()
                        .as_ref()
                        .map(|(_, s)| s.replace('"', "\'").replace('\n', " "))
                        .unwrap_or_default();
                    let event = format!(
                        "event: alert\ndata: {{\"seq\":{alert_seq},\"summary\":\"{summary}\"}}\n\n"
                    );
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (seq, last_seq, last_hb, last_mode, mode_arc, frozen_arc,
                         alert_seq_arc, last_alert_arc, alert_seq),
                    ));
                }

                // Frame event
                if cur != last_seq {
                    let frozen = *frozen_arc.lock().unwrap();
                    let event = format!(
                        "event: frame\ndata: {{\"seq\":{cur},\"frozen\":{frozen}}}\n\n"
                    );
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (seq, cur, last_hb, last_mode, mode_arc, frozen_arc,
                         alert_seq_arc, last_alert_arc, last_alert_seq),
                    ));
                }

                // Mode change event
                if mode != last_mode {
                    let event = format!("event: mode\ndata: {{\"mode\":\"{mode}\"}}\n\n");
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (seq, last_seq, last_hb, mode, mode_arc, frozen_arc,
                         alert_seq_arc, last_alert_arc, last_alert_seq),
                    ));
                }

                // Heartbeat every 15 s
                if now.duration_since(last_hb).as_secs() >= 15 {
                    return Some((
                        Ok::<_, actix_web::Error>(
                            actix_web::web::Bytes::from_static(b": heartbeat\n\n")
                        ),
                        (seq, last_seq, now, last_mode, mode_arc, frozen_arc,
                         alert_seq_arc, last_alert_arc, last_alert_seq),
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

    // ── Outbound forwarding (fire-and-forget) ────────────────────────────
    if let Some(fwd_url) = state.webhook_forward_url.lock().unwrap().clone() {
        let fwd_body = v.clone();
        tokio::spawn(async move {
            match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
            {
                Ok(client) => {
                    match client.post(&fwd_url).json(&fwd_body).send().await {
                        Ok(resp) => log::info!("webhook forwarded to {} — status {}", fwd_url, resp.status()),
                        Err(e)   => log::warn!("webhook forward to {} failed: {}", fwd_url, e),
                    }
                }
                Err(e) => log::warn!("webhook forward: could not build HTTP client: {}", e),
            }
        });
    }

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

// ─── Test pattern ─────────────────────────────────────────────────────────────

/// POST /api/test — display a SMPTE-style test card, then auto-return.
///
/// Optional body: `{ "duration_secs": 5 }` (default: 5 seconds)
async fn api_test_pattern(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let dur = body.get("duration_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 60);

    let quality    = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();

    let img  = render_test_card();
    let img  = apply_rotation(img, rotate_deg);
    let jpeg = match encode_for_panel(&img, quality) {
        Ok(j)  => j,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };

    { let cur = state.mode.lock().unwrap().clone(); if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; } }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap()         = DisplayMode::CustomImage;

    // Schedule auto-return
    let state_clone = state.get_ref().clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(dur)).await;
        let prev = state_clone.prev_mode.lock().unwrap().clone();
        *state_clone.mode.lock().unwrap()         = prev;
        *state_clone.pending_jpeg.lock().unwrap() = None;
        log::info!("test pattern: auto-returned after {}s", dur);
    });

    HttpResponse::Ok().body(format!("test pattern: showing for {}s", dur))
}

// ─── Slideshow image list + shuffle ──────────────────────────────────────────

/// GET /api/slideshow/images — list the paths currently loaded in the slideshow.
async fn api_slideshow_images(state: web::Data<DaemonState>) -> impl Responder {
    let paths = state.slideshow_paths.lock().unwrap();
    let names: Vec<String> = paths.iter()
        .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string())
        .collect();
    #[derive(Serialize)]
    struct SlideshowImageList {
        count:   usize,
        current: usize,
        images:  Vec<String>,
    }
    HttpResponse::Ok().json(SlideshowImageList {
        count:   names.len(),
        current: *state.slideshow_idx.lock().unwrap(),
        images:  names,
    })
}

/// POST /api/slideshow/shuffle — randomise the slideshow order.
async fn api_slideshow_shuffle(state: web::Data<DaemonState>) -> impl Responder {
    let mut paths = state.slideshow_paths.lock().unwrap();
    if paths.is_empty() { return HttpResponse::BadRequest().body("no slideshow loaded"); }
    // Fisher-Yates shuffle using SystemTime as entropy
    let n = paths.len();
    for i in (1..n).rev() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(i);
        let j = ts % (i + 1);
        paths.swap(i, j);
    }
    *state.slideshow_idx.lock().unwrap() = 0;
    HttpResponse::Ok().body(format!("shuffled {} images", n))
}

// ─── Device info ──────────────────────────────────────────────────────────────

/// GET /api/info — return USB device information (serial, firmware).
/// Returns null/null in headless mode (no USB device attached).
async fn api_info(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct DeviceInfo {
        version:       &'static str,
        vendor_id:     String,
        product_id:    String,
        headless:      bool,
        os:            String,
        arch:          String,
        mode_count:    usize,
        build_profile: &'static str,
    }
    HttpResponse::Ok().json(DeviceInfo {
        version:       env!("CARGO_PKG_VERSION"),
        vendor_id:     format!("0x{:04x}", state.vid),
        product_id:    format!("0x{:04x}", state.pid),
        headless:      state.headless,
        os:            std::env::consts::OS.into(),
        arch:          std::env::consts::ARCH.into(),
        mode_count:    DisplayMode::ALL_NAMES.len(),
        build_profile: if cfg!(debug_assertions) { "debug" } else { "release" },
    })
}

// ─── Preset export / import ──────────────────────────────────────────────────

/// GET /api/presets/export — download all presets as a JSON file.
async fn api_presets_export(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.presets.lock().unwrap();
    match serde_json::to_string_pretty(&*guard) {
        Ok(json) => HttpResponse::Ok()
            .content_type("application/json")
            .insert_header(("Content-Disposition", "attachment; filename=ttlcd-presets.json"))
            .body(json),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

/// POST /api/presets/import — import presets from a JSON body.
///
/// The body must be a JSON object mapping preset names to CustomTextPayload objects.
/// Existing presets with the same name are overwritten.
async fn api_presets_import(
    state: web::Data<DaemonState>,
    body:  web::Json<HashMap<String, CustomTextPayload>>,
) -> impl Responder {
    let incoming = body.into_inner();
    let count = incoming.len();
    if count == 0 {
        return HttpResponse::BadRequest().body("no presets in import body");
    }
    {
        let mut guard = state.presets.lock().unwrap();
        for (name, payload) in incoming {
            guard.insert(name, payload);
        }
        save_presets_to_disk(&guard);
    }
    log::info!("imported {} preset(s)", count);
    HttpResponse::Ok().body(format!("imported {} preset(s)", count))
}

// ─── System data JSON endpoints ───────────────────────────────────────────────

/// GET /api/processes?limit=N — return top N processes by CPU usage.
async fn api_processes_json(
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    use sysinfo::{ProcessesToUpdate, System};
    let limit: usize = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10).min(50);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    #[derive(Serialize)]
    struct ProcessInfo {
        pid:     u32,
        name:    String,
        cpu_pct: f32,
        mem_kb:  u64,
        status:  String,
    }

    let mut procs: Vec<ProcessInfo> = sys.processes().values().map(|p| ProcessInfo {
        pid:     p.pid().as_u32(),
        name:    p.name().to_string_lossy().to_string(),
        cpu_pct: p.cpu_usage(),
        mem_kb:  p.memory() / 1024,
        status:  format!("{:?}", p.status()),
    }).collect();

    procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(limit);

    HttpResponse::Ok().json(procs)
}

/// GET /api/disks — return all disk partition usage info.
async fn api_disks_json() -> impl Responder {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();

    #[derive(Serialize)]
    struct DiskInfo {
        name:       String,
        mount:      String,
        fs:         String,
        total_gb:   f64,
        used_gb:    f64,
        free_gb:    f64,
        used_pct:   f64,
        removable:  bool,
    }

    let info: Vec<DiskInfo> = disks.iter().map(|d| {
        let total = d.total_space();
        let free  = d.available_space();
        let used  = total.saturating_sub(free);
        DiskInfo {
            name:      d.name().to_string_lossy().to_string(),
            mount:     d.mount_point().to_string_lossy().to_string(),
            fs:        d.file_system().to_string_lossy().to_string(),
            total_gb:  total as f64 / 1_073_741_824.0,
            used_gb:   used  as f64 / 1_073_741_824.0,
            free_gb:   free  as f64 / 1_073_741_824.0,
            used_pct:  if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 },
            removable: d.is_removable(),
        }
    }).collect();

    HttpResponse::Ok().json(info)
}

// ─── Brightness control ──────────────────────────────────────────────────────

/// GET /api/brightness — return current brightness as JSON.
async fn api_get_brightness(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct BrightnessResp { brightness: f32 }
    HttpResponse::Ok().json(BrightnessResp {
        brightness: *state.dim_factor.lock().unwrap(),
    })
}

#[derive(Deserialize)]
struct BrightnessRequest {
    /// Brightness fraction 0.0 (off) to 1.0 (full).  Also accepts 0–100 as a
    /// convenience — values > 1 are treated as a percentage and divided by 100.
    brightness: f32,
}

/// POST /api/brightness — set display brightness (0.0–1.0 or 0–100).
async fn api_set_brightness(state: web::Data<DaemonState>, body: web::Json<BrightnessRequest>) -> impl Responder {
    let raw = body.brightness;
    let factor = if raw > 1.0 { (raw / 100.0).clamp(0.0, 1.0) } else { raw.clamp(0.0, 1.0) };
    *state.dim_factor.lock().unwrap() = factor;
    log::info!("brightness set to {:.2}", factor);
    persist_state(&state);
    HttpResponse::Ok().body(format!("{:.2}", factor))
}

// ─── Countdown cancel ─────────────────────────────────────────────────────────

/// DELETE /api/countdown — cancel the running countdown and return to the
/// previous live mode.
async fn api_cancel_countdown(state: web::Data<DaemonState>) -> impl Responder {
    let was_active = state.countdown.lock().unwrap().is_some();
    *state.countdown.lock().unwrap() = None;
    // Only revert if currently showing countdown
    if *state.mode.lock().unwrap() == DisplayMode::Countdown {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    if was_active {
        log::info!("countdown cancelled");
        HttpResponse::Ok().body("countdown cancelled")
    } else {
        HttpResponse::Ok().body("no active countdown")
    }
}

// ─── Network interface JSON ───────────────────────────────────────────────────

/// GET /api/network — return all network interfaces with Rx/Tx byte totals
/// and per-second rates (calculated over a ~1-second window).
async fn api_network_json() -> impl Responder {
    use sysinfo::Networks;

    #[derive(Serialize)]
    struct IfaceInfo {
        name:      String,
        rx_bytes:  u64,
        tx_bytes:  u64,
        rx_per_sec: u64,
        tx_per_sec: u64,
        mac:       String,
    }

    // Sample 1 — capture initial bytes
    let nets1 = Networks::new_with_refreshed_list();
    let t0 = std::time::Instant::now();
    let snap1: Vec<(String, u64, u64)> = nets1.iter()
        .map(|(name, n)| (name.clone(), n.total_received(), n.total_transmitted()))
        .collect();

    // Sleep ~1 s to compute per-second rates
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    let mut nets2 = Networks::new_with_refreshed_list();
    nets2.refresh(false);
    let elapsed = t0.elapsed().as_secs_f64().max(0.001);

    let ifaces: Vec<IfaceInfo> = nets2.iter().map(|(name, n)| {
        let rx_bytes = n.total_received();
        let tx_bytes = n.total_transmitted();
        let (rx0, tx0) = snap1.iter()
            .find(|(nm, ..)| nm == name)
            .map(|(_, rx, tx)| (*rx, *tx))
            .unwrap_or((rx_bytes, tx_bytes));
        let rx_per_sec = ((rx_bytes.saturating_sub(rx0)) as f64 / elapsed) as u64;
        let tx_per_sec = ((tx_bytes.saturating_sub(tx0)) as f64 / elapsed) as u64;
        IfaceInfo {
            name:      name.clone(),
            rx_bytes,
            tx_bytes,
            rx_per_sec,
            tx_per_sec,
            mac:       n.mac_address().to_string(),
        }
    }).collect();

    HttpResponse::Ok().json(ifaces)
}

// ─── Temperature sensor JSON ──────────────────────────────────────────────────

/// GET /api/temperatures — return all thermal sensor readings.
async fn api_temperatures_json() -> impl Responder {
    use sysinfo::Components;

    #[derive(Serialize)]
    struct SensorInfo {
        label:      String,
        current:    f32,
        high:       Option<f32>,
        critical:   Option<f32>,
    }

    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);

    let sensors: Vec<SensorInfo> = comps.iter()
        .filter_map(|c| {
            let temp = c.temperature()?;
            if temp.is_nan() || temp <= 0.0 { return None; }
            Some(SensorInfo {
                label:    c.label().to_string(),
                current:  temp,
                high:     c.max(),
                critical: c.critical(),
            })
        })
        .collect();

    HttpResponse::Ok().json(sensors)
}

// ─── CPU & net history snapshots ─────────────────────────────────────────────

/// GET /api/cpu/history — return the last 120 CPU usage samples (% 0–100).
async fn api_cpu_history() -> impl Responder {
    #[derive(Serialize)]
    struct CpuHistoryResp {
        /// Samples, oldest first. Up to 120 values.
        samples:  Vec<f32>,
        /// Number of samples in the buffer.
        count:    usize,
        /// Maximum capacity of the ring-buffer.
        capacity: usize,
    }
    let samples = get_cpu_history();
    let count   = samples.len();
    HttpResponse::Ok().json(CpuHistoryResp { samples, count, capacity: 120 })
}

/// GET /api/net/history — return the last 60 network Rx/Tx samples (bytes/sec).
async fn api_net_history() -> impl Responder {
    #[derive(Serialize)]
    struct NetHistoryResp {
        rx_bps:   Vec<u64>,
        tx_bps:   Vec<u64>,
        count:    usize,
        capacity: usize,
    }
    let (rx, tx) = get_net_history();
    let count    = rx.len();
    HttpResponse::Ok().json(NetHistoryResp { rx_bps: rx, tx_bps: tx, count, capacity: 60 })
}

// ─── PNG screenshot ───────────────────────────────────────────────────────────

/// GET /api/screenshot.png — return the current panel frame losslessly as PNG.
///
/// Useful for embedding in dashboards that require lossless images.  Note that
/// this decodes and re-encodes the stored JPEG so quality may not exceed the
/// JPEG encoder's original quality.
async fn api_screenshot_png(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        None => HttpResponse::NoContent().insert_header(("Cache-Control", "no-store")).finish(),
        Some(jpeg) => {
            match image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg) {
                Ok(img) => {
                    let mut buf = Vec::new();
                    match img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png) {
                        Ok(_) => HttpResponse::Ok()
                            .insert_header(("Cache-Control", "no-store"))
                            .insert_header(("Content-Disposition", "attachment; filename=panel.png"))
                            .content_type("image/png")
                            .body(buf),
                        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
                    }
                }
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            }
        }
    }
}

// ─── GET /api/rotation ────────────────────────────────────────────────────────

/// GET /api/rotation — return the current auto-rotation configuration.
async fn api_rotation_get(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct RotationConfig {
        modes:    Vec<String>,
        interval: u64,
        enabled:  bool,
    }
    let modes: Vec<String> = state.rotation_modes.lock().unwrap()
        .iter().map(|m| m.as_str().to_string()).collect();
    let interval = *state.rotation_interval.lock().unwrap();
    HttpResponse::Ok().json(RotationConfig {
        enabled: interval > 0 && !modes.is_empty(),
        modes,
        interval,
    })
}

// ─── SVG status badge ────────────────────────────────────────────────────────

/// GET /api/status.svg — return a Shields.io-style SVG badge showing the
/// current mode.  Suitable for embedding in dashboards and GitHub README files.
///
/// Example: `<img src="http://localhost:7070/api/status.svg">`
async fn api_status_badge(state: web::Data<DaemonState>) -> impl Responder {
    let mode    = state.mode.lock().unwrap().as_str().to_string();
    let quality = *state.quality.lock().unwrap();
    let headless= state.headless;
    let brightness = *state.dim_factor.lock().unwrap();

    // Pick badge colour based on mode category
    let badge_color = match mode.as_str() {
        "clock" | "clock_sysinfo"               => "#89b4fa",  // blue
        "sysinfo" | "cpu_detail" | "cpu_graph"  => "#cba6f7",  // mauve
        "network" | "net_graph"                 => "#94e2d5",  // teal
        "disk_usage" | "disk_io"                => "#f9e2af",  // yellow
        "process_top" | "process_memory"        => "#a6e3a1",  // green
        "temperature" | "temp_graph"            => "#fab387",  // peach
        "ping_graph"                           => "#a6e3a1",  // green
        "game_of_life"                         => "#a6e3a1",  // green
        "led_clock"                            => "#fab387",  // peach
        "bounce_ball"                          => "#cba6f7",  // mauve
        "speedometer_gauge"                    => "#89dceb",  // sky
        "disk_pie"                             => "#a6e3a1",  // green
        "thermal_dashboard"                    => "#fab387",  // peach
        "plasma"                               => "#cba6f7",  // mauve
        "fireworks"                            => "#f38ba8",  // red
        "cpu_heatmap"                          => "#a6e3a1",  // green
        "status_overview"                      => "#89dceb",  // sky
        "analog_clock"                         => "#cba6f7",  // mauve
        "net_speed_graph"                      => "#a6e3a1",  // green
        "cpu_core_bars"                        => "#cba6f7",  // mauve
        "disk_io_graph" | "graphs_overview"     => "#f9e2af",  // yellow
        "countdown"                             => "#eba0ac",  // maroon
        "recent_alerts"                         => "#f38ba8",  // red
        "idle"                                  => "#585b70",  // surface2
        _                                       => "#89dceb",  // sky
    };

    let sub = if headless { "headless".to_string() }
              else if brightness < 0.99 { format!("{}% bright", (brightness * 100.0) as u8) }
              else { format!("q{}", quality) };

    // Simple flat badge SVG (compatible with most browsers and README renderers)
    let mode_disp = mode.replace('_', " ");
    let left_w   = 48i32;
    let right_w  = (mode_disp.len() as i32 * 7).max(60);
    let total_w  = left_w + right_w;
    let _sub_w   = (sub.len() as i32 * 6).max(30);

    let lx  = left_w / 2;
    let rx  = left_w + right_w / 2;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{tw}" height="20"><defs><linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient></defs><rect rx="3" width="{tw}" height="20" fill="#555"/><rect rx="3" x="{lw}" width="{rw}" height="20" fill="{bc}"/><rect x="{lw}" width="4" height="20" fill="{bc}"/><rect rx="3" width="{tw}" height="20" fill="url(#s)"/><g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="11"><text x="{lx}" y="15" fill="#010101" fill-opacity=".3">ttlcd</text><text x="{lx}" y="14">ttlcd</text><text x="{rx}" y="15" fill="#010101" fill-opacity=".3">{md}</text><text x="{rx}" y="14" fill="#1e1e2e">{md}</text></g><title>ttlcd — {mode} | {sub}</title></svg>"##,
        tw=total_w, lw=left_w, rw=right_w, bc=badge_color,
        lx=lx, rx=rx, md=mode_disp, mode=mode, sub=sub
    );

    HttpResponse::Ok()
        .content_type("image/svg+xml")
        .insert_header(("Cache-Control", "no-cache, max-age=0"))
        .body(svg)
}

// ─── Version / uptime ───────────────────────────────────────────────────────

/// GET /api/version — daemon version info.
///
/// Returns: `{"version": "0.6.0", "name": "ttlcd"}`
async fn api_version() -> impl Responder {
    #[derive(serde::Serialize)]
    struct VersionInfo {
        name:    &'static str,
        version: &'static str,
    }
    HttpResponse::Ok().json(VersionInfo {
        name:    env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// GET /api/uptime — seconds and human-readable string since daemon start.
///
/// Returns: `{"uptime_secs": 3600, "uptime_human": "1h 0m 0s"}`
async fn api_uptime(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(serde::Serialize)]
    struct UptimeResp { uptime_secs: u64, uptime_human: String }
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let secs = now_s.saturating_sub(state.daemon_start_secs);
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let human = if d > 0 {
        format!("{d}d {h}h {m}m {s}s")
    } else if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    };
    HttpResponse::Ok().json(UptimeResp { uptime_secs: secs, uptime_human: human })
}

/// GET /api/panel/info — hardware info for the connected USB panel.
///
/// Returns null serial/firmware when running in headless mode.
async fn api_panel_info(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(serde::Serialize)]
    struct PanelInfoResp {
        connected:     bool,
        headless:      bool,
        vid:           String,
        pid:           String,
        serial:        Option<String>,
        firmware_byte: Option<u8>,
        status_byte:   Option<u8>,
        width:         u32,
        height:        u32,
    }
    let info = state.panel_info.lock().unwrap().clone();
    HttpResponse::Ok().json(PanelInfoResp {
        connected:     info.is_some(),
        headless:      state.headless,
        vid:           format!("{:04x}", state.vid),
        pid:           format!("{:04x}", state.pid),
        serial:        info.as_ref().map(|i| i.serial.clone()),
        firmware_byte: info.as_ref().map(|i| i.firmware_byte),
        status_byte:   info.as_ref().map(|i| i.status_byte),
        width:         PANEL_WIDTH,
        height:        PANEL_HEIGHT,
    })
}

// ─── Panel reconnect endpoint ────────────────────────────────────────────────

/// POST /api/panel/reconnect — trigger an immediate USB reconnect attempt.
///
/// This does not interrupt the display loop; it simply clears the consecutive_errors
/// count by signalling a reconnect intent through panel_connected=false so the
/// display loop's next iteration will attempt to reopen the device.
///
/// In headless mode this is a no-op.
async fn api_panel_reconnect(state: web::Data<DaemonState>) -> impl Responder {
    if state.headless {
        return HttpResponse::Ok().json(serde_json::json!({
            "ok": true,
            "message": "headless mode — no USB device to reconnect"
        }));
    }
    // Force panel_connected=false so the display loop retries on next tick
    *state.panel_connected.lock().unwrap() = false;
    push_history(&state, HistoryKind::System, "panel reconnect requested via API".to_string());
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "reconnect attempt scheduled — check /api/panel/info in a few seconds"
    }))
}

// ─── Banner endpoint ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BannerRequest {
    /// Up to 3 lines of text to display large and centred.
    lines:     Vec<String>,
    /// Optional subtitle label at the bottom.
    #[serde(default)]
    label:     String,
    /// Accent colour name (blue/green/yellow/red/teal/mauve/peach/white).
    #[serde(default = "default_banner_color")]
    color:     String,
    /// Show clock in the top-right corner (default true).
    #[serde(default = "default_true")]
    show_time: bool,
    /// Auto-clear after N seconds (0 = keep until cleared).
    #[serde(default)]
    duration:  u64,
}

fn default_banner_color() -> String { "blue".to_string() }

/// POST /api/banner — display up to 3 lines of large centred text on the panel.
///
/// ```json
/// {"lines": ["99.8%", "Uptime"], "label": "SLA", "color": "green"}
/// ```
async fn api_banner(
    state: web::Data<DaemonState>,
    body:  web::Json<BannerRequest>,
) -> impl Responder {
    use ttlcd::display::NamedColor;
    let quality   = *state.quality.lock().unwrap();
    let rotate    = *state.rotate_deg.lock().unwrap();
    let brightness= *state.dim_factor.lock().unwrap();
    let named = NamedColor::from_str_loose(&body.color);
    let rgb = named.to_rgb_pub();
    let refs: Vec<&str> = body.lines.iter().map(String::as_str).take(3).collect();
    let img = render_big_text_banner(&refs, &body.label, rgb, body.show_time);
    let img = apply_rotation(img, rotate);
    let img = apply_brightness(img, brightness);
    let jpeg = match encode_for_panel(&img, quality) {
        Ok(j)  => j,
        Err(e) => return HttpResponse::InternalServerError().body(format!("encode error: {e}")),
    };
    let jpeg = store_frame(&state, jpeg.clone());
    *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    if body.duration > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        *state.auto_clear_at.lock().unwrap() = Some(now + body.duration);
    }
    push_history(&state, HistoryKind::Text,
        format!("banner: {}", body.lines.first().map(|s| s.as_str()).unwrap_or("")));
    HttpResponse::Ok().body("ok")
}

// ─── Modes info ──────────────────────────────────────────────────────────────

/// GET /api/modes/info — return all modes with name and description.
async fn api_modes_info() -> impl Responder {
    #[derive(serde::Serialize)]
    struct ModeInfo { name: &'static str, description: &'static str }
    let modes: &[ModeInfo] = &[
        ModeInfo { name: "clock",          description: "Large digital clock with date and uptime" },
        ModeInfo { name: "big_clock",      description: "Full-screen giant HH:MM with large seconds and progress bar" },
        ModeInfo { name: "clock_sysinfo",  description: "Split: clock + quick stats" },
        ModeInfo { name: "sysinfo",        description: "CPU, RAM, temperature, load, uptime" },
        ModeInfo { name: "cpu_detail",     description: "Per-core CPU usage grid (up to 32 cores)" },
        ModeInfo { name: "network",        description: "Per-interface Rx/Tx rates and totals" },
        ModeInfo { name: "disk_usage",     description: "Disk partition usage bars" },
        ModeInfo { name: "process_top",    description: "Top processes by CPU usage" },
        ModeInfo { name: "temperature",    description: "Thermal sensor readings with colour-coded bars" },
        ModeInfo { name: "memory_detail",  description: "Detailed RAM/swap breakdown" },
        ModeInfo { name: "countdown",      description: "Countdown timer with colour transitions" },
        ModeInfo { name: "cpu_graph",      description: "CPU usage history bar chart (2 min)" },
        ModeInfo { name: "net_graph",      description: "Network I/O history chart (60 s)" },
        ModeInfo { name: "recent_alerts",  description: "Last 5 notifications as on-panel log" },
        ModeInfo { name: "process_memory", description: "Top processes ranked by RAM usage" },
        ModeInfo { name: "disk_io",        description: "Disk read/write throughput per device" },
        ModeInfo { name: "calendar",       description: "Full-month calendar with today highlighted" },
        ModeInfo { name: "stopwatch",      description: "Running stopwatch with lap times" },
        ModeInfo { name: "metrics",        description: "Custom key/value metrics dashboard (up to 8 items)" },
        ModeInfo { name: "weather",        description: "Weather snapshot — push via /api/weather or web UI" },
        ModeInfo { name: "notes",          description: "Bulletin board — push a list of text items via /api/notes" },
        ModeInfo { name: "qr_code",         description: "Display a scannable QR code — push payload via POST /api/qr" },
        ModeInfo { name: "pomodoro",        description: "Pomodoro focus timer: 25 min work / 5 min break cycles with visual progress ring" },
        ModeInfo { name: "battery",        description: "Battery level, charging status, voltage and estimated time remaining (Linux laptops)" },
        ModeInfo { name: "world_clock",    description: "World clock showing up to 4 cities with configurable UTC offsets and day/night coloring" },
        ModeInfo { name: "ticker",         description: "Horizontally scrolling text ticker" },
        ModeInfo { name: "http_status",    description: "HTTP health monitor dashboard — configure endpoints via /api/http_status" },
        ModeInfo { name: "shell_cmd",      description: "Shell command output viewer — set command via POST /api/shell_cmd" },
        ModeInfo { name: "ping_monitor",   description: "ICMP ping latency dashboard — configure hosts via /api/ping_monitor" },
        ModeInfo { name: "dashboard",      description: "Four-quadrant overview: clock, CPU/RAM, network, temperature" },
        ModeInfo { name: "log_tail",       description: "Live log tail viewer — configure file via POST /api/log_tail" },
        ModeInfo { name: "cpu_freq",       description: "Per-core CPU clock frequency grid (reads /proc/cpuinfo)" },
        ModeInfo { name: "idle",           description: "Keepalive — no display update" },
    ];
    HttpResponse::Ok().json(modes)
}

// ─── State reload ────────────────────────────────────────────────────────────

/// POST /api/state/reload — re-read the persisted daemon state from disk and
/// apply quality, rotate_deg, and brightness.  Mode is NOT changed (to avoid
/// interrupting an active session).
async fn api_state_reload(state: web::Data<DaemonState>) -> impl Responder {
    let ps = load_persisted_state();
    let mut changed: Vec<&str> = Vec::new();
    if let Some(q) = ps.quality {
        *state.quality.lock().unwrap() = q;
        changed.push("quality");
    }
    if let Some(r) = ps.rotate_deg {
        *state.rotate_deg.lock().unwrap() = r;
        changed.push("rotate_deg");
    }
    if let Some(b) = ps.brightness {
        *state.dim_factor.lock().unwrap() = b;
        changed.push("brightness");
    }
    if changed.is_empty() {
        HttpResponse::Ok().body("no persisted state found")
    } else {
        log::info!("reloaded persisted state: {} field(s) applied", changed.len());
        HttpResponse::Ok().body(format!("reloaded: {}", changed.join(", ")))
    }
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

// ─── Stopwatch API ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StopwatchResponse {
    running:    bool,
    elapsed_ms: u64,
    /// Accumulated milliseconds before the current running period.
    /// When running: current_elapsed = elapsed_ms + (now_ms - started_at_unix_ms)
    base_elapsed_ms:    u64,
    /// Unix-millisecond timestamp when the stopwatch was last started (0 = not running).
    started_at_unix_ms: u64,
    laps:       Vec<u64>,
    display:    String,
    lap_count:  usize,
}

/// GET /api/stopwatch — current stopwatch state.
async fn api_stopwatch_get(state: web::Data<DaemonState>) -> impl Responder {
    let sw = state.stopwatch.lock().unwrap();
    let elapsed = sw.current_elapsed_ms();
    let lap_count = sw.laps.len();
    let display = ttlcd::display::format_sw_time(elapsed);
    HttpResponse::Ok().json(StopwatchResponse {
        running:    sw.running,
        elapsed_ms: elapsed,
        base_elapsed_ms:    sw.elapsed_ms,
        started_at_unix_ms: sw.started_at_unix_ms,
        laps:       sw.laps.clone(),
        display,
        lap_count,
    })
}

/// POST /api/stopwatch/start — start or resume the stopwatch.
async fn api_stopwatch_start(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    if !sw.running {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        sw.started_at_unix_ms = now_ms;
        sw.running = true;
    }
    drop(sw);
    // Switch to stopwatch mode
    *state.mode.lock().unwrap() = DisplayMode::Stopwatch;
    *state.prev_mode.lock().unwrap() = DisplayMode::Stopwatch;
    HttpResponse::Ok().body("started")
}

/// POST /api/stopwatch/stop — pause the stopwatch.
async fn api_stopwatch_stop(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    if sw.running {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        sw.elapsed_ms += now_ms.saturating_sub(sw.started_at_unix_ms);
        sw.running = false;
        sw.started_at_unix_ms = 0;
    }
    HttpResponse::Ok().body("stopped")
}

/// POST /api/stopwatch/lap — record a lap at the current elapsed time.
async fn api_stopwatch_lap(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    let elapsed = sw.current_elapsed_ms();
    if elapsed > 0 {
        sw.laps.push(elapsed);
        let n = sw.laps.len();
        drop(sw);
        HttpResponse::Ok().json(serde_json::json!({ "lap": n, "elapsed_ms": elapsed }))
    } else {
        HttpResponse::BadRequest().body("stopwatch not started")
    }
}

/// POST /api/stopwatch/reset — stop and zero the stopwatch.
async fn api_stopwatch_reset(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    *sw = StopwatchState::new();
    HttpResponse::Ok().body("reset")
}

// ─── Display freeze / unfreeze ────────────────────────────────────────────────

/// POST /api/display/freeze — stop the display from updating (last frame stays).
async fn api_display_freeze(state: web::Data<DaemonState>) -> impl Responder {
    *state.frozen.lock().unwrap() = true;
    HttpResponse::Ok().body("frozen")
}

/// POST /api/display/unfreeze — resume normal display updates.
async fn api_display_unfreeze(state: web::Data<DaemonState>) -> impl Responder {
    *state.frozen.lock().unwrap() = false;
    HttpResponse::Ok().body("unfrozen")
}

/// GET /api/display/frozen — returns {"frozen":true|false}.
async fn api_display_frozen(state: web::Data<DaemonState>) -> impl Responder {
    let frozen = *state.frozen.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({ "frozen": frozen }))
}

// ─── Alert thresholds ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetThresholdsRequest {
    cpu_pct:       Option<f32>,
    temp_c:        Option<f32>,
    cooldown_secs: Option<u64>,
}

/// GET /api/alerts/thresholds — current threshold configuration.
async fn api_get_thresholds(state: web::Data<DaemonState>) -> impl Responder {
    let t = state.alert_thresholds.lock().unwrap().clone();
    HttpResponse::Ok().json(&t)
}

/// POST /api/alerts/thresholds — update one or more threshold values.
/// Any omitted fields keep their current value.
async fn api_set_thresholds(
    state: web::Data<DaemonState>,
    body:  web::Json<SetThresholdsRequest>,
) -> impl Responder {
    let mut t = state.alert_thresholds.lock().unwrap();
    if let Some(v) = body.cpu_pct       { t.cpu_pct       = v.clamp(0.0, 100.0); }
    if let Some(v) = body.temp_c        { t.temp_c        = v.clamp(0.0, 200.0); }
    if let Some(v) = body.cooldown_secs { t.cooldown_secs = v; }
    HttpResponse::Ok().json(&*t)
}

// ─── Night-dim schedule ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetDimScheduleRequest {
    start_hour: u8,
    end_hour:   u8,
    #[serde(default = "default_dim_level")]
    dim_level:  f32,
}
fn default_dim_level() -> f32 { 0.2 }

/// GET /api/dim/schedule — current dim schedule (null if not set).
async fn api_get_dim_schedule(state: web::Data<DaemonState>) -> impl Responder {
    let s = state.dim_schedule.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "schedule": s }))
}

/// POST /api/dim/schedule — set a night-dim schedule.
/// Example: {"start_hour":22,"end_hour":7,"dim_level":0.15}
async fn api_set_dim_schedule(
    state: web::Data<DaemonState>,
    body:  web::Json<SetDimScheduleRequest>,
) -> impl Responder {
    let sched = DimSchedule {
        start_hour: body.start_hour % 24,
        end_hour:   body.end_hour   % 24,
        dim_level:  body.dim_level.clamp(0.0, 1.0),
    };
    *state.dim_schedule.lock().unwrap() = Some(sched.clone());
    log::info!("dim schedule set: {}:00 → {}:00 @ {:.0}%", sched.start_hour, sched.end_hour, sched.dim_level * 100.0);
    HttpResponse::Ok().json(serde_json::json!({ "schedule": sched }))
}

/// DELETE /api/dim/schedule — remove the dim schedule.
async fn api_clear_dim_schedule(state: web::Data<DaemonState>) -> impl Responder {
    *state.dim_schedule.lock().unwrap() = None;
    HttpResponse::Ok().body("schedule cleared")
}

// ─── Custom metrics dashboard ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct PushMetricsRequest {
    /// Optional dashboard title (shown at top of panel).
    #[serde(default)]
    title:   String,
    /// Key/value metrics to display (up to 8 shown; extras ignored).
    metrics: Vec<Metric>,
    /// If true, switch the display to Metrics mode immediately.
    #[serde(default = "default_true_metrics")]
    show:    bool,
}
fn default_true_metrics() -> bool { true }

/// POST /api/metrics — push new metric values and optionally switch to Metrics mode.
///
/// Example body:
/// ```json
/// { "title": "CI Dashboard",
///   "metrics": [
///     { "label": "Build", "value": "OK",     "color": "green" },
///     { "label": "Tests", "value": "1234",   "color": "blue"  },
///     { "label": "Cover", "value": "92.3%",  "color": "teal"  }
///   ]
/// }
/// ```
async fn api_push_metrics(
    state: web::Data<DaemonState>,
    body:  web::Json<PushMetricsRequest>,
) -> impl Responder {
    let req = body.into_inner();
    *state.metrics_title.lock().unwrap() = req.title;
    *state.metrics_data.lock().unwrap()  = req.metrics;
    if req.show {
        let prev = state.mode.lock().unwrap().clone();
        if !prev.is_transient() {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::Metrics;
    }
    HttpResponse::Ok().json(serde_json::json!({
        "count": state.metrics_data.lock().unwrap().len()
    }))
}

/// GET /api/metrics — return current metric data.
async fn api_get_metrics(state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "title":   *state.metrics_title.lock().unwrap(),
        "metrics": *state.metrics_data.lock().unwrap(),
    }))
}

/// DELETE /api/metrics — clear all metric data.
async fn api_clear_metrics(state: web::Data<DaemonState>) -> impl Responder {
    state.metrics_title.lock().unwrap().clear();
    state.metrics_data.lock().unwrap().clear();
    HttpResponse::Ok().body("cleared")
}


// ─── HTTP JSON Metrics Source API ────────────────────────────────────────────

/// GET /api/metrics/source — return current metrics source configuration.
async fn api_get_metrics_source(state: web::Data<DaemonState>) -> impl Responder {
    let src = state.metrics_source.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "source": src }))
}

/// POST /api/metrics/source — configure an HTTP JSON metrics source and enable auto-fetch.
///
/// Body: `{"url":"https://…","title":"My Server","interval_secs":30,
///        "fields":[{"label":"CPU","path":"cpu_pct","format":"{:.1}%","color":"green"}]}`
async fn api_set_metrics_source(
    state: web::Data<DaemonState>,
    body:  web::Json<MetricsSource>,
) -> impl Responder {
    let src = body.into_inner();
    if src.url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if src.fields.is_empty() {
        return HttpResponse::BadRequest().body("fields must not be empty");
    }
    let interval = src.interval_secs.max(5);
    let src_stored = MetricsSource { interval_secs: interval, ..src.clone() };
    log::info!("metrics_source configured: url='{}' interval={}s fields={}", src.url, interval, src.fields.len());
    *state.metrics_source.lock().unwrap() = Some(src_stored);
    save_new_settings(&state);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "url": src.url, "interval_secs": interval }))
}

/// DELETE /api/metrics/source — remove the metrics source (auto-fetch stops).
async fn api_clear_metrics_source(state: web::Data<DaemonState>) -> impl Responder {
    *state.metrics_source.lock().unwrap() = None;
    save_new_settings(&state);
    log::info!("metrics_source cleared");
    HttpResponse::Ok().body("metrics source cleared")
}

/// POST /api/metrics/source/fetch — trigger an immediate fetch from the configured source.
async fn api_fetch_metrics_source_now(state: web::Data<DaemonState>) -> impl Responder {
    let src = state.metrics_source.lock().unwrap().clone();
    match src {
        None => HttpResponse::BadRequest().body("no metrics source configured"),
        Some(s) => match fetch_metrics_source(&state, &s).await {
            Ok(()) => {
                // Optionally switch to metrics mode
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
                *state.mode.lock().unwrap() = DisplayMode::Metrics;
                HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": state.metrics_data.lock().unwrap().len() }))
            }
            Err(e) => HttpResponse::BadGateway().body(format!("fetch failed: {e}")),
        }
    }
}

// ─── Display profiles ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DisplayProfile {
    name:       String,
    mode:       String,
    quality:    u8,
    rotate_deg: u16,
    brightness: f32,
    #[serde(default)]
    rotation_modes:    Vec<String>,
    #[serde(default)]
    rotation_interval: u64,
}

fn profiles_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("profiles.json"))
}

fn load_profiles() -> Vec<DisplayProfile> {
    (|| -> Option<Vec<DisplayProfile>> {
        let path = profiles_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })().unwrap_or_default()
}

fn save_profiles(profiles: &[DisplayProfile]) {
    let Some(path) = profiles_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(profiles) {
        let _ = std::fs::write(&path, text);
    }
}

/// GET /api/profiles — list all saved display profiles.
async fn api_profiles_list(_state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(load_profiles())
}

/// POST /api/profiles — save current daemon state as a named profile.
/// Body: `{"name":"my-profile"}`
#[derive(Deserialize)]
struct SaveProfileRequest { name: String }

async fn api_profiles_save(
    state: web::Data<DaemonState>,
    body:  web::Json<SaveProfileRequest>,
) -> impl Responder {
    let profile = DisplayProfile {
        name:       body.name.clone(),
        mode:       state.mode.lock().unwrap().as_str().to_string(),
        quality:    *state.quality.lock().unwrap(),
        rotate_deg: *state.rotate_deg.lock().unwrap(),
        brightness: *state.dim_factor.lock().unwrap(),
        rotation_modes:    state.rotation_modes.lock().unwrap().iter().map(|m| m.as_str().to_string()).collect(),
        rotation_interval: *state.rotation_interval.lock().unwrap(),
    };
    let mut profiles = load_profiles();
    // Replace if name already exists, else append
    if let Some(existing) = profiles.iter_mut().find(|p| p.name == body.name) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }
    save_profiles(&profiles);
    log::info!("profile '{}' saved", body.name);
    HttpResponse::Ok().json(&profile)
}

/// POST /api/profiles/{name}/apply — load and apply a saved profile.
async fn api_profiles_apply(
    state: web::Data<DaemonState>,
    name:  web::Path<String>,
) -> impl Responder {
    let profiles = load_profiles();
    let Some(profile) = profiles.iter().find(|p| p.name == name.as_str()) else {
        return HttpResponse::NotFound().body(format!("profile '{}' not found", name));
    };
    // Apply settings
    if let Some(mode) = DisplayMode::from_str_name(&profile.mode) {
        *state.mode.lock().unwrap()      = mode.clone();
        *state.prev_mode.lock().unwrap() = mode;
    }
    *state.quality.lock().unwrap()    = profile.quality;
    *state.rotate_deg.lock().unwrap() = profile.rotate_deg;
    *state.dim_factor.lock().unwrap() = profile.brightness;
    if !profile.rotation_modes.is_empty() {
        let modes: Vec<DisplayMode> = profile.rotation_modes.iter()
            .filter_map(|s| DisplayMode::from_str_name(s))
            .collect();
        *state.rotation_modes.lock().unwrap()    = modes;
        *state.rotation_interval.lock().unwrap() = profile.rotation_interval;
    }
    persist_state(&state);
    log::info!("profile '{}' applied", name);
    HttpResponse::Ok().json(profile)
}

/// DELETE /api/profiles/{name} — delete a saved profile.
async fn api_profiles_delete(
    _state: web::Data<DaemonState>,
    name:   web::Path<String>,
) -> impl Responder {
    let mut profiles = load_profiles();
    let before = profiles.len();
    profiles.retain(|p| p.name != name.as_str());
    if profiles.len() < before {
        save_profiles(&profiles);
        HttpResponse::Ok().body(format!("deleted '{}'", name))
    } else {
        HttpResponse::NotFound().body(format!("profile '{}' not found", name))
    }
}


// ─── OS Info API ──────────────────────────────────────────────────────────────

/// GET /api/sys/os — return OS, kernel, CPU, and architecture information.
async fn api_sys_os() -> impl Responder {
    // OS release info
    let (os_name, os_version, os_pretty) = std::fs::read_to_string("/etc/os-release")
        .map(|text| {
            let mut name    = String::new();
            let mut version = String::new();
            let mut pretty  = String::new();
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("NAME=")         { name    = v.trim_matches('"').to_string(); }
                if let Some(v) = line.strip_prefix("VERSION_ID=")   { version = v.trim_matches('"').to_string(); }
                if let Some(v) = line.strip_prefix("PRETTY_NAME=")  { pretty  = v.trim_matches('"').to_string(); }
            }
            (name, version, pretty)
        })
        .unwrap_or_default();

    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    let arch = std::process::Command::new("uname")
        .arg("-m")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "x86_64".to_string());

    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_string();

    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("model name").map(|r| {
                r.trim_start_matches(|c: char| c == '\t' || c == ':' || c == ' ').to_string()
            }))
        .unwrap_or_else(|| "Unknown".to_string());

    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    let mem_total_kb: u64 = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:").and_then(|r|
            r.split_whitespace().next().and_then(|v| v.parse().ok())))
        .unwrap_or(0);

    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0) as u64;

    HttpResponse::Ok().json(serde_json::json!({
        "hostname":      hostname,
        "os_name":       os_name,
        "os_version":    os_version,
        "os_pretty":     os_pretty,
        "kernel":        kernel,
        "arch":          arch,
        "cpu_model":     cpu_model,
        "cpu_cores":     cpu_cores,
        "mem_total_mb":  mem_total_kb / 1024,
        "uptime_secs":   uptime_secs,
        "daemon_version": env!("CARGO_PKG_VERSION"),
    }))
}

// ─── Comprehensive system snapshot ───────────────────────────────────────────

/// GET /api/sys/snapshot — returns all system statistics in a single call.
/// Useful for dashboards that poll a single endpoint instead of many.
async fn api_sys_snapshot(state: web::Data<DaemonState>) -> impl Responder {
    use sysinfo::{System, Disks, Networks, Components};

    let mut sys = System::new_all();
    // Two-pass CPU sampling for accuracy
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_all();

    // CPU
    let cpu_pct  = sys.global_cpu_usage();
    let cpu_cores: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_name = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default();

    // Memory
    let mem_total = sys.total_memory();
    let mem_used  = sys.used_memory();
    let swap_total= sys.total_swap();
    let swap_used = sys.used_swap();

    // Load average
    let load = System::load_average();

    // Uptime
    let uptime = System::uptime();

    // Hostname
    let hostname = System::host_name().unwrap_or_default();

    // Kernel / OS
    let os_name    = System::name().unwrap_or_default();
    let kernel_ver = System::kernel_version().unwrap_or_default();

    // Processes (top 5 by CPU)
    let mut procs: Vec<serde_json::Value> = sys.processes().values()
        .map(|p| {
            serde_json::json!({
                "pid":     p.pid().as_u32(),
                "name":    p.name().to_string_lossy(),
                "cpu_pct": p.cpu_usage(),
                "mem_bytes": p.memory(),
                "status":  format!("{:?}", p.status()),
            })
        })
        .collect();
    procs.sort_by(|a, b| b["cpu_pct"].as_f64().unwrap_or(0.0)
        .partial_cmp(&a["cpu_pct"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(5);

    // Disks
    let disks = Disks::new_with_refreshed_list();
    let disk_list: Vec<serde_json::Value> = disks.iter()
        .map(|d| serde_json::json!({
            "mount":       d.mount_point().to_string_lossy(),
            "name":        d.name().to_string_lossy(),
            "total_bytes": d.total_space(),
            "used_bytes":  d.total_space() - d.available_space(),
            "free_bytes":  d.available_space(),
            "fs":          String::from_utf8_lossy(d.file_system().as_encoded_bytes()),
        }))
        .collect();

    // Network (1-sample deltas are zero without two passes; return totals)
    let nets = Networks::new_with_refreshed_list();
    let net_list: Vec<serde_json::Value> = nets.iter()
        .map(|(iface, data)| serde_json::json!({
            "interface":   iface,
            "total_rx":    data.total_received(),
            "total_tx":    data.total_transmitted(),
            "mac":         data.mac_address().to_string(),
        }))
        .collect();

    // Temperatures
    let components = Components::new_with_refreshed_list();
    let temp_list: Vec<serde_json::Value> = components.iter()
        .filter_map(|c| c.temperature().map(|t| serde_json::json!({
            "label": c.label(),
            "temp_c": t,
            "critical_c": c.critical(),
        })))
        .collect();

    // Daemon state
    let mode       = state.mode.lock().unwrap().as_str().to_string();
    let frozen     = *state.frozen.lock().unwrap();
    let brightness = *state.dim_factor.lock().unwrap();
    let fps        = *state.render_fps.lock().unwrap();
    let frame_seq  = *state.frame_seq.lock().unwrap();
    let uptime_secs = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        now.saturating_sub(state.daemon_start_secs)
    };

    HttpResponse::Ok().json(serde_json::json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0),
        "hostname":    hostname,
        "os":          os_name,
        "kernel":      kernel_ver,
        "cpu": {
            "name":    cpu_name,
            "pct":     cpu_pct,
            "cores":   cpu_cores,
            "load_1":  load.one,
            "load_5":  load.five,
            "load_15": load.fifteen,
        },
        "memory": {
            "total":      mem_total,
            "used":       mem_used,
            "free":       mem_total.saturating_sub(mem_used),
            "swap_total": swap_total,
            "swap_used":  swap_used,
        },
        "uptime_secs": uptime,
        "processes":   procs,
        "disks":       disk_list,
        "network":     net_list,
        "temperatures": temp_list,
        "daemon": {
            "mode":          mode,
            "frozen":        frozen,
            "brightness":    brightness,
            "fps":           fps,
            "frame_seq":     frame_seq,
            "uptime_secs":   uptime_secs,
            "version":       env!("CARGO_PKG_VERSION"),
        },
    }))
}

// ─── Weather API ──────────────────────────────────────────────────────────────

/// GET /api/weather — return the currently cached weather data.
async fn api_get_weather(state: web::Data<DaemonState>) -> impl Responder {
    let wd = state.weather_data.lock().unwrap().clone();
    HttpResponse::Ok().json(wd)
}

/// POST /api/weather — push new weather data and optionally switch to Weather mode.
///
/// Body follows the [`WeatherData`] schema.  An extra `show` boolean field (default true)
/// controls whether the display switches to weather mode immediately.
async fn api_update_weather(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);

    let show = body.get("show").and_then(|v| v.as_bool()).unwrap_or(true);

    // Deserialise into WeatherData (ignore unknown fields via Value → serde)
    let mut wd: WeatherData = match serde_json::from_value(body.into_inner()) {
        Ok(d)  => d,
        Err(e) => return HttpResponse::BadRequest().body(format!("invalid body: {e}")),
    };
    // Stamp the update time
    wd.updated_at = Some(now_secs);
    *state.weather_data.lock().unwrap() = wd;

    if show {
        let prev = state.mode.lock().unwrap().clone();
        if !prev.is_transient() {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::Weather;
    }

    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// DELETE /api/weather — clear cached weather data and switch back to previous mode.
async fn api_clear_weather(state: web::Data<DaemonState>) -> impl Responder {
    *state.weather_data.lock().unwrap() = WeatherData::default();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().body("weather cleared")
}
/// POST /api/weather/fetch — trigger an immediate wttr.in fetch for the configured location.
/// Body (optional): `{"location": "London", "switch_mode": true}`
async fn api_weather_fetch_now(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let location = body.get("location")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let loc = state.weather_location.lock().unwrap().clone();
            if loc.is_empty() { None } else { Some(loc) }
        });

    match location {
        None => HttpResponse::BadRequest().body(r#"no location configured; pass {"location":"City"}"#),
        Some(loc) => {
            // Update stored location
            *state.weather_location.lock().unwrap() = loc.clone();

            match fetch_wttr(&state, &loc).await {
                Ok(()) => {
                    let switch = body.get("switch_mode").and_then(|v| v.as_bool()).unwrap_or(true);
                    if switch {
                        let cur = state.mode.lock().unwrap().clone();
                        if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
                        *state.mode.lock().unwrap() = DisplayMode::Weather;
                    }
                    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "location": loc }))
                }
                Err(e) => HttpResponse::BadGateway()
                    .body(format!("wttr.in fetch failed for '{}': {e}", loc)),
            }
        }
    }
}

#[derive(Deserialize)]
struct WeatherAutoRequest {
    location:     String,
    #[serde(default = "default_weather_auto_secs")]
    interval_secs: u64,
}
fn default_weather_auto_secs() -> u64 { 1800 }  // 30 minutes

/// PUT /api/weather/auto — configure location and auto-refresh interval.
/// Body: `{"location":"London","interval_secs":1800}`
async fn api_weather_auto(
    state: web::Data<DaemonState>,
    body:  web::Json<WeatherAutoRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.location.is_empty() {
        return HttpResponse::BadRequest().body("location must not be empty");
    }
    let secs = req.interval_secs.max(60); // minimum 1 minute
    *state.weather_location.lock().unwrap()  = req.location.clone();
    *state.weather_auto_secs.lock().unwrap() = secs;
    save_new_settings(&state);
    log::info!("weather auto-fetch enabled: location='{}' interval={}s", req.location, secs);
    HttpResponse::Ok().json(serde_json::json!({
        "ok":           true,
        "location":     req.location,
        "interval_secs": secs,
    }))
}

/// DELETE /api/weather/auto — disable auto-fetch (clears location).
async fn api_weather_auto_stop(state: web::Data<DaemonState>) -> impl Responder {
    *state.weather_location.lock().unwrap()  = String::new();
    *state.weather_auto_secs.lock().unwrap() = 0;
    save_new_settings(&state);
    log::info!("weather auto-fetch disabled");
    HttpResponse::Ok().body("weather auto-fetch disabled")
}


// ─── Notes / bulletin board API ──────────────────────────────────────────────

/// GET /api/notes — return current notes title and items.
async fn api_get_notes(state: web::Data<DaemonState>) -> impl Responder {
    let title = state.notes_title.lock().unwrap().clone();
    let items = state.notes_items.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "title": title, "items": items }))
}

/// POST /api/notes — replace or append notes.
///
/// Body JSON schema:
/// ```json
/// { "title": "Sprint 42", "items": ["Task A done", "Task B in progress"], "append": false, "show": true }
/// ```
/// - `append: true` → add to existing list; `false` (default) → replace.
/// - `show: true` (default) → switch to Notes mode immediately.
async fn api_push_notes(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let title  = body.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let append = body.get("append").and_then(|v| v.as_bool()).unwrap_or(false);
    let show   = body.get("show").and_then(|v| v.as_bool()).unwrap_or(true);

    let new_items: Vec<String> = body.get("items")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Update title if provided
    if !title.is_empty() {
        *state.notes_title.lock().unwrap() = title;
    }

    // Replace or append items
    {
        let mut guard = state.notes_items.lock().unwrap();
        if append {
            guard.extend(new_items.iter().cloned());
        } else {
            *guard = new_items;
        }
        let count = guard.len();
        drop(guard);

        if show {
            let prev = state.mode.lock().unwrap().clone();
            if !prev.is_transient() {
                *state.prev_mode.lock().unwrap() = prev;
            }
            *state.mode.lock().unwrap() = DisplayMode::Notes;
        }

        HttpResponse::Ok().json(serde_json::json!({ "count": count }))
    }
}

/// DELETE /api/notes — clear all notes and return to previous mode.
async fn api_clear_notes(state: web::Data<DaemonState>) -> impl Responder {
    state.notes_title.lock().unwrap().clear();
    state.notes_items.lock().unwrap().clear();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().body("notes cleared")
}

// ─── QR Code mode ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct QrSetRequest {
    /// The text or URL to encode.
    payload: String,
    /// Optional title shown in the header bar (defaults to "QR Code").
    #[serde(default)]
    title: String,
    /// If true, automatically switch to qr_code mode after setting.
    #[serde(default = "bool_true")]
    show: bool,
}

fn bool_true() -> bool { true }

/// GET /api/qr — return the current QR payload and title.
async fn api_get_qr(state: web::Data<DaemonState>) -> impl Responder {
    let payload = state.qr_payload.lock().unwrap().clone();
    let title   = state.qr_title.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "payload": payload, "title": title }))
}

/// POST /api/qr — set the QR code payload (and optionally title), switch to qr_code mode.
///
/// Body: `{ "payload": "https://...", "title": "My App", "show": true }`
async fn api_set_qr(state: web::Data<DaemonState>, body: web::Json<QrSetRequest>) -> impl Responder {
    let payload = body.payload.trim().to_string();
    if payload.is_empty() {
        return HttpResponse::BadRequest().body("payload must not be empty");
    }
    let title = if body.title.is_empty() { "QR Code".to_string() } else { body.title.clone() };
    *state.qr_payload.lock().unwrap() = payload.clone();
    *state.qr_title.lock().unwrap()   = title.clone();
    push_history(&state, HistoryKind::System, format!("QR code set: {}", &payload.chars().take(60).collect::<String>()));
    if body.show {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
        *state.mode.lock().unwrap() = DisplayMode::QrCode;
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "payload": payload, "title": title }))
}

/// DELETE /api/qr — clear the QR code and return to the previous mode.
async fn api_clear_qr(state: web::Data<DaemonState>) -> impl Responder {
    state.qr_payload.lock().unwrap().clear();
    *state.qr_title.lock().unwrap() = "QR Code".to_string();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    push_history(&state, HistoryKind::System, "QR code cleared".to_string());
    HttpResponse::Ok().body("qr cleared")
}

// ─── Mode schedule API ───────────────────────────────────────────────────────

/// GET /api/schedule/mode — return all mode schedule entries.
async fn api_get_mode_schedule(state: web::Data<DaemonState>) -> impl Responder {
    let entries = state.mode_schedule.lock().unwrap().clone();
    HttpResponse::Ok().json(entries)
}

/// POST /api/schedule/mode — add or replace a schedule entry.
///
/// Body: `{ "hour": 9, "minute": 0, "mode": "clock_sysinfo", "label": "Work hours" }`
/// Entries are kept sorted by (hour, minute). Adding an entry with the same (hour, minute)
/// replaces the existing one.
async fn api_add_mode_schedule(state: web::Data<DaemonState>, body: web::Json<ModeScheduleEntry>) -> impl Responder {
    // Validate mode
    if body.mode.parse::<DisplayMode>().is_err() {
        return HttpResponse::BadRequest().body(format!("unknown mode: {}", body.mode));
    }
    if body.hour > 23 || body.minute > 59 {
        return HttpResponse::BadRequest().body("hour must be 0-23, minute must be 0-59");
    }
    let entry = body.into_inner();
    let mut entries = state.mode_schedule.lock().unwrap();
    // Replace if same time slot exists, otherwise push
    if let Some(pos) = entries.iter().position(|e| e.hour == entry.hour && e.minute == entry.minute) {
        entries[pos] = entry.clone();
    } else {
        entries.push(entry.clone());
    }
    entries.sort_by_key(|e| (e.hour, e.minute));
    save_mode_schedule(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": entries.len() }))
}

/// DELETE /api/schedule/mode — remove all schedule entries.
async fn api_clear_mode_schedule(state: web::Data<DaemonState>) -> impl Responder {
    state.mode_schedule.lock().unwrap().clear();
    save_mode_schedule(&[]);
    HttpResponse::Ok().body("schedule cleared")
}

/// DELETE /api/schedule/mode/{{index}} — remove entry at zero-based index.
async fn api_remove_mode_schedule_entry(
    state: web::Data<DaemonState>,
    path:  web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut entries = state.mode_schedule.lock().unwrap();
    if idx >= entries.len() {
        return HttpResponse::BadRequest().body("index out of range");
    }
    entries.remove(idx);
    save_mode_schedule(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": entries.len() }))
}

// ─── API documentation ────────────────────────────────────────────────────────

/// GET /api/docs — brief HTML reference for all API endpoints.
///
/// Useful for quick lookup without reading source code.
async fn api_docs() -> HttpResponse {
    let version = env!("CARGO_PKG_VERSION");
    let html = format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ttlcd API Reference v{version}</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ background: #1e1e2e; color: #cdd6f4; font-family: 'Segoe UI', system-ui, sans-serif; padding: 2rem; max-width: 900px; margin: 0 auto; }}
    h1 {{ color: #cba6f7; font-size: 1.8rem; margin-bottom: 0.25rem; }}
    h1 small {{ color: #6c7086; font-size: 0.75rem; font-weight: 400; margin-left: 0.5rem; }}
    h2 {{ color: #89b4fa; font-size: 1.1rem; margin: 1.5rem 0 0.6rem; border-bottom: 1px solid #313244; padding-bottom: 0.4rem; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.85rem; margin-bottom: 0.5rem; }}
    th {{ text-align: left; color: #a6adc8; padding: 0.3rem 0.6rem; background: #181825; font-weight: 600; }}
    td {{ padding: 0.3rem 0.6rem; border-bottom: 1px solid #313244; vertical-align: top; }}
    td:first-child {{ font-family: monospace; color: #94e2d5; white-space: nowrap; }}
    td:nth-child(2) {{ color: #89b4fa; white-space: nowrap; }}
    code {{ background: #313244; padding: 0.05rem 0.3rem; border-radius: 3px; font-family: monospace; font-size: 0.82rem; }}
    a {{ color: #89b4fa; }}
    .badge-get    {{ color: #a6e3a1; }}
    .badge-post   {{ color: #89b4fa; }}
    .badge-delete {{ color: #f38ba8; }}
    footer {{ margin-top: 2rem; color: #6c7086; font-size: 0.8rem; }}
  </style>
</head>
<body>
  <h1>ttlcd API Reference <small>v{version}</small></h1>
  <p style="color:#a6adc8;margin-top:0.5rem">HTTP JSON API. All endpoints respond to <code>localhost:7070</code> by default. <a href="/">← Web UI</a></p>

  <h2>Status &amp; Info</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/status</td><td class="badge-get">GET</td><td>Full daemon status (mode, version, frame_seq, rotation, etc.)</td></tr>
    <tr><td>/api/version</td><td class="badge-get">GET</td><td>Version string only</td></tr>
    <tr><td>/api/uptime</td><td class="badge-get">GET</td><td>Daemon uptime in seconds and human-readable</td></tr>
    <tr><td>/api/panel/info</td><td class="badge-get">GET</td><td>USB panel hardware info (serial, firmware)</td></tr>
    <tr><td>/api/modes</td><td class="badge-get">GET</td><td>List of all mode names</td></tr>
    <tr><td>/api/modes/info</td><td class="badge-get">GET</td><td>Mode names with descriptions</td></tr>
    <tr><td>/health</td><td class="badge-get">GET</td><td>Liveness probe (plain text)</td></tr>
    <tr><td>/api/status.svg</td><td class="badge-get">GET</td><td>Shields.io-style SVG badge for current mode</td></tr>
  </tbody></table>

  <h2>Display Control</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/mode</td><td class="badge-post">POST</td><td>Set mode <code>&#123;"mode":"clock"&#125;</code></td></tr>
    <tr><td>/api/mode/next</td><td class="badge-post">POST</td><td>Advance to next mode in ALL_NAMES</td></tr>
    <tr><td>/api/mode/prev</td><td class="badge-post">POST</td><td>Go to previous mode</td></tr>
    <tr><td>/api/quality</td><td class="badge-post">POST</td><td>Set JPEG quality <code>&#123;"quality":85&#125;</code></td></tr>
    <tr><td>/api/orient</td><td class="badge-post">POST</td><td>Set rotation <code>&#123;"degrees":90&#125;</code></td></tr>
    <tr><td>/api/brightness</td><td class="badge-get">GET</td><td>Get brightness factor</td></tr>
    <tr><td>/api/brightness</td><td class="badge-post">POST</td><td>Set brightness <code>&#123;"brightness":0.8&#125;</code></td></tr>
    <tr><td>/api/display/freeze</td><td class="badge-post">POST</td><td>Freeze display (last frame stays)</td></tr>
    <tr><td>/api/display/unfreeze</td><td class="badge-post">POST</td><td>Resume display updates</td></tr>
    <tr><td>/api/display/frozen</td><td class="badge-get">GET</td><td>Returns <code>&#123;"frozen":true|false&#125;</code></td></tr>
    <tr><td>/api/blank</td><td class="badge-post">POST</td><td>Send a solid black frame</td></tr>
  </tbody></table>

  <h2>Content Push</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/text</td><td class="badge-post">POST</td><td>Push custom text (title, subtitle, lines, progress, color, duration_secs)</td></tr>
    <tr><td>/api/clear</td><td class="badge-post">POST</td><td>Return to previous live mode</td></tr>
    <tr><td>/api/image</td><td class="badge-post">POST</td><td>Upload image (multipart/form-data)</td></tr>
    <tr><td>/api/image/path</td><td class="badge-post">POST</td><td>Display image by server path</td></tr>
    <tr><td>/api/banner</td><td class="badge-post">POST</td><td>Large centred text banner</td></tr>
    <tr><td>/api/alert</td><td class="badge-post">POST</td><td>Alert-style push (uses red color)</td></tr>
    <tr><td>/api/notify</td><td class="badge-post">POST</td><td>Webhook-compatible notification</td></tr>
    <tr><td>/api/test</td><td class="badge-post">POST</td><td>Show test card for 5s</td></tr>
  </tbody></table>

  <h2>Presets</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/presets</td><td class="badge-get">GET</td><td>List all presets</td></tr>
    <tr><td>/api/presets</td><td class="badge-post">POST</td><td>Save preset <code>&#123;"name":"…","payload":…&#125;</code></td></tr>
    <tr><td>/api/presets</td><td class="badge-delete">DELETE</td><td>Clear all presets</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-get">GET</td><td>Get single preset payload</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-post">POST</td><td>Apply preset to panel</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-delete">DELETE</td><td>Delete preset</td></tr>
    <tr><td>/api/presets/export</td><td class="badge-get">GET</td><td>Export all presets as JSON</td></tr>
    <tr><td>/api/presets/import</td><td class="badge-post">POST</td><td>Import presets from JSON file</td></tr>
  </tbody></table>

  <h2>Timers</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/countdown</td><td class="badge-post">POST</td><td>Start countdown <code>&#123;"seconds":300,"label":"Break"&#125;</code></td></tr>
    <tr><td>/api/countdown</td><td class="badge-delete">DELETE</td><td>Cancel countdown</td></tr>
    <tr><td>/api/stopwatch</td><td class="badge-get">GET</td><td>Get stopwatch state</td></tr>
    <tr><td>/api/stopwatch/start</td><td class="badge-post">POST</td><td>Start/resume stopwatch</td></tr>
    <tr><td>/api/stopwatch/stop</td><td class="badge-post">POST</td><td>Pause stopwatch</td></tr>
    <tr><td>/api/stopwatch/lap</td><td class="badge-post">POST</td><td>Record lap</td></tr>
    <tr><td>/api/stopwatch/reset</td><td class="badge-post">POST</td><td>Reset stopwatch</td></tr>
    <tr><td>/api/schedule</td><td class="badge-post">POST</td><td>Schedule delayed notification</td></tr>
  </tbody></table>

  <h2>Slideshow</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/slideshow</td><td class="badge-post">POST</td><td>Start slideshow <code>&#123;"dir":"/path","interval":5&#125;</code></td></tr>
    <tr><td>/api/slideshow/next</td><td class="badge-post">POST</td><td>Next image</td></tr>
    <tr><td>/api/slideshow/prev</td><td class="badge-post">POST</td><td>Previous image</td></tr>
    <tr><td>/api/slideshow/shuffle</td><td class="badge-post">POST</td><td>Randomise image order</td></tr>
    <tr><td>/api/slideshow/images</td><td class="badge-get">GET</td><td>List images in current slideshow directory</td></tr>
  </tbody></table>

  <h2>Auto-Rotation</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/rotation</td><td class="badge-get">GET</td><td>Get rotation config</td></tr>
    <tr><td>/api/rotation</td><td class="badge-post">POST</td><td>Set <code>&#123;"modes":["clock","sysinfo"],"interval":30&#125;</code></td></tr>
  </tbody></table>

  <h2>Custom Metrics</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/metrics</td><td class="badge-get">GET</td><td>Get current metrics</td></tr>
    <tr><td>/api/metrics</td><td class="badge-post">POST</td><td>Push metrics <code>&#123;"title":"…","metrics":[&#123;"label":"CPU","value":"72%","color":"blue"&#125;]&#125;</code></td></tr>
    <tr><td>/api/metrics</td><td class="badge-delete">DELETE</td><td>Clear metrics</td></tr>
  </tbody></table>

  <h2>Weather &amp; Notes</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/weather</td><td class="badge-get">GET</td><td>Get cached weather data</td></tr>
    <tr><td>/api/weather</td><td class="badge-post">POST</td><td>Push weather snapshot (WeatherData JSON)</td></tr>
    <tr><td>/api/weather</td><td class="badge-delete">DELETE</td><td>Clear weather data</td></tr>
    <tr><td>/api/weather/fetch</td><td class="badge-post">POST</td><td>Trigger server-side wttr.in fetch <code>&#123;"location":"London","switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/weather/auto</td><td class="badge-put">PUT</td><td>Enable auto-fetch <code>&#123;"location":"London","interval_secs":1800&#125;</code></td></tr>
    <tr><td>/api/weather/auto</td><td class="badge-delete">DELETE</td><td>Disable weather auto-fetch</td></tr>
    <tr><td>/api/notes</td><td class="badge-get">GET</td><td>Get current notes</td></tr>
    <tr><td>/api/notes</td><td class="badge-post">POST</td><td>Push notes <code>&#123;"title":"…","items":["…"],"append":false&#125;</code></td></tr>
    <tr><td>/api/notes</td><td class="badge-delete">DELETE</td><td>Clear notes</td></tr>
    <tr><td>/api/qr</td><td class="badge-get">GET</td><td>Get current QR code payload and title</td></tr>
    <tr><td>/api/qr</td><td class="badge-post">POST</td><td>Set QR code <code>{{&quot;payload&quot;:&quot;https://…&quot;,&quot;title&quot;:&quot;…&quot;,&quot;show&quot;:true}}</code></td></tr>
    <tr><td>/api/qr</td><td class="badge-delete">DELETE</td><td>Clear QR code and return to previous mode</td></tr>
    <tr><td>/api/image/url</td><td class="badge-post">POST</td><td>Fetch image from URL and display it <code>{{&quot;url&quot;:&quot;https://…&quot;,&quot;letterbox&quot;:true}}</code></td></tr>
    <tr><td>/api/preview.png</td><td class="badge-get">GET</td><td>Current panel frame as lossless PNG</td></tr>
  </tbody></table>

  <h2>System Stats</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/sysinfo</td><td class="badge-get">GET</td><td>CPU, RAM, load, uptime</td></tr>
    <tr><td>/api/temperatures</td><td class="badge-get">GET</td><td>Thermal sensor readings</td></tr>
    <tr><td>/api/network</td><td class="badge-get">GET</td><td>Network interface rates</td></tr>
    <tr><td>/api/processes</td><td class="badge-get">GET</td><td>Top processes by CPU</td></tr>
    <tr><td>/api/disks</td><td class="badge-get">GET</td><td>Disk partition usage</td></tr>
    <tr><td>/api/cpu/history</td><td class="badge-get">GET</td><td>CPU usage ring-buffer (120 samples)</td></tr>
    <tr><td>/api/net/history</td><td class="badge-get">GET</td><td>Net Rx/Tx ring-buffer (60 samples)</td></tr>
    <tr><td>/api/sys/snapshot</td><td class="badge-get">GET</td><td>All system stats in one call</td></tr>
  </tbody></table>

  <h2>Alerts &amp; Scheduling</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/alerts/thresholds</td><td class="badge-get">GET</td><td>Get CPU/temp alert thresholds</td></tr>
    <tr><td>/api/alerts/thresholds</td><td class="badge-post">POST</td><td>Set thresholds <code>&#123;"cpu_pct":90,"temp_c":80,"cooldown_secs":300&#125;</code></td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-get">GET</td><td>Get night-dim schedule</td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-post">POST</td><td>Set <code>&#123;"start_hour":22,"end_hour":7,"dim_level":0.15&#125;</code></td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-delete">DELETE</td><td>Clear dim schedule</td></tr>
    <tr><td>/api/history</td><td class="badge-get">GET</td><td>Notification history <code>?limit=20&amp;format=json|csv</code></td></tr>
    <tr><td>/api/history</td><td class="badge-delete">DELETE</td><td>Clear history</td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-get">GET</td><td>List time-of-day mode schedule entries</td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-post">POST</td><td>Add schedule entry <code>{{&quot;hour&quot;:9,&quot;minute&quot;:0,&quot;mode&quot;:&quot;clock_sysinfo&quot;,&quot;label&quot;:&quot;…&quot;}}</code></td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-delete">DELETE</td><td>Clear all schedule entries</td></tr>
    <tr><td>/api/schedule/mode/{{index}}</td><td class="badge-delete">DELETE</td><td>Remove one schedule entry by index</td></tr>
    <tr><td>/api/fps/history</td><td class="badge-get">GET</td><td>Render FPS ring-buffer (last 60 samples)</td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-get">GET</td><td>Get webhook forward URL</td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-post">POST</td><td>Set webhook forward URL <code>{{&quot;url&quot;:&quot;https://…&quot;}}</code></td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-delete">DELETE</td><td>Clear webhook forward URL</td></tr>
  </tbody></table>

  <h2>Profiles &amp; Config</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/profiles</td><td class="badge-get">GET</td><td>List saved display profiles</td></tr>
    <tr><td>/api/profiles</td><td class="badge-post">POST</td><td>Save current state as profile <code>&#123;"name":"night"&#125;</code></td></tr>
    <tr><td>/api/profiles/&#123;name&#125;/apply</td><td class="badge-post">POST</td><td>Apply saved profile</td></tr>
    <tr><td>/api/profiles/&#123;name&#125;</td><td class="badge-delete">DELETE</td><td>Delete profile</td></tr>
    <tr><td>/api/config</td><td class="badge-get">GET</td><td>Return effective daemon configuration</td></tr>
    <tr><td>/api/state/reload</td><td class="badge-post">POST</td><td>Reload persisted state from disk</td></tr>
    <tr><td>/api/info</td><td class="badge-get">GET</td><td>Alias for /api/status</td></tr>
  </tbody></table>

  <h2>Preview &amp; Screenshot</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/preview</td><td class="badge-get">GET</td><td>Current panel frame as JPEG</td></tr>
    <tr><td>/api/screenshot</td><td class="badge-get">GET</td><td>Alias for /api/preview (download filename)</td></tr>
    <tr><td>/api/screenshot.png</td><td class="badge-get">GET</td><td>Current panel frame as lossless PNG</td></tr>
    <tr><td>/api/stream</td><td class="badge-get">GET</td><td>MJPEG live stream</td></tr>
    <tr><td>/api/events</td><td class="badge-get">GET</td><td>SSE stream: <code>frame</code> and <code>mode</code> events</td></tr>
    <tr><td>/api/render</td><td class="badge-post">POST</td><td>Render text payload to JPEG (no panel update)</td></tr>
    <tr><td>/metrics</td><td class="badge-get">GET</td><td>Prometheus-format metrics</td></tr>
  </tbody></table>

  <h2>Pomodoro Timer</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/pomodoro</td><td class="badge-get">GET</td><td>Get Pomodoro state</td></tr>
    <tr><td>/api/pomodoro/start</td><td class="badge-post">POST</td><td>Start/resume timer</td></tr>
    <tr><td>/api/pomodoro/pause</td><td class="badge-post">POST</td><td>Pause timer</td></tr>
    <tr><td>/api/pomodoro/skip</td><td class="badge-post">POST</td><td>Skip current phase</td></tr>
    <tr><td>/api/pomodoro/stop</td><td class="badge-post">POST</td><td>Stop and reset timer</td></tr>
    <tr><td>/api/pomodoro/reset</td><td class="badge-post">POST</td><td>Reset to work phase</td></tr>
    <tr><td>/api/pomodoro/config</td><td class="badge-post">POST</td><td>Configure durations <code>&#123;"work_secs":1500,"short_break_secs":300&#125;</code></td></tr>
  </tbody></table>

  <h2>World Clock, Ticker &amp; Battery</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/world_clock</td><td class="badge-get">GET</td><td>Get city list</td></tr>
    <tr><td>/api/world_clock</td><td class="badge-post">POST</td><td>Set cities <code>[&#123;"city":"Berlin","utc_offset_mins":60&#125;]</code></td></tr>
    <tr><td>/api/world_clock</td><td class="badge-delete">DELETE</td><td>Reset to default cities</td></tr>
    <tr><td>/api/ticker</td><td class="badge-get">GET</td><td>Get ticker text and color</td></tr>
    <tr><td>/api/ticker</td><td class="badge-post">POST</td><td>Set ticker <code>&#123;"text":"…","color":"teal","switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/ticker</td><td class="badge-delete">DELETE</td><td>Clear ticker</td></tr>
    <tr><td>/api/battery</td><td class="badge-get">GET</td><td>Battery level and status</td></tr>
  </tbody></table>

  <h2>HTTP Monitor, Shell Command &amp; Ping</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/http_status</td><td class="badge-get">GET</td><td>Get monitors and latest results</td></tr>
    <tr><td>/api/http_status</td><td class="badge-post">POST</td><td>Add monitor <code>&#123;"label":"…","url":"https://…"&#125;</code></td></tr>
    <tr><td>/api/http_status/&#123;index&#125;</td><td class="badge-delete">DELETE</td><td>Remove monitor by index</td></tr>
    <tr><td>/api/http_status/interval</td><td class="badge-get">GET</td><td>Set check interval <code>&#123;"secs":30&#125;</code> (PUT)</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-get">GET</td><td>Get command, output, status</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-post">POST</td><td>Set command <code>&#123;"cmd":"…","interval_secs":10,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/shell_cmd/run</td><td class="badge-post">POST</td><td>Force immediate re-run</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-delete">DELETE</td><td>Clear command</td></tr>
    <tr><td>/api/ping_monitor</td><td class="badge-get">GET</td><td>Get hosts and latest ping results</td></tr>
    <tr><td>/api/ping_monitor</td><td class="badge-post">POST</td><td>Add host <code>&#123;"label":"…","host":"8.8.8.8"&#125;</code></td></tr>
    <tr><td>/api/ping_monitor/&#123;index&#125;</td><td class="badge-delete">DELETE</td><td>Remove host by index</td></tr>
    <tr><td>/api/ping_monitor/interval</td><td class="badge-get">GET</td><td>Set ping interval <code>&#123;"secs":10&#125;</code> (PUT)</td></tr>
    <tr><td>/api/log_tail</td><td class="badge-get">GET</td><td>Get log tail config and cached lines</td></tr>
    <tr><td>/api/log_tail</td><td class="badge-post">POST</td><td>Set file <code>&#123;"path":"/var/log/syslog","n":16,"interval_secs":3,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/log_tail</td><td class="badge-delete">DELETE</td><td>Clear log tail config</td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-get">GET</td><td>Get HTTP JSON metrics source config</td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-post">POST</td><td>Set source <code>&#123;"url":"…","fields":[&#123;"label":"CPU","path":"cpu_pct","format":"&#123;&#125;%","color":"green"&#125;]&#125;</code></td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-delete">DELETE</td><td>Clear metrics source</td></tr>
    <tr><td>/api/metrics/source/fetch</td><td class="badge-post">POST</td><td>Trigger immediate fetch from metrics source</td></tr>
    <tr><td>/api/rss</td><td class="badge-get">GET</td><td>Get RSS feed config, cached title and items</td></tr>
    <tr><td>/api/rss</td><td class="badge-post">POST</td><td>Set feed <code>&#123;"url":"https://…","interval_secs":300,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/rss</td><td class="badge-delete">DELETE</td><td>Clear RSS feed config</td></tr>
    <tr><td>/api/rss/fetch</td><td class="badge-post">POST</td><td>Trigger immediate fetch of the configured RSS feed</td></tr>
    <tr><td>/api/services</td><td class="badge-get">GET</td><td>Get service unit list and latest statuses</td></tr>
    <tr><td>/api/services</td><td class="badge-post">POST</td><td>Set units <code>&#123;"units":["nginx.service","sshd"],"interval_secs":10,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/services</td><td class="badge-delete">DELETE</td><td>Clear service monitor</td></tr>
  </tbody></table>

  <h2>System Info (New)</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/sys/os</td><td class="badge-get">GET</td><td>OS name, kernel, CPU, memory, uptime, arch</td></tr>
    <tr><td>/api/sys/net</td><td class="badge-get">GET</td><td>Detailed network interfaces: addresses, counters, errors</td></tr>
    <tr><td>/api/sys/snapshot</td><td class="badge-get">GET</td><td>All system stats in one call</td></tr>
    <tr><td>/api/docker</td><td class="badge-get">GET</td><td>Docker containers list (running/stopped)</td></tr>
    <tr><td>/api/sessions</td><td class="badge-get">GET</td><td>Active user sessions from <code>who -u</code></td></tr>
  </tbody></table>

  <h2>Progress Display</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/progress</td><td class="badge-get">GET</td><td>Get current progress state</td></tr>
    <tr><td>/api/progress</td><td class="badge-post">POST</td><td>Update progress <code>&#123;"label":"…","value":47,"total":200,"color":"blue","start_timer":true&#125;</code> — auto-switches to Progress mode</td></tr>
    <tr><td>/api/progress</td><td class="badge-delete">DELETE</td><td>Clear progress and return to previous mode</td></tr>
  </tbody></table>

  <h2>Screensaver / Idle Timeout</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/screensaver</td><td class="badge-get">GET</td><td>Get screensaver config: timeout, mode, active status, seconds since activity</td></tr>
    <tr><td>/api/screensaver</td><td class="badge-post">POST</td><td>Configure screensaver <code>&#123;"timeout_secs":300,"mode":"matrix_rain"&#125;</code> — 0 to disable</td></tr>
    <tr><td>/api/screensaver</td><td class="badge-delete">DELETE</td><td>Disable screensaver</td></tr>
    <tr><td>/api/screensaver/wake</td><td class="badge-post">POST</td><td>Immediately deactivate screensaver and restore previous mode</td></tr>
  </tbody></table>

  <h2>Authentication</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/auth/status</td><td class="badge-get">GET</td><td>Check if API key auth is active</td></tr>
    <tr><td>/api/auth/key</td><td class="badge-post">POST</td><td>Enable Bearer token auth <code>&#123;"key":"secret"&#125;</code></td></tr>
    <tr><td>/api/auth/key</td><td class="badge-delete">DELETE</td><td>Disable auth (clears key)</td></tr>
  </tbody></table>

  <h2>Push Notifications (ntfy.sh)</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/ntfy/config</td><td class="badge-get">GET</td><td>Get ntfy.sh topic URL</td></tr>
    <tr><td>/api/ntfy/config</td><td class="badge-post">POST</td><td>Set topic <code>&#123;"url":"https://ntfy.sh/my-topic"&#125;</code></td></tr>
    <tr><td>/api/ntfy/config</td><td class="badge-delete">DELETE</td><td>Disable ntfy notifications</td></tr>
    <tr><td>/api/ntfy/test</td><td class="badge-post">POST</td><td>Send test notification to configured topic</td></tr>
  </tbody></table>

  <h2>Batch API</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/batch</td><td class="badge-post">POST</td><td>Execute up to 20 operations atomically <code>[&#123;"method":"POST","path":"/api/mode","body":&#123;"mode":"sysinfo"&#125;&#125;,...]</code> — returns per-op status array</td></tr>
  </tbody></table>

  <h2>On-Demand Render</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/render/&#123;mode&#125;</td><td class="badge-get">GET</td><td>Render any mode to JPEG without updating the panel. Query: <code>quality=85&amp;rotate=0</code></td></tr>
  </tbody></table>

  <h2>Settings</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/settings/export</td><td class="badge-get">GET</td><td>Export all settings as JSON</td></tr>
    <tr><td>/api/settings/import</td><td class="badge-post">POST</td><td>Import settings from JSON file</td></tr>
  </tbody></table>

  <footer>ttlcd v{version} — <a href="/">Back to Web UI</a></footer>
</body>
</html>"##, version = version);

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "no-cache"))
        .body(html)
}

// ─── FPS history ─────────────────────────────────────────────────────────────

/// GET /api/fps/history — return the render-FPS ring-buffer as a JSON array.
///
/// Returns up to 60 recent FPS samples (EMA-smoothed), oldest first.
/// Use this to draw a sparkline showing render performance over time.
async fn api_fps_history(state: web::Data<DaemonState>) -> impl Responder {
    let fh = state.fps_history.lock().unwrap();
    let samples: Vec<f32> = fh.iter().copied().collect();
    HttpResponse::Ok().json(serde_json::json!({
        "samples": samples,
        "current": state.render_fps.lock().unwrap().clone(),
        "count":   samples.len(),
    }))
}

// ─── Webhook configuration ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct WebhookConfigRequest {
    url: Option<String>,
}

/// GET /api/webhook/config — return the current webhook forward URL (if any).
async fn api_get_webhook_config(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.webhook_forward_url.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "url": url,
        "configured": url.is_some(),
    }))
}

/// POST /api/webhook/config — set the webhook forward URL.
///
/// Body: `{"url": "https://hooks.example.com/..."}`
///
/// When set, any event received at `/api/webhook` (CI/CD pushes, alerts) is
/// also forwarded to this URL as a POST with the same JSON body.
async fn api_set_webhook_config(
    state: web::Data<DaemonState>,
    body:  web::Json<WebhookConfigRequest>,
) -> impl Responder {
    let url = match &body.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return HttpResponse::BadRequest().body("url is required and must not be empty"),
    };
    *state.webhook_forward_url.lock().unwrap() = Some(url.clone());
    log::info!("webhook forward URL set to: {}", url);
    push_history(&state, HistoryKind::System, format!("Webhook forward URL configured: {}", url));
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "url": url }))
}

/// DELETE /api/webhook/config — clear the webhook forward URL.
async fn api_clear_webhook_config(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.webhook_forward_url.lock().unwrap().take().is_some();
    if was_set {
        log::info!("webhook forward URL cleared");
        push_history(&state, HistoryKind::System, "Webhook forward URL cleared".to_string());
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "cleared": was_set }))
}

// ─── Settings export / import ─────────────────────────────────────────────────

/// GET /api/settings/export — export complete daemon state as a JSON bundle.
///
/// Returns a JSON object containing:
/// - `daemon_state`: mode, quality, brightness, rotation
/// - `presets`: all saved text presets
/// - `profiles`: all saved display profiles
/// - `alert_thresholds`: CPU/temp thresholds
/// - `dim_schedule`: night dim configuration
/// - `notes`: current notes title and items
/// - `metrics`: current metrics title and items
///
/// The bundle can be re-imported via `POST /api/settings/import`.
async fn api_settings_export(state: web::Data<DaemonState>) -> impl Responder {
    let presets:    HashMap<String, CustomTextPayload> = state.presets.lock().unwrap().clone();
    let profiles:   Vec<DisplayProfile>   = load_profiles();
    let thresholds: AlertThresholds       = state.alert_thresholds.lock().unwrap().clone();
    let dim_sched:  Option<DimSchedule>   = state.dim_schedule.lock().unwrap().clone();

    let bundle = serde_json::json!({
        "export_version": 1,
        "exported_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "daemon_state": {
            "mode":       state.mode.lock().unwrap().as_str(),
            "quality":    *state.quality.lock().unwrap(),
            "brightness": *state.dim_factor.lock().unwrap(),
            "rotate_deg": *state.rotate_deg.lock().unwrap(),
        },
        "presets":   presets,
        "profiles":  profiles,
        "alert_thresholds": thresholds,
        "dim_schedule": dim_sched,
        "notes": {
            "title": *state.notes_title.lock().unwrap(),
            "items": *state.notes_items.lock().unwrap(),
        },
        "metrics": {
            "title":   *state.metrics_title.lock().unwrap(),
            "metrics": *state.metrics_data.lock().unwrap(),
        },
        "http_monitors":      state.http_monitors.lock().unwrap().clone(),
        "ping_hosts":         state.ping_hosts.lock().unwrap().clone(),
        "world_clock":        state.world_clock_cities.lock().unwrap().clone(),
        "mode_schedule":      state.mode_schedule.lock().unwrap().clone(),
        "weather_location":   state.weather_location.lock().unwrap().clone(),
        "weather_auto_secs":  *state.weather_auto_secs.lock().unwrap(),
        "log_tail_path":      state.log_tail_path.lock().unwrap().clone(),
        "log_tail_n":         *state.log_tail_n.lock().unwrap(),
        "log_tail_interval":  *state.log_tail_interval.lock().unwrap(),
        "shell_cmd":          state.shell_cmd_text.lock().unwrap().clone(),
        "shell_cmd_interval": *state.shell_cmd_interval_secs.lock().unwrap(),
        "metrics_source":     state.metrics_source.lock().unwrap().clone(),
        "rss_feed_url":       state.rss_feed_url.lock().unwrap().clone(),
        "rss_feed_interval":  *state.rss_feed_interval.lock().unwrap(),
        "service_units":      state.service_units.lock().unwrap().clone(),
        "service_poll_interval": *state.service_poll_interval.lock().unwrap(),
    });

    HttpResponse::Ok()
        .insert_header(("Content-Disposition", "attachment; filename=ttlcd-settings.json"))
        .insert_header(("Cache-Control", "no-store"))
        .json(bundle)
}

/// POST /api/settings/import — import a previously exported settings bundle.
///
/// Accepts the JSON format produced by `GET /api/settings/export`.
/// Only non-null fields are applied; missing sections are skipped.
async fn api_settings_import(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let mut applied: Vec<&str> = Vec::new();

    // daemon_state
    if let Some(ds) = body.get("daemon_state") {
        if let Some(mode_str) = ds.get("mode").and_then(|v| v.as_str()) {
            if let Some(m) = DisplayMode::from_str_name(mode_str) {
                *state.mode.lock().unwrap()      = m.clone();
                *state.prev_mode.lock().unwrap() = m;
                applied.push("mode");
            }
        }
        if let Some(q) = ds.get("quality").and_then(|v| v.as_u64()) {
            *state.quality.lock().unwrap() = q.clamp(1, 100) as u8;
            applied.push("quality");
        }
        if let Some(b) = ds.get("brightness").and_then(|v| v.as_f64()) {
            *state.dim_factor.lock().unwrap() = (b as f32).clamp(0.0, 1.0);
            applied.push("brightness");
        }
        if let Some(r) = ds.get("rotate_deg").and_then(|v| v.as_u64()) {
            let deg = match r { 90 => 90u16, 180 => 180, 270 => 270, _ => 0 };
            *state.rotate_deg.lock().unwrap() = deg;
            applied.push("rotate_deg");
        }
        persist_state(&state);
    }

    // presets
    if let Some(presets_val) = body.get("presets") {
        if let Ok(new_presets) = serde_json::from_value::<HashMap<String, CustomTextPayload>>(presets_val.clone()) {
            let mut guard = state.presets.lock().unwrap();
            for (k, v) in new_presets { guard.insert(k, v); }
            save_presets_to_disk(&guard);
            applied.push("presets");
        }
    }

    // profiles
    if let Some(profiles_val) = body.get("profiles") {
        if let Ok(new_profiles) = serde_json::from_value::<Vec<DisplayProfile>>(profiles_val.clone()) {
            let mut existing = load_profiles();
            for p in new_profiles {
                if let Some(e) = existing.iter_mut().find(|e| e.name == p.name) { *e = p; }
                else { existing.push(p); }
            }
            save_profiles(&existing);
            applied.push("profiles");
        }
    }

    // alert_thresholds
    if let Some(t_val) = body.get("alert_thresholds") {
        if let Ok(t) = serde_json::from_value::<AlertThresholds>(t_val.clone()) {
            *state.alert_thresholds.lock().unwrap() = t;
            applied.push("alert_thresholds");
        }
    }

    // dim_schedule
    if let Some(d_val) = body.get("dim_schedule") {
        if let Ok(sched) = serde_json::from_value::<Option<DimSchedule>>(d_val.clone()) {
            *state.dim_schedule.lock().unwrap() = sched;
            applied.push("dim_schedule");
        }
    }

    // notes
    if let Some(n_val) = body.get("notes") {
        if let Some(title) = n_val.get("title").and_then(|v| v.as_str()) {
            *state.notes_title.lock().unwrap() = title.to_string();
        }
        if let Some(items) = n_val.get("items").and_then(|v| v.as_array()) {
            let items: Vec<String> = items.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            *state.notes_items.lock().unwrap() = items;
        }
        applied.push("notes");
    }

    // metrics
    if let Some(m_val) = body.get("metrics") {
        if let Some(title) = m_val.get("title").and_then(|v| v.as_str()) {
            *state.metrics_title.lock().unwrap() = title.to_string();
        }
        if let Ok(metrics) = serde_json::from_value::<Vec<Metric>>(m_val.get("metrics").cloned().unwrap_or_default()) {
            *state.metrics_data.lock().unwrap() = metrics;
        }
        applied.push("metrics");
    }

    // http_monitors
    if let Some(hm_val) = body.get("http_monitors") {
        if let Ok(monitors) = serde_json::from_value::<Vec<HttpMonitorEntry>>(hm_val.clone()) {
            *state.http_monitors.lock().unwrap() = monitors.clone();
            save_http_monitors(&monitors);
            applied.push("http_monitors");
        }
    }

    // ping_hosts
    if let Some(ph_val) = body.get("ping_hosts") {
        if let Ok(hosts) = serde_json::from_value::<Vec<PingHostEntry>>(ph_val.clone()) {
            *state.ping_hosts.lock().unwrap() = hosts.clone();
            save_ping_hosts(&hosts);
            applied.push("ping_hosts");
        }
    }

    // world_clock
    if let Some(wc_val) = body.get("world_clock") {
        if let Ok(cities) = serde_json::from_value::<Vec<(String, i32)>>(wc_val.clone()) {
            *state.world_clock_cities.lock().unwrap() = cities.clone();
            save_world_clock(&cities);
            applied.push("world_clock");
        }
    }

    // mode_schedule
    if let Some(ms_val) = body.get("mode_schedule") {
        if let Ok(entries) = serde_json::from_value::<Vec<ModeScheduleEntry>>(ms_val.clone()) {
            *state.mode_schedule.lock().unwrap() = entries.clone();
            save_mode_schedule(&entries);
            applied.push("mode_schedule");
        }
    }
    // weather auto-fetch
    if let Some(loc) = body.get("weather_location").and_then(|v| v.as_str()) {
        if !loc.is_empty() {
            *state.weather_location.lock().unwrap() = loc.to_string();
            applied.push("weather_location");
        }
    }
    if let Some(secs) = body.get("weather_auto_secs").and_then(|v| v.as_u64()) {
        *state.weather_auto_secs.lock().unwrap() = secs;
        applied.push("weather_auto_secs");
    }
    // log tail
    if let Some(path) = body.get("log_tail_path").and_then(|v| v.as_str()) {
        if !path.is_empty() {
            *state.log_tail_path.lock().unwrap()    = path.to_string();
            *state.log_tail_interval.lock().unwrap() = body.get("log_tail_interval")
                .and_then(|v| v.as_u64()).unwrap_or(3).max(1);
            *state.log_tail_n.lock().unwrap()        = body.get("log_tail_n")
                .and_then(|v| v.as_u64()).unwrap_or(16) as usize;
            applied.push("log_tail");
        }
    }
    // shell_cmd
    if let Some(cmd) = body.get("shell_cmd").and_then(|v| v.as_str()) {
        if !cmd.is_empty() {
            *state.shell_cmd_text.lock().unwrap()        = cmd.to_string();
            *state.shell_cmd_interval_secs.lock().unwrap() = body.get("shell_cmd_interval")
                .and_then(|v| v.as_u64()).unwrap_or(10).max(1);
            applied.push("shell_cmd");
        }
    }
    // metrics_source
    if let Some(ms_val) = body.get("metrics_source") {
        if let Ok(src) = serde_json::from_value::<MetricsSource>(ms_val.clone()) {
            *state.metrics_source.lock().unwrap() = Some(src);
            applied.push("metrics_source");
        }
    }
    // rss feed
    if let Some(url) = body.get("rss_feed_url").and_then(|v| v.as_str()) {
        if !url.is_empty() {
            *state.rss_feed_url.lock().unwrap()      = url.to_string();
            *state.rss_feed_interval.lock().unwrap() = body.get("rss_feed_interval")
                .and_then(|v| v.as_u64()).unwrap_or(300).max(30);
            applied.push("rss_feed");
        }
    }
    // service status
    if let Some(sv_val) = body.get("service_units") {
        if let Ok(units) = serde_json::from_value::<Vec<String>>(sv_val.clone()) {
            if !units.is_empty() {
                *state.service_units.lock().unwrap()         = units;
                *state.service_poll_interval.lock().unwrap() = body.get("service_poll_interval")
                    .and_then(|v| v.as_u64()).unwrap_or(10).max(5);
                applied.push("service_units");
            }
        }
    }
    save_new_settings(&state);

    log::info!("settings imported: {} section(s) applied", applied.len());
    HttpResponse::Ok().json(serde_json::json!({
        "applied": applied,
        "count":   applied.len(),
    }))
}

// ─── Pomodoro API ─────────────────────────────────────────────────────────────

/// GET /api/pomodoro — current Pomodoro timer state.
async fn api_get_pomodoro(state: web::Data<DaemonState>) -> impl Responder {
    let p = state.pomodoro.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "phase":               p.phase.as_str(),
        "remaining_secs":      p.remaining_secs(),
        "total_secs":          p.phase_total_secs(),
        "running":             p.running,
        "completed_sessions":  p.completed_sessions,
        "sessions_in_set":     p.completed_sessions % p.long_break_interval,
        "work_secs":           p.work_secs,
        "short_break_secs":    p.short_break_secs,
        "long_break_secs":     p.long_break_secs,
        "long_break_interval": p.long_break_interval,
    }))
}

/// POST /api/pomodoro/start — start or resume the Pomodoro timer.
async fn api_pomodoro_start(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if !p.running {
            p.running               = true;
            p.phase_started_unix_ms = now_unix_ms();
        }
    }
    *state.mode.lock().unwrap() = DisplayMode::Pomodoro;
    push_history(&state, HistoryKind::System, "Pomodoro started".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// POST /api/pomodoro/pause — pause the running timer.
async fn api_pomodoro_pause(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if p.running {
            p.phase_elapsed_ms      = p.phase_elapsed_ms_now();
            p.running               = false;
            p.phase_started_unix_ms = 0;
        }
    }
    push_history(&state, HistoryKind::System, "Pomodoro paused".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// POST /api/pomodoro/skip — skip the current phase and advance to the next.
async fn api_pomodoro_skip(state: web::Data<DaemonState>) -> impl Responder {
    let next_phase = {
        let mut p = state.pomodoro.lock().unwrap();
        p.advance_phase();
        p.phase.as_str().to_string()
    };
    *state.mode.lock().unwrap() = DisplayMode::Pomodoro;
    push_history(&state, HistoryKind::System, format!("Pomodoro skipped → {}", next_phase));
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "phase": next_phase }))
}

/// POST /api/pomodoro/reset — reset the timer to the beginning of the current phase.
async fn api_pomodoro_reset(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        p.phase                 = PomodoroPhase::Work;
        p.phase_elapsed_ms      = 0;
        p.phase_started_unix_ms = 0;
        p.running               = false;
        p.completed_sessions    = 0;
    }
    push_history(&state, HistoryKind::System, "Pomodoro reset".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[derive(serde::Deserialize)]
struct PomodoroConfigRequest {
    work_mins:           Option<u64>,
    short_break_mins:    Option<u64>,
    long_break_mins:     Option<u64>,
    long_break_interval: Option<u32>,
}

/// POST /api/pomodoro/config — configure work/break durations.
///
/// Body (all fields optional):
/// ```json
/// { "work_mins": 25, "short_break_mins": 5, "long_break_mins": 15, "long_break_interval": 4 }
/// ```
async fn api_pomodoro_config(
    state: web::Data<DaemonState>,
    body:  web::Json<PomodoroConfigRequest>,
) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if let Some(m) = body.work_mins           { p.work_secs           = m * 60; }
        if let Some(m) = body.short_break_mins    { p.short_break_secs    = m * 60; }
        if let Some(m) = body.long_break_mins     { p.long_break_secs     = m * 60; }
        if let Some(n) = body.long_break_interval { p.long_break_interval = n.max(1); }
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Battery info API ─────────────────────────────────────────────────────────

/// GET /api/battery — read live battery status from the system.
///
/// On desktops or systems without a battery, returns `{"present": false}`.
/// On laptops, returns capacity, status, voltage and estimated time remaining.
async fn api_battery_json() -> impl Responder {
    use ttlcd::display::read_battery_json;
    HttpResponse::Ok().json(read_battery_json())
}

// ─── World clock API ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct WorldClockCity {
    city:           String,
    utc_offset_mins: i32,
}

/// GET /api/world_clock — return the current list of cities.
async fn api_get_world_clock(state: web::Data<DaemonState>) -> impl Responder {
    let cities = state.world_clock_cities.lock().unwrap()
        .iter()
        .map(|(c, o)| serde_json::json!({ "city": c, "utc_offset_mins": o }))
        .collect::<Vec<_>>();
    HttpResponse::Ok().json(serde_json::json!({ "cities": cities }))
}

/// POST /api/world_clock — set city list (replaces all).
///
/// Body: `[{"city":"New York","utc_offset_mins":-300}, ...]`  (up to 4 entries)
async fn api_set_world_clock(
    state: web::Data<DaemonState>,
    body:  web::Json<Vec<WorldClockCity>>,
) -> impl Responder {
    let entries: Vec<(String, i32)> = body.0.into_iter()
        .take(4)
        .map(|c| (c.city, c.utc_offset_mins.clamp(-14 * 60, 14 * 60)))
        .collect();
    let count = entries.len();
    *state.world_clock_cities.lock().unwrap() = entries.clone();
    save_world_clock(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": count }))
}

/// DELETE /api/world_clock — reset to default cities.
async fn api_clear_world_clock(state: web::Data<DaemonState>) -> impl Responder {
    state.world_clock_cities.lock().unwrap().clear();
    save_world_clock(&[]);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "cleared": true }))
}

/// POST /api/pomodoro/stop — stop the timer and return to the previous mode.
async fn api_pomodoro_stop(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        p.phase_elapsed_ms      = p.phase_elapsed_ms_now();
        p.running               = false;
        p.phase_started_unix_ms = 0;
    }
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    push_history(&state, HistoryKind::System, "Pomodoro stopped".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Ticker API ───────────────────────────────────────────────────────────────

/// GET /api/ticker — current ticker text and color.
async fn api_get_ticker(state: web::Data<DaemonState>) -> impl Responder {
    let text  = state.ticker_text.lock().unwrap().clone();
    let color = state.ticker_color.lock().unwrap().clone();
    let pos   = *state.ticker_pos.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({ "text": text, "color": color, "scroll_pos": pos }))
}

#[derive(serde::Deserialize)]
struct TickerRequest {
    text:   String,
    #[serde(default)]
    color:  Option<String>,
    #[serde(default)]
    switch_mode: Option<bool>,
}

/// POST /api/ticker — set the ticker text (and optionally switch to ticker mode).
///
/// Body: `{"text": "Hello World!", "color": "blue", "switch_mode": true}`
async fn api_set_ticker(
    state: web::Data<DaemonState>,
    body:  web::Json<TickerRequest>,
) -> impl Responder {
    let text  = body.text.clone();
    let color = body.color.clone().unwrap_or_else(|| "blue".to_string());

    *state.ticker_text.lock().unwrap()  = text.clone();
    *state.ticker_color.lock().unwrap() = color.clone();
    *state.ticker_pos.lock().unwrap()   = 0;  // reset scroll to start

    if body.switch_mode.unwrap_or(true) {
        *state.mode.lock().unwrap() = DisplayMode::Ticker;
    }

    let summary = format!("Ticker: {}", text.chars().take(60).collect::<String>());
    push_history(&state, HistoryKind::Text, summary);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "text": text, "color": color }))
}

/// DELETE /api/ticker — clear ticker text and return to previous mode.
async fn api_clear_ticker(state: web::Data<DaemonState>) -> impl Responder {
    *state.ticker_text.lock().unwrap()  = String::new();
    *state.ticker_pos.lock().unwrap()   = 0;
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── HTTP Status Monitor API handlers ────────────────────────────────────────

/// GET /api/http_status — return monitors list and latest check results.
async fn api_get_http_status(state: web::Data<DaemonState>) -> impl Responder {
    let monitors: Vec<serde_json::Value> = state.http_monitors.lock().unwrap()
        .iter().map(|m| serde_json::json!({ "label": m.label, "url": m.url }))
        .collect();
    let results = state.http_results.lock().unwrap().clone();
    let interval = *state.http_check_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "monitors": monitors,
        "results":  results,
        "check_interval_secs": interval,
    }))
}

/// POST /api/http_status — add a monitor: {"label":"...", "url":"..."}
async fn api_add_http_monitor(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let label = body.get("label").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let url   = body.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if label.is_empty() || url.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "label and url required" }));
    }
    let entry = HttpMonitorEntry { label: label.clone(), url: url.clone() };
    let mut monitors = state.http_monitors.lock().unwrap();
    if monitors.len() >= 10 {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "maximum 10 monitors" }));
    }
    monitors.push(entry);
    save_http_monitors(&monitors.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "label": label, "url": url }))
}

/// DELETE /api/http_status/{index} — remove monitor at given index.
async fn api_delete_http_monitor(
    state: web::Data<DaemonState>,
    path:  web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut monitors = state.http_monitors.lock().unwrap();
    if idx >= monitors.len() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "index out of range" }));
    }
    monitors.remove(idx);
    // Also drop the corresponding result if present
    let mut results = state.http_results.lock().unwrap();
    if idx < results.len() { results.remove(idx); }
    save_http_monitors(&monitors.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// PUT /api/http_status/interval — set check interval: {"secs": 30}
async fn api_set_http_interval(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let secs = body.get("secs").and_then(|v| v.as_u64()).unwrap_or(30).max(5);
    *state.http_check_interval_secs.lock().unwrap() = secs;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "secs": secs }))
}

// ─── Shell Command API handlers ───────────────────────────────────────────────

/// GET /api/shell_cmd — return current command, output, and status.
async fn api_get_shell_cmd(state: web::Data<DaemonState>) -> impl Responder {
    let cmd      = state.shell_cmd_text.lock().unwrap().clone();
    let output   = state.shell_cmd_output.lock().unwrap().clone();
    let exit_ok  = *state.shell_cmd_exit_ok.lock().unwrap();
    let run_at   = *state.shell_cmd_run_at.lock().unwrap();
    let interval = *state.shell_cmd_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "cmd":          cmd,
        "output":       output,
        "exit_ok":      exit_ok,
        "run_at_unix":  run_at,
        "interval_secs": interval,
    }))
}

/// POST /api/shell_cmd — set command: {"cmd":"...", "interval_secs":10, "switch_mode":true}
async fn api_set_shell_cmd(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let cmd = body.get("cmd").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if cmd.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "cmd required" }));
    }
    let interval = body.get("interval_secs").and_then(|v| v.as_u64()).unwrap_or(10);
    let switch   = body.get("switch_mode").and_then(|v| v.as_bool()).unwrap_or(false);

    *state.shell_cmd_text.lock().unwrap()          = cmd.clone();
    *state.shell_cmd_interval_secs.lock().unwrap() = interval;
    // Clear stale output
    *state.shell_cmd_output.lock().unwrap() = "(running…)".to_string();
    *state.shell_cmd_exit_ok.lock().unwrap() = true;

    if switch {
        let prev = state.mode.lock().unwrap().clone();
        if !matches!(prev, DisplayMode::ShellCmd) {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::ShellCmd;
        persist_state(&state);
    }

    // Trigger an immediate run in the background
    let state2 = state.as_ref().clone();
    let cmd2 = cmd.clone();
    tokio::spawn(async move { run_shell_cmd_once(&state2, &cmd2).await });

    save_new_settings(&state);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "cmd": cmd, "interval_secs": interval }))
}

/// POST /api/shell_cmd/run — force an immediate re-run of the current command.
async fn api_run_shell_cmd_now(state: web::Data<DaemonState>) -> impl Responder {
    let cmd = state.shell_cmd_text.lock().unwrap().clone();
    if cmd.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "no command configured" }));
    }
    *state.shell_cmd_output.lock().unwrap() = "(running…)".to_string();
    let state2 = state.as_ref().clone();
    tokio::spawn(async move { run_shell_cmd_once(&state2, &cmd).await });
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// DELETE /api/shell_cmd — clear command and return to previous mode.
async fn api_clear_shell_cmd(state: web::Data<DaemonState>) -> impl Responder {
    *state.shell_cmd_text.lock().unwrap()   = String::new();
    *state.shell_cmd_output.lock().unwrap() = String::new();
    *state.shell_cmd_run_at.lock().unwrap() = 0;
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Ping Monitor API handlers ────────────────────────────────────────────────

/// GET /api/ping_monitor — return hosts list and latest ping results.
async fn api_get_ping(state: web::Data<DaemonState>) -> impl Responder {
    let hosts: Vec<serde_json::Value> = state.ping_hosts.lock().unwrap()
        .iter().map(|h| serde_json::json!({ "label": h.label, "host": h.host }))
        .collect();
    let results = state.ping_results.lock().unwrap().clone();
    let interval = *state.ping_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "hosts":    hosts,
        "results":  results,
        "check_interval_secs": interval,
    }))
}

/// POST /api/ping_monitor — add a host: {"label":"...", "host":"8.8.8.8"}
async fn api_add_ping_host(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let label = body.get("label").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let host  = body.get("host").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if label.is_empty() || host.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "label and host required" }));
    }
    let entry = PingHostEntry { label: label.clone(), host: host.clone() };
    let mut hosts = state.ping_hosts.lock().unwrap();
    if hosts.len() >= 12 {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "maximum 12 hosts" }));
    }
    hosts.push(entry);
    save_ping_hosts(&hosts.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "label": label, "host": host }))
}

/// DELETE /api/ping_monitor/{index} — remove host at given index.
async fn api_delete_ping_host(
    state: web::Data<DaemonState>,
    path:  web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut hosts = state.ping_hosts.lock().unwrap();
    if idx >= hosts.len() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "index out of range" }));
    }
    hosts.remove(idx);
    let mut results = state.ping_results.lock().unwrap();
    if idx < results.len() { results.remove(idx); }
    save_ping_hosts(&hosts.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// PUT /api/ping_monitor/interval — set check interval: {"secs": 10}
async fn api_set_ping_interval(
    state: web::Data<DaemonState>,
    body:  web::Json<serde_json::Value>,
) -> impl Responder {
    let secs = body.get("secs").and_then(|v| v.as_u64()).unwrap_or(10).max(5);
    *state.ping_interval_secs.lock().unwrap() = secs;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "secs": secs }))
}
// ─── Log tail API ─────────────────────────────────────────────────────────────

/// GET /api/log_tail — return current config and cached lines.
async fn api_get_log_tail(state: web::Data<DaemonState>) -> impl Responder {
    let path  = state.log_tail_path.lock().unwrap().clone();
    let n     = *state.log_tail_n.lock().unwrap();
    let lines = state.log_tail_lines.lock().unwrap().clone();
    let ok    = *state.log_tail_ok.lock().unwrap();
    let secs  = *state.log_tail_interval.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "path":     path,
        "n":        n,
        "ok":       ok,
        "interval_secs": secs,
        "lines":    lines,
    }))
}

#[derive(Deserialize)]
struct LogTailRequest {
    path:            String,
    #[serde(default = "default_log_tail_n")]
    n:               usize,
    #[serde(default = "default_log_tail_interval")]
    interval_secs:   u64,
    #[serde(default)]
    switch_mode:     bool,
}
fn default_log_tail_n()        -> usize { 16 }
fn default_log_tail_interval() -> u64   { 3 }

/// POST /api/log_tail — set file path (and optionally number of lines / interval).
/// Body: `{"path":"/var/log/syslog","n":16,"interval_secs":3,"switch_mode":true}`
async fn api_set_log_tail(
    state: web::Data<DaemonState>,
    body:  web::Json<LogTailRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.path.is_empty() {
        return HttpResponse::BadRequest().body("path must not be empty");
    }
    let n    = req.n.clamp(1, 50);
    let secs = req.interval_secs.max(1);

    *state.log_tail_path.lock().unwrap()     = req.path.clone();
    *state.log_tail_n.lock().unwrap()        = n;
    *state.log_tail_interval.lock().unwrap() = secs;

    // Attempt an immediate read so the display shows content right away
    match read_tail_lines(&req.path, n) {
        Ok(lines) => {
            *state.log_tail_lines.lock().unwrap() = lines;
            *state.log_tail_ok.lock().unwrap()    = true;
        }
        Err(e) => {
            log::warn!("log_tail initial read '{}': {e}", req.path);
            *state.log_tail_ok.lock().unwrap() = false;
        }
    }

    if req.switch_mode {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
        *state.mode.lock().unwrap() = DisplayMode::LogTail;
    }

    save_new_settings(&state);
    log::info!("log_tail configured: path='{}' n={} interval={}s", req.path, n, secs);
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "path": req.path,
        "n": n,
        "interval_secs": secs,
    }))
}

/// DELETE /api/log_tail — clear log tail configuration and stop monitoring.
async fn api_clear_log_tail(state: web::Data<DaemonState>) -> impl Responder {
    *state.log_tail_path.lock().unwrap()  = String::new();
    *state.log_tail_lines.lock().unwrap() = Vec::new();
    *state.log_tail_ok.lock().unwrap()    = false;
    if matches!(*state.mode.lock().unwrap(), DisplayMode::LogTail) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("log tail cleared")
}

// ─── RSS feed API handlers ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct RssFeedStatus {
    url:      String,
    title:    String,
    ok:       bool,
    items:    Vec<(String, String)>,
    interval: u64,
}

async fn api_get_rss(state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(RssFeedStatus {
        url:      state.rss_feed_url.lock().unwrap().clone(),
        title:    state.rss_feed_title.lock().unwrap().clone(),
        ok:       *state.rss_feed_ok.lock().unwrap(),
        items:    state.rss_feed_items.lock().unwrap().clone(),
        interval: *state.rss_feed_interval.lock().unwrap(),
    })
}

#[derive(serde::Deserialize)]
struct RssFeedRequest {
    url:          String,
    #[serde(default)]
    interval_secs: u64,
    #[serde(default)]
    switch_mode:  bool,
}

async fn api_set_rss(
    state: web::Data<DaemonState>,
    body:  web::Json<RssFeedRequest>,
) -> impl Responder {
    if body.url.is_empty() {
        return HttpResponse::BadRequest().body("url is required");
    }
    *state.rss_feed_url.lock().unwrap()      = body.url.clone();
    *state.rss_feed_interval.lock().unwrap() = body.interval_secs.max(30).min(86400);
    *state.rss_feed_ok.lock().unwrap()       = false;
    if body.switch_mode {
        *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
        *state.mode.lock().unwrap()      = DisplayMode::RssFeed;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("rss feed configured")
}

async fn api_clear_rss(state: web::Data<DaemonState>) -> impl Responder {
    *state.rss_feed_url.lock().unwrap()   = String::new();
    *state.rss_feed_title.lock().unwrap() = String::new();
    *state.rss_feed_items.lock().unwrap() = Vec::new();
    *state.rss_feed_ok.lock().unwrap()    = false;
    if matches!(*state.mode.lock().unwrap(), DisplayMode::RssFeed) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("rss feed cleared")
}

async fn api_fetch_rss(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.rss_feed_url.lock().unwrap().clone();
    if url.is_empty() {
        return HttpResponse::BadRequest().body(
            r#"no url configured; use POST /api/rss first"#
        );
    }
    match fetch_rss_feed(&state, &url).await {
        Ok(())   => HttpResponse::Ok().body("rss feed fetched"),
        Err(e)   => HttpResponse::InternalServerError().body(format!("fetch error: {e:#}")),
    }
}

// ─── Service status API handlers ──────────────────────────────────────────────

#[derive(serde::Serialize)]
struct ServiceStatusResponse {
    units:    Vec<String>,
    statuses: Vec<ServiceStatusItem>,
    interval: u64,
}

#[derive(serde::Serialize)]
struct ServiceStatusItem {
    name:  String,
    state: String,
    sub:   String,
}

async fn api_get_services(state: web::Data<DaemonState>) -> impl Responder {
    let statuses: Vec<ServiceStatusItem> = state.service_statuses.lock().unwrap()
        .iter()
        .map(|s| ServiceStatusItem { name: s.name.clone(), state: s.state.clone(), sub: s.sub.clone() })
        .collect();
    HttpResponse::Ok().json(ServiceStatusResponse {
        units:    state.service_units.lock().unwrap().clone(),
        statuses,
        interval: *state.service_poll_interval.lock().unwrap(),
    })
}

#[derive(serde::Deserialize)]
struct ServicesRequest {
    units:        Vec<String>,
    #[serde(default)]
    interval_secs: u64,
    #[serde(default)]
    switch_mode:  bool,
}

async fn api_set_services(
    state: web::Data<DaemonState>,
    body:  web::Json<ServicesRequest>,
) -> impl Responder {
    if body.units.is_empty() {
        return HttpResponse::BadRequest().body("units array is required");
    }
    *state.service_units.lock().unwrap()         = body.units.clone();
    *state.service_poll_interval.lock().unwrap() = body.interval_secs.max(5).min(3600);

    // Do an immediate poll
    let units = body.units.clone();
    let statuses = tokio::task::spawn_blocking(move || query_service_statuses(&units))
        .await
        .unwrap_or_default();
    *state.service_statuses.lock().unwrap() = statuses;

    if body.switch_mode {
        *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
        *state.mode.lock().unwrap()      = DisplayMode::ServiceStatus;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("services configured")
}

async fn api_clear_services(state: web::Data<DaemonState>) -> impl Responder {
    *state.service_units.lock().unwrap()    = Vec::new();
    *state.service_statuses.lock().unwrap() = Vec::new();
    if matches!(*state.mode.lock().unwrap(), DisplayMode::ServiceStatus) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("services cleared")
}

// ─── Docker containers API ────────────────────────────────────────────────────

/// GET /api/docker — return list of Docker containers (all, running first).
async fn api_get_docker() -> impl Responder {
    let containers = tokio::task::spawn_blocking(read_docker_containers)
        .await
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = containers.iter().map(|c| {
        serde_json::json!({
            "id":      c.id,
            "name":    c.name,
            "image":   c.image,
            "status":  c.status,
            "ports":   c.ports,
            "running": c.running,
        })
    }).collect();
    HttpResponse::Ok().json(serde_json::json!({
        "containers": list,
        "running": list.iter().filter(|c| c["running"].as_bool().unwrap_or(false)).count(),
        "total":   list.len(),
    }))
}

// ─── Logged-in user sessions API ─────────────────────────────────────────────

/// GET /api/sessions — return active user sessions from `who -u`.
async fn api_get_sessions() -> impl Responder {
    let sessions = tokio::task::spawn_blocking(read_user_sessions)
        .await
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = sessions.iter().map(|s| {
        serde_json::json!({
            "username": s.username,
            "tty":      s.tty,
            "login_at": s.login_at,
            "idle":     s.idle,
            "pid":      s.pid,
            "from":     s.from,
        })
    }).collect();
    HttpResponse::Ok().json(serde_json::json!({
        "sessions": list,
        "count": list.len(),
    }))
}

// ─── API key authentication management ───────────────────────────────────────

#[derive(Deserialize)]
struct SetAuthKeyPayload {
    key: String,
}

/// POST /api/auth/key — set an API key. When set, all subsequent API requests
/// must include `Authorization: Bearer <key>` header.
///
/// Body: `{ "key": "your-secret-key" }`
async fn api_set_auth_key(
    state:   web::Data<DaemonState>,
    payload: web::Json<SetAuthKeyPayload>,
) -> impl Responder {
    let key = payload.key.trim().to_string();
    if key.is_empty() {
        return HttpResponse::BadRequest().body("key must not be empty");
    }
    *state.api_key.lock().unwrap() = Some(key.clone());
    push_history(&state, HistoryKind::System,
        format!("API key configured (length {})", key.len()));
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "API key set. Include 'Authorization: Bearer <key>' in all requests.",
        "key_length": key.len(),
    }))
}

/// DELETE /api/auth/key — remove the API key (disable authentication).
async fn api_clear_auth_key(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.api_key.lock().unwrap().take().is_some();
    if was_set {
        push_history(&state, HistoryKind::System, "API key cleared — authentication disabled".to_string());
        HttpResponse::Ok().body("API key cleared")
    } else {
        HttpResponse::Ok().body("no API key was set")
    }
}

/// GET /api/auth/status — returns whether an API key is currently configured.
async fn api_get_auth_status(state: web::Data<DaemonState>) -> impl Responder {
    let has_key = state.api_key.lock().unwrap().is_some();
    HttpResponse::Ok().json(serde_json::json!({
        "auth_required": has_key,
        "method": if has_key { "Bearer token" } else { "none" },
    }))
}

// ─── Detailed network interfaces API ─────────────────────────────────────────

/// GET /api/sys/net — returns detailed info about all network interfaces:
/// name, IPv4/IPv6 addresses, Rx/Tx bytes, packet counts, errors.
///
/// Data sources: /proc/net/dev (counters), ip addr (addresses).
async fn api_sys_net() -> impl Responder {
    let counters = parse_proc_net_dev();
    let addresses = parse_if_addresses();

    let ifaces: Vec<serde_json::Value> = counters.into_iter().map(|(name, stats)| {
        let addrs = addresses.get(&name).cloned().unwrap_or_default();
        serde_json::json!({
            "name":       name,
            "addresses":  addrs,
            "rx_bytes":   stats.rx_bytes,
            "tx_bytes":   stats.tx_bytes,
            "rx_packets": stats.rx_packets,
            "tx_packets": stats.tx_packets,
            "rx_errors":  stats.rx_errors,
            "tx_errors":  stats.tx_errors,
            "rx_dropped": stats.rx_dropped,
            "tx_dropped": stats.tx_dropped,
        })
    }).collect();

    HttpResponse::Ok().json(serde_json::json!({
        "interfaces": ifaces,
        "count": ifaces.len(),
    }))
}

struct IfStats {
    rx_bytes: u64, tx_bytes: u64,
    rx_packets: u64, tx_packets: u64,
    rx_errors: u64, tx_errors: u64,
    rx_dropped: u64, tx_dropped: u64,
}

fn parse_proc_net_dev() -> Vec<(String, IfStats)> {
    let text = match std::fs::read_to_string("/proc/net/dev") {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    // Skip first two header lines
    text.lines().skip(2).filter_map(|line| {
        let (name_raw, rest) = line.split_once(':')?;
        let name = name_raw.trim().to_string();
        let nums: Vec<u64> = rest.split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if nums.len() < 16 { return None; }
        Some((name, IfStats {
            rx_bytes:   nums[0], rx_packets: nums[1],
            rx_errors:  nums[2], rx_dropped: nums[3],
            tx_bytes:   nums[8], tx_packets: nums[9],
            tx_errors:  nums[10], tx_dropped: nums[11],
        }))
    }).collect()
}

fn parse_if_addresses() -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    // IPv4 addresses
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-4", "addr"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Format: <idx>: <name> inet <addr/prefix> ...
            if parts.len() >= 4 && parts[2] == "inet" {
                let name = parts[1].trim_end_matches(':').to_string();
                let addr = parts[3].to_string();
                map.entry(name).or_default().push(addr);
            }
        }
    }
    // IPv6 addresses
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-6", "addr"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "inet6" {
                let name = parts[1].trim_end_matches(':').to_string();
                let addr = format!("ipv6:{}", parts[3]);
                map.entry(name).or_default().push(addr);
            }
        }
    }
    map
}

// ─── ntfy.sh push notification config ────────────────────────────────────────

#[derive(Deserialize)]
struct SetNtfyPayload {
    url: String,
}

/// GET /api/ntfy/config — check current ntfy configuration.
async fn api_get_ntfy_config(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.ntfy_url.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "configured": url.is_some(),
        "url": url.unwrap_or_default(),
    }))
}

/// POST /api/ntfy/config — set the ntfy.sh topic URL.
///
/// Body: `{ "url": "https://ntfy.sh/my-topic" }`
///
/// When set, every Alert-kind event (threshold breach, service failure, etc.)
/// is POSTed to this URL as a plain-text ntfy notification.
async fn api_set_ntfy_config(
    state:   web::Data<DaemonState>,
    payload: web::Json<SetNtfyPayload>,
) -> impl Responder {
    let url = payload.url.trim().to_string();
    if url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return HttpResponse::BadRequest().body("url must start with http:// or https://");
    }
    *state.ntfy_url.lock().unwrap() = Some(url.clone());
    push_history(&state, HistoryKind::System, format!("ntfy notifications configured: {}", url));
    save_new_settings(&state);
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "url": url,
        "message": "Alert notifications will be sent to this URL",
    }))
}

/// DELETE /api/ntfy/config — disable ntfy notifications.
async fn api_clear_ntfy_config(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.ntfy_url.lock().unwrap().take().is_some();
    if was_set {
        push_history(&state, HistoryKind::System, "ntfy notifications disabled".to_string());
        save_new_settings(&state);
        HttpResponse::Ok().body("ntfy config cleared")
    } else {
        HttpResponse::Ok().body("ntfy was not configured")
    }
}

/// POST /api/ntfy/test — send a test notification to the configured ntfy URL.
async fn api_ntfy_test(state: web::Data<DaemonState>) -> impl Responder {
    let ntfy_opt = state.ntfy_url.lock().unwrap().clone();
    let Some(ntfy_url) = ntfy_opt else {
        return HttpResponse::BadRequest().body("ntfy not configured — POST /api/ntfy/config first");
    };
    let result = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client.post(&ntfy_url)
            .header("Title", "ttlcd test notification")
            .header("Priority", "default")
            .header("Tags", "white_check_mark")
            .body("This is a test notification from ttlcd.")
            .send()
            .await?;
        Ok::<_, reqwest::Error>(resp.status())
    }.await;

    match result {
        Ok(status) => {
            push_history(&state, HistoryKind::System, format!("ntfy test notification sent ({})", status));
            HttpResponse::Ok().json(serde_json::json!({
                "ok": status.is_success(),
                "http_status": status.as_u16(),
                "message": format!("Test notification sent to {} — HTTP {}", ntfy_url, status),
            }))
        }
        Err(e) => {
            HttpResponse::BadGateway().json(serde_json::json!({
                "ok": false,
                "error": format!("{}", e),
            }))
        }
    }
}

// ─── Progress display API ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetProgressPayload {
    #[serde(default)]
    label:      String,
    #[serde(default)]
    sublabel:   String,
    #[serde(default)]
    value:      f64,
    #[serde(default = "default_total")]
    total:      f64,
    #[serde(default)]
    units:      String,
    #[serde(default)]
    color:      String,
    /// If true, record current time as started_at (enables ETA calculation).
    #[serde(default)]
    start_timer: bool,
}

fn default_total() -> f64 { 100.0 }

/// GET /api/progress — return current progress state.
async fn api_get_progress(state: web::Data<DaemonState>) -> impl Responder {
    let ps = state.progress_state.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "label":      ps.label,
        "sublabel":   ps.sublabel,
        "value":      ps.value,
        "total":      ps.total,
        "units":      ps.units,
        "color":      format!("{:?}", ps.color).to_lowercase(),
        "pct":        if ps.total > 0.0 { ps.value / ps.total * 100.0 } else { 0.0 },
        "started_at": ps.started_at,
    }))
}

/// POST /api/progress — update the progress state.
///
/// Body fields (all optional):
/// - `label`       — main heading (default "Progress")
/// - `sublabel`    — secondary description line
/// - `value`       — current value (default 0)
/// - `total`       — maximum value (default 100)
/// - `units`       — unit suffix (e.g. "MB", "%")
/// - `color`       — accent: blue | green | yellow | red | mauve | teal | peach
/// - `start_timer` — true to record start time for ETA calculation
///
/// Also switches the display to Progress mode.
async fn api_set_progress(
    state:   web::Data<DaemonState>,
    payload: web::Json<SetProgressPayload>,
) -> impl Responder {
    use ttlcd::display::NamedColor;
    let p = payload.into_inner();
    let color = match p.color.to_lowercase().as_str() {
        "green"  => NamedColor::Green,
        "yellow" => NamedColor::Yellow,
        "red"    => NamedColor::Red,
        "mauve"  => NamedColor::Mauve,
        "teal"   => NamedColor::Teal,
        "peach"  => NamedColor::Peach,
        "white"  => NamedColor::White,
        _        => NamedColor::Blue,
    };
    let started_at = if p.start_timer {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    } else {
        state.progress_state.lock().unwrap().started_at
    };

    let total = if p.total > 0.0 { p.total } else { 100.0 };
    let new_state = ProgressState {
        label:      if p.label.is_empty() { "Progress".into() } else { p.label },
        sublabel:   p.sublabel,
        value:      p.value.clamp(0.0, total),
        total,
        units:      p.units,
        color,
        started_at,
    };
    let pct = new_state.value / new_state.total * 100.0;
    *state.progress_state.lock().unwrap() = new_state;

    // Auto-switch to Progress mode
    let cur = state.mode.lock().unwrap().clone();
    if !cur.is_transient() && cur != DisplayMode::Progress {
        *state.prev_mode.lock().unwrap() = cur;
    }
    *state.mode.lock().unwrap() = DisplayMode::Progress;

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "pct": pct,
    }))
}

/// DELETE /api/progress — clear progress and return to previous mode.
async fn api_clear_progress(state: web::Data<DaemonState>) -> impl Responder {
    *state.progress_state.lock().unwrap() = ProgressState::default();
    if matches!(*state.mode.lock().unwrap(), DisplayMode::Progress) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().body("progress cleared")
}

// ─── On-demand mode preview rendering ────────────────────────────────────────

/// GET /api/render/{mode} — render any display mode to a JPEG on demand.
///
/// The panel is NOT updated. This is purely a preview/screenshot endpoint.
/// Useful for generating thumbnails or testing what a mode looks like.
///
/// Returns: JPEG image (Content-Type: image/jpeg)
/// Query params:
///   `quality=N` — JPEG quality (default: current daemon quality, 1-100)
///   `rotate=N`  — Rotation in degrees: 0, 90, 180, 270 (default: 0)
///
/// Example:
///   curl "http://localhost:7070/api/render/sysinfo" > sysinfo.jpg
///   curl "http://localhost:7070/api/render/matrix_rain?quality=90" > rain.jpg
async fn api_render_mode_preview(
    state:  web::Data<DaemonState>,
    path:   web::Path<String>,
    query:  web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let mode_name = path.into_inner();
    let Some(mode) = DisplayMode::from_str_name(&mode_name) else {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({
                "error": "unknown mode",
                "available": DisplayMode::ALL_NAMES,
            }));
    };

    // Quality and rotation from query params
    let quality   = query.get("quality")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(*state.quality.lock().unwrap())
        .clamp(1, 100);
    let rotate_deg = query.get("rotate")
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0);

    // Render the requested mode in a blocking task
    let state_clone = state.clone();
    let img_result = tokio::task::spawn_blocking(move || -> anyhow::Result<image::DynamicImage> {
        use ttlcd::display::*;
        let img: image::DynamicImage = match mode {
            DisplayMode::Clock        => render_clock(),
            DisplayMode::BigClock     => render_big_clock(),
            DisplayMode::ClockSysinfo => render_clock_sysinfo(),
            DisplayMode::Sysinfo      => render_sysinfo(),
            DisplayMode::CpuDetail    => render_cpu_detail(),
            DisplayMode::Network      => render_network(),
            DisplayMode::DiskUsage    => render_disk_usage(),
            DisplayMode::ProcessTop   => render_process_top(),
            DisplayMode::Temperature  => render_temperature(),
            DisplayMode::MemoryDetail => render_memory_detail(),
            DisplayMode::CpuGraph     => render_cpu_graph(),
            DisplayMode::NetGraph     => render_net_graph(),
            DisplayMode::ProcessMemory => render_process_memory(),
            DisplayMode::DiskIo       => render_disk_io(),
            DisplayMode::Calendar     => render_calendar(),
            DisplayMode::Stopwatch    => {
                let (elapsed_ms, laps, running) = {
                    let sw = state_clone.stopwatch.lock().unwrap();
                    (sw.current_elapsed_ms(), sw.laps.clone(), sw.running)
                };
                render_stopwatch(elapsed_ms, &laps, running)
            },
            DisplayMode::Battery      => render_battery(),
            DisplayMode::Dashboard    => render_dashboard(),
            DisplayMode::CpuFreq      => render_cpu_freq(),
            DisplayMode::HostnameIp   => render_hostname_ip(),
            DisplayMode::NetConnections => render_net_connections(),
            DisplayMode::UptimeFancy  => render_uptime(),
            DisplayMode::OsInfo       => render_os_info(),
            DisplayMode::DockerStatus => {
                let containers = read_docker_containers();
                render_docker_status(&containers)
            },
            DisplayMode::LoggedInUsers => {
                let sessions = read_user_sessions();
                render_logged_in_users(&sessions)
            },
            DisplayMode::LoadGraph    => render_load_graph(),
            DisplayMode::MemGraph     => render_mem_graph(),
            DisplayMode::TempGraph    => render_temp_graph(),
            DisplayMode::DiskIoGraph    => render_disk_io_graph(),
            DisplayMode::GraphsOverview => render_graphs_overview(),
            DisplayMode::AnalogClock    => render_analog_clock(),
            DisplayMode::PingGraph      => {
                let results = state_clone.ping_results.lock().unwrap().clone();
                render_ping_graph(&results)
            },
            DisplayMode::NetSpeedGraph  => render_net_speed_graph(),
            DisplayMode::CpuCoreBars       => render_cpu_core_bars(),
            DisplayMode::SpeedometerGauge => render_speedometer_gauge(),
            DisplayMode::DiskPie          => render_disk_pie(),
            DisplayMode::ThermalDashboard => render_thermal_dashboard(),
            DisplayMode::Plasma        => render_plasma(),
            DisplayMode::Fireworks     => render_fireworks(),
            DisplayMode::CpuHeatmap    => render_cpu_heatmap(),
            DisplayMode::MatrixRain       => render_matrix_rain(),
            DisplayMode::Progress     => {
                let ps = state_clone.progress_state.lock().unwrap().clone();
                render_progress(&ps)
            },
            DisplayMode::RssFeed => {
                let title = state_clone.rss_feed_title.lock().unwrap().clone();
                let items = state_clone.rss_feed_items.lock().unwrap().clone();
                let ok    = *state_clone.rss_feed_ok.lock().unwrap();
                render_rss_feed(&title, &items, ok)
            },
            DisplayMode::ServiceStatus => {
                let units = state_clone.service_statuses.lock().unwrap().clone();
                render_service_status(&units)
            },
            // Modes that require special context — return a blank with label
            _ => {
                let mut blank = image::RgbImage::new(480, 480);
                for pix in blank.pixels_mut() { *pix = image::Rgb([30, 30, 46]); }
                image::DynamicImage::ImageRgb8(blank)
            }
        };
        Ok(img)
    }).await;

    let img = match img_result {
        Ok(Ok(i))  => i,
        Ok(Err(e)) => return HttpResponse::InternalServerError().body(format!("render error: {e}")),
        Err(e)     => return HttpResponse::InternalServerError().body(format!("task error: {e}")),
    };

    let img = apply_rotation(img, rotate_deg);
    match encode_for_panel(&img, quality) {
        Ok(jpeg) => HttpResponse::Ok()
            .content_type("image/jpeg")
            .insert_header(("X-Mode", mode_name.as_str()))
            .insert_header(("X-Quality", quality.to_string().as_str()))
            .body(jpeg),
        Err(e) => HttpResponse::InternalServerError().body(format!("encode error: {e}")),
    }
}


// ─── Auto-screensaver idle detection ─────────────────────────────────────────

/// Background thread: monitors user inactivity and activates/deactivates
/// the screensaver mode.
///
/// When `screensaver_timeout_secs` is > 0 and the time since `last_activity_secs`
/// exceeds the timeout, the daemon switches to `screensaver_mode_name`.
///
/// The screensaver is deactivated automatically when the next API call arrives
/// (handled in `auth_middleware`).
fn screensaver_loop(state: DaemonState) {
    loop {
        thread::sleep(std::time::Duration::from_secs(10));

        let timeout = *state.screensaver_timeout_secs.lock().unwrap();
        if timeout == 0 {
            continue; // screensaver disabled
        }

        let last = *state.last_activity_secs.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let already_active = *state.screensaver_active.lock().unwrap();

        if !already_active && now.saturating_sub(last) >= timeout {
            // Activate screensaver
            let ss_name = state.screensaver_mode_name.lock().unwrap().clone();
            if let Some(ss_mode) = DisplayMode::from_str_name(&ss_name) {
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() && cur.as_str() != ss_name.as_str() {
                    log::info!(
                        "screensaver: activating {} after {}s of inactivity",
                        ss_name, timeout
                    );
                    *state.prev_mode.lock().unwrap() = cur;
                    *state.mode.lock().unwrap()       = ss_mode;
                    *state.screensaver_active.lock().unwrap() = true;
                }
            }
        }
    }
}

// ─── Screensaver API handlers ─────────────────────────────────────────────────

/// GET /api/screensaver — get current screensaver configuration and status.
///
/// Response:
/// ```json
/// {
///   "enabled": true,
///   "timeout_secs": 300,
///   "mode": "matrix_rain",
///   "active": false,
///   "seconds_since_activity": 42
/// }
/// ```
async fn api_get_screensaver(state: web::Data<DaemonState>) -> impl Responder {
    let timeout = *state.screensaver_timeout_secs.lock().unwrap();
    let mode    = state.screensaver_mode_name.lock().unwrap().clone();
    let active  = *state.screensaver_active.lock().unwrap();
    let last    = *state.last_activity_secs.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let since_activity = now.saturating_sub(last);

    HttpResponse::Ok().json(serde_json::json!({
        "enabled": timeout > 0,
        "timeout_secs": timeout,
        "mode": mode,
        "active": active,
        "seconds_since_activity": since_activity,
    }))
}

#[derive(Deserialize)]
struct ScreensaverConfig {
    /// Timeout in seconds before screensaver activates. Set to 0 to disable.
    #[serde(default)]
    timeout_secs: Option<u64>,
    /// Display mode name to use as screensaver (e.g. "matrix_rain", "idle").
    #[serde(default)]
    mode: Option<String>,
}

/// POST /api/screensaver — configure the screensaver.
///
/// Body (JSON):
/// ```json
/// { "timeout_secs": 300, "mode": "matrix_rain" }
/// ```
///
/// Set `timeout_secs` to 0 (or DELETE) to disable. Omit either field to keep
/// its current value.
async fn api_set_screensaver(
    state: web::Data<DaemonState>,
    body:  web::Json<ScreensaverConfig>,
) -> impl Responder {
    let req = body.into_inner();

    if let Some(timeout) = req.timeout_secs {
        *state.screensaver_timeout_secs.lock().unwrap() = timeout;
        if timeout == 0 {
            // Also deactivate if currently active
            let was_active = {
                let mut a = state.screensaver_active.lock().unwrap();
                if *a { *a = false; true } else { false }
            };
            if was_active {
                let prev = state.prev_mode.lock().unwrap().clone();
                *state.mode.lock().unwrap() = prev;
            }
        }
    }

    if let Some(ref mode_name) = req.mode {
        if DisplayMode::from_str_name(mode_name).is_none() {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "unknown mode",
                "available": DisplayMode::ALL_NAMES,
            }));
        }
        *state.screensaver_mode_name.lock().unwrap() = mode_name.clone();
    }

    let timeout = *state.screensaver_timeout_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "enabled": timeout > 0,
        "timeout_secs": timeout,
        "mode": *state.screensaver_mode_name.lock().unwrap(),
    }))
}

/// DELETE /api/screensaver — disable the screensaver.
async fn api_clear_screensaver(state: web::Data<DaemonState>) -> impl Responder {
    *state.screensaver_timeout_secs.lock().unwrap() = 0;
    // Deactivate if currently running
    let was_active = {
        let mut a = state.screensaver_active.lock().unwrap();
        if *a { *a = false; true } else { false }
    };
    if was_active {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().body("screensaver disabled")
}

/// POST /api/screensaver/wake — immediately deactivate the screensaver
/// and restore the previous mode (useful for hardware button integrations).
async fn api_screensaver_wake(state: web::Data<DaemonState>) -> impl Responder {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    *state.last_activity_secs.lock().unwrap() = now;
    let was_active = {
        let mut a = state.screensaver_active.lock().unwrap();
        if *a { *a = false; true } else { false }
    };
    if was_active {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "was_active": was_active }))
}


// ─── Batch API ────────────────────────────────────────────────────────────────

/// POST /api/batch — execute multiple API operations in a single HTTP request.
///
/// This is useful for CI/CD pipelines, automation scripts, and any client that
/// needs to atomically configure several aspects of the daemon (e.g. set mode,
/// update metrics, and trigger an alert) without the latency of multiple round-trips.
///
/// Request body:
/// ```json
/// [
///   { "method": "POST", "path": "/api/mode",  "body": { "mode": "sysinfo" } },
///   { "method": "POST", "path": "/api/alert", "body": { "text": "Deploy started" } },
///   { "method": "POST", "path": "/api/progress", "body": { "label": "Deploy", "value": 0, "total": 100 } }
/// ]
/// ```
///
/// Returns an array of per-operation results:
/// ```json
/// [
///   { "index": 0, "status": 200, "body": "..." },
///   { "index": 1, "status": 200, "body": "..." },
///   { "index": 2, "status": 200, "body": "..." }
/// ]
/// ```
///
/// Operations are executed in order. On failure, processing continues for the
/// remaining operations (fail-soft). Up to 20 operations per batch.
///
/// Supported methods: GET, POST, DELETE.
/// Supported paths: any path that has a corresponding handler.
///
/// Note: GET operations return a simplified summary, not the full response body,
/// since this endpoint is primarily intended for write operations.
async fn api_batch(
    state: web::Data<DaemonState>,
    body:  web::Json<Vec<serde_json::Value>>,
) -> impl Responder {
    let ops = body.into_inner();
    if ops.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "empty batch" }));
    }
    if ops.len() > 20 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "batch too large",
            "max": 20,
            "got": ops.len(),
        }));
    }

    let mut results: Vec<serde_json::Value> = Vec::with_capacity(ops.len());

    for (idx, op) in ops.into_iter().enumerate() {
        let method = op.get("method").and_then(|v| v.as_str()).unwrap_or("POST").to_uppercase();
        let path   = op.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let op_body = op.get("body").cloned().unwrap_or(serde_json::Value::Null);

        let result = process_batch_op(&state, &method, &path, op_body).await;
        results.push(serde_json::json!({
            "index":  idx,
            "status": result.0,
            "body":   result.1,
        }));
    }

    let all_ok = results.iter().all(|r| r["status"].as_u64().unwrap_or(500) < 400);
    HttpResponse::Ok().json(serde_json::json!({
        "ok":      all_ok,
        "count":   results.len(),
        "results": results,
    }))
}

/// Execute a single operation within a batch request.
/// Returns (http_status_code, response_body_string).
async fn process_batch_op(
    state: &web::Data<DaemonState>,
    method: &str,
    path: &str,
    body: serde_json::Value,
) -> (u16, String) {
    match (method, path) {
        // ── Mode ──
        ("POST", "/api/mode") => {
            let mode_name = body.get("mode").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(mode) = DisplayMode::from_str_name(mode_name) {
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() { *state.prev_mode.lock().unwrap() = cur; }
                *state.mode.lock().unwrap() = mode;
                (200, format!("mode set to {}", mode_name))
            } else {
                (400, format!("unknown mode: {}", mode_name))
            }
        }
        ("POST", "/api/mode/next") => {
            let modes = DisplayMode::ALL_NAMES;
            let cur = state.mode.lock().unwrap().as_str().to_string();
            let pos = modes.iter().position(|&m| m == cur).unwrap_or(0);
            let next_name = modes[(pos + 1) % modes.len()];
            if let Some(next) = DisplayMode::from_str_name(next_name) {
                *state.mode.lock().unwrap() = next;
                (200, format!("mode → {}", next_name))
            } else { (500, "internal error".into()) }
        }
        // ── Alert ──
        ("POST", "/api/alert") => {
            let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("batch alert");
            push_history(state, HistoryKind::Alert, text.to_string());
            (200, "alert queued".into())
        }
        // ── Text ──
        ("POST", "/api/text") => {
            if let Ok(payload) = serde_json::from_value::<CustomTextPayload>(body) {
                push_history(state, HistoryKind::Text, format!("text: {}", payload.title.as_deref().unwrap_or("")));
                *state.pending_text.lock().unwrap() = Some(payload.clone());
                let prev = state.mode.lock().unwrap().clone();
                if !prev.is_transient() { *state.prev_mode.lock().unwrap() = prev; }
                *state.mode.lock().unwrap() = DisplayMode::CustomText;
                (200, "text set".into())
            } else {
                (400, "invalid text payload".into())
            }
        }
        // ── Progress ──
        ("POST", "/api/progress") => {
            let label    = body.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let value    = body.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let total    = body.get("total").and_then(|v| v.as_f64()).unwrap_or(100.0);
            let sublabel = body.get("sublabel").and_then(|v| v.as_str()).unwrap_or("").to_string();
            {
                let mut ps = state.progress_state.lock().unwrap();
                if !label.is_empty()    { ps.label    = label.clone(); }
                if !sublabel.is_empty() { ps.sublabel = sublabel; }
                ps.value = value;
                ps.total = total;
            }
            let cur = state.mode.lock().unwrap().clone();
            if !cur.is_transient() && cur != DisplayMode::Progress {
                *state.prev_mode.lock().unwrap() = cur;
                *state.mode.lock().unwrap() = DisplayMode::Progress;
            }
            (200, format!("progress {}/{}", value, total))
        }
        ("DELETE", "/api/progress") => {
            {
                let mut ps = state.progress_state.lock().unwrap();
                *ps = ttlcd::display::ProgressState::default();
            }
            let prev = state.prev_mode.lock().unwrap().clone();
            *state.mode.lock().unwrap() = prev;
            (200, "progress cleared".into())
        }
        // ── Quality ──
        ("POST", "/api/quality") => {
            if let Some(q) = body.get("quality").and_then(|v| v.as_u64()) {
                *state.quality.lock().unwrap() = (q as u8).clamp(1, 100);
                (200, format!("quality={}", q))
            } else { (400, "missing quality".into()) }
        }
        // ── Blank ──
        ("POST", "/api/blank") => {
            *state.mode.lock().unwrap() = DisplayMode::Idle;
            (200, "blanked".into())
        }
        // ── Clear ──
        ("POST", "/api/clear") => {
            let prev = state.prev_mode.lock().unwrap().clone();
            *state.mode.lock().unwrap() = prev;
            (200, "cleared".into())
        }
        // ── GET status (informational) ──
        ("GET", "/api/status") => {
            let mode = state.mode.lock().unwrap().as_str().to_string();
            let seq  = *state.frame_seq.lock().unwrap();
            (200, format!("{{\"mode\":\"{}\",\"frame_seq\":{}}}", mode, seq))
        }
        // ── Unknown ──
        _ => {
            (404, format!("unsupported batch operation: {} {}", method, path))
        }
    }
}
