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
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
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

    let now      = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &time_str);
    draw_text_mut(&mut img, OVERLAY0, 480 - 20 - tw, 18, PxScale::from(22.0), &mono, &time_str);

    if remaining_secs <= 0 {
        // Time is up
        draw_centered(&mut img, &bold, PxScale::from(60.0), 160, RED,  "TIME'S");
        draw_centered(&mut img, &bold, PxScale::from(60.0), 230, RED,  "UP");
        if !label.is_empty() {
            draw_centered(&mut img, &bold, PxScale::from(22.0), 320, OVERLAY0, label);
        }
        return DynamicImage::ImageRgb8(img);
    }

    // Colour: green when > 50% left, yellow 20–50%, red < 20%
    let frac_left = if total_secs > 0 {
        (remaining_secs as f32 / total_secs as f32).clamp(0.0, 1.0)
    } else { 1.0 };
    let accent = if frac_left > 0.5 { GREEN } else if frac_left > 0.2 { YELLOW } else { RED };

    // Format remaining time
    let h = remaining_secs / 3600;
    let m = (remaining_secs % 3600) / 60;
    let s = remaining_secs % 60;
    let (time_display, scale) = if h > 0 {
        (format!("{:02}:{:02}:{:02}", h, m, s), PxScale::from(72.0))
    } else {
        (format!("{:02}:{:02}", m, s), PxScale::from(90.0))
    };

    // Large countdown
    let tw2 = text_width(&mono, scale, &time_display);
    let x   = ((480 - tw2) / 2).max(0);
    draw_text_mut(&mut img, accent, x, 140, scale, &mono, &time_display);

    // Progress bar (remaining fraction)
    draw_bar(&mut img, 20, 260, 440, 16, frac_left, accent);

    // Label
    if !label.is_empty() {
        let lbl = if label.len() > 30 { format!("{}…", &label[..29]) } else { label.to_string() };
        draw_centered(&mut img, &bold, PxScale::from(22.0), 296, SUBTEXT, &lbl);
    }

    // Time elapsed / remaining subtext
    let _elapsed = total_secs - remaining_secs;
    let pct = ((1.0 - frac_left) * 100.0) as u32;
    let sub = format!("{}% elapsed  ({} remaining)", pct, format_uptime(remaining_secs as u64));
    draw_centered(&mut img, &bold, PxScale::from(15.0), 336, OVERLAY0, &sub);

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

