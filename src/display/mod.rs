//! Off-device frame generators.
//!
//! Each function returns a [`DynamicImage`] at the panel's native 480×480
//! resolution, ready to pass to [`crate::encode_for_panel`] or
//! [`crate::Panel::send_image`].

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use ab_glyph::{Font as AbFont, FontRef, PxScale, ScaleFont};
use chrono::{Datelike, Local, Timelike};
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{
    draw_antialiased_line_segment_mut, draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut,
};
use imageproc::pixelops::interpolate;
use imageproc::rect::Rect;
use qrcode::QrCode;
use sysinfo::System;

mod ambient;
mod basic;
mod clocks;
mod dashboards;
mod fractals;
mod games;
mod graphs;
mod monitors;
mod physics;
mod scenes;
mod system;
mod sysviz;
#[cfg(test)]
mod tests;
mod three_d;
mod widgets;

pub use ambient::*;
pub use basic::*;
pub use clocks::*;
pub use dashboards::*;
pub use fractals::*;
pub use games::*;
pub use graphs::*;
pub use monitors::*;
pub use physics::*;
pub use scenes::*;
pub use system::*;
pub use sysviz::*;
pub use three_d::*;
pub use widgets::*;

// ─── Embedded font data ──────────────────────────────────────────────────────

static FONT_BOLD_DATA: &[u8] = include_bytes!("../../assets/DejaVuSans-Bold.ttf");
static FONT_MONO_DATA: &[u8] = include_bytes!("../../assets/DejaVuSansMono-Bold.ttf");

fn font_bold() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BOLD_DATA).expect("embedded DejaVuSans-Bold.ttf is corrupt")
}

fn font_mono() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_MONO_DATA).expect("embedded DejaVuSansMono-Bold.ttf is corrupt")
}

// ─── Colour palette (Catppuccin Mocha) ───────────────────────────────────────

const BG: Rgb<u8> = Rgb([30, 30, 46]); // base
const SURFACE0: Rgb<u8> = Rgb([49, 50, 68]); // surface0
const OVERLAY0: Rgb<u8> = Rgb([108, 112, 134]); // overlay0
const TEXT: Rgb<u8> = Rgb([205, 214, 244]); // text
const SUBTEXT: Rgb<u8> = Rgb([166, 173, 200]); // subtext1
const BLUE: Rgb<u8> = Rgb([137, 180, 250]); // blue
const GREEN: Rgb<u8> = Rgb([166, 227, 161]); // green
const YELLOW: Rgb<u8> = Rgb([249, 226, 175]); // yellow
const RED: Rgb<u8> = Rgb([243, 139, 168]); // red
const MAUVE: Rgb<u8> = Rgb([203, 166, 247]); // mauve
const TEAL: Rgb<u8> = Rgb([148, 226, 213]); // teal
const PEACH: Rgb<u8> = Rgb([250, 179, 135]); // peach
const CRUST: Rgb<u8> = Rgb([17, 17, 27]); // crust (near-black, for QR modules)
const WHITE: Rgb<u8> = Rgb([255, 255, 255]); // pure white (for QR quiet zone)

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
    draw_hollow_rect_mut(
        img,
        Rect::at(bx, by).of_size(bw as u32, bh as u32),
        OVERLAY0,
    );
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
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m {}s", m, secs % 60)
    }
}

fn draw_brackets(img: &mut RgbImage) {
    let sz = 24u32;
    let th = 3u32;
    let mg: i32 = 16;
    let corners: [(i32, i32); 4] = [
        (mg, mg),
        (480 - mg - sz as i32, mg),
        (mg, 480 - mg - sz as i32),
        (480 - mg - sz as i32, 480 - mg - sz as i32),
    ];
    for (cx, cy) in corners {
        draw_filled_rect_mut(img, Rect::at(cx, cy).of_size(sz, th), MAUVE);
        draw_filled_rect_mut(img, Rect::at(cx, cy).of_size(th, sz), MAUVE);
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
        format!("{b} B")
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
