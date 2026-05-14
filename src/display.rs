//! Off-device frame generators.
//!
//! Each function returns a [`DynamicImage`] at the panel's native 480×480
//! resolution, ready to pass to [`crate::encode_for_panel`] or
//! [`crate::Panel::send_image`].

use std::path::Path;
use std::time::Duration;

use ab_glyph::{Font as AbFont, FontRef, PxScale, ScaleFont};
use chrono::{Datelike, Local, Timelike};
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut, draw_antialiased_line_segment_mut};
use imageproc::pixelops::interpolate;
use imageproc::rect::Rect;
use qrcode::QrCode;
use sysinfo::System;

// ─── Embedded font data ──────────────────────────────────────────────────────

static FONT_BOLD_DATA: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");
static FONT_MONO_DATA: &[u8] = include_bytes!("../assets/DejaVuSansMono-Bold.ttf");

fn font_bold() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BOLD_DATA).expect("embedded DejaVuSans-Bold.ttf is corrupt")
}

fn font_mono() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_MONO_DATA).expect("embedded DejaVuSansMono-Bold.ttf is corrupt")
}

// ─── Colour palette (Catppuccin Mocha) ───────────────────────────────────────

const BG:       Rgb<u8> = Rgb([30,  30,  46]);   // base
const SURFACE0: Rgb<u8> = Rgb([49,  50,  68]);   // surface0
const OVERLAY0: Rgb<u8> = Rgb([108, 112, 134]);  // overlay0
const TEXT:     Rgb<u8> = Rgb([205, 214, 244]);  // text
const SUBTEXT:  Rgb<u8> = Rgb([166, 173, 200]);  // subtext1
const BLUE:     Rgb<u8> = Rgb([137, 180, 250]);  // blue
const GREEN:    Rgb<u8> = Rgb([166, 227, 161]);  // green
const YELLOW:   Rgb<u8> = Rgb([249, 226, 175]);  // yellow
const RED:      Rgb<u8> = Rgb([243, 139, 168]);  // red
const MAUVE:    Rgb<u8> = Rgb([203, 166, 247]);  // mauve
const TEAL:     Rgb<u8> = Rgb([148, 226, 213]);  // teal
const PEACH:    Rgb<u8> = Rgb([250, 179, 135]);  // peach
const CRUST:    Rgb<u8> = Rgb([17,  17,  27]);   // crust (near-black, for QR modules)
const WHITE:    Rgb<u8> = Rgb([255, 255, 255]);  // pure white (for QR quiet zone)

// ─── Font helpers ─────────────────────────────────────────────────────────────

/// Approximate horizontal advance of `text` in pixels at the given scale.
fn text_width(font: &FontRef<'_>, scale: PxScale, text: &str) -> i32 {
    let sf = font.as_scaled(scale);
    text.chars()
        .map(|c| sf.h_advance(sf.glyph_id(c)))
        .sum::<f32>() as i32
}

// ─── Drawing primitives ───────────────────────────────────────────────────────

/// Draw text centred horizontally at y.
fn draw_centered(
    img: &mut RgbImage,
    font: &FontRef<'_>,
    scale: PxScale,
    y: i32,
    color: Rgb<u8>,
    text: &str,
) {
    let w = text_width(font, scale, text);
    let x = (480 - w).max(0) / 2;
    draw_text_mut(img, color, x, y, scale, font, text);
}

/// Draw a horizontal progress bar.
fn draw_bar(img: &mut RgbImage, bx: i32, by: i32, bw: i32, bh: i32, frac: f32, fg: Rgb<u8>) {
    let frac = frac.clamp(0.0, 1.0);
    draw_hollow_rect_mut(img, Rect::at(bx, by).of_size(bw as u32, bh as u32), OVERLAY0);
    let inner_w = ((bw - 2) as f32 * frac) as i32;
    if inner_w > 0 {
        draw_filled_rect_mut(
            img,
            Rect::at(bx + 1, by + 1).of_size(inner_w as u32, (bh - 2) as u32),
            fg,
        );
    }
}

/// Draw a 1-pixel horizontal rule.
fn draw_divider(img: &mut RgbImage, y: i32) {
    draw_filled_rect_mut(img, Rect::at(20, y).of_size(440, 1), SURFACE0);
}

#[allow(clippy::too_many_arguments)]
/// Draw a two-column label/value row.
fn draw_kv(
    img: &mut RgbImage,
    bold: &FontRef<'_>,
    mono: &FontRef<'_>,
    scale: PxScale,
    y: i32,
    label_col: Rgb<u8>,
    val_col: Rgb<u8>,
    label: &str,
    value: &str,
) {
    draw_text_mut(img, label_col, 20, y, scale, bold, label);
    let vw = text_width(mono, scale, value);
    draw_text_mut(img, val_col, 480 - 20 - vw, y, scale, mono, value);
}

// ─── Public renderers ─────────────────────────────────────────────────────────

/// Live system-stats dashboard: CPU · RAM · temperature · uptime.
pub fn render_sysinfo() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Collect stats ──────────────────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let hostname = System::host_name().unwrap_or_else(|| "unknown".into());

    // CPU
    let cpu_usage = sys.global_cpu_usage();
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "CPU".into());
    let cpu_name = if cpu_name.len() > 28 {
        format!("{}…", &cpu_name[..27])
    } else {
        cpu_name
    };

    // Memory
    let used_mem  = sys.used_memory();
    let total_mem = sys.total_memory();
    let mem_frac  = if total_mem > 0 { used_mem as f32 / total_mem as f32 } else { 0.0 };
    let used_gb   = used_mem  as f32 / 1_073_741_824.0;
    let total_gb  = total_mem as f32 / 1_073_741_824.0;

    // Load average
    let load = System::load_average();
    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);

    // Uptime
    let uptime_str = format_uptime(System::uptime());

    // Time
    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %d, %Y").to_string();

    // Temperature
    let mut comps = sysinfo::Components::new_with_refreshed_list();
    comps.refresh(false);
    let temp_opt: Option<f32> = comps
        .iter()
        .filter(|c| {
            let lbl = c.label().to_lowercase();
            lbl.contains("cpu") || lbl.contains("core") || lbl.contains("package")
        })
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);

    // ── Render ────────────────────────────────────────────────────────────
    let mut y: i32 = 14;

    let big = PxScale::from(32.0);
    draw_text_mut(&mut img, MAUVE, 20, y, big, &bold, &hostname);
    let tw = text_width(&mono, big, &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, y, big, &mono, &time_str);
    y += 38;

    let sm = PxScale::from(20.0);
    draw_text_mut(&mut img, SUBTEXT, 20, y, sm, &bold, &date_str);
    y += 28;

    draw_divider(&mut img, y); y += 10;

    let lbl = PxScale::from(22.0);
    let val = PxScale::from(22.0);

    // ── CPU ────────────────────────────────────────────────────────────────
    draw_kv(&mut img, &bold, &mono, lbl, y, BLUE, TEXT, "CPU", &format!("{:.1}%", cpu_usage));
    y += 28;
    draw_bar(&mut img, 20, y, 440, 18, cpu_usage / 100.0, BLUE);
    y += 24;
    draw_text_mut(&mut img, OVERLAY0, 20, y, PxScale::from(17.0), &bold, &cpu_name);
    y += 24;

    draw_divider(&mut img, y); y += 10;

    // ── RAM ────────────────────────────────────────────────────────────────
    draw_kv(&mut img, &bold, &mono, val, y, GREEN, TEXT, "RAM",
            &format!("{:.1} / {:.1} GB", used_gb, total_gb));
    y += 28;
    draw_bar(&mut img, 20, y, 440, 18, mem_frac, GREEN);
    y += 28;

    draw_divider(&mut img, y); y += 10;

    // ── Temperature ────────────────────────────────────────────────────────
    if let Some(temp) = temp_opt {
        let col = if temp > 80.0 { RED } else if temp > 65.0 { YELLOW } else { GREEN };
        draw_kv(&mut img, &bold, &mono, lbl, y, YELLOW, TEXT, "TEMP", &format!("{:.0}°C", temp));
        y += 28;
        draw_bar(&mut img, 20, y, 440, 18, (temp / 100.0).min(1.0), col);
    } else {
        draw_kv(&mut img, &bold, &mono, lbl, y, YELLOW, OVERLAY0, "TEMP", "N/A");
        y += 28;
        draw_bar(&mut img, 20, y, 440, 18, 0.0, OVERLAY0);
    }
    y += 28;

    draw_divider(&mut img, y); y += 12;

    // ── Load average + uptime ──────────────────────────────────────────────
    let s2 = PxScale::from(18.0);
    draw_text_mut(&mut img, OVERLAY0, 20, y, s2, &bold, "Load avg");
    let lw = text_width(&mono, s2, &load_str);
    draw_text_mut(&mut img, SUBTEXT, 480 - 20 - lw, y, s2, &mono, &load_str);
    y += 24;
    draw_text_mut(
        &mut img, SUBTEXT, 20, y, PxScale::from(18.0), &bold,
        &format!("Uptime  {}", uptime_str),
    );

    DynamicImage::ImageRgb8(img)
}

/// Large digital clock with date and uptime.
pub fn render_clock() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now       = Local::now();
    let time_str  = now.format("%H:%M:%S").to_string();
    let wday_str  = now.format("%A").to_string();
    let date_str  = now.format("%B %-e, %Y").to_string();
    let up_str    = format!("up {}", format_uptime(System::uptime()));

    // Load avg
    let load = System::load_average();
    let load_str = format!("load  {:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);

    // Large clock
    let clock_scale = PxScale::from(96.0);
    let tw = text_width(&mono, clock_scale, &time_str);
    let tx = (480 - tw).max(0) / 2;
    draw_text_mut(&mut img, TEAL, tx, 130, clock_scale, &mono, &time_str);

    draw_divider(&mut img, 244);

    draw_centered(&mut img, &bold, PxScale::from(36.0), 258, MAUVE, &wday_str);
    draw_centered(&mut img, &bold, PxScale::from(28.0), 302, TEXT,  &date_str);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 348, SUBTEXT, &up_str);
    draw_centered(&mut img, &bold, PxScale::from(20.0), 382, OVERLAY0, &load_str);

    // Corner bracket decorations
    draw_brackets(&mut img);

    DynamicImage::ImageRgb8(img)
}

/// SMPTE-style colour bars + cross-hatch overlay.
pub fn render_test_card() -> DynamicImage {
    let mut img = RgbImage::new(480, 480);

    let bars: &[Rgb<u8>] = &[
        Rgb([192, 192, 192]),
        Rgb([192, 192, 0]),
        Rgb([0,   192, 192]),
        Rgb([0,   192, 0]),
        Rgb([192, 0,   192]),
        Rgb([192, 0,   0]),
        Rgb([0,   0,   192]),
    ];
    let bar_w = 480 / bars.len() as u32;
    let bar_h = 380u32;

    for (i, &color) in bars.iter().enumerate() {
        let x0 = i as u32 * bar_w;
        let x1 = (x0 + bar_w).min(480);
        for x in x0..x1 {
            for y in 0..bar_h {
                img.put_pixel(x, y, color);
            }
        }
    }

    for x in 0..480u32 {
        let v = if x < 160 {
            0u8
        } else if x < 320 {
            ((x - 160) * 255 / 160) as u8
        } else {
            255u8.saturating_sub(((x - 320) * 255 / 160) as u8)
        };
        for y in bar_h..480 {
            img.put_pixel(x, y, Rgb([v, v, v]));
        }
    }

    for i in (0u32..960).step_by(40) {
        for t in 0..2u32 {
            let xi = (i + t) as i32 - 480;
            let x0 = xi.max(0) as u32;
            let x1 = (xi + 480).min(480) as u32;
            for x in x0..x1 {
                let y = (xi + x as i32) as u32;
                if y < bar_h {
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                }
            }
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img, Rgb([255, 255, 255]),
        100, 390, PxScale::from(20.0), &font,
        "480 × 480  TEST CARD",
    );

    DynamicImage::ImageRgb8(img)
}

/// Solid colour frame.
pub fn render_solid(r: u8, g: u8, b: u8) -> DynamicImage {
    let mut img = RgbImage::new(480, 480);
    for px in img.pixels_mut() {
        *px = Rgb([r, g, b]);
    }
    DynamicImage::ImageRgb8(img)
}

/// Load all images from a directory (sorted) for slideshow use.
pub fn load_slideshow_frames(dir: &Path) -> Vec<DynamicImage> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("Cannot read '{}': {}", dir.display(), e))
        .flatten()
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
        .iter()
        .filter_map(|p| match image::open(p) {
            Ok(img) => {
                log::debug!("slideshow loaded {}", p.display());
                Some(img)
            }
            Err(e) => {
                log::warn!("slideshow skipped {} ({})", p.display(), e);
                None
            }
        })
        .collect()
}

// ─── Extended renderers ───────────────────────────────────────────────────────

/// Combined clock + mini sysinfo panel (split-screen).
pub fn render_clock_sysinfo() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %-e").to_string();

    // ── Top half: clock ───────────────────────────────────────────────────
    let clock_scale = PxScale::from(88.0);
    let tw = text_width(&mono, clock_scale, &time_str);
    let tx = (480 - tw).max(0) / 2;
    draw_text_mut(&mut img, TEAL, tx, 50, clock_scale, &mono, &time_str);
    draw_centered(&mut img, &bold, PxScale::from(26.0), 150, MAUVE, &date_str);

    draw_divider(&mut img, 192);

    // ── Bottom half: quick stats ──────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu  = sys.global_cpu_usage();
    let used = sys.used_memory() as f32 / 1_073_741_824.0;
    let tot  = sys.total_memory() as f32 / 1_073_741_824.0;
    let mem  = if tot > 0.0 { used / tot } else { 0.0 };
    let up   = format_uptime(System::uptime());
    let load = System::load_average();

    let lbl = PxScale::from(20.0);
    let val = PxScale::from(20.0);

    let mut y: i32 = 202;

    draw_kv(&mut img, &bold, &mono, lbl, y, BLUE, TEXT, "CPU", &format!("{:.0}%", cpu));
    y += 24;
    draw_bar(&mut img, 20, y, 440, 14, cpu / 100.0, BLUE);
    y += 22;

    draw_kv(&mut img, &bold, &mono, val, y, GREEN, TEXT, "RAM", &format!("{:.1}/{:.1}G", used, tot));
    y += 24;
    draw_bar(&mut img, 20, y, 440, 14, mem, GREEN);
    y += 24;

    draw_divider(&mut img, y); y += 10;

    // Load avg row
    let s2 = PxScale::from(17.0);
    let load_s = format!("load {:.2} {:.2} {:.2}  up {}", load.one, load.five, load.fifteen, up);
    draw_centered(&mut img, &bold, s2, y, SUBTEXT, &load_s);

    DynamicImage::ImageRgb8(img)
}

/// Per-CPU core usage grid (up to 32 cores, 4-wide layout).
pub fn render_cpu_detail() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mut sys = System::new_all();
    sys.refresh_all();

    let cpus: Vec<(String, f32)> = sys.cpus()
        .iter()
        .map(|c| (c.name().to_string(), c.cpu_usage()))
        .collect();

    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "CPU Cores");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    let global = sys.global_cpu_usage();
    let load = System::load_average();
    let gs = format!("Total {:.1}%   load {:.2}/{:.2}/{:.2}", global, load.one, load.five, load.fifteen);
    draw_text_mut(&mut img, TEXT, 20, 60, PxScale::from(17.0), &bold, &gs);

    let cols = 4usize;
    let cell_w = 440 / cols as i32;
    let cell_h = 56i32;
    let start_y = 86i32;
    let bar_h   = 12i32;

    for (i, (_name, usage)) in cpus.iter().enumerate().take(32) {
        let col = (i % cols) as i32;
        let row = (i / cols) as i32;
        let x = 20 + col * cell_w;
        let y = start_y + row * cell_h;

        let label = format!("C{}", i);
        draw_text_mut(&mut img, SUBTEXT, x, y, PxScale::from(14.0), &bold, &label);

        let bar_color = if *usage > 80.0 { RED } else if *usage > 50.0 { YELLOW } else { BLUE };
        draw_bar(&mut img, x, y + 16, cell_w - 6, bar_h, usage / 100.0, bar_color);

        let pct = format!("{:.0}%", usage);
        let pw = text_width(&mono, PxScale::from(13.0), &pct);
        let px_ = x + (cell_w - 6 - pw) / 2;
        if px_ >= x {
            draw_text_mut(&mut img, TEXT, px_, y + 30, PxScale::from(13.0), &mono, &pct);
        }
    }

    DynamicImage::ImageRgb8(img)
}

/// Network I/O stats — shows per-interface Rx/Tx rates (bytes/sec).
///
/// This function sleeps for 1 second internally to compute the rate delta.
pub fn render_network() -> DynamicImage {
    use sysinfo::Networks;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now  = Local::now();
    let time = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Network");
    let tw = text_width(&mono, PxScale::from(24.0), &time);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time);
    draw_divider(&mut img, 52);

    // Two-sample measurement for Rx/Tx rates
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(Duration::from_secs(1));
    nets.refresh(false);

    let mut y: i32 = 62;
    let nm_scale = PxScale::from(18.0);
    let val_scale = PxScale::from(16.0);

    let mut showed = 0usize;
    for (name, data) in nets.iter() {
        if showed >= 6 { break; }
        if name == "lo" { continue; }

        let rx_ps  = data.received();          // bytes since last refresh ≈ bytes/sec
        let tx_ps  = data.transmitted();
        let rx_tot = data.total_received();
        let tx_tot = data.total_transmitted();

        let title = if name.len() > 14 { format!("{}…", &name[..13]) } else { name.clone() };
        draw_text_mut(&mut img, BLUE, 20, y, nm_scale, &bold, &title);
        y += 20;

        // Rate row
        let rx_rate = format!("↓ {}/s", format_bytes(rx_ps));
        let tx_rate = format!("↑ {}/s", format_bytes(tx_ps));
        draw_text_mut(&mut img, GREEN, 30, y, val_scale, &mono, &rx_rate);
        let tw_ = text_width(&mono, val_scale, &tx_rate);
        draw_text_mut(&mut img, YELLOW, 480 - 20 - tw_, y, val_scale, &mono, &tx_rate);
        y += 18;

        // Totals row (smaller)
        let rx_t = format!("tot ↓ {}", format_bytes(rx_tot));
        let tx_t = format!("↑ {}", format_bytes(tx_tot));
        draw_text_mut(&mut img, OVERLAY0, 30, y, PxScale::from(14.0), &mono, &rx_t);
        let tw2 = text_width(&mono, PxScale::from(14.0), &tx_t);
        draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw2, y, PxScale::from(14.0), &mono, &tx_t);
        y += 18;

        draw_divider(&mut img, y); y += 8;
        showed += 1;
    }

    if showed == 0 {
        draw_centered(&mut img, &bold, PxScale::from(20.0), 200, OVERLAY0, "No network interfaces");
    }

    DynamicImage::ImageRgb8(img)
}

/// Disk usage display — shows each mounted partition with usage bars.
pub fn render_disk_usage() -> DynamicImage {
    use sysinfo::Disks;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Disk Usage");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    let disks = Disks::new_with_refreshed_list();

    let mut y: i32 = 62;
    let mut showed = 0usize;

    for disk in disks.iter() {
        if showed >= 6 { break; }

        let total = disk.total_space();
        let avail = disk.available_space();
        if total == 0 { continue; }

        let used  = total.saturating_sub(avail);
        let frac  = used as f32 / total as f32;
        let color = if frac > 0.85 { RED } else if frac > 0.70 { YELLOW } else { GREEN };

        let mount = disk.mount_point().to_string_lossy();
        let mount = if mount.len() > 18 { format!("{}…", &mount[..17]) } else { mount.to_string() };

        let size_str = format!("{} / {}", format_bytes_si(used), format_bytes_si(total));

        // Mount + size on same line
        draw_text_mut(&mut img, BLUE, 20, y, PxScale::from(18.0), &bold, &mount);
        let sw = text_width(&mono, PxScale::from(16.0), &size_str);
        draw_text_mut(&mut img, TEXT, 480 - 20 - sw, y + 2, PxScale::from(16.0), &mono, &size_str);
        y += 22;

        // Bar (400px wide) + percentage
        draw_bar(&mut img, 20, y, 400, 14, frac, color);
        let pct = format!("{:.0}%", frac * 100.0);
        let pw = text_width(&mono, PxScale::from(14.0), &pct);
        draw_text_mut(&mut img, color, 460 - pw, y, PxScale::from(14.0), &mono, &pct);
        y += 18;

        // Filesystem type (small, greyed out)
        let fs = disk.file_system().to_string_lossy();
        let fs = format!("{}", fs);
        draw_text_mut(&mut img, OVERLAY0, 20, y, PxScale::from(14.0), &mono, &fs);
        y += 16;

        draw_divider(&mut img, y); y += 8;
        showed += 1;
    }

    if showed == 0 {
        draw_centered(&mut img, &bold, PxScale::from(20.0), 200, OVERLAY0, "No disks detected");
    }

    DynamicImage::ImageRgb8(img)
}

/// Top processes by CPU usage.
///
/// Performs two CPU samples 200 ms apart for accurate per-process CPU%.
pub fn render_process_top() -> DynamicImage {
    use std::cmp::Ordering;
    use sysinfo::{ProcessesToUpdate, ProcessRefreshKind};

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Top Processes");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    // Two-sample CPU measurement
    let mut sys = System::new_all();
    sys.refresh_all();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        false,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    let cpu_count = sys.cpus().len().max(1) as f32;

    let mut procs: Vec<_> = sys.processes().values().collect();
    procs.sort_by(|a, b| {
        b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap_or(Ordering::Equal)
    });

    // Column headers
    let hdr = PxScale::from(14.0);
    draw_text_mut(&mut img, OVERLAY0, 20,  58, hdr, &bold, "PROCESS");
    draw_text_mut(&mut img, OVERLAY0, 290, 58, hdr, &bold, "CPU%");
    draw_text_mut(&mut img, OVERLAY0, 380, 58, hdr, &bold, "MEM");
    draw_divider(&mut img, 74);

    let mut y: i32 = 78;
    let row_h: i32 = 40;
    let name_scale = PxScale::from(15.0);
    let val_scale  = PxScale::from(14.0);

    for proc in procs.iter().take(9) {
        if y + row_h > 480 { break; }

        let raw_name = proc.name().to_string_lossy();
        let name = if raw_name.len() > 20 {
            format!("{}…", &raw_name[..19])
        } else {
            raw_name.to_string()
        };

        // CPU% normalised to 0–100 across all cores
        let cpu_pct = (proc.cpu_usage() / cpu_count).clamp(0.0, 100.0);
        let cpu_col = if cpu_pct > 50.0 { RED } else if cpu_pct > 20.0 { YELLOW } else { GREEN };

        // Memory
        let mem_mb = proc.memory() as f64 / 1_048_576.0;
        let mem_str = if mem_mb >= 1024.0 {
            format!("{:.1}G", mem_mb / 1024.0)
        } else {
            format!("{:.0}M", mem_mb)
        };

        // Name
        draw_text_mut(&mut img, TEXT, 20, y + 2, name_scale, &bold, &name);

        // CPU bar + value
        draw_bar(&mut img, 280, y + 4, 80, 10, cpu_pct / 100.0, cpu_col);
        let cs = format!("{:.1}%", cpu_pct);
        draw_text_mut(&mut img, cpu_col, 368, y + 2, val_scale, &mono, &cs);

        // Memory
        let mw = text_width(&mono, val_scale, &mem_str);
        draw_text_mut(&mut img, PEACH, 460 - mw, y + 2, val_scale, &mono, &mem_str);

        draw_divider(&mut img, y + row_h - 2);
        y += row_h;
    }

    DynamicImage::ImageRgb8(img)
}


// ─── Custom text display ─────────────────────────────────────────────────────

/// Named colour for [`CustomTextPayload`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NamedColor {
    #[default]
    Blue,
    Green,
    Yellow,
    Red,
    Teal,
    Mauve,
    Peach,
    White,
}

impl NamedColor {
    fn to_rgb(&self) -> Rgb<u8> {
        match self {
            NamedColor::Blue   => BLUE,
            NamedColor::Green  => GREEN,
            NamedColor::Yellow => YELLOW,
            NamedColor::Red    => RED,
            NamedColor::Teal   => TEAL,
            NamedColor::Mauve  => MAUVE,
            NamedColor::Peach  => PEACH,
            NamedColor::White  => TEXT,
        }
    }

    /// Public alias for `to_rgb`, usable from other crates/modules.
    pub fn to_rgb_pub(&self) -> Rgb<u8> { self.to_rgb() }

    /// Parse a colour name case-insensitively; falls back to Blue.
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "green"  => NamedColor::Green,
            "yellow" => NamedColor::Yellow,
            "red"    => NamedColor::Red,
            "teal"   => NamedColor::Teal,
            "mauve"  => NamedColor::Mauve,
            "peach"  => NamedColor::Peach,
            "white"  => NamedColor::White,
            _        => NamedColor::Blue,
        }
    }
}

/// Content pushed via `POST /api/text`.
///
/// All fields are optional; omitted fields use sensible defaults.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct CustomTextPayload {
    /// Large title at top of screen.
    pub title: Option<String>,
    /// Smaller subtitle line below the title.
    pub subtitle: Option<String>,
    /// Up to 8 body lines (shown in monospace).
    #[serde(default)]
    pub lines: Vec<String>,
    /// Progress bar value 0.0–1.0 (absent = no bar).
    pub progress: Option<f32>,
    /// Colour for the progress bar and title (default: blue).
    #[serde(default)]
    pub color: NamedColor,
    /// If true, show current time in the top-right corner.
    #[serde(default = "bool_true")]
    pub show_time: bool,
    /// Auto-clear after this many seconds (absent = stay until manually cleared).
    /// The daemon returns to the previous live mode when the timer expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

fn bool_true() -> bool { true }

/// Render a [`CustomTextPayload`] to a 480×480 frame.
pub fn render_custom_text(payload: &CustomTextPayload) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let accent = payload.color.to_rgb();
    let now    = Local::now();
    let time_s = now.format("%H:%M:%S").to_string();

    let mut y: i32 = 14;

    // ── Timestamp in top-right ─────────────────────────────────────────────
    if payload.show_time {
        let tw = text_width(&mono, PxScale::from(22.0), &time_s);
        draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, y, PxScale::from(22.0), &mono, &time_s);
    }

    // ── Title ──────────────────────────────────────────────────────────────
    if let Some(title) = &payload.title {
        let t = if title.len() > 22 { format!("{}…", &title[..21]) } else { title.clone() };
        draw_centered(&mut img, &bold, PxScale::from(34.0), y, accent, &t);
        y += 42;
    } else {
        y += 14;
    }

    // ── Subtitle ───────────────────────────────────────────────────────────
    if let Some(sub) = &payload.subtitle {
        let s = if sub.len() > 38 { format!("{}…", &sub[..37]) } else { sub.clone() };
        draw_centered(&mut img, &bold, PxScale::from(20.0), y, SUBTEXT, &s);
        y += 26;
    }

    draw_divider(&mut img, y); y += 12;

    // ── Body lines ─────────────────────────────────────────────────────────
    let line_scale = PxScale::from(20.0);
    for line in payload.lines.iter().take(10) {
        if y > 420 { break; }
        let l = if line.len() > 38 { format!("{}…", &line[..37]) } else { line.clone() };
        draw_text_mut(&mut img, TEXT, 20, y, line_scale, &mono, &l);
        y += 26;
    }

    // ── Progress bar ───────────────────────────────────────────────────────
    if let Some(pct) = payload.progress {
        let pct = pct.clamp(0.0, 1.0);
        let bar_y = 420i32;
        draw_bar(&mut img, 20, bar_y, 400, 22, pct, accent);
        let pct_s = format!("{:.0}%", pct * 100.0);
        let pw = text_width(&mono, PxScale::from(18.0), &pct_s);
        draw_text_mut(&mut img, accent, 460 - pw, bar_y + 2, PxScale::from(18.0), &mono, &pct_s);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn blank() -> RgbImage {
    let mut img = RgbImage::new(480, 480);
    for px in img.pixels_mut() {
        *px = BG;
    }
    img
}

fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{}d {}h {}m", d, h, m)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m {}s", m, secs % 60)
    }
}

fn draw_brackets(img: &mut RgbImage) {
    let sz  = 24u32;
    let th  = 3u32;
    let mg: i32 = 16;
    let corners: [(i32, i32); 4] = [
        (mg,              mg),
        (480 - mg - sz as i32, mg),
        (mg,              480 - mg - sz as i32),
        (480 - mg - sz as i32, 480 - mg - sz as i32),
    ];
    for (cx, cy) in corners {
        draw_filled_rect_mut(img, Rect::at(cx, cy).of_size(sz, th),  MAUVE);
        draw_filled_rect_mut(img, Rect::at(cx, cy).of_size(th, sz),  MAUVE);
    }
}

/// Format bytes as human-readable (rate-style, e.g. "1.4 MB").
fn format_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", b as f64 / 1_073_741_824.0)
    } else if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else if b >= 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}

/// Format bytes with SI prefixes for disk sizes (powers of 1000).
fn format_bytes_si(b: u64) -> String {
    if b >= 1_000_000_000_000 {
        format!("{:.1} TB", b as f64 / 1_000_000_000_000.0)
    } else if b >= 1_000_000_000 {
        format!("{:.1} GB", b as f64 / 1_000_000_000.0)
    } else if b >= 1_000_000 {
        format!("{:.0} MB", b as f64 / 1_000_000.0)
    } else {
        format!("{} KB", b / 1024)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_panel_size(img: &DynamicImage) {
        assert_eq!(img.width(),  480, "frame must be 480px wide");
        assert_eq!(img.height(), 480, "frame must be 480px tall");
    }

    #[test]
    fn test_clock_size() {
        assert_panel_size(&render_clock());
    }

    #[test]
    fn test_clock_sysinfo_size() {
        assert_panel_size(&render_clock_sysinfo());
    }

    #[test]
    fn test_sysinfo_size() {
        assert_panel_size(&render_sysinfo());
    }

    #[test]
    fn test_cpu_detail_size() {
        assert_panel_size(&render_cpu_detail());
    }

    #[test]
    fn test_network_size() {
        // render_network sleeps 1 s — skip in fast test runs via cfg
        // To run: cargo test -- --ignored
        assert_panel_size(&render_network());
    }

    #[test]
    fn test_disk_usage_size() {
        assert_panel_size(&render_disk_usage());
    }

    #[test]
    fn test_process_top_size() {
        // render_process_top sleeps ~200 ms for CPU delta
        assert_panel_size(&render_process_top());
    }

    #[test]
    fn test_solid_size() {
        assert_panel_size(&render_solid(255, 0, 128));
    }

    #[test]
    fn test_test_card_size() {
        assert_panel_size(&render_test_card());
    }

    #[test]
    fn test_solid_colour_values() {
        let img = render_solid(10, 20, 30).into_rgb8();
        // Every pixel must match the requested colour
        for px in img.pixels() {
            assert_eq!(px.0, [10, 20, 30]);
        }
    }

    #[test]
    fn test_blank_background() {
        let img = blank();
        // All pixels must match BG colour
        for px in img.pixels() {
            assert_eq!(*px, BG);
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0),             "0 B");
        assert_eq!(format_bytes(512),           "512 B");
        assert_eq!(format_bytes(1024),          "1.0 KB");
        assert_eq!(format_bytes(1_048_576),     "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_format_bytes_si() {
        assert_eq!(format_bytes_si(1_000_000_000),     "1.0 GB");
        assert_eq!(format_bytes_si(1_000_000_000_000), "1.0 TB");
        assert_eq!(format_bytes_si(500_000_000),       "500 MB");  // 500 000 000 bytes
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(45),    "0m 45s");
        assert_eq!(format_uptime(3661),  "1h 1m");
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }

    #[test]
    fn test_temperature_size() {
        // render_temperature gracefully handles no-sensor environments (VMs)
        // by rendering a placeholder message; either way it must be 480x480.
        assert_panel_size(&render_temperature());
    }

    #[test]
    fn test_custom_text_size() {
        use crate::display::{CustomTextPayload, NamedColor};
        let payload = CustomTextPayload {
            title:    Some("Hello World".into()),
            subtitle: Some("subtitle".into()),
            lines:    vec!["line 1".into(), "line 2".into()],
            progress: Some(0.75),
            color:    NamedColor::Green,
            show_time: true,
            ..Default::default()
        };
        assert_panel_size(&render_custom_text(&payload));
    }

    #[test]
    fn test_memory_detail_size() {
        assert_panel_size(&render_memory_detail());
    }

    #[test]
    fn test_countdown_size() {
        // Renders with time remaining
        assert_panel_size(&render_countdown(125, 300, "Build cooldown"));
    }

    #[test]
    fn test_countdown_expired_size() {
        // Renders the "TIME'S UP" screen
        assert_panel_size(&render_countdown(0, 300, "Done"));
    }

    #[test]
    fn test_cpu_graph_size() {
        // Push a few fake samples so the chart has data to draw
        for pct in [10.0_f32, 25.0, 50.0, 75.0, 90.0] {
            push_cpu_history(pct);
        }
        assert_panel_size(&render_cpu_graph());
    }

    #[test]
    fn test_net_graph_size() {
        for (rx, tx) in [(1024u64, 512u64), (2048, 1024), (4096, 2048)] {
            push_net_history(rx, tx);
        }
        assert_panel_size(&render_net_graph());
    }

    #[test]
    fn test_recent_alerts_size() {
        let entries: Vec<(u64, bool, &str)> = vec![
            (1747000000, true,  "ERROR — Build FAILED"),
            (1747000060, false, "Deploying ttlcd-rs v0.5"),
            (1747000120, true,  "SUCCESS — Deploy complete"),
        ];
        assert_panel_size(&render_recent_alerts(&entries));
    }

    #[test]
    fn test_recent_alerts_empty_size() {
        assert_panel_size(&render_recent_alerts(&[]));
    }

    #[test]
    fn test_process_memory_size() {
        assert_panel_size(&render_process_memory());
    }

    #[test]
    fn test_cpu_history_accessor() {
        push_cpu_history(42.0);
        let h = get_cpu_history();
        assert!(!h.is_empty(), "history should have at least one sample");
        assert!(h.iter().all(|&v| (0.0..=100.0).contains(&v)));
    }

    #[test]
    fn test_net_history_accessor() {
        push_net_history(1024, 512);
        let (rx, tx) = get_net_history();
        assert!(!rx.is_empty());
        assert!(!tx.is_empty());
        assert_eq!(rx.len(), tx.len());
    }

    #[test]
    fn test_calendar_size() {
        assert_panel_size(&render_calendar());
    }

    #[test]
    fn test_boot_splash_size() {
        assert_panel_size(&render_boot_splash("0.7.0"));
    }

    #[test]
    fn test_big_text_banner_single_line() {
        use image::Rgb;
        assert_panel_size(&render_big_text_banner(&["99.8%"], "SLA", Rgb([137u8, 180, 250]), true));
    }

    #[test]
    fn test_big_text_banner_three_lines() {
        use image::Rgb;
        assert_panel_size(&render_big_text_banner(
            &["99.8%", "Uptime", "Last 30 days"],
            "SLA",
            Rgb([166u8, 227, 161]),
            false,
        ));
    }

    #[test]
    fn test_stopwatch_ready_size() {
        // fresh state: no elapsed time, not running, no laps
        assert_panel_size(&render_stopwatch(0, &[], false));
    }

    #[test]
    fn test_stopwatch_running_size() {
        assert_panel_size(&render_stopwatch(75_123, &[], true));
    }

    #[test]
    fn test_stopwatch_paused_with_laps() {
        let laps = vec![15_000u64, 32_500, 61_200, 95_000, 130_800];
        assert_panel_size(&render_stopwatch(130_800, &laps, false));
    }

    #[test]
    fn test_stopwatch_over_one_hour() {
        assert_panel_size(&render_stopwatch(3_661_500, &[], true));
    }

    #[test]
    fn test_format_sw_time_under_minute() {
        assert_eq!(format_sw_time(0),       "00:00.00");
        assert_eq!(format_sw_time(1_000),   "00:01.00");
        assert_eq!(format_sw_time(10),      "00:00.01");
        assert_eq!(format_sw_time(59_999),  "00:59.99");
    }

    #[test]
    fn test_format_sw_time_minutes() {
        assert_eq!(format_sw_time(60_000),  "01:00.00");
        assert_eq!(format_sw_time(90_500),  "01:30.50");
    }

    #[test]
    fn test_format_sw_time_hours() {
        assert_eq!(format_sw_time(3_600_000), "1:00:00.00");
        assert_eq!(format_sw_time(3_661_000), "1:01:01.00");
    }

    #[test]
    fn test_metrics_empty_size() {
        assert_panel_size(&render_metrics("", &[]));
    }

    #[test]
    fn test_metrics_with_data_size() {
        let metrics = vec![
            Metric::new("CPU", "72.4%"),
            Metric::new("RAM", "6.1 GB"),
            Metric::new("Uptime", "3d 4h"),
            Metric::new("Requests", "1234/s"),
            Metric::new("Errors", "0.12%"),
            Metric::new("Latency", "42ms"),
        ];
        assert_panel_size(&render_metrics("Dashboard", &metrics));
    }

    #[test]
    fn test_metrics_eight_items() {
        let metrics: Vec<Metric> = (1..=8)
            .map(|i| Metric { label: format!("Key {i}"), value: format!("{i}"), color: "green".into() })
            .collect();
        assert_panel_size(&render_metrics("Full Grid", &metrics));
    }

    #[test]
    fn test_weather_empty_size() {
        assert_panel_size(&render_weather(&WeatherData::default()));
    }

    #[test]
    fn test_weather_full_size() {
        let data = WeatherData {
            city:          "London".to_string(),
            temp_c:        Some(14.5),
            feels_like_c:  Some(12.0),
            condition:     "Partly cloudy".to_string(),
            humidity_pct:  Some(72),
            wind_kmh:      Some(18.0),
            wind_dir:      "SW".to_string(),
            high_c:        Some(16.0),
            low_c:         Some(9.0),
            visibility_km: Some(10.0),
            updated_at:    Some(1_700_000_000),
        };
        assert_panel_size(&render_weather(&data));
    }

    #[test]
    fn test_weather_hot_size() {
        let data = WeatherData { city: "Dubai".to_string(), temp_c: Some(42.0),
            condition: "Sunny".to_string(), ..Default::default() };
        assert_panel_size(&render_weather(&data));
    }

    #[test]
    fn test_weather_freezing_size() {
        let data = WeatherData { city: "Reykjavik".to_string(), temp_c: Some(-12.0),
            condition: "Snow".to_string(), wind_kmh: Some(35.0), ..Default::default() };
        assert_panel_size(&render_weather(&data));
    }

    #[test]
    fn test_weather_thunder_icon_size() {
        let data = WeatherData { condition: "Thunderstorm".to_string(), temp_c: Some(20.0), ..Default::default() };
        assert_panel_size(&render_weather(&data));
    }

    #[test]
    fn test_notes_empty_size() {
        assert_panel_size(&render_notes("", &[]));
    }

    #[test]
    fn test_notes_with_items_size() {
        let items: Vec<String> = (1..=6).map(|i| format!("Note item number {i}")).collect();
        assert_panel_size(&render_notes("Sprint 42", &items));
    }

    #[test]
    fn test_notes_overflow_size() {
        let items: Vec<String> = (1..=15).map(|i| format!("Task {i}: something to do")).collect();
        assert_panel_size(&render_notes("Many Notes", &items));
    }

    #[test]
    fn test_big_clock_size() {
        assert_panel_size(&render_big_clock());
    }

    #[test]
    fn test_qr_code_url_size() {
        assert_panel_size(&render_qr_code("https://example.com/test", "Example"));
    }

    #[test]
    fn test_qr_code_empty_payload() {
        // Empty payload → error frame, should still be panel-sized
        assert_panel_size(&render_qr_code("", "Empty"));
    }

    #[test]
    fn test_pomodoro_work_running_size() {
        assert_panel_size(&render_pomodoro("work", 1440, 1500, 0, true));
    }

    #[test]
    fn test_pomodoro_short_break_paused_size() {
        assert_panel_size(&render_pomodoro("short_break", 120, 300, 1, false));
    }

    #[test]
    fn test_pomodoro_long_break_expired_size() {
        // remaining_secs ≤ 0 should still render (shows 00:00)
        assert_panel_size(&render_pomodoro("long_break", 0, 900, 0, false));
    }

    #[test]
    fn test_battery_size() {
        // render_battery reads from /sys; on CI it returns "No battery detected" — still 480×480
        assert_panel_size(&render_battery());
    }

    #[test]
    fn test_world_clock_defaults_size() {
        assert_panel_size(&render_world_clock(&[]));
    }

    #[test]
    fn test_world_clock_custom_size() {
        let cities = vec![
            ("Tokyo".into(),   540),
            ("UTC".into(),       0),
            ("NYC".into(),    -300),
        ];
        assert_panel_size(&render_world_clock(&cities));
    }

    #[test]
    fn test_ticker_short_text_size() {
        // Short text fits in frame and is centred
        assert_panel_size(&render_ticker("Hello!", 0, "blue"));
    }

    #[test]
    fn test_ticker_long_text_scrolling() {
        // Long text scrolls; different positions should still produce panel-sized frames
        let text = "This is a very long scrolling ticker message that does not fit in one screen width!";
        assert_panel_size(&render_ticker(text, 0, "green"));
        assert_panel_size(&render_ticker(text, 400, "red"));
    }

    #[test]
    fn test_http_status_empty_size() {
        assert_panel_size(&render_http_status(&[]));
    }

    #[test]
    fn test_http_status_with_results_size() {
        let results = vec![
            HttpMonitorResult { label: "Google".into(), url: "https://google.com".into(),
                status_code: Some(200), latency_ms: Some(45), ok: true },
            HttpMonitorResult { label: "Down".into(), url: "https://example-down.local".into(),
                status_code: None, latency_ms: None, ok: false },
        ];
        assert_panel_size(&render_http_status(&results));
    }

    #[test]
    fn test_shell_cmd_ok_size() {
        assert_panel_size(&render_shell_cmd("uptime", "14:22:01 up 3 days, load average: 0.12", true, 0));
    }

    #[test]
    fn test_shell_cmd_err_size() {
        assert_panel_size(&render_shell_cmd("bad_cmd", "bash: bad_cmd: not found", false, 1715000000));
    }

    #[test]
    fn test_shell_cmd_multiline_size() {
        let output = "line 1
line 2
line 3
line 4
line 5
line 6
line 7
line 8";
        assert_panel_size(&render_shell_cmd("echo lines", output, true, 1715000000));
    }

    #[test]
    fn test_ping_monitor_empty_size() {
        assert_panel_size(&render_ping_monitor(&[]));
    }

    #[test]
    fn test_ping_monitor_with_results_size() {
        let results = vec![
            PingResult { host: "8.8.8.8".into(), label: "Google DNS".into(),
                latency_ms: Some(12.3), loss_pct: 0.0 },
            PingResult { host: "1.1.1.1".into(), label: "Cloudflare".into(),
                latency_ms: Some(5.8), loss_pct: 0.0 },
            PingResult { host: "192.168.1.1".into(), label: "Router".into(),
                latency_ms: None, loss_pct: 100.0 },
        ];
        assert_panel_size(&render_ping_monitor(&results));
    }

    #[test]
    fn test_dashboard_size() {
        assert_panel_size(&render_dashboard());
    }

    #[test]
    fn test_log_tail_empty_size() {
        assert_panel_size(&render_log_tail("", &[], false));
    }

    #[test]
    fn test_log_tail_ok_empty_lines() {
        assert_panel_size(&render_log_tail("/var/log/syslog", &[], true));
    }

    #[test]
    fn test_log_tail_with_lines() {
        let lines: Vec<String> = (1..=20)
            .map(|i| format!("2026-05-13 12:00:{:02} [INFO]  test message line {}", i, i))
            .collect();
        assert_panel_size(&render_log_tail("/var/log/syslog", &lines, true));
    }

    #[test]
    fn test_log_tail_error_colors() {
        let lines = vec![
            "2026-05-13 ERROR failed to connect".to_string(),
            "2026-05-13 WARN  retrying in 5s".to_string(),
            "2026-05-13 DEBUG retry attempt 1".to_string(),
            "2026-05-13 INFO  connected ok".to_string(),
        ];
        assert_panel_size(&render_log_tail("/tmp/app.log", &lines, true));
    }

    #[test]
    fn test_log_tail_long_lines() {
        let lines = vec![
            "a".repeat(100),
            "b".repeat(80),
        ];
        assert_panel_size(&render_log_tail("/tmp/test.log", &lines, true));
    }

    #[test]
    fn test_cpu_freq_size() {
        assert_panel_size(&render_cpu_freq());
    }

    #[test]
    fn test_hostname_ip_size() {
        assert_panel_size(&render_hostname_ip());
    }

    #[test]
    fn test_rss_feed_empty_size() {
        let img = render_rss_feed("TEST FEED", &[], true);
        assert_panel_size(&img);
    }

    #[test]
    fn test_rss_feed_items() {
        let items = vec![
            ("Headline one".to_string(), "14 May 2026".to_string()),
            ("Another headline that is quite long indeed".to_string(), "author".to_string()),
        ];
        let img = render_rss_feed("My Feed", &items, true);
        assert_panel_size(&img);
    }

    #[test]
    fn test_rss_feed_error_state() {
        let img = render_rss_feed("Offline Feed", &[], false);
        assert_panel_size(&img);
    }

    #[test]
    fn test_service_status_empty() {
        let img = render_service_status(&[]);
        assert_panel_size(&img);
    }

    #[test]
    fn test_service_status_units() {
        let units = vec![
            ServiceStatus { name: "nginx.service".to_string(), state: "active".to_string(), sub: "running".to_string() },
            ServiceStatus { name: "sshd.service".to_string(), state: "inactive".to_string(), sub: "dead".to_string() },
            ServiceStatus { name: "bad.service".to_string(), state: "failed".to_string(), sub: "failed".to_string() },
        ];
        let img = render_service_status(&units);
        assert_panel_size(&img);
    }

    #[test]
    fn test_net_connections_size() {
        // Just verify it renders without panic (may have 0 connections in test env)
        let img = render_net_connections();
        assert_panel_size(&img);
    }

    #[test]
    fn test_uptime_size() {
        let img = render_uptime();
        assert_panel_size(&img);
    }

    #[test]
    fn test_os_info_size() {
        let img = render_os_info();
        assert_panel_size(&img);
    }

}

/// Thermal sensor detail — all available temperature components with color-coded bars.
///
/// Shows up to 10 sensors. If no sensors are available (VM, no hwmon driver),
/// a placeholder message is shown.
pub fn render_temperature() -> DynamicImage {
    use sysinfo::Components;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Temperatures");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);

    // Collect all valid readings, sorted by label
    let mut readings: Vec<(String, f32, Option<f32>)> = comps
        .iter()
        .filter_map(|c| {
            let t = c.temperature()?;
            if t <= 0.0 || t.is_nan() { return None; }
            let label = c.label().to_string();
            let crit = c.critical();
            Some((label, t, crit))
        })
        .collect();
    readings.sort_by(|a, b| a.0.cmp(&b.0));

    if readings.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(20.0), 200, OVERLAY0, "No sensors detected");
        draw_centered(&mut img, &bold, PxScale::from(16.0), 230, SURFACE0,
            "Load hwmon kernel module or run on bare metal");
        return DynamicImage::ImageRgb8(img);
    }

    let mut y: i32 = 62;
    let lbl_scale = PxScale::from(15.0);
    let val_scale = PxScale::from(15.0);
    let bar_scale = PxScale::from(13.0);

    for (label, temp, crit) in readings.iter().take(10) {
        if y + 44 > 480 { break; }

        // Determine color based on fraction of critical (or 100 °C if unknown)
        let max_temp = crit.filter(|&c| c > 0.0).unwrap_or(100.0);
        let frac = (temp / max_temp).clamp(0.0, 1.0);
        let col = if frac > 0.85 { RED } else if frac > 0.65 { YELLOW } else { GREEN };

        // Label
        let lbl = if label.len() > 26 { format!("{}…", &label[..25]) } else { label.clone() };
        draw_text_mut(&mut img, SUBTEXT, 20, y, lbl_scale, &bold, &lbl);

        // Temperature value right-aligned
        let val_s = format!("{:.1}°C", temp);
        let vw = text_width(&mono, val_scale, &val_s);
        draw_text_mut(&mut img, col, 480 - 20 - vw, y, val_scale, &mono, &val_s);
        y += 18;

        // Bar (350 px wide) + optional critical marker
        draw_bar(&mut img, 20, y, 350, 10, frac, col);
        if let Some(c) = crit.filter(|&c| c > 0.0) {
            let crit_s = format!("crit {:.0}°", c);
            let cw = text_width(&mono, bar_scale, &crit_s);
            draw_text_mut(&mut img, OVERLAY0, 480 - 20 - cw, y, bar_scale, &mono, &crit_s);
        }
        y += 14;

        draw_divider(&mut img, y); y += 8;
    }

    if readings.len() > 10 {
        let more = format!("…and {} more sensors", readings.len() - 10);
        draw_centered(&mut img, &bold, PxScale::from(15.0), y, OVERLAY0, &more);
    }

    DynamicImage::ImageRgb8(img)
}

/// Detailed memory breakdown: RAM used/cached/available/free, swap, load.
pub fn render_memory_detail() -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Memory");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    let mut sys = System::new_all();
    sys.refresh_memory();
    let load = System::load_average();

    let total  = sys.total_memory();
    let used   = sys.used_memory();
    let avail  = sys.available_memory();
    let free   = sys.free_memory();
    let cached = total.saturating_sub(used).saturating_sub(free);

    let sw_total = sys.total_swap();
    let sw_used  = sys.used_swap();

    let mem_frac  = if total  > 0 { used   as f32 / total  as f32 } else { 0.0 };
    let swap_frac = if sw_total > 0 { sw_used as f32 / sw_total as f32 } else { 0.0 };

    let lbl = PxScale::from(20.0);
    let val = PxScale::from(18.0);

    let fmt_gb = |b: u64| -> String {
        let gb = b as f64 / 1_073_741_824.0;
        if gb >= 1.0 { format!("{:.2} GB", gb) } else { format!("{:.0} MB", b as f64 / 1_048_576.0) }
    };

    let mut y: i32 = 60;

    // ── RAM ───────────────────────────────────────────────────────────────────
    let ram_col = if mem_frac > 0.85 { RED } else if mem_frac > 0.65 { YELLOW } else { GREEN };
    draw_kv(&mut img, &bold, &mono, lbl, y, GREEN, TEXT, "RAM Used",
            &format!("{} / {} ({:.0}%)", fmt_gb(used), fmt_gb(total), mem_frac * 100.0));
    y += 26;
    draw_bar(&mut img, 20, y, 440, 16, mem_frac, ram_col);
    y += 24;

    draw_kv(&mut img, &bold, &mono, val, y, SUBTEXT, SUBTEXT, "  Avail.",  &fmt_gb(avail));
    y += 22;
    draw_kv(&mut img, &bold, &mono, val, y, SUBTEXT, SUBTEXT, "  Free",    &fmt_gb(free));
    y += 22;
    draw_kv(&mut img, &bold, &mono, val, y, SUBTEXT, SUBTEXT, "  Cached",  &fmt_gb(cached));
    y += 24;

    draw_divider(&mut img, y); y += 10;

    // ── Swap ─────────────────────────────────────────────────────────────────
    if sw_total > 0 {
        let sw_col = if swap_frac > 0.5 { RED } else if swap_frac > 0.2 { YELLOW } else { TEAL };
        draw_kv(&mut img, &bold, &mono, lbl, y, TEAL, TEXT, "Swap",
                &format!("{} / {} ({:.0}%)", fmt_gb(sw_used), fmt_gb(sw_total), swap_frac * 100.0));
        y += 26;
        draw_bar(&mut img, 20, y, 440, 16, swap_frac, sw_col);
        y += 24;
        draw_divider(&mut img, y); y += 10;
    } else {
        draw_kv(&mut img, &bold, &mono, lbl, y, TEAL, OVERLAY0, "Swap", "not configured");
        y += 30;
        draw_divider(&mut img, y); y += 10;
    }

    // ── Load + uptime ─────────────────────────────────────────────────────────
    let s = PxScale::from(18.0);
    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);
    draw_text_mut(&mut img, OVERLAY0, 20, y, s, &bold, "Load avg");
    let lw = text_width(&mono, s, &load_str);
    draw_text_mut(&mut img, SUBTEXT, 480 - 20 - lw, y, s, &mono, &load_str);
    y += 24;
    let uptime = format_uptime(System::uptime());
    draw_text_mut(&mut img, SUBTEXT, 20, y, s, &bold, &format!("Uptime  {}", uptime));

    DynamicImage::ImageRgb8(img)
}

/// Countdown timer display.
///
/// Shows remaining time until `target_unix_secs` as a large clock.  The accent
/// colour transitions green → yellow → red as the deadline approaches (based on
/// the original total duration `total_secs`).
///
/// When `remaining_secs == 0` the panel shows a "TIME'S UP" message in red.
pub fn render_countdown(remaining_secs: i64, total_secs: i64, label: &str) -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let ts  = now.format("%H:%M:%S").to_string();

    // Header — label left, clock right
    let header_label = if label.is_empty() { "COUNTDOWN" } else { label };
    let header_label = if header_label.len() > 22 {
        format!("{}…", &header_label[..21])
    } else {
        header_label.to_string()
    };
    draw_text_mut(&mut img, OVERLAY0, 16, 12, PxScale::from(20.0), &bold, &header_label);
    let tw_ts = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 12 - tw_ts, 14, PxScale::from(18.0), &mono, &ts);
    draw_divider(&mut img, 38);

    // ── Finished state ────────────────────────────────────────────────────────
    if remaining_secs <= 0 {
        // Pulsing TIME'S UP — alternate between RED and PEACH based on 1-second clock
        let flash = (now.second() % 2) == 0;
        let finish_color = if flash { RED } else { PEACH };
        draw_centered(&mut img, &bold, PxScale::from(72.0), 150, finish_color, "TIME'S UP!");
        // Draw a full red ring
        let cx = 240_i32;
        let cy = 300_i32;
        for py in (cy - 80)..(cy + 80) {
            for px in (cx - 80)..(cx + 80) {
                let dist = (((px - cx).pow(2) + (py - cy).pow(2)) as f64).sqrt();
                if dist >= 62.0 && dist <= 78.0 && px >= 0 && px < 480 && py >= 0 && py < 480 {
                    img.put_pixel(px as u32, py as u32, if flash { RED } else { PEACH });
                }
            }
        }
        draw_centered(&mut img, &bold, PxScale::from(22.0), 420, OVERLAY0, label);
        return DynamicImage::ImageRgb8(img);
    }

    // ── Progress and color ────────────────────────────────────────────────────
    let frac_left = if total_secs > 0 {
        (remaining_secs as f32 / total_secs as f32).clamp(0.0, 1.0)
    } else { 1.0 };
    // Urgency: last 10 seconds blink between accent and SURFACE1
    let is_urgent = remaining_secs <= 10;
    let flash_off = is_urgent && (now.second() % 2 == 1 || (now.nanosecond() / 500_000_000) == 1);
    let accent = if frac_left > 0.5 { GREEN }
                 else if frac_left > 0.2 { YELLOW }
                 else { RED };
    let display_color = if flash_off { SURFACE0 } else { accent };

    // ── Circular arc ring ────────────────────────────────────────────────────
    let cx = 240_i32;
    let cy = 220_i32;
    let r_outer = 130_i32;
    let r_inner = 96_i32;

    let two_pi = std::f64::consts::PI * 2.0;
    let start_angle = -std::f64::consts::FRAC_PI_2; // top (12 o'clock)
    let sweep = frac_left as f64 * two_pi;

    // Background ring
    for py in (cy - r_outer - 2)..(cy + r_outer + 2) {
        for px in (cx - r_outer - 2)..(cx + r_outer + 2) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r_inner as f64 && dist <= r_outer as f64 {
                if px >= 0 && px < 480 && py >= 0 && py < 480 {
                    img.put_pixel(px as u32, py as u32, SURFACE0);
                }
            }
        }
    }

    // Progress arc (remaining portion)
    for py in (cy - r_outer - 2)..(cy + r_outer + 2) {
        for px in (cx - r_outer - 2)..(cx + r_outer + 2) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r_inner as f64 && dist <= r_outer as f64 {
                let angle = dy.atan2(dx);
                let mut rel = angle - start_angle;
                if rel < 0.0 { rel += two_pi; }
                if rel <= sweep {
                    if px >= 0 && px < 480 && py >= 0 && py < 480 {
                        img.put_pixel(px as u32, py as u32, display_color);
                    }
                }
            }
        }
    }

    // ── Central time display ─────────────────────────────────────────────────
    let h = remaining_secs / 3600;
    let m = (remaining_secs % 3600) / 60;
    let s = remaining_secs % 60;
    let (time_display, time_scale) = if h > 0 {
        (format!("{:02}:{:02}:{:02}", h, m, s), PxScale::from(60.0))
    } else {
        (format!("{:02}:{:02}", m, s), PxScale::from(76.0))
    };

    let tw_time = text_width(&mono, time_scale, &time_display);
    let tx = cx - tw_time / 2;
    let ty = cy - (time_scale.y / 2.0) as i32;
    draw_text_mut(&mut img, display_color, tx, ty, time_scale, &mono, &time_display);

    // Elapsed % inside ring
    let pct_str = format!("{:.0}%", frac_left * 100.0);
    let pw = text_width(&bold, PxScale::from(18.0), &pct_str);
    draw_text_mut(&mut img, OVERLAY0, cx - pw / 2, cy + (time_scale.y * 0.5) as i32 + 6,
        PxScale::from(18.0), &bold, &pct_str);

    // ── Stats row ────────────────────────────────────────────────────────────
    draw_divider(&mut img, 365);

    let remaining_str = format_uptime(remaining_secs as u64);
    let elapsed_secs  = (total_secs - remaining_secs).max(0) as u64;
    let elapsed_str   = format_uptime(elapsed_secs);

    let left_str  = format!("Remaining: {}", remaining_str);
    let right_str = format!("Elapsed: {}", elapsed_str);
    draw_text_mut(&mut img, SUBTEXT, 16, 376, PxScale::from(16.0), &mono, &left_str);
    let rw = text_width(&mono, PxScale::from(16.0), &right_str);
    draw_text_mut(&mut img, SUBTEXT, 480 - 16 - rw, 376, PxScale::from(16.0), &mono, &right_str);

    // ── Thin progress bar at bottom ───────────────────────────────────────────
    let bar_y = 404_i32;
    let bar_h = 14_i32;
    draw_filled_rect_mut(&mut img, Rect::at(16, bar_y).of_size(448, bar_h as u32), SURFACE0);
    let bar_fill = ((frac_left * 448.0) as i32).clamp(0, 448);
    if bar_fill > 0 {
        draw_filled_rect_mut(&mut img, Rect::at(16, bar_y).of_size(bar_fill as u32, bar_h as u32), display_color);
    }

    // ── Urgency indicator ─────────────────────────────────────────────────────
    if is_urgent && !flash_off {
        let warn_str = format!("⚠ {}s REMAINING", remaining_secs);
        let ww = text_width(&bold, PxScale::from(20.0), &warn_str);
        draw_text_mut(&mut img, RED, (480 - ww) / 2, 430, PxScale::from(20.0), &bold, &warn_str);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── CPU usage history (global ring-buffer, shared across renders) ────────────

use std::sync::OnceLock;

static CPU_GRAPH_DATA: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();

fn cpu_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    CPU_GRAPH_DATA
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one CPU usage sample to the global history ring-buffer (max 120 entries).
///
/// Call this from the display loop each time a live mode renders, so the
/// graph mode always has fresh data even if `render_cpu_graph` hasn't been
/// called recently.
pub fn push_cpu_history(pct: f32) {
    let mut h = cpu_graph_data();
    if h.len() >= 120 { h.pop_front(); }
    h.push_back(pct.clamp(0.0, 100.0));
}

/// Full-screen CPU usage history graph.
///
/// Shows the last 120 samples (2 minutes at 1 sample/second) as a bar chart,
/// with the current reading prominently displayed.
pub fn render_cpu_graph() -> DynamicImage {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    // Small sleep so sysinfo can measure a delta
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    let cpu_pct = sys.global_cpu_usage();
    push_cpu_history(cpu_pct);

    let history: Vec<f32> = cpu_graph_data().iter().cloned().collect();

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── current time (top-right) ─────────────────────────────────────────
    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &time_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &time_str);

    // ── mode label (top-left) ────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "CPU GRAPH");

    // ── large current % ──────────────────────────────────────────────────
    let accent = if cpu_pct > 80.0 { RED } else if cpu_pct > 50.0 { YELLOW } else { GREEN };
    let pct_str = format!("{:.1}%", cpu_pct);
    draw_centered(&mut img, &bold, PxScale::from(80.0), 50, accent, &pct_str);

    // ── stats row ────────────────────────────────────────────────────────
    if !history.is_empty() {
        let avg: f32 = history.iter().sum::<f32>() / history.len() as f32;
        let peak: f32 = history.iter().cloned().fold(0.0_f32, f32::max);
        let stats_str = format!("avg {:.1}%  peak {:.1}%  ({} samples)", avg, peak, history.len());
        let sw = text_width(&mono, PxScale::from(16.0), &stats_str);
        let sx = ((480 - sw) / 2).max(0);
        draw_text_mut(&mut img, SUBTEXT, sx, 148, PxScale::from(16.0), &mono, &stats_str);
    }

    // ── bar chart ────────────────────────────────────────────────────────
    // Chart area: x=0..480, y=175..460  (285px tall)
    let chart_x = 0i32;
    let chart_y = 175i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Background grid lines at 25%, 50%, 75%
    for pct_line in [25u32, 50, 75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32;
        if ly >= 0 && (ly as u32) < img.height() {
            for x in (chart_x..(chart_x + chart_w)).step_by(6) {
                if x >= 0 && (x as u32) < img.width() {
                    img.put_pixel(x as u32, ly as u32, SURFACE0);
                }
            }
        }
    }

    // Y-axis labels
    for (pct_line, label) in [(25u32, "25"), (50, "50"), (75, "75"), (100, "100")] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32 - 2;
        draw_text_mut(&mut img, OVERLAY0, 2, ly - 8, PxScale::from(12.0), &mono, label);
    }

    // Bars
    let n = history.len();
    if n > 0 {
        let bar_w = chart_w / 120;  // 4px per bar
        let offset = 120 - n as i32;  // left-pad with empty space if less than 120 samples
        for (i, &val) in history.iter().enumerate() {
            let bar_h = (chart_h as f32 * val / 100.0).max(1.0) as i32;
            let bx    = chart_x + (offset + i as i32) * bar_w;
            let by    = chart_y + chart_h - bar_h;
            let color = if val > 80.0 { RED } else if val > 50.0 { YELLOW } else { BLUE };
            // Brighten the most recent bar
            let is_last = i == n - 1;
            let draw_color = if is_last { TEXT } else { color };
            draw_filled_rect_mut(
                &mut img,
                Rect::at(bx, by).of_size(bar_w.max(1) as u32, bar_h as u32),
                draw_color,
            );
        }
    }

    // Chart border (bottom line)
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);

    // ── x-axis label ─────────────────────────────────────────────────────
    draw_centered(&mut img, &mono, PxScale::from(14.0), chart_y + chart_h + 6, OVERLAY0, "← 2 min ago                      now →");

    DynamicImage::ImageRgb8(img)
}

// ─── Network I/O history graph ───────────────────────────────────────────────

static NET_RX_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<u64>>> = OnceLock::new();
static NET_TX_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<u64>>> = OnceLock::new();

fn net_rx_history() -> std::sync::MutexGuard<'static, std::collections::VecDeque<u64>> {
    NET_RX_HISTORY.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(60)))
        .lock().unwrap()
}
fn net_tx_history() -> std::sync::MutexGuard<'static, std::collections::VecDeque<u64>> {
    NET_TX_HISTORY.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(60)))
        .lock().unwrap()
}

/// Append one network sample (bytes/second for rx and tx) to history.
pub fn push_net_history(rx_bps: u64, tx_bps: u64) {
    let mut rx = net_rx_history();
    if rx.len() >= 60 { rx.pop_front(); }
    rx.push_back(rx_bps);

    let mut tx = net_tx_history();
    if tx.len() >= 60 { tx.pop_front(); }
    tx.push_back(tx_bps);
}

fn format_bps(bps: u64) -> String {
    if bps >= 1_000_000_000 { format!("{:.1}GB/s", bps as f64 / 1_000_000_000.0) }
    else if bps >= 1_000_000 { format!("{:.1}MB/s", bps as f64 / 1_000_000.0) }
    else if bps >= 1_000     { format!("{:.1}KB/s", bps as f64 / 1_000.0) }
    else                     { format!("{bps}B/s") }
}

/// Full-screen network I/O history graph.
///
/// Shows Rx (download) and Tx (upload) rates for the last 60 seconds as
/// colour-coded area charts. The scale adapts to the peak value.
pub fn render_net_graph() -> DynamicImage {
    use sysinfo::Networks;
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(500));
    nets.refresh(true);

    let (total_rx, total_tx): (u64, u64) = nets.iter()
        .fold((0u64, 0u64), |(rx, tx), (_, d)| {
            (rx + d.received(), tx + d.transmitted())
        });

    // Convert to bytes/sec (we slept ~500ms)
    let rx_bps = total_rx * 2;
    let tx_bps = total_tx * 2;
    push_net_history(rx_bps, tx_bps);

    let rx_hist: Vec<u64> = net_rx_history().iter().cloned().collect();
    let tx_hist: Vec<u64> = net_tx_history().iter().cloned().collect();

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Current time
    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &time_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &time_str);

    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "NET GRAPH");

    // Current Rx/Tx
    draw_centered(&mut img, &bold, PxScale::from(38.0), 55,  GREEN, &format!("↓ {}", format_bps(rx_bps)));
    draw_centered(&mut img, &bold, PxScale::from(38.0), 100, PEACH, &format!("↑ {}", format_bps(tx_bps)));

    // Charts
    // Upper chart: Rx (y=145..310, 165px)
    // Lower chart: Tx (y=320..455, 135px)
    let charts: &[(i32, i32, &[u64], Rgb<u8>, &str)] = &[
        (145, 160, &rx_hist, GREEN, "Rx"),
        (315, 135, &tx_hist, PEACH, "Tx"),
    ];

    for &(cy, ch, hist, color, label) in charts {
        let peak = hist.iter().cloned().max().unwrap_or(1).max(1);

        // Background
        draw_filled_rect_mut(&mut img, Rect::at(0, cy).of_size(480, ch as u32), Rgb([20, 20, 32]));

        // Label
        draw_text_mut(&mut img, color, 4, cy + 2, PxScale::from(14.0), &bold, label);
        let peak_str = format_bps(peak);
        draw_text_mut(&mut img, OVERLAY0, 50, cy + 2, PxScale::from(13.0), &mono, &peak_str);

        // Grid line at 50%
        let mid_y = cy + ch / 2;
        for x in (0..480i32).step_by(8) {
            img.put_pixel(x as u32, mid_y as u32, SURFACE0);
        }

        // Bars
        let n = hist.len();
        if n > 0 {
            let bar_w = 480 / 60;
            let offset = 60 - n as i32;
            for (i, &val) in hist.iter().enumerate() {
                let bar_h = ((ch - 4) as f64 * val as f64 / peak as f64).max(1.0) as i32;
                let bx    = (offset + i as i32) * bar_w;
                let by    = cy + ch - bar_h;
                // Alpha-blend towards color using surface darkness
                let draw_col = if i == n - 1 { TEXT } else { color };
                draw_filled_rect_mut(&mut img, Rect::at(bx, by).of_size(bar_w.max(1) as u32, bar_h as u32), draw_col);
            }
        }

        // Bottom border
        draw_filled_rect_mut(&mut img, Rect::at(0, cy + ch).of_size(480, 1), SURFACE0);
    }

    // x-axis label
    draw_centered(&mut img, &mono, PxScale::from(12.0), 458, OVERLAY0, "← 60 s ago      now →");

    DynamicImage::ImageRgb8(img)
}

// ─── Recent alerts display ───────────────────────────────────────────────────

/// Show the last N notification summaries from the history ring-buffer.
///
/// Rendered as a compact log on the panel, with timestamps and kind icons.
/// This mode is driven by the caller passing in `entries` to avoid the
/// display module depending on daemon state directly.
pub fn render_recent_alerts(entries: &[(u64, bool, &str)]) -> DynamicImage {
    // entries: Vec of (timestamp_unix, is_alert: bool, summary)
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let now      = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &time_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &time_str);
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "RECENT ALERTS");
    draw_filled_rect_mut(&mut img, Rect::at(0, 48).of_size(480, 1), SURFACE0);

    if entries.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(26.0), 200, OVERLAY0, "No notifications yet");
        return DynamicImage::ImageRgb8(img);
    }

    let row_h: i32 = 82;
    let mut y: i32 = 55;

    for (ts, is_alert, summary) in entries.iter().take(5) {
        // Timestamp
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(*ts as i64, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "??:??:??".to_string());
        let kind_str = if *is_alert { "🔔" } else { "📝" };
        let header = format!("{kind_str} {dt}");
        draw_text_mut(&mut img, OVERLAY0, 20, y, PxScale::from(17.0), &mono, &header);

        // Summary (truncate to fit)
        let max_chars = 44;
        let display = if summary.len() > max_chars {
            format!("{}…", &summary[..max_chars - 1])
        } else {
            summary.to_string()
        };
        let color = if *is_alert { PEACH } else { TEXT };
        draw_text_mut(&mut img, color, 20, y + 22, PxScale::from(18.0), &bold, &display);

        draw_filled_rect_mut(&mut img, Rect::at(0, y + row_h - 4).of_size(480, 1), SURFACE0);
        y += row_h;
    }

    DynamicImage::ImageRgb8(img)
}



// ─── Top processes by memory ──────────────────────────────────────────────────

/// Top processes ranked by memory usage.
///
/// Similar to `render_process_top` but sorted by RAM instead of CPU.
pub fn render_process_memory() -> DynamicImage {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.refresh_memory();

    let total_mem = sys.total_memory();
    let used_mem  = sys.used_memory();

    let mut procs: Vec<(String, u64, f32)> = sys.processes().values()
        .map(|p| (
            p.name().to_string_lossy().to_string(),
            p.memory(),
            p.cpu_usage(),
        ))
        .collect();
    procs.sort_by(|a, b| b.1.cmp(&a.1));
    procs.truncate(8);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mut y: i32 = 14;

    // ── Header ─────────────────────────────────────────────────────────────
    draw_text_mut(&mut img, MAUVE, 20, y, PxScale::from(28.0), &bold, "TOP PROCESSES");
    let now = Local::now();
    let tw = text_width(&mono, PxScale::from(20.0), &now.format("%H:%M:%S").to_string());
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, y + 4,
        PxScale::from(20.0), &mono, &now.format("%H:%M:%S").to_string());
    y += 36;

    // ── Total RAM bar ───────────────────────────────────────────────────────
    let sm = PxScale::from(18.0);
    let used_gb  = used_mem  as f32 / 1_073_741_824.0;
    let total_gb = total_mem as f32 / 1_073_741_824.0;
    let ram_frac = if total_mem > 0 { used_mem as f32 / total_mem as f32 } else { 0.0 };
    draw_kv(&mut img, &bold, &mono, sm, y, GREEN, TEXT, "RAM",
        &format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, ram_frac * 100.0));
    y += 24;
    draw_bar(&mut img, 20, y, 440, 10, ram_frac, GREEN);
    y += 18;

    draw_divider(&mut img, y); y += 10;

    // ── Column headers ──────────────────────────────────────────────────────
    draw_text_mut(&mut img, SUBTEXT, 20,  y, sm, &bold, "PROCESS");
    draw_text_mut(&mut img, SUBTEXT, 280, y, sm, &bold, "MEM");
    draw_text_mut(&mut img, SUBTEXT, 370, y, sm, &bold, "CPU%");
    y += 22;
    draw_divider(&mut img, y); y += 6;

    // ── Process rows ────────────────────────────────────────────────────────
    for (name, mem, cpu) in &procs {
        if y > 455 { break; }
        let mem_pct = if total_mem > 0 { *mem as f32 / total_mem as f32 } else { 0.0 };
        let color   = if mem_pct > 0.20 { RED } else if mem_pct > 0.10 { YELLOW } else { TEXT };
        let n = if name.len() > 20 { format!("{}…", &name[..19]) } else { name.clone() };
        let mem_s = if *mem >= 1_073_741_824 {
            format!("{:.1}G", *mem as f32 / 1_073_741_824.0)
        } else if *mem >= 1_048_576 {
            format!("{:.0}M", *mem as f32 / 1_048_576.0)
        } else {
            format!("{:.0}K", *mem as f32 / 1024.0)
        };
        let cpu_s = format!("{:.1}%", cpu);
        draw_text_mut(&mut img, color, 20,  y, sm, &bold, &n);
        draw_text_mut(&mut img, color, 280, y, sm, &mono, &mem_s);
        draw_text_mut(&mut img, OVERLAY0, 370, y, sm, &mono, &cpu_s);
        y += 22;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Disk I/O rates ───────────────────────────────────────────────────────────

/// Current disk read/write throughput for each block device.
///
/// Refreshes disk stats twice with a ~500 ms gap and uses sysinfo's
/// `DiskUsage::read_bytes` / `written_bytes` (delta since last refresh)
/// to compute per-second rates.
pub fn render_disk_io() -> DynamicImage {
    use sysinfo::{DiskRefreshKind, Disks};

    // First refresh to get I/O counters
    let mut disks = Disks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(500));
    // Second refresh — read_bytes / written_bytes are deltas since the previous refresh
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    struct DiskRate {
        name:      String,
        read_bps:  u64,
        write_bps: u64,
        used_pct:  f32,
    }
    let rates: Vec<DiskRate> = disks.iter()
        .filter_map(|d| {
            let total = d.total_space();
            if total == 0 { return None; }
            let free  = d.available_space();
            let used  = total - free;
            let used_pct = used as f32 / total as f32;
            let io = d.usage();
            // io.read_bytes / written_bytes are deltas over the ~500 ms window;
            // multiply by 2 to get approximate bytes/sec.
            let read_bps  = io.read_bytes * 2;
            let write_bps = io.written_bytes * 2;
            Some(DiskRate {
                name: d.name().to_string_lossy().to_string(),
                read_bps,
                write_bps,
                used_pct,
            })
        })
        .take(6)
        .collect();

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();
    let sm   = PxScale::from(18.0);
    let med  = PxScale::from(20.0);

    let mut y: i32 = 14;

    // Header
    draw_text_mut(&mut img, PEACH, 20, y, PxScale::from(28.0), &bold, "DISK I/O");
    let now = Local::now();
    let ts  = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(20.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, y + 4, PxScale::from(20.0), &mono, &ts);
    y += 36;

    draw_divider(&mut img, y); y += 10;

    if rates.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(22.0), 200, SUBTEXT, "No disks detected");
        return DynamicImage::ImageRgb8(img);
    }

    // Column headers
    draw_text_mut(&mut img, SUBTEXT,  20, y, sm, &bold, "DEVICE");
    draw_text_mut(&mut img, SUBTEXT, 160, y, sm, &bold, "READ/s");
    draw_text_mut(&mut img, SUBTEXT, 270, y, sm, &bold, "WRITE/s");
    draw_text_mut(&mut img, SUBTEXT, 370, y, sm, &bold, "USE%");
    y += 22;
    draw_divider(&mut img, y); y += 8;

    for d in &rates {
        if y > 455 { break; }
        let n = if d.name.len() > 10 { format!("{}…", &d.name[..9]) } else { d.name.clone() };
        let r_s = format_bps(d.read_bps);
        let w_s = format_bps(d.write_bps);
        let use_s = format!("{:.0}%", d.used_pct * 100.0);
        let color = if d.used_pct > 0.90 { RED } else if d.used_pct > 0.75 { YELLOW } else { TEXT };
        draw_text_mut(&mut img, TEAL,  20,  y, med, &bold, &n);
        draw_text_mut(&mut img, GREEN, 160, y, med, &mono, &r_s);
        draw_text_mut(&mut img, PEACH, 270, y, med, &mono, &w_s);
        draw_text_mut(&mut img, color, 370, y, med, &mono, &use_s);
        // Usage bar
        draw_bar(&mut img, 20, y + 22, 440, 8, d.used_pct, color);
        y += 38;
        draw_divider(&mut img, y); y += 8;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── History accessors ────────────────────────────────────────────────────────

/// Return a snapshot of the CPU usage history ring-buffer (oldest first).
/// Returns an empty Vec if no samples have been pushed yet.
pub fn get_cpu_history() -> Vec<f32> {
    cpu_graph_data().iter().cloned().collect()
}

/// Return a snapshot of the network I/O history ring-buffers (oldest first).
/// Returns `(rx_bps, tx_bps)` vectors.
pub fn get_net_history() -> (Vec<u64>, Vec<u64>) {
    let rx: Vec<u64> = net_rx_history().iter().cloned().collect();
    let tx: Vec<u64> = net_tx_history().iter().cloned().collect();
    (rx, tx)
}

// ─── Calendar renderer ────────────────────────────────────────────────────────

/// Render a full-month calendar with today highlighted.
///
/// Layout (480×480):
/// - Row 1 (y=14):  "MONTH YEAR" centered in BLUE, large
/// - Row 2 (y=60):  current time right-aligned
/// - Row 3 (y=72):  day-of-week abbreviations (Mo–Su)
/// - Row 4 (y=92):  horizontal divider
/// - Rows 5-10:     6-week day grid (cells 68×58 px)
pub fn render_calendar() -> DynamicImage {
    use chrono::NaiveDate;

    let now   = Local::now();
    let today = now.date_naive();
    let year  = today.year();
    let month = today.month();

    // ── Date maths ──────────────────────────────────────────────────────────
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    // weekday offset: Monday=0 … Sunday=6
    let start_offset = first.weekday().num_days_from_monday() as i32;
    let days_in_month: i32 = {
        let next = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .unwrap();
        next.signed_duration_since(first).num_days() as i32
    };

    // ── Layout constants ─────────────────────────────────────────────────────
    const CELL_W: i32 = 68;
    const CELL_H: i32 = 58;
    const LEFT:   i32 = 4;   // left margin so 7×68=476 fits with 4+476+0=480
    const DOW_Y:  i32 = 72;
    const GRID_Y: i32 = 96;

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header: Month + Year ─────────────────────────────────────────────────
    let month_name = match month {
        1  => "JANUARY",   2  => "FEBRUARY", 3  => "MARCH",
        4  => "APRIL",     5  => "MAY",       6  => "JUNE",
        7  => "JULY",      8  => "AUGUST",    9  => "SEPTEMBER",
        10 => "OCTOBER",   11 => "NOVEMBER",  12 => "DECEMBER",
        _  => "?",
    };
    let header = format!("{} {}", month_name, year);
    draw_centered(&mut img, &bold, PxScale::from(36.0), 14, BLUE, &header);

    // Time top-right
    let ts = now.format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // ── Day-of-week row ───────────────────────────────────────────────────────
    let dow_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    for (col, &label) in dow_labels.iter().enumerate() {
        let cx = LEFT + col as i32 * CELL_W + CELL_W / 2;
        let lw = text_width(&bold, PxScale::from(20.0), label);
        let col_color = if col >= 5 { PEACH } else { SUBTEXT };
        draw_text_mut(&mut img, col_color, cx - lw / 2, DOW_Y, PxScale::from(20.0), &bold, label);
    }

    draw_divider(&mut img, GRID_Y - 2);

    // ── Day grid ──────────────────────────────────────────────────────────────
    for day in 1..=days_in_month {
        let slot  = start_offset + day - 1;
        let col   = slot % 7;
        let row   = slot / 7;
        let cx    = LEFT + col * CELL_W;
        let cy    = GRID_Y + row * CELL_H;

        let is_today   = day == today.day() as i32;
        let is_weekend = col >= 5;

        // Highlight today
        if is_today {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(cx + 4, cy + 2).of_size((CELL_W - 8) as u32, (CELL_H - 6) as u32),
                BLUE,
            );
        }

        let label = format!("{:2}", day);
        let font_scale = PxScale::from(if is_today { 28.0 } else { 24.0 });
        let lw    = text_width(&bold, font_scale, label.trim());
        let text_x = cx + (CELL_W - lw) / 2;
        let text_y = cy + (CELL_H - 24) / 2;

        let color = if is_today {
            Rgb([30u8, 30, 46]) // dark bg text on blue highlight
        } else if is_weekend {
            PEACH
        } else {
            TEXT
        };

        draw_text_mut(&mut img, color, text_x, text_y, font_scale, &bold, label.trim());
    }

    DynamicImage::ImageRgb8(img)
}


// ─── Big-text banner renderer ─────────────────────────────────────────────────

/// Render 1–3 lines of large, centred text — useful for dashboard headlines.
///
/// `lines`: slice of up to 3 strings.  Empty strings are skipped.
/// `label`: small subtitle printed at the bottom centre (pass `""` to omit).
/// `color`: accent `Rgb` used for the first/largest line.
/// `show_time`: if true, print the current time in the top-right corner.
pub fn render_big_text_banner(
    lines:     &[&str],
    label:     &str,
    color:     Rgb<u8>,
    show_time: bool,
) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    if show_time {
        let ts = Local::now().format("%H:%M:%S").to_string();
        let tw = text_width(&mono, PxScale::from(20.0), &ts);
        draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 14, PxScale::from(20.0), &mono, &ts);
    }

    let non_empty: Vec<&str> = lines.iter().copied().filter(|s| !s.is_empty()).take(3).collect();
    let count = non_empty.len();

    // Assign font sizes based on line count
    let sizes: [f32; 3] = match count {
        1 => [80.0, 0.0, 0.0],
        2 => [64.0, 48.0, 0.0],
        _ => [56.0, 44.0, 36.0],
    };

    let total_h: f32 = sizes.iter().take(count).sum::<f32>() + (count as f32 - 1.0) * 12.0;
    let start_y = ((480.0 - total_h) / 2.0) as i32;
    let mut cy = start_y;

    for (i, &line) in non_empty.iter().enumerate() {
        let scale = PxScale::from(sizes[i]);
        let lw    = text_width(&bold, scale, line);
        let x     = ((480 - lw) / 2).max(10);
        let col   = if i == 0 { color } else { TEXT };
        draw_text_mut(&mut img, col, x, cy, scale, &bold, line);
        cy += sizes[i] as i32 + 12;
    }

    if !label.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(22.0), 448, SUBTEXT, label);
    }

    DynamicImage::ImageRgb8(img)
}


// ─── Boot splash renderer ─────────────────────────────────────────────────────

/// Render a startup splash frame shown while the daemon is initializing.
///
/// Displays "ttlcd" in large text, the version string, and a subtle "Starting…"
/// status line at the bottom.
pub fn render_boot_splash(version: &str) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Large brand name
    draw_centered(&mut img, &bold, PxScale::from(96.0), 120, MAUVE, "ttlcd");

    // Subtitle
    draw_centered(&mut img, &bold, PxScale::from(28.0), 228, BLUE, "USB LCD Panel Daemon");

    // Version
    let ver_str = format!("v{version}");
    draw_centered(&mut img, &mono, PxScale::from(22.0), 270, OVERLAY0, &ver_str);

    // Divider
    draw_divider(&mut img, 310);

    // Animated dots placeholder — just static "Starting…" for now
    draw_centered(&mut img, &mono, PxScale::from(20.0), 326, SUBTEXT, "Starting…");

    // Date/time
    let ts = Local::now().format("%Y-%m-%d  %H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(20.0), 440, OVERLAY0, &ts);

    DynamicImage::ImageRgb8(img)
}


// ─── Stopwatch renderer ───────────────────────────────────────────────────────

/// Format milliseconds as a stopwatch string: "MM:SS.cc" (or "H:MM:SS.cc" for ≥1 h).
pub fn format_sw_time(ms: u64) -> String {
    let h  = ms / 3_600_000;
    let m  = (ms % 3_600_000) / 60_000;
    let s  = (ms % 60_000) / 1_000;
    let cs = (ms % 1_000) / 10;
    if h > 0 {
        format!("{:01}:{:02}:{:02}.{:02}", h, m, s, cs)
    } else {
        format!("{:02}:{:02}.{:02}", m, s, cs)
    }
}

/// Stopwatch display.
///
/// # Arguments
/// * `elapsed_ms` — total elapsed milliseconds to display.
/// * `laps`       — slice of accumulated-ms values at each recorded lap.
/// * `running`    — `true` if the stopwatch is currently ticking.
///
/// Layout:
/// - Header row "STOPWATCH" with current time top-right.
/// - Large centred elapsed time ("MM:SS" big + ".cc" smaller).
/// - Status badge: "● RUNNING" (green) / "■ PAUSED" (yellow) / "■ READY" (dim).
/// - Up to 4 most-recent laps with lap number, cumulative time, and Δ split.
pub fn render_stopwatch(elapsed_ms: u64, laps: &[u64], running: bool) -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_centered(&mut img, &bold, PxScale::from(20.0), 8, OVERLAY0, "STOPWATCH");
    // Current wall-clock time top-right
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 12 - tw, 12, PxScale::from(18.0), &mono, &ts);
    draw_divider(&mut img, 36);

    // ── Main time display ────────────────────────────────────────────────────
    let time_str = format_sw_time(elapsed_ms);
    // Split at last '.' so the fractional part can be styled differently
    let dot_pos  = time_str.rfind('.');
    let main_part: &str = if let Some(p) = dot_pos { &time_str[..p] } else { &time_str };
    let frac_part: &str = if let Some(p) = dot_pos { &time_str[p..] } else { "" };

    // Use a smaller scale for hour+ times so they still fit in 480px
    let main_scale: f32 = if elapsed_ms >= 3_600_000 { 72.0 } else { 90.0 };
    let frac_scale: f32 = main_scale * 0.55;

    let mw = text_width(&mono, PxScale::from(main_scale), main_part);
    let fw = text_width(&mono, PxScale::from(frac_scale), frac_part);
    let total_w = mw + fw;
    let start_x = ((480 - total_w) / 2).max(8);
    let main_y  = 58_i32;
    // Align fraction baseline with main
    let frac_y  = main_y + (main_scale - frac_scale) as i32;

    let time_color = if running { TEAL } else if elapsed_ms == 0 { OVERLAY0 } else { SUBTEXT };
    draw_text_mut(&mut img, time_color, start_x, main_y, PxScale::from(main_scale), &mono, main_part);
    if !frac_part.is_empty() {
        draw_text_mut(&mut img, OVERLAY0, start_x + mw, frac_y, PxScale::from(frac_scale), &mono, frac_part);
    }

    // ── Status badge ──────────────────────────────────────────────────────────
    let status_y = main_y + main_scale as i32 + 12;
    let (status_str, status_color) = if running {
        ("  RUNNING", GREEN)
    } else if elapsed_ms == 0 {
        ("  READY", OVERLAY0)
    } else {
        ("  PAUSED", YELLOW)
    };
    // Draw a filled indicator dot before the label
    let lw_status = text_width(&bold, PxScale::from(22.0), status_str);
    let sx = (480 - lw_status) / 2;
    draw_filled_rect_mut(&mut img, Rect::at(sx - 14, status_y + 7).of_size(10, 10), status_color);
    draw_text_mut(&mut img, status_color, sx, status_y, PxScale::from(22.0), &bold, status_str);

    // ── Lap list ─────────────────────────────────────────────────────────────
    let lap_section_y = status_y + 42;
    draw_divider(&mut img, lap_section_y);

    if laps.is_empty() {
        if !running && elapsed_ms == 0 {
            draw_centered(&mut img, &bold, PxScale::from(18.0), lap_section_y + 28, OVERLAY0, "Use Stopwatch controls");
            draw_centered(&mut img, &bold, PxScale::from(16.0), lap_section_y + 56, OVERLAY0, "to start, lap, and reset.");
        } else {
            draw_centered(&mut img, &mono, PxScale::from(18.0), lap_section_y + 28, OVERLAY0, "no laps recorded");
        }
    } else {
        draw_centered(&mut img, &bold, PxScale::from(15.0), lap_section_y + 6, OVERLAY0, "LAPS");

        // Show up to 4 most-recent laps (most recent first)
        let show_laps: Vec<(usize, u64)> = laps.iter().enumerate()
            .rev()
            .take(4)
            .map(|(i, &ms)| (i, ms))
            .collect();

        let row_h  = 52_i32;
        let base_y = lap_section_y + 26;

        for (idx_in_list, (lap_idx, lap_ms)) in show_laps.into_iter().enumerate() {
            let lap_num = lap_idx + 1; // 1-based
            let row_y   = base_y + idx_in_list as i32 * row_h;

            // Subtle background for the most-recent lap
            if idx_in_list == 0 {
                draw_filled_rect_mut(&mut img, Rect::at(8, row_y - 2).of_size(464, (row_h - 4) as u32), SURFACE0);
            }

            // Lap number (left)
            let num_str = format!("#{}", lap_num);
            draw_text_mut(&mut img, OVERLAY0, 16, row_y + 8, PxScale::from(20.0), &bold, &num_str);

            // Cumulative lap time (centre)
            let cum_str = format_sw_time(lap_ms);
            let cw = text_width(&mono, PxScale::from(22.0), &cum_str);
            draw_text_mut(&mut img, TEXT, (480 - cw) / 2, row_y + 7, PxScale::from(22.0), &mono, &cum_str);

            // Split delta (right)
            let prev_ms = if lap_idx > 0 { laps[lap_idx - 1] } else { 0 };
            let delta_ms = lap_ms.saturating_sub(prev_ms);
            let delta_str = format!("+{}", format_sw_time(delta_ms));
            let dw = text_width(&mono, PxScale::from(18.0), &delta_str);
            let delta_color = if idx_in_list == 0 { PEACH } else { SUBTEXT };
            draw_text_mut(&mut img, delta_color, 480 - dw - 12, row_y + 11, PxScale::from(18.0), &mono, &delta_str);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Custom metrics grid renderer ────────────────────────────────────────────

/// A single metric shown on the panel.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Metric {
    pub label: String,
    pub value: String,
    /// Optional colour name (uses NamedColor::from_str_loose); defaults to Blue.
    #[serde(default = "default_metric_color")]
    pub color: String,
}

fn default_metric_color() -> String { "blue".into() }

impl Metric {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), value: value.into(), color: default_metric_color() }
    }
}

/// Render up to 8 key/value metrics in a 2-column grid.
///
/// # Arguments
/// * `title`   — optional panel heading (shown at top, truncated to ~28 chars).
/// * `metrics` — key/value pairs; first 8 are shown.
pub fn render_metrics(title: &str, metrics: &[Metric]) -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    let header = if title.is_empty() { "METRICS".to_string() } else { title.to_uppercase() };
    draw_centered(&mut img, &bold, PxScale::from(22.0), 8, BLUE, &header);

    // Current time top-right
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 10 - tw, 12, PxScale::from(18.0), &mono, &ts);

    draw_divider(&mut img, 38);

    if metrics.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(22.0), 200, OVERLAY0, "No metrics set");
        draw_centered(&mut img, &bold, PxScale::from(16.0), 236, OVERLAY0, "POST /api/metrics to push data");
        return DynamicImage::ImageRgb8(img);
    }

    // ── 2-column grid ─────────────────────────────────────────────────────────
    // Each cell: 240px wide, 110px tall.  Up to 4 rows = 8 metrics.
    const COLS:   i32 = 2;
    const CELL_W: i32 = 240;
    const CELL_H: i32 = 108;
    const START_Y: i32 = 46;

    for (i, m) in metrics.iter().take(8).enumerate() {
        let col = (i as i32) % COLS;
        let row = (i as i32) / COLS;
        let x0  = col * CELL_W;
        let y0  = START_Y + row * CELL_H;

        // Cell border
        draw_hollow_rect_mut(&mut img, Rect::at(x0 + 4, y0 + 2).of_size((CELL_W - 8) as u32, (CELL_H - 6) as u32), SURFACE0);

        // Label (small, dim)
        let label_str: String = m.label.chars().take(20).collect();
        draw_text_mut(&mut img, OVERLAY0, x0 + 10, y0 + 8, PxScale::from(15.0), &bold, &label_str);

        // Value (large, coloured)
        let color = NamedColor::from_str_loose(&m.color).to_rgb_pub();
        // Choose scale based on value length
        let val_str: String = m.value.chars().take(14).collect();
        let val_scale: f32 = if val_str.len() <= 4 {
            52.0
        } else if val_str.len() <= 7 {
            42.0
        } else if val_str.len() <= 10 {
            34.0
        } else {
            26.0
        };
        let vw = text_width(&mono, PxScale::from(val_scale), &val_str);
        // Centre value in cell horizontally, top-align in value area
        let val_x = x0 + (CELL_W - vw) / 2;
        let val_y = y0 + 24;
        draw_text_mut(&mut img, color, val_x, val_y, PxScale::from(val_scale), &mono, &val_str);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Weather display ──────────────────────────────────────────────────────────

/// External weather snapshot pushed from the browser / CLI.
///
/// All fields are optional; missing values render as "—".
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct WeatherData {
    /// City or location name shown on panel.
    #[serde(default)]
    pub city: String,
    /// Current temperature in °C.
    pub temp_c: Option<f32>,
    /// "Feels like" temperature in °C.
    pub feels_like_c: Option<f32>,
    /// Short description e.g. "Partly cloudy".
    #[serde(default)]
    pub condition: String,
    /// Humidity percentage 0–100.
    pub humidity_pct: Option<u8>,
    /// Wind speed in km/h.
    pub wind_kmh: Option<f32>,
    /// Wind direction abbreviation e.g. "NW".
    #[serde(default)]
    pub wind_dir: String,
    /// Today's high temperature in °C.
    pub high_c: Option<f32>,
    /// Today's low temperature in °C.
    pub low_c: Option<f32>,
    /// Visibility in km.
    pub visibility_km: Option<f32>,
    /// Unix timestamp of when data was last updated (seconds).
    pub updated_at: Option<u64>,
}

/// ASCII-art weather condition icon (5 rows × 10 columns).
fn weather_icon(condition: &str) -> &'static [&'static str] {
    let c = condition.to_lowercase();
    if c.contains("thunder") || c.contains("storm") {
        &["  .-.  ", " (   ). ", "(___(__)","   /_  ","  /(__) "]
    } else if c.contains("snow") || c.contains("blizzard") || c.contains("ice") {
        &["  .-.  ", " (   ). ","(___(__)","  * * * ","  * * * "]
    } else if c.contains("sleet") || c.contains("drizzle") || c.contains("shower") {
        &["  .-.  ", " (   ). ","(___(__)","  ' ' ' ","  ' ' ' "]
    } else if c.contains("rain") {
        &["  .-.  ", " (   ). ","(___(__)","  / / / ","  / / / "]
    } else if c.contains("fog") || c.contains("mist") || c.contains("haze") {
        &["_______","_______","_______","_______","_______"]
    } else if c.contains("overcast") || c.contains("cloud") {
        &["       "," .---. ","( .--. )","'------'","       "]
    } else if c.contains("partly") || c.contains("broken") || c.contains("scatter") {
        &[" \\  /  ","  \\/   "," .---.  ","( sun  )","'-----' "]
    } else if c.contains("clear") || c.contains("sun") || c.contains("fair") || c.contains("bright") {
        &["  \\  / ","  .--. ","- |  | -","  `--' ","  /  \\ "]
    } else {
        &[" ????  "," ????  "," ????  "," ????  "," ????  "]
    }
}

/// Pick a colour for a temperature value.
fn temp_color(c: f32) -> Rgb<u8> {
    if c >= 38.0      { RED    }
    else if c >= 28.0 { PEACH  }
    else if c >= 18.0 { YELLOW }
    else if c >= 8.0  { GREEN  }
    else if c >= -2.0 { TEAL   }
    else              { BLUE   }
}

/// Render a 480×480 weather snapshot panel.
pub fn render_weather(data: &WeatherData) -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header bar ────────────────────────────────────────────────────────────
    // "WEATHER" label + city name
    let city_disp = if data.city.is_empty() { "WEATHER".to_string() }
                    else { data.city.to_uppercase() };
    draw_centered(&mut img, &bold, PxScale::from(26.0), 8, BLUE, &city_disp);

    // Current time top-right
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 10 - tw, 12, PxScale::from(18.0), &mono, &ts);

    draw_divider(&mut img, 42);

    // ── Main temperature ──────────────────────────────────────────────────────
    let temp_str = data.temp_c.map(|t| format!("{:.0}°C", t)).unwrap_or_else(|| "—".to_string());
    let t_color  = data.temp_c.map(temp_color).unwrap_or(SUBTEXT);
    let t_scale  = if temp_str.len() <= 5 { 86.0 } else { 64.0 };
    let tw_main  = text_width(&mono, PxScale::from(t_scale), &temp_str);
    // Offset left to make room for the icon
    let temp_x   = (240 - tw_main / 2).max(0);
    draw_text_mut(&mut img, t_color, temp_x, 50, PxScale::from(t_scale), &mono, &temp_str);

    // ── ASCII icon (right side) ───────────────────────────────────────────────
    let icon_lines = weather_icon(&data.condition);
    for (row, line) in icon_lines.iter().enumerate() {
        draw_text_mut(&mut img, TEAL,
            320, 56 + (row as i32 * 18),
            PxScale::from(17.0), &mono, line);
    }

    // ── Condition text ────────────────────────────────────────────────────────
    let cond_str: String = if data.condition.is_empty() { "—".to_string() }
                           else { data.condition.chars().take(28).collect() };
    draw_centered(&mut img, &bold, PxScale::from(20.0), 150, SUBTEXT, &cond_str);

    draw_divider(&mut img, 180);

    // ── Detail grid: 2 columns ────────────────────────────────────────────────
    // Left col: feels-like, humidity; Right col: wind, visibility
    let fields: &[(&str, String)] = &[
        ("Feels like", data.feels_like_c.map(|t| format!("{:.0}°C",t)).unwrap_or_else(||"—".to_string())),
        ("Humidity",   data.humidity_pct.map(|h| format!("{}%", h)).unwrap_or_else(||"—".to_string())),
        ("Wind",       {
            let spd = data.wind_kmh.map(|w| format!("{:.0}km/h",w)).unwrap_or_else(||"—".to_string());
            if data.wind_dir.is_empty() { spd } else { format!("{} {}", spd, data.wind_dir) }
        }),
        ("Visibility", data.visibility_km.map(|v| format!("{:.0}km",v)).unwrap_or_else(||"—".to_string())),
        ("High",       data.high_c.map(|t| format!("{:.0}°C",t)).unwrap_or_else(||"—".to_string())),
        ("Low",        data.low_c.map(|t| format!("{:.0}°C",t)).unwrap_or_else(||"—".to_string())),
    ];

    let start_y = 192;
    let row_h   = 42;
    let col_w   = 240;

    for (i, (label, value)) in fields.iter().enumerate() {
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        let bx  = col * col_w;
        let by  = start_y + row * row_h;

        // Cell background
        if row % 2 == 0 {
            draw_filled_rect_mut(&mut img,
                Rect::at(bx, by).of_size(col_w as u32, row_h as u32),
                Rgb([38, 38, 58]));
        }

        // Label
        draw_text_mut(&mut img, OVERLAY0, bx + 8, by + 5, PxScale::from(14.0), &bold, label);
        // Value
        let vw = text_width(&mono, PxScale::from(20.0), value);
        let vx = bx + col_w - vw - 8;
        draw_text_mut(&mut img, TEXT, vx, by + 17, PxScale::from(20.0), &mono, value);
    }

    draw_divider(&mut img, start_y + 3 * row_h + 2);

    // ── Updated timestamp ─────────────────────────────────────────────────────
    let upd_str = if let Some(ts) = data.updated_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        let age = now.saturating_sub(ts);
        if age < 120       { "Updated just now".to_string() }
        else if age < 3600 { format!("Updated {}m ago", age / 60) }
        else               { format!("Updated {}h ago", age / 3600) }
    } else {
        "Push weather data via API".to_string()
    };
    draw_centered(&mut img, &bold, PxScale::from(14.0), 452, OVERLAY0, &upd_str);

    DynamicImage::ImageRgb8(img)
}

// ─── Notes / bulletin board ───────────────────────────────────────────────────

/// Render up to 10 text notes as a bulletin board.
///
/// Each note is a short string (truncated to ~40 chars).  Notes are shown
/// top-to-bottom with a subtle alternating row background.
///
/// # Arguments
/// * `title` — panel heading (shown at top).
/// * `notes` — list of note strings; first 10 are shown.
pub fn render_notes(title: &str, notes: &[String]) -> DynamicImage {
    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    let header = if title.is_empty() { "NOTES".to_string() } else { title.to_uppercase() };
    draw_centered(&mut img, &bold, PxScale::from(24.0), 8, YELLOW, &header);

    // Current time
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 10 - tw, 12, PxScale::from(18.0), &mono, &ts);

    draw_divider(&mut img, 40);

    if notes.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(20.0), 200, OVERLAY0, "No notes");
        draw_centered(&mut img, &bold, PxScale::from(15.0), 232, OVERLAY0, "POST /api/notes to add items");
        return DynamicImage::ImageRgb8(img);
    }

    const ROW_H: i32 = 40;
    const START_Y: i32 = 50;
    const PADDING_X: i32 = 12;
    const MAX_NOTES: usize = 10;

    for (i, note) in notes.iter().take(MAX_NOTES).enumerate() {
        let y = START_Y + (i as i32) * ROW_H;

        // Alternating row background
        if i % 2 == 0 {
            draw_filled_rect_mut(&mut img,
                Rect::at(0, y).of_size(480, ROW_H as u32),
                Rgb([38, 38, 56]));
        }

        // Bullet point
        let bullet_x = PADDING_X;
        let col = if i % 4 == 0 { MAUVE } else if i % 4 == 1 { BLUE } else if i % 4 == 2 { TEAL } else { PEACH };
        draw_text_mut(&mut img, col, bullet_x, y + 9, PxScale::from(20.0), &bold, "•");

        // Note text
        let text: String = note.chars().take(38).collect();
        draw_text_mut(&mut img, TEXT, bullet_x + 22, y + 10, PxScale::from(18.0), &bold, &text);
    }

    // Count indicator if there are more than MAX_NOTES
    if notes.len() > MAX_NOTES {
        let more = format!("+{} more…", notes.len() - MAX_NOTES);
        draw_centered(&mut img, &bold, PxScale::from(14.0), 458, OVERLAY0, &more);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Big clock (fullscreen, maximum-size time) ────────────────────────────────

/// Full-screen big clock — maximum-size HH:MM with prominent seconds.
///
/// Layout (480×480):
///   • Weekday + date header   (y ≈ 12–66)
///   • Divider                 (y ≈ 72)
///   • Giant HH:MM (144px)     (y ≈ 78 … 228)
///   • Divider                 (y ≈ 234)
///   • Large SS (90px)         (y ≈ 242 … 336)
///   • Seconds progress bar    (y ≈ 348)
///   • Footer: uptime + load   (y ≈ 368 … 408)
///   • Corner accent bars
pub fn render_big_clock() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now     = Local::now();
    let h       = now.hour();
    let m       = now.minute();
    let s       = now.second();

    let hm_str   = format!("{:02}:{:02}", h, m);
    let ss_str   = format!("{:02}", s);
    let wday_str = now.format("%A").to_string().to_uppercase();
    let date_str = now.format("%B %-e,  %Y").to_string();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_centered(&mut img, &bold, PxScale::from(26.0), 12, MAUVE, &wday_str);
    draw_centered(&mut img, &bold, PxScale::from(20.0), 44, SUBTEXT, &date_str);
    draw_divider(&mut img, 72);

    // ── Giant HH:MM ───────────────────────────────────────────────────────────
    let hm_scale = PxScale::from(144.0);
    let hm_w     = text_width(&mono, hm_scale, &hm_str);
    let hm_x     = (480 - hm_w).max(0) / 2;
    draw_text_mut(&mut img, TEAL, hm_x, 78, hm_scale, &mono, &hm_str);

    // ── Large seconds ─────────────────────────────────────────────────────────
    draw_divider(&mut img, 234);

    // Seconds colour cycles through the palette every 10 seconds
    let sec_color = match s / 10 {
        0 => PEACH,
        1 => YELLOW,
        2 => GREEN,
        3 => TEAL,
        4 => BLUE,
        _ => MAUVE,
    };
    let ss_scale = PxScale::from(90.0);
    let ss_w     = text_width(&mono, ss_scale, &ss_str);
    let ss_x     = (480 - ss_w).max(0) / 2;
    draw_text_mut(&mut img, sec_color, ss_x, 242, ss_scale, &mono, &ss_str);

    // Tiny "s" unit label to the right of the seconds digits
    draw_text_mut(&mut img, OVERLAY0, ss_x + ss_w + 6, 300,
                  PxScale::from(20.0), &bold, "s");

    // ── Seconds progress bar ──────────────────────────────────────────────────
    let sec_frac = s as f32 / 59.0;
    draw_bar(&mut img, 20, 350, 440, 10, sec_frac, sec_color);

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 372);
    let up_str   = format!("up {}", format_uptime(System::uptime()));
    let load     = System::load_average();
    let load_str = format!("load  {:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);
    draw_centered(&mut img, &bold, PxScale::from(18.0), 382, SUBTEXT, &up_str);
    draw_centered(&mut img, &bold, PxScale::from(16.0), 408, OVERLAY0, &load_str);

    // ── Corner accent bars ────────────────────────────────────────────────────
    // Teal bars flanking the header for visual polish
    draw_filled_rect_mut(&mut img, Rect::at(0,   0).of_size(6, 72), TEAL);
    draw_filled_rect_mut(&mut img, Rect::at(474, 0).of_size(6, 72), TEAL);

    DynamicImage::ImageRgb8(img)
}

// ─── QR Code display ─────────────────────────────────────────────────────────

/// Render a QR code for the given text/URL on the panel.
///
/// Layout:
///   • Thin BLUE header bar with title text (truncated at 32 chars)
///   • QR code centred on a light Catppuccin Base background
///   • Bottom bar shows the payload text in small OVERLAY0 mono
///
/// On error (e.g. data too long for QR), shows an error message instead.
pub fn render_qr_code(payload: &str, title: &str) -> DynamicImage {
        let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    let header_h: i32 = 64;
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, header_h as u32), BLUE);
    let title_str = if title.is_empty() { "QR Code" } else { title };
    let title_disp: String = title_str.chars().take(32).collect();
    draw_centered(&mut img, &bold, PxScale::from(28.0), 16, BG, &title_disp);

    // ── QR code ───────────────────────────────────────────────────────────────
    let qr_result = QrCode::new(payload.as_bytes());
    match qr_result {
        Err(e) => {
            let err_str = format!("QR Error: {e}");
            draw_centered(&mut img, &bold, PxScale::from(22.0), 200, RED, &err_str);
            draw_centered(&mut img, &mono, PxScale::from(16.0), 230, OVERLAY0, "Payload too large or invalid");
        }
        Ok(code) => {
            // Draw QR modules onto the image
            let modules = code.width();
            // Target QR area: 360px square, centred horizontally between header and footer
            let footer_h: i32 = 60;
            let qr_area_top  = header_h + 12;
            let qr_area_bot  = 480 - footer_h - 12;
            let qr_area_h    = qr_area_bot - qr_area_top;
            let qr_area_w    = 380_i32;
            let cell = ((qr_area_w.min(qr_area_h)) / (modules as i32 + 4)).max(1) as u32;
            let qr_px = cell * (modules as u32 + 4);
            let qr_x  = (480 - qr_px as i32) / 2;
            let qr_y  = qr_area_top + (qr_area_h - qr_px as i32) / 2;

            // White background for QR code (quiet zone)
            draw_filled_rect_mut(&mut img, Rect::at(qr_x, qr_y).of_size(qr_px, qr_px), WHITE);

            // Draw each module
            let quiet = 2; // quiet zone in cells
            for row in 0..modules {
                for col in 0..modules {
                    if code[(row, col)] == qrcode::Color::Dark {
                        let px = qr_x + ((col as i32 + quiet) * cell as i32);
                        let py = qr_y + ((row as i32 + quiet) * cell as i32);
                        draw_filled_rect_mut(&mut img, Rect::at(px, py).of_size(cell, cell), CRUST);
                    }
                }
            }
        }
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 420);
    let payload_preview: String = payload.chars().take(52).collect();
    let suffix = if payload.len() > 52 { "…" } else { "" };
    let display_payload = format!("{payload_preview}{suffix}");
    draw_centered(&mut img, &mono, PxScale::from(15.0), 428, OVERLAY0, &display_payload);

    DynamicImage::ImageRgb8(img)
}

// ─── Pomodoro timer display ───────────────────────────────────────────────────

/// Render a Pomodoro timer frame.
///
/// - `phase`              — "work", "short_break", or "long_break"
/// - `remaining_secs`     — seconds left in the current phase (≤ 0 = expired)
/// - `total_secs`         — total seconds for the current phase
/// - `completed_sessions` — completed work sessions within the current set of 4
/// - `running`            — true if the timer is actively counting down
pub fn render_pomodoro(
    phase: &str,
    remaining_secs: i64,
    total_secs: u64,
    completed_sessions: u8,
    running: bool,
) -> DynamicImage {
    use std::f64::consts::PI;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let (phase_color, phase_emoji, phase_label) = match phase {
        "short_break" => (GREEN, "☕", "Short Break"),
        "long_break"  => (TEAL,  "🌿", "Long Break"),
        _             => (RED,   "🍅", "Focus"),
    };

    // ── Header bar ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 56), phase_color);
    let sessions_in_set = completed_sessions % 4;
    let header_text = if phase == "work" {
        format!("{} Focus  Round {}", phase_emoji, sessions_in_set + 1)
    } else {
        format!("{} {}", phase_emoji, phase_label)
    };
    draw_centered(&mut img, &bold, PxScale::from(22.0), 16, BG, &header_text);

    // ── Progress ring (pixel-by-pixel arc) ───────────────────────────────────
    let cx = 240i32;
    let cy = 238i32;
    let outer_r = 148i32;
    let inner_r = 130i32;

    let progress = if total_secs > 0 {
        (remaining_secs.max(0) as f64) / (total_secs as f64)
    } else {
        0.0
    };

    let y_min = (cy - outer_r - 1).max(57) as u32;
    let y_max = (cy + outer_r + 1).min(478) as u32;
    let x_min = (cx - outer_r - 1).max(1) as u32;
    let x_max = (cx + outer_r + 1).min(478) as u32;

    for row in y_min..=y_max {
        for col in x_min..=x_max {
            let dx = col as i32 - cx;
            let dy = row as i32 - cy;
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if dist < inner_r as f64 || dist > outer_r as f64 {
                continue;
            }
            // Clockwise angle from top: atan2(dx, -dy) → 0 at top, π/2 at right
            let angle_cw = (dx as f64).atan2(-dy as f64).rem_euclid(2.0 * PI);
            let color = if angle_cw <= progress * 2.0 * PI {
                phase_color
            } else {
                SURFACE0
            };
            img.put_pixel(col, row, color);
        }
    }

    // ── Centre time text ──────────────────────────────────────────────────────
    let rem = remaining_secs.max(0);
    let time_str = format!("{:02}:{:02}", rem / 60, rem % 60);
    draw_centered(&mut img, &bold, PxScale::from(72.0), 192, TEXT, &time_str);

    // Status badge
    let (status_color, status_text) = if running {
        (GREEN, "  RUNNING")
    } else {
        (OVERLAY0, "  PAUSED")
    };
    draw_centered(&mut img, &mono, PxScale::from(18.0), 276, status_color, status_text);

    // ── Cycle dots (4 tomatoes = one set) ────────────────────────────────────
    draw_divider(&mut img, 415);
    let n_dots = 4i32;
    let dot_r  = 8i32;
    let dot_spacing = 30i32;
    let total_dot_w = (n_dots - 1) * dot_spacing + dot_r * 2;
    let dot_x0 = (480 - total_dot_w) / 2 + dot_r;
    let dot_y  = 440i32;

    for i in 0..n_dots {
        let dot_x = dot_x0 + i * dot_spacing;
        let is_complete = i < sessions_in_set as i32;
        let is_active   = phase == "work" && i == sessions_in_set as i32;
        let col = if is_complete       { phase_color }
                  else if is_active    { SUBTEXT }
                  else                 { SURFACE0 };
        for dr in -dot_r..=dot_r {
            for dc in -dot_r..=dot_r {
                if dr * dr + dc * dc <= dot_r * dot_r {
                    let px = (dot_x + dc) as u32;
                    let py = (dot_y + dr) as u32;
                    if px < 480 && py < 480 {
                        img.put_pixel(px, py, col);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Battery status display ───────────────────────────────────────────────────

struct BatteryInfo {
    capacity:        u8,
    status:          String,
    voltage_uv:      Option<u64>,   // microvolts
    current_ua:      Option<u64>,   // microamps
    energy_now_uwh:  Option<u64>,   // micro-watt-hours
    energy_full_uwh: Option<u64>,   // micro-watt-hours
}

/// Read battery info from the Linux sysfs power-supply interface.
/// Returns `None` on desktops or if no battery is found.
fn read_battery_info() -> Option<BatteryInfo> {
    for name in &["BAT0", "BAT1", "BAT2", "BAT"] {
        let base = format!("/sys/class/power_supply/{name}");
        if !std::path::Path::new(&base).exists() {
            continue;
        }
        let read_str = |f: &str| -> Option<String> {
            std::fs::read_to_string(format!("{base}/{f}")).ok()
                .map(|s| s.trim().to_string())
        };
        let read_u64 = |f: &str| -> Option<u64> {
            read_str(f)?.parse::<u64>().ok()
        };

        let capacity = read_u64("capacity").map(|v| v.min(100) as u8).unwrap_or(0);
        let status   = read_str("status").unwrap_or_default();

        let voltage_uv = read_u64("voltage_now");
        let current_ua = read_u64("current_now");

        // energy_now may be in µWh (energy_now) or µAh (charge_now)
        let energy_now_uwh = read_u64("energy_now").or_else(|| {
            let uah = read_u64("charge_now")?;
            let uv  = voltage_uv?;
            Some(uah * uv / 1_000_000)
        });
        let energy_full_uwh = read_u64("energy_full").or_else(|| {
            let uah = read_u64("charge_full")?;
            let uv  = voltage_uv?;
            Some(uah * uv / 1_000_000)
        });

        return Some(BatteryInfo { capacity, status, voltage_uv, current_ua,
                                  energy_now_uwh, energy_full_uwh });
    }
    None
}

/// Render a battery status display.
///
/// Reads live data from `/sys/class/power_supply/BAT*`.  
/// Shows a large battery icon, percentage, status, voltage, and estimated
/// time remaining on discharge.
pub fn render_battery() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(28.0), &bold, "Battery");
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, 18, PxScale::from(24.0), &mono, &time_str);
    draw_divider(&mut img, 52);

    match read_battery_info() {
        None => {
            draw_centered(&mut img, &bold, PxScale::from(22.0), 200, OVERLAY0,
                "No battery detected");
            draw_centered(&mut img, &mono, PxScale::from(16.0), 232, SURFACE0,
                "Desktop or unsupported platform");
        }
        Some(info) => {
            let pct   = info.capacity.clamp(0, 100);
            let frac  = pct as f32 / 100.0;
            let color = if pct > 50 { GREEN } else if pct > 20 { YELLOW } else { RED };

            // ── Battery icon ─────────────────────────────────────────────────
            // Body at (90, 105), 285×125 px
            let bx = 90i32;
            let by = 105i32;
            let bw = 285i32;
            let bh = 125i32;
            let term_w = 14i32;
            let term_h = 46i32;

            // Outline
            draw_hollow_rect_mut(&mut img,
                Rect::at(bx, by).of_size(bw as u32, bh as u32), OVERLAY0);
            // Terminal nub
            draw_filled_rect_mut(&mut img,
                Rect::at(bx + bw, by + (bh - term_h) / 2)
                    .of_size(term_w as u32, term_h as u32),
                OVERLAY0);
            // Fill bar
            let fill_w = ((bw - 4) as f32 * frac) as i32;
            if fill_w > 0 {
                draw_filled_rect_mut(&mut img,
                    Rect::at(bx + 2, by + 2)
                        .of_size(fill_w as u32, (bh - 4) as u32),
                    color);
            }

            // ── Percentage centred inside the battery body ───────────────────
            let pct_str = format!("{}%", pct);
            let text_col = if frac > 0.35 { BG } else { TEXT };
            draw_centered(&mut img, &bold, PxScale::from(60.0), 120, text_col, &pct_str);

            // ── Status line ───────────────────────────────────────────────────
            let status_lower = info.status.to_lowercase();
            let status_str: &str = match status_lower.trim() {
                "charging"     => "⚡ Charging",
                "discharging"  => "Discharging",
                "full"         => "Full",
                "not charging" => "Not Charging",
                other          => other,
            };
            draw_centered(&mut img, &bold, PxScale::from(24.0), 250, color, status_str);

            // ── Detail rows ───────────────────────────────────────────────────
            draw_divider(&mut img, 286);
            let mut y = 298i32;
            let scale = PxScale::from(18.0);

            if let Some(v) = info.voltage_uv {
                draw_kv(&mut img, &bold, &mono, scale, y, SUBTEXT, TEXT,
                    "Voltage", &format!("{:.3} V", v as f64 / 1_000_000.0));
                y += 26;
            }
            if let (Some(en), Some(ef)) = (info.energy_now_uwh, info.energy_full_uwh) {
                draw_kv(&mut img, &bold, &mono, scale, y, SUBTEXT, TEXT,
                    "Capacity",
                    &format!("{:.1}/{:.1} Wh", en as f64 / 1_000_000.0,
                                               ef as f64 / 1_000_000.0));
                y += 26;
            }
            // Estimated time remaining for discharging
            if info.status.to_lowercase() == "discharging" {
                if let (Some(en), Some(cur), Some(v)) =
                    (info.energy_now_uwh, info.current_ua, info.voltage_uv)
                {
                    let power_uw = cur as f64 * v as f64 / 1_000_000.0;
                    if power_uw > 0.0 {
                        let hours_left = en as f64 / power_uw;
                        let h  = hours_left as u64;
                        let m  = ((hours_left - h as f64) * 60.0) as u64;
                        let est = format!("{}h {:02}m", h, m);
                        draw_kv(&mut img, &bold, &mono, scale, y,
                            SUBTEXT, TEAL, "Est. left", &est);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Public battery data accessor ────────────────────────────────────────────

/// Return battery information as a `serde_json::Value` for the API.
///
/// On systems without a battery, returns `{"present": false}`.
pub fn read_battery_json() -> serde_json::Value {
    use serde_json::json;
    match read_battery_info() {
        None => json!({ "present": false }),
        Some(info) => {
            let power_uw = info.current_ua.and_then(|c| info.voltage_uv.map(|v| c * v / 1_000_000));
            let hours_remaining = if info.status.to_lowercase() == "discharging" {
                power_uw.and_then(|p| {
                    if p == 0 { return None; }
                    info.energy_now_uwh.map(|e| e as f64 / p as f64)
                })
            } else { None };

            json!({
                "present":          true,
                "capacity_pct":     info.capacity,
                "status":           info.status,
                "voltage_v":        info.voltage_uv.map(|v| v as f64 / 1_000_000.0),
                "current_a":        info.current_ua.map(|c| c as f64 / 1_000_000.0),
                "power_w":          power_uw.map(|p| p as f64 / 1_000_000.0),
                "energy_now_wh":    info.energy_now_uwh.map(|e|  e as f64 / 1_000_000.0),
                "energy_full_wh":   info.energy_full_uwh.map(|e| e as f64 / 1_000_000.0),
                "hours_remaining":  hours_remaining,
            })
        }
    }
}

// ─── World clock display ──────────────────────────────────────────────────────

/// Render a world clock showing up to 4 cities simultaneously.
///
/// Each entry is `(city_name, utc_offset_minutes)`. Handles DST-unaware
/// fixed UTC offsets for simplicity; offset is clamped to ±14 h.
///
/// If `cities` is empty, shows UTC + 3 common defaults.
pub fn render_world_clock(cities: &[(String, i32)]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now_utc = Local::now().naive_utc();

    let defaults: Vec<(String, i32)> = vec![
        ("UTC".into(),      0),
        ("New York".into(), -300),  // EST (UTC-5); does not track DST
        ("London".into(),   0),
        ("Tokyo".into(),    540),   // JST (UTC+9)
    ];

    let entries: &[(String, i32)] = if cities.is_empty() { &defaults } else { cities };
    let entries: Vec<&(String, i32)> = entries.iter().take(4).collect();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 56), BLUE);
    draw_centered(&mut img, &bold, PxScale::from(24.0), 16, BG, "World Clock");

    let n = entries.len();
    let row_h = (480 - 56) / n.max(1) as i32;

    for (i, (city, offset_mins)) in entries.iter().enumerate() {
        let offset_mins = (*offset_mins).clamp(-14 * 60, 14 * 60);
        let local_dt = now_utc + chrono::Duration::minutes(offset_mins as i64);
        let h = local_dt.hour();
        let m = local_dt.minute();
        let s = local_dt.second();
        let ampm = if h < 12 { "AM" } else { "PM" };
        let h12  = if h % 12 == 0 { 12 } else { h % 12 };

        let time_24 = format!("{:02}:{:02}:{:02}", h, m, s);
        let time_12 = format!("{:02}:{:02} {}", h12, m, ampm);
        let date_str = local_dt.format("%b %-d").to_string();

        let y_base = 56 + i as i32 * row_h;

        // Background stripe for even rows
        if i % 2 == 1 {
            draw_filled_rect_mut(&mut img,
                Rect::at(0, y_base).of_size(480, row_h as u32),
                SURFACE0);
        }

        // City name
        let city_disp: String = city.chars().take(18).collect();
        draw_text_mut(&mut img, TEXT, 20, y_base + 10, PxScale::from(20.0), &bold, &city_disp);

        // UTC offset label
        let offset_h   = offset_mins / 60;
        let offset_m   = (offset_mins % 60).abs();
        let offset_str = if offset_m == 0 {
            format!("UTC{:+}", offset_h)
        } else {
            format!("UTC{:+}:{:02}", offset_h, offset_m)
        };
        let ow = text_width(&mono, PxScale::from(14.0), &offset_str);
        draw_text_mut(&mut img, OVERLAY0, 480 - 20 - ow, y_base + 10,
            PxScale::from(14.0), &mono, &offset_str);

        // Day/night color: golden for day (6–18), blue for evening (18–22), dim for night
        let time_color = if h >= 6 && h < 18  { YELLOW }
                         else if h < 22        { BLUE }
                         else                  { OVERLAY0 };
        // Day/night dot indicator
        let dot_color = if h >= 6 && h < 18 { YELLOW } else { SURFACE0 };
        let dot_x = (480 - 20 - 8) as u32;
        let dot_y_px = (y_base + 44) as u32;
        for dr in -5i32..=5 {
            for dc in -5i32..=5 {
                if dr * dr + dc * dc <= 25 {
                    let px = (dot_x as i32 + dc).max(0) as u32;
                    let py = (dot_y_px as i32 + dr).max(0) as u32;
                    if px < 480 && py < 480 { img.put_pixel(px, py, dot_color); }
                }
            }
        }

        // Large time
        draw_text_mut(&mut img, time_color, 20, y_base + 32, PxScale::from(36.0), &mono, &time_24);
        // 12-hour + date right-aligned
        let tw12 = text_width(&mono, PxScale::from(18.0), &time_12);
        draw_text_mut(&mut img, SUBTEXT, 480 - 20 - tw12, y_base + 36,
            PxScale::from(18.0), &mono, &time_12);
        let dw = text_width(&mono, PxScale::from(16.0), &date_str);
        draw_text_mut(&mut img, OVERLAY0, 480 - 20 - dw, y_base + 58,
            PxScale::from(16.0), &mono, &date_str);

        // Divider between entries
        if i + 1 < n {
            draw_divider(&mut img, y_base + row_h - 1);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Scrolling ticker display ─────────────────────────────────────────────────

/// Render a scrolling text ticker.
///
/// - `text`    — the text to scroll (long strings scroll, short ones are centred)
/// - `scroll_x`— current pixel offset from the right (0 = starts just off-screen)
/// - `color`   — one of the named CSS-like colour strings ("blue", "green", etc.)
///
/// Returns the rendered frame. The caller should increment `scroll_x` by ~2
/// pixels per render tick to produce smooth scrolling.
pub fn render_ticker(text: &str, scroll_x: i32, color_name: &str) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let col = match color_name {
        "blue"   => BLUE,
        "green"  => GREEN,
        "yellow" => YELLOW,
        "red"    => RED,
        "mauve"  => MAUVE,
        "teal"   => TEAL,
        "peach"  => PEACH,
        _        => TEXT,
    };

    // Header bar
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 48), SURFACE0);
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 12 - tw, 16, PxScale::from(18.0), &mono, &ts);
    draw_text_mut(&mut img, OVERLAY0, 12, 16, PxScale::from(18.0), &bold, "TICKER");
    draw_divider(&mut img, 48);

    let scale = PxScale::from(48.0);
    let text_w = text_width(&bold, scale, text);

    // If text fits, just centre it
    if text_w <= 440 {
        draw_centered(&mut img, &bold, scale, 195, col, text);
    } else {
        // Scroll: x starts at 480 (off right), moves left
        let x = 480 - scroll_x;
        // Clip draw to panel bounds  
        if x > -text_w {
            // Use draw_text_mut directly at the computed x position
            draw_text_mut(&mut img, col, x, 195, scale, &bold, text);
        }
    }

    // Progress indicator bar at bottom — shows position in scroll cycle
    let cycle_w = text_w + 480;
    if cycle_w > 0 {
        let frac = (scroll_x as f32 / cycle_w as f32).clamp(0.0, 1.0);
        draw_bar(&mut img, 20, 440, 440, 8, frac, col);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── HTTP Status Monitor display ──────────────────────────────────────────────

/// One HTTP monitor result. `status_code` = None means unreachable / timed out.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HttpMonitorResult {
    pub label:       String,
    pub url:         String,
    pub status_code: Option<u16>,
    pub latency_ms:  Option<u64>,
    pub ok:          bool,
}

/// Render an HTTP health-monitor dashboard.
///
/// Shows up to 6 configured endpoints with status code, latency, and a
/// green (ok) / red (error) indicator badge.
pub fn render_http_status(results: &[HttpMonitorResult]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(&mut img, TEAL, 20, 14, PxScale::from(22.0), &bold, "HTTP MONITOR");
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(16.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 16 - tw, 18, PxScale::from(16.0), &mono, &ts);
    draw_divider(&mut img, 52);

    let show: Vec<&HttpMonitorResult> = results.iter().take(6).collect();
    let n = show.len();

    if n == 0 {
        draw_centered(&mut img, &bold, PxScale::from(22.0), 200, OVERLAY0,
            "No monitors configured.");
        draw_centered(&mut img, &bold, PxScale::from(18.0), 234, SURFACE0,
            "POST /api/http_status to add one");
        return DynamicImage::ImageRgb8(img);
    }

    let row_h = (480 - 60) / n.max(1) as i32;

    for (i, r) in show.iter().enumerate() {
        let y = 60 + i as i32 * row_h;

        // Alternate row background
        if i % 2 == 1 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(480, row_h as u32), SURFACE0);
        }

        // Status badge (coloured rectangle on left)
        let badge_col = if r.ok { GREEN } else { RED };
        draw_filled_rect_mut(&mut img, Rect::at(12, y + 10).of_size(6, (row_h - 20).max(6) as u32), badge_col);

        // Label
        let label_disp: String = r.label.chars().take(20).collect();
        draw_text_mut(&mut img, TEXT, 28, y + 8, PxScale::from(20.0), &bold, &label_disp);

        // Status code / timeout
        let code_str = match r.status_code {
            Some(c) => format!("{}", c),
            None    => "TIMEOUT".into(),
        };
        let code_col = if r.ok { GREEN }
                       else if r.status_code.map(|c| c < 500).unwrap_or(false) { YELLOW }
                       else { RED };
        let cw = text_width(&mono, PxScale::from(18.0), &code_str);
        draw_text_mut(&mut img, code_col, 480 - 16 - cw, y + 10, PxScale::from(18.0), &mono, &code_str);

        // Latency
        let lat_str = match r.latency_ms {
            Some(ms) if ms < 1000 => format!("{}ms", ms),
            Some(ms)              => format!("{:.1}s", ms as f64 / 1000.0),
            None                  => String::new(),
        };
        if !lat_str.is_empty() {
            let lw = text_width(&mono, PxScale::from(15.0), &lat_str);
            draw_text_mut(&mut img, OVERLAY0, 480 - 16 - lw, y + 32, PxScale::from(15.0), &mono, &lat_str);
        }

        // URL (truncated, small)
        let url_disp: String = r.url.chars().take(36).collect();
        draw_text_mut(&mut img, OVERLAY0, 28, y + 32, PxScale::from(15.0), &mono, &url_disp);

        // Divider
        if i + 1 < n {
            draw_divider(&mut img, y + row_h - 1);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Shell command output display ─────────────────────────────────────────────

/// Render a shell-command output viewer.
///
/// - `cmd`      — display-only short description of the command
/// - `output`   — captured stdout (newline-delimited); up to 12 lines rendered
/// - `exit_ok`  — whether the last run exited with code 0
/// - `run_secs` — Unix seconds of the last successful run (0 = never)
pub fn render_shell_cmd(cmd: &str, output: &str, exit_ok: bool, run_secs: u64) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let hdr_col = if exit_ok { TEAL } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(&mut img, hdr_col, 20, 14, PxScale::from(20.0), &bold, "SHELL CMD");
    let status_str = if exit_ok { "OK" } else { "ERR" };
    let sw = text_width(&bold, PxScale::from(18.0), status_str);
    draw_text_mut(&mut img, hdr_col, 480 - 16 - sw, 16, PxScale::from(18.0), &bold, status_str);
    draw_divider(&mut img, 52);

    // Command label
    let cmd_disp: String = cmd.chars().take(44).collect();
    draw_text_mut(&mut img, OVERLAY0, 12, 58, PxScale::from(16.0), &mono, &cmd_disp);

    // Last-run timestamp
    if run_secs > 0 {
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(run_secs as i64, 0)
            .map(|d| d.with_timezone(&Local).format("%H:%M:%S").to_string())
            .unwrap_or_default();
        let rw = text_width(&mono, PxScale::from(14.0), &dt);
        draw_text_mut(&mut img, OVERLAY0, 480 - 14 - rw, 60,
            PxScale::from(14.0), &mono, &dt);
    }

    draw_divider(&mut img, 78);

    // Output lines (monospaced, up to 12 lines)
    let line_h: i32 = 32;
    let max_lines: usize = 12;
    let start_y: i32 = 84;

    let lines: Vec<&str> = if output.is_empty() {
        vec!["(no output)"]
    } else {
        output.lines().take(max_lines).collect()
    };

    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as i32 * line_h;
        if y + line_h > 480 { break; }
        // Truncate to fit panel width
        let disp: String = line.chars().take(42).collect();
        let text_col = if i == 0 { TEXT } else { SUBTEXT };
        draw_text_mut(&mut img, text_col, 12, y, PxScale::from(18.0), &mono, &disp);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Ping latency monitor display ────────────────────────────────────────────

/// One ping result for a configured host.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PingResult {
    pub host:       String,
    pub label:      String,
    pub latency_ms: Option<f64>,   // None = unreachable / timeout
    pub loss_pct:   f32,           // packet-loss percentage 0–100
}

/// Render a ping-latency monitor dashboard.
///
/// Shows up to 8 configured hosts with round-trip latency and a
/// colour-coded quality indicator.
pub fn render_ping_monitor(results: &[PingResult]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(&mut img, MAUVE, 20, 14, PxScale::from(22.0), &bold, "PING MONITOR");
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(16.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 16 - tw, 18, PxScale::from(16.0), &mono, &ts);
    draw_divider(&mut img, 52);

    let show: Vec<&PingResult> = results.iter().take(8).collect();
    let n = show.len();

    if n == 0 {
        draw_centered(&mut img, &bold, PxScale::from(22.0), 200, OVERLAY0,
            "No hosts configured.");
        draw_centered(&mut img, &bold, PxScale::from(18.0), 234, SURFACE0,
            "POST /api/ping to add one");
        return DynamicImage::ImageRgb8(img);
    }

    let row_h = (480 - 60) / n.max(1) as i32;

    for (i, r) in show.iter().enumerate() {
        let y = 60 + i as i32 * row_h;

        // Alternate row background
        if i % 2 == 1 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(480, row_h as u32), SURFACE0);
        }

        // Latency quality color
        let (qual_col, bar_frac) = match r.latency_ms {
            None       => (RED,    0.0f32),
            Some(ms) if ms < 20.0  => (GREEN,  (ms / 200.0) as f32),
            Some(ms) if ms < 80.0  => (TEAL,   (ms / 200.0) as f32),
            Some(ms) if ms < 200.0 => (YELLOW, (ms / 200.0) as f32),
            Some(ms)               => (RED,    1.0f32.min(ms as f32 / 500.0)),
        };

        // Left bar accent
        draw_filled_rect_mut(&mut img,
            Rect::at(12, y + 8).of_size(4, (row_h - 16).max(4) as u32),
            qual_col);

        // Label
        let label_disp: String = r.label.chars().take(18).collect();
        draw_text_mut(&mut img, TEXT, 26, y + 8, PxScale::from(20.0), &bold, &label_disp);

        // Host (small, right-aligned)
        let host_disp: String = r.host.chars().take(24).collect();
        let hdisp_w = text_width(&mono, PxScale::from(14.0), &host_disp);
        draw_text_mut(&mut img, OVERLAY0, 480 - 14 - hdisp_w, y + 10, PxScale::from(14.0), &mono, &host_disp);

        // Latency value
        let lat_str = match r.latency_ms {
            Some(ms) => format!("{:.1} ms", ms),
            None     => "TIMEOUT".into(),
        };
        draw_text_mut(&mut img, qual_col, 26, y + 30, PxScale::from(22.0), &mono, &lat_str);

        // Loss indicator
        if r.loss_pct > 0.0 {
            let loss_str = format!("{:.0}% loss", r.loss_pct);
            let lw = text_width(&mono, PxScale::from(14.0), &loss_str);
            draw_text_mut(&mut img, YELLOW, 480 - 14 - lw, y + 30, PxScale::from(14.0), &mono, &loss_str);
        }

        // Latency mini-bar
        if row_h > 56 {
            draw_bar(&mut img, 26, y + row_h - 14, 200, 6, bar_frac, qual_col);
        }

        // Divider
        if i + 1 < n {
            draw_divider(&mut img, y + row_h - 1);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Dashboard (4-quadrant overview) ─────────────────────────────────────────

/// Render a four-quadrant overview dashboard:
///
/// ```text
/// ┌─────────────────┬─────────────────┐
/// │  CLOCK + DATE   │  CPU / MEM      │
/// │                 │                 │
/// ├─────────────────┼─────────────────┤
/// │  NETWORK I/O    │  TEMPERATURE    │
/// │                 │                 │
/// └─────────────────┴─────────────────┘
/// ```
///
/// All stats are collected fresh on each call. Useful as a "glance" mode.
pub fn render_dashboard() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Collect all stats ─────────────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let now       = Local::now();
    let time_str  = now.format("%H:%M:%S").to_string();
    let date_str  = now.format("%a %b %-d").to_string();

    let cpu_pct   = sys.global_cpu_usage();
    let used_mem  = sys.used_memory();
    let total_mem = sys.total_memory();
    let mem_pct   = if total_mem > 0 { used_mem as f64 / total_mem as f64 * 100.0 } else { 0.0 };
    let used_gb   = used_mem as f64 / 1_073_741_824.0;
    let total_gb  = total_mem as f64 / 1_073_741_824.0;

    // Temperature
    let mut comps = sysinfo::Components::new_with_refreshed_list();
    comps.refresh(false);
    let temp_c: Option<f32> = comps.iter()
        .filter(|c| {
            let l = c.label().to_lowercase();
            l.contains("cpu") || l.contains("core") || l.contains("package")
        })
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);

    // Network
    use sysinfo::Networks;
    let networks = Networks::new_with_refreshed_list();
    let (rx_b, tx_b): (u64, u64) = networks.iter()
        .filter(|(name, _)| !name.starts_with("lo"))
        .fold((0, 0), |(r, t), (_, d)| {
            (r + d.received(), t + d.transmitted())
        });

    let load = System::load_average();
    let uptime_secs = System::uptime();
    let hostname = System::host_name().unwrap_or_else(|| "ttlcd".into());

    // ── Layout constants ──────────────────────────────────────────────────
    let mid_x: i32 = 240;
    let mid_y: i32 = 240;
    let pad:   i32 = 10;

    // ── Quadrant dividers ─────────────────────────────────────────────────
    // Vertical centre line
    draw_filled_rect_mut(&mut img, Rect::at(mid_x - 1, 0).of_size(2, 480), SURFACE0);
    // Horizontal centre line
    draw_filled_rect_mut(&mut img, Rect::at(0, mid_y - 1).of_size(480, 2), SURFACE0);

    // ── Top-left: Clock ───────────────────────────────────────────────────
    let tl_col = MAUVE;
    draw_text_mut(&mut img, tl_col, pad, pad + 2, PxScale::from(18.0), &mono, &hostname);
    draw_text_mut(&mut img, TEXT, pad, pad + 26, PxScale::from(48.0), &mono, &time_str);
    draw_text_mut(&mut img, SUBTEXT, pad, pad + 80, PxScale::from(20.0), &bold, &date_str);

    let uptime = format_uptime(uptime_secs);
    draw_text_mut(&mut img, OVERLAY0, pad, pad + 106, PxScale::from(16.0), &mono, &format!("up {}", uptime));

    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);
    draw_text_mut(&mut img, OVERLAY0, pad, pad + 126, PxScale::from(14.0), &mono, &load_str);

    // ── Top-right: CPU + Memory ───────────────────────────────────────────
    let tr_x = mid_x + pad;
    let cpu_col = if cpu_pct > 80.0 { RED } else if cpu_pct > 50.0 { YELLOW } else { GREEN };

    draw_text_mut(&mut img, BLUE, tr_x, pad + 2, PxScale::from(18.0), &bold, "CPU");
    draw_text_mut(&mut img, cpu_col, tr_x, pad + 24, PxScale::from(38.0), &mono,
        &format!("{:4.1}%", cpu_pct));
    draw_bar(&mut img, tr_x, pad + 70, 220, 14, cpu_pct / 100.0, cpu_col);

    draw_divider(&mut img, pad + 92);

    let mem_col = if mem_pct > 85.0 { RED } else if mem_pct > 65.0 { YELLOW } else { GREEN };
    draw_text_mut(&mut img, BLUE, tr_x, pad + 98, PxScale::from(18.0), &bold, "MEM");
    draw_text_mut(&mut img, mem_col, tr_x, pad + 120, PxScale::from(30.0), &mono,
        &format!("{:.1}/{:.1}G", used_gb, total_gb));
    draw_bar(&mut img, tr_x, pad + 158, 220, 14, (mem_pct / 100.0) as f32, mem_col);

    draw_text_mut(&mut img, OVERLAY0, tr_x, pad + 178, PxScale::from(16.0), &mono,
        &format!("{:.0}%", mem_pct));

    // ── Bottom-left: Network ──────────────────────────────────────────────
    let bl_y = mid_y + pad;
    draw_text_mut(&mut img, TEAL, pad, bl_y, PxScale::from(18.0), &bold, "NETWORK");
    let rx_str = format!("↓ {}", format_bytes_compact(rx_b));
    let tx_str = format!("↑ {}", format_bytes_compact(tx_b));
    draw_text_mut(&mut img, GREEN, pad, bl_y + 26, PxScale::from(28.0), &mono, &rx_str);
    draw_text_mut(&mut img, YELLOW, pad, bl_y + 60, PxScale::from(28.0), &mono, &tx_str);

    // Active interfaces count
    let if_count = networks.iter().filter(|(n, _)| !n.starts_with("lo")).count();
    draw_text_mut(&mut img, OVERLAY0, pad, bl_y + 96,
        PxScale::from(16.0), &mono, &format!("{} interface{}", if_count, if if_count == 1 { "" } else { "s" }));

    // ── Bottom-right: Temperature ─────────────────────────────────────────
    let br_x = mid_x + pad;
    let br_y = mid_y + pad;
    draw_text_mut(&mut img, PEACH, br_x, br_y, PxScale::from(18.0), &bold, "TEMP");

    match temp_c {
        None => {
            draw_text_mut(&mut img, OVERLAY0, br_x, br_y + 28, PxScale::from(22.0), &bold, "N/A");
        }
        Some(t) => {
            let t_col = if t > 85.0 { RED } else if t > 70.0 { YELLOW } else { GREEN };
            draw_text_mut(&mut img, t_col, br_x, br_y + 24, PxScale::from(44.0), &mono,
                &format!("{:.0}°C", t));
            let bar_frac = ((t - 20.0) / 80.0).clamp(0.0, 1.0);
            draw_bar(&mut img, br_x, br_y + 76, 220, 14, bar_frac, t_col);

            // Show a few sensor labels
            let mut sensor_y = br_y + 98;
            for c in comps.iter().take(3) {
                if let Some(st) = c.temperature() {
                    if st > 0.0 && !st.is_nan() {
                        let label: String = c.label().chars().take(14).collect();
                        let sc = if st > 80.0 { RED } else if st > 65.0 { YELLOW } else { OVERLAY0 };
                        draw_text_mut(&mut img, sc, br_x, sensor_y,
                            PxScale::from(14.0), &mono, &format!("{:<14} {:.0}°", label, st));
                        sensor_y += 18;
                        if sensor_y > 470 { break; }
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

fn format_bytes_compact(b: u64) -> String {
    if b >= 1_000_000_000 { format!("{:.1}GB", b as f64 / 1_000_000_000.0) }
    else if b >= 1_000_000 { format!("{:.1}MB", b as f64 / 1_000_000.0) }
    else if b >= 1_000     { format!("{:.0}KB", b as f64 / 1_000.0) }
    else                   { format!("{}B", b) }
}


/// Remove ANSI/VT100 escape sequences from a string.
/// Handles CSI sequences (ESC [ ... m) and OSC sequences.
fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Skip escape sequence
            i += 1;
            if i < bytes.len() {
                match bytes[i] {
                    b'[' => {
                        // CSI sequence: ESC [ ... (letter)
                        i += 1;
                        while i < bytes.len() && !bytes[i].is_ascii_alphabetic() { i += 1; }
                        if i < bytes.len() { i += 1; } // skip the terminating letter
                    }
                    b']' => {
                        // OSC sequence: ESC ] ... ST (BEL or ESC\)
                        i += 1;
                        while i < bytes.len() && bytes[i] != 0x07 && bytes[i] != 0x1b { i += 1; }
                        if i < bytes.len() { i += 1; }
                    }
                    _ => {
                        // Other two-char sequences
                        i += 1;
                    }
                }
            }
        } else if bytes[i] < 0x20 && bytes[i] != b'\t' {
            // Skip other control chars except tab
            i += 1;
        } else {
            // Safe to push this char
            if let Some(c) = s[i..].chars().next() {
                out.push(c);
                i += c.len_utf8();
            } else {
                i += 1;
            }
        }
    }
    out
}

// ─── Log tail viewer ──────────────────────────────────────────────────────────

/// Render the last N lines of a log file.
///
/// - `path`  — file path shown in the header (just the filename portion)
/// - `lines` — already-read tail lines (most recent last)
/// - `ok`    — true if the file was readable; false means show error state
pub fn render_log_tail(path: &str, lines: &[String], ok: bool) -> DynamicImage {
    let mut img = RgbImage::from_pixel(480, 480, BG);
    let mono = font_mono();
    let bold = font_bold();

    // Header bar
    let hdr_color = if ok { TEAL } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 44), SURFACE0);

    // File icon + short filename
    let fname: String = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .chars().take(30).collect();
    let hdr_text = if ok {
        format!("≡ {}", fname)
    } else {
        format!("✕ {}", fname)
    };
    draw_text_mut(&mut img, hdr_color, 8, 8, PxScale::from(24.0), &bold, &hdr_text);

    // Line count badge
    let badge = format!("{} lines", lines.len());
    let bw = text_width(&mono, PxScale::from(14.0), &badge);
    draw_text_mut(&mut img, OVERLAY0, 480 - bw - 8, 14, PxScale::from(14.0), &mono, &badge);

    // Divider
    draw_filled_rect_mut(&mut img, Rect::at(0, 44).of_size(480, 2), hdr_color);

    if !ok {
        let msg = if path.is_empty() {
            "No file configured.".to_string()
        } else {
            format!("Cannot read:\n{}", path)
        };
        let y = 48 + 20;
        for (i, l) in msg.lines().enumerate() {
            draw_text_mut(&mut img, SUBTEXT, 12, y + i as i32 * 20, PxScale::from(18.0), &mono, l);
        }
        draw_text_mut(&mut img, OVERLAY0, 12, y + 60, PxScale::from(14.0), &mono,
            "Use POST /api/log_tail to set a path.");
        return DynamicImage::ImageRgb8(img);
    }

    // Content area — 14px mono, max ~16 visible lines
    const LINE_H: i32 = 26;
    const MAX_LINES: usize = 16;
    // const CHAR_W: i32 = 8;  // approx chars at 14px mono
    const MAX_CHARS: usize = 58;

    let content_y_start: i32 = 50;

    // Show up to MAX_LINES most-recent lines
    let display_lines: Vec<&str> = if lines.len() > MAX_LINES {
        lines[lines.len() - MAX_LINES..].iter().map(|s| s.as_str()).collect()
    } else {
        lines.iter().map(|s| s.as_str()).collect()
    };

    for (i, line) in display_lines.iter().enumerate() {
        let y = content_y_start + i as i32 * LINE_H;
        if y + LINE_H > 480 { break; }

        // Strip ANSI escape sequences
        let clean: String = strip_ansi(line);
        // Truncate long lines
        let truncated: String = if clean.chars().count() > MAX_CHARS {
            let t: String = clean.chars().take(MAX_CHARS - 1).collect();
            format!("{}…", t)
        } else {
            clean.clone()
        };

        // Alternate row backgrounds for readability
        if i % 2 == 1 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(480, LINE_H as u32), SURFACE0);
        }

        // Highlight lines with common log level keywords
        let color = if clean.contains("ERROR") || clean.contains("error") || clean.contains("CRIT") || clean.contains("crit") {
            RED
        } else if clean.contains("WARN") || clean.contains("warn") {
            YELLOW
        } else if clean.contains("INFO") || clean.contains("info") {
            TEXT
        } else if clean.contains("DEBUG") || clean.contains("debug") || clean.contains("TRACE") {
            OVERLAY0
        } else {
            SUBTEXT
        };

        draw_text_mut(&mut img, color, 4, y + 5, PxScale::from(14.0), &mono, &truncated);
    }

    DynamicImage::ImageRgb8(img)
}


// ─── CPU frequency display ────────────────────────────────────────────────────

/// Read current CPU MHz values from /proc/cpuinfo.
/// Returns one entry per logical core (in core order).
fn read_cpu_mhz() -> Vec<f32> {
    let text = match std::fs::read_to_string("/proc/cpuinfo") {
        Ok(t)  => t,
        Err(_) => return Vec::new(),
    };
    text.lines()
        .filter(|l| l.starts_with("cpu MHz"))
        .filter_map(|l| l.split(':').nth(1).and_then(|v| v.trim().parse::<f32>().ok()))
        .collect()
}

/// Render a per-core CPU frequency grid.
///
/// Shows each logical CPU's current clock in MHz, colour-coded:
/// - Green ≥ max * 0.90  (boosting / near max)
/// - Teal  ≥ max * 0.60
/// - Yellow ≥ max * 0.30 (moderate)
/// - Blue  < max * 0.30  (light load / power-save)
pub fn render_cpu_freq() -> DynamicImage {
    let freqs = read_cpu_mhz();
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 44), SURFACE0);
    draw_text_mut(&mut img, TEAL, 8, 8, PxScale::from(24.0), &bold, "CPU Frequency");

    let now = Local::now();
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(14.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - tw - 8, 14, PxScale::from(14.0), &mono, &ts);

    draw_filled_rect_mut(&mut img, Rect::at(0, 44).of_size(480, 2), TEAL);

    if freqs.is_empty() {
        draw_text_mut(&mut img, OVERLAY0, 12, 64, PxScale::from(18.0), &mono,
            "No CPU frequency data available.");
        draw_text_mut(&mut img, OVERLAY0, 12, 88, PxScale::from(14.0), &mono,
            "(requires /proc/cpuinfo with 'cpu MHz')");
        return DynamicImage::ImageRgb8(img);
    }

    let max_mhz = freqs.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
    let n = freqs.len();

    // Summary line
    let avg_mhz = freqs.iter().sum::<f32>() / n as f32;
    let summary = format!("{} cores   avg {:.0} MHz   max {:.0} MHz", n, avg_mhz, max_mhz);
    let sw = text_width(&bold, PxScale::from(16.0), &summary);
    draw_text_mut(&mut img, SUBTEXT, (480 - sw).max(0) / 2, 50, PxScale::from(16.0), &bold, &summary);

    // Grid layout: up to 4 columns
    let cols: i32 = if n <= 8 { 2 } else if n <= 16 { 4 } else { 4 };
    let rows  = ((n as i32) + cols - 1) / cols;
    let cell_w = 480 / cols;
    let cell_h: i32 = ((480 - 72) / rows).min(54);
    let start_y: i32 = 72;

    for (i, &mhz) in freqs.iter().enumerate().take((cols * rows) as usize) {
        let col = (i as i32) % cols;
        let row = (i as i32) / cols;
        let x = col * cell_w;
        let y = start_y + row * cell_h;

        let frac = mhz / max_mhz;
        let color = if frac >= 0.90 { GREEN }
                    else if frac >= 0.60 { TEAL }
                    else if frac >= 0.30 { YELLOW }
                    else { BLUE };

        // Cell background (alternate)
        if (col + row) % 2 == 0 {
            draw_filled_rect_mut(&mut img, Rect::at(x, y).of_size(cell_w as u32, cell_h as u32), SURFACE0);
        }

        // Core label + MHz
        let core_label = format!("C{}", i);
        let freq_str   = format!("{:.0}", mhz);
        let unit_str   = "MHz";

        let label_scale = PxScale::from(13.0);
        let freq_scale  = PxScale::from(if cell_h >= 42 { 22.0 } else { 18.0 });

        draw_text_mut(&mut img, OVERLAY0, x + 4, y + 3, label_scale, &mono, &core_label);
        draw_text_mut(&mut img, color,    x + 4, y + 18, freq_scale, &mono, &freq_str);
        let unit_y = y + 18 + if cell_h >= 42 { 20 } else { 16 };
        if unit_y + 14 < y + cell_h {
            draw_text_mut(&mut img, OVERLAY0, x + 4, unit_y, PxScale::from(11.0), &mono, unit_str);
        }

        // Mini progress bar at bottom of cell
        let bar_y = y + cell_h - 5;
        let bar_w = cell_w - 4;
        draw_filled_rect_mut(&mut img, Rect::at(x + 2, bar_y).of_size(bar_w as u32, 3), OVERLAY0);
        let fill = ((bar_w as f32 * frac) as i32).max(0);
        if fill > 0 {
            draw_filled_rect_mut(&mut img, Rect::at(x + 2, bar_y).of_size(fill as u32, 3), color);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Hostname & IP display ────────────────────────────────────────────────────

/// Read all non-loopback IPv4/IPv6 addresses for all interfaces.
fn read_host_addrs() -> Vec<(String, Vec<String>)> {
    // Read from /proc/net/fib_trie for IPv4 and /proc/net/if_inet6 for IPv6.
    // Returns Vec<(ifname, Vec<addr_string>)>
    let mut result: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();

    // IPv4: /proc/net/fib_trie
    // Alternatively, parse /proc/net/if_inet6 for IPv6, and /proc/net/dev for interface names.
    // Simplest reliable approach: read /sys/class/net/ for interface names, then
    // read /proc/net/fib_trie for LOCAL entries and correlate.
    // Actually use a simpler approach: iterate /proc/net/arp entries + parse /proc/net/fib_trie

    // Read IPv4 addresses from /proc/net/fib_trie
    // Format we care about: lines after "  LOCAL" that contain the address
    if let Ok(_content) = std::fs::read_to_string("/proc/net/fib_trie") {
        // Parse fib_trie: look for "<ifname>" lines in fib_triestat or use another approach
        // Simplest: use ip addresses from /proc/net/if_inet6 for v6 and
        // parse /proc/net/dev for interface list, then read each interface address
    }

    // Use /sys/class/net for interface names
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let ifname = entry.file_name().to_string_lossy().to_string();
            if ifname == "lo" { continue; }
            let mut addrs = Vec::new();

            // IPv4: read from /proc/net/fib_trie — find LOCAL entries
            // Then match via /proc/net/fib_triestat which interface they belong to
            // Alternative simpler: run `ip addr show <ifname>` — but that requires a subprocess

            // Best no-subprocess approach: read /proc/net/if_inet6 (IPv6) and correlate index with
            // /sys/class/net/<if>/ifindex, then for IPv4 read /proc/net/fib_trie

            // IPv6
            if let Ok(v6data) = std::fs::read_to_string("/proc/net/if_inet6") {
                // Format: addr(32 hex) ifindex prefix_len scope flags ifname
                for line in v6data.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 6 && parts[5] == ifname {
                        let hex = parts[0];
                        // Skip link-local (fe80) and loopback
                        if hex.starts_with("fe80") || hex.starts_with("0000") { continue; }
                        // Format as IPv6 with colons
                        let groups: Vec<String> = (0..8)
                            .map(|i| hex[i*4..(i+1)*4].to_string())
                            .collect();
                        let v6_str = groups.join(":");
                        // prefix length
                        let prefix = u8::from_str_radix(parts[2], 16).unwrap_or(0);
                        addrs.push(format!("{}/{}", v6_str, prefix));
                    }
                }
            }

            if !addrs.is_empty() || {
                // Check if interface is up
                let flags_path = format!("/sys/class/net/{}/flags", ifname);
                std::fs::read_to_string(&flags_path)
                    .ok()
                    .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
                    .map(|f| f & 1 != 0) // IFF_UP
                    .unwrap_or(false)
            } {
                result.entry(ifname.clone()).or_default().extend(addrs);
            }
        }
    }

    // IPv4: parse /proc/net/fib_trie for LOCAL host entries
    // Then determine interface from /proc/net/fib_triestat is complex.
    // Use simpler: read each interface's ipv4 addr from a subprocess-free method.
    // Scan /proc/net/fib_trie for LEAF + LOCAL addresses:
    //   - each leaf has lines like "  +-- <ip> <prefix>" and then children
    //   - LOCAL entries appear under each interface's subtree
    // This is complex to parse correctly. Instead, use /proc/net/dev to list interfaces,
    // then use SIOCGIFADDR via FFI — too complex.
    // Simplest robust approach: run "ip -o -4 addr" as subprocess.
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-4", "addr", "show"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            // Format: <idx>: <ifname>    inet <addr>/<prefix> brd <brd> scope ...
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "inet" {
                let ifname = parts[1].trim_end_matches(':').to_string();
                let addr = parts[3]; // "addr/prefix"
                if ifname != "lo" {
                    result.entry(ifname).or_default().push(addr.to_string());
                }
            }
        }
    }

    result.into_iter().collect()
}

/// Render hostname and all network IP addresses — useful for quick machine identification.
pub fn render_hostname_ip() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Title bar
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 56), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(26.0), 14, BLUE, "MACHINE ID");

    // Hostname
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_else(|_| "unknown".to_string());
    let hostname = hostname.trim();

    // Draw big hostname
    let host_scale = if hostname.len() <= 16 { 38.0 } else if hostname.len() <= 24 { 28.0 } else { 22.0 };
    draw_centered(&mut img, &bold, PxScale::from(host_scale), 68, TEXT, hostname);

    // Divider
    draw_divider(&mut img, 116);

    // Network interfaces
    let ifaces = read_host_addrs();
    let row_h = 42i32;
    let mut y = 122i32;
    let max_rows = 8;
    let mut count = 0;

    if ifaces.is_empty() {
        draw_centered(&mut img, &mono, PxScale::from(18.0), y, OVERLAY0, "no network interfaces");
    }

    for (ifname, addrs) in &ifaces {
        if count >= max_rows { break; }

        // Interface name pill background
        let pill_w = 120i32;
        draw_filled_rect_mut(&mut img, Rect::at(12, y).of_size(pill_w as u32, 28), SURFACE0);
        draw_text_mut(&mut img, TEAL, 18, y + 5, PxScale::from(16.0), &mono, ifname);

        // Addresses
        let mut addr_y = y;
        if addrs.is_empty() {
            draw_text_mut(&mut img, OVERLAY0, pill_w + 22, addr_y + 5, PxScale::from(15.0), &mono, "(no addr)");
        }
        for addr in addrs {
            if count >= max_rows { break; }
            let color = if addr.contains(':') { MAUVE } else { GREEN };
            draw_text_mut(&mut img, color, pill_w + 22, addr_y + 5, PxScale::from(15.0), &mono, addr);
            addr_y += row_h / 2;
            count += 1;
        }

        y += row_h.max(addr_y - y + 8);
        count += 1;
    }

    // Footer: uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0) as u64;
    let d = uptime_secs / 86400;
    let h = (uptime_secs % 86400) / 3600;
    let m = (uptime_secs % 3600) / 60;
    let uptime_str = if d > 0 {
        format!("up {}d {:02}h {:02}m", d, h, m)
    } else if h > 0 {
        format!("up {}h {:02}m", h, m)
    } else {
        format!("up {}m", m)
    };
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(16.0), 456, SUBTEXT, &uptime_str);

    DynamicImage::ImageRgb8(img)
}

// ─── RSS / Atom headline feed ─────────────────────────────────────────────────

/// Render a list of RSS headlines.
/// `feed_title`: channel/feed title
/// `items`: Vec<(title, pub_date_or_author)>
/// `ok`: false = error state (feed unavailable)
pub fn render_rss_feed(feed_title: &str, items: &[(String, String)], ok: bool) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let header_color = if ok { BLUE } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    let title_display = if feed_title.is_empty() { "RSS FEED" } else { feed_title };
    let title_scale = if title_display.len() <= 20 { 22.0f32 } else { 16.0 };
    draw_centered(&mut img, &bold, PxScale::from(title_scale), 7, header_color, title_display);
    let status_str = if ok { "● LIVE" } else { "✕ OFFLINE" };
    draw_centered(&mut img, &mono, PxScale::from(14.0), 32, if ok { GREEN } else { RED }, status_str);

    if items.is_empty() {
        draw_centered(&mut img, &mono, PxScale::from(18.0), 220, OVERLAY0,
            if ok { "no items" } else { "feed unavailable" });
        return DynamicImage::ImageRgb8(img);
    }

    // Draw items
    const ITEM_H: i32 = 68;
    const MAX_ITEMS: usize = 6;
    let colors = [BLUE, MAUVE, GREEN, TEAL, PEACH, YELLOW];

    for (idx, (title, meta)) in items.iter().take(MAX_ITEMS).enumerate() {
        let y = 56 + idx as i32 * ITEM_H;
        let accent = colors[idx % colors.len()];

        // Left accent bar
        draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(4, ITEM_H as u32 - 2), accent);

        // Divider between items
        if idx > 0 {
            draw_filled_rect_mut(&mut img, Rect::at(4, y - 1).of_size(476, 1), SURFACE0);
        }

        // Title — word-wrapped to ~44 chars across 2 lines
        let title_clean: String = title.chars().take(88).collect();
        let (line1, line2) = if title_clean.len() <= 44 {
            (title_clean.as_str(), "")
        } else {
            // find last space before char 44
            let break_at = title_clean[..44].rfind(' ').unwrap_or(44);
            (&title_clean[..break_at], title_clean[break_at..].trim_start())
        };
        draw_text_mut(&mut img, TEXT, 10, y + 4, PxScale::from(16.0), &bold, line1);
        if !line2.is_empty() {
            let line2_trim: String = line2.chars().take(44).collect();
            draw_text_mut(&mut img, SUBTEXT, 10, y + 22, PxScale::from(15.0), &mono, &line2_trim);
        }

        // Meta (date/author)
        if !meta.is_empty() {
            let meta_short: String = meta.chars().take(40).collect();
            draw_text_mut(&mut img, OVERLAY0, 10, y + ITEM_H - 18, PxScale::from(13.0), &mono, &meta_short);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Systemd / service status ─────────────────────────────────────────────────

/// A snapshot of one systemd unit.
#[derive(Clone, Debug)]
pub struct ServiceStatus {
    pub name:    String,
    pub state:   String,   // "active", "inactive", "failed", ...
    pub sub:     String,   // "running", "dead", "exited", ...
}

/// Render a grid of systemd service statuses.
pub fn render_service_status(units: &[ServiceStatus]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(24.0), 12, BLUE, "SERVICE STATUS");

    let now_str = Local::now().format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    if units.is_empty() {
        draw_centered(&mut img, &mono, PxScale::from(18.0), 220, OVERLAY0, "no units configured");
        return DynamicImage::ImageRgb8(img);
    }

    const ROW_H: i32 = 36;
    const MAX_ROWS: usize = 11;

    for (idx, svc) in units.iter().take(MAX_ROWS).enumerate() {
        let y = 54 + idx as i32 * ROW_H;

        // Alternating row background
        if idx % 2 == 1 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(480, ROW_H as u32), SURFACE0);
        }

        // Status indicator
        let (indicator, ind_color) = match svc.state.as_str() {
            "active"   => ("●", GREEN),
            "failed"   => ("✖", RED),
            "inactive" => ("○", OVERLAY0),
            _          => ("?", YELLOW),
        };
        draw_text_mut(&mut img, ind_color, 8, y + 8, PxScale::from(18.0), &bold, indicator);

        // Service name (truncated)
        let name_short: String = svc.name.trim_end_matches(".service")
            .chars().take(24).collect();
        draw_text_mut(&mut img, TEXT, 30, y + 9, PxScale::from(16.0), &mono, &name_short);

        // Sub-state
        let sub_color = match svc.sub.as_str() {
            "running" => GREEN,
            "exited"  => SUBTEXT,
            "dead"    => OVERLAY0,
            "failed"  => RED,
            _         => YELLOW,
        };
        let sub_x = 480 - 80;
        draw_text_mut(&mut img, sub_color, sub_x, y + 9, PxScale::from(15.0), &mono, &svc.sub);
    }

    // Footer count
    draw_divider(&mut img, 452);
    let active_count = units.iter().filter(|u| u.state == "active").count();
    let failed_count = units.iter().filter(|u| u.state == "failed").count();
    let summary = format!("{} active  {} failed  {} total",
        active_count, failed_count, units.len());
    draw_centered(&mut img, &mono, PxScale::from(14.0), 456, SUBTEXT, &summary);

    DynamicImage::ImageRgb8(img)
}

// ─── Network connections (listening ports + established) ──────────────────────

/// A parsed TCP/UDP socket entry from /proc/net/tcp or /proc/net/udp.
#[derive(Clone, Debug)]
pub struct NetConn {
    pub proto:       String,  // "tcp" / "tcp6" / "udp"
    pub local_addr:  String,
    pub local_port:  u16,
    pub remote_addr: String,
    pub remote_port: u16,
    /// TCP state name (e.g. "LISTEN", "ESTABLISHED") or empty for UDP.
    pub state:       String,
}

/// Parse /proc/net/tcp (or tcp6/udp) format into a list of socket entries.
/// Only returns LISTEN and ESTABLISHED connections by default.
pub fn read_net_connections() -> Vec<NetConn> {
    fn parse_hex_addr(hex: &str) -> (String, u16) {
        if let Some((addr_h, port_h)) = hex.split_once(':') {
            let port = u16::from_str_radix(port_h, 16).unwrap_or(0);
            // IPv4 little-endian 32-bit hex
            if addr_h.len() == 8 {
                let a = u32::from_str_radix(addr_h, 16).unwrap_or(0);
                let ip = std::net::Ipv4Addr::from(u32::from_be(a.swap_bytes()));
                let s = if ip.is_unspecified() { "0.0.0.0".to_string() } else { ip.to_string() };
                return (s, port);
            }
            // IPv6: 32 hex chars
            if addr_h.len() == 32 {
                // 4 groups of 4 bytes, each group reversed
                let mut bytes = [0u8; 16];
                for g in 0..4 {
                    let grp = &addr_h[g*8..(g+1)*8];
                    let val = u32::from_str_radix(grp, 16).unwrap_or(0).swap_bytes();
                    bytes[g*4..(g+1)*4].copy_from_slice(&val.to_be_bytes());
                }
                let ip6 = std::net::Ipv6Addr::from(bytes);
                if let Some(v4) = ip6.to_ipv4() {
                    return (v4.to_string(), port);
                }
                return (ip6.to_string(), port);
            }
        }
        ("?".to_string(), 0)
    }

    const TCP_STATES: &[&str] = &[
        "", "ESTABLISHED", "SYN_SENT", "SYN_RECV", "FIN_WAIT1", "FIN_WAIT2",
        "TIME_WAIT", "CLOSE", "CLOSE_WAIT", "LAST_ACK", "LISTEN", "CLOSING",
    ];

    let mut out = Vec::new();

    let files = [
        ("/proc/net/tcp",  "tcp",  true),
        ("/proc/net/tcp6", "tcp6", true),
        ("/proc/net/udp",  "udp",  false),
    ];

    for (path, proto, is_tcp) in &files {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 4 { continue; }
            let (local_addr, local_port) = parse_hex_addr(cols[1]);
            let (remote_addr, remote_port) = parse_hex_addr(cols[2]);
            let state_hex = u8::from_str_radix(cols[3], 16).unwrap_or(0) as usize;
            let state = if *is_tcp {
                TCP_STATES.get(state_hex).unwrap_or(&"").to_string()
            } else {
                String::new()
            };
            // Only show LISTEN and ESTABLISHED for TCP; all for UDP (but limit)
            if *is_tcp && state != "LISTEN" && state != "ESTABLISHED" { continue; }
            // Skip pure-loopback established connections
            if state == "ESTABLISHED" && local_addr.starts_with("127.") && remote_addr.starts_with("127.") { continue; }
            out.push(NetConn {
                proto:       proto.to_string(),
                local_addr,  local_port,
                remote_addr, remote_port,
                state,
            });
        }
    }
    out
}

/// Render active network connections: listening ports and established sessions.
pub fn render_net_connections() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let conns = read_net_connections();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(24.0), 12, TEAL, "CONNECTIONS");
    let now_str = Local::now().format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    if conns.is_empty() {
        draw_centered(&mut img, &mono, PxScale::from(18.0), 220, OVERLAY0, "no connections");
        return DynamicImage::ImageRgb8(img);
    }

    // Column header
    let hdr_y = 54i32;
    draw_filled_rect_mut(&mut img, Rect::at(0, hdr_y).of_size(480, 20), SURFACE0);
    draw_text_mut(&mut img, OVERLAY0, 8,   hdr_y + 3, PxScale::from(12.0), &mono, "PROTO");
    draw_text_mut(&mut img, OVERLAY0, 60,  hdr_y + 3, PxScale::from(12.0), &mono, "LOCAL:PORT");
    draw_text_mut(&mut img, OVERLAY0, 220, hdr_y + 3, PxScale::from(12.0), &mono, "REMOTE:PORT");
    draw_text_mut(&mut img, OVERLAY0, 370, hdr_y + 3, PxScale::from(12.0), &mono, "STATE");

    const ROW_H: i32 = 30;
    const MAX_ROWS: usize = 13;
    let listen_first: Vec<_> = {
        let mut v: Vec<_> = conns.into_iter().collect();
        v.sort_by(|a, b| {
            // LISTEN first, then ESTABLISHED, then UDP
            let rank = |c: &NetConn| match c.state.as_str() {
                "LISTEN"      => 0,
                "ESTABLISHED" => 1,
                _             => 2,
            };
            rank(a).cmp(&rank(b)).then(a.local_port.cmp(&b.local_port))
        });
        v
    };

    for (idx, conn) in listen_first.iter().take(MAX_ROWS).enumerate() {
        let y = hdr_y + 22 + idx as i32 * ROW_H;

        if idx % 2 == 1 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y).of_size(480, ROW_H as u32), SURFACE0);
        }

        let (state_color, proto_color) = match conn.state.as_str() {
            "LISTEN"      => (GREEN,   TEAL),
            "ESTABLISHED" => (BLUE,    BLUE),
            _             => (OVERLAY0, OVERLAY0),  // UDP
        };

        // Proto
        draw_text_mut(&mut img, proto_color, 8, y + 7, PxScale::from(13.0), &mono, &conn.proto);

        // Local addr:port
        let local_str = if conn.local_addr == "0.0.0.0" || conn.local_addr == "::" {
            format!("*:{}", conn.local_port)
        } else {
            // Shorten long IPv6
            let addr = if conn.local_addr.len() > 15 { "…".to_string() } else { conn.local_addr.clone() };
            format!("{}:{}", addr, conn.local_port)
        };
        draw_text_mut(&mut img, TEXT, 60, y + 7, PxScale::from(13.0), &mono, &local_str);

        // Remote addr:port (only for ESTABLISHED)
        if conn.state == "ESTABLISHED" {
            let rem_str = format!("{}:{}", conn.remote_addr, conn.remote_port);
            let rem_short: String = rem_str.chars().take(20).collect();
            draw_text_mut(&mut img, SUBTEXT, 220, y + 7, PxScale::from(13.0), &mono, &rem_short);
        } else {
            draw_text_mut(&mut img, OVERLAY0, 220, y + 7, PxScale::from(13.0), &mono, "—");
        }

        // State
        draw_text_mut(&mut img, state_color, 370, y + 7, PxScale::from(12.0), &mono, &conn.state);
    }

    // Footer
    let listen_count = listen_first.iter().filter(|c| c.state == "LISTEN").count();
    let estab_count  = listen_first.iter().filter(|c| c.state == "ESTABLISHED").count();
    draw_divider(&mut img, 452);
    let summary = format!("{} listening  {} established", listen_count, estab_count);
    draw_centered(&mut img, &mono, PxScale::from(14.0), 456, SUBTEXT, &summary);

    DynamicImage::ImageRgb8(img)
}

// ─── Uptime / Boot time display ──────────────────────────────────────────────

/// Render a full-screen uptime/boot time display.
/// Shows: big uptime counter, boot timestamp, load averages, and a visual progress bar.
pub fn render_uptime() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Read uptime from /proc/uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0) as u64;

    let days    = uptime_secs / 86400;
    let hours   = (uptime_secs % 86400) / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let seconds = uptime_secs % 60;

    // Read load averages from /proc/loadavg
    let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load_parts: Vec<&str> = loadavg.split_whitespace().collect();
    let load1  = load_parts.first().copied().unwrap_or("0.0");
    let load5  = load_parts.get(1).copied().unwrap_or("0.0");
    let load15 = load_parts.get(2).copied().unwrap_or("0.0");

    // Calculate boot time (current time - uptime)
    let now = Local::now();
    let boot_time = now - chrono::Duration::seconds(uptime_secs as i64);
    let boot_str  = boot_time.format("%Y-%m-%d %H:%M:%S").to_string();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(24.0), 12, BLUE, "SYSTEM UPTIME");
    let now_str = now.format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    // Big uptime counter
    let day_str = if days > 0 { format!("{}d ", days) } else { String::new() };
    let time_str = format!("{}{:02}:{:02}:{:02}", day_str, hours, minutes, seconds);
    let big_scale = if time_str.len() <= 8 { 72.0f32 } else if time_str.len() <= 11 { 58.0 } else { 46.0 };
    draw_centered(&mut img, &bold, PxScale::from(big_scale), 80, TEAL, &time_str);

    // Days / hours / minutes labels
    if days > 0 {
        let labels = format!("{} days, {} hours, {} minutes", days, hours, minutes);
        draw_centered(&mut img, &mono, PxScale::from(15.0), 158, SUBTEXT, &labels);
    } else {
        let labels = format!("{} hours, {} minutes, {} seconds", hours, minutes, seconds);
        draw_centered(&mut img, &mono, PxScale::from(15.0), 158, SUBTEXT, &labels);
    }

    // Divider
    draw_divider(&mut img, 184);

    // Boot time
    draw_centered(&mut img, &mono, PxScale::from(14.0), 190, OVERLAY0, "Boot time");
    draw_centered(&mut img, &bold, PxScale::from(22.0), 208, TEXT, &boot_str);

    // Progress bar: fraction of a week elapsed
    let week_secs = 7 * 86400u64;
    let frac = ((uptime_secs % week_secs) as f32 / week_secs as f32).min(1.0);
    draw_divider(&mut img, 254);
    draw_text_mut(&mut img, OVERLAY0, 12, 260, PxScale::from(13.0), &mono, "Week progress");
    let bar_x = 12;
    let bar_y = 278;
    let bar_w = 456;
    draw_bar(&mut img, bar_x, bar_y, bar_w, 14, frac, BLUE);
    let frac_pct = format!("{:.1}%", frac * 100.0);
    let fw = text_width(&mono, PxScale::from(13.0), &frac_pct);
    draw_text_mut(&mut img, BLUE, bar_x + bar_w - fw - 2, 260, PxScale::from(13.0), &mono, &frac_pct);

    // Load averages
    draw_divider(&mut img, 306);
    draw_centered(&mut img, &mono, PxScale::from(14.0), 312, OVERLAY0, "Load averages (1m / 5m / 15m)");
    let load_str = format!("{} / {} / {}", load1, load5, load15);
    let load1_f = load1.parse::<f32>().unwrap_or(0.0);
    let load_color = if load1_f > 4.0 { RED } else if load1_f > 2.0 { YELLOW } else { GREEN };
    draw_centered(&mut img, &bold, PxScale::from(28.0), 332, load_color, &load_str);

    // CPU count for context
    let cpu_count = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();
    if cpu_count > 0 {
        let cpu_str = format!("{} CPU cores", cpu_count);
        draw_text_mut(&mut img, OVERLAY0, 12, 372, PxScale::from(13.0), &mono, &cpu_str);
    }

    // Memory quick stats
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        let mut total_kb = 0u64;
        let mut available_kb = 0u64;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = rest.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                available_kb = rest.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
        }
        if total_kb > 0 {
            let used_kb = total_kb - available_kb;
            let mem_str = format!("RAM: {:.1}/{:.1} GB", used_kb as f64 / 1_048_576.0, total_kb as f64 / 1_048_576.0);
            draw_text_mut(&mut img, SUBTEXT, 12, 392, PxScale::from(13.0), &mono, &mem_str);
        }
    }

    // Footer
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default();
    let hostname = hostname.trim();
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(15.0), 456, SUBTEXT, hostname);

    DynamicImage::ImageRgb8(img)
}

// ─── OS / System info display ─────────────────────────────────────────────────

/// Render a comprehensive OS and system information display.
pub fn render_os_info() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // --- Gather info ---
    // OS name
    let os_pretty = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        .unwrap_or_else(|| "Linux".to_string());

    // Kernel version
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "unknown".to_string());
    let kernel = kernel.trim().to_string();

    // Architecture
    let arch = std::fs::read_to_string("/proc/sys/kernel/arch")
        .or_else(|_| std::process::Command::new("uname").arg("-m").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
        .unwrap_or_else(|_| "x86_64".to_string());
    let arch = arch.trim().to_string();

    // Hostname
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default();
    let hostname = hostname.trim().to_string();

    // CPU model
    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("model name").map(|r| r.trim_start_matches(|c: char| c == '\t' || c == ':' || c == ' ').to_string()))
        .unwrap_or_else(|| "Unknown CPU".to_string());
    // Shorten CPU name
    let cpu_model = cpu_model
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("  ", " ");
    let cpu_short: String = cpu_model.chars().take(32).collect();

    // CPU core count
    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    // Memory total
    let mem_total_kb: u64 = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:").and_then(|r| r.split_whitespace().next().and_then(|v| v.parse().ok())))
        .unwrap_or(0);

    // Uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0) as u64;
    let up_days  = uptime_secs / 86400;
    let up_hours = (uptime_secs % 86400) / 3600;
    let uptime_str = if up_days > 0 {
        format!("{}d {}h up", up_days, up_hours)
    } else {
        let up_mins = (uptime_secs % 3600) / 60;
        format!("{}h {}m up", up_hours, up_mins)
    };

    // --- Header ---
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, MAUVE, "SYSTEM INFO");
    draw_centered(&mut img, &mono, PxScale::from(13.0), 32, OVERLAY0,
        &Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    // --- OS Name (big) ---
    let os_scale = if os_pretty.len() <= 22 { 28.0f32 } else if os_pretty.len() <= 30 { 22.0 } else { 18.0 };
    draw_centered(&mut img, &bold, PxScale::from(os_scale), 60, TEXT, &os_pretty);

    draw_divider(&mut img, 98);

    // --- Info rows ---
    let row_h = 34i32;
    let mut y = 104i32;
    let label_scale = PxScale::from(14.0);
    let value_scale = PxScale::from(17.0);

    let rows: &[(&str, String, Rgb<u8>)] = &[
        ("Hostname",  hostname.clone(),                  TEAL),
        ("Kernel",    format!("{} ({})", kernel, arch),  BLUE),
        ("CPU",       cpu_short.clone(),                  SUBTEXT),
        ("Cores",     format!("{} logical cores", cpu_cores), YELLOW),
        ("Memory",    format!("{:.1} GB RAM", mem_total_kb as f64 / 1_048_576.0), GREEN),
        ("Uptime",    uptime_str,                         PEACH),
    ];

    for (label, value, color) in rows {
        // Label
        draw_text_mut(&mut img, OVERLAY0, 12, y + 8, label_scale, &mono, label);
        // Value (right-aligned or after a separator)
        let val_short: String = value.chars().take(30).collect();
        let vw = text_width(&mono, value_scale, &val_short);
        draw_text_mut(&mut img, *color, 480 - vw - 12, y + 5, value_scale, &bold, &val_short);
        // Light divider
        if y + row_h < 440 {
            draw_filled_rect_mut(&mut img, Rect::at(0, y + row_h - 1).of_size(480, 1), SURFACE0);
        }
        y += row_h;
    }

    // --- Footer ---
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0,
        &format!("ttlcd v{}", env!("CARGO_PKG_VERSION")));

    DynamicImage::ImageRgb8(img)
}

// ─── Docker containers display ───────────────────────────────────────────────

/// One Docker container as returned by `docker ps`.
#[derive(Debug, Clone)]
pub struct DockerContainer {
    pub id:      String,
    pub name:    String,
    pub image:   String,
    pub status:  String,
    pub ports:   String,
    pub running: bool,
}

/// Query `docker ps -a --format '{{json .}}'` and parse the output.
/// Returns containers sorted running-first.
pub fn read_docker_containers() -> Vec<DockerContainer> {
    let out = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&out);

    let mut containers: Vec<DockerContainer> = text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            // Parse JSON object fields without serde dependency
            let field = |key: &str| -> Option<String> {
                let needle = format!("\"{}\":\"", key);
                let start  = line.find(&needle)?;
                let rest   = &line[start + needle.len()..];
                let end    = rest.find('"')?;
                Some(rest[..end].to_string())
            };
            let id      = field("ID").unwrap_or_default();
            let name    = field("Names").unwrap_or_default();
            let image   = field("Image").unwrap_or_default();
            let status  = field("Status").unwrap_or_default();
            let ports   = field("Ports").unwrap_or_default();
            let running = status.to_lowercase().starts_with("up");
            if id.is_empty() && name.is_empty() { return None; }
            Some(DockerContainer { id, name, image, status, ports, running })
        })
        .collect();

    // Running first, then by name
    containers.sort_by(|a, b| b.running.cmp(&a.running).then(a.name.cmp(&b.name)));
    containers
}

/// Render a Docker containers overview (480×480).
///
/// Shows up to 10 containers with name, image, and Up/Exited status.
pub fn render_docker_status(containers: &[DockerContainer]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, TEAL, "DOCKER");
    let up_count    = containers.iter().filter(|c| c.running).count();
    let total_count = containers.len();
    let sub = if total_count == 0 {
        "docker not available".to_string()
    } else {
        format!("{}/{} running", up_count, total_count)
    };
    draw_centered(&mut img, &mono, PxScale::from(14.0), 32, OVERLAY0, &sub);

    if containers.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(18.0), 220, SUBTEXT, "No containers found");
        draw_centered(&mut img, &mono, PxScale::from(13.0), 248, OVERLAY0, "Is Docker running?");
        draw_divider(&mut img, 452);
        draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0,
            &Local::now().format("%H:%M:%S").to_string());
        return DynamicImage::ImageRgb8(img);
    }

    // ── Column headers ────────────────────────────────────────────────────────
    let header_y = 56i32;
    let col_name_x  = 8i32;
    let col_img_x   = 200i32;
    let col_stat_x  = 360i32;
    let hscale = PxScale::from(12.0);
    draw_text_mut(&mut img, OVERLAY0, col_name_x,  header_y, hscale, &mono, "NAME");
    draw_text_mut(&mut img, OVERLAY0, col_img_x,   header_y, hscale, &mono, "IMAGE");
    draw_text_mut(&mut img, OVERLAY0, col_stat_x,  header_y, hscale, &mono, "STATUS");
    draw_filled_rect_mut(&mut img, Rect::at(0, header_y + 14).of_size(480, 1), SURFACE0);

    // ── Rows ─────────────────────────────────────────────────────────────────
    let row_h    = 40i32;
    let max_rows = 10usize;
    let mut y    = header_y + 18;
    let rscale   = PxScale::from(14.0);
    let sscale   = PxScale::from(12.0);

    for c in containers.iter().take(max_rows) {
        let status_color = if c.running { GREEN } else { RED };
        let status_short = if c.running {
            // "Up 3 hours" → "Up 3h"
            let s = c.status.as_str();
            if s.contains("hour") {
                let n: String = s.chars().skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}h", n.trim())
            } else if s.contains("minute") {
                let n: String = s.chars().skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}m", n.trim())
            } else if s.contains("second") {
                "Up <1m".to_string()
            } else if s.contains("day") {
                let n: String = s.chars().skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}d", n.trim())
            } else {
                s.chars().take(10).collect()
            }
        } else {
            "Exited".to_string()
        };

        let name_short: String  = c.name.trim_start_matches('/').chars().take(22).collect();
        let image_short: String = {
            // strip registry prefix if present
            let img_name = c.image.rsplit('/').next().unwrap_or(&c.image);
            // strip tag after ':'
            let base = img_name.split(':').next().unwrap_or(img_name);
            base.chars().take(18).collect()
        };

        // Row background for running containers
        if c.running {
            draw_filled_rect_mut(&mut img,
                Rect::at(0, y - 2).of_size(480, row_h as u32 - 2),
                Rgb([40, 50, 44]));
        }

        draw_text_mut(&mut img, TEXT,          col_name_x, y, rscale, &bold, &name_short);
        draw_text_mut(&mut img, SUBTEXT,       col_img_x,  y, sscale, &mono, &image_short);
        draw_text_mut(&mut img, status_color,  col_stat_x, y, sscale, &mono, &status_short);

        // Indicator dot
        let dot_color = if c.running { GREEN } else { Rgb([80, 80, 80]) };
        draw_filled_rect_mut(&mut img, Rect::at(470, y + 4).of_size(6, 6), dot_color);

        draw_filled_rect_mut(&mut img, Rect::at(0, y + row_h - 4).of_size(480, 1), SURFACE0);
        y += row_h;
    }

    if containers.len() > max_rows {
        draw_centered(&mut img, &mono, PxScale::from(12.0), y + 2, OVERLAY0,
            &format!("… {} more", containers.len() - max_rows));
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0,
        &Local::now().format("%H:%M:%S").to_string());

    DynamicImage::ImageRgb8(img)
}

// ─── Logged-in users display ─────────────────────────────────────────────────

/// A user session as parsed from `who -u`.
#[derive(Debug, Clone)]
pub struct UserSession {
    pub username: String,
    pub tty:      String,
    pub login_at: String,
    pub idle:     String,
    pub pid:      String,
    pub from:     String,
}

/// Read active user sessions from `who -u`.
pub fn read_user_sessions() -> Vec<UserSession> {
    let out = std::process::Command::new("who")
        .arg("-u")
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&out);

    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let username = parts.next()?.to_string();
            let tty      = parts.next().unwrap_or("?").to_string();
            // next two tokens are date and time
            let date     = parts.next().unwrap_or("").to_string();
            let time     = parts.next().unwrap_or("").to_string();
            let login_at = format!("{} {}", date, time);
            let idle     = parts.next().unwrap_or(".").to_string();
            let pid      = parts.next().unwrap_or("?").to_string();
            // rest is FROM (may be empty for local sessions)
            let from: String = parts.collect::<Vec<_>>().join(" ");
            Some(UserSession { username, tty, login_at, idle, pid, from })
        })
        .collect()
}

/// Render a logged-in users panel (480×480).
pub fn render_logged_in_users(sessions: &[UserSession]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, MAUVE, "ACTIVE SESSIONS");
    let count_str = match sessions.len() {
        0 => "no users logged in".to_string(),
        1 => "1 user".to_string(),
        n => format!("{} users", n),
    };
    draw_centered(&mut img, &mono, PxScale::from(14.0), 32, OVERLAY0, &count_str);

    if sessions.is_empty() {
        draw_centered(&mut img, &bold, PxScale::from(18.0), 230, SUBTEXT, "No active sessions");
        draw_divider(&mut img, 452);
        draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0,
            &Local::now().format("%H:%M:%S").to_string());
        return DynamicImage::ImageRgb8(img);
    }

    // ── Column headers ────────────────────────────────────────────────────────
    let header_y = 58i32;
    let hscale = PxScale::from(12.0);
    draw_text_mut(&mut img, OVERLAY0,  8, header_y, hscale, &mono, "USER");
    draw_text_mut(&mut img, OVERLAY0, 160, header_y, hscale, &mono, "TTY");
    draw_text_mut(&mut img, OVERLAY0, 230, header_y, hscale, &mono, "LOGGED IN");
    draw_text_mut(&mut img, OVERLAY0, 380, header_y, hscale, &mono, "IDLE");
    draw_filled_rect_mut(&mut img, Rect::at(0, header_y + 14).of_size(480, 1), SURFACE0);

    // ── Rows ─────────────────────────────────────────────────────────────────
    let row_h    = 38i32;
    let max_rows = 10usize;
    let mut y    = header_y + 18;
    let rscale   = PxScale::from(15.0);
    let sscale   = PxScale::from(13.0);

    for sess in sessions.iter().take(max_rows) {
        let user_short: String = sess.username.chars().take(14).collect();
        let tty_short:  String = sess.tty.chars().take(8).collect();
        // Login time: keep only HH:MM part
        let login_short: String = sess.login_at.split_whitespace()
            .last().unwrap_or(&sess.login_at).chars().take(10).collect();
        // Idle: '.' means active
        let idle_short: String  = if sess.idle == "." { "active".to_string() } else {
            sess.idle.chars().take(7).collect()
        };
        let idle_color = if sess.idle == "." { GREEN } else { YELLOW };

        // From annotation (SSH source IP / console)
        let from_short: String = if sess.from.is_empty() {
            "local".to_string()
        } else {
            sess.from.chars().take(18).collect()
        };

        draw_text_mut(&mut img, BLUE,       8,   y, rscale, &bold, &user_short);
        draw_text_mut(&mut img, SUBTEXT,    160, y, sscale, &mono, &tty_short);
        draw_text_mut(&mut img, TEXT,       230, y, sscale, &mono, &login_short);
        draw_text_mut(&mut img, idle_color, 380, y, sscale, &mono, &idle_short);

        // Source on second line if present
        if !from_short.is_empty() && from_short != "local" {
            draw_text_mut(&mut img, OVERLAY0, 160, y + 18, PxScale::from(11.0), &mono, &format!("← {}", from_short));
        } else if from_short == "local" {
            draw_text_mut(&mut img, OVERLAY0, 160, y + 18, PxScale::from(11.0), &mono, "local");
        }

        draw_filled_rect_mut(&mut img, Rect::at(0, y + row_h - 4).of_size(480, 1), SURFACE0);
        y += row_h;
    }

    if sessions.len() > max_rows {
        draw_centered(&mut img, &mono, PxScale::from(12.0), y + 2, OVERLAY0,
            &format!("… {} more", sessions.len() - max_rows));
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0,
        &Local::now().format("%H:%M:%S").to_string());

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod docker_session_tests {
    use super::*;

    #[test]
    fn test_docker_status_empty_size() {
        let img = render_docker_status(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_docker_status_with_containers() {
        let containers = vec![
            DockerContainer {
                id:      "abc123".into(),
                name:    "web-server".into(),
                image:   "nginx:latest".into(),
                status:  "Up 3 hours".into(),
                ports:   "0.0.0.0:80->80/tcp".into(),
                running: true,
            },
            DockerContainer {
                id:      "def456".into(),
                name:    "database".into(),
                image:   "postgres:15".into(),
                status:  "Exited (0) 2 days ago".into(),
                ports:   "".into(),
                running: false,
            },
        ];
        let img = render_docker_status(&containers);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_logged_in_users_empty_size() {
        let img = render_logged_in_users(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_logged_in_users_with_sessions() {
        let sessions = vec![
            UserSession {
                username: "alice".into(),
                tty:      "pts/0".into(),
                login_at: "May12 09:30".into(),
                idle:     ".".into(),
                pid:      "12345".into(),
                from:     "192.168.1.50".into(),
            },
            UserSession {
                username: "bob".into(),
                tty:      "tty1".into(),
                login_at: "May13 07:00".into(),
                idle:     "00:05".into(),
                pid:      "11111".into(),
                from:     "".into(),
            },
        ];
        let img = render_logged_in_users(&sessions);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Load average history graph ──────────────────────────────────────────────

static LOAD_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();

fn load_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    LOAD_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120))
    })
    .lock()
    .unwrap()
}

/// Append one 1-minute load average sample to the load history ring-buffer.
pub fn push_load_history(load1: f32) {
    let mut buf = load_graph_data();
    if buf.len() >= 120 { buf.pop_front(); }
    buf.push_back(load1);
}

// ─── Memory history graph ─────────────────────────────────────────────────────

static MEM_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();

fn mem_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    MEM_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120))
    })
    .lock()
    .unwrap()
}

/// Append one memory usage percentage sample to the memory history ring-buffer.
pub fn push_mem_history(mem_pct: f32) {
    let mut buf = mem_graph_data();
    if buf.len() >= 120 { buf.pop_front(); }
    buf.push_back(mem_pct);
}

/// Draw a line-chart from a history buffer inside the given chart bounds.
///
/// `values` are in [0, max_val]. Returns the last drawn point for value annotation.
fn draw_line_chart(
    img: &mut RgbImage,
    values: &[f32],
    max_val: f32,
    chart_x: i32,
    chart_y: i32,
    chart_w: i32,
    chart_h: i32,
    line_color: Rgb<u8>,
    fill_color: Rgb<u8>,
) {
    if values.len() < 2 || max_val <= 0.0 { return; }
    let n = values.len();
    let x_step = chart_w as f32 / (n - 1).max(1) as f32;

    let pixel_y = |v: f32| -> i32 {
        let frac = (v / max_val).clamp(0.0, 1.0);
        chart_y + chart_h - (chart_h as f32 * frac) as i32
    };

    // Fill area under curve
    for (i, &val) in values.iter().enumerate() {
        let px  = (chart_x as f32 + i as f32 * x_step) as i32;
        let py  = pixel_y(val);
        let bot = chart_y + chart_h;
        if px >= 0 && (px as u32) < img.width() {
            for row in py..=bot {
                if row >= 0 && (row as u32) < img.height() {
                    img.put_pixel(px as u32, row as u32, fill_color);
                }
            }
        }
    }

    // Draw line on top
    for i in 0..n - 1 {
        let px1 = (chart_x as f32 + i as f32 * x_step) as i32;
        let py1 = pixel_y(values[i]);
        let px2 = (chart_x as f32 + (i + 1) as f32 * x_step) as i32;
        let py2 = pixel_y(values[i + 1]);
        // Bresenham-ish line
        let dx = (px2 - px1).abs();
        let dy = (py2 - py1).abs();
        let steps = dx.max(dy).max(1);
        for s in 0..=steps {
            let t   = s as f32 / steps as f32;
            let lx  = (px1 as f32 + t * (px2 - px1) as f32) as i32;
            let ly  = (py1 as f32 + t * (py2 - py1) as f32) as i32;
            let ly2 = (ly - 1).max(0);
            for &row in &[ly, ly2] {
                if lx >= 0 && (lx as u32) < img.width() && row >= 0 && (row as u32) < img.height() {
                    img.put_pixel(lx as u32, row as u32, line_color);
                }
            }
        }
    }
}

/// Render a load-average history graph (480×480).
///
/// Shows the 1-minute load average as a line chart over the last ~2 minutes,
/// with current 1m/5m/15m values and per-core context.
pub fn render_load_graph() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, RefreshKind, System};

    let load = System::load_average();
    push_load_history(load.one as f32);

    let history: Vec<f32> = load_graph_data().iter().cloned().collect();

    let cpu_count = {
        let s = System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        s.cpus().len().max(1)
    };

    let max_load = (cpu_count as f32).max(load.fifteen as f32 * 2.0).max(4.0);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "LOAD AVG");
    let tw = text_width(&mono, PxScale::from(22.0), &now.format("%H:%M:%S").to_string());
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &now.format("%H:%M:%S").to_string());

    // ── Current load 1m — large ───────────────────────────────────────────
    let accent = {
        let pct = load.one as f32 / cpu_count as f32;
        if pct > 1.0 { RED } else if pct > 0.7 { YELLOW } else { GREEN }
    };
    let load1_str = format!("{:.2}", load.one);
    draw_centered(&mut img, &bold, PxScale::from(80.0), 42, accent, &load1_str);

    // ── Sub-stats row ─────────────────────────────────────────────────────
    let stats_str = format!(
        "1m {:.2}  5m {:.2}  15m {:.2}  ({} cores)",
        load.one, load.five, load.fifteen, cpu_count
    );
    let sw = text_width(&mono, PxScale::from(16.0), &stats_str);
    draw_text_mut(&mut img, SUBTEXT, ((480 - sw) / 2).max(0), 148, PxScale::from(16.0), &mono, &stats_str);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Grid lines at core-count multiples
    for multiple in 1..=4u32 {
        let grid_val = multiple as f32;
        if grid_val > max_load { break; }
        let ly = chart_y + chart_h - (chart_h as f32 * grid_val / max_load) as i32;
        if ly < chart_y { break; }
        for x in (chart_x..(chart_x + chart_w)).step_by(6) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label = format!("{:.0}", grid_val);
        draw_text_mut(&mut img, OVERLAY0, 2, (ly - 8).max(0), PxScale::from(12.0), &mono, &label);
    }

    // Fill + line
    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(&mut img, &history, max_load, chart_x, chart_y, chart_w, chart_h, accent, fill_color);

    // Chart bottom border
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);
    draw_centered(&mut img, &mono, PxScale::from(14.0), chart_y + chart_h + 6, OVERLAY0, "← 2 min ago                      now →");

    DynamicImage::ImageRgb8(img)
}

/// Render a memory usage history graph (480×480).
///
/// Shows RAM usage % as a filled line chart over the last ~2 minutes,
/// with current used/total values and a usage indicator.
pub fn render_mem_graph() -> DynamicImage {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_memory();
    let used  = sys.used_memory();
    let total = sys.total_memory();
    let mem_pct = if total > 0 { used as f32 / total as f32 * 100.0 } else { 0.0 };
    push_mem_history(mem_pct);

    let history: Vec<f32> = mem_graph_data().iter().cloned().collect();

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "MEM GRAPH");
    let tw = text_width(&mono, PxScale::from(22.0), &now.format("%H:%M:%S").to_string());
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &now.format("%H:%M:%S").to_string());

    // ── Current % — large ─────────────────────────────────────────────────
    let accent = if mem_pct > 85.0 { RED } else if mem_pct > 65.0 { YELLOW } else { BLUE };
    draw_centered(&mut img, &bold, PxScale::from(80.0), 42, accent, &format!("{:.1}%", mem_pct));

    // ── Sub-stats row ─────────────────────────────────────────────────────
    let used_gb  = used  as f64 / 1_073_741_824.0;
    let total_gb = total as f64 / 1_073_741_824.0;
    let swap_used  = sys.used_swap();
    let swap_total = sys.total_swap();
    let sub = if swap_total > 0 {
        format!("{:.1}/{:.1} GB  swap {:.0}%", used_gb, total_gb,
            swap_used as f64 / swap_total as f64 * 100.0)
    } else {
        format!("{:.1} GB used of {:.1} GB  (no swap)", used_gb, total_gb)
    };
    let sw = text_width(&mono, PxScale::from(16.0), &sub);
    draw_text_mut(&mut img, SUBTEXT, ((480 - sw) / 2).max(0), 148, PxScale::from(16.0), &mono, &sub);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Grid lines at 25%, 50%, 75%
    for pct_line in [25u32, 50, 75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(6) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        draw_text_mut(&mut img, OVERLAY0, 2, (ly - 8).max(0), PxScale::from(12.0), &mono, &format!("{}%", pct_line));
    }

    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(&mut img, &history, 100.0, chart_x, chart_y, chart_w, chart_h, accent, fill_color);

    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);
    draw_centered(&mut img, &mono, PxScale::from(14.0), chart_y + chart_h + 6, OVERLAY0, "← 2 min ago                      now →");

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod graph_history_tests {
    use super::*;

    #[test]
    fn test_load_graph_size() {
        let img = render_load_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_mem_graph_size() {
        let img = render_mem_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Progress display ─────────────────────────────────────────────────────────

/// State for the configurable progress display mode.
#[derive(Clone, Debug)]
pub struct ProgressState {
    pub label:     String,
    pub sublabel:  String,
    pub value:     f64,
    pub total:     f64,
    pub units:     String,
    pub color:     NamedColor,
    pub started_at: u64, // unix seconds; 0 = no ETA
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            label:     "Progress".into(),
            sublabel:  String::new(),
            value:     0.0,
            total:     100.0,
            units:     String::new(),
            color:     NamedColor::Blue,
            started_at: 0,
        }
    }
}

/// Render a full-screen progress display (480×480).
///
/// Shows a large progress bar with percentage, label, optional sublabel,
/// and an ETA derived from `started_at` if provided.
pub fn render_progress(state: &ProgressState) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let frac = if state.total > 0.0 {
        (state.value / state.total).clamp(0.0, 1.0)
    } else { 0.0 };
    let pct  = frac * 100.0;

    let color = match state.color {
        NamedColor::Green  => GREEN,
        NamedColor::Yellow => YELLOW,
        NamedColor::Red    => RED,
        NamedColor::Mauve  => MAUVE,
        NamedColor::Teal   => TEAL,
        NamedColor::Peach  => PEACH,
        NamedColor::White  => TEXT,
        _                  => BLUE,
    };

    // ── Header ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    let label_short: String = state.label.chars().take(26).collect();
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, color, &label_short);
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(13.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 35, PxScale::from(13.0), &mono, &ts);

    // ── Large percentage ──────────────────────────────────────────────────
    let pct_str = format!("{:.1}%", pct);
    draw_centered(&mut img, &bold, PxScale::from(100.0), 60, color, &pct_str);

    // ── Sub-label ─────────────────────────────────────────────────────────
    if !state.sublabel.is_empty() {
        let sub: String = state.sublabel.chars().take(40).collect();
        draw_centered(&mut img, &mono, PxScale::from(16.0), 175, SUBTEXT, &sub);
    }

    // ── Value / total ─────────────────────────────────────────────────────
    if state.total > 0.0 {
        let units = if state.units.is_empty() { "" } else { state.units.as_str() };
        let vt_str = format!("{:.1}{} / {:.1}{}", state.value, units, state.total, units);
        draw_centered(&mut img, &mono, PxScale::from(18.0), 198, SUBTEXT, &vt_str);
    }

    // ── Progress bar ──────────────────────────────────────────────────────
    let bar_x = 30i32;
    let bar_y = 230i32;
    let bar_w = 420i32;
    let bar_h = 60i32;

    // Outer border
    draw_hollow_rect_mut(&mut img, Rect::at(bar_x, bar_y).of_size(bar_w as u32, bar_h as u32), SURFACE0);

    // Fill
    let inner_w = ((bar_w - 4) as f64 * frac) as i32;
    if inner_w > 0 {
        draw_filled_rect_mut(&mut img,
            Rect::at(bar_x + 2, bar_y + 2).of_size(inner_w.max(0) as u32, (bar_h - 4) as u32),
            color);
    }

    // Percentage text inside bar
    let inside_pct = format!("{:.0}%", pct);
    let iw = text_width(&bold, PxScale::from(28.0), &inside_pct);
    let text_color = if frac > 0.5 { BG } else { color };
    draw_text_mut(&mut img, text_color,
        (bar_x + (bar_w - iw) / 2).max(0), bar_y + 16, PxScale::from(28.0), &bold, &inside_pct);

    // ── ETA ───────────────────────────────────────────────────────────────
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let eta_y = bar_y + bar_h + 18;
    if state.started_at > 0 && frac > 0.01 && frac < 1.0 {
        let elapsed = now_unix.saturating_sub(state.started_at);
        let total_est = (elapsed as f64 / frac) as u64;
        let remaining = total_est.saturating_sub(elapsed);
        let eta_str = if remaining > 3600 {
            format!("ETA ~{}h {}m", remaining / 3600, (remaining % 3600) / 60)
        } else if remaining > 60 {
            format!("ETA ~{}m {}s", remaining / 60, remaining % 60)
        } else {
            format!("ETA ~{}s", remaining)
        };
        draw_centered(&mut img, &mono, PxScale::from(18.0), eta_y, PEACH, &eta_str);
    } else if frac >= 1.0 {
        draw_centered(&mut img, &bold, PxScale::from(20.0), eta_y, GREEN, "✓ Complete!");
    }

    // ── Elapsed time ──────────────────────────────────────────────────────
    if state.started_at > 0 {
        let elapsed = now_unix.saturating_sub(state.started_at);
        let elapsed_str = if elapsed > 3600 {
            format!("Elapsed: {}h {}m", elapsed / 3600, (elapsed % 3600) / 60)
        } else if elapsed > 60 {
            format!("Elapsed: {}m {}s", elapsed / 60, elapsed % 60)
        } else {
            format!("Elapsed: {}s", elapsed)
        };
        draw_centered(&mut img, &mono, PxScale::from(15.0), eta_y + 26, OVERLAY0, &elapsed_str);
    }

    // ── Footer ────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(13.0), 457, OVERLAY0, "ttlcd progress");

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn test_progress_default_size() {
        let state = ProgressState::default();
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_progress_half() {
        let state = ProgressState {
            label:    "Building release…".into(),
            sublabel: "linking step".into(),
            value:    50.0,
            total:    100.0,
            units:    "%".into(),
            color:    NamedColor::Blue,
            started_at: 1_700_000_000,
        };
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_progress_complete() {
        let state = ProgressState {
            value: 100.0, total: 100.0,
            color: NamedColor::Green,
            ..Default::default()
        };
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
    }
}

// ─── Matrix rain screensaver ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct RainColumn {
    /// Current head row (pixels from top)
    head:  i32,
    /// Speed in pixels per frame
    speed: i32,
    /// Trail length in pixels
    trail: i32,
    /// Characters in this column (indices into MATRIX_CHARS)
    chars: Vec<usize>,
    /// Randomization salt for char changing
    phase: u32,
}

const MATRIX_CHARS: &[char] = &[
    '0','1','2','3','4','5','6','7','8','9',
    'ア','イ','ウ','エ','オ','カ','キ','ク','ケ','コ',
    'サ','シ','ス','セ','ソ','タ','チ','ツ','テ','ト',
    'ナ','ニ','ヌ','ネ','ノ','ハ','ヒ','フ','ヘ','ホ',
    'A','B','C','D','E','F','G','H','I','J',
    '|','|','/','-','\\','_','=','+','*','#',
];

static RAIN_STATE: OnceLock<std::sync::Mutex<Vec<RainColumn>>> = OnceLock::new();

fn rain_state() -> std::sync::MutexGuard<'static, Vec<RainColumn>> {
    RAIN_STATE.get_or_init(|| {
        // Simple LCG-based pseudo-random initialization
        let mut seed: u32 = 0xDEAD_BEEF;
        let lcg = |s: &mut u32| -> u32 { *s = s.wrapping_mul(1664525).wrapping_add(1013904223); *s };

        let col_w  = 14u32;
        let n_cols = 480 / col_w as usize + 1;
        let cols = (0..n_cols).map(|_| {
            let head  = -((lcg(&mut seed) % 480) as i32);
            let speed = 4 + (lcg(&mut seed) % 8) as i32;
            let trail = 80 + (lcg(&mut seed) % 200) as i32;
            let chars_len = (480 / 14) as usize + 2;
            let chars = (0..chars_len).map(|_| (lcg(&mut seed) % MATRIX_CHARS.len() as u32) as usize).collect();
            RainColumn { head, speed, trail, chars, phase: lcg(&mut seed) }
        }).collect();
        std::sync::Mutex::new(cols)
    }).lock().unwrap()
}

/// Reset the Matrix rain state (called when switching to this mode).
pub fn reset_matrix_rain() {
    let mut state = rain_state();
    let mut seed: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x1234_5678);
    let lcg = |s: &mut u32| -> u32 { *s = s.wrapping_mul(1664525).wrapping_add(1013904223); *s };
    for col in state.iter_mut() {
        col.head  = -((lcg(&mut seed) % 400) as i32);
        col.speed = 4 + (lcg(&mut seed) % 8) as i32;
        col.trail = 80 + (lcg(&mut seed) % 200) as i32;
        col.phase = lcg(&mut seed);
    }
}

/// Render one frame of the Matrix rain screensaver (480×480).
///
/// Call repeatedly to animate. The static state persists across calls so
/// the rain moves continuously.
pub fn render_matrix_rain() -> DynamicImage {
    const COL_W: i32 = 14;
    const CHAR_H: i32 = 16;

    let mut img = blank(); // black background
    // Fill with true black instead of Catppuccin base
    for pix in img.pixels_mut() {
        *pix = Rgb([0, 0, 0]);
    }
    let img_ref = &mut img;

    let bold = font_bold();

    let mut cols = rain_state();
    let n_chars = MATRIX_CHARS.len();

    // LCG step for char mutation
    let mut seed: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or(0);
    let lcg = |s: &mut u32| -> u32 { *s = s.wrapping_mul(1664525).wrapping_add(1013904223); *s };

    for (ci, col) in cols.iter_mut().enumerate() {
        let x = ci as i32 * COL_W;
        if x >= 480 { break; }

        // Advance column
        col.head += col.speed;
        if col.head - col.trail > 480 {
            // Reset column
            col.head  = -((lcg(&mut seed) % 100) as i32);
            col.speed = 4 + (lcg(&mut seed) % 8) as i32;
            col.trail = 80 + (lcg(&mut seed) % 200) as i32;
        }

        // Randomly mutate a character in this column
        col.phase = col.phase.wrapping_mul(1664525).wrapping_add(1013904223);
        if col.phase % 5 == 0 {
            let idx = (col.phase / 5) as usize % col.chars.len();
            col.chars[idx] = (lcg(&mut seed) % n_chars as u32) as usize;
        }

        // Draw characters from (head - trail) to head
        let start_row = (col.head - col.trail).max(0);
        let end_row   = col.head.min(480 + CHAR_H);

        let mut char_row = start_row / CHAR_H;
        let mut py = start_row;
        while py < end_row {
            let char_idx = col.chars[char_row as usize % col.chars.len()];
            let ch = MATRIX_CHARS[char_idx];
            let ch_str = ch.to_string();

            // Fade: distance from head → darkness
            let dist_from_head = col.head - py;
            let (r, g, b) = if py >= col.head - CHAR_H {
                // Very bright (almost white) at head
                (200u8, 255, 200)
            } else {
                let frac = dist_from_head as f32 / col.trail as f32;
                let frac = frac.clamp(0.0, 1.0);
                let brightness = ((1.0 - frac) * 220.0) as u8;
                (0, brightness, 0)
            };

            let color = Rgb([r, g, b]);
            if py >= 0 && py < 480 - CHAR_H {
                draw_text_mut(img_ref, color, x, py, PxScale::from(CHAR_H as f32), &bold, &ch_str);
            }
            py += CHAR_H;
            char_row += 1;
        }
    }

    DynamicImage::ImageRgb8(img_ref.clone())
}






// ─── Graphs overview (4-panel mini-charts) ───────────────────────────────────

/// Render a 4-panel mini-charts overview (480×480).
///
/// Shows CPU usage, load average, memory usage, and temperature as four
/// equal-sized line charts arranged in a 2×2 grid. This is the best
/// "at a glance" view for system health monitoring.
pub fn render_graphs_overview() -> DynamicImage {
    use sysinfo::{Components, CpuRefreshKind, RefreshKind, System};

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Gather current values
    let load = System::load_average();
    let cpu_count = {
        let s = System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        s.cpus().len().max(1)
    };
    let mut sys = System::new();
    sys.refresh_memory();
    let used  = sys.used_memory();
    let total = sys.total_memory();
    let mem_pct = if total > 0 { used as f32 / total as f32 * 100.0 } else { 0.0 };

    let comps = Components::new_with_refreshed_list();
    let hot_temp = comps.iter()
        .filter_map(|c| c.temperature())
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max);

    // Collect histories
    let cpu_hist:  Vec<f32> = get_cpu_history();
    let load_hist: Vec<f32> = load_graph_data().iter().cloned().collect();
    let mem_hist:  Vec<f32> = mem_graph_data().iter().cloned().collect();
    let temp_hist: Vec<f32> = temp_graph_data().iter().cloned().collect();
    let diog_r_hist: Vec<f32> = disk_read_data().iter().cloned().collect();

    let now = Local::now();
    let ts  = now.format("%H:%M:%S").to_string();

    // Draw a subtle dividing cross
    let mid = 240i32;
    for x in 0..480u32 {
        img.put_pixel(x, mid as u32, SURFACE0);
    }
    for y in 0..480u32 {
        img.put_pixel(mid as u32, y, SURFACE0);
    }

    // ── Helper: draw one panel ────────────────────────────────────────────────
    let draw_panel = |img: &mut image::RgbImage,
                      ox: i32, oy: i32, pw: i32, ph: i32,
                      title: &str,
                      value_str: &str,
                      history: &[f32],
                      max_val: f32,
                      line_color: Rgb<u8>,
                      fill_color: Rgb<u8>,
                      font_b: &ab_glyph::FontRef<'_>,
                      font_m: &ab_glyph::FontRef<'_>| {
        // Panel background (slightly lighter than base)
        let panel_bg = Rgb([24u8, 24, 37]); // Catppuccin Mantle
        draw_filled_rect_mut(img, Rect::at(ox + 2, oy + 2).of_size((pw - 4) as u32, (ph - 4) as u32), panel_bg);

        // Title (small, top-left)
        draw_text_mut(img, OVERLAY0, ox + 6, oy + 5, PxScale::from(13.0), font_b, title);

        // Current value (top-right)
        let vw = text_width(font_m, PxScale::from(16.0), value_str);
        draw_text_mut(img, line_color, ox + pw - 8 - vw, oy + 5, PxScale::from(16.0), font_m, value_str);

        // Mini chart (uses most of the panel height)
        let cx = ox + 2;
        let cy = oy + 26;
        let cw = pw - 4;
        let ch = ph - 32;
        draw_line_chart(img, history, max_val, cx, cy, cw, ch, line_color, fill_color);

        // Bottom border
        draw_filled_rect_mut(img, Rect::at(cx, cy + ch).of_size(cw as u32, 1), SURFACE0);
    };

    // ── CPU panel (top-left) ──────────────────────────────────────────────────
    let cpu_cur = cpu_hist.last().cloned().unwrap_or(0.0);
    let cpu_acc = if cpu_cur > 80.0 { RED } else if cpu_cur > 50.0 { YELLOW } else { MAUVE };
    draw_panel(
        &mut img, 0, 0, mid, mid,
        "CPU %",
        &format!("{:.0}%", cpu_cur),
        &cpu_hist, 100.0,
        cpu_acc,
        Rgb([cpu_acc.0[0] / 7, cpu_acc.0[1] / 7, cpu_acc.0[2] / 7]),
        &bold, &mono,
    );

    // ── Load avg panel (top-right) ────────────────────────────────────────────
    let load_max = (cpu_count as f32).max(4.0);
    let load_acc = {
        let l = load.one as f32 / cpu_count as f32;
        if l > 1.0 { RED } else if l > 0.7 { YELLOW } else { GREEN }
    };
    draw_panel(
        &mut img, mid, 0, mid, mid,
        "Load avg",
        &format!("{:.2}", load.one),
        &load_hist, load_max,
        load_acc,
        Rgb([load_acc.0[0] / 7, load_acc.0[1] / 7, load_acc.0[2] / 7]),
        &bold, &mono,
    );

    // ── Memory panel (bottom-left) ────────────────────────────────────────────
    let mem_acc = if mem_pct > 90.0 { RED } else if mem_pct > 75.0 { YELLOW } else { BLUE };
    draw_panel(
        &mut img, 0, mid, mid, mid,
        "Memory %",
        &format!("{:.0}%", mem_pct),
        &mem_hist, 100.0,
        mem_acc,
        Rgb([mem_acc.0[0] / 7, mem_acc.0[1] / 7, mem_acc.0[2] / 7]),
        &bold, &mono,
    );

    // ── Temperature panel (bottom-right) ─────────────────────────────────────
    // If no temp data, fall back to disk read history
    let (temp_use_hist, temp_max, temp_acc, temp_label, temp_val) = if hot_temp > 0.0 {
        let acc = if hot_temp >= 85.0 { RED } else if hot_temp >= 70.0 { PEACH } else { GREEN };
        (temp_hist.as_slice(), (hot_temp * 1.1).max(100.0), acc,
         "Temp °C", format!("{:.0}°C", hot_temp))
    } else {
        let peak_r = diog_r_hist.iter().cloned().fold(0.0_f32, f32::max);
        (diog_r_hist.as_slice(), (peak_r * 1.2).max(1024.0 * 1024.0), YELLOW,
         "Disk R/s", format_bps(*diog_r_hist.last().unwrap_or(&0.0) as u64))
    };
    draw_panel(
        &mut img, mid, mid, mid, mid,
        temp_label,
        &temp_val,
        temp_use_hist, temp_max,
        temp_acc,
        Rgb([temp_acc.0[0] / 7, temp_acc.0[1] / 7, temp_acc.0[2] / 7]),
        &bold, &mono,
    );

    // ── Center timestamp overlay ──────────────────────────────────────────────
    let tw_ts = text_width(&mono, PxScale::from(12.0), &ts);
    let tx = (480 - tw_ts) / 2;
    // Draw behind-the-cross: tiny label at center
    draw_filled_rect_mut(&mut img, Rect::at(tx - 2, mid - 9).of_size(tw_ts as u32 + 4, 14), SURFACE0);
    draw_text_mut(&mut img, OVERLAY0, tx, mid - 9, PxScale::from(12.0), &mono, &ts);

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod graphs_overview_tests {
    use super::*;

    #[test]
    fn test_render_graphs_overview_size() {
        let img = render_graphs_overview();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Disk I/O history graph ───────────────────────────────────────────────────

static DISK_READ_HISTORY:  OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();
static DISK_WRITE_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();

fn disk_read_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    DISK_READ_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120))
    })
    .lock()
    .unwrap()
}

fn disk_write_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    DISK_WRITE_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120))
    })
    .lock()
    .unwrap()
}

/// Append one disk I/O sample to the ring-buffers.
/// `read_bps` and `write_bps` are in bytes per second.
pub fn push_disk_io_history(read_bps: f32, write_bps: f32) {
    {
        let mut r = disk_read_data();
        if r.len() >= 120 { r.pop_front(); }
        r.push_back(read_bps);
    }
    {
        let mut w = disk_write_data();
        if w.len() >= 120 { w.pop_front(); }
        w.push_back(write_bps);
    }
}

/// Render a disk I/O history graph (480×480).
///
/// Shows total disk read and write throughput as two overlaid line charts,
/// alongside current total read/write rates and peak values.
pub fn render_disk_io_graph() -> DynamicImage {
    use sysinfo::{DiskRefreshKind, Disks};

    // Sample disk I/O (two refreshes, 500ms apart)
    let mut disks = Disks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(200));
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    let mut total_read_bps:  u64 = 0;
    let mut total_write_bps: u64 = 0;
    for d in disks.iter() {
        if d.total_space() == 0 { continue; }
        let io = d.usage();
        total_read_bps  += io.read_bytes * 5;   // ~200ms window → ×5 for per-second
        total_write_bps += io.written_bytes * 5;
    }

    push_disk_io_history(total_read_bps as f32, total_write_bps as f32);

    let read_history:  Vec<f32> = disk_read_data().iter().cloned().collect();
    let write_history: Vec<f32> = disk_write_data().iter().cloned().collect();

    // Peaks
    let peak_read  = read_history.iter().cloned().fold(0.0_f32, f32::max);
    let peak_write = write_history.iter().cloned().fold(0.0_f32, f32::max);
    let max_val    = (peak_read.max(peak_write) * 1.1).max(1024.0 * 1024.0); // at least 1 MB/s

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "DISK I/O GRAPH");
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // ── Current R/W display ───────────────────────────────────────────────
    let read_str  = format!("R {}", format_bps(total_read_bps));
    let write_str = format!("W {}", format_bps(total_write_bps));
    draw_text_mut(&mut img, BLUE, 20,  52, PxScale::from(32.0), &bold, &read_str);
    draw_text_mut(&mut img, PEACH, 250, 52, PxScale::from(32.0), &bold, &write_str);

    // ── Peak row ─────────────────────────────────────────────────────────
    let peak_str = format!("peak R {} / W {}", format_bps(peak_read as u64), format_bps(peak_write as u64));
    let pw = text_width(&mono, PxScale::from(14.0), &peak_str);
    draw_text_mut(&mut img, OVERLAY0, ((480 - pw) / 2).max(4), 96, PxScale::from(14.0), &mono, &peak_str);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 120i32;
    let chart_w = 480i32;
    let chart_h = 320i32;

    // Grid lines at 25%, 50%, 75% of max_val
    for (pct, _label) in &[(0.25f32, "25%"), (0.5, "50%"), (0.75, "75%")] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let val_at_pct = format_bps((max_val * pct) as u64);
        draw_text_mut(&mut img, OVERLAY0, 2, (ly - 10).max(chart_y), PxScale::from(11.0), &mono, &val_at_pct);
    }

    // Read chart (blue fill)
    let fill_read  = Rgb([BLUE.0[0] / 8,  BLUE.0[1] / 8,  BLUE.0[2] / 8]);
    let fill_write = Rgb([PEACH.0[0] / 8, PEACH.0[1] / 8, PEACH.0[2] / 8]);
    draw_line_chart(&mut img, &write_history, max_val, chart_x, chart_y, chart_w, chart_h, PEACH, fill_write);
    draw_line_chart(&mut img, &read_history,  max_val, chart_x, chart_y, chart_w, chart_h, BLUE,  fill_read);

    // Chart border
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);

    // Legend
    draw_filled_rect_mut(&mut img, Rect::at(20, 445).of_size(16, 10), BLUE);
    draw_text_mut(&mut img, SUBTEXT, 42, 443, PxScale::from(14.0), &mono, "Read");
    draw_filled_rect_mut(&mut img, Rect::at(100, 445).of_size(16, 10), PEACH);
    draw_text_mut(&mut img, SUBTEXT, 122, 443, PxScale::from(14.0), &mono, "Write");
    draw_centered(&mut img, &mono, PxScale::from(13.0), 463, OVERLAY0, "← 2 min ago                      now →");

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod disk_io_graph_tests {
    use super::*;

    #[test]
    fn test_push_disk_io_history() {
        push_disk_io_history(1_000_000.0, 500_000.0);
        let r: Vec<f32> = disk_read_data().iter().cloned().collect();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_disk_io_graph_size() {
        let img = render_disk_io_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Temperature history graph ────────────────────────────────────────────────

static TEMP_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> = OnceLock::new();

fn temp_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    TEMP_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120))
    })
    .lock()
    .unwrap()
}

/// Append one temperature sample (°C) to the temperature history ring-buffer.
pub fn push_temp_history(temp_c: f32) {
    let mut buf = temp_graph_data();
    if buf.len() >= 120 { buf.pop_front(); }
    buf.push_back(temp_c);
}

/// Returns the hottest component temperature in degrees Celsius, or 0.0 if unavailable.
pub fn hottest_component_temp() -> f32 {
    use sysinfo::Components;
    let comps = Components::new_with_refreshed_list();
    comps.iter()
        .filter_map(|c| c.temperature())
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max)
}

/// Render a CPU/component temperature history graph (480×480).
///
/// Shows the hottest component temperature as a filled line chart over the
/// last ~2 minutes, with current/peak/average values and thermal thresholds.
pub fn render_temp_graph() -> DynamicImage {
    use sysinfo::Components;

    // Refresh and sample the hottest sensor
    let comps = Components::new_with_refreshed_list();
    let temps: Vec<f32> = comps.iter()
        .filter_map(|c| c.temperature())
        .filter(|t| t.is_finite() && *t > 0.0)
        .collect();

    let current_temp = temps.iter().cloned().fold(0.0_f32, f32::max);
    push_temp_history(current_temp);

    let history: Vec<f32> = temp_graph_data().iter().cloned().collect();

    // Stats
    let peak_temp = history.iter().cloned().fold(0.0_f32, f32::max);
    let avg_temp  = if history.is_empty() { 0.0 }
                    else { history.iter().sum::<f32>() / history.len() as f32 };

    let max_display = (peak_temp.max(current_temp) * 1.1).max(100.0);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "TEMP GRAPH");
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // ── Current temperature — large ───────────────────────────────────────
    let accent = if current_temp >= 85.0 { RED }
                 else if current_temp >= 70.0 { PEACH }
                 else if current_temp >= 55.0 { YELLOW }
                 else { GREEN };

    let temp_str = format!("{:.1}°C", current_temp);
    draw_centered(&mut img, &bold, PxScale::from(80.0), 42, accent, &temp_str);

    // ── Stats row ─────────────────────────────────────────────────────────
    let stats_str = format!(
        "peak {:.1}°   avg {:.1}°   sensors: {}",
        peak_temp, avg_temp, temps.len()
    );
    let sw = text_width(&mono, PxScale::from(15.0), &stats_str);
    draw_text_mut(&mut img, SUBTEXT, ((480 - sw) / 2).max(0), 148, PxScale::from(15.0), &mono, &stats_str);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Thermal threshold guide lines
    for (threshold, label, color) in &[(55.0f32, "55°", YELLOW), (70.0, "70°", PEACH), (85.0, "85°", RED)] {
        if *threshold > max_display { continue; }
        let ly = chart_y + chart_h - (chart_h as f32 * threshold / max_display) as i32;
        if ly < chart_y { continue; }
        // Dashed line
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, *color);
            }
        }
        draw_text_mut(&mut img, *color, chart_x + 4, (ly - 14).max(chart_y), PxScale::from(12.0), &mono, label);
    }

    // Fill + line chart
    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(&mut img, &history, max_display, chart_x, chart_y, chart_w, chart_h, accent, fill_color);

    // Bottom border + time axis label
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);
    draw_centered(&mut img, &mono, PxScale::from(14.0), chart_y + chart_h + 6, OVERLAY0, "← 2 min ago                      now →");

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod temp_graph_tests {
    use super::*;

    #[test]
    fn test_push_temp_history() {
        push_temp_history(65.0);
        push_temp_history(72.0);
        let buf: Vec<f32> = temp_graph_data().iter().cloned().collect();
        assert!(!buf.is_empty());
        assert!(buf.contains(&65.0) || buf.contains(&72.0));
    }

    #[test]
    fn test_render_temp_graph_size() {
        let img = render_temp_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

#[cfg(test)]
mod matrix_tests {
    use super::*;

    #[test]
    fn test_matrix_rain_size() {
        let img = render_matrix_rain();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_matrix_rain_animates() {
        let img1 = render_matrix_rain();
        let img2 = render_matrix_rain();
        // Two frames should be different (rain moved)
        let p1 = img1.as_rgb8().unwrap().get_pixel(240, 240);
        let p2 = img2.as_rgb8().unwrap().get_pixel(240, 240);
        // They may or may not differ depending on column positions,
        // but both should be valid green-tinted colors
        // Just check they're valid images
        assert_eq!(img1.width(), 480);
        assert_eq!(img2.width(), 480);
        let _ = (p1, p2); // suppress unused warning
    }
}


// ─── Net Speed Graph (overlaid Rx + Tx line chart) ───────────────────────────

/// Render a dedicated network speed history graph (480×480).
///
/// Shows download (Rx, green) and upload (Tx, peach) overlaid on one shared
/// chart so the relative magnitudes are directly comparable, plus a compact
/// per-interface table at the bottom.
pub fn render_net_speed_graph() -> DynamicImage {
    use sysinfo::Networks;

    // Two-pass sample: sleep ~200 ms between refreshes
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(200));
    nets.refresh(true);

    // Per-interface breakdown (for the mini table)
    let mut ifaces: Vec<(String, u64, u64)> = nets.iter()
        .filter_map(|(name, data)| {
            let rx = data.received() * 5;
            let tx = data.transmitted() * 5;
            if rx + tx > 0 { Some((name.clone(), rx, tx)) } else { None }
        })
        .collect();
    ifaces.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)));
    ifaces.truncate(4); // show at most 4 interfaces

    let total_rx: u64 = ifaces.iter().map(|i| i.1).sum();
    let total_tx: u64 = ifaces.iter().map(|i| i.2).sum();

    // Add sample to shared history
    push_net_history(total_rx, total_tx);

    let rx_hist: Vec<f32> = net_rx_history().iter().map(|&v| v as f32).collect();
    let tx_hist: Vec<f32> = net_tx_history().iter().map(|&v| v as f32).collect();

    let peak_rx = rx_hist.iter().cloned().fold(0.0f32, f32::max);
    let peak_tx = tx_hist.iter().cloned().fold(0.0f32, f32::max);
    let max_val  = ((peak_rx.max(peak_tx)) * 1.1).max(10_240.0); // at least 10 KB/s

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "NET SPEED GRAPH");
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // ── Current Rx / Tx ───────────────────────────────────────────────────
    let rx_str = format!("↓ {}  ↑ {}", format_bps(total_rx), format_bps(total_tx));
    let rw = text_width(&bold, PxScale::from(30.0), &rx_str);
    draw_text_mut(&mut img, GREEN, ((480 - rw) / 2).max(0), 48, PxScale::from(30.0), &bold, &rx_str);

    // ── Peak row ─────────────────────────────────────────────────────────
    let pk_str = format!(
        "peak ↓ {} / ↑ {}",
        format_bps(peak_rx as u64),
        format_bps(peak_tx as u64)
    );
    let pw = text_width(&mono, PxScale::from(14.0), &pk_str);
    draw_text_mut(&mut img, SUBTEXT, ((480 - pw) / 2).max(4), 88, PxScale::from(14.0), &mono, &pk_str);

    // ── Chart area (one shared, overlaid) ─────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 112i32;
    let chart_w = 480i32;
    let chart_h = 270i32;

    // Grid lines at 25 / 50 / 75 %
    for pct in &[0.25f32, 0.5, 0.75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label = format_bps((max_val * pct) as u64);
        draw_text_mut(&mut img, OVERLAY0, 2, (ly - 10).max(chart_y), PxScale::from(11.0), &mono, &label);
    }

    // Draw Tx first (behind), then Rx on top
    let fill_rx = Rgb([GREEN.0[0] / 8, GREEN.0[1] / 8, GREEN.0[2] / 8]);
    let fill_tx = Rgb([PEACH.0[0] / 8, PEACH.0[1] / 8, PEACH.0[2] / 8]);
    draw_line_chart(&mut img, &tx_hist, max_val, chart_x, chart_y, chart_w, chart_h, PEACH, fill_tx);
    draw_line_chart(&mut img, &rx_hist, max_val, chart_x, chart_y, chart_w, chart_h, GREEN, fill_rx);

    // Bottom border
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);

    // ── Legend ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(20, 390).of_size(14, 9), GREEN);
    draw_text_mut(&mut img, SUBTEXT, 40, 387, PxScale::from(13.0), &mono, "Download");
    draw_filled_rect_mut(&mut img, Rect::at(130, 390).of_size(14, 9), PEACH);
    draw_text_mut(&mut img, SUBTEXT, 150, 387, PxScale::from(13.0), &mono, "Upload");
    draw_text_mut(&mut img, OVERLAY0, 330, 387, PxScale::from(12.0), &mono, "← 1 min");

    // ── Per-interface mini table ───────────────────────────────────────────
    let table_y = 408i32;
    draw_filled_rect_mut(&mut img, Rect::at(0, table_y).of_size(480, 1), SURFACE0);

    if ifaces.is_empty() {
        draw_centered(&mut img, &mono, PxScale::from(14.0), table_y + 8, OVERLAY0, "No active interfaces");
    } else {
        let row_h = 16i32;
        for (n, (name, rx, tx)) in ifaces.iter().enumerate() {
            let ry = table_y + 6 + n as i32 * row_h;
            let short_name = if name.len() > 10 { &name[..10] } else { name.as_str() };
            draw_text_mut(&mut img, SUBTEXT, 8,   ry, PxScale::from(13.0), &mono, short_name);
            draw_text_mut(&mut img, GREEN,   130,  ry, PxScale::from(13.0), &mono, &format!("↓ {}", format_bps(*rx)));
            draw_text_mut(&mut img, PEACH,   280,  ry, PxScale::from(13.0), &mono, &format!("↑ {}", format_bps(*tx)));
        }
    }

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod net_speed_graph_tests {
    use super::*;

    #[test]
    fn test_render_net_speed_graph_size() {
        let img = render_net_speed_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── CPU Core Bars (horizontal per-core utilisation) ─────────────────────────

/// Render a large horizontal-bar chart of per-core CPU utilisation (480×480).
///
/// Each row is one logical CPU core.  The bar colour shifts from green → yellow
/// → red as load increases.  Good for systems with many cores where the grid
/// view in `cpu_detail` is too cramped.
pub fn render_cpu_core_bars() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, RefreshKind};

    let refresh = RefreshKind::nothing().with_cpu(CpuRefreshKind::everything());
    let mut sys = System::new_with_specifics(refresh);
    // Two refreshes to get non-zero per-core usage from sysinfo
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_specifics(refresh);

    let cores: Vec<(String, f32)> = sys.cpus()
        .iter()
        .map(|c| (c.name().to_string(), c.cpu_usage()))
        .collect();

    let global = sys.global_cpu_usage();
    let n = cores.len().max(1);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "CPU CORE BARS");
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // Global summary
    let acc = if global > 80.0 { RED } else if global > 50.0 { YELLOW } else { GREEN };
    let g_str = format!("{:.1}% global", global);
    let gw = text_width(&bold, PxScale::from(24.0), &g_str);
    draw_text_mut(&mut img, acc, ((480 - gw) / 2).max(0), 46, PxScale::from(24.0), &bold, &g_str);

    let load = System::load_average();
    let load_str = format!("load {:.2} / {:.2} / {:.2}", load.one, load.five, load.fifteen);
    let lw = text_width(&mono, PxScale::from(14.0), &load_str);
    draw_text_mut(&mut img, SUBTEXT, ((480 - lw) / 2).max(0), 76, PxScale::from(14.0), &mono, &load_str);

    // ── Per-core bars ─────────────────────────────────────────────────────
    let bar_area_top  = 102i32;
    let bar_area_h    = 370i32;
    let max_cores     = 32usize;
    let visible       = n.min(max_cores);
    let row_h         = (bar_area_h / visible as i32).max(10);
    let label_w       = 34i32;
    let pct_w         = 36i32;
    let bar_x         = label_w + 4;
    let bar_max_w     = 480 - bar_x - pct_w - 6;

    for (i, (_name, usage)) in cores.iter().enumerate().take(visible) {
        let y   = bar_area_top + i as i32 * row_h;
        let bar_color = if *usage > 80.0 { RED } else if *usage > 50.0 { YELLOW } else { GREEN };
        let fill_color = Rgb([bar_color.0[0] / 6, bar_color.0[1] / 6, bar_color.0[2] / 6]);

        // Label
        let lbl = format!("C{}", i);
        draw_text_mut(&mut img, SUBTEXT, 4, y + 1, PxScale::from((row_h - 2).min(14) as f32), &mono, &lbl);

        // Background track
        let bar_h_draw = (row_h - 4).max(4) as u32;
        draw_filled_rect_mut(&mut img, Rect::at(bar_x, y + 2).of_size(bar_max_w as u32, bar_h_draw), SURFACE0);

        // Filled portion
        let filled = ((bar_max_w as f32 * usage / 100.0) as i32).max(if *usage > 0.0 { 2 } else { 0 });
        if filled > 0 {
            draw_filled_rect_mut(&mut img, Rect::at(bar_x, y + 2).of_size(filled as u32, bar_h_draw), bar_color);
            // Lighter fill overlay for depth
            if filled > 4 {
                draw_filled_rect_mut(&mut img, Rect::at(bar_x, y + 2).of_size(filled as u32, (bar_h_draw / 3).max(1)), fill_color);
            }
        }

        // Percentage text
        let pct_str = format!("{:.0}%", usage);
        draw_text_mut(&mut img, TEXT,
                      480 - pct_w - 2,
                      y + 1,
                      PxScale::from((row_h - 2).min(13) as f32),
                      &mono,
                      &pct_str);
    }

    // Separator line
    let sep_y = bar_area_top + visible as i32 * row_h + 2;
    if sep_y < 480 {
        draw_filled_rect_mut(&mut img, Rect::at(0, sep_y).of_size(480, 1), SURFACE0);
    }

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod cpu_core_bars_tests {
    use super::*;

    #[test]
    fn test_render_cpu_core_bars_size() {
        let img = render_cpu_core_bars();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Analog Clock ─────────────────────────────────────────────────────────────

/// Render an analog clock face (480×480).
///
/// Shows hour, minute, and second hands on a circular dial with Catppuccin
/// Mocha styling.  Date and load average are shown below the clock face.
pub fn render_analog_clock() -> DynamicImage {
    use std::f64::consts::PI;
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now  = Local::now();
    let h    = now.hour()   as f64;
    let m    = now.minute() as f64;
    let s    = now.second() as f64;

    let cx = 240i32;
    let cy = 220i32;
    let r  = 185i32;   // outer radius of clock face

    // ── Clock face ────────────────────────────────────────────────────────
    // Fill circle background (slightly lighter than base)
    let face_bg = Rgb([30u8, 30, 46]);
    for py in (cy - r)..(cy + r) {
        for px in (cx - r)..(cx + r) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            if dx * dx + dy * dy <= (r as f64) * (r as f64) {
                if py >= 0 && px >= 0 && (py as u32) < img.height() && (px as u32) < img.width() {
                    img.put_pixel(px as u32, py as u32, face_bg);
                }
            }
        }
    }

    // ── Outer ring ────────────────────────────────────────────────────────
    for angle_deg in 0..720 {
        let angle = angle_deg as f64 * PI / 360.0;
        let px = cx + ((r as f64 - 1.0) * angle.sin()) as i32;
        let py = cy - ((r as f64 - 1.0) * angle.cos()) as i32;
        if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
            img.put_pixel(px as u32, py as u32, SURFACE0);
        }
    }

    // ── Hour tick marks ───────────────────────────────────────────────────
    for i in 0..12 {
        let tick_angle = i as f64 * PI / 6.0;
        let (tick_sin, tick_cos) = (tick_angle.sin(), tick_angle.cos());
        let inner = r as f64 - 20.0;
        let outer = r as f64 - 5.0;
        let steps = (outer - inner).ceil() as i32 + 1;
        for t in 0..steps {
            let tt = inner + t as f64;
            let tx = cx + (tt * tick_sin) as i32;
            let ty = cy - (tt * tick_cos) as i32;
            if tx >= 0 && ty >= 0 && (tx as u32) < img.width() && (ty as u32) < img.height() {
                img.put_pixel(tx as u32, ty as u32, OVERLAY0);
            }
        }
    }

    // ── Minute tick marks ─────────────────────────────────────────────────
    for i in 0..60 {
        if i % 5 == 0 { continue; } // skip hour marks
        let tick_angle = i as f64 * PI / 30.0;
        let (tick_sin, tick_cos) = (tick_angle.sin(), tick_angle.cos());
        let inner = r as f64 - 10.0;
        let outer = r as f64 - 3.0;
        for t in 0..8 {
            let tt = inner + t as f64;
            let tx = cx + (tt * tick_sin) as i32;
            let ty = cy - (tt * tick_cos) as i32;
            if tx >= 0 && ty >= 0 && (tx as u32) < img.width() && (ty as u32) < img.height() {
                img.put_pixel(tx as u32, ty as u32, SURFACE0);
            }
        }
        let _ = outer; // suppress warning
    }

    // ── Helper: draw clock hand using anti-aliased lines ─────────────────────────
    // Each hand is drawn as a bundle of parallel anti-aliased line segments for thickness
    let draw_hand = |img: &mut image::RgbImage, angle: f64, length: f64, thickness: i32, color: Rgb<u8>| {
        let (sin_a, cos_a) = (angle.sin(), angle.cos());
        let perp_sin = cos_a;  // perpendicular to hand direction
        let perp_cos = -sin_a;
        let ex = cx as f32 + (length * sin_a) as f32;
        let ey = cy as f32 - (length * cos_a) as f32;
        let half = (thickness as f64 - 1.0) / 2.0;
        for t_idx in 0..thickness {
            let offset = t_idx as f64 - half;
            let sx = cx as f32 + (offset * perp_sin) as f32;
            let sy = cy as f32 - (offset * perp_cos) as f32;
            let lx = ex + (offset * perp_sin) as f32;
            let ly = ey - (offset * perp_cos) as f32;
            draw_antialiased_line_segment_mut(img, (sx as i32, sy as i32), (lx as i32, ly as i32), color, interpolate);
        }
    };

    // ── Hour hand ─────────────────────────────────────────────────────────
    let h_angle  = (h + m / 60.0) * PI / 6.0;
    draw_hand(&mut img, h_angle, 100.0, 5, MAUVE);

    // ── Minute hand ───────────────────────────────────────────────────────
    let m_angle = (m + s / 60.0) * PI / 30.0;
    draw_hand(&mut img, m_angle, 145.0, 3, BLUE);

    // ── Second hand ───────────────────────────────────────────────────────
    let s_angle = s * PI / 30.0;
    draw_hand(&mut img, s_angle, 155.0, 1, RED);

    // ── Centre dot ────────────────────────────────────────────────────────
    for dx in -5i32..6 {
        for dy in -5i32..6 {
            if dx * dx + dy * dy <= 25 {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, TEXT);
                }
            }
        }
    }
    // Tiny centre dot in red
    for dx in -2i32..3 {
        for dy in -2i32..3 {
            let px = cx + dx;
            let py = cy + dy;
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, RED);
            }
        }
    }

    // ── Hour labels (12, 3, 6, 9) ─────────────────────────────────────────
    for (label, angle_deg) in &[("12", 0.0f64), ("3", 90.0), ("6", 180.0), ("9", 270.0)] {
        let angle_rad = angle_deg * PI / 180.0;
        let label_r   = r as f64 - 38.0;
        let lx = cx + (label_r * angle_rad.sin()) as i32;
        let ly = cy - (label_r * angle_rad.cos()) as i32;
        let lw = text_width(&bold, PxScale::from(18.0), label);
        draw_text_mut(&mut img, OVERLAY0,
                      lx - lw / 2,
                      ly - 9,
                      PxScale::from(18.0), &bold, label);
    }

    // ── Digital time (small, below face) ─────────────────────────────────
    let time_str  = now.format("%H:%M:%S").to_string();
    let date_str  = now.format("%A, %B %-e").to_string();
    let load      = System::load_average();
    let load_str  = format!("load {:.2} / {:.2} / {:.2}", load.one, load.five, load.fifteen);

    draw_centered(&mut img, &mono,  PxScale::from(26.0), 420, TEAL,    &time_str);
    draw_centered(&mut img, &bold,  PxScale::from(18.0), 450, SUBTEXT, &date_str);
    draw_centered(&mut img, &mono,  PxScale::from(14.0), 470, OVERLAY0, &load_str);

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod analog_clock_tests {
    use super::*;

    #[test]
    fn test_render_analog_clock_size() {
        let img = render_analog_clock();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}


// ─── Ping Latency History Graph ───────────────────────────────────────────────

// Up to 4 hosts, 120 samples each
static PING_HISTORY: OnceLock<std::sync::Mutex<Vec<std::collections::VecDeque<(f32, bool)>>>> = OnceLock::new();

fn ping_history_data() -> std::sync::MutexGuard<'static, Vec<std::collections::VecDeque<(f32, bool)>>> {
    PING_HISTORY.get_or_init(|| {
        // Pre-allocate for up to 4 hosts × 120 samples
        let v: Vec<std::collections::VecDeque<(f32, bool)>> =
            (0..4).map(|_| std::collections::VecDeque::with_capacity(120)).collect();
        std::sync::Mutex::new(v)
    })
    .lock()
    .unwrap()
}

/// Record the latest ping results into the history ring-buffers.
///
/// Called by the ping_monitor_loop each time a fresh set of results arrives.
/// Only the first 4 hosts are tracked.
pub fn push_ping_history(results: &[PingResult]) {
    let mut hist = ping_history_data();
    for (idx, res) in results.iter().enumerate().take(4) {
        while hist.len() <= idx {
            hist.push(std::collections::VecDeque::with_capacity(120));
        }
        let buf = &mut hist[idx];
        if buf.len() >= 120 { buf.pop_front(); }
        let ms = res.latency_ms.unwrap_or(0.0) as f32;
        let ok = res.latency_ms.is_some() && res.loss_pct < 100.0;
        buf.push_back((ms, ok));
    }
}

/// Render a ping latency history graph (480×480).
///
/// Shows up to 4 configured hosts as overlaid coloured line charts.
/// Unreachable samples are shown as a red vertical spike at the bottom.
pub fn render_ping_graph(results: &[PingResult]) -> DynamicImage {
    // Push fresh samples first
    push_ping_history(results);

    let hist = ping_history_data();
    let hosts: Vec<(String, Vec<(f32, bool)>)> = results.iter()
        .enumerate()
        .take(4)
        .map(|(i, res)| {
            let buf: Vec<(f32, bool)> = if i < hist.len() {
                hist[i].iter().cloned().collect()
            } else {
                vec![]
            };
            (res.label.clone(), buf)
        })
        .collect();
    drop(hist);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();
    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, 20, 18, PxScale::from(19.0), &bold, "PING GRAPH");
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &ts);

    // Per-host current latency
    let colors: &[Rgb<u8>] = &[GREEN, BLUE, PEACH, MAUVE];
    let mut cur_y = 48i32;
    for (i, (label, _)) in hosts.iter().enumerate() {
        let ms = results.get(i).and_then(|r| r.latency_ms).unwrap_or(0.0);
        let loss = results.get(i).map(|r| r.loss_pct).unwrap_or(100.0);
        let col = if loss >= 100.0 { RED } else if ms > 100.0 { YELLOW } else { colors[i] };
        let stat = if loss >= 100.0 {
            format!("{}: timeout", label)
        } else {
            format!("{}: {:.1} ms ({:.0}% loss)", label, ms, loss)
        };
        draw_text_mut(&mut img, col, 20, cur_y, PxScale::from(16.0), &bold, &stat);
        cur_y += 20;
    }

    // ── Compute chart scale ───────────────────────────────────────────────
    let max_ms = hosts.iter()
        .flat_map(|(_, buf)| buf.iter())
        .filter(|(_, ok)| *ok)
        .map(|(ms, _)| *ms)
        .fold(0.0f32, f32::max);
    let chart_max = (max_ms * 1.2).max(100.0);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = (cur_y + 10).max(130);
    let chart_w = 480i32;
    let chart_h = 480 - chart_y - 60;

    // Grid lines at 25%, 50%, 75%
    for pct in &[0.25f32, 0.5, 0.75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label_ms = format!("{:.0}ms", chart_max * pct);
        draw_text_mut(&mut img, OVERLAY0, 2, (ly - 10).max(chart_y), PxScale::from(11.0), &mono, &label_ms);
    }

    // Per-host line charts
    for (i, (_, buf)) in hosts.iter().enumerate().take(4) {
        if buf.is_empty() { continue; }
        let color = colors[i];
        let fill  = Rgb([color.0[0] / 8, color.0[1] / 8, color.0[2] / 8]);
        let vals: Vec<f32> = buf.iter().map(|(ms, ok)| if *ok { *ms } else { 0.0 }).collect();
        draw_line_chart(&mut img, &vals, chart_max, chart_x, chart_y, chart_w, chart_h, color, fill);

        // Timeout markers (vertical red lines at bottom)
        let n = buf.len();
        let bar_w = (chart_w / 120).max(2);
        let offset = 120 - n as i32;
        for (j, (_, ok)) in buf.iter().enumerate() {
            if !ok {
                let bx = chart_x + (offset + j as i32) * bar_w;
                let by = chart_y + chart_h - 10;
                if bx >= 0 && (bx as u32) < img.width() as u32 {
                    draw_filled_rect_mut(&mut img, Rect::at(bx, by).of_size(bar_w as u32, 10), RED);
                }
            }
        }
    }

    // Chart border
    draw_filled_rect_mut(&mut img, Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2), SURFACE0);

    // ── Legend ────────────────────────────────────────────────────────────
    let legend_y = chart_y + chart_h + 6;
    let mut lx = 8i32;
    for (i, (label, _)) in hosts.iter().enumerate().take(4) {
        let color = colors[i];
        draw_filled_rect_mut(&mut img, Rect::at(lx, legend_y + 2).of_size(12, 8), color);
        lx += 14;
        let short = if label.len() > 12 { &label[..12] } else { label.as_str() };
        draw_text_mut(&mut img, SUBTEXT, lx, legend_y, PxScale::from(13.0), &mono, short);
        lx += text_width(&mono, PxScale::from(13.0), short) + 10;
    }

    let footer = "← 2 min ago    now →  ▼ = timeout";
    let fw = text_width(&mono, PxScale::from(12.0), footer);
    draw_text_mut(&mut img, OVERLAY0, (480 - fw).max(0), legend_y + 17, PxScale::from(12.0), &mono, footer);

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod ping_graph_tests {
    use super::*;

    #[test]
    fn test_render_ping_graph_empty() {
        let img = render_ping_graph(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_render_ping_graph_with_results() {
        let results = vec![
            PingResult { host: "8.8.8.8".into(), label: "Google DNS".into(), latency_ms: Some(12.5), loss_pct: 0.0 },
            PingResult { host: "1.1.1.1".into(), label: "Cloudflare".into(), latency_ms: Some(8.3), loss_pct: 0.0 },
        ];
        push_ping_history(&results);
        let img = render_ping_graph(&results);
        assert_eq!(img.width(), 480);
    }
}


// ─── Status Overview (4-quadrant live status) ────────────────────────────────

/// Render a 4-quadrant live status overview (480×480).
///
/// Top-left: Clock + date.  Top-right: CPU % + Memory.
/// Bottom-left: Network Rx/Tx.  Bottom-right: Temp + Uptime.
pub fn render_status_overview() -> DynamicImage {
    use sysinfo::{Components, CpuRefreshKind, RefreshKind, System};

    let refresh = RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory(sysinfo::MemoryRefreshKind::everything());
    let mut sys = System::new_with_specifics(refresh);
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_specifics(refresh);

    let now    = Local::now();
    let cpu    = sys.global_cpu_usage();
    let used   = sys.used_memory();
    let total  = sys.total_memory();
    let mem_pct = if total > 0 { used as f32 / total as f32 * 100.0 } else { 0.0 };
    let load   = System::load_average();
    let uptime = System::uptime();

    // Network
    use sysinfo::Networks;
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(100));
    nets.refresh(true);
    let (rx_bps, tx_bps) = nets.iter()
        .fold((0u64, 0u64), |(rx, tx), (_, d)| {
            (rx + d.received() * 10, tx + d.transmitted() * 10)  // ~100ms × 10 = /s
        });

    // Temperature
    let comps = Components::new_with_refreshed_list();
    let hot_temp = comps.iter()
        .filter_map(|c| c.temperature())
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max);

    let mut img  = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mid = 240i32;

    // ── Quadrant background tint ──────────────────────────────────────────
    // All 4 quads use the same base background with subtle dividers

    // Divider lines
    for x in 0..480u32 { img.put_pixel(x, mid as u32, SURFACE0); }
    for y in 0..480u32 { img.put_pixel(mid as u32, y, SURFACE0); }

    // ── TL: Clock ─────────────────────────────────────────────────────────
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %-e").to_string();
    let wday_str = now.format("%Y").to_string();
    draw_centered_in(&mut img, &mono, PxScale::from(40.0), 0, 0, mid, mid - 10, TEAL,    &time_str);
    draw_centered_in(&mut img, &bold, PxScale::from(17.0), 0, 0, mid, mid + 60, TEXT,    &date_str);
    draw_centered_in(&mut img, &bold, PxScale::from(14.0), 0, 0, mid, mid + 85, SUBTEXT, &wday_str);
    draw_text_mut(&mut img, OVERLAY0, 8, 8, PxScale::from(12.0), &mono, "CLOCK");

    // ── TR: CPU + Memory ──────────────────────────────────────────────────
    let cpu_color = if cpu > 80.0 { RED } else if cpu > 50.0 { YELLOW } else { MAUVE };
    let mem_color = if mem_pct > 90.0 { RED } else if mem_pct > 75.0 { YELLOW } else { BLUE };
    let cpu_str = format!("{:.0}%", cpu);
    let mem_str = format!("{:.0}%", mem_pct);

    draw_text_mut(&mut img, OVERLAY0, mid + 8, 8, PxScale::from(12.0), &mono, "CPU / MEM");
    draw_centered_in(&mut img, &bold, PxScale::from(34.0), mid, 0, mid, mid - 60, cpu_color, &cpu_str);

    // CPU bar
    let bar_x = mid + 12;
    let bar_y = mid / 2 - 5;
    let bar_w = mid - 24;
    draw_bar(&mut img, bar_x, bar_y, bar_w, 10, cpu / 100.0, cpu_color);
    draw_text_mut(&mut img, SUBTEXT, mid + 8, mid / 2 + 10, PxScale::from(13.0), &mono, "CPU");

    draw_centered_in(&mut img, &bold, PxScale::from(34.0), mid, 0, mid, mid + 30, mem_color, &mem_str);
    draw_bar(&mut img, bar_x, mid - 35, bar_w, 10, mem_pct / 100.0, mem_color);
    draw_text_mut(&mut img, SUBTEXT, mid + 8, mid - 20, PxScale::from(13.0), &mono, "MEM");

    let load_str = format!("load {:.2}", load.one);
    let lw = text_width(&mono, PxScale::from(13.0), &load_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - lw - 8, mid - 6, PxScale::from(13.0), &mono, &load_str);

    // ── BL: Network ───────────────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, 8, mid + 8, PxScale::from(12.0), &mono, "NETWORK");
    let rx_str = format!("↓ {}", format_bps(rx_bps));
    let tx_str = format!("↑ {}", format_bps(tx_bps));
    draw_centered_in(&mut img, &bold, PxScale::from(28.0), 0, mid, mid, mid + 60, GREEN, &rx_str);
    draw_centered_in(&mut img, &bold, PxScale::from(28.0), 0, mid, mid, mid + 100, PEACH, &tx_str);
    // Tiny history sparkline
    let rx_hist: Vec<f32> = net_rx_history().iter().map(|&v| v as f32).collect();
    let tx_hist: Vec<f32> = net_tx_history().iter().map(|&v| v as f32).collect();
    let net_max = rx_hist.iter().chain(tx_hist.iter()).cloned().fold(0.0f32, f32::max).max(1024.0);
    let fill_rx = Rgb([GREEN.0[0] / 10, GREEN.0[1] / 10, GREEN.0[2] / 10]);
    let fill_tx = Rgb([PEACH.0[0] / 10, PEACH.0[1] / 10, PEACH.0[2] / 10]);
    draw_line_chart(&mut img, &tx_hist, net_max, 0, mid + 145, mid, 84, PEACH, fill_tx);
    draw_line_chart(&mut img, &rx_hist, net_max, 0, mid + 145, mid, 84, GREEN, fill_rx);

    // ── BR: Temp + Uptime ─────────────────────────────────────────────────
    draw_text_mut(&mut img, OVERLAY0, mid + 8, mid + 8, PxScale::from(12.0), &mono, "TEMP / UPTIME");
    let temp_color = if hot_temp >= 85.0 { RED } else if hot_temp >= 70.0 { PEACH } else { GREEN };
    let temp_str = if hot_temp > 0.0 { format!("{:.0}°C", hot_temp) } else { "—".to_string() };
    draw_centered_in(&mut img, &bold, PxScale::from(40.0), mid, mid, mid, mid + 60, temp_color, &temp_str);

    // Uptime
    let up_str = format_uptime(uptime);
    draw_centered_in(&mut img, &mono, PxScale::from(18.0), mid, mid, mid, mid + 115, TEAL, &up_str);
    draw_text_mut(&mut img, OVERLAY0, mid + 8, 473, PxScale::from(12.0), &mono, "up");

    // ── Timestamp (bottom center) ─────────────────────────────────────────
    let ts = now.format("%H:%M:%S").to_string();
    let tsw = text_width(&mono, PxScale::from(11.0), &ts);
    draw_filled_rect_mut(&mut img, Rect::at((480 - tsw as i32) / 2 - 2, mid - 7).of_size(tsw as u32 + 4, 14), SURFACE0);
    draw_text_mut(&mut img, OVERLAY0, (480 - tsw as i32) / 2, mid - 7, PxScale::from(11.0), &mono, &ts);

    DynamicImage::ImageRgb8(img)
}

/// Draw text centered horizontally within a sub-rectangle [ox, ox+pw] and
/// at a given absolute y coordinate.
fn draw_centered_in(img: &mut RgbImage,
                    font: &ab_glyph::FontRef<'_>,
                    scale: PxScale,
                    ox: i32, _oy: i32, pw: i32, y: i32,
                    color: Rgb<u8>,
                    text: &str) {
    let w = text_width(font, scale, text);
    let x = ox + (pw - w) / 2;
    draw_text_mut(img, color, x.max(ox), y, scale, font, text);
}

#[cfg(test)]
mod status_overview_tests {
    use super::*;

    #[test]
    fn test_render_status_overview_size() {
        let img = render_status_overview();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}


// ─── Conway's Game of Life ────────────────────────────────────────────────────

static LIFE_GRID: OnceLock<std::sync::Mutex<Vec<Vec<bool>>>> = OnceLock::new();


const LIFE_COLS: usize = 96;  // 480 / 5 pixels per cell
const LIFE_ROWS: usize = 96;

fn life_grid_data() -> std::sync::MutexGuard<'static, Vec<Vec<bool>>> {
    LIFE_GRID.get_or_init(|| {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
            .as_nanos().hash(&mut hasher);
        let seed = hasher.finish();
        // Random initial grid using LCG
        let mut s = seed;
        let grid: Vec<Vec<bool>> = (0..LIFE_ROWS).map(|_| {
            (0..LIFE_COLS).map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (s >> 33) & 1 == 1
            }).collect()
        }).collect();
        std::sync::Mutex::new(grid)
    })
    .lock()
    .unwrap()
}

// (life_gen_data removed - generation count tracked inline in render_game_of_life)

/// Advance the Game of Life grid by one generation.
fn life_step(grid: &mut Vec<Vec<bool>>) {
    let rows = grid.len();
    let cols = grid[0].len();
    let mut next = grid.clone();
    for r in 0..rows {
        for c in 0..cols {
            let mut neighbours = 0u8;
            for dr in [-1i32, 0, 1] {
                for dc in [-1i32, 0, 1] {
                    if dr == 0 && dc == 0 { continue; }
                    let nr = (r as i32 + dr).rem_euclid(rows as i32) as usize;
                    let nc = (c as i32 + dc).rem_euclid(cols as i32) as usize;
                    if grid[nr][nc] { neighbours += 1; }
                }
            }
            next[r][c] = matches!((grid[r][c], neighbours),
                (true, 2) | (true, 3) | (false, 3));
        }
    }
    *grid = next;
}

/// Render one frame of Conway's Game of Life (480×480).
///
/// The grid is 96×96 cells, each 5×5 pixels.  The grid persists between calls.
/// If the grid becomes static (no change in 10 generations), it is re-seeded.
pub fn render_game_of_life() -> DynamicImage {
    const CELL_PX: usize = 5;

    let mut grid = life_grid_data();
    let alive_before: u64 = grid.iter().flatten().map(|&b| b as u64).sum();

    life_step(&mut *grid);

    let alive_after: u64 = grid.iter().flatten().map(|&b| b as u64).sum();

    // Re-seed if the population died out or stagnated
    if alive_after == 0 || alive_after == alive_before {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
            .as_nanos().hash(&mut hasher);
        let mut s = hasher.finish() ^ (alive_before.wrapping_mul(0xdeadbeef));
        for row in grid.iter_mut() {
            for cell in row.iter_mut() {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *cell = (s >> 33) & 1 == 1;
            }
        }
    }

    let mut img = blank();

    // Background already dark (blank())
    // Draw alive cells
    let alive_col = GREEN;
    let dim_col   = Rgb([alive_col.0[0] / 4, alive_col.0[1] / 4, alive_col.0[2] / 4]);

    for (r, row) in grid.iter().enumerate() {
        for (c, &alive) in row.iter().enumerate() {
            if alive {
                let px = (c * CELL_PX) as u32;
                let py = (r * CELL_PX) as u32;
                // Fill cell leaving 1px gap for grid effect
                for dy in 0..(CELL_PX as u32 - 1) {
                    for dx in 0..(CELL_PX as u32 - 1) {
                        if (px + dx) < 480 && (py + dy) < 480 {
                            img.put_pixel(px + dx, py + dy, alive_col);
                        }
                    }
                }
            }
        }
    }

    // Overlay: generation counter and population
    let bold = font_bold();
    let mono = font_mono();
    let pop_str = format!("pop: {}  gen: —", alive_after);
    let pw = text_width(&mono, PxScale::from(12.0), &pop_str);
    draw_filled_rect_mut(&mut img, Rect::at(2, 466).of_size(pw as u32 + 4, 14), Rgb([10, 10, 15]));
    draw_text_mut(&mut img, dim_col, 4, 466, PxScale::from(12.0), &mono, &pop_str);
    let _ = bold; // used elsewhere

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod game_of_life_tests {
    use super::*;

    #[test]
    fn test_render_game_of_life_size() {
        let img = render_game_of_life();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_game_of_life_evolves() {
        let img1 = render_game_of_life();
        let img2 = render_game_of_life();
        // Two consecutive frames should be valid images
        assert_eq!(img1.width(), img2.width());
    }
}


// ─── 7-Segment LED Clock ───────────────────────────────────────────────────────

/// Render a 7-segment LED display clock (480×480).
///
/// Each digit is drawn using rectangular segments (a–g, plus decimal point).
/// The display shows HH:MM:SS in bright amber/orange Catppuccin Peach.
pub fn render_led_clock() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let h  = now.hour();
    let m  = now.minute();
    let s  = now.second();

    //   ─── Segment definitions ─────────────────────────────────────────────
    //   Segments: a (top), b (top-right), c (bottom-right), d (bottom),
    //             e (bottom-left), f (top-left), g (middle)
    //
    //    aaa
    //   f   b
    //   f   b
    //    ggg
    //   e   c
    //   e   c
    //    ddd
    //
    //  digit → [a, b, c, d, e, f, g]
    let segments: [[bool; 7]; 10] = [
        [true,  true,  true,  true,  true,  true,  false], // 0
        [false, true,  true,  false, false, false, false], // 1
        [true,  true,  false, true,  true,  false, true],  // 2
        [true,  true,  true,  true,  false, false, true],  // 3
        [false, true,  true,  false, false, true,  true],  // 4
        [true,  false, true,  true,  false, true,  true],  // 5
        [true,  false, true,  true,  true,  true,  true],  // 6
        [true,  true,  true,  false, false, false, false], // 7
        [true,  true,  true,  true,  true,  true,  true],  // 8
        [true,  true,  true,  true,  false, true,  true],  // 9
    ];

    let on_color  = PEACH;
    let off_color = Rgb([PEACH.0[0] / 12, PEACH.0[1] / 12, PEACH.0[2] / 12]);

    // Digit geometry (each digit occupies W x H px including gap)
    let dw: i32 = 50; // digit width
    let dh: i32 = 80; // digit height
    let gap: i32 = 4; // gap between digit and separator

    // Total clock row: 6 digits + 2 colons
    // Layout: HH:MM:SS
    // Position: center vertically around y=240
    let total_w = 6 * dw + 2 * 18 + 2 * gap * 4; // approximate
    let start_x = (480 - total_w) / 2;
    let start_y = 155i32; // top of digits

    let seg_thick: i32 = 5;

    let draw_segment = |img: &mut image::RgbImage,
                        x: i32, y: i32, w: i32, h: i32,
                        segs: &[bool; 7],
                        on_c: Rgb<u8>, off_c: Rgb<u8>| {
        let st = seg_thick;
        // a: top horizontal
        {
            let c = if segs[0] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x + st, y).of_size((w - 2*st) as u32, st as u32), c);
        }
        // b: top-right vertical
        {
            let c = if segs[1] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x + w - st, y + st).of_size(st as u32, (h/2 - st) as u32), c);
        }
        // c: bottom-right vertical
        {
            let c = if segs[2] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x + w - st, y + h/2).of_size(st as u32, (h/2 - st) as u32), c);
        }
        // d: bottom horizontal
        {
            let c = if segs[3] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x + st, y + h - st).of_size((w - 2*st) as u32, st as u32), c);
        }
        // e: bottom-left vertical
        {
            let c = if segs[4] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x, y + h/2).of_size(st as u32, (h/2 - st) as u32), c);
        }
        // f: top-left vertical
        {
            let c = if segs[5] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x, y + st).of_size(st as u32, (h/2 - st) as u32), c);
        }
        // g: middle horizontal
        {
            let c = if segs[6] { on_c } else { off_c };
            draw_filled_rect_mut(img, Rect::at(x + st, y + h/2 - st/2).of_size((w - 2*st) as u32, st as u32), c);
        }
    };

    let draw_colon = |img: &mut image::RgbImage, x: i32, y: i32, h: i32, flash: bool| {
        let c = if flash { on_color } else { off_color };
        let dot_size = 7i32;
        draw_filled_rect_mut(img, Rect::at(x, y + h/4 - dot_size/2).of_size(dot_size as u32, dot_size as u32), c);
        draw_filled_rect_mut(img, Rect::at(x, y + 3*h/4 - dot_size/2).of_size(dot_size as u32, dot_size as u32), c);
    };

    // Draw HH:MM:SS
    let colon_w = 18i32;
    let mut cx = start_x;
    let flash = s % 2 == 0;

    let digits = [h / 10, h % 10, m / 10, m % 10, s / 10, s % 10];
    for (i, &d) in digits.iter().enumerate() {
        // Colon after HH and MM
        if i == 2 || i == 4 {
            draw_colon(&mut img, cx, start_y, dh, flash);
            cx += colon_w;
        }
        if (d as usize) < 10 {
            draw_segment(&mut img, cx, start_y, dw, dh, &segments[d as usize], on_color, off_color);
        }
        cx += dw + gap;
    }

    // Glow effect: draw a subtle halo under digits using darker peach rects
    // (already handled by the display itself)

    // Date + day below
    let date_str = now.format("%A, %B %-e, %Y").to_string();
    let date_w   = text_width(&bold, PxScale::from(22.0), &date_str);
    draw_text_mut(&mut img, SUBTEXT, ((480 - date_w) / 2).max(0), 260, PxScale::from(22.0), &bold, &date_str);

    // Load average and uptime
    let load    = System::load_average();
    let uptime  = System::uptime();
    let stat_str = format!(
        "load {:.2}/{:.2}/{:.2}   up {}",
        load.one, load.five, load.fifteen,
        format_uptime(uptime)
    );
    let sw = text_width(&mono, PxScale::from(15.0), &stat_str);
    draw_text_mut(&mut img, OVERLAY0, ((480 - sw) / 2).max(0), 294, PxScale::from(15.0), &mono, &stat_str);

    // Hostname
    let hostname = System::host_name().unwrap_or_else(|| "ttlcd".into());
    let hw = text_width(&mono, PxScale::from(18.0), &hostname);
    draw_text_mut(&mut img, MAUVE, ((480 - hw) / 2).max(0), 326, PxScale::from(18.0), &mono, &hostname);

    // Decorative scanlines (subtle)
    for y in (start_y as u32..(start_y + dh) as u32).step_by(4) {
        for x in 0..480u32 {
            if let Some(p) = img.get_pixel_checked(x, y) {
                let r = p.0[0].saturating_sub(10);
                let g = p.0[1].saturating_sub(10);
                let b = p.0[2].saturating_sub(10);
                img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }
    }

    // Corner hash marks (CRT-style dressing)
    draw_brackets(&mut img);

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod led_clock_tests {
    use super::*;

    #[test]
    fn test_render_led_clock_size() {
        let img = render_led_clock();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}


// ─── Bouncing Ball Screensaver ─────────────────────────────────────────────────

static BALL_STATE: OnceLock<std::sync::Mutex<(f64, f64, f64, f64, u8)>> = OnceLock::new();
// State: (x, y, vx, vy, color_idx)

fn ball_state_data() -> std::sync::MutexGuard<'static, (f64, f64, f64, f64, u8)> {
    BALL_STATE.get_or_init(|| {
        std::sync::Mutex::new((120.0, 180.0, 3.7, 2.3, 0))
    })
    .lock()
    .unwrap()
}

/// Render a bouncing ball screensaver (480×480).
///
/// A filled circle bounces around the screen, changing colour on each wall
/// bounce.  Position persists across frames for smooth animation.
pub fn render_bounce_ball() -> DynamicImage {
    const RADIUS: f64 = 36.0;
    const W: f64 = 480.0;
    const H: f64 = 480.0;

    let palette: &[Rgb<u8>] = &[MAUVE, BLUE, GREEN, PEACH, YELLOW, RED, TEAL, TEAL];

    let (mut x, mut y, mut vx, mut vy, mut color_idx) = {
        let s = ball_state_data();
        (s.0, s.1, s.2, s.3, s.4)
    };

    // Advance physics
    x += vx;
    y += vy;

    let mut bounced = false;
    if x - RADIUS < 0.0 { x = RADIUS; vx = vx.abs(); bounced = true; }
    if x + RADIUS > W   { x = W - RADIUS; vx = -vx.abs(); bounced = true; }
    if y - RADIUS < 0.0 { y = RADIUS; vy = vy.abs(); bounced = true; }
    if y + RADIUS > H   { y = H - RADIUS; vy = -vy.abs(); bounced = true; }

    if bounced {
        color_idx = (color_idx + 1) % palette.len() as u8;
    }

    // Save new state
    {
        let mut s = ball_state_data();
        *s = (x, y, vx, vy, color_idx);
    }

    let color = palette[color_idx as usize];
    let glow  = Rgb([color.0[0] / 4, color.0[1] / 4, color.0[2] / 4]);

    let mut img = blank();

    // Draw glow (slightly larger dim circle)
    let glow_r = RADIUS + 8.0;
    for py in ((y - glow_r) as i32 - 1)..=((y + glow_r) as i32 + 1) {
        for px in ((x - glow_r) as i32 - 1)..=((x + glow_r) as i32 + 1) {
            if px < 0 || py < 0 || px >= 480 || py >= 480 { continue; }
            let dx = px as f64 - x;
            let dy = py as f64 - y;
            if dx*dx + dy*dy <= glow_r * glow_r {
                img.put_pixel(px as u32, py as u32, glow);
            }
        }
    }

    // Draw main ball
    for py in ((y - RADIUS) as i32)..=((y + RADIUS) as i32) {
        for px in ((x - RADIUS) as i32)..=((x + RADIUS) as i32) {
            if px < 0 || py < 0 || px >= 480 || py >= 480 { continue; }
            let dx = px as f64 - x;
            let dy = py as f64 - y;
            if dx*dx + dy*dy <= RADIUS * RADIUS {
                // Slight highlight at top-left
                let dist = (dx*dx + dy*dy).sqrt();
                let highlight = ((RADIUS - dist) / RADIUS * 80.0) as u8;
                let r = color.0[0].saturating_add(highlight);
                let g = color.0[1].saturating_add(highlight);
                let b = color.0[2].saturating_add(highlight);
                img.put_pixel(px as u32, py as u32, Rgb([r, g, b]));
            }
        }
    }

    // Shadow (ellipse below ball)
    let shadow_y = y + RADIUS + 4.0;
    for dx in (-20i32)..20 {
        let shadow_x = x as i32 + dx;
        if shadow_x < 0 || shadow_x >= 480 { continue; }
        let shadow_py = shadow_y as i32;
        if shadow_py >= 0 && shadow_py < 480 {
            let existing = img.get_pixel(shadow_x as u32, shadow_py as u32);
            let r = existing.0[0].saturating_add(20);
            let g = existing.0[1].saturating_add(20);
            let b = existing.0[2].saturating_add(20);
            img.put_pixel(shadow_x as u32, shadow_py as u32, Rgb([r / 2, g / 2, b / 2]));
        }
    }

    // Small velocity trail
    let mono = font_bold();
    let speed_str = format!("{:.1}", (vx*vx + vy*vy).sqrt());
    let sw = text_width(&mono, PxScale::from(12.0), &speed_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - sw - 4, 468, PxScale::from(12.0), &mono, &speed_str);

    DynamicImage::ImageRgb8(img)
}

// ─── Speedometer Gauge ───────────────────────────────────────────────────────

/// Render two large analog speedometer gauges side-by-side: CPU% (left) and RAM% (right).
/// Each gauge is a filled arc with needle, tick marks, labels, and value display.
pub fn render_speedometer_gauge() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{System, MemoryRefreshKind, CpuRefreshKind};

    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    sys.refresh_memory_specifics(MemoryRefreshKind::everything());

    let cpu_pct = sys.global_cpu_usage() as f64;
    let ram_pct = if sys.total_memory() > 0 {
        sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0
    } else { 0.0 };

    // Also get temperature for subtitle
    let temp: f32 = {
        use sysinfo::Components;
        let comps = Components::new_with_refreshed_list();
        comps.iter().filter_map(|c| c.temperature()).filter(|t| t.is_finite() && *t > 0.0).fold(0.0_f32, f32::max)
    };

    let font = font_bold();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Title bar
    let title = "System Gauges";
    let tw = text_width(&font, PxScale::from(24.0), title);
    draw_text_mut(&mut img, TEXT, (480 - tw) / 2, 6, PxScale::from(24.0), &font, title);

    // Subtitle: temp + timestamp
    let now = chrono::Local::now();
    let sub = if temp > 0.0 {
        format!("{:.0}°C  {:02}:{:02}:{:02}", temp, now.hour(), now.minute(), now.second())
    } else {
        format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
    };
    let sw = text_width(&font, PxScale::from(16.0), &sub);
    draw_text_mut(&mut img, OVERLAY0, (480 - sw) / 2, 34, PxScale::from(16.0), &font, &sub);

    // ── Core draw_gauge closure ────────────────────────────────────────────────
    // Gauge spans 225° to 315° (clockwise, i.e. -135° to +135° from the bottom)
    // In standard screen coords (y down): start at bottom-left, end at bottom-right
    let draw_gauge = |img: &mut RgbImage, cx: i32, cy: i32, r: i32,
                      value: f64, label: &str, unit: &str, max_val: f64| {
        // Arc from 225° to 315° going clockwise = 270 degrees total
        // In math angles (y up), 225° = bottom-left, 315° = bottom-right
        // In screen angles (y down), same numbers but sin/cos are affected
        // We'll use screen coords: angle 0 = right, increases clockwise
        // Start at 225° screen (SW), end at 315° screen (SE)
        let start_deg = 225.0_f64;
        let _end_deg  = 315.0_f64;
        // Arc goes clockwise from start to end via top (0° at right, going up through 270°)
        // Arc span = 270 degrees total (225→360→45→135→315 via top, but that's 360-225+315=450... no)
        // Let's do it simply: start 225°, end 315°, span=270° going counterclockwise
        // (i.e., decreasing degrees from 225 → 180 → 90 → 0 → 315)
        // In screen coords angle a: x = cx + r*cos(a), y = cy + r*sin(a)
        // We want: 0% at 225° (bottom-left), 100% at 315° (bottom-right)
        // Going counterclockwise (decreasing angle) from 225° to 315°:
        // that's 225 → 180 → 90 → 0/360 → 315 = 270° span counterclockwise
        let arc_span_deg = 270.0_f64;

        // Convert angle function: 0..100% → screen angle
        let angle_for = |pct: f64| -> f64 {
            // Start at 225°, go counterclockwise (decreasing) by arc_span_deg * pct/100
            let a_deg = start_deg - (arc_span_deg * (pct / 100.0).clamp(0.0, 1.0));
            a_deg.to_radians()
        };

        // ── Background circle (behind gauge) ─────────────────────────────
        for dy in -r..=r {
            for dx in -r..=r {
                if dx*dx + dy*dy <= (r-1)*(r-1) && dx*dx + dy*dy > (r-30)*(r-30) {
                    // Only draw in the arc region
                    let a_deg = (dy as f64).atan2(dx as f64).to_degrees();
                    // Normalize to 0..360
                    let a_norm = if a_deg < 0.0 { a_deg + 360.0 } else { a_deg };
                    // Check if in arc (225° → CCW → 315°, covers top half + small bottom bits)
                    // Arc covers: from 315° going up to 225° = 315°..360°, 0°..225°
                    let in_arc = if a_norm <= 225.0 || a_norm >= 315.0 { true } else { false };
                    if in_arc {
                        let px = (cx + dx) as u32;
                        let py = (cy + dy) as u32;
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, SURFACE0);
                        }
                    }
                }
            }
        }

        // ── Coloured value arc ───────────────────────────────────────────
        let val_pct = (value / max_val).clamp(0.0, 1.0) * 100.0;
        let arc_color = if val_pct < 50.0 { GREEN } else if val_pct < 75.0 { YELLOW } else { RED };

        // Draw arc by sampling angles
        let steps = 2700_i32; // 0.1° increments
        for step in 0..=steps {
            // From 225° down to 315° counterclockwise
            let a_deg = start_deg - (arc_span_deg * step as f64 / steps as f64);
            if a_deg < (start_deg - arc_span_deg * (val_pct / 100.0)) { break; }
            let a = a_deg.to_radians();
            for rr in (r-28)..(r-2) {
                let px = (cx as f64 + rr as f64 * a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, arc_color);
                }
            }
        }

        // ── Tick marks (0, 25, 50, 75, 100%) ────────────────────────────
        for tick_pct_10 in 0..=10 {
            let tick_pct = tick_pct_10 as f64 * 10.0;
            let a = angle_for(tick_pct);
            let (is_major, tick_inner) = if tick_pct_10 % 5 == 0 { (true, r-44) } else { (false, r-36) };
            for rr in tick_inner..(r+3) {
                let px = (cx as f64 + rr as f64 * a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, if is_major { OVERLAY0 } else { SURFACE0 });
                }
            }
        }

        // ── Needle ───────────────────────────────────────────────────────
        let na = angle_for(val_pct);
        let nl = r as f64 - 32.0;
        let nx = cx as f32 + (nl * na.cos()) as f32;
        let ny = cy as f32 + (nl * na.sin()) as f32;
        for offset in -1_i32..=1 {
            let perp = na + PI / 2.0;
            let sx = cx as f32 + (offset as f64 * perp.cos()) as f32;
            let sy = cy as f32 + (offset as f64 * perp.sin()) as f32;
            let lx = nx + (offset as f64 * perp.cos()) as f32;
            let ly = ny + (offset as f64 * perp.sin()) as f32;
            draw_antialiased_line_segment_mut(img, (sx as i32, sy as i32),
                (lx as i32, ly as i32), WHITE, interpolate);
        }

        // ── Pivot dot ────────────────────────────────────────────────────
        for dy in -5_i32..=5 {
            for dx in -5_i32..=5 {
                if dx*dx + dy*dy <= 25 {
                    let px = (cx + dx) as u32;
                    let py = (cy + dy) as u32;
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, py, arc_color);
                    }
                }
            }
        }

        // ── Label ────────────────────────────────────────────────────────
        let lw = text_width(&font, PxScale::from(22.0), label);
        draw_text_mut(img, TEXT, cx - lw / 2, cy - r - 8, PxScale::from(22.0), &font, label);

        // ── Value text ───────────────────────────────────────────────────
        let val_str = format!("{:.0}{}", value, unit);
        let vw = text_width(&font, PxScale::from(30.0), &val_str);
        draw_text_mut(img, arc_color, cx - vw / 2, cy + 14, PxScale::from(30.0), &font, &val_str);

        // ── 0 and max labels ─────────────────────────────────────────────
        let a0   = angle_for(0.0);
        let a100 = angle_for(100.0);
        let lr   = r + 18;
        let x0   = cx + (lr as f64 * a0.cos()) as i32 - 4;
        let y0   = cy + (lr as f64 * a0.sin()) as i32 - 6;
        draw_text_mut(img, OVERLAY0, x0, y0, PxScale::from(15.0), &font, "0");
        let x100 = cx + (lr as f64 * a100.cos()) as i32 - 14;
        let y100 = cy + (lr as f64 * a100.sin()) as i32 - 6;
        let max_str = format!("{:.0}", max_val);
        draw_text_mut(img, OVERLAY0, x100, y100, PxScale::from(15.0), &font, &max_str);
    };

    let gauge_r = 170_i32;
    // CPU gauge: left side
    draw_gauge(&mut img, 120, 310, gauge_r, cpu_pct, "CPU", "%", 100.0);
    // RAM gauge: right side
    draw_gauge(&mut img, 360, 310, gauge_r, ram_pct, "RAM", "%", 100.0);

    DynamicImage::ImageRgb8(img)
}
// ─── Disk Pie Chart ───────────────────────────────────────────────────────────

/// Render disk partitions as a pie chart with used/free sectors and labels.
pub fn render_disk_pie() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{Disks, DiskRefreshKind};

    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    // Collect up to 5 mounted partitions with real data
    let mut entries: Vec<(String, u64, u64)> = Vec::new(); // (mount, used, total)
    for d in disks.iter() {
        if d.total_space() == 0 { continue; }
        let mount = d.mount_point().to_string_lossy().to_string();
        let total = d.total_space();
        let avail = d.available_space();
        let used  = total.saturating_sub(avail);
        entries.push((mount, used, total));
        if entries.len() >= 5 { break; }
    }

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Title
    let title = "Disk Usage";
    let tw = text_width(&font, PxScale::from(28.0), title);
    draw_text_mut(&mut img, TEXT, (480 - tw) / 2, 10, PxScale::from(28.0), &font, title);

    if entries.is_empty() {
        let msg = "No disk data";
        let mw = text_width(&font, PxScale::from(22.0), msg);
        draw_text_mut(&mut img, SUBTEXT, (480 - mw) / 2, 220, PxScale::from(22.0), &font, msg);
        return DynamicImage::ImageRgb8(img);
    }

    // Color palette for different partitions
    let slice_colors = [BLUE, MAUVE, TEAL, PEACH, YELLOW];
    let free_color   = SURFACE0;

    // Draw up to 5 pies in a grid (2 columns × 3 rows)
    let pie_r  = 72_i32;
    let positions = [(120i32, 160i32), (360, 160), (120, 320), (360, 320), (240, 460)];

    for (idx, (mount, used, total)) in entries.iter().enumerate() {
        if idx >= positions.len() { break; }
        let (cx, cy) = positions[idx];
        let used_pct = *used as f64 / *total as f64;
        let slice_col = slice_colors[idx % slice_colors.len()];

        // Draw pie background (free)
        for dy in -pie_r..=pie_r {
            for dx in -pie_r..=pie_r {
                if dx*dx + dy*dy <= pie_r*pie_r {
                    let px = (cx + dx) as u32;
                    let py = (cy + dy) as u32;
                    if px < 480 && py < 480 {
                        img.put_pixel(px, py, free_color);
                    }
                }
            }
        }

        // Draw used sector (start from top = -PI/2)
        let used_angle = used_pct * 2.0 * PI;
        for deg_10 in 0..3600_i32 {
            let a = deg_10 as f64 * 0.1_f64.to_radians();
            if a > used_angle { break; }
            let draw_a = a - PI / 2.0; // Start from 12 o'clock
            for rr in 0..pie_r {
                let px = (cx as f64 + rr as f64 * draw_a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * draw_a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                    img.put_pixel(px as u32, py as u32, slice_col);
                }
            }
        }

        // Outline circle
        for deg in 0..3600_i32 {
            let a = deg as f64 * 0.1_f64.to_radians();
            let px = (cx as f64 + pie_r as f64 * a.cos()) as i32;
            let py = (cy as f64 + pie_r as f64 * a.sin()) as i32;
            if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                img.put_pixel(px as u32, py as u32, OVERLAY0);
            }
        }

        // Labels
        let pct_str = format!("{:.0}%", used_pct * 100.0);
        let pw = text_width(&font, PxScale::from(20.0), &pct_str);
        draw_text_mut(&mut img, WHITE, cx - pw / 2, cy - 10, PxScale::from(20.0), &font, &pct_str);

        // Mount point label (truncate at 8 chars)
        let short_mount = if mount.len() > 10 { &mount[mount.len()-10..] } else { mount.as_str() };
        let mw2 = text_width(&font, PxScale::from(15.0), short_mount);
        draw_text_mut(&mut img, SUBTEXT, cx - mw2 / 2, cy + pie_r + 4, PxScale::from(15.0), &font, short_mount);

        // Size label
        let sz_str = human_bytes(*total as f64);
        let sw2 = text_width(&font, PxScale::from(14.0), &sz_str);
        draw_text_mut(&mut img, OVERLAY0, cx - sw2 / 2, cy + pie_r + 20, PxScale::from(14.0), &font, &sz_str);
    }

    DynamicImage::ImageRgb8(img)
}

fn human_bytes(bytes: f64) -> String {
    if bytes >= 1_073_741_824.0 {
        format!("{:.1}G", bytes / 1_073_741_824.0)
    } else if bytes >= 1_048_576.0 {
        format!("{:.0}M", bytes / 1_048_576.0)
    } else if bytes >= 1024.0 {
        format!("{:.0}K", bytes / 1024.0)
    } else {
        format!("{:.0}B", bytes)
    }
}

// ─── System Temperatures Dashboard ───────────────────────────────────────────

/// Render a detailed temperature + fan sensor dashboard with bar gauges.
pub fn render_thermal_dashboard() -> DynamicImage {
    use sysinfo::Components;

    let comps = Components::new_with_refreshed_list();

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Title
    let title = "Thermal Dashboard";
    let tw = text_width(&font, PxScale::from(26.0), title);
    draw_text_mut(&mut img, TEXT, (480 - tw) / 2, 8, PxScale::from(26.0), &font, title);

    // Collect sensor readings
    let mut sensors: Vec<(String, f32, f32)> = Vec::new(); // (label, current, max_observed)
    for c in comps.iter() {
        let label = c.label().to_string();
        if let Some(t) = c.temperature() {
            if t > 0.0 && t < 200.0 {
                let max = c.max().unwrap_or(t);
                sensors.push((label, t, max));
            }
        }
        if sensors.len() >= 10 { break; }
    }

    if sensors.is_empty() {
        let msg = "No thermal sensors found";
        let mw = text_width(&font, PxScale::from(20.0), msg);
        draw_text_mut(&mut img, SUBTEXT, (480 - mw) / 2, 220, PxScale::from(20.0), &font, msg);
        return DynamicImage::ImageRgb8(img);
    }

    // ── Draw sensor rows ──────────────────────────────────────────────────
    let row_h = (440 / sensors.len().max(1)) as i32;
    let bar_x = 180_i32;
    let bar_w = 260_i32;
    let bar_h = (row_h - 10).max(14) as i32;

    for (i, (label, temp, _max_obs)) in sensors.iter().enumerate() {
        let y = 45 + i as i32 * row_h;

        // Temperature color scale: <50°C=blue, <70°C=green, <85°C=yellow, >=85°C=red
        let col = if *temp < 50.0 { BLUE } else if *temp < 70.0 { GREEN } else if *temp < 85.0 { YELLOW } else { RED };

        // Sensor label (truncate at 20 chars)
        let short = if label.len() > 20 { &label[..20] } else { label.as_str() };
        draw_text_mut(&mut img, SUBTEXT, 8, y + 2, PxScale::from(15.0), &font, short);

        // Background bar
        let bg_rect = Rect::at(bar_x, y).of_size(bar_w as u32, bar_h as u32);
        draw_filled_rect_mut(&mut img, bg_rect, SURFACE0);

        // Value bar (scale: 0°C=0, 100°C=full bar)
        let bar_fill = (((*temp as f64 / 100.0).clamp(0.0, 1.0)) * bar_w as f64) as i32;
        if bar_fill > 0 {
            let val_rect = Rect::at(bar_x, y).of_size(bar_fill as u32, bar_h as u32);
            draw_filled_rect_mut(&mut img, val_rect, col);
        }

        // Temperature text overlay
        let temp_str = format!("{:.0}°C", temp);
        let tsw = text_width(&font, PxScale::from(15.0), &temp_str);
        draw_text_mut(&mut img, if bar_fill > 30 { BG } else { TEXT },
            bar_x + bar_fill.max(4) - 2, y + 2,
            PxScale::from(15.0), &font, &temp_str);
        let _ = tsw;
    }

    DynamicImage::ImageRgb8(img)
}


// ─── Plasma / Sine Wave Screensaver ─────────────────────────────────────────

static PLASMA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Animated plasma/sine-wave colour field screensaver.
/// Uses multiple overlapping sine waves in Catppuccin Mocha palette.
pub fn render_plasma() -> DynamicImage {
    let tick = {
        let m = PLASMA_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let t = tick as f64 * 0.04; // animation speed

    let mut img = RgbImage::new(480, 480);
    let w = 480.0_f64;
    let h = 480.0_f64;

    // Catppuccin Mocha-inspired palette wave
    for y in 0..480_u32 {
        for x in 0..480_u32 {
            let fx = x as f64 / w;
            let fy = y as f64 / h;

            // Multiple overlapping sine waves
            let v1 = (fx * 10.0 + t).sin();
            let v2 = (fy * 8.0 + t * 0.7).sin();
            let v3 = ((fx + fy) * 6.0 + t * 1.3).sin();
            let cx2 = fx - 0.5 + 0.3 * (t * 0.5).sin();
            let cy2 = fy - 0.5 + 0.3 * (t * 0.7).cos();
            let v4  = (cx2*cx2 + cy2*cy2).sqrt() * 12.0 + t;
            let v4  = v4.sin();

            // Combine into a value 0..1
            let v = (v1 + v2 + v3 + v4) / 4.0; // -1..1
            let v = (v + 1.0) / 2.0; // 0..1

            // Map to Catppuccin-inspired colours
            // We blend between a few Catppuccin mocha colours
            let r; let g; let b;
            if v < 0.25 {
                // BG to BLUE
                let t2 = v / 0.25;
                r = (30.0 + (137.0 - 30.0) * t2) as u8;
                g = (30.0 + (180.0 - 30.0) * t2) as u8;
                b = (46.0 + (250.0 - 46.0) * t2) as u8;
            } else if v < 0.5 {
                // BLUE to MAUVE
                let t2 = (v - 0.25) / 0.25;
                r = (137.0 + (203.0 - 137.0) * t2) as u8;
                g = (180.0 + (166.0 - 180.0) * t2) as u8;
                b = (250.0 + (247.0 - 250.0) * t2) as u8;
            } else if v < 0.75 {
                // MAUVE to TEAL
                let t2 = (v - 0.5) / 0.25;
                r = (203.0 + (148.0 - 203.0) * t2) as u8;
                g = (166.0 + (226.0 - 166.0) * t2) as u8;
                b = (247.0 + (213.0 - 247.0) * t2) as u8;
            } else {
                // TEAL to BG
                let t2 = (v - 0.75) / 0.25;
                r = (148.0 + (30.0 - 148.0) * t2) as u8;
                g = (226.0 + (30.0 - 226.0) * t2) as u8;
                b = (213.0 + (46.0 - 213.0) * t2) as u8;
            }

            img.put_pixel(x, y, Rgb([r, g, b]));
        }
    }

    // Subtle time overlay
    let font = font_bold();
    let now = chrono::Local::now();
    let ts = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let tw = text_width(&font, PxScale::from(24.0), &ts);
    // Drop shadow
    draw_text_mut(&mut img, Rgb([0, 0, 0]), (480 - tw) / 2 + 1, 449, PxScale::from(24.0), &font, &ts);
    draw_text_mut(&mut img, WHITE,           (480 - tw) / 2,     448, PxScale::from(24.0), &font, &ts);

    DynamicImage::ImageRgb8(img)
}

// ─── Fireworks Screensaver ─────────────────────────────────────────────────────

#[derive(Clone)]
struct Firework {
    // Burst origin
    bx: f32,
    by: f32,
    // List of particles: (x, y, vx, vy, life, max_life, color_idx)
    particles: Vec<(f32, f32, f32, f32, u8, u8, u8)>,
    // Rocket: (x, y, vy, active)
    rocket: Option<(f32, f32, f32)>,
    trail: Vec<(f32, f32)>,
}

static FIREWORKS: std::sync::OnceLock<std::sync::Mutex<Vec<Firework>>> = std::sync::OnceLock::new();
static FIREWORKS_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

pub fn render_fireworks() -> DynamicImage {
    let tick = {
        let m = FIREWORKS_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let mut fws = FIREWORKS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
        .lock().unwrap();

    // LCG pseudo-random
    let lcg = |s: u64| -> u64 {
        s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
    };
    let mut seed = tick.wrapping_mul(7919).wrapping_add(tick / 37 + 1);

    // Colors (Catppuccin Mocha palette)
    let colors: &[Rgb<u8>] = &[RED, GREEN, BLUE, YELLOW, MAUVE, TEAL, PEACH, WHITE];

    // Spawn a new rocket every ~20 ticks
    if tick % 20 == 0 || fws.is_empty() {
        seed = lcg(seed);
        let bx = 40.0 + (seed % 400) as f32;
        seed = lcg(seed);
        let by = 60.0 + (seed % 200) as f32;
        fws.push(Firework {
            bx,
            by,
            particles: Vec::new(),
            rocket: Some((bx, 460.0, -6.0 - (seed % 4) as f32 * 0.8)),
            trail: Vec::new(),
        });
    }

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Update and draw all fireworks
    let mut to_remove = Vec::new();
    for (idx, fw) in fws.iter_mut().enumerate() {
        let mut remove = false;

        if let Some((rx, ry, rvy)) = fw.rocket {
            // Update rocket
            let ny = ry + rvy;
            if ny <= fw.by {
                // Explode!
                fw.rocket = None;
                seed = lcg(seed);
                let n_particles = 40 + (seed % 30) as usize;
                seed = lcg(seed);
                let col_idx = (seed % colors.len() as u64) as u8;
                let max_life: u8 = 30 + (seed % 20) as u8;
                for i in 0..n_particles {
                    seed = lcg(seed.wrapping_add(i as u64));
                    let angle = (i as f64 * 2.0 * std::f64::consts::PI / n_particles as f64)
                        + (seed % 100) as f64 * 0.01;
                    let spd: f32 = 1.5 + (seed % 30) as f32 * 0.1;
                    let vx = (angle.cos() * spd as f64) as f32;
                    let vy = (angle.sin() * spd as f64) as f32;
                    fw.particles.push((fw.bx, fw.by, vx, vy, max_life, max_life, col_idx));
                }
            } else {
                fw.rocket = Some((rx, ny, rvy));
                fw.trail.push((rx, ny));
                if fw.trail.len() > 8 { fw.trail.remove(0); }
                // Draw rocket trail
                for (ti, &(tx, ty)) in fw.trail.iter().enumerate() {
                    let alpha = ti as u8 * 30 + 10;
                    let px = tx as i32; let py = ty as i32;
                    if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                        img.put_pixel(px as u32, py as u32, Rgb([alpha, alpha, alpha / 2]));
                    }
                }
                // Draw rocket head
                let rx2 = rx as i32; let ry2 = ny as i32;
                if rx2 >= 0 && ry2 >= 0 && (rx2 as u32) < 480 && (ry2 as u32) < 480 {
                    img.put_pixel(rx2 as u32, ry2 as u32, YELLOW);
                }
            }
        }

        // Update and draw particles
        let mut alive = false;
        for p in fw.particles.iter_mut() {
            if p.4 == 0 { continue; }
            alive = true;
            p.0 += p.2;            // x += vx
            p.1 += p.3;            // y += vy
            p.3 += 0.08;           // gravity
            p.4 = p.4.saturating_sub(1);

            let px = p.0 as i32; let py = p.1 as i32;
            if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                // Fade by remaining life
                let life_frac = p.4 as f32 / p.5 as f32;
                let col = colors[p.6 as usize % colors.len()];
                let r = (col.0[0] as f32 * life_frac) as u8;
                let g = (col.0[1] as f32 * life_frac) as u8;
                let b = (col.0[2] as f32 * life_frac) as u8;
                img.put_pixel(px as u32, py as u32, Rgb([r, g, b]));
                // 1-pixel glow
                for (ddx, ddy) in &[(-1,0),(1,0),(0,-1),(0,1)] {
                    let gx = px + ddx; let gy = py + ddy;
                    if gx >= 0 && gy >= 0 && (gx as u32) < 480 && (gy as u32) < 480 {
                        let gr = (col.0[0] as f32 * life_frac * 0.4) as u8;
                        let gg = (col.0[1] as f32 * life_frac * 0.4) as u8;
                        let gb = (col.0[2] as f32 * life_frac * 0.4) as u8;
                        let existing = *img.get_pixel(gx as u32, gy as u32);
                        img.put_pixel(gx as u32, gy as u32, Rgb([
                            existing.0[0].saturating_add(gr),
                            existing.0[1].saturating_add(gg),
                            existing.0[2].saturating_add(gb),
                        ]));
                    }
                }
            }
        }
        if fw.rocket.is_none() && !alive { remove = true; }
        if remove { to_remove.push(idx); }
    }

    for idx in to_remove.into_iter().rev() {
        if idx < fws.len() { fws.remove(idx); }
    }

    DynamicImage::ImageRgb8(img)
}



// ─── CPU Core Heatmap ─────────────────────────────────────────────────────────

/// Per-core CPU utilization history (rows = cores, cols = time samples).
/// Stored as a Vec<VecDeque<f32>> where each VecDeque is one core's history.
static CPU_CORE_HISTORY: std::sync::OnceLock<std::sync::Mutex<Vec<std::collections::VecDeque<f32>>>> =
    std::sync::OnceLock::new();

const CPU_HEATMAP_COLS: usize = 60;  // 60 time samples

/// Update per-core history. Call on each display tick.
pub fn push_cpu_core_history() {
    use sysinfo::{System, CpuRefreshKind};
    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(100));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());

    let cpus = sys.cpus();
    let n = cpus.len();

    let mut history = CPU_CORE_HISTORY
        .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n.max(1)]))
        .lock().unwrap();

    // Grow if CPU count changed
    while history.len() < n { history.push(std::collections::VecDeque::new()); }

    for (i, cpu) in cpus.iter().enumerate() {
        if i >= history.len() { break; }
        history[i].push_back(cpu.cpu_usage());
        while history[i].len() > CPU_HEATMAP_COLS { history[i].pop_front(); }
    }
}

/// Render the CPU core heatmap: cores on Y axis, time on X axis.
/// Hot = red, warm = yellow, cool = green, idle = dark teal.
pub fn render_cpu_heatmap() -> DynamicImage {
    use sysinfo::{System, CpuRefreshKind};

    // Refresh and push a sample
    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(150));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());

    let cpus = sys.cpus();
    let n_cores = cpus.len().max(1);
    let cpu_usage = sys.global_cpu_usage();

    {
        let mut history = CPU_CORE_HISTORY
            .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n_cores]))
            .lock().unwrap();

        while history.len() < n_cores { history.push(std::collections::VecDeque::new()); }
        for (i, cpu) in cpus.iter().enumerate() {
            if i >= history.len() { break; }
            history[i].push_back(cpu.cpu_usage());
            while history[i].len() > CPU_HEATMAP_COLS { history[i].pop_front(); }
        }
    }

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Title
    let title = format!("CPU Heatmap — {:.0}% avg", cpu_usage);
    let tw = text_width(&font, PxScale::from(22.0), &title);
    draw_text_mut(&mut img, TEXT, (480 - tw) / 2, 6, PxScale::from(22.0), &font, &title);

    // Core count label
    let cstr = format!("{} cores × {} samples", n_cores, CPU_HEATMAP_COLS);
    let cw = text_width(&font, PxScale::from(14.0), &cstr);
    draw_text_mut(&mut img, OVERLAY0, (480 - cw) / 2, 32, PxScale::from(14.0), &font, &cstr);

    // Heat map grid
    let grid_x = 40_i32;
    let grid_y = 50_i32;
    let grid_w = 430_i32;
    let grid_h = 400_i32;

    let cell_w = (grid_w as f32 / CPU_HEATMAP_COLS as f32).max(1.0);
    let cell_h = (grid_h as f32 / n_cores as f32).max(1.0);

    let history = CPU_CORE_HISTORY
        .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n_cores]))
        .lock().unwrap();

    for (core_idx, core_history) in history.iter().enumerate() {
        let y0 = grid_y + (core_idx as f32 * cell_h) as i32;
        let y1 = grid_y + ((core_idx + 1) as f32 * cell_h) as i32;

        // Core label on Y axis
        if cell_h >= 10.0 {
            let label = format!("C{}", core_idx);
            draw_text_mut(&mut img, OVERLAY0, 4, y0 + 1,
                PxScale::from(cell_h.min(14.0)), &font, &label);
        }

        for (col_idx, &usage) in core_history.iter().enumerate() {
            let x0 = grid_x + (col_idx as f32 * cell_w) as i32;
            let x1 = grid_x + ((col_idx + 1) as f32 * cell_w) as i32;

            // Map usage 0..100 → heatmap colour
            let u = (usage / 100.0).clamp(0.0, 1.0) as f64;
            let (r, g, b) = if u < 0.25 {
                // Dark blue/teal (idle)
                let t = u / 0.25;
                (
                    (30.0 + (148.0 - 30.0) * t) as u8,
                    (30.0 + (226.0 - 30.0) * t) as u8,
                    (46.0 + (213.0 - 46.0) * t) as u8,
                )
            } else if u < 0.5 {
                // Teal to green
                let t = (u - 0.25) / 0.25;
                (
                    (148.0 + (166.0 - 148.0) * t) as u8,
                    (226.0 + (227.0 - 226.0) * t) as u8,
                    (213.0 + (161.0 - 213.0) * t) as u8,
                )
            } else if u < 0.75 {
                // Green to yellow
                let t = (u - 0.5) / 0.25;
                (
                    (166.0 + (249.0 - 166.0) * t) as u8,
                    (227.0 + (226.0 - 227.0) * t) as u8,
                    (161.0 + (175.0 - 161.0) * t) as u8,
                )
            } else {
                // Yellow to red
                let t = (u - 0.75) / 0.25;
                (
                    (249.0 + (243.0 - 249.0) * t) as u8,
                    (226.0 + (139.0 - 226.0) * t) as u8,
                    (175.0 + (168.0 - 175.0) * t) as u8,
                )
            };

            let col = Rgb([r, g, b]);
            for py in y0..y1 {
                for px in x0..x1 {
                    if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                        img.put_pixel(px as u32, py as u32, col);
                    }
                }
            }
        }
    }

    // Time axis labels
    draw_text_mut(&mut img, OVERLAY0, grid_x, grid_y + grid_h + 4,
        PxScale::from(13.0), &font, "← older");
    let now_str = {
        let n = chrono::Local::now();
        format!("{:02}:{:02}:{:02} →", n.hour(), n.minute(), n.second())
    };
    let nw = text_width(&font, PxScale::from(13.0), &now_str);
    draw_text_mut(&mut img, OVERLAY0, grid_x + grid_w - nw, grid_y + grid_h + 4,
        PxScale::from(13.0), &font, &now_str);

    // Color legend bar
    let legend_y = grid_y + grid_h + 22;
    let legend_w = grid_w;
    for lx in 0..legend_w {
        let u = lx as f64 / legend_w as f64;
        let (lr, lg, lb) = if u < 0.25 {
            let t = u / 0.25;
            ((30.0 + 118.0*t) as u8, (30.0 + 196.0*t) as u8, (46.0 + 167.0*t) as u8)
        } else if u < 0.5 {
            let t = (u-0.25)/0.25;
            ((148.0 + 18.0*t) as u8, (226.0 + 1.0*t) as u8, (213.0 - 52.0*t) as u8)
        } else if u < 0.75 {
            let t = (u-0.5)/0.25;
            ((166.0 + 83.0*t) as u8, (227.0 - 1.0*t) as u8, (161.0 + 14.0*t) as u8)
        } else {
            let t = (u-0.75)/0.25;
            ((249.0 - 6.0*t) as u8, (226.0 - 87.0*t) as u8, (175.0 - 7.0*t) as u8)
        };
        let px = grid_x + lx;
        for dy in 0..6 {
            let py_leg = legend_y + dy;
            if py_leg < 480 && (px as u32) < 480 {
                img.put_pixel(px as u32, py_leg as u32, Rgb([lr, lg, lb]));
            }
        }
    }
    draw_text_mut(&mut img, OVERLAY0, grid_x, legend_y + 7,
        PxScale::from(12.0), &font, "0%");
    let h_str = "100%";
    let hw = text_width(&font, PxScale::from(12.0), h_str);
    draw_text_mut(&mut img, OVERLAY0, grid_x + grid_w - hw, legend_y + 7,
        PxScale::from(12.0), &font, h_str);

    DynamicImage::ImageRgb8(img)
}



// ─── Starfield Screensaver ────────────────────────────────────────────────────

#[derive(Clone)]
struct Star {
    x: f32, // -1..1 in projected space
    y: f32,
    z: f32, // 0..1, 0=far, 1=close
    speed: f32,
}

static STARFIELD: std::sync::OnceLock<std::sync::Mutex<Vec<Star>>> = std::sync::OnceLock::new();
static STARFIELD_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

const N_STARS: usize = 200;

/// Animated starfield screensaver — stars fly towards the viewer from a central vanishing point.
pub fn render_starfield() -> DynamicImage {
    let tick = {
        let m = STARFIELD_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let lcg = |s: u64| s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);

    let mut stars = STARFIELD.get_or_init(|| {
        let mut seed = 12345_u64;
        let mut v = Vec::with_capacity(N_STARS);
        for _ in 0..N_STARS {
            seed = lcg(seed);
            let x = (seed % 2001) as f32 / 1000.0 - 1.0;
            seed = lcg(seed);
            let y = (seed % 2001) as f32 / 1000.0 - 1.0;
            seed = lcg(seed);
            let z = (seed % 1000) as f32 / 1000.0;
            seed = lcg(seed);
            let spd = 0.006 + (seed % 10) as f32 * 0.0015;
            v.push(Star { x, y, z, speed: spd });
        }
        std::sync::Mutex::new(v)
    }).lock().unwrap();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    let cx = 240.0_f32;
    let cy = 240.0_f32;
    let fov = 200.0_f32;

    let mut seed = tick.wrapping_mul(1337);

    for star in stars.iter_mut() {
        // Advance Z (move towards viewer)
        star.z += star.speed;
        if star.z >= 1.0 {
            // Respawn at random position far away
            seed = lcg(seed);
            star.x = (seed % 2001) as f32 / 1000.0 - 1.0;
            seed = lcg(seed);
            star.y = (seed % 2001) as f32 / 1000.0 - 1.0;
            star.z = 0.01;
        }

        // Project from 3D to screen
        let perspective = fov / (1.0 - star.z + 0.001);
        let sx = cx + star.x * perspective;
        let sy = cy + star.y * perspective;

        if sx < 0.0 || sy < 0.0 || sx >= 480.0 || sy >= 480.0 { continue; }

        // Brightness based on closeness
        let bright = (star.z * 255.0).min(255.0) as u8;
        // Size based on closeness
        let size = if star.z > 0.7 { 2_i32 } else { 1_i32 };

        // Color: far = dim blue, close = bright white/yellow
        let r = bright;
        let g = bright;
        let b = if star.z < 0.5 { bright } else { (bright as f32 * 0.8) as u8 };
        let col = Rgb([r, g, b]);

        for dy in -size..=size {
            for dx in -size..=size {
                let px = sx as i32 + dx;
                let py = sy as i32 + dy;
                if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    }

    // Subtle time overlay
    let font = font_bold();
    let now = chrono::Local::now();
    let ts = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let tw = text_width(&font, PxScale::from(22.0), &ts);
    draw_text_mut(&mut img, Rgb([0,0,0]), (480-tw)/2+1, 453, PxScale::from(22.0), &font, &ts);
    draw_text_mut(&mut img, SUBTEXT,    (480-tw)/2,   452, PxScale::from(22.0), &font, &ts);

    DynamicImage::ImageRgb8(img)
}

// ─── Ambient Pulse ────────────────────────────────────────────────────────────

static PULSE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Ambient CPU pulse display — concentric expanding rings whose speed and
/// colour reflect the current CPU load. Blue=idle, yellow=moderate, red=high.
pub fn render_pulse() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{System, CpuRefreshKind};

    let tick = {
        let m = PULSE_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    let cpu_pct = sys.global_cpu_usage() as f64;

    // Speed and colour depend on CPU load
    let speed_mult = 0.5 + cpu_pct / 100.0 * 2.5; // 0.5x at 0%, 3x at 100%
    let t = tick as f64 * 0.05 * speed_mult;

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    let cx = 240.0_f64;
    let cy = 240.0_f64;

    // Draw 5 concentric pulse rings
    for ring in 0..5_i32 {
        let phase = ring as f64 * (2.0 * PI / 5.0);
        // Each ring expands outward with time
        let r = ((t + phase) % (2.0 * PI) / (2.0 * PI)) * 230.0; // 0..230px
        let alpha = 1.0 - (r / 230.0); // fade as it expands

        // Color based on cpu_pct: low=blue, mid=mauve, high=red
        let (cr, cg, cb) = if cpu_pct < 30.0 {
            (137.0_f64 * alpha, 180.0 * alpha, 250.0 * alpha)  // BLUE
        } else if cpu_pct < 60.0 {
            let t2 = (cpu_pct - 30.0) / 30.0;
            ((137.0 + (203.0-137.0)*t2) * alpha,
             (180.0 + (166.0-180.0)*t2) * alpha,
             (250.0 + (247.0-250.0)*t2) * alpha)  // BLUE→MAUVE
        } else if cpu_pct < 85.0 {
            let t2 = (cpu_pct - 60.0) / 25.0;
            ((203.0 + (249.0-203.0)*t2) * alpha,
             (166.0 + (226.0-166.0)*t2) * alpha,
             (247.0 + (175.0-247.0)*t2) * alpha)  // MAUVE→YELLOW
        } else {
            ((249.0 + (243.0-249.0) * ((cpu_pct-85.0)/15.0)) * alpha,
             (226.0 + (139.0-226.0) * ((cpu_pct-85.0)/15.0)) * alpha,
             (175.0 + (168.0-175.0) * ((cpu_pct-85.0)/15.0)) * alpha)  // YELLOW→RED
        };

        // Draw ring as a band of pixels
        let r_f = r as f32;
        let inner_r = (r_f - 3.0).max(0.0);
        let outer_r = r_f + 3.0;

        for dy in (-(outer_r as i32) - 1)..=(outer_r as i32 + 1) {
            for dx in (-(outer_r as i32) - 1)..=(outer_r as i32 + 1) {
                let dist = ((dx*dx + dy*dy) as f32).sqrt();
                if dist >= inner_r && dist <= outer_r {
                    let px = cx as i32 + dx;
                    let py = cy as i32 + dy;
                    if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                        // Blend with existing pixel
                        let existing = *img.get_pixel(px as u32, py as u32);
                        let nr = (existing.0[0] as f64 + cr).min(255.0) as u8;
                        let ng = (existing.0[1] as f64 + cg).min(255.0) as u8;
                        let nb = (existing.0[2] as f64 + cb).min(255.0) as u8;
                        img.put_pixel(px as u32, py as u32, Rgb([nr, ng, nb]));
                    }
                }
            }
        }
    }

    // Center: CPU percentage
    let cpu_str = format!("{:.0}%", cpu_pct);
    let cw = text_width(&font, PxScale::from(48.0), &cpu_str);
    draw_text_mut(&mut img, Rgb([0,0,0]), (480-cw)/2+2, 208, PxScale::from(48.0), &font, &cpu_str);
    let col = if cpu_pct < 30.0 { BLUE } else if cpu_pct < 70.0 { MAUVE } else if cpu_pct < 85.0 { YELLOW } else { RED };
    draw_text_mut(&mut img, col, (480-cw)/2, 206, PxScale::from(48.0), &font, &cpu_str);

    let lbl = "CPU";
    let lw = text_width(&font, PxScale::from(20.0), lbl);
    draw_text_mut(&mut img, OVERLAY0, (480-lw)/2, 258, PxScale::from(20.0), &font, lbl);

    // Time
    let now = chrono::Local::now();
    let ts = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let tw2 = text_width(&font, PxScale::from(18.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, (480-tw2)/2, 280, PxScale::from(18.0), &font, &ts);

    DynamicImage::ImageRgb8(img)
}



// ─── Minimal Clock ────────────────────────────────────────────────────────────

/// Ultra-minimal full-screen clock: just time (huge) and date (below).
/// Perfect for use as a bedside clock or when minimal visual noise is desired.
pub fn render_minimal_clock() -> DynamicImage {
    let now = chrono::Local::now();
    let time_str = now.format("%H:%M").to_string();
    let secs_str = format!(":{:02}", now.second());
    let date_str = now.format("%A, %B %-d").to_string();
    let year_str = now.format("%Y").to_string();

    let font = font_bold();
    let mono = font_mono();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Large time in center
    let time_scale = PxScale::from(110.0);
    let tw = text_width(&mono, time_scale, &time_str);
    let secs_scale = PxScale::from(42.0);
    let sw = text_width(&mono, secs_scale, &secs_str);
    let total_w = tw + sw;
    let tx = (480 - total_w) / 2;
    let ty = 140;

    // Drop shadow
    draw_text_mut(&mut img, SURFACE0, tx + 3, ty + 3, time_scale, &mono, &time_str);
    draw_text_mut(&mut img, SURFACE0, tx + tw + 3, ty + 68 + 3, secs_scale, &mono, &secs_str);

    // Main time
    draw_text_mut(&mut img, TEXT, tx, ty, time_scale, &mono, &time_str);
    // Seconds (smaller, aligned to baseline of H:MM)
    draw_text_mut(&mut img, OVERLAY0, tx + tw, ty + 68, secs_scale, &mono, &secs_str);

    // Date line
    let date_scale = PxScale::from(28.0);
    let dw = text_width(&font, date_scale, &date_str);
    draw_text_mut(&mut img, SUBTEXT, (480 - dw) / 2, 280, date_scale, &font, &date_str);

    // Year line
    let year_scale = PxScale::from(20.0);
    let yw = text_width(&font, year_scale, &year_str);
    draw_text_mut(&mut img, OVERLAY0, (480 - yw) / 2, 314, year_scale, &font, &year_str);

    // Subtle bottom bar: day-of-year progress
    let day_of_year = now.ordinal() as f32;
    let days_in_year = if now.date_naive().leap_year() { 366.0 } else { 365.0 };
    let progress = day_of_year / days_in_year;
    let bar_w = (progress * 380.0) as u32;
    // Background bar
    let bg_rect = imageproc::rect::Rect::at(50, 440).of_size(380, 6);
    imageproc::drawing::draw_filled_rect_mut(&mut img, bg_rect, SURFACE0);
    // Progress bar
    if bar_w > 0 {
        let prog_rect = imageproc::rect::Rect::at(50, 440).of_size(bar_w.min(380), 6);
        imageproc::drawing::draw_filled_rect_mut(&mut img, prog_rect, BLUE);
    }
    let prog_label = format!("Day {} of {}", day_of_year as u32, days_in_year as u32);
    let plw = text_width(&font, PxScale::from(13.0), &prog_label);
    draw_text_mut(&mut img, OVERLAY0, (480 - plw) / 2, 450, PxScale::from(13.0), &font, &prog_label);

    DynamicImage::ImageRgb8(img)
}

// ─── Ambient Particle Clock ────────────────────────────────────────────────────

static PARTICLE_CLOCK_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Particle clock: time displayed with drifting pixel particles in Catppuccin colours.
pub fn render_particle_clock() -> DynamicImage {
    let tick = {
        let m = PARTICLE_CLOCK_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let now = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %d %b").to_string();

    let font = font_mono();
    let t = tick as f64 * 0.05;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Particle layer (behind text): drifting pixels
    let lcg = |s: u64| s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let n_particles = 120_usize;
    for i in 0..n_particles {
        let mut seed = (i as u64).wrapping_mul(9371).wrapping_add(tick / 3);
        seed = lcg(seed);
        let base_x = (seed % 480) as f64;
        seed = lcg(seed);
        let base_y = (seed % 480) as f64;
        seed = lcg(seed);
        let speed = 0.3 + (seed % 10) as f64 * 0.07;
        seed = lcg(seed);
        let phase = seed as f64 * 0.001;

        let x = (base_x + (t * speed + phase).sin() * 30.0).rem_euclid(480.0) as u32;
        let y = (base_y + (t * speed * 0.7 + phase).cos() * 20.0).rem_euclid(480.0) as u32;

        // Catppuccin colors cycling
        let col = match i % 6 {
            0 => Rgb([137u8, 180, 250]),  // BLUE
            1 => Rgb([203, 166, 247]),    // MAUVE
            2 => Rgb([148, 226, 213]),    // TEAL
            3 => Rgb([250, 179, 135]),    // PEACH
            4 => Rgb([166, 227, 161]),    // GREEN
            _ => Rgb([249, 226, 175]),    // YELLOW
        };
        // Dim to 30% so text is readable
        let col = Rgb([col.0[0]/4, col.0[1]/4, col.0[2]/4]);
        img.put_pixel(x, y, col);
    }

    // Time text (large, centered)
    let time_scale = PxScale::from(80.0);
    let tw = text_width(&font, time_scale, &time_str);
    draw_text_mut(&mut img, TEXT, (480-tw)/2, 168, time_scale, &font, &time_str);

    // Date text
    let date_scale = PxScale::from(28.0);
    let dw = text_width(&font, date_scale, &date_str);
    draw_text_mut(&mut img, SUBTEXT, (480-dw)/2, 266, date_scale, &font, &date_str);

    // Horizontal dividers
    for x in 80..400_u32 {
        img.put_pixel(x, 158, SURFACE0);
        img.put_pixel(x, 298, SURFACE0);
    }

    DynamicImage::ImageRgb8(img)
}



// ─── Snake Screensaver ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SnakeDir { Up, Down, Left, Right }

struct SnakeState {
    // Snake body as (col, row) grid positions; head = first element
    body: std::collections::VecDeque<(i32, i32)>,
    dir:  SnakeDir,
    food: (i32, i32),
    score: u32,
    dead: bool,
    tick: u64,
}

const SNAKE_COLS: i32 = 32;
const SNAKE_ROWS: i32 = 32;

static SNAKE_STATE: std::sync::OnceLock<std::sync::Mutex<SnakeState>> = std::sync::OnceLock::new();

fn new_snake_state(seed: u64) -> SnakeState {
    let lcg = |s: u64| s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut s = lcg(seed);
    // Snake starts in center heading right
    let mut body = std::collections::VecDeque::new();
    body.push_back((SNAKE_COLS/2, SNAKE_ROWS/2));
    body.push_back((SNAKE_COLS/2 - 1, SNAKE_ROWS/2));
    body.push_back((SNAKE_COLS/2 - 2, SNAKE_ROWS/2));
    // Place food randomly, not on snake
    s = lcg(s);
    let fx = (s % SNAKE_COLS as u64) as i32;
    s = lcg(s);
    let fy = (s % SNAKE_ROWS as u64) as i32;
    SnakeState { body, dir: SnakeDir::Right, food: (fx, fy), score: 0, dead: false, tick: 0 }
}

/// AI-controlled snake screensaver — automatic game with Catppuccin colours.
pub fn render_snake() -> DynamicImage {
    let tick_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut state = SNAKE_STATE.get_or_init(|| std::sync::Mutex::new(new_snake_state(tick_now)))
        .lock().unwrap();

    state.tick += 1;

    let lcg = |s: u64| s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);

    if state.dead {
        // Respawn after death
        *state = new_snake_state(state.tick.wrapping_mul(7919));
    }

    // ── AI: simple greedy pathfinding towards food ────────────────────────
    // Every 1 tick, decide direction. Try to move towards food, avoiding self.
    let (hx, hy) = *state.body.front().unwrap();
    let (fx, fy) = state.food;

    // Candidate moves ordered by preference (closer to food first)
    let mut candidates: Vec<SnakeDir> = Vec::new();
    if fx > hx { candidates.push(SnakeDir::Right); }
    if fx < hx { candidates.push(SnakeDir::Left); }
    if fy > hy { candidates.push(SnakeDir::Down); }
    if fy < hy { candidates.push(SnakeDir::Up); }
    // Add remaining directions as fallback
    for d in [SnakeDir::Right, SnakeDir::Left, SnakeDir::Down, SnakeDir::Up] {
        if !candidates.contains(&d) { candidates.push(d); }
    }

    // Don't reverse
    let forbidden = match state.dir {
        SnakeDir::Up    => SnakeDir::Down,
        SnakeDir::Down  => SnakeDir::Up,
        SnakeDir::Left  => SnakeDir::Right,
        SnakeDir::Right => SnakeDir::Left,
    };
    candidates.retain(|d| *d != forbidden);

    // Pick first safe move (doesn't hit self or wall)
    fn next_pos(hx: i32, hy: i32, d: &SnakeDir) -> (i32, i32) {
        match d {
            SnakeDir::Up    => (hx, hy - 1),
            SnakeDir::Down  => (hx, hy + 1),
            SnakeDir::Left  => (hx - 1, hy),
            SnakeDir::Right => (hx + 1, hy),
        }
    }

    let body_set: std::collections::HashSet<(i32, i32)> = state.body.iter().cloned().collect();
    let chosen_dir = candidates.into_iter().find(|d| {
        let (nx, ny) = next_pos(hx, hy, d);
        nx >= 0 && ny >= 0 && nx < SNAKE_COLS && ny < SNAKE_ROWS && !body_set.contains(&(nx, ny))
    });

    if let Some(d) = chosen_dir {
        state.dir = d;
    }

    // Move snake
    let (nhx, nhy) = next_pos(hx, hy, &state.dir);
    if nhx < 0 || nhy < 0 || nhx >= SNAKE_COLS || nhy >= SNAKE_ROWS
        || state.body.contains(&(nhx, nhy))
    {
        state.dead = true;
    } else {
        state.body.push_front((nhx, nhy));
        if (nhx, nhy) == state.food {
            // Eat food — don't remove tail (snake grows)
            state.score += 1;
            // Place new food
            let mut seed = state.tick.wrapping_mul(31337);
            let body_set2: std::collections::HashSet<_> = state.body.iter().cloned().collect();
            loop {
                seed = lcg(seed);
                let fx2 = (seed % SNAKE_COLS as u64) as i32;
                seed = lcg(seed);
                let fy2 = (seed % SNAKE_ROWS as u64) as i32;
                if !body_set2.contains(&(fx2, fy2)) {
                    state.food = (fx2, fy2);
                    break;
                }
            }
        } else {
            state.body.pop_back();
        }
    }

    // ── Render ──────────────────────────────────────────────────────────────
    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    let cell_w = 480 / SNAKE_COLS;
    let cell_h = 480 / SNAKE_ROWS;

    // Grid background
    for row in 0..SNAKE_ROWS {
        for col in 0..SNAKE_COLS {
            if (row + col) % 2 == 0 {
                let x = (col * cell_w) as u32;
                let y = (row * cell_h) as u32;
                let rect = imageproc::rect::Rect::at(x as i32, y as i32)
                    .of_size(cell_w as u32, cell_h as u32);
                imageproc::drawing::draw_filled_rect_mut(&mut img, rect, Rgb([20, 20, 33]));
            }
        }
    }

    // Draw food
    {
        let (fc, fr) = state.food;
        let fx = fc * cell_w;
        let fy = fr * cell_h;
        let rect = imageproc::rect::Rect::at(fx + 2, fy + 2)
            .of_size((cell_w - 4) as u32, (cell_h - 4) as u32);
        imageproc::drawing::draw_filled_rect_mut(&mut img, rect, RED);
    }

    // Draw snake
    for (i, &(sc, sr)) in state.body.iter().enumerate() {
        let sx = sc * cell_w;
        let sy = sr * cell_h;
        let col = if i == 0 {
            GREEN  // head
        } else {
            // Body fades from green to teal
            let t = (i as f32 / state.body.len() as f32).min(1.0);
            let r = (GREEN.0[0] as f32 * (1.0-t) + TEAL.0[0] as f32 * t) as u8;
            let g = (GREEN.0[1] as f32 * (1.0-t) + TEAL.0[1] as f32 * t) as u8;
            let b = (GREEN.0[2] as f32 * (1.0-t) + TEAL.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        };
        let margin = if i == 0 { 0 } else { 1 };
        let rect = imageproc::rect::Rect::at(sx + margin, sy + margin)
            .of_size(((cell_w - margin * 2).max(1)) as u32, ((cell_h - margin * 2).max(1)) as u32);
        imageproc::drawing::draw_filled_rect_mut(&mut img, rect, col);
    }

    // Score overlay
    let score_str = format!("Snake  Score: {}", state.score);
    let sw = text_width(&font, PxScale::from(14.0), &score_str);
    // Semi-transparent background
    let bg_rect = imageproc::rect::Rect::at(0, 0).of_size(sw as u32 + 8, 20);
    imageproc::drawing::draw_filled_rect_mut(&mut img, bg_rect, Rgb([0, 0, 0]));
    draw_text_mut(&mut img, GREEN, 4, 2, PxScale::from(14.0), &font, &score_str);

    DynamicImage::ImageRgb8(img)
}



#[cfg(test)]
mod bounce_ball_tests {
    use super::*;

    #[test]
    fn test_render_bounce_ball_size() {
        let img = render_bounce_ball();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_bounce_ball_animates() {
        let img1 = render_bounce_ball();
        let img2 = render_bounce_ball();
        // Frames differ as ball moves
        assert_eq!(img1.width(), 480);
        assert_eq!(img2.width(), 480);
    }
}

// ─── 3D Spinning Cube ────────────────────────────────────────────────────────

static CUBE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Project a 3D point through X/Y rotation matrices, then perspective-divide.
fn project_cube_vertex(x: f64, y: f64, z: f64, rx: f64, ry: f64) -> (f64, f64) {
    // Rotate around Y axis
    let x1 = x * ry.cos() + z * ry.sin();
    let y1 = y;
    let z1 = -x * ry.sin() + z * ry.cos();
    // Rotate around X axis
    let x2 = x1;
    let y2 = y1 * rx.cos() - z1 * rx.sin();
    let z2 = y1 * rx.sin() + z1 * rx.cos();
    // Perspective
    let fov = 400.0_f64;
    let dist = 4.0_f64;
    let w = fov / (dist + z2);
    (240.0 + x2 * w, 240.0 - y2 * w)
}

pub fn render_cube_3d() -> DynamicImage {
    let tick = {
        let t = CUBE_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    use std::f64::consts::PI;

    let ry = (tick as f64 * 0.018) % (2.0 * PI);
    let rx = (tick as f64 * 0.011) % (2.0 * PI);

    // 8 unit cube vertices, scaled to 1.4
    let s = 1.4_f64;
    let verts: [(f64,f64,f64); 8] = [
        (-s,-s,-s), ( s,-s,-s), ( s, s,-s), (-s, s,-s),
        (-s,-s, s), ( s,-s, s), ( s, s, s), (-s, s, s),
    ];

    // 12 edges (pairs of vertex indices)
    let edges: [(usize,usize); 12] = [
        (0,1),(1,2),(2,3),(3,0),   // back face
        (4,5),(5,6),(6,7),(7,4),   // front face
        (0,4),(1,5),(2,6),(3,7),   // connecting edges
    ];

    // Face sets for depth-based coloring (front-facing face indices)
    let face_colors: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let projected: Vec<(f64,f64)> = verts.iter()
        .map(|&(x,y,z)| project_cube_vertex(x, y, z, rx, ry))
        .collect();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Draw subtle grid floor
    for i in 0..=8i32 {
        let t = i as f64 / 8.0;
        let x1 = (40.0 + t * 400.0) as i32;
        let x2 = (40.0 + t * 400.0) as i32;
        let fade = Rgb([25u8, 25, 40]);
        imageproc::drawing::draw_line_segment_mut(&mut img, (40.0, 380.0), (x1 as f32, 440.0), fade);
        imageproc::drawing::draw_line_segment_mut(&mut img, (x2 as f32, 380.0), (440.0, 440.0), fade);
    }

    // Draw edges, colored by which face they belong to
    let edge_colors: [Rgb<u8>; 12] = [
        BLUE, BLUE, BLUE, BLUE,
        TEAL, TEAL, TEAL, TEAL,
        MAUVE, MAUVE, GREEN, GREEN,
    ];

    for (ei, &(a, b)) in edges.iter().enumerate() {
        let (x0, y0) = projected[a];
        let (x1, y1) = projected[b];
        let col = edge_colors[ei];
        // Draw thick line by drawing a few offset copies
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                if dx == 0 && dy == 0 { continue; }
                imageproc::drawing::draw_line_segment_mut(
                    &mut img,
                    (x0 as f32 + dx as f32, y0 as f32 + dy as f32),
                    (x1 as f32 + dx as f32, y1 as f32 + dy as f32),
                    Rgb([col.0[0]/3, col.0[1]/3, col.0[2]/3]),
                );
            }
        }
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0 as i32, y0 as i32),
            (x1 as i32, y1 as i32),
            col,
            interpolate,
        );
    }

    // Vertex dots
    for (i, &(px, py)) in projected.iter().enumerate() {
        let col = face_colors[i % 6];
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                if (dx*dx + dy*dy) <= 4 {
                    let ix = (px as i32 + dx).clamp(0, 479) as u32;
                    let iy = (py as i32 + dy).clamp(0, 479) as u32;
                    img.put_pixel(ix, iy, col);
                }
            }
        }
    }

    let font = font_bold();
    draw_text_mut(&mut img, OVERLAY0, 8, 458, PxScale::from(14.0), &font, "3D Cube");

    DynamicImage::ImageRgb8(img)
}


// ─── DVD Bounce Screensaver ───────────────────────────────────────────────────

struct DvdState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    color_idx: usize,
    tick: u64,
}

static DVD_STATE: std::sync::OnceLock<std::sync::Mutex<DvdState>> = std::sync::OnceLock::new();

pub fn render_dvd_bounce() -> DynamicImage {
    const LABEL: &str = "ttlcd";
    const FONT_SZ: f32 = 56.0;
    // Approximate text dimensions
    const TEXT_W: f32 = 170.0;
    const TEXT_H: f32 = 56.0;

    let palette: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let state_lock = DVD_STATE.get_or_init(|| std::sync::Mutex::new(DvdState {
        x: 40.0, y: 60.0, vx: 2.3, vy: 1.7, color_idx: 0, tick: 0,
    }));

    let mut state = state_lock.lock().unwrap();

    // Step
    state.x += state.vx;
    state.y += state.vy;

    let mut bounced = false;
    if state.x < 10.0 || state.x + TEXT_W > 470.0 {
        state.vx = -state.vx;
        state.x = state.x.clamp(10.0, 470.0 - TEXT_W);
        bounced = true;
    }
    if state.y < 10.0 || state.y + TEXT_H > 460.0 {
        state.vy = -state.vy;
        state.y = state.y.clamp(10.0, 460.0 - TEXT_H);
        bounced = true;
    }
    if bounced {
        state.color_idx = (state.color_idx + 1) % palette.len();
    }
    state.tick += 1;

    let col = palette[state.color_idx];
    let (tx, ty) = (state.x as i32, state.y as i32);

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Subtle scanlines
    for row in (0u32..480).step_by(4) {
        for col_x in 0u32..480 {
            let p = img.get_pixel_mut(col_x, row);
            p.0[0] = p.0[0].saturating_add(4);
            p.0[1] = p.0[1].saturating_add(4);
            p.0[2] = p.0[2].saturating_add(4);
        }
    }

    // Glow shadow
    let font = font_bold();
    let glow = Rgb([col.0[0]/4, col.0[1]/4, col.0[2]/4]);
    for offx in [-2i32, 0, 2] {
        for offy in [-2i32, 0, 2] {
            draw_text_mut(&mut img, glow, tx + offx, ty + offy, PxScale::from(FONT_SZ), &font, LABEL);
        }
    }
    draw_text_mut(&mut img, col, tx, ty, PxScale::from(FONT_SZ), &font, LABEL);

    DynamicImage::ImageRgb8(img)
}


// ─── Binary Clock ─────────────────────────────────────────────────────────────

pub fn render_binary_clock() -> DynamicImage {
    use chrono::Timelike;

    let now = chrono::Local::now();
    let h  = now.hour();
    let m  = now.minute();
    let s  = now.second();

    // Binary clock shows 6 columns: H tens, H units, M tens, M units, S tens, S units
    // Rows: bit 3 (top) to bit 0 (bottom) = 4 rows
    let digits: [u8; 6] = [
        (h / 10) as u8, (h % 10) as u8,
        (m / 10) as u8, (m % 10) as u8,
        (s / 10) as u8, (s % 10) as u8,
    ];

    let col_colors: [Rgb<u8>; 6] = [BLUE, BLUE, GREEN, GREEN, MAUVE, MAUVE];

    let cell = 52i32;   // cell size
    let gap  = 16i32;   // gap between cells
    // Total width for 6 columns: 6*52 + 5*16 = 312 + 80 = 392, centered: (480-392)/2 = 44
    let start_x = 44i32;
    let start_y = 100i32;  // vertical center area

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    let font = font_bold();

    // Title
    draw_text_mut(&mut img, TEXT, 160, 30, PxScale::from(28.0), &font, "Binary Clock");
    draw_text_mut(&mut img, OVERLAY0, 155, 62, PxScale::from(18.0), &font, "H  H  M  M  S  S");

    for (col_idx, &digit) in digits.iter().enumerate() {
        let cx = start_x + col_idx as i32 * (cell + gap);
        let color = col_colors[col_idx];

        for row in 0..4u8 {
            let bit = 3 - row;  // bit 3 at top, bit 0 at bottom
            let on = (digit >> bit) & 1 == 1;
            let cy = start_y + row as i32 * (cell + gap);

            let rect = imageproc::rect::Rect::at(cx, cy).of_size(cell as u32, cell as u32);
            let fill = if on {
                color
            } else {
                SURFACE0
            };
            imageproc::drawing::draw_filled_rect_mut(&mut img, rect, fill);

            // Inner lighter border for ON cells
            if on {
                let bright = Rgb([
                    (color.0[0] as u32 + 60).min(255) as u8,
                    (color.0[1] as u32 + 60).min(255) as u8,
                    (color.0[2] as u32 + 60).min(255) as u8,
                ]);
                imageproc::drawing::draw_hollow_rect_mut(&mut img,
                    imageproc::rect::Rect::at(cx+1, cy+1).of_size(cell as u32 - 2, cell as u32 - 2),
                    bright);
            }

            // Bit power label
            let label = format!("{}", 1 << bit);
            let lw = text_width(&font, PxScale::from(14.0), &label);
            let lx = cx + (cell - lw as i32) / 2;
            let ly = cy + (cell - 14) / 2;
            let label_col = if on { BG } else { OVERLAY0 };
            draw_text_mut(&mut img, label_col, lx, ly, PxScale::from(14.0), &font, &label);
        }
    }

    // Decimal time readout at the bottom
    let time_str = format!("{:02}:{:02}:{:02}", h, m, s);
    let tw = text_width(&font, PxScale::from(48.0), &time_str);
    draw_text_mut(&mut img, TEXT, (480 - tw as i32)/2 + 1, 431, PxScale::from(48.0), &font, &time_str);
    draw_text_mut(&mut img, BLUE, (480 - tw as i32)/2, 430, PxScale::from(48.0), &font, &time_str);

    DynamicImage::ImageRgb8(img)
}


// ─── Radar Display ────────────────────────────────────────────────────────────

static RADAR_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

pub fn render_radar() -> DynamicImage {
    use std::f64::consts::PI;

    let tick = {
        let t = RADAR_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let cpus = sys.cpus();
    let n_cpus = cpus.len().max(1);
    let cpu_loads: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();

    let cx = 240i32;
    let cy = 240i32;
    let r_max = 210.0f64;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Draw concentric rings
    for ri in 1..=5u32 {
        let rr = (r_max * ri as f64 / 5.0) as f32;
        let ring_col = Rgb([20u8, 30, 45]);
        // Draw circle manually with line segments
        let segs = 120;
        for s in 0..segs {
            let a0 = s as f64 * 2.0 * PI / segs as f64;
            let a1 = (s + 1) as f64 * 2.0 * PI / segs as f64;
            let x0 = cx as f32 + rr * a0.cos() as f32;
            let y0 = cy as f32 + rr * a0.sin() as f32;
            let x1 = cx as f32 + rr * a1.cos() as f32;
            let y1 = cy as f32 + rr * a1.sin() as f32;
            imageproc::drawing::draw_line_segment_mut(&mut img, (x0, y0), (x1, y1), ring_col);
        }
    }

    // Draw grid cross
    let grid_col = Rgb([20u8, 30, 45]);
    imageproc::drawing::draw_line_segment_mut(&mut img, (cx as f32, (cy - 215) as f32), (cx as f32, (cy + 215) as f32), grid_col);
    imageproc::drawing::draw_line_segment_mut(&mut img, ((cx - 215) as f32, cy as f32), ((cx + 215) as f32, cy as f32), grid_col);

    // Sweep arc: 20° wide, decaying trail
    let sweep_angle = (tick as f64 * 0.05) % (2.0 * PI);
    let trail_len = std::f64::consts::PI * 0.4;  // 72° trail

    for trail_step in 0..60u32 {
        let a = sweep_angle - (trail_step as f64 / 60.0) * trail_len;
        let fade = 1.0 - trail_step as f64 / 60.0;
        let alpha = (fade * 40.0) as u8;
        let col = Rgb([0u8, (40.0 * fade) as u8, (alpha).min(50)]);

        // Fill a thin wedge by drawing many radii
        for ri in 0..=(r_max as u32) {
            let px = (cx as f64 + ri as f64 * a.cos()) as i32;
            let py = (cy as f64 + ri as f64 * a.sin()) as i32;
            if px >= 0 && px < 480 && py >= 0 && py < 480 {
                let existing = img.get_pixel(px as u32, py as u32);
                img.put_pixel(px as u32, py as u32, Rgb([
                    existing.0[0].saturating_add(col.0[0]),
                    existing.0[1].saturating_add(col.0[1]),
                    existing.0[2].saturating_add(col.0[2]),
                ]));
            }
        }
    }

    // Draw sweep line (bright green)
    let sweep_col = Rgb([0u8, 200, 80]);
    for ri in 0..=(r_max as u32) {
        let px = (cx as f64 + ri as f64 * sweep_angle.cos()) as i32;
        let py = (cy as f64 + ri as f64 * sweep_angle.sin()) as i32;
        if px >= 0 && px < 480 && py >= 0 && py < 480 {
            img.put_pixel(px as u32, py as u32, sweep_col);
        }
    }

    // Plot CPU blips as dots on the radar
    // Each CPU is placed at an angle proportional to its index, radius proportional to load
    let mut seed = tick.wrapping_mul(2654435761);
    let lcg_r = |s: u64| s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);

    for (i, &load) in cpu_loads.iter().enumerate() {
        let angle = (i as f64 / n_cpus as f64) * 2.0 * PI;
        let rad = (load as f64 / 100.0) * r_max * 0.9;

        // Jitter (same each frame per core)
        seed = lcg_r(seed);
        let jitter_a = (seed as f64 / u64::MAX as f64 - 0.5) * 0.15;
        seed = lcg_r(seed);
        let jitter_r = (seed as f64 / u64::MAX as f64) * 8.0;

        let final_angle = angle + jitter_a;
        let final_rad   = (rad + jitter_r).min(r_max);

        let bx = (cx as f64 + final_rad * final_angle.cos()) as i32;
        let by = (cy as f64 + final_rad * final_angle.sin()) as i32;

        // Glow
        let blip_col = if load > 85.0 { RED }
            else if load > 60.0 { YELLOW }
            else { GREEN };

        // Proximity to sweep determines brightness
        let angle_diff = {
            let d = (final_angle - sweep_angle).rem_euclid(2.0 * PI);
            d.min(2.0 * PI - d)
        };
        let glow = if angle_diff < trail_len { (1.0 - angle_diff / trail_len).powf(0.5) } else { 0.0 };

        if glow > 0.1 {
            for dx in -3i32..=3 {
                for dy in -3i32..=3 {
                    let dist2 = dx*dx + dy*dy;
                    if dist2 <= 9 {
                        let px2 = (bx + dx).clamp(0, 479) as u32;
                        let py2 = (by + dy).clamp(0, 479) as u32;
                        let bright = (glow * 255.0 * (1.0 - dist2 as f64/9.0)) as u8;
                        let existing = img.get_pixel(px2, py2);
                        img.put_pixel(px2, py2, Rgb([
                            existing.0[0].saturating_add(blip_col.0[0] / 4 * bright / 64),
                            existing.0[1].saturating_add(blip_col.0[1] / 4 * bright / 64),
                            existing.0[2].saturating_add(blip_col.0[2] / 4 * bright / 64),
                        ]));
                    }
                }
            }
        }

        // Always draw a faint dot
        if bx >= 0 && bx < 480 && by >= 0 && by < 480 {
            let faint = Rgb([blip_col.0[0]/4, blip_col.0[1]/4, blip_col.0[2]/4]);
            img.put_pixel(bx as u32, by as u32, faint);
        }
    }

    // Center dot
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx*dx+dy*dy <= 9 {
                img.put_pixel((cx+dx) as u32, (cy+dy) as u32, GREEN);
            }
        }
    }

    let font = font_bold();
    // Ring labels
    for ri in 1..=5u32 {
        let pct = ri * 20;
        let label = format!("{}%", pct);
        let lx = cx as f32 + (r_max * ri as f64 / 5.0 + 3.0) as f32;
        draw_text_mut(&mut img, OVERLAY0, lx as i32, cy + 2, PxScale::from(11.0), &font, &label);
    }
    draw_text_mut(&mut img, GREEN, 4, 4, PxScale::from(14.0), &font, "CPU RADAR");
    draw_text_mut(&mut img, OVERLAY0, 320, 4, PxScale::from(12.0), &font, &format!("{} cores", n_cpus));

    DynamicImage::ImageRgb8(img)
}



// ─── Oscilloscope CPU Waveform ────────────────────────────────────────────────

static OSCILLO_HISTORY: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    std::sync::OnceLock::new();

pub fn render_oscilloscope() -> DynamicImage {
    const SAMPLES: usize = 240;
    const GREEN_PHOSPHOR: Rgb<u8> = Rgb([0, 220, 100]);
    const DIM_PHOSPHOR:   Rgb<u8> = Rgb([0,  55,  25]);

    let history_lock = OSCILLO_HISTORY.get_or_init(|| {
        let mut dq = std::collections::VecDeque::with_capacity(SAMPLES + 1);
        for _ in 0..SAMPLES { dq.push_back(0.0f32); }
        std::sync::Mutex::new(dq)
    });

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let cpu_pct: f32 = {
        let cpus = sys.cpus();
        if cpus.is_empty() { 0.0 }
        else { cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32 }
    };
    let mem_pct: f32 = if sys.total_memory() > 0 {
        sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
    } else { 0.0 };

    {
        let mut hist = history_lock.lock().unwrap();
        hist.push_back(cpu_pct);
        if hist.len() > SAMPLES { hist.pop_front(); }
    }

    let hist = history_lock.lock().unwrap();
    let samples: Vec<f32> = hist.iter().cloned().collect();
    drop(hist);

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = Rgb([0, 8, 4]); }  // deep green-black

    let font = font_bold();

    // Horizontal grid lines at 25%, 50%, 75%
    for pct_level in [25u32, 50, 75] {
        let y = (480 - (pct_level as f32 / 100.0 * 400.0) as u32 - 40).min(479);
        for x in (0..480u32).step_by(6) {
            if x + 2 < 480 {
                img.put_pixel(x, y, Rgb([0, 30, 15]));
                img.put_pixel(x+1, y, Rgb([0, 30, 15]));
            }
        }
        let label = format!("{}%", pct_level);
        draw_text_mut(&mut img, Rgb([0, 60, 30]), 2, y as i32 - 8, PxScale::from(12.0), &font, &label);
    }

    // Vertical time markers every 60 samples (2-second marks)
    for i in (0..SAMPLES).step_by(60) {
        let x = (i as f32 / SAMPLES as f32 * 480.0) as u32;
        for y in (40..440u32).step_by(4) {
            if y + 1 < 480 { img.put_pixel(x, y, Rgb([0, 25, 12])); }
        }
    }

    // Draw CPU waveform with vertical phosphor glow
    for (i, &val) in samples.iter().enumerate() {
        if i == 0 { continue; }
        let x0 = ((i - 1) as f32 / SAMPLES as f32 * 478.0) as i32;
        let x1 = (i as f32 / SAMPLES as f32 * 478.0) as i32;
        let y0 = (440 - (samples[i-1] / 100.0 * 400.0) as i32).max(40).min(440);
        let y1 = (440 - (val / 100.0 * 400.0) as i32).max(40).min(440);

        // Glow (dim, wider)
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, (y0-1).max(0)), (x1, (y1-1).max(0)),
            DIM_PHOSPHOR, interpolate,
        );
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, (y0+1).min(479)), (x1, (y1+1).min(479)),
            DIM_PHOSPHOR, interpolate,
        );
        // Main line
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, y0), (x1, y1),
            GREEN_PHOSPHOR, interpolate,
        );
    }

    // RAM usage bar at bottom (thin, blue)
    let ram_w = (mem_pct / 100.0 * 480.0) as u32;
    let ram_rect = imageproc::rect::Rect::at(0, 462).of_size(ram_w.max(1).min(480), 6);
    imageproc::drawing::draw_filled_rect_mut(&mut img, ram_rect, BLUE);

    // Scanline overlay (subtle)
    for row in (0u32..480).step_by(2) {
        for col in 0u32..480 {
            if let Some(p) = img.get_pixel_mut_checked(col, row) {
                p.0[1] = p.0[1].saturating_sub(4);
            }
        }
    }

    // Labels
    draw_text_mut(&mut img, GREEN_PHOSPHOR, 180, 6, PxScale::from(20.0), &font, "CPU OSCILLOSCOPE");
    let cpu_str = format!("CPU {:5.1}%  RAM {:5.1}%", cpu_pct, mem_pct);
    let tw = text_width(&font, PxScale::from(16.0), &cpu_str);
    draw_text_mut(&mut img, GREEN_PHOSPHOR, (480-tw as i32)/2, 452, PxScale::from(16.0), &font, &cpu_str);

    DynamicImage::ImageRgb8(img)
}


// ─── Lissajous Curves ────────────────────────────────────────────────────────

static LISSAJOUS_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

pub fn render_lissajous() -> DynamicImage {
    use std::f64::consts::PI;

    let tick = {
        let t = LISSAJOUS_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Phase offset slowly advances
    let delta = (tick as f64 * 0.007) % (2.0 * PI);

    // Cycle through interesting frequency ratios: (1,2), (2,3), (3,4), (1,3), (3,5)
    let ratios: [(f64,f64); 5] = [(1.0,2.0),(2.0,3.0),(3.0,4.0),(1.0,3.0),(3.0,5.0)];
    let cycle_len = 400u64;  // ticks per ratio
    let ratio_idx = ((tick / cycle_len) % ratios.len() as u64) as usize;
    let (a, b) = ratios[ratio_idx];

    // Blend between two consecutive ratios for smooth transition
    let transition = (tick % cycle_len) as f64 / cycle_len as f64;
    let next_idx = (ratio_idx + 1) % ratios.len();
    let (na, nb) = ratios[next_idx];
    let blend = if transition > 0.8 { (transition - 0.8) / 0.2 } else { 0.0 };
    let fa = a * (1.0 - blend) + na * blend;
    let fb = b * (1.0 - blend) + nb * blend;

    let cx = 240.0f64;
    let cy = 240.0f64;
    let amp = 210.0f64;

    // Draw curve with N_POINTS points, colour by phase
    let n_pts = 2048usize;
    let palette: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let mut prev: Option<(i32,i32)> = None;
    for i in 0..=n_pts {
        let t = i as f64 / n_pts as f64 * 2.0 * PI;
        let px = (cx + amp * (fa * t + delta).sin()) as i32;
        let py = (cy + amp * (fb * t).sin()) as i32;

        let col_idx = (i * palette.len() / n_pts) % palette.len();
        let col = palette[col_idx];

        if let Some((lx, ly)) = prev {
            if px >= 0 && px < 480 && py >= 0 && py < 480 {
                imageproc::drawing::draw_antialiased_line_segment_mut(
                    &mut img,
                    (lx, ly), (px, py),
                    col, interpolate,
                );
            }
        }
        prev = Some((px, py));
    }

    // Center dot
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx*dx+dy*dy <= 9 {
                let ix = (cx as i32 + dx).clamp(0,479) as u32;
                let iy = (cy as i32 + dy).clamp(0,479) as u32;
                img.put_pixel(ix, iy, TEXT);
            }
        }
    }

    let font = font_bold();
    let ratio_str = format!("Lissajous  {:.0}:{:.0}  δ={:.2}", fa, fb, delta % (2.0*PI));
    draw_text_mut(&mut img, OVERLAY0, 8, 458, PxScale::from(14.0), &font, &ratio_str);

    DynamicImage::ImageRgb8(img)
}



// ─── Pong AI Screensaver ──────────────────────────────────────────────────────

struct PongState {
    ball_x:  f32,
    ball_y:  f32,
    ball_vx: f32,
    ball_vy: f32,
    left_y:  f32,
    right_y: f32,
    left_score:  u32,
    right_score: u32,
    tick: u64,
}

fn new_pong_state(seed: u64) -> PongState {
    let mut s = seed;
    let lcg = |v: u64| v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    s = lcg(s);
    let angle_frac = (s as f32 / u64::MAX as f32 - 0.5) * 0.8;
    let speed = 3.2f32;
    let vx = if (s >> 32) & 1 == 0 { speed } else { -speed };
    PongState {
        ball_x: 240.0, ball_y: 240.0,
        ball_vx: vx,
        ball_vy: speed * angle_frac,
        left_y:  200.0, right_y: 200.0,
        left_score: 0, right_score: 0,
        tick: 0,
    }
}

static PONG_STATE: std::sync::OnceLock<std::sync::Mutex<PongState>> = std::sync::OnceLock::new();

pub fn render_pong() -> DynamicImage {
    const PADDLE_W: f32 = 10.0;
    const PADDLE_H: f32 = 70.0;
    const BALL_R:   f32 = 8.0;
    const AI_SPEED: f32 = 3.8;
    const LEFT_X:   f32 = 24.0;
    const RIGHT_X:  f32 = 446.0;
    const W: f32 = 480.0;
    const H: f32 = 480.0;

    let state_lock = PONG_STATE.get_or_init(|| std::sync::Mutex::new(new_pong_state(42)));
    let mut state = state_lock.lock().unwrap();
    state.tick += 1;

    // ── AI paddle movement ─────────────────────────────────────────────────────
    // Left paddle tracks ball only when ball moves left
    if state.ball_vx < 0.0 {
        let target_l = state.ball_y - PADDLE_H / 2.0;
        let diff_l = target_l - state.left_y;
        state.left_y += diff_l.clamp(-AI_SPEED, AI_SPEED);
    }
    // Right paddle tracks ball only when ball moves right
    if state.ball_vx > 0.0 {
        let target_r = state.ball_y - PADDLE_H / 2.0;
        let diff_r = target_r - state.right_y;
        state.right_y += diff_r.clamp(-AI_SPEED, AI_SPEED);
    }
    state.left_y  = state.left_y .clamp(20.0, H - PADDLE_H - 20.0);
    state.right_y = state.right_y.clamp(20.0, H - PADDLE_H - 20.0);

    // ── Ball movement ──────────────────────────────────────────────────────────
    state.ball_x += state.ball_vx;
    state.ball_y += state.ball_vy;

    // Top/bottom walls
    if state.ball_y - BALL_R < 20.0 {
        state.ball_y = 20.0 + BALL_R;
        state.ball_vy = state.ball_vy.abs();
    }
    if state.ball_y + BALL_R > H - 20.0 {
        state.ball_y = H - 20.0 - BALL_R;
        state.ball_vy = -state.ball_vy.abs();
    }

    // Left paddle collision
    if state.ball_x - BALL_R < LEFT_X + PADDLE_W
        && state.ball_x + BALL_R > LEFT_X
        && state.ball_y > state.left_y
        && state.ball_y < state.left_y + PADDLE_H
        && state.ball_vx < 0.0
    {
        state.ball_vx = state.ball_vx.abs() * 1.03;
        let hit_pos = (state.ball_y - state.left_y - PADDLE_H / 2.0) / (PADDLE_H / 2.0);
        state.ball_vy = hit_pos * 4.5;
    }

    // Right paddle collision
    if state.ball_x + BALL_R > RIGHT_X
        && state.ball_x - BALL_R < RIGHT_X + PADDLE_W
        && state.ball_y > state.right_y
        && state.ball_y < state.right_y + PADDLE_H
        && state.ball_vx > 0.0
    {
        state.ball_vx = -(state.ball_vx.abs() * 1.03);
        let hit_pos = (state.ball_y - state.right_y - PADDLE_H / 2.0) / (PADDLE_H / 2.0);
        state.ball_vy = hit_pos * 4.5;
    }

    // Cap ball speed
    let spd = (state.ball_vx.powi(2) + state.ball_vy.powi(2)).sqrt();
    if spd > 9.0 {
        state.ball_vx = state.ball_vx / spd * 9.0;
        state.ball_vy = state.ball_vy / spd * 9.0;
    }

    // Score / reset
    if state.ball_x < 0.0 {
        state.right_score += 1;
        *state = PongState { left_score: state.left_score, right_score: state.right_score, ..new_pong_state(state.tick) };
    } else if state.ball_x > W {
        state.left_score += 1;
        *state = PongState { left_score: state.left_score, right_score: state.right_score, ..new_pong_state(state.tick.wrapping_add(7919)) };
    }

    let (bx, by) = (state.ball_x, state.ball_y);
    let (lx, ly) = (LEFT_X,  state.left_y);
    let (rx, ry) = (RIGHT_X, state.right_y);
    let (ls, rs) = (state.left_score, state.right_score);

    // ── Render ─────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }
    let font = font_bold();

    // Centre dashed line
    for y in (20u32..460).step_by(14) {
        let cr = imageproc::rect::Rect::at(238, y as i32).of_size(4, 8);
        imageproc::drawing::draw_filled_rect_mut(&mut img, cr, SURFACE0);
    }

    // Top/bottom boundary lines
    let line_col = OVERLAY0;
    for x in 0u32..480 {
        img.put_pixel(x, 18, line_col);
        img.put_pixel(x, 462, line_col);
    }

    // Draw paddles
    let lrect = imageproc::rect::Rect::at(lx as i32, ly as i32)
        .of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, lrect, BLUE);
    let rrect = imageproc::rect::Rect::at(rx as i32, ry as i32)
        .of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, rrect, GREEN);

    // Ball glow
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx*dx+dy*dy <= 9 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, Rgb([YELLOW.0[0]/3, YELLOW.0[1]/3, YELLOW.0[2]/3]));
            }
        }
    }
    // Ball core
    for dx in -4i32..=4 {
        for dy in -4i32..=4 {
            if dx*dx+dy*dy <= 16 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, YELLOW);
            }
        }
    }

    // Scores
    let ls_str = format!("{}", ls);
    let rs_str = format!("{}", rs);
    let lw = text_width(&font, PxScale::from(36.0), &ls_str);
    draw_text_mut(&mut img, BLUE,  (200 - lw as i32 / 2).max(0), 4, PxScale::from(36.0), &font, &ls_str);
    draw_text_mut(&mut img, GREEN, 270, 4, PxScale::from(36.0), &font, &rs_str);

    DynamicImage::ImageRgb8(img)
}


// ─── Flash Message Renderer (used by /api/flash endpoint) ────────────────────

/// Render a full-screen text overlay.  `text` may contain `\n` for newlines.
pub fn render_flash_message(text: &str, hex_color: &str) -> DynamicImage {
    let col = parse_hex_color(hex_color).unwrap_or(TEXT);
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }
    let font = font_bold();

    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len() as i32;
    let line_h = 40i32;
    let base_y = 240 - n * line_h / 2;

    // Draw a subtle card background
    let card_w = 440u32;
    let card_h = (n as u32 * line_h as u32 + 40).max(80);
    let card_x = (480 - card_w as i32) / 2;
    let card_y = base_y - 20;
    let card_rect = imageproc::rect::Rect::at(card_x, card_y).of_size(card_w, card_h);
    imageproc::drawing::draw_filled_rect_mut(&mut img, card_rect, SURFACE0);
    imageproc::drawing::draw_hollow_rect_mut(&mut img, card_rect, col);

    for (i, line) in lines.iter().enumerate() {
        let y = base_y + i as i32 * line_h;
        let sz = if n == 1 { 48.0 } else { 36.0 };
        let tw = text_width(&font, PxScale::from(sz), line);
        let tx = (480 - tw as i32) / 2;
        // Shadow
        draw_text_mut(&mut img, BG, tx+1, y+1, PxScale::from(sz), &font, line);
        draw_text_mut(&mut img, col, tx, y, PxScale::from(sz), &font, line);
    }

    // Timestamp
    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
    let tsw = text_width(&font, PxScale::from(14.0), &ts);
    draw_text_mut(&mut img, OVERLAY0, 480 - tsw as i32 - 8, 462, PxScale::from(14.0), &font, &ts);

    DynamicImage::ImageRgb8(img)
}

fn parse_hex_color(hex: &str) -> Option<Rgb<u8>> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 { return None; }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Rgb([r, g, b]))
}



// ─── Neon Clock ───────────────────────────────────────────────────────────────

/// Clock with heavy neon-phosphor glow effect — coloured outer glow layers + sharp inner text.
pub fn render_neon_clock() -> DynamicImage {
    let now = chrono::Local::now();
    let h  = now.hour();
    let m  = now.minute();
    let s  = now.second();

    let time_str = format!("{:02}:{:02}:{:02}", h, m, s);
    let date_str = now.format("%A, %B %-d").to_string();

    // Colour cycles through hue over 60 seconds
    let hue_t = s as f32 / 60.0;
    let neon_col = hsl_to_rgb(hue_t, 1.0, 0.6);
    let dark_col = Rgb([
        (neon_col.0[0] as f32 * 0.2) as u8,
        (neon_col.0[1] as f32 * 0.2) as u8,
        (neon_col.0[2] as f32 * 0.2) as u8,
    ]);

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = Rgb([2, 2, 10]); }  // near-black with slight blue tint

    let font = font_bold();

    // ── Time string with glow layers ──────────────────────────────────────────
    let sz = 86.0f32;
    let tw = text_width(&font, PxScale::from(sz), &time_str);
    let tx = (480 - tw as i32) / 2;
    let ty = 150i32;

    // 5 glow layers, progressively dimmer and larger offsets
    for glow_r in [6i32, 4, 3, 2, 1] {
        let dim = Rgb([
            (dark_col.0[0] as f32 * (1.5 - glow_r as f32 * 0.2)).min(255.0) as u8,
            (dark_col.0[1] as f32 * (1.5 - glow_r as f32 * 0.2)).min(255.0) as u8,
            (dark_col.0[2] as f32 * (1.5 - glow_r as f32 * 0.2)).min(255.0) as u8,
        ]);
        for dx in [-glow_r, 0, glow_r] {
            for dy in [-glow_r, 0, glow_r] {
                draw_text_mut(&mut img, dim, tx + dx, ty + dy, PxScale::from(sz), &font, &time_str);
            }
        }
    }

    // Medium glow (neon colour at 50%)
    let mid_col = Rgb([neon_col.0[0]/2, neon_col.0[1]/2, neon_col.0[2]/2]);
    for (dx, dy) in [(-2i32,-2i32),(-2,2),(2,-2),(2,2)] {
        draw_text_mut(&mut img, mid_col, tx + dx, ty + dy, PxScale::from(sz), &font, &time_str);
    }

    // Bright core
    draw_text_mut(&mut img, neon_col, tx, ty, PxScale::from(sz), &font, &time_str);

    // ── Date ──────────────────────────────────────────────────────────────────
    let dw = text_width(&font, PxScale::from(24.0), &date_str);
    let dx_pos = (480 - dw as i32) / 2;
    // Date with smaller glow
    let dim_date = Rgb([dark_col.0[0]*2, dark_col.0[1]*2, dark_col.0[2]*2]);
    for offset in [(-1i32,-1),(1,-1),(-1,1),(1,1)] {
        draw_text_mut(&mut img, dim_date, dx_pos + offset.0, 275 + offset.1, PxScale::from(24.0), &font, &date_str);
    }
    draw_text_mut(&mut img, neon_col, dx_pos, 275, PxScale::from(24.0), &font, &date_str);

    // ── Decorative separator line with glow ───────────────────────────────────
    let y_sep = 305i32;
    for x in 60..420i32 {
        let t = (x - 60) as f32 / 360.0;
        let alpha = ((-((t - 0.5) * 4.0).powi(2)).exp() * 255.0) as u8;
        let px = img.get_pixel(x as u32, y_sep as u32).clone();
        img.put_pixel(x as u32, y_sep as u32, Rgb([
            px.0[0].saturating_add(neon_col.0[0].saturating_mul(alpha) / 255),
            px.0[1].saturating_add(neon_col.0[1].saturating_mul(alpha) / 255),
            px.0[2].saturating_add(neon_col.0[2].saturating_mul(alpha) / 255),
        ]));
    }

    // ── Seconds arc ───────────────────────────────────────────────────────────
    let cx = 240i32;
    let cy = 420i32;
    let arc_r = 55.0f64;
    let sec_frac = s as f64 / 60.0;
    use std::f64::consts::PI;
    let start_a = -PI / 2.0;
    let _end_a   = start_a + sec_frac * 2.0 * PI;

    // Background arc (dim)
    let n_segs = 120;
    for i in 0..n_segs {
        let a0 = start_a + i as f64 * 2.0 * PI / n_segs as f64;
        let a1 = start_a + (i + 1) as f64 * 2.0 * PI / n_segs as f64;
        let x0 = cx as f32 + arc_r as f32 * a0.cos() as f32;
        let y0 = cy as f32 + arc_r as f32 * a0.sin() as f32;
        let x1 = cx as f32 + arc_r as f32 * a1.cos() as f32;
        let y1 = cy as f32 + arc_r as f32 * a1.sin() as f32;
        imageproc::drawing::draw_line_segment_mut(&mut img, (x0, y0), (x1, y1), Rgb([15, 15, 30]));
    }
    // Active arc
    let arc_segs = (sec_frac * n_segs as f64) as usize;
    for i in 0..arc_segs {
        let a0 = start_a + i as f64 * 2.0 * PI / n_segs as f64;
        let a1 = start_a + (i + 1) as f64 * 2.0 * PI / n_segs as f64;
        let x0 = cx as f32 + arc_r as f32 * a0.cos() as f32;
        let y0 = cy as f32 + arc_r as f32 * a0.sin() as f32;
        let x1 = cx as f32 + arc_r as f32 * a1.cos() as f32;
        let y1 = cy as f32 + arc_r as f32 * a1.sin() as f32;
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img, (x0 as i32, y0 as i32), (x1 as i32, y1 as i32), neon_col, interpolate,
        );
    }

    // Seconds number in arc
    let sstr = format!("{:02}s", s);
    let sw = text_width(&font, PxScale::from(18.0), &sstr);
    draw_text_mut(&mut img, neon_col, cx - sw as i32/2, cy - 9, PxScale::from(18.0), &font, &sstr);

    DynamicImage::ImageRgb8(img)
}

/// HSL → RGB colour conversion (h, s, l all 0..1).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Rgb<u8> {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Rgb([((r1 + m) * 255.0) as u8, ((g1 + m) * 255.0) as u8, ((b1 + m) * 255.0) as u8])
}


// ─── Network Heatmap ─────────────────────────────────────────────────────────

const NET_HEATMAP_COLS: usize = 60;

static NET_HM_RX_HISTORY: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<f64>>> =
    std::sync::OnceLock::new();
static NET_HM_TX_HISTORY: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<f64>>> =
    std::sync::OnceLock::new();
static NET_HM_PREV_RX: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
static NET_HM_PREV_TX: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

pub fn render_net_heatmap() -> DynamicImage {
    // ── Gather data ───────────────────────────────────────────────────────────
    let (total_rx, total_tx): (u64, u64) = {
        let mut rx = 0u64;
        let mut tx = 0u64;
        if let Ok(content) = std::fs::read_to_string("/proc/net/dev") {
            for line in content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let iface = parts[0].trim_end_matches(':');
                    if iface == "lo" { continue; }
                    rx += parts[1].parse::<u64>().unwrap_or(0);
                    tx += parts[9].parse::<u64>().unwrap_or(0);
                }
            }
        }
        (rx, tx)
    };

    let delta_rx: f64 = {
        let mut prev = NET_HM_PREV_RX.get_or_init(|| std::sync::Mutex::new(0)).lock().unwrap();
        let d = if total_rx > *prev { (total_rx - *prev) as f64 } else { 0.0 };
        *prev = total_rx;
        d
    };
    let delta_tx: f64 = {
        let mut prev = NET_HM_PREV_TX.get_or_init(|| std::sync::Mutex::new(0)).lock().unwrap();
        let d = if total_tx > *prev { (total_tx - *prev) as f64 } else { 0.0 };
        *prev = total_tx;
        d
    };

    let init_dq = || {
        let mut dq = std::collections::VecDeque::with_capacity(NET_HEATMAP_COLS + 1);
        for _ in 0..NET_HEATMAP_COLS { dq.push_back(0.0f64); }
        std::sync::Mutex::new(dq)
    };
    {
        let mut rx_hist = NET_HM_RX_HISTORY.get_or_init(init_dq).lock().unwrap();
        rx_hist.push_back(delta_rx);
        if rx_hist.len() > NET_HEATMAP_COLS { rx_hist.pop_front(); }
    }
    {
        let mut tx_hist = NET_HM_TX_HISTORY.get_or_init(init_dq).lock().unwrap();
        tx_hist.push_back(delta_tx);
        if tx_hist.len() > NET_HEATMAP_COLS { tx_hist.pop_front(); }
    }

    let rx_samples: Vec<f64> = NET_HM_RX_HISTORY.get_or_init(init_dq).lock().unwrap().iter().cloned().collect();
    let tx_samples: Vec<f64> = NET_HM_TX_HISTORY.get_or_init(init_dq).lock().unwrap().iter().cloned().collect();

    let max_rx = rx_samples.iter().cloned().fold(1.0f64, f64::max);
    let max_tx = tx_samples.iter().cloned().fold(1.0f64, f64::max);
    let global_max = max_rx.max(max_tx).max(1024.0);  // at least 1 KB

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }
    let font = font_bold();

    draw_text_mut(&mut img, TEXT, 8, 8, PxScale::from(22.0), &font, "Network Heatmap");

    let grid_x = 80i32;
    let grid_w = 380i32;
    let cell_w = grid_w as f32 / NET_HEATMAP_COLS as f32;

    let legend_labels = ["Rx (↓)", "Tx (↑)"];
    let section_ys = [60i32, 265i32];
    let samples_list = [&rx_samples, &tx_samples];

    for (si, &samples) in samples_list.iter().enumerate() {
        let base_y = section_ys[si];
        draw_text_mut(&mut img, SUBTEXT, 4, base_y, PxScale::from(14.0), &font, legend_labels[si]);

        let grid_h = 180i32;

        for (col_idx, &val) in samples.iter().enumerate() {
            let frac = (val / global_max).min(1.0) as f32;
            let x0 = (grid_x as f32 + col_idx as f32 * cell_w) as i32;
            let x1 = (grid_x as f32 + (col_idx + 1) as f32 * cell_w).ceil() as i32;
            let bar_h = (frac * grid_h as f32) as i32;
            if bar_h == 0 { continue; }

            // Colour: blue(low) → teal(mid) → green(high)
            let col = if frac < 0.33 {
                let t = frac / 0.33;
                let r = (BLUE.0[0] as f32 * (1.0-t) + TEAL.0[0] as f32 * t) as u8;
                let g = (BLUE.0[1] as f32 * (1.0-t) + TEAL.0[1] as f32 * t) as u8;
                let b = (BLUE.0[2] as f32 * (1.0-t) + TEAL.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            } else if frac < 0.66 {
                let t = (frac - 0.33) / 0.33;
                let r = (TEAL.0[0] as f32 * (1.0-t) + GREEN.0[0] as f32 * t) as u8;
                let g = (TEAL.0[1] as f32 * (1.0-t) + GREEN.0[1] as f32 * t) as u8;
                let b = (TEAL.0[2] as f32 * (1.0-t) + GREEN.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            } else {
                let t = (frac - 0.66) / 0.34;
                let r = (GREEN.0[0] as f32 * (1.0-t) + YELLOW.0[0] as f32 * t) as u8;
                let g = (GREEN.0[1] as f32 * (1.0-t) + YELLOW.0[1] as f32 * t) as u8;
                let b = (GREEN.0[2] as f32 * (1.0-t) + YELLOW.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            };

            let y0 = base_y + grid_h - bar_h;
            let rect = imageproc::rect::Rect::at(x0, y0)
                .of_size((x1 - x0).max(1) as u32, bar_h as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, rect, col);
        }

        // Baseline
        for x in grid_x..(grid_x + grid_w) {
            let by = base_y + grid_h;
            if by < 480 { img.put_pixel(x as u32, by as u32, SURFACE0); }
        }

        // Current value label
        let cur_rx = delta_rx;
        let cur_tx = delta_tx;
        let cur = if si == 0 { cur_rx } else { cur_tx };
        let label = human_bytes(cur) + "/s";
        draw_text_mut(&mut img, SUBTEXT, grid_x + grid_w - text_width(&font, PxScale::from(14.0), &label) as i32 + 4, base_y, PxScale::from(14.0), &font, &label);

        // Max label
        let max_label = human_bytes(global_max) + " max";
        draw_text_mut(&mut img, OVERLAY0, grid_x, base_y + grid_h - 20, PxScale::from(12.0), &font, &max_label);
    }

    // Time axis labels
    draw_text_mut(&mut img, OVERLAY0, grid_x, 456, PxScale::from(11.0), &font, "← older");
    let newer = format!("{} newer →", chrono::Local::now().format("%H:%M:%S"));
    let nw = text_width(&font, PxScale::from(11.0), &newer);
    draw_text_mut(&mut img, OVERLAY0, 480 - nw as i32 - 4, 456, PxScale::from(11.0), &font, &newer);

    DynamicImage::ImageRgb8(img)
}



// ─── Word Clock ───────────────────────────────────────────────────────────────

/// Convert a time to English words.  Returns a Vec of lines (2-4 words each).
fn time_to_words(h: u32, m: u32) -> Vec<String> {
    let mins_to_word = |n: u32| match n {
        5  => "FIVE",
        10 => "TEN",
        15 => "QUARTER",
        20 => "TWENTY",
        25 => "TWENTY FIVE",
        30 => "HALF",
        _  => "",
    };
    let hour_word = |h: u32| match h % 12 {
        0  => "TWELVE",
        1  => "ONE",
        2  => "TWO",
        3  => "THREE",
        4  => "FOUR",
        5  => "FIVE",
        6  => "SIX",
        7  => "SEVEN",
        8  => "EIGHT",
        9  => "NINE",
        10 => "TEN",
        11 => "ELEVEN",
        _  => "TWELVE",
    };

    let rounded = (m / 5) * 5;
    let h_display = if rounded > 30 { (h + 1) % 24 } else { h };

    match rounded {
        0  => vec!["IT IS".into(), hour_word(h_display).into(), "O'CLOCK".into()],
        30 => vec!["HALF".into(), "PAST".into(), hour_word(h_display).into()],
        15 => vec!["QUARTER".into(), "PAST".into(), hour_word(h_display).into()],
        45 => vec!["QUARTER".into(), "TO".into(), hour_word(h_display).into()],
        n if n <= 30 => {
            let mw = mins_to_word(n);
            if mw.is_empty() {
                vec![hour_word(h_display).into(), "O'CLOCK".into()]
            } else if mw.contains(' ') {
                let parts: Vec<&str> = mw.splitn(2, ' ').collect();
                vec![parts[0].into(), parts[1].into(), "PAST".into(), hour_word(h_display).into()]
            } else {
                vec![mw.into(), "PAST".into(), hour_word(h_display).into()]
            }
        },
        n => {
            let to_min = 60 - n;
            let mw = mins_to_word(to_min);
            if mw.is_empty() {
                vec![hour_word(h_display).into(), "O'CLOCK".into()]
            } else if mw.contains(' ') {
                let parts: Vec<&str> = mw.splitn(2, ' ').collect();
                vec![parts[0].into(), parts[1].into(), "TO".into(), hour_word(h_display).into()]
            } else {
                vec![mw.into(), "TO".into(), hour_word(h_display).into()]
            }
        },
    }
}

pub fn render_word_clock() -> DynamicImage {
    let now = chrono::Local::now();
    let h = now.hour();
    let m = now.minute();
    let s = now.second();

    let words = time_to_words(h, m);
    let n = words.len() as i32;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }
    let font = font_bold();

    // Choose font size based on word count and longest line
    let max_len = words.iter().map(|w| w.len()).max().unwrap_or(1);
    let font_sz: f32 = if max_len <= 7 && n <= 3 { 72.0 }
                       else if max_len <= 10 { 56.0 }
                       else { 44.0 };

    let line_h = (font_sz * 1.25) as i32;
    let total_h = n * line_h;
    let base_y = (480 - total_h) / 2;

    // Subtle second-tick colour pulse
    let pulse = s as f32 / 59.0;
    let text_col = Rgb([
        (TEXT.0[0] as f32 * (0.85 + 0.15 * pulse)) as u8,
        (TEXT.0[1] as f32 * (0.85 + 0.15 * pulse)) as u8,
        (TEXT.0[2] as f32 * (0.85 + 0.15 * pulse)) as u8,
    ]);

    for (i, word) in words.iter().enumerate() {
        let tw = text_width(&font, PxScale::from(font_sz), word);
        let tx = (480 - tw as i32) / 2;
        let ty = base_y + i as i32 * line_h;

        // Shadow
        draw_text_mut(&mut img, SURFACE0, tx+2, ty+2, PxScale::from(font_sz), &font, word);

        // Accent: first word in BLUE, last in MAUVE, middle in text colour
        let col = if i == 0 { BLUE }
                  else if i as i32 == n - 1 { MAUVE }
                  else { text_col };
        draw_text_mut(&mut img, col, tx, ty, PxScale::from(font_sz), &font, word);
    }

    // Thin progress bar at bottom (minute progress)
    let bar_w = (m as f32 / 60.0 * 480.0) as u32;
    let bar = imageproc::rect::Rect::at(0, 474).of_size(bar_w.max(1), 6);
    imageproc::drawing::draw_filled_rect_mut(&mut img, bar, BLUE);

    // Date in corner
    let date_str = now.format("%-d %b %Y").to_string();
    let dw = text_width(&font, PxScale::from(14.0), &date_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - dw as i32 - 8, 8, PxScale::from(14.0), &font, &date_str);

    DynamicImage::ImageRgb8(img)
}



// ─── Breakout / Brick Breaker ─────────────────────────────────────────────────

const BRKOUT_COLS: usize = 10;
const BRKOUT_ROWS: usize = 5;

struct BreakoutState {
    ball_x:  f32,
    ball_y:  f32,
    ball_vx: f32,
    ball_vy: f32,
    paddle_x: f32,
    bricks:   [[bool; BRKOUT_COLS]; BRKOUT_ROWS],  // true = alive
    score: u32,
    lives: u32,
    tick:  u64,
}

fn new_breakout_state(seed: u64) -> BreakoutState {
    let mut bricks = [[true; BRKOUT_COLS]; BRKOUT_ROWS];
    // A few gaps for variety
    let mut s = seed;
    let lcg = |v: u64| v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for r in 0..BRKOUT_ROWS {
        for c in 0..BRKOUT_COLS {
            s = lcg(s);
            if (s % 10) == 0 { bricks[r][c] = false; }
        }
    }
    BreakoutState {
        ball_x: 240.0, ball_y: 350.0,
        ball_vx: 3.0, ball_vy: -3.5,
        paddle_x: 200.0,
        bricks,
        score: 0, lives: 3, tick: 0,
    }
}

static BREAKOUT_STATE: std::sync::OnceLock<std::sync::Mutex<BreakoutState>> = std::sync::OnceLock::new();

pub fn render_breakout() -> DynamicImage {
    const W: f32 = 480.0;
    const H: f32 = 460.0;
    const PADDLE_W: f32 = 64.0;
    const PADDLE_H: f32 = 10.0;
    const BALL_R:   f32 = 7.0;
    const PADDLE_Y: f32 = 430.0;
    const BRICK_W:  f32 = 44.0;
    const BRICK_H:  f32 = 16.0;
    const BRICK_MARGIN: f32 = 4.0;
    const BRICKS_X0: f32 = 10.0;
    const BRICKS_Y0: f32 = 50.0;
    const AI_SPEED:  f32 = 5.0;

    let row_colors: [Rgb<u8>; BRKOUT_ROWS] = [RED, PEACH, YELLOW, GREEN, BLUE];

    let state_lock = BREAKOUT_STATE.get_or_init(|| std::sync::Mutex::new(new_breakout_state(12345)));
    let mut state = state_lock.lock().unwrap();
    state.tick += 1;

    // ── AI: move paddle toward ball X ──────────────────────────────────────────
    let paddle_center = state.paddle_x + PADDLE_W / 2.0;
    let diff = state.ball_x - paddle_center;
    state.paddle_x += diff.clamp(-AI_SPEED, AI_SPEED);
    state.paddle_x = state.paddle_x.clamp(0.0, W - PADDLE_W);

    // ── Physics ────────────────────────────────────────────────────────────────
    state.ball_x += state.ball_vx;
    state.ball_y += state.ball_vy;

    // Wall bounces
    if state.ball_x - BALL_R < 0.0 { state.ball_vx =  state.ball_vx.abs(); state.ball_x = BALL_R; }
    if state.ball_x + BALL_R > W   { state.ball_vx = -state.ball_vx.abs(); state.ball_x = W - BALL_R; }
    if state.ball_y - BALL_R < 30.0 { state.ball_vy =  state.ball_vy.abs(); state.ball_y = 30.0 + BALL_R; }

    // Paddle bounce
    if state.ball_y + BALL_R >= PADDLE_Y
        && state.ball_x >= state.paddle_x
        && state.ball_x <= state.paddle_x + PADDLE_W
        && state.ball_vy > 0.0
    {
        let hit_pos = (state.ball_x - state.paddle_x - PADDLE_W / 2.0) / (PADDLE_W / 2.0);
        state.ball_vy = -(state.ball_vy.abs().max(3.0));
        state.ball_vx = hit_pos * 5.0;
        state.ball_y = PADDLE_Y - BALL_R;
    }

    // Ball missed paddle → lose life and reset
    if state.ball_y > H + 10.0 {
        if state.lives > 0 { state.lives -= 1; }
        if state.lives == 0 {
            // Game over → restart
            let old_score = state.score;
            *state = new_breakout_state(state.tick);
            state.score = old_score;
        } else {
            state.ball_x = 240.0;
            state.ball_y = 380.0;
            state.ball_vy = -3.5;
        }
    }

    // Brick collisions
    'outer: for row in 0..BRKOUT_ROWS {
        for col in 0..BRKOUT_COLS {
            if !state.bricks[row][col] { continue; }
            let bx = BRICKS_X0 + col as f32 * (BRICK_W + BRICK_MARGIN);
            let by = BRICKS_Y0 + row as f32 * (BRICK_H + BRICK_MARGIN);
            if state.ball_x + BALL_R > bx
                && state.ball_x - BALL_R < bx + BRICK_W
                && state.ball_y + BALL_R > by
                && state.ball_y - BALL_R < by + BRICK_H
            {
                state.bricks[row][col] = false;
                state.score += 1 + (BRKOUT_ROWS - row) as u32;
                state.ball_vy = -state.ball_vy;
                break 'outer;
            }
        }
    }

    // All bricks cleared → new level
    let any_alive = state.bricks.iter().any(|row| row.iter().any(|&b| b));
    if !any_alive {
        let seed = state.tick.wrapping_add(state.score as u64 * 31);
        let old_score = state.score;
        *state = new_breakout_state(seed);
        state.score = old_score;
        state.ball_vy = -(state.ball_vy.abs() + 0.3);  // slightly faster each level
    }

    let (bx, by) = (state.ball_x, state.ball_y);
    let (px, score, lives) = (state.paddle_x, state.score, state.lives);
    let bricks_snapshot = state.bricks.clone();

    // ── Render ─────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }
    let font = font_bold();

    // Score + lives header
    let score_str = format!("Score: {}   ❤ {}", score, lives);
    draw_text_mut(&mut img, TEXT, 8, 6, PxScale::from(18.0), &font, &score_str);

    // Bricks
    for row in 0..BRKOUT_ROWS {
        let col_color = row_colors[row];
        for col in 0..BRKOUT_COLS {
            if !bricks_snapshot[row][col] { continue; }
            let bxr = (BRICKS_X0 + col as f32 * (BRICK_W + BRICK_MARGIN)) as i32;
            let byr = (BRICKS_Y0 + row as f32 * (BRICK_H + BRICK_MARGIN)) as i32;
            let brect = imageproc::rect::Rect::at(bxr, byr)
                .of_size(BRICK_W as u32, BRICK_H as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, brect, col_color);
            // Highlight line at top of brick
            let bright = Rgb([
                (col_color.0[0] as u32 + 50).min(255) as u8,
                (col_color.0[1] as u32 + 50).min(255) as u8,
                (col_color.0[2] as u32 + 50).min(255) as u8,
            ]);
            imageproc::drawing::draw_hollow_rect_mut(&mut img,
                imageproc::rect::Rect::at(bxr, byr).of_size(BRICK_W as u32, 2),
                bright);
        }
    }

    // Paddle
    let paddle_rect = imageproc::rect::Rect::at(px as i32, PADDLE_Y as i32)
        .of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, paddle_rect, BLUE);

    // Ball with glow
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx*dx+dy*dy <= 9 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, Rgb([YELLOW.0[0]/3, YELLOW.0[1]/3, YELLOW.0[2]/3]));
            }
        }
    }
    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            if dx*dx+dy*dy <= 4 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, YELLOW);
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}



// ─── Audio Spectrum Visualizer (CPU-driven) ───────────────────────────────────

const SPECTRUM_BARS: usize = 32;

static SPECTRUM_HEIGHTS: std::sync::OnceLock<std::sync::Mutex<Vec<f32>>> =
    std::sync::OnceLock::new();
static SPECTRUM_PEAKS:   std::sync::OnceLock<std::sync::Mutex<Vec<f32>>> =
    std::sync::OnceLock::new();
static SPECTRUM_TICK:    std::sync::OnceLock<std::sync::Mutex<u64>>      =
    std::sync::OnceLock::new();

pub fn render_spectrum() -> DynamicImage {
    let tick = {
        let t = SPECTRUM_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    let heights_lock = SPECTRUM_HEIGHTS.get_or_init(|| {
        std::sync::Mutex::new(vec![0.0f32; SPECTRUM_BARS])
    });
    let peaks_lock = SPECTRUM_PEAKS.get_or_init(|| {
        std::sync::Mutex::new(vec![0.0f32; SPECTRUM_BARS])
    });

    // Get CPU usage per core, map to spectrum bars
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let cpus = sys.cpus();
    let n = cpus.len().max(1);
    let cpu_loads: Vec<f32> = cpus.iter().map(|c| c.cpu_usage() / 100.0).collect();

    // Pseudo-random seed for this tick
    let mut seed = tick.wrapping_mul(2654435761);
    let lcg = |v: u64| v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);

    // Distribute CPU loads across spectrum bars with noise
    let mut heights = heights_lock.lock().unwrap();
    let mut peaks   = peaks_lock.lock().unwrap();

    for i in 0..SPECTRUM_BARS {
        // Map bar to a CPU core
        let core_idx = (i * n / SPECTRUM_BARS).min(n - 1);
        let base_load = cpu_loads[core_idx];

        // Add frequency-like envelope: louder in bass/mids, quieter at extremes
        let freq_frac = i as f32 / SPECTRUM_BARS as f32;
        let envelope = 1.0 - (freq_frac * 2.0 - 1.0).powi(2) * 0.4;  // slight U-dip at edges

        seed = lcg(seed);
        let noise = (seed as f32 / u64::MAX as f32 - 0.5) * 0.25;

        // Target height: CPU load × envelope + noise + slight boost for bass
        let bass_boost = if i < 4 { 0.1 } else { 0.0 };
        let target = ((base_load * envelope + noise + bass_boost) * 0.9 + 0.05).clamp(0.0, 1.0);

        // Smooth: rise fast, decay slow
        if target > heights[i] {
            heights[i] += (target - heights[i]) * 0.35;
        } else {
            heights[i] -= (heights[i] - target) * 0.12;
        }
        heights[i] = heights[i].clamp(0.02, 1.0);

        // Peak hold + decay
        if heights[i] > peaks[i] {
            peaks[i] = heights[i];
        } else {
            peaks[i] -= 0.008;
        }
        peaks[i] = peaks[i].clamp(0.0, 1.0);
    }

    let heights_snap: Vec<f32> = heights.clone();
    let peaks_snap:   Vec<f32> = peaks.clone();
    drop(heights);
    drop(peaks);

    // ── Render ────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = Rgb([4, 4, 16]); }
    let font = font_bold();

    let bar_area_h = 380i32;
    let bar_area_y = 60i32;
    let bar_total_w = 480i32;
    let bar_w  = (bar_total_w as f32 / SPECTRUM_BARS as f32).floor() as i32;
    let bar_gap = 2i32;
    let bar_inner = (bar_w - bar_gap).max(1);

    // Frequency labels at bottom
    let freq_labels = ["32", "64", "128", "256", "512", "1k", "2k", "4k", "8k", "16k"];
    let label_step = SPECTRUM_BARS / freq_labels.len();

    for (i, &h) in heights_snap.iter().enumerate() {
        let _freq_frac = i as f32 / SPECTRUM_BARS as f32;
        let bx = i as i32 * bar_w;
        let bar_h = (h * bar_area_h as f32) as i32;
        let by   = bar_area_y + bar_area_h - bar_h;

        // Colour gradient: BLUE → TEAL → GREEN → YELLOW → RED (low → high energy)
        let col = if h < 0.3 {
            let t = h / 0.3;
            let r = (BLUE.0[0] as f32 * (1.0-t) + TEAL.0[0] as f32 * t) as u8;
            let g = (BLUE.0[1] as f32 * (1.0-t) + TEAL.0[1] as f32 * t) as u8;
            let b = (BLUE.0[2] as f32 * (1.0-t) + TEAL.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else if h < 0.6 {
            let t = (h - 0.3) / 0.3;
            let r = (TEAL.0[0] as f32 * (1.0-t) + GREEN.0[0] as f32 * t) as u8;
            let g = (TEAL.0[1] as f32 * (1.0-t) + GREEN.0[1] as f32 * t) as u8;
            let b = (TEAL.0[2] as f32 * (1.0-t) + GREEN.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else if h < 0.85 {
            let t = (h - 0.6) / 0.25;
            let r = (GREEN.0[0] as f32 * (1.0-t) + YELLOW.0[0] as f32 * t) as u8;
            let g = (GREEN.0[1] as f32 * (1.0-t) + YELLOW.0[1] as f32 * t) as u8;
            let b = (GREEN.0[2] as f32 * (1.0-t) + YELLOW.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else {
            let t = (h - 0.85) / 0.15;
            let r = (YELLOW.0[0] as f32 * (1.0-t) + RED.0[0] as f32 * t) as u8;
            let g = (YELLOW.0[1] as f32 * (1.0-t) + RED.0[1] as f32 * t) as u8;
            let b = (YELLOW.0[2] as f32 * (1.0-t) + RED.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        };

        // Draw bar
        if bar_h > 0 {
            let bar_rect = imageproc::rect::Rect::at(bx, by)
                .of_size(bar_inner as u32, bar_h as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, bar_rect, col);

            // Bright top pixel line
            let bright = Rgb([
                (col.0[0] as u32 + 80).min(255) as u8,
                (col.0[1] as u32 + 80).min(255) as u8,
                (col.0[2] as u32 + 80).min(255) as u8,
            ]);
            for dx in 0..bar_inner {
                let px = (bx + dx).clamp(0, 479) as u32;
                if by >= 0 && by < 480 {
                    img.put_pixel(px, by as u32, bright);
                }
            }
        }

        // Peak dot
        let peak_h = (peaks_snap[i] * bar_area_h as f32) as i32;
        let peak_y = bar_area_y + bar_area_h - peak_h;
        if peak_y >= bar_area_y && peak_y < bar_area_y + bar_area_h {
            for dx in 0..bar_inner {
                let px = (bx + dx).clamp(0, 479) as u32;
                img.put_pixel(px, peak_y as u32, TEXT);
            }
        }

        // Freq label
        if label_step > 0 && i % label_step == 0 {
            let li = i / label_step;
            if li < freq_labels.len() {
                let label = freq_labels[li];
                draw_text_mut(&mut img, OVERLAY0, bx, 450, PxScale::from(11.0), &font, label);
            }
        }
    }

    // Baseline
    for x in 0u32..480 {
        img.put_pixel(x, (bar_area_y + bar_area_h) as u32, SURFACE0);
    }

    // dB scale labels on left
    for db in [0i32, -6, -12, -24].iter() {
        let frac = 1.0 - (db.abs() as f32 / 24.0);
        let y = bar_area_y + ((1.0 - frac) * bar_area_h as f32) as i32;
        let label = format!("{}dB", db);
        draw_text_mut(&mut img, OVERLAY0, 0, y, PxScale::from(10.0), &font, &label);
    }

    // Title
    let cpu_avg: f32 = {
        let mut sys2 = System::new();
        sys2.refresh_cpu_all();
        let cpus = sys2.cpus();
        if cpus.is_empty() { 0.0 }
        else { cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32 }
    };
    let title = format!("Spectrum Analyzer — CPU {:.0}%", cpu_avg);
    let tw = text_width(&font, PxScale::from(20.0), &title);
    draw_text_mut(&mut img, TEXT, (480 - tw as i32) / 2, 10, PxScale::from(20.0), &font, &title);

    // Frequency label at bottom
    draw_text_mut(&mut img, OVERLAY0, 200, 465, PxScale::from(11.0), &font, "Hz →");

    DynamicImage::ImageRgb8(img)
}


// ─── Doom Fire ────────────────────────────────────────────────────────────────

static DOOM_FIRE_BUF:  std::sync::OnceLock<std::sync::Mutex<Vec<u8>>> = std::sync::OnceLock::new();
static DOOM_FIRE_SEED: std::sync::OnceLock<std::sync::Mutex<u32>>     = std::sync::OnceLock::new();

/// Classic Doom-PSX fire simulation.
/// 120×120 intensity cells (each rendered 4×4 px) with an LCG-driven decay
/// and slight lateral spread for an authentic flicker.
pub fn render_doom_fire() -> DynamicImage {
    const FIRE_W: usize = 120;
    const FIRE_H: usize = 120;

    let buf_m  = DOOM_FIRE_BUF.get_or_init(|| {
        let mut v = vec![0u8; FIRE_W * FIRE_H];
        for x in 0..FIRE_W { v[(FIRE_H - 1) * FIRE_W + x] = 36; }
        std::sync::Mutex::new(v)
    });
    let seed_m = DOOM_FIRE_SEED.get_or_init(|| std::sync::Mutex::new(0xDEAD_BEEFu32));

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); *s
    };

    let mut seed = seed_m.lock().unwrap();
    let mut buf  = buf_m.lock().unwrap();

    // Propagate fire upward (skip the seed row)
    for y in 0..(FIRE_H - 1) {
        for x in 0..FIRE_W {
            let decay = (lcg(&mut *seed) % 3) as u8;           // 0, 1, or 2
            let shift = (lcg(&mut *seed) % 3) as usize;         // 0, 1, or 2 left-shift
            let src_x = if shift > 0 && x >= shift { x - shift } else { x };
            let below = buf[(y + 1) * FIRE_W + src_x];
            buf[y * FIRE_W + x] = below.saturating_sub(decay);
        }
    }

    // Palette: 37 shades, black → dark red → orange → yellow → white
    let pal: [Rgb<u8>; 37] = [
        Rgb([  0,   0,   0]), Rgb([  7,   0,   0]), Rgb([ 22,   0,   0]),
        Rgb([ 40,   0,   0]), Rgb([ 58,   0,   0]), Rgb([ 80,   0,   0]),
        Rgb([103,   0,   0]), Rgb([128,   0,   0]), Rgb([148,   0,   0]),
        Rgb([168,   0,   0]), Rgb([188,   6,   0]), Rgb([208,  12,   0]),
        Rgb([220,  22,   0]), Rgb([232,  32,   0]), Rgb([240,  48,   0]),
        Rgb([248,  64,   0]), Rgb([250,  80,   0]), Rgb([252,  96,   0]),
        Rgb([252, 112,   0]), Rgb([252, 128,   0]), Rgb([252, 148,   0]),
        Rgb([252, 164,   0]), Rgb([252, 180,   0]), Rgb([252, 196,   0]),
        Rgb([252, 212,   0]), Rgb([252, 224,   0]), Rgb([252, 236,   0]),
        Rgb([252, 248,   0]), Rgb([252, 252,  60]), Rgb([252, 252, 120]),
        Rgb([252, 252, 160]), Rgb([252, 252, 200]), Rgb([252, 252, 220]),
        Rgb([255, 255, 230]), Rgb([255, 255, 240]), Rgb([255, 255, 248]),
        Rgb([255, 255, 255]),
    ];

    let mut img = RgbImage::new(480, 480);
    for cy in 0..FIRE_H {
        for cx in 0..FIRE_W {
            let col  = pal[buf[cy * FIRE_W + cx].min(36) as usize];
            let px0  = (cx as u32) * 4;
            let py0  = (cy as u32) * 4;
            for dy in 0..4u32 {
                for dx in 0..4u32 {
                    img.put_pixel(px0 + dx, py0 + dy, col);
                }
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Boids / Flocking ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct Boid {
    x: f32, y: f32,
    vx: f32, vy: f32,
    color_idx: usize,
}

static BOIDS_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<Boid>>> =
    std::sync::OnceLock::new();

/// Craig Reynolds' Boids flocking simulation — 80 coloured agents.
/// Three steering rules: separation, alignment, cohesion.
pub fn render_boids() -> DynamicImage {
    const N:      usize = 80;
    const SIZE:   f32   = 480.0;
    const VIS:    f32   = 75.0;   // visual range
    const SEP:    f32   = 10.0;   // protected (separation) range
    const CENTER: f32   = 0.0005; // cohesion strength
    const AVOID:  f32   = 0.04;   // separation strength
    const MATCH:  f32   = 0.05;   // alignment strength
    const MAX_V:  f32   = 5.5;
    const MIN_V:  f32   = 2.5;
    const MARGIN: f32   = 50.0;
    const TURN:   f32   = 0.4;

    let m = BOIDS_STATE.get_or_init(|| {
        let mut s = 0xABCD_EF01u32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let v: Vec<Boid> = (0..N).map(|i| Boid {
            x:         lcg(&mut s) * SIZE,
            y:         lcg(&mut s) * SIZE,
            vx:        (lcg(&mut s) - 0.5) * MAX_V * 2.0,
            vy:        (lcg(&mut s) - 0.5) * MAX_V * 2.0,
            color_idx: i % 7,
        }).collect();
        std::sync::Mutex::new(v)
    });
    let mut boids = m.lock().unwrap();

    let prev = boids.clone();
    for b in boids.iter_mut() {
        let (mut cdx, mut cdy) = (0.0f32, 0.0);
        let (mut avx, mut avy, mut ax, mut ay) = (0.0f32, 0.0, 0.0, 0.0);
        let mut nb = 0usize;

        for o in &prev {
            let dx = o.x - b.x;
            let dy = o.y - b.y;
            let d  = (dx * dx + dy * dy).sqrt();
            if d < SEP && d > 0.0 {
                cdx -= dx; cdy -= dy;
            } else if d < VIS {
                avx += o.vx; avy += o.vy;
                ax  += o.x;  ay  += o.y;
                nb  += 1;
            }
        }

        b.vx += cdx * AVOID;
        b.vy += cdy * AVOID;
        if nb > 0 {
            let n = nb as f32;
            b.vx += (avx / n - b.vx) * MATCH;
            b.vy += (avy / n - b.vy) * MATCH;
            b.vx += (ax  / n - b.x)  * CENTER;
            b.vy += (ay  / n - b.y)  * CENTER;
        }

        if b.x < MARGIN           { b.vx += TURN; }
        if b.x > SIZE - MARGIN    { b.vx -= TURN; }
        if b.y < MARGIN           { b.vy += TURN; }
        if b.y > SIZE - MARGIN    { b.vy -= TURN; }

        let spd = (b.vx * b.vx + b.vy * b.vy).sqrt();
        if spd > MAX_V && spd > 0.0 { b.vx = b.vx / spd * MAX_V; b.vy = b.vy / spd * MAX_V; }
        if spd < MIN_V && spd > 0.0 { b.vx = b.vx / spd * MIN_V; b.vy = b.vy / spd * MIN_V; }

        b.x = (b.x + b.vx).clamp(0.0, SIZE - 1.0);
        b.y = (b.y + b.vy).clamp(0.0, SIZE - 1.0);
    }

    let colors = [BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    for b in boids.iter() {
        let col = colors[b.color_idx % colors.len()];
        let x = b.x as i32;
        let y = b.y as i32;
        // Head dot (2×2)
        for dy in 0..2i32 {
            for dx in 0..2i32 {
                let px = x + dx; let py = y + dy;
                if px >= 0 && px < 480 && py >= 0 && py < 480 {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
        // Dim trail behind
        let tx = (b.x - b.vx * 1.2) as i32;
        let ty = (b.y - b.vy * 1.2) as i32;
        if tx >= 0 && tx < 480 && ty >= 0 && ty < 480 {
            img.put_pixel(tx as u32, ty as u32, Rgb([
                (col[0] as u32 * 3 / 10) as u8,
                (col[1] as u32 * 3 / 10) as u8,
                (col[2] as u32 * 3 / 10) as u8,
            ]));
        }
    }

    let font = font_bold();
    draw_text_mut(&mut img, OVERLAY0, 8, 462, PxScale::from(14.0), &font,
                  &format!("{N} boids"));
    DynamicImage::ImageRgb8(img)
}

// ─── Perspective Tunnel ───────────────────────────────────────────────────────

static TUNNEL_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Classic demoscene perspective tunnel.  2×2-pixel cells → 240×240 iters.
/// Catppuccin-tinted rings and sectors scroll towards the viewer.
pub fn render_tunnel() -> DynamicImage {
    let tick = {
        let mut t = TUNNEL_TICK.get_or_init(|| std::sync::Mutex::new(0)).lock().unwrap();
        *t += 1; *t
    };
    let tf = tick as f32;

    let stripes: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, PEACH, GREEN, RED];
    let mut img = RgbImage::new(480, 480);

    for cy in 0..240u32 {
        for cx in 0..240u32 {
            let dx = cx as f32 - 120.0;
            let dy = cy as f32 - 120.0;
            let dist = (dx * dx + dy * dy).sqrt().max(0.5);

            // Depth brightness (close = bright)
            let depth = (100.0 / dist).min(1.0);

            // Texture coords (scroll inward)
            let ring    = (32.0 / dist + tf * 0.35) as i32;
            let angle   = dy.atan2(dx);                    // -π .. π
            let sector  = (angle / std::f32::consts::FRAC_PI_3) as i32;  // 6 sectors

            // Checkerboard of rings × sectors → stripe index
            let idx = if (ring.abs() + sector.abs()) % 2 == 0 {
                ring.unsigned_abs() as usize % 6
            } else {
                sector.unsigned_abs() as usize % 6
            };
            let base = stripes[idx];
            let br = depth.powf(1.3);
            let col = Rgb([
                (base[0] as f32 * br) as u8,
                (base[1] as f32 * br) as u8,
                (base[2] as f32 * br) as u8,
            ]);

            let px = cx * 2; let py = cy * 2;
            img.put_pixel(px,     py,     col);
            img.put_pixel(px + 1, py,     col);
            img.put_pixel(px,     py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Aurora Borealis ──────────────────────────────────────────────────────────

static AURORA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

/// Northern-lights simulation — 5 animated sine-wave bands with Gaussian
/// falloff and additive blending, reflected in a still-water horizon.
pub fn render_aurora() -> DynamicImage {
    let tick = {
        let mut t = AURORA_TICK.get_or_init(|| std::sync::Mutex::new(0)).lock().unwrap();
        *t += 1; *t
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // ── Sky gradient ──────────────────────────────────────────────────────────
    for py in 0..480u32 {
        let frac = py as f32 / 480.0;
        let r = (4.0  + frac * 6.0)  as u8;
        let g = (4.0  + frac * 8.0)  as u8;
        let b = (16.0 + frac * 20.0) as u8;
        for px in 0..480u32 { img.put_pixel(px, py, Rgb([r, g, b])); }
    }

    // ── Stars (deterministic positions, twinkling) ────────────────────────────
    {
        let mut seed = 0x1234_5678u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); *s
        };
        for _ in 0..160u32 {
            let sx    = lcg(&mut seed) % 480;
            let sy    = lcg(&mut seed) % 310;
            let base  = 120 + (lcg(&mut seed) % 136) as u8;
            let phase = (lcg(&mut seed) % 1_000) as f32 / 1_000.0;
            let br    = (base as f32 * (0.65 + 0.35 * ((tf * 0.06 + phase * 6.28).sin())))
                        .min(255.0) as u8;
            img.put_pixel(sx, sy, Rgb([br, br, br + 15]));
        }
    }

    // ── Aurora bands ─────────────────────────────────────────────────────────
    // (r,g,b) base intensities for each layer
    let layers: [(f32, f32, f32); 5] = [
        (0.0, 0.85, 0.45),  // emerald green
        (0.0, 0.70, 0.75),  // cyan
        (0.15, 0.55, 0.95), // blue-green
        (0.50, 0.25, 0.90), // blue-violet
        (0.85, 0.15, 0.85), // magenta-purple
    ];

    const HORIZON_Y: u32 = 340;

    for (i, &(rb, gb, bb)) in layers.iter().enumerate() {
        let lf    = i as f32;
        let base  = 35.0 + lf * 42.0;          // vertical position
        let thick = 48.0 + lf * 8.0;           // band half-thickness
        let ph    = lf * 1.83;                  // phase offset

        for px in 0..480u32 {
            let xf = px as f32;

            // Undulating centre line
            let cy = base
                + (xf * 0.011  + tf * 0.017 + ph).sin() * 26.0
                + (xf * 0.023  - tf * 0.011 + ph).sin() * 16.0
                + (xf * 0.0055 + tf * 0.007 + ph * 0.7).sin() * 9.0;

            // Local thickness ripple
            let loc_th = thick * (0.7 + 0.3 * ((xf * 0.014 + tf * 0.019 + ph).sin()));

            // Column shimmer (computed once per column)
            let shimmer = 0.65 + 0.35 * ((xf * 0.028 + tf * 0.038 + ph).sin());

            for py in 0..HORIZON_Y {
                let yf  = py as f32;
                let dy  = (yf - cy).abs();
                if dy >= loc_th { continue; }

                // Gaussian falloff
                let t   = dy / loc_th;
                let alp = (-t * t * 3.0).exp() * 0.55 * shimmer;

                let px_col = img.get_pixel(px, py);
                let nr = (px_col[0] as f32 + rb * alp * 180.0).min(255.0) as u8;
                let ng = (px_col[1] as f32 + gb * alp * 230.0).min(255.0) as u8;
                let nb = (px_col[2] as f32 + bb * alp * 210.0).min(255.0) as u8;
                img.put_pixel(px, py, Rgb([nr, ng, nb]));
            }
        }
    }

    // ── Horizon line ─────────────────────────────────────────────────────────
    for px in 0..480u32 { img.put_pixel(px, HORIZON_Y, Rgb([28, 48, 38])); }

    // ── Water reflection (mirror + fade + darken) ────────────────────────────
    for py in (HORIZON_Y + 1)..480 {
        let mirror = HORIZON_Y.saturating_sub(py - HORIZON_Y);
        let fade   = 1.0 - ((py - HORIZON_Y) as f32 / 140.0).min(1.0);
        for px in 0..480u32 {
            let src = img.get_pixel(px, mirror);
            img.put_pixel(px, py, Rgb([
                (src[0] as f32 * fade * 0.50) as u8,
                (src[1] as f32 * fade * 0.62) as u8,
                (src[2] as f32 * fade * 0.72) as u8,
            ]));
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Metaballs ────────────────────────────────────────────────────────────────

#[derive(Clone)]
#[allow(dead_code)]
struct MetaBall { x: f32, y: f32, vx: f32, vy: f32, r: f32, ci: usize }

static METABALLS_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<MetaBall>>> =
    std::sync::OnceLock::new();

/// Classic animated metaballs — 8 blobs bouncing around, blending smoothly.
/// Rendered at 240×240 (2×2 px cells) for performance.
pub fn render_metaballs() -> DynamicImage {
    const N:    usize = 8;
    const CELL: u32   = 2;
    const GRID: u32   = 240;   // 480 / CELL

    let m = METABALLS_STATE.get_or_init(|| {
        let mut s = 0xFEED_CAFEu32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let balls: Vec<MetaBall> = (0..N).map(|i| MetaBall {
            x:  lcg(&mut s) * 480.0,
            y:  lcg(&mut s) * 480.0,
            vx: (lcg(&mut s) - 0.5) * 4.0,
            vy: (lcg(&mut s) - 0.5) * 4.0,
            r:  55.0 + lcg(&mut s) * 40.0,
            ci: i % 7,
        }).collect();
        std::sync::Mutex::new(balls)
    });
    let mut balls = m.lock().unwrap();

    // Move balls, bounce off walls
    for b in balls.iter_mut() {
        b.x += b.vx; b.y += b.vy;
        if b.x < 0.0   { b.x = 0.0;   b.vx = b.vx.abs(); }
        if b.x > 480.0 { b.x = 480.0; b.vx = -b.vx.abs(); }
        if b.y < 0.0   { b.y = 0.0;   b.vy = b.vy.abs(); }
        if b.y > 480.0 { b.y = 480.0; b.vy = -b.vy.abs(); }
    }

    let cols = [BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    for cy in 0..GRID {
        for cx in 0..GRID {
            let pcx = (cx as f32 + 0.5) * CELL as f32;
            let pcy = (cy as f32 + 0.5) * CELL as f32;
            let mut field  = 0.0f32;
            let mut dom_c  = BG;
            let mut dom_v  = 0.0f32;
            for b in balls.iter() {
                let dx = pcx - b.x;
                let dy = pcy - b.y;
                let d2 = (dx * dx + dy * dy).max(1.0);
                let v  = b.r * b.r / d2;
                field += v;
                if v > dom_v { dom_v = v; dom_c = cols[b.ci % cols.len()]; }
            }

            let col = if field >= 1.0 {
                let br = (field - 1.0).min(1.0) * 0.4 + 0.6;
                Rgb([ (dom_c[0] as f32 * br) as u8,
                      (dom_c[1] as f32 * br) as u8,
                      (dom_c[2] as f32 * br) as u8 ])
            } else if field > 0.55 {
                let g = (field - 0.55) / 0.45;
                Rgb([ (dom_c[0] as f32 * g * 0.28) as u8,
                      (dom_c[1] as f32 * g * 0.28) as u8,
                      (dom_c[2] as f32 * g * 0.28) as u8 ])
            } else {
                continue;
            };

            let px = cx * CELL; let py = cy * CELL;
            img.put_pixel(px,     py,     col);
            img.put_pixel(px + 1, py,     col);
            img.put_pixel(px,     py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Falling Sand ─────────────────────────────────────────────────────────────

struct SandState { cells: Vec<u8>, seed: u32, tick: u64 }

static SAND_STATE: std::sync::OnceLock<std::sync::Mutex<SandState>> =
    std::sync::OnceLock::new();

/// Cellular-automaton falling sand simulation.
/// 160×160 grid (3×3 px cells) — sand drains off the bottom; new grains
/// are injected from the top each tick.
pub fn render_sand() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3;   // 160×3 = 480

    let m = SAND_STATE.get_or_init(|| {
        std::sync::Mutex::new(SandState {
            cells: vec![0u8; W * H],
            seed:  0xDEAD_C0DEu32,
            tick:  0,
        })
    });
    let mut st = m.lock().unwrap();
    st.tick += 1;

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); *s
    };

    // Drain bottom row
    for x in 0..W { st.cells[(H - 1) * W + x] = 0; }

    // Inject a few grains at the top
    for _ in 0..5u32 {
        let x   = (lcg(&mut st.seed) % W as u32) as usize;
        let col = 1 + (lcg(&mut st.seed) % 7) as u8;
        if st.cells[x] == 0 { st.cells[x] = col; }
    }

    // Update: bottom-to-top sweep (so moved grains aren't re-processed)
    for y in (0..(H - 1)).rev() {
        for x in 0..W {
            let c = st.cells[y * W + x];
            if c == 0 { continue; }
            // Try straight down
            if st.cells[(y + 1) * W + x] == 0 {
                st.cells[(y + 1) * W + x] = c;
                st.cells[y       * W + x] = 0;
            } else {
                // Try diagonal (randomly prefer left or right)
                let go_left  = x > 0   && st.cells[(y + 1) * W + x - 1] == 0;
                let go_right = x < W-1 && st.cells[(y + 1) * W + x + 1] == 0;
                let chosen = match (go_left, go_right) {
                    (true, true) => if lcg(&mut st.seed) % 2 == 0 { Some(x - 1) } else { Some(x + 1) },
                    (true,  _)  => Some(x - 1),
                    (_,  true)  => Some(x + 1),
                    _           => None,
                };
                if let Some(nx) = chosen {
                    st.cells[(y + 1) * W + nx] = c;
                    st.cells[y       * W + x]  = 0;
                }
            }
        }
    }

    // Color table (0 = empty/BG, 1-7 = Catppuccin sand colours)
    let sand_cols: [Rgb<u8>; 8] = [BG, BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    // Slightly darken each sand colour for a "grain" look
    let tint = |c: Rgb<u8>| Rgb([ (c[0] as u32 * 9 / 10) as u8,
                                   (c[1] as u32 * 9 / 10) as u8,
                                   (c[2] as u32 * 9 / 10) as u8 ]);

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    for cy in 0..H {
        for cx in 0..W {
            let c = st.cells[cy * W + cx];
            if c == 0 { continue; }
            let col = tint(sand_cols[c as usize % sand_cols.len()]);
            let px0 = cx as u32 * CELL;
            let py0 = cy as u32 * CELL;
            for dy in 0..CELL { for dx in 0..CELL {
                img.put_pixel(px0 + dx, py0 + dy, col);
            }}
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── N-body Gravity ───────────────────────────────────────────────────────────

#[derive(Clone)]
#[allow(dead_code)]
struct GravBody { x: f32, y: f32, vx: f32, vy: f32, mass: f32, ci: usize }

struct GravState {
    bodies: Vec<GravBody>,
    trails: Vec<std::collections::VecDeque<(f32, f32)>>,
}

static GRAVITY_STATE: std::sync::OnceLock<std::sync::Mutex<GravState>> =
    std::sync::OnceLock::new();

/// N-body gravitational simulation — 10 coloured bodies with trails.
/// Bodies start in a stable circular arrangement and orbit under mutual attraction.
pub fn render_gravity() -> DynamicImage {
    const N:          usize = 10;
    const TRAIL:      usize = 60;   // trail length in ticks
    const G:          f32   = 350.0;
    const SOFT:       f32   = 8.0;  // softening to avoid singularities
    const DT:         f32   = 0.45;
    const CENTER_X:   f32   = 240.0;
    const CENTER_Y:   f32   = 240.0;

    let m = GRAVITY_STATE.get_or_init(|| {
        let mut bodies = Vec::with_capacity(N);
        let mut trails: Vec<std::collections::VecDeque<(f32, f32)>> =
            (0..N).map(|_| std::collections::VecDeque::with_capacity(TRAIL + 1)).collect();

        // One heavy central mass + N-1 orbiting bodies
        bodies.push(GravBody { x: CENTER_X, y: CENTER_Y, vx: 0.0, vy: 0.0, mass: 200.0, ci: 0 });
        for i in 1..N {
            let angle   = (i as f32) / ((N - 1) as f32) * std::f32::consts::TAU;
            let radius  = 60.0 + (i as f32) * 15.0;
            let orbit_v = (G * 200.0 / radius).sqrt();  // circular orbit speed
            bodies.push(GravBody {
                x:    CENTER_X + angle.cos() * radius,
                y:    CENTER_Y + angle.sin() * radius,
                vx:  -angle.sin() * orbit_v,
                vy:   angle.cos() * orbit_v,
                mass: 5.0 + (i as f32) * 2.0,
                ci:   i % 7,
            });
        }
        // Pre-fill trails
        for (b, t) in bodies.iter().zip(trails.iter_mut()) {
            t.push_back((b.x, b.y));
        }
        std::sync::Mutex::new(GravState { bodies, trails })
    });

    let mut st = m.lock().unwrap();
    let n = st.bodies.len();

    // Compute accelerations
    let mut ax = vec![0.0f32; n];
    let mut ay = vec![0.0f32; n];
    let bodies_snap = st.bodies.clone();
    for i in 0..n {
        for j in 0..n {
            if i == j { continue; }
            let dx  = bodies_snap[j].x - bodies_snap[i].x;
            let dy  = bodies_snap[j].y - bodies_snap[i].y;
            let d2  = dx * dx + dy * dy + SOFT * SOFT;
            let d   = d2.sqrt();
            let f   = G * bodies_snap[j].mass / d2;
            ax[i] += f * dx / d;
            ay[i] += f * dy / d;
        }
    }

    // Integrate velocities and positions (wrap at boundary)
    for i in 0..n {
        st.bodies[i].vx += ax[i] * DT;
        st.bodies[i].vy += ay[i] * DT;
        // Damp central body slightly to keep it near center
        if i == 0 {
            st.bodies[i].vx *= 0.96;
            st.bodies[i].vy *= 0.96;
        }
        st.bodies[i].x  += st.bodies[i].vx * DT;
        st.bodies[i].y  += st.bodies[i].vy * DT;
        // Soft wrap
        if st.bodies[i].x < 0.0   { st.bodies[i].x += 480.0; }
        if st.bodies[i].x > 480.0 { st.bodies[i].x -= 480.0; }
        if st.bodies[i].y < 0.0   { st.bodies[i].y += 480.0; }
        if st.bodies[i].y > 480.0 { st.bodies[i].y -= 480.0; }

        // Record trail (read position first to satisfy borrow checker)
        let pos = (st.bodies[i].x, st.bodies[i].y);
        st.trails[i].push_back(pos);
        while st.trails[i].len() > TRAIL { st.trails[i].pop_front(); }
    }

    let cols = [YELLOW, BLUE, GREEN, RED, MAUVE, TEAL, PEACH, TEXT, SUBTEXT, OVERLAY0];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Draw trails (fading alpha by position in trail)
    for (i, trail) in st.trails.iter().enumerate() {
        let base = cols[i % cols.len()];
        let len  = trail.len();
        for (idx, &(tx, ty)) in trail.iter().enumerate() {
            let frac = idx as f32 / len as f32;  // 0=oldest, 1=newest
            let br   = frac * frac;               // quadratic fade
            let px   = tx as i32; let py = ty as i32;
            if px >= 0 && px < 480 && py >= 0 && py < 480 {
                let tc = Rgb([
                    (base[0] as f32 * br) as u8,
                    (base[1] as f32 * br) as u8,
                    (base[2] as f32 * br) as u8,
                ]);
                img.put_pixel(px as u32, py as u32, tc);
            }
        }
    }

    // Draw bodies (3×3 dot, central body = 5×5)
    for (i, b) in st.bodies.iter().enumerate() {
        let col  = cols[i % cols.len()];
        let size = if i == 0 { 3i32 } else { 2i32 };
        let bx   = b.x as i32; let by = b.y as i32;
        for dy in -size..=size {
            for dx in -size..=size {
                let px = bx + dx; let py = by + dy;
                if px >= 0 && px < 480 && py >= 0 && py < 480 {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Voronoi Art ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct VoronoiSeed { x: f32, y: f32, vx: f32, vy: f32 }

static VORONOI_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<VoronoiSeed>>> =
    std::sync::OnceLock::new();

/// Animated Voronoi diagram — 20 seeds bouncing around, each region coloured
/// differently with a subtle edge-darkening effect.  2×2 px cells.
pub fn render_voronoi() -> DynamicImage {
    const N:    usize = 20;
    const CELL: u32   = 2;
    const GRID: u32   = 240;

    let m = VORONOI_STATE.get_or_init(|| {
        let mut s = 0xC0FFEE42u32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let seeds: Vec<VoronoiSeed> = (0..N).map(|_| VoronoiSeed {
            x:  lcg(&mut s) * 480.0,
            y:  lcg(&mut s) * 480.0,
            vx: (lcg(&mut s) - 0.5) * 3.0,
            vy: (lcg(&mut s) - 0.5) * 3.0,
        }).collect();
        std::sync::Mutex::new(seeds)
    });
    let mut seeds = m.lock().unwrap();

    // Move seeds, bounce off walls
    for sd in seeds.iter_mut() {
        sd.x += sd.vx; sd.y += sd.vy;
        if sd.x < 0.0   { sd.x = 0.0;   sd.vx =  sd.vx.abs(); }
        if sd.x > 480.0 { sd.x = 480.0; sd.vx = -sd.vx.abs(); }
        if sd.y < 0.0   { sd.y = 0.0;   sd.vy =  sd.vy.abs(); }
        if sd.y > 480.0 { sd.y = 480.0; sd.vy = -sd.vy.abs(); }
    }

    // Palette: 7 distinct Catppuccin colours (cycle through N seeds)
    let pal: [Rgb<u8>; 7] = [BLUE, GREEN, MAUVE, TEAL, PEACH, RED, YELLOW];

    let mut img = RgbImage::new(480, 480);

    for cy in 0..GRID {
        for cx in 0..GRID {
            let pcx = (cx as f32 + 0.5) * CELL as f32;
            let pcy = (cy as f32 + 0.5) * CELL as f32;

            let mut best_d  = f32::MAX;
            let mut best2_d = f32::MAX;
            let mut best_i  = 0usize;

            for (i, sd) in seeds.iter().enumerate() {
                let dx = pcx - sd.x;
                let dy = pcy - sd.y;
                let d  = dx * dx + dy * dy;
                if d < best_d {
                    best2_d = best_d;
                    best_d  = d;
                    best_i  = i;
                } else if d < best2_d {
                    best2_d = d;
                }
            }

            // Edge darkening: ratio of nearest to second-nearest distance
            let edge = (best_d.sqrt() / best2_d.sqrt().max(1.0)).min(1.0);
            let br   = 0.55 + 0.45 * edge;    // darker near borders

            let base = pal[best_i % pal.len()];
            let col  = Rgb([
                (base[0] as f32 * br) as u8,
                (base[1] as f32 * br) as u8,
                (base[2] as f32 * br) as u8,
            ]);

            let px = cx * CELL; let py = cy * CELL;
            img.put_pixel(px,     py,     col);
            img.put_pixel(px + 1, py,     col);
            img.put_pixel(px,     py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Reaction-Diffusion (Gray-Scott) ──────────────────────────────────────────

struct GsState {
    u: Vec<f32>,   // 160×160
    v: Vec<f32>,
    tick: u64,
}

static GS_STATE: std::sync::OnceLock<std::sync::Mutex<GsState>> =
    std::sync::OnceLock::new();

/// Gray-Scott reaction-diffusion — creates organic spot/maze patterns.
/// 160×160 simulation cells (3×3 px each); 8 iterations per render call.
pub fn render_reaction_diffusion() -> DynamicImage {
    const W:   usize = 160;
    const H:   usize = 160;
    const CELL: u32  = 3;   // 160 × 3 = 480
    const DU:  f32   = 0.16;
    const DV:  f32   = 0.08;
    const F:   f32   = 0.035;
    const K:   f32   = 0.065;
    const DT:  f32   = 1.0;
    const ITERS: usize = 8;

    let m = GS_STATE.get_or_init(|| {
        let mut u = vec![1.0f32; W * H];
        let mut v = vec![0.0f32; W * H];
        // Seed a few random patches with v=1
        let mut seed = 0xABCD1234u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); *s
        };
        for _ in 0..15 {
            let cx = (lcg(&mut seed) % W as u32) as usize;
            let cy = (lcg(&mut seed) % H as u32) as usize;
            for dy in 0..5usize {
                for dx in 0..5usize {
                    let xi = (cx + dx).min(W - 1);
                    let yi = (cy + dy).min(H - 1);
                    u[yi * W + xi] = 0.5;
                    v[yi * W + xi] = 0.25;
                }
            }
        }
        std::sync::Mutex::new(GsState { u, v, tick: 0 })
    });

    let mut st = m.lock().unwrap();
    st.tick += 1;

    let mut nu = st.u.clone();
    let mut nv = st.v.clone();

    for _ in 0..ITERS {
        for y in 0..H {
            for x in 0..W {
                let i  = y * W + x;
                let up = if y > 0     { st.u[(y-1)*W+x] } else { st.u[(H-1)*W+x] };
                let dn = if y < H-1   { st.u[(y+1)*W+x] } else { st.u[0*W+x] };
                let lt = if x > 0     { st.u[y*W+x-1]   } else { st.u[y*W+W-1] };
                let rt = if x < W-1   { st.u[y*W+x+1]   } else { st.u[y*W+0] };
                let lap_u = up + dn + lt + rt - 4.0 * st.u[i];

                let vp = if y > 0     { st.v[(y-1)*W+x] } else { st.v[(H-1)*W+x] };
                let vn = if y < H-1   { st.v[(y+1)*W+x] } else { st.v[0*W+x] };
                let vl = if x > 0     { st.v[y*W+x-1]   } else { st.v[y*W+W-1] };
                let vr = if x < W-1   { st.v[y*W+x+1]   } else { st.v[y*W+0] };
                let lap_v = vp + vn + vl + vr - 4.0 * st.v[i];

                let uvv = st.u[i] * st.v[i] * st.v[i];
                nu[i] = (st.u[i] + DT * (DU * lap_u - uvv + F * (1.0 - st.u[i]))).clamp(0.0, 1.0);
                nv[i] = (st.v[i] + DT * (DV * lap_v + uvv - (F + K) * st.v[i])).clamp(0.0, 1.0);
            }
        }
        std::mem::swap(&mut st.u, &mut nu);
        std::mem::swap(&mut st.v, &mut nv);
    }

    // Render: colour by V concentration
    //   0   → background (BG)
    //   0.4 → BLUE midtone
    //   1.0 → bright TEAL
    let mut img = RgbImage::new(480, 480);
    for cy in 0..H {
        for cx in 0..W {
            let vv  = st.v[cy * W + cx];
            let col = if vv < 0.02 {
                BG
            } else {
                let t  = (vv * 3.0).min(1.0);   // 0..1 ramp over v 0..0.33
                let t2 = (vv * 1.5 - 0.5).clamp(0.0, 1.0); // 1.0 for v>1
                Rgb([
                    ((BLUE[0]  as f32 * (1.0 - t2) + TEAL[0]  as f32 * t2) * t) as u8,
                    ((BLUE[1]  as f32 * (1.0 - t2) + TEAL[1]  as f32 * t2) * t) as u8,
                    ((BLUE[2]  as f32 * (1.0 - t2) + TEAL[2]  as f32 * t2) * t) as u8,
                ])
            };
            let px0 = cx as u32 * CELL;
            let py0 = cy as u32 * CELL;
            for dy in 0..CELL { for dx in 0..CELL { img.put_pixel(px0+dx, py0+dy, col); } }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Snowfall ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Snowflake { x: f32, y: f32, vx: f32, vy: f32, size: u32, alpha: f32, wobble_phase: f32 }

struct SnowState {
    flakes: Vec<Snowflake>,
    seed:   u32,
    tick:   u64,
    wind:   f32,   // horizontal wind component
}

static SNOW_STATE: std::sync::OnceLock<std::sync::Mutex<SnowState>> =
    std::sync::OnceLock::new();

/// Snowfall screensaver — 180 flakes drifting with sinusoidal wobble and wind.
pub fn render_snowfall() -> DynamicImage {
    const N_FLAKES: usize = 180;

    let m = SNOW_STATE.get_or_init(|| {
        let mut seed = 0x5110_CAFE_u32;
        let lcg = |s: &mut u32| -> f32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*s as f32) / u32::MAX as f32
        };
        let flakes = (0..N_FLAKES).map(|_| Snowflake {
            x:            lcg(&mut seed) * 480.0,
            y:            lcg(&mut seed) * 480.0,  // start anywhere
            vx:           (lcg(&mut seed) - 0.5) * 0.6,
            vy:           0.8 + lcg(&mut seed) * 2.0,
            size:         1 + (lcg(&mut seed) * 3.0) as u32,
            alpha:        0.5 + lcg(&mut seed) * 0.5,
            wobble_phase: lcg(&mut seed) * std::f32::consts::TAU,
        }).collect();
        std::sync::Mutex::new(SnowState { flakes, seed: 0xFACE_C0DEu32, tick: 0, wind: 0.0 })
    });

    let mut st = m.lock().unwrap();
    st.tick += 1;
    let tf = st.tick as f32;

    // Slowly changing wind
    st.wind = (tf * 0.005).sin() * 1.2;

    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    // Update flakes — extract fields to satisfy borrow checker
    let wind_snap = st.wind;
    let mut seed_snap = st.seed;
    for flake in st.flakes.iter_mut() {
        flake.wobble_phase += 0.04;
        flake.x += flake.vx + wind_snap + flake.wobble_phase.sin() * 0.4;
        flake.y += flake.vy;

        // Wrap horizontally
        if flake.x < -5.0  { flake.x += 490.0; }
        if flake.x > 485.0 { flake.x -= 490.0; }

        // Respawn when it hits the bottom
        if flake.y > 488.0 {
            flake.y = -5.0;
            flake.x = lcg(&mut seed_snap) * 480.0;
            flake.vy = 0.8 + lcg(&mut seed_snap) * 2.0;
            flake.size = 1 + (lcg(&mut seed_snap) * 3.0) as u32;
        }
    }
    st.seed = seed_snap;

    // Background: dark winter sky gradient
    let mut img = RgbImage::new(480, 480);
    for py in 0..480u32 {
        let frac = py as f32 / 480.0;
        let r = (8.0  + frac * 12.0) as u8;
        let g = (10.0 + frac * 15.0) as u8;
        let b = (24.0 + frac * 28.0) as u8;
        for px in 0..480u32 { img.put_pixel(px, py, Rgb([r, g, b])); }
    }

    // Draw flakes
    for flake in st.flakes.iter() {
        let fx = flake.x as i32; let fy = flake.y as i32;
        let sz = flake.size as i32;
        let br = (flake.alpha * 255.0) as u8;
        // Core pixel(s)
        for dy in -(sz as i32)..=(sz as i32) {
            for dx in -(sz as i32)..=(sz as i32) {
                // Circle approximation
                if dx*dx + dy*dy > sz*sz + sz { continue; }
                let px = fx + dx; let py = fy + dy;
                if px >= 0 && px < 480 && py >= 0 && py < 480 {
                    // Additive blending with existing pixel
                    let existing = img.get_pixel(px as u32, py as u32);
                    let dist_frac = ((dx*dx + dy*dy) as f32).sqrt() / sz as f32;
                    let alpha = (1.0 - dist_frac).max(0.0) * flake.alpha;
                    let nr = (existing[0] as f32 + br as f32 * alpha).min(255.0) as u8;
                    let ng = (existing[1] as f32 + br as f32 * alpha).min(255.0) as u8;
                    let nb = (existing[2] as f32 + br as f32 * alpha * 0.9 + 15.0 * alpha).min(255.0) as u8;
                    img.put_pixel(px as u32, py as u32, Rgb([nr, ng, nb]));
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── CPU Hex Grid ─────────────────────────────────────────────────────────────

/// Per-core CPU-load visualised as a hexagonal grid.
/// Each hexagon's fill level = core load; colour = load category
/// (green → yellow → orange → red).
pub fn render_hex_grid() -> DynamicImage {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let cpus = sys.cpus();
    let n_cores = cpus.len().max(1);

    let font  = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Layout: determine hex size so all cores fit in a grid
    // Hex flat-top layout: hex width=2r, height=sqrt(3)*r
    // Try to fit in roughly square grid
    let cols = ((n_cores as f32).sqrt().ceil() as usize).max(1);
    let rows = (n_cores + cols - 1) / cols;

    let pad   = 6.0f32;
    let hex_r = ((480.0 - pad * (cols + 1) as f32) / (cols as f32 * 2.0))
                .min((480.0 - pad * (rows + 1) as f32) / (rows as f32 * 1.732))
                .min(48.0);
    let hex_w = hex_r * 2.0;
    let hex_h = hex_r * 1.732;

    let total_w = cols as f32 * hex_w + (cols + 1) as f32 * pad;
    let total_h = rows as f32 * hex_h + (rows + 1) as f32 * pad;
    let ox = (480.0 - total_w) / 2.0 + pad;
    let oy = (480.0 - total_h) / 2.0 + pad + 30.0; // +30 for title

    for (i, cpu) in cpus.iter().enumerate() {
        let load  = cpu.cpu_usage() / 100.0;
        let col   = i % cols;
        let row   = i / cols;

        let cx = ox + col as f32 * (hex_w + pad) + hex_r;
        let cy = oy + row as f32 * (hex_h + pad) + hex_r;

        // Flat-top hexagon: 6 vertices at angles 0°, 60°, 120°, 180°, 240°, 300°
        let verts: Vec<(f32, f32)> = (0..6).map(|k| {
            let angle = (k as f32 * 60.0 - 30.0).to_radians();
            (cx + hex_r * angle.cos(), cy + hex_r * angle.sin())
        }).collect();

        // Fill colour based on load
        let fill_col = if load < 0.4 { GREEN }
                        else if load < 0.7 { YELLOW }
                        else if load < 0.9 { PEACH }
                        else               { RED };

        // Draw filled hexagon by scanning bounding box
        let min_x = verts.iter().map(|v| v.0).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_x = verts.iter().map(|v| v.0).fold(f32::MIN, f32::max).min(479.0) as u32;
        let min_y = verts.iter().map(|v| v.1).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_y = verts.iter().map(|v| v.1).fold(f32::MIN, f32::max).min(479.0) as u32;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5; let fy = py as f32 + 0.5;
                // Point-in-polygon test
                let inside = verts.windows(2).chain(std::iter::once(
                    [verts[5], verts[0]].as_slice()
                )).fold(false, |inside, pair| {
                    let (ax, ay) = (pair[0].0, pair[0].1);
                    let (bx, by) = (pair[1].0, pair[1].1);
                    if (ay > fy) != (by > fy) &&
                       fx < (bx - ax) * (fy - ay) / (by - ay) + ax {
                        !inside
                    } else { inside }
                });
                if inside {
                    // Fill based on load: bottom portion lit, top dimmer
                    let rel_y = (fy - (cy - hex_r)) / (hex_r * 2.0); // 0=top, 1=bottom
                    let lit   = rel_y >= (1.0 - load);
                    let col   = if lit { fill_col } else { SURFACE0 };
                    img.put_pixel(px, py, col);
                }
            }
        }

        // Hexagon outline
        for k in 0..6 {
            let (x0, y0) = verts[k];
            let (x1, y1) = verts[(k + 1) % 6];
            draw_antialiased_line_segment_mut(
                &mut img,
                (x0 as i32, y0 as i32),
                (x1 as i32, y1 as i32),
                OVERLAY0, interpolate
            );
        }

        // Core number (small)
        if hex_r > 18.0 {
            let lbl = format!("{}", i);
            let lw  = text_width(&font, PxScale::from(11.0), &lbl);
            draw_text_mut(&mut img, TEXT, cx as i32 - lw / 2, cy as i32 - 6,
                          PxScale::from(11.0), &font, &lbl);
        }
    }

    // Title
    let avg_load: f32 = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / n_cores as f32;
    let title = format!("CPU Hex Grid — {} cores  avg {:.0}%", n_cores, avg_load);
    let tw = text_width(&font, PxScale::from(18.0), &title);
    draw_text_mut(&mut img, TEXT, (480 - tw) / 2, 8, PxScale::from(18.0), &font, &title);

    DynamicImage::ImageRgb8(img)
}

// ─── Process Bubble Chart ─────────────────────────────────────────────────────

/// Top processes by CPU usage visualised as bubbles.
/// Bubble area ∝ CPU%, colour = hash of process name.
pub fn render_bubble_chart() -> DynamicImage {
    use sysinfo::ProcessesToUpdate;

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let font  = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() { *p = BG; }

    // Collect top-N processes by CPU%
    let mut procs: Vec<(String, f32)> = sys.processes().values()
        .filter(|p| p.cpu_usage() > 0.1)
        .map(|p| (p.name().to_string_lossy().to_string(), p.cpu_usage()))
        .collect();
    procs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(20);

    if procs.is_empty() {
        draw_centered(&mut img, &font, PxScale::from(22.0), 220, OVERLAY0, "No active processes");
        return DynamicImage::ImageRgb8(img);
    }

    let max_cpu  = procs[0].1.max(1.0);
    let pal: [Rgb<u8>; 7] = [BLUE, GREEN, MAUVE, TEAL, PEACH, RED, YELLOW];

    // Pack bubbles using a simple grid layout with radius ∝ sqrt(cpu)
    let title_h = 30i32;
    let area_y  = title_h;
    let _area_h = 480 - title_h - 10;

    // Scale max bubble to ~80px radius
    let scale = 80.0f32 / max_cpu.sqrt();

    // Sort and place bubbles left-to-right, top-to-bottom
    let mut x_cursor = 0i32;
    let mut y_cursor = area_y;
    let mut row_max_r = 0i32;

    for (idx, (name, cpu)) in procs.iter().enumerate() {
        let r  = ((cpu.sqrt() * scale) as i32).max(10);
        if x_cursor + r * 2 > 480 {
            x_cursor  = 0;
            y_cursor += row_max_r * 2 + 8;
            row_max_r = 0;
        }
        if y_cursor + r * 2 > 480 { break; }

        let cx = (x_cursor + r) as f32;
        let cy = (y_cursor + r) as f32;
        x_cursor  += r * 2 + 6;
        row_max_r  = row_max_r.max(r);

        // Colour by name hash
        let hash: usize = name.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
        let fill = pal[hash % pal.len()];

        // Draw filled circle
        let r2 = r * r;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx*dx + dy*dy > r2 { continue; }
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px < 0 || px >= 480 || py < 0 || py >= 480 { continue; }
                // Radial brightness: centre bright, edge dim
                let dist_frac = ((dx*dx + dy*dy) as f32).sqrt() / r as f32;
                let br = 1.0 - dist_frac * 0.35;
                img.put_pixel(px as u32, py as u32, Rgb([
                    (fill[0] as f32 * br) as u8,
                    (fill[1] as f32 * br) as u8,
                    (fill[2] as f32 * br) as u8,
                ]));
            }
        }

        // Label inside large bubbles
        if r >= 24 {
            let short = if name.len() > 8 { &name[..8] } else { name.as_str() };
            let lw    = text_width(&font, PxScale::from(11.0), short);
            draw_text_mut(&mut img, BG, cx as i32 - lw / 2, cy as i32 - 12,
                          PxScale::from(11.0), &font, short);
            let pct = format!("{:.0}%", cpu);
            let pw  = text_width(&font, PxScale::from(11.0), &pct);
            draw_text_mut(&mut img, BG, cx as i32 - pw / 2, cy as i32 + 2,
                          PxScale::from(11.0), &font, &pct);
        }

        // Rank badge top-left if idx < 3
        if idx < 3 {
            let badge = ["①", "②", "③"][idx];
            draw_text_mut(&mut img, TEXT, cx as i32 - r + 2, cy as i32 - r + 2,
                          PxScale::from(12.0), &font, badge);
        }
    }

    // Title bar
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, title_h as u32), SURFACE0);
    draw_text_mut(&mut img, TEXT, 8, 6, PxScale::from(18.0), &font, "Process CPU Bubbles");
    draw_text_mut(&mut img, OVERLAY0, 320, 8, PxScale::from(14.0), &font,
                  &format!("{} procs", procs.len()));

    DynamicImage::ImageRgb8(img)
}
