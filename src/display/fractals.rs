//! Screensavers built on curves, fractals and other closed-form patterns.

use super::*;

// ─── Plasma / Sine Wave Screensaver ─────────────────────────────────────────

pub(super) static PLASMA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

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
            let v4 = (cx2 * cx2 + cy2 * cy2).sqrt() * 12.0 + t;
            let v4 = v4.sin();

            // Combine into a value 0..1
            let v = (v1 + v2 + v3 + v4) / 4.0; // -1..1
            let v = (v + 1.0) / 2.0; // 0..1

            // Map to Catppuccin-inspired colours
            // We blend between a few Catppuccin mocha colours
            let r;
            let g;
            let b;
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
    draw_text_mut(
        &mut img,
        Rgb([0, 0, 0]),
        (480 - tw) / 2 + 1,
        449,
        PxScale::from(24.0),
        &font,
        &ts,
    );
    draw_text_mut(
        &mut img,
        WHITE,
        (480 - tw) / 2,
        448,
        PxScale::from(24.0),
        &font,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Lissajous Curves ────────────────────────────────────────────────────────

pub(super) static LISSAJOUS_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

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
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Phase offset slowly advances
    let delta = (tick as f64 * 0.007) % (2.0 * PI);

    // Cycle through interesting frequency ratios: (1,2), (2,3), (3,4), (1,3), (3,5)
    let ratios: [(f64, f64); 5] = [(1.0, 2.0), (2.0, 3.0), (3.0, 4.0), (1.0, 3.0), (3.0, 5.0)];
    let cycle_len = 400u64; // ticks per ratio
    let ratio_idx = ((tick / cycle_len) % ratios.len() as u64) as usize;
    let (a, b) = ratios[ratio_idx];

    // Blend between two consecutive ratios for smooth transition
    let transition = (tick % cycle_len) as f64 / cycle_len as f64;
    let next_idx = (ratio_idx + 1) % ratios.len();
    let (na, nb) = ratios[next_idx];
    let blend = if transition > 0.8 {
        (transition - 0.8) / 0.2
    } else {
        0.0
    };
    let fa = a * (1.0 - blend) + na * blend;
    let fb = b * (1.0 - blend) + nb * blend;

    let cx = 240.0f64;
    let cy = 240.0f64;
    let amp = 210.0f64;

    // Draw curve with N_POINTS points, colour by phase
    let n_pts = 2048usize;
    let palette: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let mut prev: Option<(i32, i32)> = None;
    for i in 0..=n_pts {
        let t = i as f64 / n_pts as f64 * 2.0 * PI;
        let px = (cx + amp * (fa * t + delta).sin()) as i32;
        let py = (cy + amp * (fb * t).sin()) as i32;

        let col_idx = (i * palette.len() / n_pts) % palette.len();
        let col = palette[col_idx];

        if let Some((lx, ly)) = prev {
            if (0..480).contains(&px) && (0..480).contains(&py) {
                imageproc::drawing::draw_antialiased_line_segment_mut(
                    &mut img,
                    (lx, ly),
                    (px, py),
                    col,
                    interpolate,
                );
            }
        }
        prev = Some((px, py));
    }

    // Center dot
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx * dx + dy * dy <= 9 {
                let ix = (cx as i32 + dx).clamp(0, 479) as u32;
                let iy = (cy as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(ix, iy, TEXT);
            }
        }
    }

    let font = font_bold();
    let ratio_str = format!(
        "Lissajous  {:.0}:{:.0}  δ={:.2}",
        fa,
        fb,
        delta % (2.0 * PI)
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        458,
        PxScale::from(14.0),
        &font,
        &ratio_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Voronoi Art ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct VoronoiSeed {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

pub(super) static VORONOI_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<VoronoiSeed>>> =
    std::sync::OnceLock::new();

/// Animated Voronoi diagram — 20 seeds bouncing around, each region coloured
/// differently with a subtle edge-darkening effect.  2×2 px cells.
pub fn render_voronoi() -> DynamicImage {
    const N: usize = 20;
    const CELL: u32 = 2;
    const GRID: u32 = 240;

    let m = VORONOI_STATE.get_or_init(|| {
        let mut s = 0xC0FFEE42u32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let seeds: Vec<VoronoiSeed> = (0..N)
            .map(|_| VoronoiSeed {
                x: lcg(&mut s) * 480.0,
                y: lcg(&mut s) * 480.0,
                vx: (lcg(&mut s) - 0.5) * 3.0,
                vy: (lcg(&mut s) - 0.5) * 3.0,
            })
            .collect();
        std::sync::Mutex::new(seeds)
    });
    let mut seeds = m.lock().unwrap();

    // Move seeds, bounce off walls
    for sd in seeds.iter_mut() {
        sd.x += sd.vx;
        sd.y += sd.vy;
        if sd.x < 0.0 {
            sd.x = 0.0;
            sd.vx = sd.vx.abs();
        }
        if sd.x > 480.0 {
            sd.x = 480.0;
            sd.vx = -sd.vx.abs();
        }
        if sd.y < 0.0 {
            sd.y = 0.0;
            sd.vy = sd.vy.abs();
        }
        if sd.y > 480.0 {
            sd.y = 480.0;
            sd.vy = -sd.vy.abs();
        }
    }

    // Palette: 7 distinct Catppuccin colours (cycle through N seeds)
    let pal: [Rgb<u8>; 7] = [BLUE, GREEN, MAUVE, TEAL, PEACH, RED, YELLOW];

    let mut img = RgbImage::new(480, 480);

    for cy in 0..GRID {
        for cx in 0..GRID {
            let pcx = (cx as f32 + 0.5) * CELL as f32;
            let pcy = (cy as f32 + 0.5) * CELL as f32;

            let mut best_d = f32::MAX;
            let mut best2_d = f32::MAX;
            let mut best_i = 0usize;

            for (i, sd) in seeds.iter().enumerate() {
                let dx = pcx - sd.x;
                let dy = pcy - sd.y;
                let d = dx * dx + dy * dy;
                if d < best_d {
                    best2_d = best_d;
                    best_d = d;
                    best_i = i;
                } else if d < best2_d {
                    best2_d = d;
                }
            }

            // Edge darkening: ratio of nearest to second-nearest distance
            let edge = (best_d.sqrt() / best2_d.sqrt().max(1.0)).min(1.0);
            let br = 0.55 + 0.45 * edge; // darker near borders

            let base = pal[best_i % pal.len()];
            let col = Rgb([
                (base[0] as f32 * br) as u8,
                (base[1] as f32 * br) as u8,
                (base[2] as f32 * br) as u8,
            ]);

            let px = cx * CELL;
            let py = cy * CELL;
            img.put_pixel(px, py, col);
            img.put_pixel(px + 1, py, col);
            img.put_pixel(px, py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Reaction-Diffusion (Gray-Scott) ──────────────────────────────────────────

pub(super) struct GsState {
    u: Vec<f32>, // 160×160
    v: Vec<f32>,
    tick: u64,
}

pub(super) static GS_STATE: std::sync::OnceLock<std::sync::Mutex<GsState>> =
    std::sync::OnceLock::new();

/// Gray-Scott reaction-diffusion — creates organic spot/maze patterns.
/// 160×160 simulation cells (3×3 px each); 8 iterations per render call.
pub fn render_reaction_diffusion() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3; // 160 × 3 = 480
    const DU: f32 = 0.16;
    const DV: f32 = 0.08;
    const F: f32 = 0.035;
    const K: f32 = 0.065;
    const DT: f32 = 1.0;
    const ITERS: usize = 8;

    let m = GS_STATE.get_or_init(|| {
        let mut u = vec![1.0f32; W * H];
        let mut v = vec![0.0f32; W * H];
        // Seed a few random patches with v=1
        let mut seed = 0xABCD1234u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s
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
                let i = y * W + x;
                let up = if y > 0 {
                    st.u[(y - 1) * W + x]
                } else {
                    st.u[(H - 1) * W + x]
                };
                let dn = if y < H - 1 {
                    st.u[(y + 1) * W + x]
                } else {
                    st.u[x]
                };
                let lt = if x > 0 {
                    st.u[y * W + x - 1]
                } else {
                    st.u[y * W + W - 1]
                };
                let rt = if x < W - 1 {
                    st.u[y * W + x + 1]
                } else {
                    st.u[y * W]
                };
                let lap_u = up + dn + lt + rt - 4.0 * st.u[i];

                let vp = if y > 0 {
                    st.v[(y - 1) * W + x]
                } else {
                    st.v[(H - 1) * W + x]
                };
                let vn = if y < H - 1 {
                    st.v[(y + 1) * W + x]
                } else {
                    st.v[x]
                };
                let vl = if x > 0 {
                    st.v[y * W + x - 1]
                } else {
                    st.v[y * W + W - 1]
                };
                let vr = if x < W - 1 {
                    st.v[y * W + x + 1]
                } else {
                    st.v[y * W]
                };
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
            let vv = st.v[cy * W + cx];
            let col = if vv < 0.02 {
                BG
            } else {
                let t = (vv * 3.0).min(1.0); // 0..1 ramp over v 0..0.33
                let t2 = (vv * 1.5 - 0.5).clamp(0.0, 1.0); // 1.0 for v>1
                Rgb([
                    ((BLUE[0] as f32 * (1.0 - t2) + TEAL[0] as f32 * t2) * t) as u8,
                    ((BLUE[1] as f32 * (1.0 - t2) + TEAL[1] as f32 * t2) * t) as u8,
                    ((BLUE[2] as f32 * (1.0 - t2) + TEAL[2] as f32 * t2) * t) as u8,
                ])
            };
            let px0 = cx as u32 * CELL;
            let py0 = cy as u32 * CELL;
            for dy in 0..CELL {
                for dx in 0..CELL {
                    img.put_pixel(px0 + dx, py0 + dy, col);
                }
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Julia Set (Animated) ─────────────────────────────────────────────────────

pub(super) static JULIA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Animated Julia set fractal — the parameter c rotates slowly, producing
/// a continuously morphing fractal.  Rendered at 240×240 (2×2 px cells).
pub fn render_julia_set() -> DynamicImage {
    let tick = {
        let mut t = JULIA_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *t += 1;
        *t
    };
    let tf = tick as f64;

    // Animate c around a circle near the boundary of the Mandelbrot set
    let angle = tf * 0.013;
    let cr = 0.7885 * angle.cos();
    let ci = 0.7885 * angle.sin();

    const MAX_ITER: u32 = 64;
    const ZOOM: f64 = 1.5; // 1.5 → shows the full Julia set
    const CELL: u32 = 2;
    const GRID: u32 = 240;

    let mut img = RgbImage::new(480, 480);

    // Colour palette: map iteration count to Catppuccin colour
    let pal: [Rgb<u8>; 8] = [BG, BLUE, TEAL, GREEN, YELLOW, PEACH, RED, MAUVE];

    for cy in 0..GRID {
        for cx in 0..GRID {
            // Map pixel to complex plane
            let zr = (cx as f64 / GRID as f64 - 0.5) * 2.0 * ZOOM;
            let zi = (cy as f64 / GRID as f64 - 0.5) * 2.0 * ZOOM;

            // Iterate z → z² + c
            let (mut zr, mut zi) = (zr, zi);
            let mut iter = 0u32;
            while zr * zr + zi * zi <= 4.0 && iter < MAX_ITER {
                let tmp = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = tmp;
                iter += 1;
            }

            // Smooth colouring using fractional escape count
            let smooth = if iter < MAX_ITER {
                let log2 = (zr * zr + zi * zi).ln().ln() / std::f64::consts::LN_2;
                iter as f64 + 1.0 - log2
            } else {
                0.0 // inside set → background
            };

            let col = if smooth <= 0.0 {
                BG
            } else {
                let t = (smooth / MAX_ITER as f64).min(1.0);
                let idx = (t * 6.99) as usize;
                let frac = t * 7.0 - idx as f64;
                let a = pal[idx.min(6)];
                let b = pal[(idx + 1).min(7)];
                Rgb([
                    (a[0] as f64 + (b[0] as f64 - a[0] as f64) * frac) as u8,
                    (a[1] as f64 + (b[1] as f64 - a[1] as f64) * frac) as u8,
                    (a[2] as f64 + (b[2] as f64 - a[2] as f64) * frac) as u8,
                ])
            };

            let px = cx * CELL;
            let py = cy * CELL;
            img.put_pixel(px, py, col);
            img.put_pixel(px + 1, py, col);
            img.put_pixel(px, py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Hypnotic Spiral ─────────────────────────────────────────────────────────

pub(super) static HYPNO_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Hypnotic spiral — combination of radial distance and angle modulated
/// by slow tick-based oscillation, coloured with a Catppuccin-derived palette.
pub fn render_hypno() -> DynamicImage {
    let tick = {
        let mut t = HYPNO_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *t += 1;
        *t
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    for py in 0..480u32 {
        for px in 0..480u32 {
            let dx = px as f32 - 240.0;
            let dy = py as f32 - 240.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);

            // Two overlaid spiral patterns
            let s1 = (dist * 0.08 - tf * 0.04 + angle * 3.0).sin();
            let s2 = (dist * 0.05 + tf * 0.025 - angle * 5.0).cos();
            let wave = (s1 * s2 + (dist * 0.04 + tf * 0.02).sin() * 0.3) * 0.5 + 0.5;

            // Colour: map 0..1 to a Catppuccin cycle
            let h = (wave + tf * 0.002) % 1.0;
            let col = hsl_to_rgb(h, 0.9, 0.45 + wave * 0.2);

            img.put_pixel(px, py, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_lorenz  — Lorenz attractor 3D trail with slowly-rotating view
// ─────────────────────────────────────────────────────────────────────────────
/// (trail of 3-D points, frame counter)
pub(super) type LorenzState = (Vec<(f32, f32, f32)>, u64);

pub(super) static LORENZ_STATE: std::sync::OnceLock<std::sync::Mutex<LorenzState>> =
    std::sync::OnceLock::new();

pub fn render_lorenz() -> DynamicImage {
    const SIGMA: f32 = 10.0;
    const RHO: f32 = 28.0;
    const BETA: f32 = 8.0 / 3.0;
    const DT: f32 = 0.004;
    const TRAIL: usize = 3000;
    const STEPS_PER_FRAME: usize = 8;

    let state = LORENZ_STATE.get_or_init(|| {
        let mut pts: Vec<(f32, f32, f32)> = Vec::with_capacity(TRAIL);
        pts.push((0.1, 0.0, 0.0));
        std::sync::Mutex::new((pts, 0u64))
    });
    let mut st = state.lock().unwrap();
    let (ref mut pts, ref mut tick) = *st;
    *tick += 1;

    for _ in 0..STEPS_PER_FRAME {
        let &(x, y, z) = pts.last().unwrap();
        let dx = SIGMA * (y - x);
        let dy = x * (RHO - z) - y;
        let dz = x * y - BETA * z;
        pts.push((x + dx * DT, y + dy * DT, z + dz * DT));
        if pts.len() > TRAIL {
            pts.remove(0);
        }
    }

    let tf = *tick as f32;
    let rot_z = tf * 0.003;
    let (sz, cz) = rot_z.sin_cos();
    let rot_x = 0.55_f32;
    let (sx, cx) = rot_x.sin_cos();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let n = pts.len();
    for (i, &(x, y, z)) in pts.iter().enumerate() {
        let nx = x / 25.0;
        let ny = y / 25.0;
        let nz = (z - 25.0) / 25.0;

        let rx = nx * cz - ny * sz;
        let ry = nx * sz + ny * cz;
        let fy = ry * cx - nz * sx;

        let px_i = (240.0 + rx * 80.0) as i32;
        let py_i = (240.0 + fy * 80.0) as i32;
        if !(0..480).contains(&px_i) || !(0..480).contains(&py_i) {
            continue;
        }

        let t = i as f32 / n as f32;
        let r = (BLUE[0] as f32 + (MAUVE[0] as f32 - BLUE[0] as f32) * t) as u8;
        let g = (BLUE[1] as f32 + (MAUVE[1] as f32 - BLUE[1] as f32) * t) as u8;
        let b = (BLUE[2] as f32 + (MAUVE[2] as f32 - BLUE[2] as f32) * t) as u8;
        // Alpha-blend over background
        let alpha = (t * t * 255.0) as u8;
        let px = px_i as u32;
        let py_u = py_i as u32;
        let bg = img.get_pixel(px, py_u);
        let fa = alpha as f32 / 255.0;
        img.put_pixel(
            px,
            py_u,
            Rgb([
                (bg[0] as f32 * (1.0 - fa) + r as f32 * fa) as u8,
                (bg[1] as f32 * (1.0 - fa) + g as f32 * fa) as u8,
                (bg[2] as f32 * (1.0 - fa) + b as f32 * fa) as u8,
            ]),
        );
    }
    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_mandelbrot  — animated Mandelbrot zoom into the Seahorse Valley
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static MANDELBROT_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_mandelbrot() -> DynamicImage {
    let tick = {
        let m = MANDELBROT_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    const W: u32 = 240;
    const H: u32 = 240;
    const CELL: u32 = 2;
    const MAX_ITER: u32 = 96;
    const CX: f64 = -0.743_643_887;
    const CY: f64 = 0.131_825_904;

    let tf = tick as f64;
    let zoom = (0.8_f64 / (1.0 + tf * 0.003)).max(1e-7);

    let palette: [[u8; 3]; 8] = [
        [BG[0], BG[1], BG[2]],
        [BLUE[0], BLUE[1], BLUE[2]],
        [TEAL[0], TEAL[1], TEAL[2]],
        [GREEN[0], GREEN[1], GREEN[2]],
        [YELLOW[0], YELLOW[1], YELLOW[2]],
        [PEACH[0], PEACH[1], PEACH[2]],
        [RED[0], RED[1], RED[2]],
        [MAUVE[0], MAUVE[1], MAUVE[2]],
    ];

    let mut img = RgbImage::new(480, 480);

    for cy_cell in 0..H {
        for cx_cell in 0..W {
            let x0 = CX + (cx_cell as f64 / W as f64 - 0.5) * 2.5 * zoom;
            let y0 = CY + (cy_cell as f64 / H as f64 - 0.5) * 2.5 * zoom;
            let mut x = 0.0_f64;
            let mut y = 0.0_f64;
            let mut iter = 0u32;
            while x * x + y * y <= 4.0 && iter < MAX_ITER {
                let xn = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xn;
                iter += 1;
            }
            let col: [u8; 3] = if iter == MAX_ITER {
                [BG[0], BG[1], BG[2]]
            } else {
                let r2 = (x * x + y * y) as f32;
                let smooth = if r2 > 1.0 {
                    iter as f32 + 1.0 - (r2.ln() / 2.0_f32.ln()).ln() / 2.0_f32.ln()
                } else {
                    iter as f32
                };
                let t = (smooth / 12.0).rem_euclid(1.0);
                let idx = (t * 7.0) as usize;
                let frac = (t * 7.0).fract();
                let c0 = palette[idx % 8];
                let c1 = palette[(idx + 1) % 8];
                [
                    (c0[0] as f32 + (c1[0] as f32 - c0[0] as f32) * frac) as u8,
                    (c0[1] as f32 + (c1[1] as f32 - c0[1] as f32) * frac) as u8,
                    (c0[2] as f32 + (c1[2] as f32 - c0[2] as f32) * frac) as u8,
                ]
            };

            for dy in 0..CELL {
                for dx in 0..CELL {
                    img.put_pixel(cx_cell * CELL + dx, cy_cell * CELL + dy, Rgb(col));
                }
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_kaleidoscope  — 6-fold symmetry plasma pattern
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static KALEIDO_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_kaleidoscope() -> DynamicImage {
    let tick = {
        let m = KALEIDO_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.015;
    const FOLDS: f32 = 6.0;
    const TAU: f32 = std::f32::consts::TAU;

    let mut img = RgbImage::new(480, 480);

    for py in 0..480u32 {
        for px in 0..480u32 {
            let dx = px as f32 - 240.0;
            let dy = py as f32 - 240.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let mut angle = dy.atan2(dx);

            let sector_angle = std::f32::consts::PI / FOLDS;
            angle = (angle + std::f32::consts::PI) % (2.0 * sector_angle);
            if angle > sector_angle {
                angle = 2.0 * sector_angle - angle;
            }

            let ex = dist * angle.cos();
            let ey = dist * angle.sin();

            let v = ((ex * 0.012 + tf).sin()
                + (ey * 0.012 - tf * 0.7).sin()
                + ((ex + ey) * 0.008 + tf * 1.3).sin()
                + (dist * 0.015 - tf * 0.9).sin())
                * 0.25
                + 0.5;

            let h = (v + tf * 0.01) % 1.0;
            // HSL to RGB inline (already have hsl_to_rgb)
            let col = hsl_to_rgb(h, 0.95, 0.45 + v * 0.15);
            let _ = TAU;
            img.put_pixel(px, py, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_sierpinski  — animated Sierpiński triangle fractal (chaos game)
// ─────────────────────────────────────────────────────────────────────────────
pub(super) type SierpinskiState = (Vec<(f32, f32)>, u32, u64);

pub(super) static SIERP_STATE: std::sync::OnceLock<std::sync::Mutex<SierpinskiState>> =
    std::sync::OnceLock::new();

pub fn render_sierpinski() -> DynamicImage {
    const N_DOTS: usize = 25_000;
    let state = SIERP_STATE
        .get_or_init(|| std::sync::Mutex::new((vec![(240.0f32, 240.0f32)], 0xF4AC_7A1Bu32, 0u64)));
    let mut st = state.lock().unwrap();
    let (ref mut dots, ref mut seed, ref mut tick) = *st;
    *tick += 1;
    let tf = *tick as f32;

    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    // Slowly rotate the triangle over time
    let base_angle = tf * 0.005;
    let r = 210.0_f32;
    let verts: [(f32, f32); 3] = [
        (
            240.0 + r * (base_angle + std::f32::consts::TAU * 0.0 / 3.0).cos(),
            240.0 + r * (base_angle + std::f32::consts::TAU * 0.0 / 3.0).sin(),
        ),
        (
            240.0 + r * (base_angle + std::f32::consts::TAU * 1.0 / 3.0).cos(),
            240.0 + r * (base_angle + std::f32::consts::TAU * 1.0 / 3.0).sin(),
        ),
        (
            240.0 + r * (base_angle + std::f32::consts::TAU * 2.0 / 3.0).cos(),
            240.0 + r * (base_angle + std::f32::consts::TAU * 2.0 / 3.0).sin(),
        ),
    ];

    // Chaos game iterations
    while dots.len() < N_DOTS {
        let last = *dots.last().unwrap();
        let vi = (lcg(seed) * 3.0) as usize % 3;
        let v = verts[vi];
        dots.push(((last.0 + v.0) * 0.5, (last.1 + v.1) * 0.5));
    }
    // Reset periodically to avoid stale dots after rotation
    if *tick % 200 == 0 {
        dots.clear();
        dots.push((240.0, 240.0));
    }

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let cols: [Rgb<u8>; 3] = [BLUE, MAUVE, TEAL];

    for (i, &(x, y)) in dots.iter().enumerate() {
        let px = x as u32;
        let py = y as u32;
        if px >= 480 || py >= 480 {
            continue;
        }
        // Colour by which third of the fractal
        let dx = x - 240.0;
        let dy = y - 240.0;
        let angle_f = (dy.atan2(dx) + std::f32::consts::PI) / std::f32::consts::TAU;
        let ci = (angle_f * 3.0) as usize % 3;
        let brightness = 0.5 + (i as f32 / N_DOTS as f32) * 0.5;
        let c = cols[ci];
        let existing = *img.get_pixel(px, py);
        img.put_pixel(
            px,
            py,
            Rgb([
                (existing[0] as f32 + c[0] as f32 * brightness * 0.6).min(255.0) as u8,
                (existing[1] as f32 + c[1] as f32 * brightness * 0.6).min(255.0) as u8,
                (existing[2] as f32 + c[2] as f32 * brightness * 0.6).min(255.0) as u8,
            ]),
        );
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_moss_growth  — cellular automaton: slow-growing organic moss/lichen
// ─────────────────────────────────────────────────────────────────────────────
pub(super) type MossState = (Vec<u8>, Vec<f32>, u32, u64);

pub(super) static MOSS_STATE: std::sync::OnceLock<std::sync::Mutex<MossState>> =
    std::sync::OnceLock::new();

pub fn render_moss_growth() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3;

    let state = MOSS_STATE.get_or_init(|| {
        let mut s = 0x4051_C0FFu32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let mut cells = vec![0u8; W * H];
        // Seed small patches
        for _ in 0..20 {
            let x = (lcg(&mut s) * (W - 10) as f32) as usize;
            let y = (lcg(&mut s) * (H - 10) as f32) as usize;
            for dy in 0..3 {
                for dx in 0..3 {
                    cells[(y + dy) * W + x + dx] = 1;
                }
            }
        }
        let health = vec![1.0f32; W * H];
        std::sync::Mutex::new((cells, health, s, 0u64))
    });

    let mut st = state.lock().unwrap();
    let (ref mut cells, ref mut health, ref mut seed, ref mut tick) = *st;
    *tick += 1;

    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    // Cellular automaton step (every frame)
    let old = cells.clone();
    let old_h = health.clone();

    for y in 1..(H - 1) {
        for x in 1..(W - 1) {
            let i = y * W + x;
            let neighbors: [usize; 8] = [
                (y - 1) * W + x - 1,
                (y - 1) * W + x,
                (y - 1) * W + x + 1,
                y * W + x - 1,
                y * W + x + 1,
                (y + 1) * W + x - 1,
                (y + 1) * W + x,
                (y + 1) * W + x + 1,
            ];
            let live_n: u32 = neighbors.iter().map(|&ni| old[ni] as u32).sum();
            let avg_h: f32 = neighbors.iter().map(|&ni| old_h[ni]).sum::<f32>() / 8.0;

            if old[i] == 0 {
                // Spread: grow if 1-3 live neighbors and random chance
                if (1..=3).contains(&live_n) && lcg(seed) < 0.03 {
                    cells[i] = 1;
                    health[i] = (avg_h * 0.8).clamp(0.1, 1.0);
                }
            } else {
                // Alive: die if overcrowded or underpopulated
                if live_n == 0 || live_n > 5 {
                    health[i] -= 0.03;
                    if health[i] <= 0.0 {
                        cells[i] = 0;
                        health[i] = 0.0;
                    }
                } else {
                    health[i] = (health[i] + 0.005).min(1.0);
                }
            }
        }
    }

    // Re-seed if population crashes
    let live_count: u32 = cells.iter().map(|&c| c as u32).sum();
    if live_count < 200 {
        let x = (lcg(seed) * (W - 5) as f32) as usize;
        let y = (lcg(seed) * (H - 5) as f32) as usize;
        for dy in 0..4 {
            for dx in 0..4 {
                cells[(y + dy) * W + x + dx] = 1;
                health[(y + dy) * W + x + dx] = 1.0;
            }
        }
    }

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x1e, 0x1a, 0x0e]);
    }

    let cols: [(f32, f32, f32); 4] = [
        (0.32, 0.6, 0.25),  // light green
        (0.38, 0.55, 0.22), // medium green
        (0.28, 0.65, 0.30), // lime green
        (0.42, 0.45, 0.18), // dark green
    ];

    for y in 0..H {
        for x in 0..W {
            let i = y * W + x;
            if cells[i] == 0 {
                continue;
            }
            let h = health[i];
            // Pick colour by position hash
            let ci = (x * 7 + y * 13) % 4;
            let (ch, cs, mut cl) = cols[ci];
            cl *= h;
            let col = hsl_to_rgb(ch, cs, cl);

            for dy in 0..CELL {
                for dx in 0..CELL {
                    img.put_pixel(x as u32 * CELL + dx, y as u32 * CELL + dy, col);
                }
            }
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        SUBTEXT,
        8,
        462,
        PxScale::from(12.0),
        &font,
        &format!("Moss: {live_count} cells"),
    );

    DynamicImage::ImageRgb8(img)
}

// ── Spirograph ────────────────────────────────────────────────────────────────
pub fn render_spirograph() -> DynamicImage {
    static SPIRO_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = SPIRO_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([24u8, 24, 37]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let cx = 240.0f32;
    let cy = 240.0f32;

    // Hypotrochoid: x=(R-r)*cos(t)+d*cos((R-r)/r*t)
    //               y=(R-r)*sin(t)-d*sin((R-r)/r*t)
    // 5 curves with different parameters and hue
    let curves: &[(f32, f32, f32, f32, [u8; 3])] = &[
        (100.0, 41.0, 80.0, 0.0, [137, 180, 250]), // blue
        (108.0, 33.0, 68.0, 0.5, [203, 166, 247]), // mauve
        (92.0, 29.0, 62.0, 1.1, [166, 227, 161]),  // green
        (115.0, 46.0, 74.0, 1.7, [250, 179, 135]), // peach
        (104.0, 38.0, 66.0, 2.3, [249, 226, 175]), // yellow
    ];

    let phase_drift = tf * 0.006;

    for &(big_r, small_r, d, offset, col) in curves {
        let ratio = big_r / small_r;
        let period = std::f32::consts::TAU * ratio.ceil();
        let steps = 2400usize;
        let mut prev: Option<(i32, i32)> = None;
        for i in 0..=steps {
            let t = i as f32 / steps as f32 * period + phase_drift + offset;
            let inner_t = (big_r - small_r) / small_r * t + phase_drift;
            let x = (big_r - small_r) * t.cos() + d * inner_t.cos();
            let y = (big_r - small_r) * t.sin() - d * inner_t.sin();
            let px = (cx + x) as i32;
            let py = (cy + y) as i32;
            // Draw thick dot
            if (1..479).contains(&px) && (1..479).contains(&py) {
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let qx = (px + dx) as u32;
                        let qy = (py + dy) as u32;
                        let existing = img.get_pixel(qx, qy);
                        img.put_pixel(
                            qx,
                            qy,
                            Rgb([
                                ((existing[0] as u32 + col[0] as u32 * 2 / 3).min(255)) as u8,
                                ((existing[1] as u32 + col[1] as u32 * 2 / 3).min(255)) as u8,
                                ((existing[2] as u32 + col[2] as u32 * 2 / 3).min(255)) as u8,
                            ]),
                        );
                    }
                }
            }
            prev = Some((px, py));
        }
        let _ = prev;
    }

    DynamicImage::ImageRgb8(img)
}

// ── Fractal Tree ──────────────────────────────────────────────────────────────
pub fn render_fractal_tree() -> DynamicImage {
    static TREE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = TREE_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // Night sky gradient (upper) to ground (lower)
    for py in 0..480u32 {
        let t = py as f32 / 480.0;
        let (r, g, b) = if t < 0.75 {
            (8u8, 10u8, (20.0 + t * 15.0) as u8)
        } else {
            let gt = (t - 0.75) / 0.25;
            (
                (gt * 30.0) as u8,
                (20.0 + gt * 30.0) as u8,
                (35.0 - gt * 15.0) as u8,
            )
        };
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Moon
    for py in 20i32..80 {
        for px in 360i32..420 {
            let dx = px as f32 - 390.0;
            let dy = py as f32 - 50.0;
            if dx * dx + dy * dy <= 22.0 * 22.0 {
                img.put_pixel(px as u32, py as u32, Rgb([240, 235, 190]));
            }
        }
    }

    let sway = (tf * 0.025).sin() * 0.14;

    // Iterative branching tree
    // Stack: (x, y, angle, length, depth, green_frac)
    let root_angle = -std::f32::consts::FRAC_PI_2 + sway * 0.3;
    let mut stack: Vec<(f32, f32, f32, f32, u8)> = vec![(240.0, 460.0, root_angle, 75.0, 9u8)];

    while let Some((x, y, angle, length, depth)) = stack.pop() {
        if depth == 0 || length < 2.5 {
            continue;
        }

        let ex = x + angle.cos() * length;
        let ey = y + angle.sin() * length;

        let col = match depth {
            8..=9 => Rgb([75u8, 45, 18]),
            5..=7 => Rgb([90u8, 58, 25]),
            3..=4 => Rgb([50u8, 80, 30]),
            _ => {
                let g_var = depth.wrapping_mul(43).wrapping_add(30);
                Rgb([25u8, 100 + g_var / 3, 28])
            }
        };

        let thick = (depth as i32).min(4);
        let dx_abs = (ex - x).abs() as i32;
        let dy_abs = (ey - y).abs() as i32;
        let steps = dx_abs.max(dy_abs).max(1);
        for si in 0..=steps {
            let t = si as f32 / steps as f32;
            let spx = (x + (ex - x) * t) as i32;
            let spy = (y + (ey - y) * t) as i32;
            for ty in -thick..=thick {
                for tx in -thick..=thick {
                    if tx * tx + ty * ty <= thick * thick {
                        let bx = spx + tx;
                        let by = spy + ty;
                        if (0..480).contains(&bx) && (0..480).contains(&by) {
                            img.put_pixel(bx as u32, by as u32, col);
                        }
                    }
                }
            }
        }

        let new_len = length * 0.70;
        let spread = 0.38 + sway * (1.0 / (depth as f32 + 1.0));
        stack.push((ex, ey, angle - spread, new_len, depth - 1));
        stack.push((ex, ey, angle + spread + sway * 0.5, new_len, depth - 1));
    }

    DynamicImage::ImageRgb8(img)
}

// ── Color Mosaic ──────────────────────────────────────────────────────────────
// A dynamic tile mosaic where each tile cycles through a unique hue with wave
// patterns propagating across the grid.
pub fn render_color_mosaic() -> DynamicImage {
    static MOSAIC_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = MOSAIC_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let tile_w = 24u32;
    let tile_h = 24u32;
    let cols = 480 / tile_w;
    let rows = 480 / tile_h;

    let mut img = RgbImage::new(480, 480);

    for row in 0..rows {
        for col in 0..cols {
            let cx = col as f32 + 0.5;
            let cy = row as f32 + 0.5;
            let dist_center =
                ((cx - cols as f32 / 2.0).powi(2) + (cy - rows as f32 / 2.0).powi(2)).sqrt();

            // Multiple wave sources create interference
            let wave1 = (dist_center * 0.4 - tf * 0.04).cos();
            let wave2 = (cx * 0.6 - tf * 0.05).sin();
            let wave3 = (cy * 0.5 + tf * 0.03).cos();
            let combined = (wave1 + wave2 + wave3) / 3.0;

            let hue = ((cx * 0.07 + cy * 0.05 + combined * 0.5 + tf * 0.008).fract() + 1.0) % 1.0;
            let lightness = 0.45 + combined * 0.15;
            let sat = 0.75 + wave3 * 0.15;

            let tile_col = hsl_to_rgb(hue, sat.clamp(0.5, 0.9), lightness.clamp(0.25, 0.65));
            let (r, g, b) = (tile_col[0], tile_col[1], tile_col[2]);

            // Tile fill with 1px dark border
            let px0 = col * tile_w;
            let py0 = row * tile_h;
            for dy in 0..tile_h {
                for dx in 0..tile_w {
                    let (tr, tg, tb) = if dx == 0 || dy == 0 {
                        (8u8, 8, 8) // border
                    } else if dx == tile_w - 1 || dy == tile_h - 1 {
                        (
                            (r as f32 * 0.6) as u8,
                            (g as f32 * 0.6) as u8,
                            (b as f32 * 0.6) as u8,
                        )
                    } else {
                        (r, g, b)
                    };
                    img.put_pixel(px0 + dx, py0 + dy, Rgb([tr, tg, tb]));
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Hex Plasma ────────────────────────────────────────────────────────────────
// Hexagonal grid with each cell independently pulsing via plasma functions.
pub fn render_hex_plasma() -> DynamicImage {
    static HEX_PLASMA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = HEX_PLASMA_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // Hex grid parameters
    let hex_r = 28.0f32; // circumradius
    let hex_h = hex_r * 1.732_050_8; // sqrt(3) * r = row height
    let hex_w = hex_r * 2.0;

    // Fill the whole image
    for py in 0..480u32 {
        for px in 0..480u32 {
            let x = px as f32;
            let y = py as f32;

            // Find which hex cell this pixel belongs to
            // Axial coordinates
            let col_f = x / (hex_w * 0.75);
            let col_i = col_f as i32;
            let row_offset = if col_i % 2 != 0 { hex_h * 0.5 } else { 0.0 };
            let row_f = (y - row_offset) / hex_h;
            let row_i = row_f as i32;

            // Centre of this hex
            let cx = col_i as f32 * hex_w * 0.75 + hex_r;
            let cy = row_i as f32 * hex_h + row_offset + hex_h * 0.5;

            // Distance to hex center for plasma value
            let dist = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let hex_dist = dist / hex_r; // 0..1 within hex

            // Plasma value from multiple waves
            let v1 = (cx * 0.015 + tf * 0.04).sin();
            let v2 = (cy * 0.012 - tf * 0.03).cos();
            let v3 = ((cx - 240.0).powi(2) + (cy - 240.0).powi(2)).sqrt() * 0.008 - tf * 0.025;
            let plasma = (v1 + v2 + v3.cos()) / 3.0; // -1..1

            let hue = ((plasma * 0.5 + 0.5 + tf * 0.005) % 1.0 + 1.0) % 1.0;
            let sat = 0.8 + hex_dist * 0.1;
            let lit = 0.35 + (1.0 - hex_dist.min(1.0)) * 0.25;

            // Border: dim near hex edges
            let border = if hex_dist > 0.88 {
                (1.0 - hex_dist) / 0.12
            } else {
                1.0
            };

            let col = hsl_to_rgb(hue, sat.clamp(0.5, 1.0), (lit * border).clamp(0.05, 0.65));
            img.put_pixel(px, py, col);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Wave Interference ─────────────────────────────────────────────────────────
// Two-source wave interference pattern that animates the source positions.
pub fn render_wave_interference() -> DynamicImage {
    static WAVE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = WAVE_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);

    // Two wave sources that slowly orbit around the centre
    let orbit = 110.0f32;
    let s1 = (
        240.0 + (tf * 0.018).cos() * orbit,
        240.0 + (tf * 0.018).sin() * orbit,
    );
    let s2 = (
        240.0 - (tf * 0.018).cos() * orbit,
        240.0 - (tf * 0.018).sin() * orbit,
    );
    // Third subtle source
    let s3 = (
        240.0 + (tf * 0.011 + 1.2).cos() * 60.0,
        240.0 + (tf * 0.011 + 1.2).sin() * 60.0,
    );

    let wave_k = 0.065f32; // wave number
    let omega_t = tf * 0.085;

    for py in 0..480u32 {
        for px in 0..480u32 {
            let x = px as f32;
            let y = py as f32;

            let r1 = ((x - s1.0).powi(2) + (y - s1.1).powi(2)).sqrt();
            let r2 = ((x - s2.0).powi(2) + (y - s2.1).powi(2)).sqrt();
            let r3 = ((x - s3.0).powi(2) + (y - s3.1).powi(2)).sqrt();

            // Superposed amplitude (with 1/sqrt(r) decay)
            let amp1 = (wave_k * r1 - omega_t).cos() / (r1.sqrt() * 0.12 + 1.0);
            let amp2 = (wave_k * r2 - omega_t).cos() / (r2.sqrt() * 0.12 + 1.0);
            let amp3 = (wave_k * r3 - omega_t * 0.7).cos() * 0.4 / (r3.sqrt() * 0.15 + 1.0);
            let combined = amp1 + amp2 + amp3;

            // Map amplitude to colour (use HSL)
            let hue = ((combined * 0.3 + 0.55 + tf * 0.003) % 1.0 + 1.0) % 1.0;
            let lit = (combined * 0.35 + 0.42).clamp(0.1, 0.75);
            let sat = 0.85 + combined.abs() * 0.1;

            img.put_pixel(px, py, hsl_to_rgb(hue, sat.clamp(0.5, 1.0), lit));
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Clifford Strange Attractor ────────────────────────────────────────────────
// de Jong / Clifford attractor with slowly morphing parameters.
pub fn render_clifford_attractor() -> DynamicImage {
    static CLIFF_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = CLIFF_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([12u8, 10, 18]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Slowly morph parameters through interesting regions
    let a = 1.7 + (tf * 0.003).sin() * 0.6;
    let b = 1.7 + (tf * 0.0023 + 1.0).cos() * 0.6;
    let c = 0.6 + (tf * 0.0017 + 2.0).sin() * 0.4;
    let d = 1.2 + (tf * 0.0021 + 3.0).cos() * 0.4;

    // Run iterator from warm-up state
    let mut x = 0.1f32;
    let mut y = 0.1f32;

    // Warm up (skip transient)
    for _ in 0..200 {
        let nx = (a * y).sin() + c * (a * x).cos();
        let ny = (b * x).sin() + d * (b * y).cos();
        x = nx;
        y = ny;
    }

    // Accumulate density
    let n_pts = 40_000usize;
    // Accumulate in a brightness buffer (u16 per pixel)
    let mut bright = vec![0u32; 480 * 480];

    // Scale: attractor lives roughly in [-3, 3] × [-3, 3]
    let scale = 65.0f32;
    let cx = 240.0f32;
    let cy = 240.0f32;

    for _ in 0..n_pts {
        let nx = (a * y).sin() + c * (a * x).cos();
        let ny = (b * x).sin() + d * (b * y).cos();
        x = nx;
        y = ny;
        let px = (cx + x * scale) as i32;
        let py = (cy + y * scale) as i32;
        if (0..480).contains(&px) && (0..480).contains(&py) {
            bright[py as usize * 480 + px as usize] += 1;
        }
    }

    // Find max density for normalisation
    let max_b = *bright.iter().max().unwrap_or(&1) as f32;
    if max_b < 0.1 {
        return DynamicImage::ImageRgb8(img);
    }

    // Colour: hue based on x/y position, brightness from density
    for py in 0..480u32 {
        for px in 0..480u32 {
            let b_val = bright[py as usize * 480 + px as usize];
            if b_val == 0 {
                continue;
            }
            let t = (b_val as f32 / max_b).powf(0.4);
            // Position-based hue drift with time
            let hue = ((px as f32 / 480.0 * 0.6 + py as f32 / 480.0 * 0.4 + tf * 0.004) % 1.0
                + 1.0)
                % 1.0;
            let col = hsl_to_rgb(hue, 0.85, (0.1 + t * 0.65).min(0.8));
            img.put_pixel(px, py, col);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Mandala ───────────────────────────────────────────────────────────────────
// An animated mandala with N-fold rotational symmetry built from layered curves.
pub fn render_mandala() -> DynamicImage {
    static MANDALA_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = MANDALA_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([22u8, 18, 28]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let cx = 240.0f32;
    let cy = 240.0f32;
    let symmetry = 8u32; // 8-fold
    let sym_angle = std::f32::consts::TAU / symmetry as f32;

    // Layer 1: outer ring petals
    {
        let layer_r = 180.0f32;
        let n_petals = 16u32;
        let petal_r = 40.0f32;
        let layer_rot = tf * 0.008;
        for pi in 0..n_petals {
            let base_a = pi as f32 / n_petals as f32 * std::f32::consts::TAU + layer_rot;
            let cx2 = cx + base_a.cos() * layer_r;
            let cy2 = cy + base_a.sin() * layer_r;
            let hue = (pi as f32 / n_petals as f32 + tf * 0.003) % 1.0;
            // Draw petal: ellipse with major axis pointing toward centre
            for dy in -(petal_r as i32)..=(petal_r as i32) {
                for dx in -(petal_r as i32)..=(petal_r as i32) {
                    // Rotate into petal frame
                    let ca = base_a.cos();
                    let sa = base_a.sin();
                    let lx = ca * dx as f32 + sa * dy as f32;
                    let ly = -sa * dx as f32 + ca * dy as f32;
                    let inside = (lx / petal_r).powi(2) + (ly / (petal_r * 0.45)).powi(2);
                    if inside <= 1.0 {
                        let alpha = (1.0 - inside) * 0.6;
                        let bx = cx2 as i32 + dx;
                        let by = cy2 as i32 + dy;
                        if (0..480).contains(&bx) && (0..480).contains(&by) {
                            let p = img.get_pixel(bx as u32, by as u32);
                            let col = hsl_to_rgb(hue, 0.85, 0.5 + inside * 0.1);
                            img.put_pixel(
                                bx as u32,
                                by as u32,
                                Rgb([
                                    (p[0] as f32 * (1.0 - alpha) + col[0] as f32 * alpha).min(255.0)
                                        as u8,
                                    (p[1] as f32 * (1.0 - alpha) + col[1] as f32 * alpha).min(255.0)
                                        as u8,
                                    (p[2] as f32 * (1.0 - alpha) + col[2] as f32 * alpha).min(255.0)
                                        as u8,
                                ]),
                            );
                        }
                    }
                }
            }
        }
    }

    // Layer 2: symmetric spiraling arms
    {
        let arm_rot = tf * -0.012;
        let n_arms = symmetry;
        let steps = 250usize;
        for arm in 0..n_arms {
            let arm_base = arm as f32 * sym_angle + arm_rot;
            let hue = (arm as f32 / n_arms as f32 + tf * 0.004) % 1.0;
            for si in 0..steps {
                let t = si as f32 / steps as f32;
                let r = t * 170.0;
                // Spiral outward
                let angle =
                    arm_base + t * std::f32::consts::TAU * 1.5 + (tf * 0.015 + t * 4.0).sin() * 0.2;
                let px = cx + angle.cos() * r;
                let py = cy + angle.sin() * r;
                let alpha = (0.7 - t * 0.4).max(0.1);
                let lit = 0.4 + t * 0.25;
                let col = hsl_to_rgb(hue, 0.9, lit);
                let bx = px as i32;
                let by = py as i32;
                if (0..480).contains(&bx) && (0..480).contains(&by) {
                    let p = img.get_pixel(bx as u32, by as u32);
                    img.put_pixel(
                        bx as u32,
                        by as u32,
                        Rgb([
                            (p[0] as f32 * (1.0 - alpha) + col[0] as f32 * alpha).min(255.0) as u8,
                            (p[1] as f32 * (1.0 - alpha) + col[1] as f32 * alpha).min(255.0) as u8,
                            (p[2] as f32 * (1.0 - alpha) + col[2] as f32 * alpha).min(255.0) as u8,
                        ]),
                    );
                }
            }
        }
    }

    // Layer 3: inner rings
    {
        let ring_rot = tf * 0.02;
        for ri in 0..5u32 {
            let r_base = 25.0 + ri as f32 * 22.0;
            let n_dots = (8 + ri * 4) as usize;
            let hue = (ri as f32 * 0.12 + tf * 0.006) % 1.0;
            for di in 0..n_dots {
                let a =
                    di as f32 / n_dots as f32 * std::f32::consts::TAU + ring_rot + ri as f32 * 0.3;
                let r = r_base + (a * 3.0 + tf * 0.05).sin() * 8.0;
                let px = (cx + a.cos() * r) as i32;
                let py = (cy + a.sin() * r) as i32;
                if (1..479).contains(&px) && (1..479).contains(&py) {
                    let col = hsl_to_rgb(hue, 0.9, 0.65);
                    for dy in -2i32..=2 {
                        for dx in -2i32..=2 {
                            if dx * dx + dy * dy <= 4 {
                                img.put_pixel((px + dx) as u32, (py + dy) as u32, col);
                            }
                        }
                    }
                }
            }
        }
    }

    // Centre glow
    for dy in -20i32..=20 {
        for dx in -20i32..=20 {
            let d = (dx * dx + dy * dy) as f32;
            if d <= 400.0 {
                let t = 1.0 - d / 400.0;
                let hue = (tf * 0.01) % 1.0;
                let col = hsl_to_rgb(hue, 1.0, 0.7 * t);
                let bx = cx as i32 + dx;
                let by = cy as i32 + dy;
                let p = img.get_pixel(bx as u32, by as u32);
                img.put_pixel(
                    bx as u32,
                    by as u32,
                    Rgb([
                        (p[0] as f32 + (col[0] as f32 - p[0] as f32) * t * 0.8).min(255.0) as u8,
                        (p[1] as f32 + (col[1] as f32 - p[1] as f32) * t * 0.8).min(255.0) as u8,
                        (p[2] as f32 + (col[2] as f32 - p[2] as f32) * t * 0.8).min(255.0) as u8,
                    ]),
                );
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Rose Curve ────────────────────────────────────────────────────────────────
// Animated parametric rose curve r = cos(k * theta) with rotating petals.
pub fn render_rose_curve() -> DynamicImage {
    static ROSE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = ROSE_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([16u8, 12, 24]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let cx = 240.0f32;
    let cy = 240.0f32;
    let scale = 200.0f32;

    // Draw several overlapping rose curves with different k values
    // k slowly evolves through non-integer values for complex petals
    let k_base = 2.0 + (tf * 0.002).sin() * 1.5;

    let curves: &[(f32, f32, f32, [u8; 3])] = &[
        (k_base, 0.0, 1.0, [137, 180, 250]),
        (k_base * 1.5, 0.3, 0.7, [203, 166, 247]),
        (k_base + 0.5, 1.0, 0.8, [166, 227, 161]),
        (k_base * 0.7, 2.1, 0.6, [249, 226, 175]),
    ];

    let steps = 3600usize;
    for &(k, phase, amp, col) in curves {
        // For non-integer k, need to go around far enough for full pattern
        let periods = if k.fract() < 0.01 {
            1.0
        } else {
            (k * 2.0).ceil() / 2.0
        };
        let theta_max = std::f32::consts::TAU * periods;
        let rot = tf * 0.008 + phase;

        for si in 0..=steps {
            let theta = si as f32 / steps as f32 * theta_max;
            let r = scale * amp * (k * theta).cos();
            let x = cx + r * (theta + rot).cos();
            let y = cy + r * (theta + rot).sin();
            let px = x as i32;
            let py = y as i32;
            if (1..479).contains(&px) && (1..479).contains(&py) {
                // Colour varies along curve
                let hue = ((theta / theta_max + tf * 0.004 + phase * 0.1) % 1.0 + 1.0) % 1.0;
                let rcol = hsl_to_rgb(hue, 0.9, 0.55);
                let _ = col;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let bx = px + dx;
                        let by = py + dy;
                        if (0..480).contains(&bx) && (0..480).contains(&by) {
                            let p = img.get_pixel(bx as u32, by as u32);
                            img.put_pixel(
                                bx as u32,
                                by as u32,
                                Rgb([
                                    ((p[0] as u32 + rcol[0] as u32) / 2).min(255) as u8,
                                    ((p[1] as u32 + rcol[1] as u32) / 2).min(255) as u8,
                                    ((p[2] as u32 + rcol[2] as u32) / 2).min(255) as u8,
                                ]),
                            );
                        }
                    }
                }
            }
        }
    }

    // Centre glow
    for dy in -8i32..=8 {
        for dx in -8i32..=8 {
            if dx * dx + dy * dy <= 64 {
                let bx = cx as i32 + dx;
                let by = cy as i32 + dy;
                if (0..480).contains(&bx) && (0..480).contains(&by) {
                    img.put_pixel(bx as u32, by as u32, Rgb([240, 230, 255]));
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Truchet Tiles ─────────────────────────────────────────────────────────────
// Animated Truchet tile pattern — each tile is a quarter circle, orientation
// chosen by a hash, creating flowing curved lines.
pub fn render_truchet() -> DynamicImage {
    static TRUCHET_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = TRUCHET_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let tile_size = 40u32;
    let cols = 480_u32.div_ceil(tile_size);
    let rows = 480_u32.div_ceil(tile_size);

    let mut img = RgbImage::new(480, 480);

    // Background: dark
    let bg = Rgb([20u8, 16, 28]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let stroke_w = 5.0f32;

    // Every few seconds, flip a random subset of tiles
    let era = (tick / 45) as u32;

    for row in 0..rows {
        for col in 0..cols {
            // Tile orientation from hash (with era for slow mutation)
            let mut h = col
                .wrapping_mul(31337u32)
                .wrapping_add(row.wrapping_mul(73856093u32));
            h = h.wrapping_add(era.wrapping_mul(2_246_822_519u32));
            h = h.wrapping_mul(2_654_435_761);
            let flip = (h >> 31) == 0; // orientation bit

            // Tile colour: hue from position + time
            let hue = ((col as f32 * 0.08 + row as f32 * 0.06 + tf * 0.005) % 1.0 + 1.0) % 1.0;
            let col_rgb = hsl_to_rgb(hue, 0.8, 0.55);

            let ox = (col * tile_size) as f32;
            let oy = (row * tile_size) as f32;
            let ts = tile_size as f32;

            // Draw two quarter-circle arcs
            // flip=false: arcs at (0,0)→(ts,0) and (0,ts)→(ts,ts)
            //             i.e., top-right and bottom-left corners
            // flip=true:  arcs at (0,0)→(0,ts) and (ts,0)→(ts,ts)
            //             i.e., top-left and bottom-right corners

            let r = ts / 2.0;
            let arc_steps = 40usize;
            let draw_arc = |img: &mut RgbImage,
                            arc_cx: f32,
                            arc_cy: f32,
                            start_a: f32,
                            end_a: f32,
                            col: Rgb<u8>| {
                for si in 0..=arc_steps {
                    let t = si as f32 / arc_steps as f32;
                    let a = start_a + (end_a - start_a) * t;
                    let ax = ox + arc_cx + a.cos() * r;
                    let ay = oy + arc_cy + a.sin() * r;
                    // Draw thick point
                    let sw = stroke_w as i32;
                    for dy in -sw..=sw {
                        for dx in -sw..=sw {
                            let d = (dx * dx + dy * dy) as f32;
                            if d <= stroke_w * stroke_w {
                                let bx = ax as i32 + dx;
                                let by = ay as i32 + dy;
                                if (0..480).contains(&bx) && (0..480).contains(&by) {
                                    img.put_pixel(bx as u32, by as u32, col);
                                }
                            }
                        }
                    }
                }
            };

            if !flip {
                // Top-right corner arc: centre at (ts, 0), from 180° to 270°
                draw_arc(
                    &mut img,
                    ts,
                    0.0,
                    std::f32::consts::PI,
                    std::f32::consts::PI + std::f32::consts::FRAC_PI_2,
                    col_rgb,
                );
                // Bottom-left corner arc: centre at (0, ts), from 0° to 90°
                draw_arc(&mut img, 0.0, ts, 0.0, std::f32::consts::FRAC_PI_2, col_rgb);
            } else {
                // Top-left corner arc: centre at (0, 0), from 0° to 90° (→ 270° to 360°)
                draw_arc(
                    &mut img,
                    0.0,
                    0.0,
                    -std::f32::consts::FRAC_PI_2,
                    0.0,
                    col_rgb,
                );
                // Bottom-right corner arc: centre at (ts, ts), from 90° to 180°
                draw_arc(
                    &mut img,
                    ts,
                    ts,
                    std::f32::consts::FRAC_PI_2,
                    std::f32::consts::PI,
                    col_rgb,
                );
            }
        }
    }

    let _ = tf;
    DynamicImage::ImageRgb8(img)
}

// ── Fourier Epicycles ─────────────────────────────────────────────────────────
// A chain of Fourier epicycles (rotating circles) that together trace a figure.
pub fn render_fourier_epicycles() -> DynamicImage {
    static FOURIER_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = FOURIER_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([14u8, 12, 20]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let cx = 240.0f32;
    let cy = 200.0f32;

    // Fourier components: (freq_multiplier, amplitude, initial_phase)
    // These approximate a square wave
    let components: &[(f32, f32, f32)] = &[
        (1.0, 80.0, 0.0),
        (3.0, 26.67, 0.0),
        (5.0, 16.0, 0.0),
        (7.0, 11.43, 0.0),
        (9.0, 8.89, 0.0),
        (11.0, 7.27, 0.0),
        (13.0, 6.15, 0.0),
    ];

    let base_freq = tf * 0.02;
    let n_trace = 600usize;

    // Trace the full path for the last N_TRACE steps
    let mut path: Vec<(f32, f32)> = Vec::with_capacity(n_trace);
    for ti in 0..=n_trace {
        let t = base_freq - ti as f32 * 0.02;
        let mut x = cx;
        #[allow(unused_mut)]
        let mut y = cy;
        for &(freq, amp, phase) in components {
            let angle = freq * t + phase;
            x += angle.cos() * amp;
            y += angle.sin() * amp;
        }
        path.push((x, y));
    }

    // Draw traced path
    for (i, &(px, py)) in path.iter().enumerate() {
        let age = n_trace - i;
        let alpha = (1.0 - age as f32 / n_trace as f32).powi(2);
        let hue = (age as f32 / n_trace as f32 * 0.7 + tf * 0.003) % 1.0;
        let col = hsl_to_rgb(hue, 0.9, 0.5 + alpha * 0.2);
        let bx = px as i32;
        let by = py as i32;
        if (0..480).contains(&bx) && (0..480).contains(&by) {
            let p = img.get_pixel(bx as u32, by as u32);
            img.put_pixel(
                bx as u32,
                by as u32,
                Rgb([
                    (p[0] as f32 * (1.0 - alpha) + col[0] as f32 * alpha).min(255.0) as u8,
                    (p[1] as f32 * (1.0 - alpha) + col[1] as f32 * alpha).min(255.0) as u8,
                    (p[2] as f32 * (1.0 - alpha) + col[2] as f32 * alpha).min(255.0) as u8,
                ]),
            );
        }
    }

    // Draw live epicycles
    let mut x = cx;
    let mut y = cy;
    for &(freq, amp, phase) in components {
        let angle = freq * base_freq + phase;
        let nx = x + angle.cos() * amp;
        let ny = y + angle.sin() * amp;

        // Draw circle
        let circle_steps = 80usize;
        for ci in 0..=circle_steps {
            let ca = ci as f32 / circle_steps as f32 * std::f32::consts::TAU;
            let cpx = (x + ca.cos() * amp) as i32;
            let cpy = (y + ca.sin() * amp) as i32;
            if (0..480).contains(&cpx) && (0..480).contains(&cpy) {
                let p = img.get_pixel(cpx as u32, cpy as u32);
                img.put_pixel(
                    cpx as u32,
                    cpy as u32,
                    Rgb([
                        (p[0] as f32 * 0.6 + 40.0).min(255.0) as u8,
                        (p[1] as f32 * 0.6 + 40.0).min(255.0) as u8,
                        (p[2] as f32 * 0.6 + 60.0).min(255.0) as u8,
                    ]),
                );
            }
        }

        // Draw radius arm
        let dx = (nx - x).abs() as i32;
        let dy_abs = (ny - y).abs() as i32;
        let steps = dx.max(dy_abs).max(1);
        for si in 0..=steps {
            let t = si as f32 / steps as f32;
            let apx = (x + (nx - x) * t) as i32;
            let apy = (y + (ny - y) * t) as i32;
            if (0..480).contains(&apx) && (0..480).contains(&apy) {
                img.put_pixel(apx as u32, apy as u32, Rgb([120, 120, 160]));
            }
        }

        x = nx;
        y = ny;
    }

    // Draw tip dot
    let tip_x = x as i32;
    let tip_y = y as i32;
    for dy in -3i32..=3 {
        for dx in -3i32..=3 {
            if dx * dx + dy * dy <= 9 {
                let bx = tip_x + dx;
                let by = tip_y + dy;
                if (0..480).contains(&bx) && (0..480).contains(&by) {
                    img.put_pixel(bx as u32, by as u32, Rgb([255, 240, 200]));
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Rule 30 ───────────────────────────────────────────────────────────────────
// 1D cellular automaton using Wolfram Rule 30 (or Rule 90 by era), scrolling
// downward, colour-coded by cell history.
pub fn render_rule_30() -> DynamicImage {
    const W: usize = 240; // cells wide (each cell = 2px)
    const H: usize = 240; // rows visible
    static RULE30_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<Vec<u8>>, u64)>> =
        std::sync::OnceLock::new();

    let mut guard = RULE30_STATE
        .get_or_init(|| {
            let mut rows = vec![vec![0u8; W]; H];
            // Seed: single 1 at center of the newest (bottom) row, which is the
            // one each tick evolves from.
            rows[H - 1][W / 2] = 1;
            std::sync::Mutex::new((rows, 0u64))
        })
        .lock()
        .unwrap();
    let (ref mut rows, ref mut tick) = *guard;
    *tick += 1;

    // Evolve one new row (append at bottom, drop top)
    if rows[H - 1].iter().all(|&c| c == 0) {
        rows[H - 1][W / 2] = 1; // pattern died out — reseed
    }
    let last = &rows[H - 1].clone();
    let mut new_row = vec![0u8; W];
    // Alternate between Rule 30 (era 0) and Rule 90 (era 1) every 120 ticks
    let era = (*tick / 120) % 2;
    let rule: u8 = if era == 0 { 30 } else { 90 };
    for x in 0..W {
        let left = if x > 0 { last[x - 1] } else { last[W - 1] };
        let center = last[x];
        let right = if x + 1 < W { last[x + 1] } else { last[0] };
        let pattern = (left << 2) | (center << 1) | right;
        new_row[x] = (rule >> pattern) & 1;
    }
    rows.remove(0);
    rows.push(new_row);

    let tf = *tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([12u8, 10, 16]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Render: each cell is 2×2 px, drawn in a 480×480 region with 2px border
    for (ry, row) in rows.iter().enumerate().take(H) {
        for (rx, &cell) in row.iter().enumerate().take(W) {
            if cell == 0 {
                continue;
            }
            let px0 = rx * 2;
            let py0 = ry * 2;
            // Colour: hue based on position and time
            let hue = ((rx as f32 / W as f32 * 0.5 + ry as f32 / H as f32 * 0.5 + tf * 0.005)
                % 1.0
                + 1.0)
                % 1.0;
            let col = hsl_to_rgb(hue, 0.85, 0.55);
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let bx = px0 + dx;
                    let by = py0 + dy;
                    if bx < 480 && by < 480 {
                        img.put_pixel(bx as u32, by as u32, col);
                    }
                }
            }
        }
    }

    // Label current rule
    let rule_label = if era == 0 { "Rule 30" } else { "Rule 90" };
    let _ = rule_label;

    DynamicImage::ImageRgb8(img)
}

// ── Lemniscate of Bernoulli ──────────────────────────────────────────────────
// The figure-eight / infinity symbol drawn as an animated parametric curve with
// evolving parameters.
pub fn render_lemniscate() -> DynamicImage {
    static LEMN_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = LEMN_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([14u8, 10, 20]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    let cx = 240.0f32;
    let cy = 240.0f32;

    // Draw several lemniscate/Cassini variants
    // General: r^2 = a^2 * cos(2*theta) or r^2 = 2*a^2 * cos(2*theta - phi)
    let curves: &[(f32, f32, f32, f32, f32)] = &[
        (200.0, 1.0, 0.0, 0.0, 0.0),
        (175.0, 0.95, 0.5, 1.05, 0.4),
        (160.0, 1.05, 1.0, 2.10, 0.8),
        (140.0, 0.9, 1.5, 3.15, 1.2),
    ];

    for &(a_base, a_vary, phase, hue_off, phase2) in curves {
        let a = a_base * (1.0 + a_vary * 0.05 * (tf * 0.01 + phase2).sin());
        let rot = tf * 0.007 + phase;
        let steps = 3600usize;
        for si in 0..=steps {
            let theta = si as f32 / steps as f32 * std::f32::consts::TAU;
            let r2_val = a * a * (2.0 * theta).cos();
            if r2_val < 0.0 {
                continue;
            }
            let r = r2_val.sqrt();
            let x = cx + r * (theta + rot).cos();
            let y = cy + r * (theta + rot).sin();
            let px = x as i32;
            let py = y as i32;
            if (1..479).contains(&px) && (1..479).contains(&py) {
                let hue = ((theta / std::f32::consts::TAU + hue_off * 0.2 + tf * 0.004) % 1.0
                    + 1.0)
                    % 1.0;
                let col = hsl_to_rgb(hue, 0.9, 0.55);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let bx = px + dx;
                        let by = py + dy;
                        if (0..480).contains(&bx) && (0..480).contains(&by) {
                            let p = img.get_pixel(bx as u32, by as u32);
                            img.put_pixel(
                                bx as u32,
                                by as u32,
                                Rgb([
                                    ((p[0] as u32 + col[0] as u32 * 2) / 3).min(255) as u8,
                                    ((p[1] as u32 + col[1] as u32 * 2) / 3).min(255) as u8,
                                    ((p[2] as u32 + col[2] as u32 * 2) / 3).min(255) as u8,
                                ]),
                            );
                        }
                    }
                }
            }
        }
    }

    // Centre dot
    for dy in -5i32..=5 {
        for dx in -5i32..=5 {
            if dx * dx + dy * dy <= 25 {
                img.put_pixel(
                    (cx as i32 + dx) as u32,
                    (cy as i32 + dy) as u32,
                    Rgb([250, 240, 255]),
                );
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}
