//! Slow abstract screensavers.

use super::*;

// ─── Ambient Pulse ────────────────────────────────────────────────────────────

pub(super) static PULSE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Ambient CPU pulse display — concentric expanding rings whose speed and
/// colour reflect the current CPU load. Blue=idle, yellow=moderate, red=high.
pub fn render_pulse() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{CpuRefreshKind, System};

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
    for p in img.pixels_mut() {
        *p = BG;
    }

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
            (137.0_f64 * alpha, 180.0 * alpha, 250.0 * alpha) // BLUE
        } else if cpu_pct < 60.0 {
            let t2 = (cpu_pct - 30.0) / 30.0;
            (
                (137.0 + (203.0 - 137.0) * t2) * alpha,
                (180.0 + (166.0 - 180.0) * t2) * alpha,
                (250.0 + (247.0 - 250.0) * t2) * alpha,
            ) // BLUE→MAUVE
        } else if cpu_pct < 85.0 {
            let t2 = (cpu_pct - 60.0) / 25.0;
            (
                (203.0 + (249.0 - 203.0) * t2) * alpha,
                (166.0 + (226.0 - 166.0) * t2) * alpha,
                (247.0 + (175.0 - 247.0) * t2) * alpha,
            ) // MAUVE→YELLOW
        } else {
            (
                (249.0 + (243.0 - 249.0) * ((cpu_pct - 85.0) / 15.0)) * alpha,
                (226.0 + (139.0 - 226.0) * ((cpu_pct - 85.0) / 15.0)) * alpha,
                (175.0 + (168.0 - 175.0) * ((cpu_pct - 85.0) / 15.0)) * alpha,
            ) // YELLOW→RED
        };

        // Draw ring as a band of pixels
        let r_f = r as f32;
        let inner_r = (r_f - 3.0).max(0.0);
        let outer_r = r_f + 3.0;

        for dy in (-(outer_r as i32) - 1)..=(outer_r as i32 + 1) {
            for dx in (-(outer_r as i32) - 1)..=(outer_r as i32 + 1) {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
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
    let cpu_str = format!("{cpu_pct:.0}%");
    let cw = text_width(&font, PxScale::from(48.0), &cpu_str);
    draw_text_mut(
        &mut img,
        Rgb([0, 0, 0]),
        (480 - cw) / 2 + 2,
        208,
        PxScale::from(48.0),
        &font,
        &cpu_str,
    );
    let col = if cpu_pct < 30.0 {
        BLUE
    } else if cpu_pct < 70.0 {
        MAUVE
    } else if cpu_pct < 85.0 {
        YELLOW
    } else {
        RED
    };
    draw_text_mut(
        &mut img,
        col,
        (480 - cw) / 2,
        206,
        PxScale::from(48.0),
        &font,
        &cpu_str,
    );

    let lbl = "CPU";
    let lw = text_width(&font, PxScale::from(20.0), lbl);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - lw) / 2,
        258,
        PxScale::from(20.0),
        &font,
        lbl,
    );

    // Time
    let now = chrono::Local::now();
    let ts = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let tw2 = text_width(&font, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - tw2) / 2,
        280,
        PxScale::from(18.0),
        &font,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_dna_helix  — DNA double helix scrolling animation
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static DNA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();

pub fn render_dna_helix() -> DynamicImage {
    let tick = {
        let m = DNA_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let scroll = tick as f32 * 1.2;
    const CX: f32 = 240.0;
    const AMPLITUDE: f32 = 130.0;
    const PERIOD: f32 = 140.0;
    let pair_cols: [Rgb<u8>; 4] = [GREEN, TEAL, MAUVE, PEACH];

    // Helper: alpha-blend a colour onto img
    let blend = |img: &mut RgbImage, px: u32, py: u32, col: Rgb<u8>, alpha: u8| {
        let fa = alpha as f32 / 255.0;
        let bg = *img.get_pixel(px, py);
        img.put_pixel(
            px,
            py,
            Rgb([
                (bg[0] as f32 * (1.0 - fa) + col[0] as f32 * fa) as u8,
                (bg[1] as f32 * (1.0 - fa) + col[1] as f32 * fa) as u8,
                (bg[2] as f32 * (1.0 - fa) + col[2] as f32 * fa) as u8,
            ]),
        );
    };

    // Draw base pairs
    let mut y = -20.0_f32;
    while y < 520.0 {
        let phase = (y + scroll) / PERIOD * std::f32::consts::TAU;
        let facing = phase.cos().abs();
        if facing < 0.5 {
            let x1 = CX + AMPLITUDE * phase.cos();
            let x2 = CX - AMPLITUDE * phase.cos();
            let ci = (((y + scroll) / 30.0) as usize) % 4;
            let col = pair_cols[ci];
            let alpha = (255.0 * (0.5 - facing) / 0.5) as u8;
            let (xa, xb) = if x1 < x2 {
                (x1 as i32, x2 as i32)
            } else {
                (x2 as i32, x1 as i32)
            };
            let yi = y as i32;
            if (0..480).contains(&yi) {
                for xi in xa..=xb {
                    if (0..480).contains(&xi) {
                        blend(&mut img, xi as u32, yi as u32, col, alpha);
                    }
                }
            }
        }
        y += 8.0;
    }

    // Draw the two strands
    for py in 0i32..480 {
        let phase = (py as f32 + scroll) / PERIOD * std::f32::consts::TAU;
        let ax = (CX + AMPLITUDE * phase.cos()) as i32;
        for dx in -3i32..=3 {
            let px_i = ax + dx;
            if (0..480).contains(&px_i) {
                let alpha = (255.0 * (1.0 - dx.abs() as f32 / 4.0)) as u8;
                blend(&mut img, px_i as u32, py as u32, BLUE, alpha);
            }
        }
        let bx = (CX - AMPLITUDE * phase.cos()) as i32;
        for dx in -3i32..=3 {
            let px_i = bx + dx;
            if (0..480).contains(&px_i) {
                let alpha = (255.0 * (1.0 - dx.abs() as f32 / 4.0)) as u8;
                blend(&mut img, px_i as u32, py as u32, RED, alpha);
            }
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        180,
        8,
        PxScale::from(16.0),
        &font,
        "DNA Helix",
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_heartbeat  — ECG-style heartbeat waveform driven by CPU load
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static HEARTBEAT_STATE: std::sync::OnceLock<
    std::sync::Mutex<(std::collections::VecDeque<f32>, u64)>,
> = std::sync::OnceLock::new();

pub fn render_heartbeat() -> DynamicImage {
    let state = HEARTBEAT_STATE.get_or_init(|| {
        std::sync::Mutex::new((std::collections::VecDeque::with_capacity(480), 0u64))
    });
    let mut st = state.lock().unwrap();
    let (ref mut wave, ref mut tick) = *st;
    *tick += 1;
    let tf = *tick as f32;

    // Sample CPU load
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    let cpu = sys.global_cpu_usage() / 100.0;

    // ECG waveform synthesis:
    // Base = fast sine for P-wave, tall spike for QRS complex, T-wave
    let period = 80.0_f32; // frames per beat
    let phase = (tf % period) / period;

    let ecg_val = if phase < 0.05 {
        // P wave (small bump)
        (phase / 0.05 * std::f32::consts::PI).sin() * 0.15
    } else if phase < 0.35 {
        // Flat PQ segment
        0.0
    } else if phase < 0.38 {
        // Q dip
        -0.2
    } else if phase < 0.42 {
        // R spike (height proportional to CPU)
        let t = (phase - 0.38) / 0.04;
        let spike = (t * std::f32::consts::PI).sin();
        spike * (0.8 + cpu * 0.8)
    } else if phase < 0.46 {
        // S dip
        -0.15
    } else if phase < 0.65 {
        // Flat ST segment
        0.0
    } else if phase < 0.80 {
        // T wave
        let t = (phase - 0.65) / 0.15;
        (t * std::f32::consts::PI).sin() * 0.25
    } else {
        0.0
    };

    wave.push_back(ecg_val);
    if wave.len() > 480 {
        wave.pop_front();
    }

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Grid lines
    for i in (0..480u32).step_by(60) {
        for py in 0..480u32 {
            let bg = *img.get_pixel(i, py);
            img.put_pixel(i, py, Rgb(bg.0.map(|v| v.saturating_add(8))));
        }
        for px in 0..480u32 {
            let bg = *img.get_pixel(px, i);
            img.put_pixel(px, i, Rgb(bg.0.map(|v| v.saturating_add(8))));
        }
    }

    // Draw waveform
    let centre_y = 240.0_f32;
    let scale = 180.0_f32;
    let wave_vec: Vec<f32> = wave.iter().copied().collect();
    let n = wave_vec.len();
    let x_start = 480 - n as u32;

    for i in 1..wave_vec.len() {
        let y0_f = centre_y - wave_vec[i - 1] * scale;
        let y1_f = centre_y - wave_vec[i] * scale;
        let x0 = x_start as i32 + i as i32 - 1;
        let x1 = x_start as i32 + i as i32;

        // Draw line segment with glow
        let steps = ((y1_f - y0_f).abs() as i32).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = (x0 as f32 + (x1 - x0) as f32 * t) as i32;
            let y = (y0_f + (y1_f - y0_f) * t) as i32;
            // Glow: ±2 pixels
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let glow_d = (dx * dx + dy * dy) as f32;
                    let alpha = (1.0 - glow_d / 8.0).max(0.0);
                    let px_g = x + dx;
                    let py_g = y + dy;
                    if (0..480).contains(&px_g) && (0..480).contains(&py_g) {
                        let bg = *img.get_pixel(px_g as u32, py_g as u32);
                        let fa = alpha;
                        img.put_pixel(
                            px_g as u32,
                            py_g as u32,
                            Rgb([
                                (bg[0] as f32 * (1.0 - fa) + GREEN[0] as f32 * fa) as u8,
                                (bg[1] as f32 * (1.0 - fa) + GREEN[1] as f32 * fa) as u8,
                                (bg[2] as f32 * (1.0 - fa) + GREEN[2] as f32 * fa) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    // CPU bar at top
    let cpu_w = (cpu * 460.0) as u32;
    for px in 0..cpu_w {
        for py in 0..6u32 {
            let t = px as f32 / 460.0;
            img.put_pixel(
                px + 10,
                py + 8,
                Rgb([
                    (GREEN[0] as f32 * (1.0 - t) + RED[0] as f32 * t) as u8,
                    (GREEN[1] as f32 * (1.0 - t) + RED[1] as f32 * t) as u8,
                    (GREEN[2] as f32 * (1.0 - t) + RED[2] as f32 * t) as u8,
                ]),
            );
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        TEXT,
        8,
        460,
        PxScale::from(14.0),
        &font,
        &format!(
            "CPU {:3.0}% BPM~{:.0}",
            cpu * 100.0,
            60000.0 / (period * 33.0)
        ),
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_morphing_blobs  — organic morphing blob shapes with signed-distance
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static MORPH_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_morphing_blobs() -> DynamicImage {
    let tick = {
        let m = MORPH_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.02;
    let mut img = RgbImage::new(480, 480);

    const W: u32 = 160;
    const H: u32 = 160;
    const CELL: u32 = 3;

    // 5 blobs with orbiting positions
    let blobs: [(f32, f32, f32); 5] = [
        (240.0 + 120.0 * tf.cos(), 240.0 + 80.0 * tf.sin(), 55.0),
        (
            240.0 + 100.0 * (tf * 0.7).sin(),
            240.0 + 120.0 * (tf * 0.7).cos(),
            45.0,
        ),
        (
            240.0 + 70.0 * (tf * 1.3).cos(),
            240.0 + 70.0 * (tf * 1.1).sin(),
            40.0,
        ),
        (
            240.0 + 90.0 * (tf * 0.5 + 1.0).sin(),
            240.0 + 90.0 * (tf * 0.5).cos(),
            50.0,
        ),
        (240.0, 240.0 + 50.0 * (tf * 0.9).sin(), 35.0),
    ];

    let blob_colours: [Rgb<u8>; 5] = [BLUE, MAUVE, TEAL, PEACH, RED];

    for cy in 0..H {
        for cx in 0..W {
            let px = cx as f32 / W as f32 * 480.0;
            let py = cy as f32 / H as f32 * 480.0;

            // Evaluate metaball field
            let mut field = 0.0f32;
            let mut weighted_col = [0.0f32; 3];

            for (bi, &(bx, by, br)) in blobs.iter().enumerate() {
                let dx = px - bx;
                let dy = py - by;
                let d2 = dx * dx + dy * dy;
                let w = (br * br) / d2.max(1.0);
                field += w;
                let c = blob_colours[bi % blob_colours.len()];
                weighted_col[0] += c[0] as f32 * w;
                weighted_col[1] += c[1] as f32 * w;
                weighted_col[2] += c[2] as f32 * w;
            }

            let col = if field > 1.0 {
                // Inside blob — interpolated colour
                Rgb([
                    (weighted_col[0] / field).min(255.0) as u8,
                    (weighted_col[1] / field).min(255.0) as u8,
                    (weighted_col[2] / field).min(255.0) as u8,
                ])
            } else if field > 0.55 {
                // Edge glow
                let t = (field - 0.55) / 0.45;
                Rgb([
                    (BG[0] as f32 * (1.0 - t) + (weighted_col[0] / field.max(0.01)) * t).min(255.0)
                        as u8,
                    (BG[1] as f32 * (1.0 - t) + (weighted_col[1] / field.max(0.01)) * t).min(255.0)
                        as u8,
                    (BG[2] as f32 * (1.0 - t) + (weighted_col[2] / field.max(0.01)) * t).min(255.0)
                        as u8,
                ])
            } else {
                BG
            };

            for dy in 0..CELL {
                for dx in 0..CELL {
                    img.put_pixel(cx * CELL + dx, cy * CELL + dy, col);
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_lava_lamp  — warm lava-lamp blob simulation
// ─────────────────────────────────────────────────────────────────────────────
pub(super) struct LavaBlob {
    x: f32,
    y: f32,
    vy: f32,
    r: f32,
    phase: f32,
    ci: usize,
}
pub(super) static LAVA_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<LavaBlob>>> =
    std::sync::OnceLock::new();

pub fn render_lava_lamp() -> DynamicImage {
    const N: usize = 8;
    const W: u32 = 120;
    const H: u32 = 120;
    const CELL: u32 = 4;

    let state = LAVA_STATE.get_or_init(|| {
        let mut s = 0x1A7A_1A4Bu32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let blobs = (0..N)
            .map(|i| LavaBlob {
                x: 20.0 + lcg(&mut s) * 440.0,
                y: lcg(&mut s) * 480.0,
                vy: (lcg(&mut s) - 0.5) * 1.0,
                r: 28.0 + lcg(&mut s) * 32.0,
                phase: lcg(&mut s) * std::f32::consts::TAU,
                ci: i % 5,
            })
            .collect();
        std::sync::Mutex::new(blobs)
    });
    let mut blobs = state.lock().unwrap();

    // Background: warm gradient
    let mut img = RgbImage::new(480, 480);
    for py in 0u32..480 {
        let t = py as f32 / 480.0;
        img.put_pixel(0, py, Rgb([0x18, 0x08, 0x0a])); // seed row for fill
        for px in 0u32..480 {
            img.put_pixel(
                px,
                py,
                Rgb([
                    (0x18_u32 + (t * (0x30 - 0x18) as f32) as u32) as u8,
                    (0x08_u32 + (t * (0x10 - 0x08) as f32) as u32) as u8,
                    (0x0a_u32 + (t * (0x14 - 0x0a) as f32) as u32) as u8,
                ]),
            );
        }
    }

    let lava_cols: [[u8; 3]; 5] = [
        [0xf3, 0x8b, 0xa8], // red
        [0xfa, 0xb3, 0x87], // peach
        [0xf9, 0xe2, 0xaf], // yellow
        [0xcb, 0xa6, 0xf7], // mauve
        [0xff, 0x60, 0x00], // orange
    ];

    // Update blobs
    for blob in blobs.iter_mut() {
        blob.phase += 0.02;
        blob.vy += (blob.y / 480.0 - 0.5) * -0.05; // buoyancy: pull toward centre
        blob.vy += (blob.phase * 0.5).sin() * 0.03; // slow wobble
        blob.vy = blob.vy.clamp(-1.5, 1.5);
        blob.y += blob.vy;
        // Horizontal wobble
        blob.x += (blob.phase * 0.7).sin() * 0.4;
        blob.x = blob.x.clamp(blob.r + 5.0, 480.0 - blob.r - 5.0);
        if blob.y < -blob.r {
            blob.y = 480.0 + blob.r;
        }
        if blob.y > 480.0 + blob.r {
            blob.y = -blob.r;
        }
    }

    // Render metaballs on 120×120 grid
    for cy in 0..H {
        for cx in 0..W {
            let px_f = cx as f32 / W as f32 * 480.0;
            let py_f = cy as f32 / H as f32 * 480.0;

            let mut field = 0.0f32;
            let mut wc = [0.0f32; 3];
            for blob in blobs.iter() {
                let dx = px_f - blob.x;
                let dy = py_f - blob.y;
                let d2 = dx * dx + dy * dy;
                let w = blob.r * blob.r / d2.max(1.0);
                field += w;
                let c = lava_cols[blob.ci % lava_cols.len()];
                wc[0] += c[0] as f32 * w;
                wc[1] += c[1] as f32 * w;
                wc[2] += c[2] as f32 * w;
            }

            if field > 1.0 {
                let col = [
                    (wc[0] / field).min(255.0) as u8,
                    (wc[1] / field).min(255.0) as u8,
                    (wc[2] / field).min(255.0) as u8,
                ];
                for dy in 0..CELL {
                    for dx in 0..CELL {
                        img.put_pixel(cx * CELL + dx, cy * CELL + dy, Rgb(col));
                    }
                }
            } else if field > 0.5 {
                // Glow
                let t = (field - 0.5) * 2.0;
                let bg = *img.get_pixel(cx * CELL, cy * CELL);
                let col = Rgb([
                    (bg[0] as f32 * (1.0 - t) + wc[0] / field.max(0.01) * t).min(255.0) as u8,
                    (bg[1] as f32 * (1.0 - t) + wc[1] / field.max(0.01) * t).min(255.0) as u8,
                    (bg[2] as f32 * (1.0 - t) + wc[2] / field.max(0.01) * t).min(255.0) as u8,
                ]);
                for dy in 0..CELL {
                    for dx in 0..CELL {
                        img.put_pixel(cx * CELL + dx, cy * CELL + dy, col);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_fiber_optic  — glowing fiber-optic light strands radiating from centre
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static FIBER_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_fiber_optic() -> DynamicImage {
    let tick = {
        let m = FIBER_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.016;
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x04, 0x04, 0x0a]);
    }

    const N_FIBERS: usize = 80;
    const CX: f32 = 240.0;
    const CY: f32 = 240.0;

    let fiber_cols: [Rgb<u8>; 6] = [BLUE, TEAL, GREEN, MAUVE, PEACH, RED];

    let mut fs = 0xF1BE_7A3Cu32;
    let fiber_lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    for fi in 0..N_FIBERS {
        let base_angle = fi as f32 / N_FIBERS as f32 * std::f32::consts::TAU;
        // Each fiber curves away from its base angle with a sinusoidal bend
        let fiber_phase = fiber_lcg(&mut fs) * std::f32::consts::TAU;
        let bend_amp = fiber_lcg(&mut fs) * 0.5;
        let ci = fi % fiber_cols.len();
        let col = fiber_cols[ci];

        // Animate: brightness pulses along fiber
        let pulse_phase = fiber_lcg(&mut fs) * std::f32::consts::TAU;
        let pulse_freq = 0.3 + fiber_lcg(&mut fs) * 0.7;

        // Draw fiber as a curve with 60 segments
        let mut prev_x = CX;
        let mut prev_y = CY;

        for seg in 1..=60usize {
            let t = seg as f32 / 60.0;
            let r = t * 230.0;
            let curve = bend_amp * (fiber_phase + t * 4.0 + tf * 0.5).sin();
            let angle = base_angle + curve;
            let x = CX + angle.cos() * r;
            let y = CY + angle.sin() * r;

            // Pulse brightness along the fiber
            let dist_from_pulse = ((t - ((tf * pulse_freq + pulse_phase) % 1.0)).abs()) % 1.0;
            let brightness = ((-dist_from_pulse * 8.0).exp() + (1.0 - t) * 0.3).min(1.0);

            if (0.0..480.0).contains(&x) && (0.0..480.0).contains(&y) {
                let bg = *img.get_pixel(x as u32, y as u32);
                img.put_pixel(
                    x as u32,
                    y as u32,
                    Rgb([
                        (bg[0] as f32 + col[0] as f32 * brightness).min(255.0) as u8,
                        (bg[1] as f32 + col[1] as f32 * brightness).min(255.0) as u8,
                        (bg[2] as f32 + col[2] as f32 * brightness).min(255.0) as u8,
                    ]),
                );
            }
            prev_x = x;
            prev_y = y;
        }
        let _ = (prev_x, prev_y);
    }

    // Central glow
    for r in 0..=25u32 {
        for angle_deg in 0..360u32 {
            let a = angle_deg as f32 * std::f32::consts::TAU / 360.0;
            let px = (CX + a.cos() * r as f32) as u32;
            let py = (CY + a.sin() * r as f32) as u32;
            if px < 480 && py < 480 {
                let glow = (-(r as f32) * 0.2).exp();
                let bg = *img.get_pixel(px, py);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (bg[0] as f32 + 255.0 * glow).min(255.0) as u8,
                        (bg[1] as f32 + 255.0 * glow).min(255.0) as u8,
                        (bg[2] as f32 + 255.0 * glow).min(255.0) as u8,
                    ]),
                );
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_plasma_ball  — electric plasma ball with tendrils
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static PLASMA_BALL_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_plasma_ball() -> DynamicImage {
    let tick = {
        let m = PLASMA_BALL_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.025;
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x06, 0x04, 0x10]);
    }

    const CX: f32 = 240.0;
    const CY: f32 = 240.0;
    const R: f32 = 160.0;

    for py in 0u32..480 {
        for px in 0u32..480 {
            let dx = px as f32 - CX;
            let dy = py as f32 - CY;
            let r = (dx * dx + dy * dy).sqrt();

            if r > R + 15.0 {
                continue;
            }

            let angle = dy.atan2(dx);
            let t_sphere = (r / R).min(1.0);

            // Electric field lines: sum of multiple sine frequencies
            let e1 = (r * 0.04 - tf * 2.0 + angle * 5.0 + (tf * 0.7).sin()).sin();
            let e2 = (r * 0.06 - tf * 1.5 + angle * 3.0 + (tf * 1.1).cos()).sin();
            let e3 = (r * 0.08 - tf * 3.0 + angle * 7.0).sin();
            let e4 = ((dx * 0.05).sin() + (dy * 0.05).cos() + tf * 0.8).sin();

            let energy = ((e1 + e2 * 0.5 + e3 * 0.25 + e4 * 0.25) * 0.4 + 0.5).powi(2)
                * (1.0 - t_sphere * 0.5);

            // Glass sphere effect: bright at rim
            let rim = if r > R - 8.0 && r <= R + 5.0 {
                (1.0 - (r - (R - 4.0)).abs() / 9.0).max(0.0) * 0.5
            } else {
                0.0
            };

            // Core glow
            let core_glow = (-r * 0.012).exp() * 0.4;

            let final_energy = (energy + rim + core_glow).min(1.0);
            if final_energy < 0.05 {
                continue;
            }

            // Color: purple/blue/white electric
            let h = ((e1 * 0.1 + tf * 0.003) + 0.7) % 1.0;
            let sat = 0.8 - t_sphere * 0.3;
            let lit = 0.15 + final_energy * 0.65;
            let col = hsl_to_rgb(h, sat, lit);
            img.put_pixel(px, py, col);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Bubble Rise ───────────────────────────────────────────────────────────────
pub fn render_bubble_rise() -> DynamicImage {
    struct Bubble {
        x: f32,
        y: f32,
        vy: f32,
        r: f32,
        wobble_phase: f32,
        wobble_speed: f32,
        hue: f32,
        alive: bool,
    }
    static BUBBLE_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<Bubble>, u32, u64)>> =
        std::sync::OnceLock::new();

    let mut guard = BUBBLE_STATE
        .get_or_init(|| std::sync::Mutex::new((Vec::new(), 0xBBB1_E5EDu32, 0u64)))
        .lock()
        .unwrap();
    let (ref mut bubbles, ref mut seed, ref mut tick) = *guard;
    *tick += 1;
    let tf = *tick as f32;

    let mut s = *seed;
    let mut rng = || -> f32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s as f32) / (u32::MAX as f32)
    };

    // Spawn new bubbles
    if *tick % 4 == 0 && bubbles.len() < 55 {
        bubbles.push(Bubble {
            x: rng() * 440.0 + 20.0,
            y: 490.0,
            vy: -(0.8 + rng() * 1.4),
            r: 8.0 + rng() * 18.0,
            wobble_phase: rng() * std::f32::consts::TAU,
            wobble_speed: 0.04 + rng() * 0.05,
            hue: rng(),
            alive: true,
        });
    }

    // Update
    for b in bubbles.iter_mut() {
        b.y += b.vy;
        b.wobble_phase += b.wobble_speed;
        b.x += b.wobble_phase.sin() * 0.5;
        if b.y + b.r < 0.0 {
            b.alive = false;
        }
    }
    bubbles.retain(|b| b.alive);
    *seed = s;

    let mut img = RgbImage::new(480, 480);

    // Deep ocean gradient
    for py in 0..480u32 {
        let t = py as f32 / 480.0;
        let r = (8.0 + t * 5.0) as u8;
        let g = (20.0 + t * 30.0) as u8;
        let b = (60.0 + t * 80.0) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Caustic light rays
    for py in 0..480u32 {
        let t = py as f32 / 480.0;
        for px in (0..480u32).step_by(3) {
            let wave = ((px as f32 * 0.025 + tf * 0.02).sin()
                + (px as f32 * 0.04 - tf * 0.015).cos())
                * 0.5;
            if wave > 0.3 {
                let intensity = (wave - 0.3) * 0.06 * (1.0 - t);
                let p = img.get_pixel(px, py);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (p[0] as f32 + intensity * 30.0).min(255.0) as u8,
                        (p[1] as f32 + intensity * 60.0).min(255.0) as u8,
                        (p[2] as f32 + intensity * 80.0).min(255.0) as u8,
                    ]),
                );
            }
        }
    }

    // Draw bubbles
    for b in bubbles.iter() {
        // Convert hue to RGB
        let h6 = b.hue * 6.0;
        let hi = h6 as u32 % 6;
        let f = h6.fract();
        let v = 0.7f32;
        let s_v = 0.4f32;
        let (br, bg, bb) = match hi {
            0 => (v, v * (1.0 - s_v * (1.0 - f)), v * (1.0 - s_v)),
            1 => (v * (1.0 - s_v * f), v, v * (1.0 - s_v)),
            2 => (v * (1.0 - s_v), v, v * (1.0 - s_v * (1.0 - f))),
            3 => (v * (1.0 - s_v), v * (1.0 - s_v * f), v),
            4 => (v * (1.0 - s_v * (1.0 - f)), v * (1.0 - s_v), v),
            _ => (v, v * (1.0 - s_v), v * (1.0 - s_v * f)),
        };

        let ir = b.r as i32 + 2;
        let bx = b.x as i32;
        let by = b.y as i32;
        for dy in -ir..=ir {
            for dx in -ir..=ir {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                let px = bx + dx;
                let py = by + dy;
                if !(0..480).contains(&px) || !(0..480).contains(&py) {
                    continue;
                }
                if dist <= b.r {
                    // Bubble ring: only draw rim and highlight
                    let rim = (dist - (b.r - 2.0)).max(0.0) / 2.0;
                    let inner_glow = ((b.r * 0.45 - dist) / (b.r * 0.45)).max(0.0);
                    if rim > 0.0 || inner_glow > 0.0 {
                        let alpha = (rim * 0.7 + inner_glow * 0.15).min(1.0);
                        let p = img.get_pixel(px as u32, py as u32);
                        img.put_pixel(
                            px as u32,
                            py as u32,
                            Rgb([
                                (p[0] as f32 * (1.0 - alpha) + br * 255.0 * alpha).min(255.0) as u8,
                                (p[1] as f32 * (1.0 - alpha) + bg * 255.0 * alpha).min(255.0) as u8,
                                (p[2] as f32 * (1.0 - alpha) + bb * 255.0 * alpha).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }

        // Specular highlight (upper-left)
        let hx = (b.x - b.r * 0.35) as i32;
        let hy = (b.y - b.r * 0.35) as i32;
        let hr = (b.r * 0.25) as i32;
        for dy in -hr..=hr {
            for dx in -hr..=hr {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist <= hr as f32 {
                    let px = hx + dx;
                    let py = hy + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        let alpha = (1.0 - dist / hr as f32) * 0.5;
                        let p = img.get_pixel(px as u32, py as u32);
                        img.put_pixel(
                            px as u32,
                            py as u32,
                            Rgb([
                                (p[0] as f32 + (255.0 - p[0] as f32) * alpha).min(255.0) as u8,
                                (p[1] as f32 + (255.0 - p[1] as f32) * alpha).min(255.0) as u8,
                                (p[2] as f32 + (255.0 - p[2] as f32) * alpha).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    let _ = tf;
    DynamicImage::ImageRgb8(img)
}

// ── Neural Network Visualization ─────────────────────────────────────────────
// Animated network of nodes and weighted connections with a signal pulse.
pub fn render_neural_net() -> DynamicImage {
    static NN_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = NN_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([15u8, 12, 22]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Network layout: 4 layers with varying node counts
    let layers: &[usize] = &[4, 7, 7, 3];
    let layer_x = [70.0f32, 185.0, 295.0, 410.0];

    // Collect all node positions
    let mut nodes: Vec<(f32, f32, usize, usize)> = Vec::new(); // (x, y, layer, idx)
    for (li, &count) in layers.iter().enumerate() {
        let spacing = 380.0 / (count + 1) as f32;
        for ni in 0..count {
            let nx = layer_x[li];
            let ny = spacing * (ni + 1) as f32 + 50.0;
            nodes.push((nx, ny, li, ni));
        }
    }

    // Draw connections between adjacent layers
    let signal_speed = 0.025f32;
    let signal_phase = tf * signal_speed;

    // For each connection, compute "excitement" based on signal phase
    let mut layer_start = [0usize; 5];
    let mut cum = 0;
    for (i, &n) in layers.iter().enumerate() {
        layer_start[i] = cum;
        cum += n;
    }

    // Seed-based weights (deterministic)
    let mut ws: u32 = 0xDEAD_BEEFu32;
    for li in 0..(layers.len() - 1) {
        let src_start = layer_start[li];
        let dst_start = layer_start[li + 1];
        for si in 0..layers[li] {
            for di in 0..layers[li + 1] {
                ws = ws.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let weight = (ws as f32 / u32::MAX as f32) * 2.0 - 1.0; // -1..1
                let (sx, sy, _, _) = nodes[src_start + si];
                let (dx, dy, _, _) = nodes[dst_start + di];

                // Signal pulse travelling along this connection
                let conn_phase =
                    (li as f32 * 0.4 + si as f32 * 0.15 + di as f32 * 0.09 + signal_phase).fract();
                let pulse = if conn_phase < 0.15 {
                    let t = conn_phase / 0.15;
                    (t * std::f32::consts::PI).sin()
                } else {
                    0.0
                };

                let excite = weight.abs() * 0.5 + pulse * 0.5;
                let r_val = if weight > 0.0 {
                    (80.0 + excite * 120.0) as u8
                } else {
                    30
                };
                let g_val = if weight > 0.0 {
                    (50.0 + excite * 80.0) as u8
                } else {
                    (30.0 + excite * 100.0) as u8
                };
                let b_val = if weight < 0.0 {
                    (80.0 + excite * 120.0) as u8
                } else {
                    60
                };

                // Draw line
                let steps = 60i32;
                for step in 0..=steps {
                    let t = step as f32 / steps as f32;
                    let px = (sx + (dx - sx) * t) as i32;
                    let py = (sy + (dy - sy) * t) as i32;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        let p = img.get_pixel(px as u32, py as u32);
                        let alpha = (excite * 0.4 + 0.06).min(1.0);
                        img.put_pixel(
                            px as u32,
                            py as u32,
                            Rgb([
                                (p[0] as f32 * (1.0 - alpha) + r_val as f32 * alpha).min(255.0)
                                    as u8,
                                (p[1] as f32 * (1.0 - alpha) + g_val as f32 * alpha).min(255.0)
                                    as u8,
                                (p[2] as f32 * (1.0 - alpha) + b_val as f32 * alpha).min(255.0)
                                    as u8,
                            ]),
                        );
                    }
                }

                // Draw pulse dot
                if pulse > 0.05 {
                    let px = (sx + (dx - sx) * conn_phase / 0.15) as i32;
                    let py = (sy + (dy - sy) * conn_phase / 0.15) as i32;
                    for dy2 in -2i32..=2 {
                        for dx2 in -2i32..=2 {
                            let bx = px + dx2;
                            let by = py + dy2;
                            if (0..480).contains(&bx) && (0..480).contains(&by) {
                                let p = img.get_pixel(bx as u32, by as u32);
                                img.put_pixel(
                                    bx as u32,
                                    by as u32,
                                    Rgb([
                                        (p[0] as f32 + 180.0 * pulse).min(255.0) as u8,
                                        (p[1] as f32 + 180.0 * pulse).min(255.0) as u8,
                                        (p[2] as f32 + 220.0 * pulse).min(255.0) as u8,
                                    ]),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Draw nodes
    for (nx, ny, li, ni) in &nodes {
        // Node activation based on time
        let activation =
            ((tf * 0.07 + *li as f32 * 1.2 + *ni as f32 * 0.7).sin() * 0.5 + 0.5).powi(2);
        let node_r = 12i32;
        for dy in -node_r..=node_r {
            for dx in -node_r..=node_r {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist <= node_r as f32 {
                    let bx = *nx as i32 + dx;
                    let by = *ny as i32 + dy;
                    if (0..480).contains(&bx) && (0..480).contains(&by) {
                        let rim = (dist - (node_r as f32 - 2.5)).max(0.0) / 2.5;
                        let inner = (1.0 - dist / node_r as f32) * 0.5;
                        let alpha = rim * 0.8 + inner * activation * 0.7;
                        let layer_hue = *li as f32 / (layers.len() - 1) as f32;
                        let (lr, lg, lb) = (
                            (100.0 + layer_hue * 80.0 + activation * 100.0).min(255.0),
                            (80.0 + (1.0 - layer_hue) * 80.0 + activation * 120.0).min(255.0),
                            (160.0 + activation * 95.0).min(255.0),
                        );
                        let p = img.get_pixel(bx as u32, by as u32);
                        img.put_pixel(
                            bx as u32,
                            by as u32,
                            Rgb([
                                (p[0] as f32 * (1.0 - alpha) + lr * alpha).min(255.0) as u8,
                                (p[1] as f32 * (1.0 - alpha) + lg * alpha).min(255.0) as u8,
                                (p[2] as f32 * (1.0 - alpha) + lb * alpha).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Ribbon Dance ──────────────────────────────────────────────────────────────
// Colourful flowing ribbons that undulate and twist across the display.
pub fn render_ribbon_dance() -> DynamicImage {
    static RIBBON_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = RIBBON_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([16u8, 12, 18]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // 7 ribbons with different parameters
    type Ribbon = (f32, f32, f32, f32, f32, [u8; 3]);
    let ribbons: &[Ribbon] = &[
        (0.0, 0.8, 2.3, 0.0, 0.012, [137, 180, 250]),   // blue
        (0.14, 0.9, 3.1, 0.7, 0.015, [203, 166, 247]),  // mauve
        (0.28, 0.7, 2.7, 1.4, 0.010, [166, 227, 161]),  // green
        (0.42, 1.0, 3.5, 2.1, 0.013, [250, 179, 135]),  // peach
        (0.57, 0.85, 2.9, 2.8, 0.011, [249, 226, 175]), // yellow
        (0.71, 0.75, 3.3, 3.5, 0.014, [243, 139, 168]), // red
        (0.85, 0.95, 2.5, 4.2, 0.016, [148, 226, 213]), // teal
    ];

    let n_pts = 200usize;
    let ribbon_half_width = 12.0f32;

    for &(hue_base, amp, freq, phase, speed, col) in ribbons {
        let _ = hue_base;
        // Compute spine points
        let mut spine: Vec<(f32, f32)> = Vec::with_capacity(n_pts + 1);
        for i in 0..=n_pts {
            let t = i as f32 / n_pts as f32;
            let x = t * 520.0 - 20.0;
            // Multiple harmonics for complex shape
            let y = 240.0
                + amp * 90.0 * (freq * t * std::f32::consts::TAU + phase + tf * speed).sin()
                + amp
                    * 30.0
                    * (freq * 1.7 * t * std::f32::consts::TAU - phase * 0.8 + tf * speed * 1.3)
                        .cos()
                + amp
                    * 15.0
                    * (freq * 3.1 * t * std::f32::consts::TAU + phase * 1.5 - tf * speed * 0.7)
                        .sin();
            spine.push((x, y));
        }

        // Draw ribbon as thick strokes
        for i in 0..n_pts {
            let (x1, y1) = spine[i];
            let (x2, y2) = spine[i + 1];
            let dx = x2 - x1;
            let dy = y2 - y1;
            let seg_len = (dx * dx + dy * dy).sqrt().max(0.001);
            let nx = -dy / seg_len;
            let ny = dx / seg_len;

            // Alpha variation (fade at edges, twist shimmer)
            let twist = (i as f32 * 0.05 + tf * 0.02 + phase).sin();
            let ribbon_w = ribbon_half_width * (0.7 + twist.abs() * 0.3);
            let alpha_center = 0.75 + twist * 0.15;

            let steps = (seg_len.ceil() as i32).max(1);
            for si in 0..=steps {
                let st = si as f32 / steps as f32;
                let mx = x1 + dx * st;
                let my = y1 + dy * st;

                // Draw across the ribbon width
                let w_steps = 8i32;
                for wi in -w_steps..=w_steps {
                    let wt = wi as f32 / w_steps as f32; // -1..1
                    let wdist = wt.abs();
                    let alpha = alpha_center * (1.0 - wdist * wdist);
                    if alpha < 0.02 {
                        continue;
                    }

                    let rpx = (mx + nx * wt * ribbon_w) as i32;
                    let rpy = (my + ny * wt * ribbon_w) as i32;
                    if !(0..480).contains(&rpx) || !(0..480).contains(&rpy) {
                        continue;
                    }

                    // Highlight at center of ribbon
                    let highlight = (1.0 - wdist * 2.0).max(0.0) * 0.4;
                    let p = img.get_pixel(rpx as u32, rpy as u32);
                    img.put_pixel(
                        rpx as u32,
                        rpy as u32,
                        Rgb([
                            (p[0] as f32 * (1.0 - alpha)
                                + (col[0] as f32 + highlight * 80.0).min(255.0) * alpha)
                                .min(255.0) as u8,
                            (p[1] as f32 * (1.0 - alpha)
                                + (col[1] as f32 + highlight * 80.0).min(255.0) * alpha)
                                .min(255.0) as u8,
                            (p[2] as f32 * (1.0 - alpha)
                                + (col[2] as f32 + highlight * 80.0).min(255.0) * alpha)
                                .min(255.0) as u8,
                        ]),
                    );
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Glitch Art ────────────────────────────────────────────────────────────────
// RGB channel displacement and scan-line corruption effects.
pub fn render_glitch_art() -> DynamicImage {
    static GLITCH_TICK: std::sync::OnceLock<std::sync::Mutex<(u64, u32)>> =
        std::sync::OnceLock::new();
    let (tick, glitch_seed) = {
        let mut g = GLITCH_TICK
            .get_or_init(|| std::sync::Mutex::new((0u64, 0xA1B2_C3D4u32)))
            .lock()
            .unwrap();
        g.0 += 1;
        // New glitch pattern every ~20 frames
        if g.0.is_multiple_of(22) {
            g.1 = (g.0 as u32).wrapping_mul(2_654_435_761);
        }
        (g.0, g.1)
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // Base layer: time-varying gradient with geometric shapes
    for py in 0..480u32 {
        for px in 0..480u32 {
            let x = px as f32 / 480.0;
            let y = py as f32 / 480.0;
            // Moving plasma base
            let v1 = (x * 8.0 + tf * 0.03).sin();
            let v2 = (y * 6.0 - tf * 0.025).cos();
            let v3 = ((x + y) * 4.0 + tf * 0.02).sin();
            let plasma = (v1 + v2 + v3) / 3.0;
            let hue = ((plasma * 0.4 + tf * 0.004) % 1.0 + 1.0) % 1.0;
            img.put_pixel(px, py, hsl_to_rgb(hue, 0.7, 0.25 + plasma * 0.1));
        }
    }

    // Glitch: horizontal scan-line shifts
    let mut gs = glitch_seed;
    let n_glitch_bands = 15u32;
    for _ in 0..n_glitch_bands {
        gs = gs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let band_y = (gs >> 23) % 480;
        gs = gs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let band_h = 2 + (gs >> 28) % 12;
        gs = gs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let shift = (gs as i32 % 80) - 40; // -40..40

        for by in band_y..(band_y + band_h).min(480) {
            let row_pixels: Vec<Rgb<u8>> = (0..480u32)
                .map(|x| {
                    let src_x = ((x as i32 + shift).rem_euclid(480)) as u32;
                    *img.get_pixel(src_x, by)
                })
                .collect();
            for (bx, col) in row_pixels.into_iter().enumerate() {
                img.put_pixel(bx as u32, by, col);
            }
        }
    }

    // RGB channel separation
    gs = gs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let r_shift = (gs as i32 % 20) - 10;
    gs = gs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let b_shift = -((gs as i32 % 20) - 10);

    // Separate channels with slight horizontal offset
    let channels: Vec<(u8, u8, u8)> = (0..480 * 480)
        .map(|i| {
            let px = i % 480;
            let py = i / 480;
            let p = img.get_pixel(px as u32, py as u32);
            (p[0], p[1], p[2])
        })
        .collect();

    let _ = (r_shift, b_shift, channels);

    // Simpler approach: per-row R/B offset
    for py in 0..480u32 {
        let row: Vec<Rgb<u8>> = (0..480u32).map(|px| *img.get_pixel(px, py)).collect();
        for px in 0..480u32 {
            let r = row[(px as i32 + r_shift).rem_euclid(480) as usize][0];
            let g = row[px as usize][1];
            let b = row[(px as i32 + b_shift).rem_euclid(480) as usize][2];
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Scan lines (CRT effect): every 3 rows, slightly darken
    for py in (0..480u32).step_by(3) {
        for px in 0..480u32 {
            let p = img.get_pixel(px, py);
            img.put_pixel(
                px,
                py,
                Rgb([
                    (p[0] as f32 * 0.7) as u8,
                    (p[1] as f32 * 0.7) as u8,
                    (p[2] as f32 * 0.7) as u8,
                ]),
            );
        }
    }

    // Noise pixels (random salt/pepper for digital corruption feel)
    let mut ns = glitch_seed ^ 0xFACE_CAFE;
    for _ in 0..80u32 {
        ns = ns.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let nx = ns % 480;
        ns = ns.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let ny = (ns >> 16) % 480;
        img.put_pixel(nx, ny, Rgb([255, 255, 255]));
    }

    let _ = tf;
    DynamicImage::ImageRgb8(img)
}
