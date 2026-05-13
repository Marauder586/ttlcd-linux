//! Off-device frame generators.
//!
//! Each function returns a [`DynamicImage`] at the panel's native 480×480
//! resolution, ready to pass to [`crate::encode_for_panel`] or
//! [`crate::Panel::send_image`].

use std::path::Path;
use std::time::Duration;

use ab_glyph::{Font as AbFont, FontRef, PxScale, ScaleFont};
use chrono::Local;
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
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
}
