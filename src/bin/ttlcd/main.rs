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

use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use anyhow::{Context, Result};
use chrono::{Local, Timelike};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use image::{imageops, DynamicImage};
use serde::{Deserialize, Serialize};
use ttlcd::display::{get_cpu_history, get_net_history};
use ttlcd::{
    display::{
        load_slideshow_frames, push_cpu_history, push_disk_io_history, push_load_history,
        push_mem_history, push_ping_history, push_temp_history, read_docker_containers,
        read_user_sessions, render_analog_clock, render_asteroid_field, render_aurora,
        render_battery, render_big_clock, render_big_text_banner, render_binary_clock,
        render_boids, render_boot_splash, render_bounce_ball, render_breakout, render_bubble_chart,
        render_bubble_rise, render_calendar, render_city_skyline, render_clifford_attractor,
        render_clock, render_clock_faces, render_clock_matrix, render_clock_sysinfo,
        render_color_mosaic, render_confetti, render_conway_colors, render_countdown,
        render_cpu_3d_bars, render_cpu_core_bars, render_cpu_detail, render_cpu_freq,
        render_cpu_graph, render_cpu_heatmap, render_cube_3d, render_custom_text, render_dashboard,
        render_disco_floor, render_disk_io, render_disk_io_graph, render_disk_pie,
        render_disk_usage, render_dna_helix, render_docker_status, render_doom_fire,
        render_double_pendulum, render_dvd_bounce, render_ekg_strip, render_fiber_optic,
        render_fire_text, render_fireworks, render_flash_message, render_flow_field,
        render_fourier_epicycles, render_fractal_tree, render_galaxy, render_game_of_life,
        render_glitch_art, render_gradient_clock, render_graphs_overview, render_gravity,
        render_heartbeat, render_hex_grid, render_hex_plasma, render_hostname_ip,
        render_http_status, render_hypno, render_icosahedron, render_julia_set,
        render_kaleidoscope, render_lava_lamp, render_led_clock, render_lemniscate,
        render_lightning_storm, render_lissajous, render_load_graph, render_log_tail,
        render_logged_in_users, render_lorenz, render_magnetic_field, render_mandala,
        render_mandelbrot, render_matrix_rain, render_mem_graph, render_memory_detail,
        render_metaballs, render_metrics, render_minimal_clock, render_morphing_blobs,
        render_moss_growth, render_nbody_gravity, render_neon_clock, render_neon_grid,
        render_neon_sign, render_net_connections, render_net_graph, render_net_heatmap,
        render_net_speed_graph, render_network, render_network_globe, render_neural_net,
        render_notes, render_ocean_waves, render_os_info, render_oscilloscope,
        render_particle_clock, render_pendulum_wave, render_perlin_terrain, render_ping_graph,
        render_ping_monitor, render_plasma, render_plasma_ball, render_pomodoro, render_pong,
        render_process_memory, render_process_top, render_progress, render_pulse, render_qr_code,
        render_radar, render_rain_glass, render_reaction_diffusion, render_recent_alerts,
        render_ribbon_dance, render_ripple, render_rose_curve, render_rss_feed, render_rule_30,
        render_sand, render_sand_waterfall, render_service_status, render_shell_cmd,
        render_sierpinski, render_smoke_plume, render_snake, render_snowfall, render_solar_system,
        render_solid, render_spectrum, render_speedometer_gauge, render_spirograph,
        render_starfield, render_status_overview, render_stopwatch, render_sysinfo,
        render_system_art, render_system_health, render_temp_graph, render_temperature,
        render_test_card, render_thermal_dashboard, render_ticker, render_torus, render_truchet,
        render_tunnel, render_uptime, render_voronoi, render_warp_speed, render_wave_interference,
        render_waveform_bars, render_weather, render_word_clock, render_world_clock,
        CustomTextPayload, HttpMonitorResult, Metric, PingResult, ProgressState, WeatherData,
    },
    encode_for_panel, letterbox_for_panel, Panel, DEFAULT_JPEG_QUALITY, PANEL_HEIGHT, PANEL_WIDTH,
};

mod api_core;
mod api_features;
mod api_panel;
mod api_push;
mod api_status;
mod api_system;
mod background;
mod daemon;
mod render_loop;
mod state;
#[cfg(test)]
mod tests;

use api_core::*;
use api_features::*;
use api_panel::*;
use api_push::*;
use api_status::*;
use api_system::*;
use background::*;
use daemon::*;
use render_loop::*;
use state::*;

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
        /// Bind address. Must be a loopback address: the web UI and API are
        /// unauthenticated and only served to this machine.
        #[arg(long, default_value = "127.0.0.1")]
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
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u16::from_str_radix(s, 16)
}

fn parse_mode(s: &str) -> Result<String, String> {
    DisplayMode::from_str_name(s)
        .map(|_| s.to_string())
        .ok_or_else(|| {
            format!(
                "unknown mode '{}'; choose: {}",
                s,
                DisplayMode::ALL_NAMES.join(", ")
            )
        })
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
        90 => DynamicImage::ImageRgb8(imageops::rotate90(&img.into_rgb8())),
        180 => DynamicImage::ImageRgb8(imageops::rotate180(&img.into_rgb8())),
        270 => DynamicImage::ImageRgb8(imageops::rotate270(&img.into_rgb8())),
        _ => img,
    }
}

/// Darken all pixels by `factor` (1.0 = full brightness, 0.0 = black).
/// Returns the image unchanged when factor ≥ 0.99 to avoid a costly scan.
fn apply_brightness(img: DynamicImage, factor: f32) -> DynamicImage {
    if factor >= 0.99 {
        return img;
    }
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
    if reset {
        Panel::open_with_reset(vid, pid)
    } else {
        Panel::open(vid, pid)
    }
}

/// Collect sorted image file paths from a directory.
fn collect_image_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(std::iter::Iterator::flatten)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "gif"
            )
        })
        .collect();
    paths.sort();
    paths
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let cfg = load_config();

    match cli.cmd {
        Cmd::Modes => {
            println!("{:<18}  DESCRIPTION", "NAME");
            println!("{}", "-".repeat(70));
            for info in &[
                ("clock", "Large digital clock with date and uptime"),
                (
                    "big_clock",
                    "Full-screen giant HH:MM with seconds and progress bar",
                ),
                ("clock_sysinfo", "Split: clock + quick stats"),
                ("sysinfo", "CPU, RAM, temperature, load, uptime"),
                ("cpu_detail", "Per-core CPU usage grid (up to 32 cores)"),
                ("network", "Per-interface Rx/Tx rates and totals"),
                ("disk_usage", "Disk partition usage bars"),
                ("process_top", "Top processes by CPU usage"),
                ("temperature", "Thermal sensor readings"),
                ("memory_detail", "Detailed RAM/swap breakdown"),
                ("countdown", "Countdown timer with colour transitions"),
                ("cpu_graph", "CPU usage history bar chart (2 min)"),
                ("net_graph", "Network I/O history chart (60 s)"),
                ("recent_alerts", "Last 5 notifications as on-panel log"),
                ("process_memory", "Top processes ranked by RAM usage"),
                ("disk_io", "Disk read/write throughput per device"),
                ("calendar", "Full-month calendar with today highlighted"),
                ("idle", "Keepalive — no display update"),
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

        Cmd::Image {
            path,
            quality,
            letterbox,
            no_init,
            rotate,
        } => {
            let img = image::open(&path).with_context(|| format!("open {}", path.display()))?;
            let img = apply_rotation(img, rotate);
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            if letterbox {
                panel.send_jpeg(&letterbox_for_panel(&img, quality)?)?;
            } else {
                panel.send_image(&img, quality)?;
            }
            log::info!("frame sent — holding (Ctrl-C to exit)");
            panel.hold();
        }

        Cmd::Test { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            panel.send_image(&test_pattern(), quality)?;
            log::info!("frame sent — holding (Ctrl-C to exit)");
            panel.hold();
        }

        Cmd::Testcard { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            panel.send_image(&render_test_card(), quality)?;
            log::info!("frame sent — holding (Ctrl-C to exit)");
            panel.hold();
        }

        Cmd::Solid {
            r,
            g,
            b,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            panel.send_image(&render_solid(r, g, b), quality)?;
            log::info!("frame sent — holding (Ctrl-C to exit)");
            panel.hold();
        }

        Cmd::Sysinfo {
            interval,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                panel.send_image(&render_sysinfo(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::Clock { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(1) {
                    thread::sleep(Duration::from_secs(1) - elapsed);
                }
            }
        }

        Cmd::Slideshow {
            dir,
            interval,
            quality,
            no_init,
            letterbox,
        } => {
            let frames = load_slideshow_frames(&dir);
            if frames.is_empty() {
                anyhow::bail!("No supported images in {}", dir.display());
            }
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
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

        Cmd::Network {
            interval,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                panel.send_image(&render_network(), quality)?;
                if interval > 1 {
                    thread::sleep(Duration::from_secs(interval - 1));
                }
            }
        }

        Cmd::CpuDetail {
            interval,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                panel.send_image(&render_cpu_detail(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::ClockSysinfo { quality, no_init } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                let start = Instant::now();
                panel.send_image(&render_clock_sysinfo(), quality)?;
                let elapsed = start.elapsed();
                if elapsed < Duration::from_secs(2) {
                    thread::sleep(Duration::from_secs(2) - elapsed);
                }
            }
        }

        Cmd::DiskUsage {
            interval,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                panel.send_image(&render_disk_usage(), quality)?;
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Cmd::ProcessTop {
            interval,
            quality,
            no_init,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if !no_init {
                panel.init()?;
            }
            loop {
                panel.send_image(&render_process_top(), quality)?;
                if interval > 0 {
                    thread::sleep(Duration::from_secs(interval));
                }
            }
        }

        Cmd::Daemon {
            port,
            bind,
            mode,
            quality,
            headless,
            rotate,
            rotation_modes,
            rotation_interval,
        } => {
            let eff_port = cfg.daemon.port.unwrap_or(port);
            let eff_bind = cfg.daemon.bind.clone().unwrap_or(bind);
            let eff_mode = cfg.daemon.mode.clone().unwrap_or(mode);
            let eff_quality = cfg.daemon.quality.unwrap_or(quality);
            let eff_headless = cfg.daemon.headless.unwrap_or(headless);
            let eff_rotate = cfg.daemon.rotate.unwrap_or(rotate);
            let eff_vid = cfg
                .device
                .vid
                .as_deref()
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(cli.vid);
            let eff_pid = cfg
                .device
                .pid
                .as_deref()
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
                eff_vid,
                eff_pid,
                eff_port,
                eff_bind,
                eff_mode,
                eff_quality,
                eff_headless,
                eff_rotate,
                eff_rot_modes,
                eff_rot_interval,
            )?;
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
            quality,
            no_prelude: _,
            no_init_ack,
            packet_delay_ms,
            pings,
            ping_opcode,
            solid,
        } => {
            let mut panel = open_panel(cli.vid, cli.pid, cli.reset)?;
            if no_init_ack {
                panel.init_no_ack()?;
            } else {
                panel.init()?;
            }
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
            panel.send_jpeg_with_delay(&encode_for_panel(&img, quality)?, packet_delay_ms)?;
            for _ in 0..pings {
                panel.send_ctrl_opcode(ping_opcode as u8)?;
                thread::sleep(Duration::from_millis(200));
            }
        }

        Cmd::Encode {
            input,
            output,
            quality,
            letterbox,
            rotate,
        } => {
            let img = image::open(&input).with_context(|| format!("open {}", input.display()))?;
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
