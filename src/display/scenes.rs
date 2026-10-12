//! Scenery screensavers: weather, fire, city and neon effects.

use super::*;

// ─── Matrix rain screensaver ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub(super) struct RainColumn {
    /// Current head row (pixels from top)
    head: i32,
    /// Speed in pixels per frame
    speed: i32,
    /// Trail length in pixels
    trail: i32,
    /// Characters in this column (indices into MATRIX_CHARS)
    chars: Vec<usize>,
    /// Randomization salt for char changing
    phase: u32,
}

pub(super) const MATRIX_CHARS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ',
    'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ',
    'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J',
    '|', '|', '/', '-', '\\', '_', '=', '+', '*', '#',
];

pub(super) static RAIN_STATE: OnceLock<std::sync::Mutex<Vec<RainColumn>>> = OnceLock::new();

pub(super) fn rain_state() -> std::sync::MutexGuard<'static, Vec<RainColumn>> {
    RAIN_STATE
        .get_or_init(|| {
            // Simple LCG-based pseudo-random initialization
            let mut seed: u32 = 0xDEAD_BEEF;
            let lcg = |s: &mut u32| -> u32 {
                *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                *s
            };

            let col_w = 14u32;
            let n_cols = 480 / col_w as usize + 1;
            let cols = (0..n_cols)
                .map(|_| {
                    let head = -((lcg(&mut seed) % 480) as i32);
                    let speed = 4 + (lcg(&mut seed) % 8) as i32;
                    let trail = 80 + (lcg(&mut seed) % 200) as i32;
                    let chars_len = (480 / 14) as usize + 2;
                    let chars = (0..chars_len)
                        .map(|_| (lcg(&mut seed) % MATRIX_CHARS.len() as u32) as usize)
                        .collect();
                    RainColumn {
                        head,
                        speed,
                        trail,
                        chars,
                        phase: lcg(&mut seed),
                    }
                })
                .collect();
            std::sync::Mutex::new(cols)
        })
        .lock()
        .unwrap()
}

/// Reset the Matrix rain state (called when switching to this mode).
pub fn reset_matrix_rain() {
    let mut state = rain_state();
    let mut seed: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x1234_5678, |d| d.subsec_nanos());
    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        *s
    };
    for col in state.iter_mut() {
        col.head = -((lcg(&mut seed) % 400) as i32);
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
        .map_or(0, |d| d.subsec_nanos() ^ (d.as_secs() as u32));
    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        *s
    };

    for (ci, col) in cols.iter_mut().enumerate() {
        let x = ci as i32 * COL_W;
        if x >= 480 {
            break;
        }

        // Advance column
        col.head += col.speed;
        if col.head - col.trail > 480 {
            // Reset column
            col.head = -((lcg(&mut seed) % 100) as i32);
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
        let end_row = col.head.min(480 + CHAR_H);

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
            if (0..480 - CHAR_H).contains(&py) {
                draw_text_mut(
                    img_ref,
                    color,
                    x,
                    py,
                    PxScale::from(CHAR_H as f32),
                    &bold,
                    &ch_str,
                );
            }
            py += CHAR_H;
            char_row += 1;
        }
    }

    DynamicImage::ImageRgb8(img_ref.clone())
}

// ─── Fireworks Screensaver ─────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Firework {
    // Burst origin
    bx: f32,
    by: f32,
    // List of particles: (x, y, vx, vy, life, max_life, color_idx)
    particles: Vec<(f32, f32, f32, f32, u8, u8, u8)>,
    // Rocket: (x, y, vy, active)
    rocket: Option<(f32, f32, f32)>,
    trail: Vec<(f32, f32)>,
}

pub(super) static FIREWORKS: std::sync::OnceLock<std::sync::Mutex<Vec<Firework>>> =
    std::sync::OnceLock::new();
pub(super) static FIREWORKS_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_fireworks() -> DynamicImage {
    let tick = {
        let m = FIREWORKS_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let mut fws = FIREWORKS
        .get_or_init(|| std::sync::Mutex::new(Vec::new()))
        .lock()
        .unwrap();

    // LCG pseudo-random
    let lcg = |s: u64| -> u64 {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
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
    for p in img.pixels_mut() {
        *p = BG;
    }

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
                    fw.particles
                        .push((fw.bx, fw.by, vx, vy, max_life, max_life, col_idx));
                }
            } else {
                fw.rocket = Some((rx, ny, rvy));
                fw.trail.push((rx, ny));
                if fw.trail.len() > 8 {
                    fw.trail.remove(0);
                }
                // Draw rocket trail
                for (ti, &(tx, ty)) in fw.trail.iter().enumerate() {
                    let alpha = ti as u8 * 30 + 10;
                    let px = tx as i32;
                    let py = ty as i32;
                    if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                        img.put_pixel(px as u32, py as u32, Rgb([alpha, alpha, alpha / 2]));
                    }
                }
                // Draw rocket head
                let rx2 = rx as i32;
                let ry2 = ny as i32;
                if rx2 >= 0 && ry2 >= 0 && (rx2 as u32) < 480 && (ry2 as u32) < 480 {
                    img.put_pixel(rx2 as u32, ry2 as u32, YELLOW);
                }
            }
        }

        // Update and draw particles
        let mut alive = false;
        for p in &mut fw.particles {
            if p.4 == 0 {
                continue;
            }
            alive = true;
            p.0 += p.2; // x += vx
            p.1 += p.3; // y += vy
            p.3 += 0.08; // gravity
            p.4 = p.4.saturating_sub(1);

            let px = p.0 as i32;
            let py = p.1 as i32;
            if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                // Fade by remaining life
                let life_frac = p.4 as f32 / p.5 as f32;
                let col = colors[p.6 as usize % colors.len()];
                let r = (col.0[0] as f32 * life_frac) as u8;
                let g = (col.0[1] as f32 * life_frac) as u8;
                let b = (col.0[2] as f32 * life_frac) as u8;
                img.put_pixel(px as u32, py as u32, Rgb([r, g, b]));
                // 1-pixel glow
                for (ddx, ddy) in &[(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let gx = px + ddx;
                    let gy = py + ddy;
                    if gx >= 0 && gy >= 0 && (gx as u32) < 480 && (gy as u32) < 480 {
                        let gr = (col.0[0] as f32 * life_frac * 0.4) as u8;
                        let gg = (col.0[1] as f32 * life_frac * 0.4) as u8;
                        let gb = (col.0[2] as f32 * life_frac * 0.4) as u8;
                        let existing = *img.get_pixel(gx as u32, gy as u32);
                        img.put_pixel(
                            gx as u32,
                            gy as u32,
                            Rgb([
                                existing.0[0].saturating_add(gr),
                                existing.0[1].saturating_add(gg),
                                existing.0[2].saturating_add(gb),
                            ]),
                        );
                    }
                }
            }
        }
        if fw.rocket.is_none() && !alive {
            remove = true;
        }
        if remove {
            to_remove.push(idx);
        }
    }

    for idx in to_remove.into_iter().rev() {
        if idx < fws.len() {
            fws.remove(idx);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Doom Fire ────────────────────────────────────────────────────────────────

pub(super) static DOOM_FIRE_BUF: std::sync::OnceLock<std::sync::Mutex<Vec<u8>>> =
    std::sync::OnceLock::new();
pub(super) static DOOM_FIRE_SEED: std::sync::OnceLock<std::sync::Mutex<u32>> =
    std::sync::OnceLock::new();

/// Classic Doom-PSX fire simulation.
/// 120×120 intensity cells (each rendered 4×4 px) with an LCG-driven decay
/// and slight lateral spread for an authentic flicker.
pub fn render_doom_fire() -> DynamicImage {
    const FIRE_W: usize = 120;
    const FIRE_H: usize = 120;

    let buf_m = DOOM_FIRE_BUF.get_or_init(|| {
        let mut v = vec![0u8; FIRE_W * FIRE_H];
        for x in 0..FIRE_W {
            v[(FIRE_H - 1) * FIRE_W + x] = 36;
        }
        std::sync::Mutex::new(v)
    });
    let seed_m = DOOM_FIRE_SEED.get_or_init(|| std::sync::Mutex::new(0xDEAD_BEEFu32));

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };

    let mut seed = seed_m.lock().unwrap();
    let mut buf = buf_m.lock().unwrap();

    // Propagate fire upward (skip the seed row)
    for y in 0..(FIRE_H - 1) {
        for x in 0..FIRE_W {
            let decay = (lcg(&mut seed) % 3) as u8; // 0, 1, or 2
            let shift = (lcg(&mut seed) % 3) as usize; // 0, 1, or 2 left-shift
            let src_x = if shift > 0 && x >= shift {
                x - shift
            } else {
                x
            };
            let below = buf[(y + 1) * FIRE_W + src_x];
            buf[y * FIRE_W + x] = below.saturating_sub(decay);
        }
    }

    // Palette: 37 shades, black → dark red → orange → yellow → white
    let pal: [Rgb<u8>; 37] = [
        Rgb([0, 0, 0]),
        Rgb([7, 0, 0]),
        Rgb([22, 0, 0]),
        Rgb([40, 0, 0]),
        Rgb([58, 0, 0]),
        Rgb([80, 0, 0]),
        Rgb([103, 0, 0]),
        Rgb([128, 0, 0]),
        Rgb([148, 0, 0]),
        Rgb([168, 0, 0]),
        Rgb([188, 6, 0]),
        Rgb([208, 12, 0]),
        Rgb([220, 22, 0]),
        Rgb([232, 32, 0]),
        Rgb([240, 48, 0]),
        Rgb([248, 64, 0]),
        Rgb([250, 80, 0]),
        Rgb([252, 96, 0]),
        Rgb([252, 112, 0]),
        Rgb([252, 128, 0]),
        Rgb([252, 148, 0]),
        Rgb([252, 164, 0]),
        Rgb([252, 180, 0]),
        Rgb([252, 196, 0]),
        Rgb([252, 212, 0]),
        Rgb([252, 224, 0]),
        Rgb([252, 236, 0]),
        Rgb([252, 248, 0]),
        Rgb([252, 252, 60]),
        Rgb([252, 252, 120]),
        Rgb([252, 252, 160]),
        Rgb([252, 252, 200]),
        Rgb([252, 252, 220]),
        Rgb([255, 255, 230]),
        Rgb([255, 255, 240]),
        Rgb([255, 255, 248]),
        Rgb([255, 255, 255]),
    ];

    let mut img = RgbImage::new(480, 480);
    for cy in 0..FIRE_H {
        for cx in 0..FIRE_W {
            let col = pal[buf[cy * FIRE_W + cx].min(36) as usize];
            let px0 = (cx as u32) * 4;
            let py0 = (cy as u32) * 4;
            for dy in 0..4u32 {
                for dx in 0..4u32 {
                    img.put_pixel(px0 + dx, py0 + dy, col);
                }
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Aurora Borealis ──────────────────────────────────────────────────────────

pub(super) static AURORA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Northern-lights simulation — 5 animated sine-wave bands with Gaussian
/// falloff and additive blending, reflected in a still-water horizon.
pub fn render_aurora() -> DynamicImage {
    let tick = {
        let mut t = AURORA_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *t += 1;
        *t
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // ── Sky gradient ──────────────────────────────────────────────────────────
    for py in 0..480u32 {
        let frac = py as f32 / 480.0;
        let r = (4.0 + frac * 6.0) as u8;
        let g = (4.0 + frac * 8.0) as u8;
        let b = (16.0 + frac * 20.0) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // ── Stars (deterministic positions, twinkling) ────────────────────────────
    {
        let mut seed = 0x1234_5678u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s
        };
        for _ in 0..160u32 {
            let sx = lcg(&mut seed) % 480;
            let sy = lcg(&mut seed) % 310;
            let base = 120 + (lcg(&mut seed) % 136) as u8;
            let phase = (lcg(&mut seed) % 1_000) as f32 / 1_000.0;
            let br = (base as f32
                * (0.65 + 0.35 * ((tf * 0.06 + phase * std::f32::consts::TAU).sin())))
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
        let lf = i as f32;
        let base = 35.0 + lf * 42.0; // vertical position
        let thick = 48.0 + lf * 8.0; // band half-thickness
        let ph = lf * 1.83; // phase offset

        for px in 0..480u32 {
            let xf = px as f32;

            // Undulating centre line
            let cy = base
                + (xf * 0.011 + tf * 0.017 + ph).sin() * 26.0
                + (xf * 0.023 - tf * 0.011 + ph).sin() * 16.0
                + (xf * 0.0055 + tf * 0.007 + ph * 0.7).sin() * 9.0;

            // Local thickness ripple
            let loc_th = thick * (0.7 + 0.3 * ((xf * 0.014 + tf * 0.019 + ph).sin()));

            // Column shimmer (computed once per column)
            let shimmer = 0.65 + 0.35 * ((xf * 0.028 + tf * 0.038 + ph).sin());

            for py in 0..HORIZON_Y {
                let yf = py as f32;
                let dy = (yf - cy).abs();
                if dy >= loc_th {
                    continue;
                }

                // Gaussian falloff
                let t = dy / loc_th;
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
    for px in 0..480u32 {
        img.put_pixel(px, HORIZON_Y, Rgb([28, 48, 38]));
    }

    // ── Water reflection (mirror + fade + darken) ────────────────────────────
    for py in (HORIZON_Y + 1)..480 {
        let mirror = HORIZON_Y.saturating_sub(py - HORIZON_Y);
        let fade = 1.0 - ((py - HORIZON_Y) as f32 / 140.0).min(1.0);
        for px in 0..480u32 {
            let src = img.get_pixel(px, mirror);
            img.put_pixel(
                px,
                py,
                Rgb([
                    (src[0] as f32 * fade * 0.50) as u8,
                    (src[1] as f32 * fade * 0.62) as u8,
                    (src[2] as f32 * fade * 0.72) as u8,
                ]),
            );
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Snowfall ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Snowflake {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: u32,
    alpha: f32,
    wobble_phase: f32,
}

pub(super) struct SnowState {
    flakes: Vec<Snowflake>,
    seed: u32,
    tick: u64,
    wind: f32, // horizontal wind component
}

pub(super) static SNOW_STATE: std::sync::OnceLock<std::sync::Mutex<SnowState>> =
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
        let flakes = (0..N_FLAKES)
            .map(|_| Snowflake {
                x: lcg(&mut seed) * 480.0,
                y: lcg(&mut seed) * 480.0, // start anywhere
                vx: (lcg(&mut seed) - 0.5) * 0.6,
                vy: 0.8 + lcg(&mut seed) * 2.0,
                size: 1 + (lcg(&mut seed) * 3.0) as u32,
                alpha: 0.5 + lcg(&mut seed) * 0.5,
                wobble_phase: lcg(&mut seed) * std::f32::consts::TAU,
            })
            .collect();
        std::sync::Mutex::new(SnowState {
            flakes,
            seed: 0xFACE_C0DEu32,
            tick: 0,
            wind: 0.0,
        })
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
    for flake in &mut st.flakes {
        flake.wobble_phase += 0.04;
        flake.x += flake.vx + wind_snap + flake.wobble_phase.sin() * 0.4;
        flake.y += flake.vy;

        // Wrap horizontally
        if flake.x < -5.0 {
            flake.x += 490.0;
        }
        if flake.x > 485.0 {
            flake.x -= 490.0;
        }

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
        let r = (8.0 + frac * 12.0) as u8;
        let g = (10.0 + frac * 15.0) as u8;
        let b = (24.0 + frac * 28.0) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Draw flakes
    for flake in &st.flakes {
        let fx = flake.x as i32;
        let fy = flake.y as i32;
        let sz = flake.size as i32;
        let br = (flake.alpha * 255.0) as u8;
        // Core pixel(s)
        for dy in -sz..=sz {
            for dx in -sz..=sz {
                // Circle approximation
                if dx * dx + dy * dy > sz * sz + sz {
                    continue;
                }
                let px = fx + dx;
                let py = fy + dy;
                if (0..480).contains(&px) && (0..480).contains(&py) {
                    // Additive blending with existing pixel
                    let existing = img.get_pixel(px as u32, py as u32);
                    let dist_frac = ((dx * dx + dy * dy) as f32).sqrt() / sz as f32;
                    let alpha = (1.0 - dist_frac).max(0.0) * flake.alpha;
                    let nr = (existing[0] as f32 + br as f32 * alpha).min(255.0) as u8;
                    let ng = (existing[1] as f32 + br as f32 * alpha).min(255.0) as u8;
                    let nb = (existing[2] as f32 + br as f32 * alpha * 0.9 + 15.0 * alpha)
                        .min(255.0) as u8;
                    img.put_pixel(px as u32, py as u32, Rgb([nr, ng, nb]));
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Confetti ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Confetto {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    rot: f32,
    rot_v: f32,
    w: f32,
    h: f32, // dimensions
    color_idx: usize,
}

pub(super) struct ConfettiState {
    pieces: Vec<Confetto>,
    seed: u32,
    tick: u64,
}

pub(super) static CONFETTI_STATE: std::sync::OnceLock<std::sync::Mutex<ConfettiState>> =
    std::sync::OnceLock::new();

/// Celebration confetti — 120 coloured rectangular pieces drifting down
/// with rotation, slight wind, and respawning from the top.
pub fn render_confetti() -> DynamicImage {
    const N: usize = 120;

    let m = CONFETTI_STATE.get_or_init(|| {
        let mut seed = 0xFEED_FAB1u32;
        let lcg = |s: &mut u32| -> f32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*s as f32) / u32::MAX as f32
        };
        let pieces = (0..N)
            .map(|i| Confetto {
                x: lcg(&mut seed) * 480.0,
                y: lcg(&mut seed) * 480.0,
                vx: (lcg(&mut seed) - 0.5) * 2.0,
                vy: 1.5 + lcg(&mut seed) * 2.5,
                rot: lcg(&mut seed) * std::f32::consts::TAU,
                rot_v: (lcg(&mut seed) - 0.5) * 0.15,
                w: 6.0 + lcg(&mut seed) * 10.0,
                h: 4.0 + lcg(&mut seed) * 5.0,
                color_idx: i % 7,
            })
            .collect();
        std::sync::Mutex::new(ConfettiState {
            pieces,
            seed: 0x00AB_CDEF_u32,
            tick: 0,
        })
    });

    let mut st = m.lock().unwrap();
    st.tick += 1;
    let tf = st.tick as f32;

    let wind = (tf * 0.008).sin() * 0.8;
    let mut seed = st.seed;
    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    for piece in &mut st.pieces {
        piece.x += piece.vx + wind;
        piece.y += piece.vy;
        piece.rot += piece.rot_v;
        if piece.x < -20.0 {
            piece.x += 520.0;
        }
        if piece.x > 500.0 {
            piece.x -= 520.0;
        }
        if piece.y > 490.0 {
            piece.y = -10.0;
            piece.x = lcg(&mut seed) * 480.0;
            piece.vx = (lcg(&mut seed) - 0.5) * 2.0;
            piece.vy = 1.5 + lcg(&mut seed) * 2.5;
        }
    }
    st.seed = seed;

    let cols = [BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for piece in &st.pieces {
        let col = cols[piece.color_idx % cols.len()];
        let cos = piece.rot.cos();
        let sin = piece.rot.sin();
        let hw = piece.w / 2.0;
        let hh = piece.h / 2.0;

        // Draw a rotated rectangle as 4 filled scanlines
        let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
        let world: Vec<(f32, f32)> = corners
            .iter()
            .map(|&(lx, ly)| (piece.x + lx * cos - ly * sin, piece.y + lx * sin + ly * cos))
            .collect();

        // Rasterise: find bounding box, test point-in-rotated-rect
        let min_x = world.iter().map(|v| v.0).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_x = world
            .iter()
            .map(|v| v.0)
            .fold(f32::MIN, f32::max)
            .min(479.0) as u32;
        let min_y = world.iter().map(|v| v.1).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_y = world
            .iter()
            .map(|v| v.1)
            .fold(f32::MIN, f32::max)
            .min(479.0) as u32;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                // Transform to local coords
                let dx = px as f32 - piece.x;
                let dy = py as f32 - piece.y;
                let lx = dx * cos + dy * sin;
                let ly = -dx * sin + dy * cos;
                if lx.abs() <= hw && ly.abs() <= hh {
                    img.put_pixel(px, py, col);
                }
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Neon Grid (Tron-style) ───────────────────────────────────────────────────

pub(super) static NEON_GRID_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Tron-inspired neon grid — glowing lines, pulsing nodes, travelling colour
/// pulses along the wire-frame.
pub fn render_neon_grid() -> DynamicImage {
    let tick = {
        let mut t = NEON_GRID_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *t += 1;
        *t
    };
    let tf = tick as f32;

    const COLS: u32 = 10;
    const ROWS: u32 = 10;
    let cw = 480.0 / COLS as f32;
    let ch = 480.0 / ROWS as f32;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([3, 5, 15]);
    } // near-black blue

    let line_cols: [Rgb<u8>; 4] = [
        Rgb([0, 180, 255]),  // cyan
        Rgb([0, 255, 180]),  // teal
        Rgb([180, 80, 255]), // purple
        Rgb([0, 220, 120]),  // green
    ];

    // Draw horizontal lines
    for row in 0..=ROWS {
        let y = (row as f32 * ch) as u32;
        let col_idx = (row as usize + (tf * 0.02) as usize) % line_cols.len();
        let pulse = (tf * 0.05 + row as f32 * 0.7).sin() * 0.4 + 0.6;
        let lc = line_cols[col_idx];
        let col = Rgb([
            (lc[0] as f32 * pulse) as u8,
            (lc[1] as f32 * pulse) as u8,
            (lc[2] as f32 * pulse) as u8,
        ]);
        for x in 0..480u32 {
            if y < 480 {
                img.put_pixel(x, y, col);
                // Glow row above/below
                if y > 0 {
                    let g = Rgb([
                        (lc[0] as f32 * pulse * 0.25) as u8,
                        (lc[1] as f32 * pulse * 0.25) as u8,
                        (lc[2] as f32 * pulse * 0.25) as u8,
                    ]);
                    let e = img.get_pixel(x, y - 1);
                    img.put_pixel(
                        x,
                        y - 1,
                        Rgb([
                            (e[0] as u32 + g[0] as u32).min(255) as u8,
                            (e[1] as u32 + g[1] as u32).min(255) as u8,
                            (e[2] as u32 + g[2] as u32).min(255) as u8,
                        ]),
                    );
                }
            }
        }
    }

    // Draw vertical lines
    for col in 0..=COLS {
        let x = (col as f32 * cw) as u32;
        let col_idx = (col as usize * 2 + (tf * 0.015) as usize) % line_cols.len();
        let pulse = (tf * 0.06 + col as f32 * 0.5).sin() * 0.4 + 0.6;
        let lc = line_cols[col_idx];
        let c = Rgb([
            (lc[0] as f32 * pulse) as u8,
            (lc[1] as f32 * pulse) as u8,
            (lc[2] as f32 * pulse) as u8,
        ]);
        for y in 0..480u32 {
            if x < 480 {
                img.put_pixel(x, y, c);
            }
        }
    }

    // Bright node dots at intersections
    for row in 0..=ROWS {
        for col in 0..=COLS {
            let nx = (col as f32 * cw) as i32;
            let ny = (row as f32 * ch) as i32;
            let node_pulse =
                ((tf * 0.08 + (row + col * 3) as f32 * 0.4).sin() * 0.5 + 0.5).powf(2.0);
            let node_col = line_cols[(row as usize + col as usize) % line_cols.len()];
            let nc = Rgb([
                (node_col[0] as f32 * node_pulse + 200.0 * node_pulse) as u8,
                (node_col[1] as f32 * node_pulse + 200.0 * node_pulse) as u8,
                (node_col[2] as f32 * node_pulse + 200.0 * node_pulse) as u8,
            ]);
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    if dx * dx + dy * dy > 5 {
                        continue;
                    }
                    let px = nx + dx;
                    let py = ny + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        img.put_pixel(px as u32, py as u32, nc);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_rain_glass  — raindrops running down a glass pane
// ─────────────────────────────────────────────────────────────────────────────
pub(super) struct Raindrop {
    x: f32,
    y: f32,
    vy: f32,
    size: f32,
    trail_len: f32,
    wobble_seed: f32,
    alive: bool,
}

pub(super) static RAIN_GLASS_STATE: std::sync::OnceLock<
    std::sync::Mutex<(Vec<Raindrop>, u32, u64)>,
> = std::sync::OnceLock::new();

pub fn render_rain_glass() -> DynamicImage {
    let lcg_f = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    let state =
        RAIN_GLASS_STATE.get_or_init(|| std::sync::Mutex::new((Vec::new(), 0x5A15_5EEDu32, 0u64)));
    let mut st = state.lock().unwrap();
    let (ref mut drops, ref mut seed, ref mut tick) = *st;
    *tick += 1;
    let tf = *tick as f32;

    let mut img = RgbImage::new(480, 480);
    // Dark blue-grey gradient background
    for py in 0u32..480 {
        let t = py as f32 / 480.0;
        let r = (0x24_u32 + ((0x1e - 0x24) as f32 * t) as u32) as u8;
        let g = (0x28_u32 + ((0x1e - 0x28) as f32 * t) as u32) as u8;
        let b = (0x3c_u32 + ((0x2e - 0x3c) as f32 * t) as u32) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Spawn new drops
    if *tick % 3 == 0 && drops.len() < 80 {
        let x = lcg_f(seed) * 460.0 + 10.0;
        let size = 2.0 + lcg_f(seed) * 4.0;
        let vy = 0.6 + lcg_f(seed) * 1.8;
        let trail_len = 20.0 + lcg_f(seed) * 60.0;
        let wobble_seed = lcg_f(seed) * 100.0;
        drops.push(Raindrop {
            x,
            y: 0.0,
            vy,
            size,
            trail_len,
            wobble_seed,
            alive: true,
        });
    }

    // Update and draw drops
    for drop in drops.iter_mut() {
        if !drop.alive {
            continue;
        }
        drop.y += drop.vy;
        if drop.y > 490.0 {
            drop.alive = false;
            continue;
        }

        let wobble = ((drop.y * 0.03 + drop.wobble_seed + tf * 0.05).sin()) * 2.0;
        let x = drop.x + wobble;
        let y = drop.y;

        // Trail
        let trail = drop.trail_len.min(y);
        let drop_col = Rgb([0x89u8, 0xb4u8, 0xfau8]);
        for i in 0..(trail as i32) {
            let ty = y - i as f32;
            let tx = (x + wobble * (1.0 - i as f32 / trail)) as i32;
            let alpha = ((1.0 - i as f32 / trail) * 160.0) as u8;
            if (0..480).contains(&tx) && ty >= 0.0 && (ty as u32) < 480 {
                let fa = alpha as f32 / 255.0;
                let bg = *img.get_pixel(tx as u32, ty as u32);
                img.put_pixel(
                    tx as u32,
                    ty as u32,
                    Rgb([
                        (bg[0] as f32 * (1.0 - fa) + drop_col[0] as f32 * fa) as u8,
                        (bg[1] as f32 * (1.0 - fa) + drop_col[1] as f32 * fa) as u8,
                        (bg[2] as f32 * (1.0 - fa) + drop_col[2] as f32 * fa) as u8,
                    ]),
                );
            }
        }

        // Drop body oval
        let rx = drop.size;
        let ry = drop.size * 1.6;
        let body_col = Rgb([0xb4u8, 0xdeu8, 0xffu8]);
        for dy in (-(ry as i32))..=(ry as i32) {
            for dx in (-(rx as i32))..=(rx as i32) {
                let d = (dx as f32 / rx).powi(2) + (dy as f32 / ry).powi(2);
                if d <= 1.0 {
                    let alpha = ((1.0 - d) * 180.0) as u8;
                    let px_i = x as i32 + dx;
                    let py_i = y as i32 + dy;
                    if (0..480).contains(&px_i) && (0..480).contains(&py_i) {
                        let fa = alpha as f32 / 255.0;
                        let bg = *img.get_pixel(px_i as u32, py_i as u32);
                        img.put_pixel(
                            px_i as u32,
                            py_i as u32,
                            Rgb([
                                (bg[0] as f32 * (1.0 - fa) + body_col[0] as f32 * fa) as u8,
                                (bg[1] as f32 * (1.0 - fa) + body_col[1] as f32 * fa) as u8,
                                (bg[2] as f32 * (1.0 - fa) + body_col[2] as f32 * fa) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    drops.retain(|d| d.alive);

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_fire_text  — "ttlcd" burning in Doom-style fire
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static FIRE_TEXT_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<u8>, u32, u64)>> =
    std::sync::OnceLock::new();

pub fn render_fire_text() -> DynamicImage {
    const FIRE_W: usize = 120;
    const FIRE_H: usize = 120;
    const CELL: usize = 4;

    let state = FIRE_TEXT_STATE
        .get_or_init(|| std::sync::Mutex::new((vec![0u8; FIRE_W * FIRE_H], 0xFEED_FAC3u32, 0u64)));
    let mut st = state.lock().unwrap();
    let (ref mut buf, ref mut seed, ref mut tick) = *st;
    *tick += 1;

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };

    // Every 60 ticks re-seed the text mask into the bottom rows
    if *tick % 4 == 0 {
        // Seed row = bottom row of fire buffer
        // Use rows 110-119 as heat source from "text pixels"
        // We'll stamp a simple bitmap for "ttlcd" at row 115
        for x in 0..FIRE_W {
            buf[119 * FIRE_W + x] = 36;
        }
        // Row 110-118: text mask  — simplified rasterised glyphs for "ttlcd"
        // 6 chars × 6px wide = 36px wide, centred at 42..77
        let glyphs: &[&[u8]] = &[
            // 't'
            &[
                0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1,
                0, 0,
            ],
            // 't' again
            &[
                0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1,
                0, 0,
            ],
            // 'l'
            &[
                0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1,
                0, 0,
            ],
            // 'c'
            &[
                0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1,
                0, 0,
            ],
            // 'd'
            &[
                0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1,
                0, 0,
            ],
        ];
        let start_x = 15usize; // start column
        let start_row = 110usize;
        for (ci, glyph) in glyphs.iter().enumerate() {
            for row in 0..5usize {
                for col in 0..6usize {
                    if glyph[row * 6 + col] == 1 {
                        let fx = start_x + ci * 7 + col;
                        let fy = start_row + row;
                        if fx < FIRE_W && fy < FIRE_H {
                            buf[fy * FIRE_W + fx] = 36;
                        }
                    }
                }
            }
        }
    }

    // Propagate fire upward
    for y in 1..FIRE_H {
        for x in 0..FIRE_W {
            let below = buf[(FIRE_H - 1 - y + 1) * FIRE_W + x] as u32;
            let decay = lcg(seed) % 3;
            let shift = (lcg(seed) % 3) as i32 - 1;
            let nx = ((x as i32 + shift).rem_euclid(FIRE_W as i32)) as usize;
            let new_val = below.saturating_sub(decay);
            buf[(FIRE_H - 1 - y) * FIRE_W + nx] = new_val as u8;
        }
    }

    // Render
    let palette: [[u8; 3]; 37] = [
        [0x1e, 0x1e, 0x2e],
        [0x28, 0x15, 0x10],
        [0x3c, 0x1a, 0x08],
        [0x55, 0x20, 0x04],
        [0x6e, 0x26, 0x02],
        [0x88, 0x2d, 0x01],
        [0xa0, 0x35, 0x00],
        [0xba, 0x3e, 0x00],
        [0xd3, 0x48, 0x00],
        [0xec, 0x54, 0x00],
        [0xf9, 0x62, 0x00],
        [0xfa, 0x72, 0x00],
        [0xfb, 0x83, 0x00],
        [0xfc, 0x95, 0x00],
        [0xfd, 0xa8, 0x00],
        [0xfe, 0xbb, 0x00],
        [0xff, 0xce, 0x00],
        [0xff, 0xd8, 0x08],
        [0xff, 0xe2, 0x14],
        [0xff, 0xea, 0x22],
        [0xff, 0xf0, 0x32],
        [0xff, 0xf4, 0x44],
        [0xff, 0xf7, 0x58],
        [0xff, 0xf9, 0x6e],
        [0xff, 0xfb, 0x86],
        [0xff, 0xfc, 0x9e],
        [0xff, 0xfd, 0xb6],
        [0xff, 0xfe, 0xcc],
        [0xff, 0xfe, 0xe0],
        [0xff, 0xff, 0xf0],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
        [0xff, 0xff, 0xff],
    ];

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for fy in 0..FIRE_H {
        for fx in 0..FIRE_W {
            let heat = buf[fy * FIRE_W + fx] as usize;
            if heat == 0 {
                continue;
            }
            let col = palette[heat.min(36)];
            for cy in 0..CELL {
                for cx in 0..CELL {
                    let px = fx * CELL + cx;
                    let py = fy * CELL + cy;
                    if px < 480 && py < 480 {
                        img.put_pixel(px as u32, py as u32, Rgb(col));
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_disco_floor  — flashing coloured tiles like a disco dance floor
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static DISCO_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_disco_floor() -> DynamicImage {
    let tick = {
        let m = DISCO_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    const COLS: u32 = 8;
    const ROWS: u32 = 8;
    const CELL_W: u32 = 60;
    const CELL_H: u32 = 60;
    const GAP: u32 = 2;

    let tf = tick as f32 * 0.08;

    let disco_cols: [[u8; 3]; 9] = [
        [BLUE[0], BLUE[1], BLUE[2]],
        [RED[0], RED[1], RED[2]],
        [GREEN[0], GREEN[1], GREEN[2]],
        [YELLOW[0], YELLOW[1], YELLOW[2]],
        [MAUVE[0], MAUVE[1], MAUVE[2]],
        [TEAL[0], TEAL[1], TEAL[2]],
        [PEACH[0], PEACH[1], PEACH[2]],
        [0xff, 0xff, 0xff],
        [0x89, 0xdc, 0xeb],
    ];

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x11, 0x11, 0x18]);
    }

    for row in 0..ROWS {
        for col in 0..COLS {
            // Each tile has a phase offset
            let phase = row as f32 * 0.9 + col as f32 * 0.7 + tf;
            let brightness = (phase.sin() * 0.5 + 0.5).powi(2);

            // Colour cycles through palette based on tile position and time
            let ci = ((row * COLS + col) as usize + (tick / 12) as usize + row as usize)
                % disco_cols.len();
            let base = disco_cols[ci];

            let x0 = col * CELL_W + GAP;
            let y0 = row * CELL_H + GAP;
            let x1 = x0 + CELL_W - GAP * 2;
            let y1 = y0 + CELL_H - GAP * 2;

            for py in y0..y1 {
                for px in x0..x1 {
                    // Spotlight vignette within tile
                    let tcx = (px - x0) as f32 / (CELL_W - GAP * 2 - 1) as f32 - 0.5;
                    let tcy = (py - y0) as f32 / (CELL_H - GAP * 2 - 1) as f32 - 0.5;
                    let vignette = (1.0 - (tcx * tcx + tcy * tcy) * 4.0).max(0.0);
                    let br = brightness * vignette;

                    // Floor reflection: brighter at centre-bottom of each cell
                    let reflect = (1.0 - (py - y0) as f32 / (CELL_H - GAP * 2) as f32) * 0.3;
                    let final_br = (br + reflect).min(1.0);

                    img.put_pixel(
                        px,
                        py,
                        Rgb([
                            (base[0] as f32 * final_br) as u8,
                            (base[1] as f32 * final_br) as u8,
                            (base[2] as f32 * final_br) as u8,
                        ]),
                    );
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_city_skyline  — night city silhouette with twinkling windows
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static CITY_TICK: std::sync::OnceLock<std::sync::Mutex<(Vec<u32>, u64)>> =
    std::sync::OnceLock::new();

pub fn render_city_skyline() -> DynamicImage {
    const N_BUILDINGS: usize = 20;
    const GROUND_Y: u32 = 380;
    const SKY_H: u32 = GROUND_Y;

    let state = CITY_TICK.get_or_init(|| {
        // Generate building widths and heights using a deterministic sequence
        let mut s = 0xC175_B1D6u32;
        let lcg = |r: &mut u32| -> u32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *r
        };
        let mut buildings = Vec::with_capacity(N_BUILDINGS);
        for _ in 0..N_BUILDINGS {
            let w = 16 + lcg(&mut s) % 32;
            let h = 60 + lcg(&mut s) % 200;
            buildings.push(w << 16 | h); // pack width|height
        }
        std::sync::Mutex::new((buildings, 0u64))
    });
    let mut st = state.lock().unwrap();
    let (ref buildings, ref mut tick) = *st;
    *tick += 1;
    let tf = *tick as f32;

    let mut img = RgbImage::new(480, 480);

    // Sky gradient: deep navy to lighter blue
    for py in 0..480u32 {
        let t = py as f32 / SKY_H as f32;
        let r = (0x11_u32 + ((0x1e - 0x11) as f32 * t) as u32) as u8;
        let g = (0x15_u32 + ((0x1e - 0x15) as f32 * t) as u32) as u8;
        let b = (0x28_u32 + ((0x2e - 0x28) as f32 * t) as u32) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Stars in the sky
    let mut star_seed = 0xA5A5_A5A5u32;
    let star_lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };
    for _ in 0..200 {
        let sx = star_lcg(&mut star_seed) % 480;
        let sy = star_lcg(&mut star_seed) % (SKY_H - 30);
        let twinkle = (sx as f32 * 0.17 + sy as f32 * 0.13 + tf * 0.04).sin() * 0.4 + 0.6;
        let br = (200.0 * twinkle) as u8;
        if sy < 480 {
            img.put_pixel(sx, sy, Rgb([br, br, br]));
        }
    }

    // Moon
    let moon_x = 380i32;
    let moon_y = 60i32;
    let moon_r = 28i32;
    for dy in -moon_r..=moon_r {
        for dx in -moon_r..=moon_r {
            if dx * dx + dy * dy <= moon_r * moon_r {
                let px_i = moon_x + dx;
                let py_i = moon_y + dy;
                if (0..480).contains(&px_i) && (0..480).contains(&py_i) {
                    // Moon glow falloff
                    let d = ((dx * dx + dy * dy) as f32).sqrt() / moon_r as f32;
                    let br = (255.0 * (1.0 - d * 0.3)) as u8;
                    img.put_pixel(
                        px_i as u32,
                        py_i as u32,
                        Rgb([br, br, (br as u32 * 90 / 100) as u8]),
                    );
                }
            }
        }
    }

    // Draw buildings
    let total_w: u32 = buildings.iter().map(|&b| (b >> 16) + 4).sum();
    let scale_x = 480.0 / total_w as f32;
    let mut x = 0u32;
    let mut win_seed = 0xF00D_CAFEu32;

    for &bld in buildings {
        let bw = ((bld >> 16) as f32 * scale_x) as u32;
        let bh = (bld & 0xFFFF).min(GROUND_Y - 10);
        let by = GROUND_Y - bh;

        // Building silhouette (dark grey)
        for py in by..GROUND_Y {
            for px in x..(x + bw).min(480) {
                let shade = if py < by + 2 || px == x || px == (x + bw - 1).min(479) {
                    Rgb([0x35, 0x37, 0x4a]) // edge highlight
                } else {
                    Rgb([0x24, 0x27, 0x3a])
                };
                img.put_pixel(px, py, shade);
            }
        }

        // Windows (small rectangles)
        let win_w = 6u32;
        let win_h = 8u32;
        let win_gap = 4u32;
        let mut wy = by + 8;
        while wy + win_h < GROUND_Y - 5 {
            let mut wx = x + 5;
            while wx + win_w < x + bw - 3 {
                let on = star_lcg(&mut win_seed) % 3 != 0;
                let flicker = (wx as f32 * 0.31 + wy as f32 * 0.17 + tf * 0.02).sin();
                let bright = on && flicker > -0.8;
                if bright {
                    let win_col_idx = star_lcg(&mut win_seed) % 4;
                    let win_col: Rgb<u8> = match win_col_idx {
                        0 => Rgb([0xf9, 0xe2, 0xaf]), // warm yellow
                        1 => Rgb([0xa0, 0xc8, 0xff]), // cool blue-white
                        2 => Rgb([0xff, 0xe0, 0xa0]), // warm white
                        _ => Rgb([0xe0, 0xf0, 0xff]), // cool white
                    };
                    for wpy in wy..(wy + win_h) {
                        for wpx in wx..(wx + win_w) {
                            if wpx < 480 && wpy < 480 {
                                img.put_pixel(wpx, wpy, win_col);
                            }
                        }
                    }
                }
                wx += win_w + win_gap;
            }
            wy += win_h + win_gap;
        }

        x += bw + 2;
        if x >= 480 {
            break;
        }
    }

    // Ground / road
    for py in GROUND_Y..480u32 {
        let t = (py - GROUND_Y) as f32 / (480 - GROUND_Y) as f32;
        let shade = (0x18_u32 + ((0x30 - 0x18) as f32 * t) as u32) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([shade, shade, shade + 8]));
        }
    }

    // Road markings
    for i in (0..480u32).step_by(60) {
        for px in i..(i + 30).min(480) {
            if (px + (*tick as u32 / 2)) % 60 < 30 {
                img.put_pixel(px, GROUND_Y + 15, Rgb([0x80, 0x80, 0x60]));
                img.put_pixel(px, GROUND_Y + 16, Rgb([0x80, 0x80, 0x60]));
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_neon_sign  — flickering neon "ttlcd" sign on dark wall
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static NEON_SIGN_TICK: std::sync::OnceLock<std::sync::Mutex<(u64, u32)>> =
    std::sync::OnceLock::new();

pub fn render_neon_sign() -> DynamicImage {
    let state = NEON_SIGN_TICK.get_or_init(|| std::sync::Mutex::new((0u64, 0x0E00_1234u32)));
    let mut st = state.lock().unwrap();
    let (ref mut tick, ref mut seed) = *st;
    *tick += 1;
    let tf = *tick as f32;

    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    let mut img = RgbImage::new(480, 480);

    // Dark brick-wall background
    for py in 0u32..480 {
        for px in 0u32..480 {
            let brick_row = py / 20;
            let brick_col_off = if brick_row % 2 == 0 { 0u32 } else { 25 };
            let brick_col = (px + brick_col_off) / 50;
            let mortar_h = py % 20 < 2;
            let mortar_v = (px + brick_col_off) % 50 < 2;
            let shade: u8 = if mortar_h || mortar_v {
                0x20
            } else {
                // Brick variation
                let var = ((brick_col as i32 * 17 + brick_row as i32 * 31) & 0xF) as u8;
                0x30 + var
            };
            img.put_pixel(
                px,
                py,
                Rgb([
                    shade,
                    (shade as u32 * 2 / 3) as u8,
                    (shade as u32 * 2 / 3) as u8,
                ]),
            );
        }
    }

    // Neon colours for each letter: t=pink, t=blue, l=green, c=yellow, d=mauve
    let neon_colours: [Rgb<u8>; 5] = [RED, BLUE, GREEN, YELLOW, MAUVE];
    // Letters are drawn as 3D neon tube outlines using thick anti-aliased lines
    // We'll rasterise large stroked letter outlines using simple segment drawing

    // Positions of letter centres (approximate), scale=large
    let letter_cx = [70, 150, 230, 320, 400i32];
    let cy = 220i32;

    // Simple T stroke: horizontal bar + vertical stem
    // Each letter: define strokes as line segments
    // Format: Vec of (x0, y0, x1, y1) in local coords (±50 scale)
    let letter_strokes: [&[(i32, i32, i32, i32)]; 5] = [
        // 't' — horizontal at 0, vertical from -40 to +40
        &[(-25, -40, 25, -40), (0, -40, 0, 40)],
        // 't' again
        &[(-25, -40, 25, -40), (0, -40, 0, 40)],
        // 'l' — vertical only
        &[(0, -40, 0, 40), (-15, 40, 15, 40)],
        // 'c' — arc approximated by 3 segments
        &[(20, -40, -20, -40), (-20, -40, -20, 40), (-20, 40, 20, 40)],
        // 'd' — right side + arc
        &[
            (20, -40, 20, 40),
            (20, -40, -10, -40),
            (-20, -20, -20, 20),
            (-10, 40, 20, 40),
        ],
    ];

    let scale = 42i32;

    for (li, &lx) in letter_cx.iter().enumerate() {
        let col = neon_colours[li % neon_colours.len()];
        let flicker_base = (tf * 0.07 + li as f32 * 1.3).sin();
        let flicker_fast = (tf * 0.29 + li as f32 * 0.7).sin();
        // Occasional full letter flicker off
        let flicker_off = lcg(seed) > 0.997;
        let brightness = if flicker_off {
            0.0
        } else {
            (flicker_base * 0.1 + flicker_fast * 0.05 + 0.85).clamp(0.0, 1.0)
        };

        for &(lx0, ly0, lx1, ly1) in letter_strokes[li] {
            let x0 = lx + lx0 * scale / 50;
            let y0 = cy + ly0 * scale / 50;
            let x1 = lx + lx1 * scale / 50;
            let y1 = cy + ly1 * scale / 50;

            let steps = ((x1 - x0).abs() + (y1 - y0).abs()).max(1);
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let px_f = x0 as f32 + (x1 - x0) as f32 * t;
                let py_f = y0 as f32 + (y1 - y0) as f32 * t;

                // Glow: multiple radii
                for glow_r in 0..12i32 {
                    let glow_falloff = (-glow_r as f32 * 0.35).exp();
                    for dy in -glow_r..=glow_r {
                        for dx in -glow_r..=glow_r {
                            if dx * dx + dy * dy > glow_r * glow_r {
                                continue;
                            }
                            let px_i = (px_f as i32 + dx) as u32;
                            let py_i = (py_f as i32 + dy) as u32;
                            if px_i >= 480 || py_i >= 480 {
                                continue;
                            }
                            let fa = glow_falloff * brightness;
                            let bg = *img.get_pixel(px_i, py_i);
                            img.put_pixel(
                                px_i,
                                py_i,
                                Rgb([
                                    (bg[0] as f32 * (1.0 - fa) + col[0] as f32 * fa).min(255.0)
                                        as u8,
                                    (bg[1] as f32 * (1.0 - fa) + col[1] as f32 * fa).min(255.0)
                                        as u8,
                                    (bg[2] as f32 * (1.0 - fa) + col[2] as f32 * fa).min(255.0)
                                        as u8,
                                ]),
                            );
                        }
                    }
                }
            }
        }
    }

    // Ground reflection
    let reflect_y = cy + scale + 10;
    if reflect_y > 0 && (reflect_y as u32) < 460 {
        for py_src in (reflect_y as u32)..(reflect_y as u32 + 80).min(480) {
            let dist = (py_src - reflect_y as u32) as f32;
            let fade = (1.0 - dist / 80.0) * 0.3;
            let src_py = (2 * reflect_y as u32).saturating_sub(py_src);
            if src_py < 480 {
                for px in 0..480u32 {
                    let src = *img.get_pixel(px, src_py);
                    let dst = *img.get_pixel(px, py_src);
                    img.put_pixel(
                        px,
                        py_src,
                        Rgb([
                            (dst[0] as f32 * (1.0 - fade) + src[0] as f32 * fade) as u8,
                            (dst[1] as f32 * (1.0 - fade) + src[1] as f32 * fade) as u8,
                            (dst[2] as f32 * (1.0 - fade) + src[2] as f32 * fade) as u8,
                        ]),
                    );
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_ocean_waves  — rolling sine-based ocean surface
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static OCEAN_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_ocean_waves() -> DynamicImage {
    let tick = {
        let m = OCEAN_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.04;
    let mut img = RgbImage::new(480, 480);

    for py in 0u32..480 {
        for px in 0u32..480 {
            let x = px as f32 / 480.0;
            let y = py as f32 / 480.0;

            // Sky above, water below
            // Wave surface height at this x position
            let w1 = (x * 6.0 + tf).sin() * 0.04;
            let w2 = (x * 11.0 - tf * 1.3).sin() * 0.02;
            let w3 = (x * 3.5 + tf * 0.7).cos() * 0.03;
            let surface_y = 0.45 + w1 + w2 + w3;

            let col = if y < surface_y - 0.01 {
                // Sky gradient
                let t_sky = y / (surface_y - 0.01);
                Rgb([
                    (0x1e_u8 as f32 + t_sky * (0x74 - 0x1e) as f32) as u8,
                    (0x1e_u8 as f32 + t_sky * (0xc7 - 0x1e) as f32) as u8,
                    (0x2e_u8 as f32 + t_sky * (0xec - 0x2e) as f32) as u8,
                ])
            } else if y < surface_y + 0.01 {
                // Foam / wave crest
                let dist = (y - surface_y).abs() / 0.01;
                let foam = (1.0 - dist) * 0.8;
                Rgb([
                    (0xcdd6f4 >> 16 & 0xFF) as u8,
                    (0xcdd6f4 >> 8 & 0xFF) as u8,
                    (255.0 * foam) as u8,
                ])
            } else {
                // Water depth: fade to darker blue
                let depth = (y - surface_y) / (1.0 - surface_y);
                // Refraction distortion
                let distort = ((x * 8.0 + depth * 4.0 + tf).sin() * 0.005) + depth;
                let caustic =
                    ((x * 20.0 + tf * 2.0).sin() * (distort * 10.0 + tf).cos() * 0.5 + 0.5) * 0.2;
                Rgb([
                    ((0x17 as f32 + caustic * 40.0) * (1.0 - depth * 0.7)) as u8,
                    ((0x5a as f32 + caustic * 60.0) * (1.0 - depth * 0.5)) as u8,
                    ((0x8c as f32 + caustic * 40.0) * (1.0 - depth * 0.3)) as u8,
                ])
            };

            img.put_pixel(px, py, col);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Lightning Storm ───────────────────────────────────────────────────────────
pub fn render_lightning_storm() -> DynamicImage {
    static LS_STATE: std::sync::OnceLock<std::sync::Mutex<(u64, u32, f32)>> =
        std::sync::OnceLock::new();
    let (tick, bolt_seed, bolt_bv) = {
        let mut g = LS_STATE
            .get_or_init(|| std::sync::Mutex::new((0u64, 0xBEEF_1234u32, 0.0f32)))
            .lock()
            .unwrap();
        g.0 += 1;
        if g.0 % 110 == 1 {
            g.1 = (g.0 as u32)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(0x9E37_79B9);
            g.2 = 1.0;
        } else {
            g.2 = (g.2 - 0.018).max(0.0);
        }
        (g.0, g.1, g.2)
    };

    let mut img = RgbImage::new(480, 480);

    // Dark stormy sky gradient
    for py in 0..480u32 {
        let t = py as f32 / 480.0;
        let r = (12.0 + t * 10.0) as u8;
        let gv = (10.0 + t * 8.0) as u8;
        let b = (18.0 + t * 18.0) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, gv, b]));
        }
    }

    // Cloud texture (static, LCG-derived)
    {
        let mut cs: u32 = 0xC10D_D0A5u32;
        for _ in 0..1200u32 {
            cs = cs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let cx2 = cs % 480;
            cs = cs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let cy2 = cs % 160;
            cs = cs.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let cb = 8u8 + (cs >> 26) as u8;
            let p = img.get_pixel(cx2, cy2);
            img.put_pixel(cx2, cy2, Rgb([p[0] + cb / 2, p[1] + cb / 2, p[2] + cb]));
        }
    }

    // Sky flash when bolt is fresh
    if bolt_bv > 0.4 {
        let flash = (bolt_bv - 0.4) / 0.6 * 0.25;
        for py in 0..160u32 {
            for px in 0..480u32 {
                let p = img.get_pixel(px, py);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (p[0] as f32 + 50.0 * flash).min(255.0) as u8,
                        (p[1] as f32 + 55.0 * flash).min(255.0) as u8,
                        (p[2] as f32 + 80.0 * flash).min(255.0) as u8,
                    ]),
                );
            }
        }
    }

    // Draw bolt from seed (deterministic shape)
    if bolt_bv > 0.0 {
        let mut s = bolt_seed;
        let rand = |s: &mut u32| -> f32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*s as f32) / (u32::MAX as f32)
        };

        let start_x = (rand(&mut s) * 380.0 + 50.0) as i32;
        // Stack: (x, y, angle, length_remaining, depth)
        let mut stack: Vec<(i32, i32, f32, f32, u8)> = vec![(start_x, 5, 0.05, 340.0, 7u8)];
        let mut segs: Vec<(i32, i32, i32, i32)> = Vec::new();

        while let Some((x, y, angle, remaining, depth)) = stack.pop() {
            if depth == 0 || remaining < 8.0 {
                continue;
            }
            let seg_len = remaining * (0.35 + rand(&mut s) * 0.35);
            let dev = (rand(&mut s) - 0.5) * 0.55;
            let new_angle = angle + dev;
            let ex = x + (new_angle.sin() * seg_len) as i32;
            let ey = y + (new_angle.cos() * seg_len) as i32;
            segs.push((x, y, ex, ey));
            stack.push((ex, ey, new_angle, remaining - seg_len, depth - 1));
            if rand(&mut s) < 0.38 && depth > 2 {
                let bangle = new_angle + (rand(&mut s) - 0.5) * 1.4;
                stack.push((ex, ey, bangle, seg_len * 0.45, depth - 3));
            }
        }

        for &(x1, y1, x2, y2) in &segs {
            let dx_abs = (x2 - x1).abs();
            let dy_abs = (y2 - y1).abs();
            let steps = dx_abs.max(dy_abs).max(1);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let px = x1 + ((x2 - x1) as f32 * t) as i32;
                let py = y1 + ((y2 - y1) as f32 * t) as i32;
                if (1..479).contains(&px) && (0..480).contains(&py) {
                    img.put_pixel(
                        px as u32,
                        py as u32,
                        Rgb([
                            (200.0 * bolt_bv) as u8,
                            (220.0 * bolt_bv) as u8,
                            (255.0 * bolt_bv) as u8,
                        ]),
                    );
                    for (gx, gy) in &[
                        (px - 1, py),
                        (px + 1, py),
                        (px, py.saturating_sub(1)),
                        (px, (py + 1).min(479)),
                    ] {
                        let gxi = *gx as u32;
                        let gyi = *gy as u32;
                        if *gx >= 0 && *gx < 480 && *gy >= 0 && *gy < 480 {
                            let gp = img.get_pixel(gxi, gyi);
                            img.put_pixel(
                                gxi,
                                gyi,
                                Rgb([
                                    (gp[0] as f32 + 70.0 * bolt_bv).min(255.0) as u8,
                                    (gp[1] as f32 + 80.0 * bolt_bv).min(255.0) as u8,
                                    (gp[2] as f32 + 120.0 * bolt_bv).min(255.0) as u8,
                                ]),
                            );
                        }
                    }
                }
            }
        }
    }

    let _ = tick;
    DynamicImage::ImageRgb8(img)
}
