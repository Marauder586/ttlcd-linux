//! Screensavers that step a physical simulation.

use super::*;

// ─── Boids / Flocking ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Boid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    color_idx: usize,
}

pub(super) static BOIDS_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<Boid>>> =
    std::sync::OnceLock::new();

/// Craig Reynolds' Boids flocking simulation — 80 coloured agents.
/// Three steering rules: separation, alignment, cohesion.
pub fn render_boids() -> DynamicImage {
    const N: usize = 80;
    const SIZE: f32 = 480.0;
    const VIS: f32 = 75.0; // visual range
    const SEP: f32 = 10.0; // protected (separation) range
    const CENTER: f32 = 0.0005; // cohesion strength
    const AVOID: f32 = 0.04; // separation strength
    const MATCH: f32 = 0.05; // alignment strength
    const MAX_V: f32 = 5.5;
    const MIN_V: f32 = 2.5;
    const MARGIN: f32 = 50.0;
    const TURN: f32 = 0.4;

    let m = BOIDS_STATE.get_or_init(|| {
        let mut s = 0xABCD_EF01u32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let v: Vec<Boid> = (0..N)
            .map(|i| Boid {
                x: lcg(&mut s) * SIZE,
                y: lcg(&mut s) * SIZE,
                vx: (lcg(&mut s) - 0.5) * MAX_V * 2.0,
                vy: (lcg(&mut s) - 0.5) * MAX_V * 2.0,
                color_idx: i % 7,
            })
            .collect();
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
            let d = (dx * dx + dy * dy).sqrt();
            if d < SEP && d > 0.0 {
                cdx -= dx;
                cdy -= dy;
            } else if d < VIS {
                avx += o.vx;
                avy += o.vy;
                ax += o.x;
                ay += o.y;
                nb += 1;
            }
        }

        b.vx += cdx * AVOID;
        b.vy += cdy * AVOID;
        if nb > 0 {
            let n = nb as f32;
            b.vx += (avx / n - b.vx) * MATCH;
            b.vy += (avy / n - b.vy) * MATCH;
            b.vx += (ax / n - b.x) * CENTER;
            b.vy += (ay / n - b.y) * CENTER;
        }

        if b.x < MARGIN {
            b.vx += TURN;
        }
        if b.x > SIZE - MARGIN {
            b.vx -= TURN;
        }
        if b.y < MARGIN {
            b.vy += TURN;
        }
        if b.y > SIZE - MARGIN {
            b.vy -= TURN;
        }

        let spd = (b.vx * b.vx + b.vy * b.vy).sqrt();
        if spd > MAX_V {
            b.vx = b.vx / spd * MAX_V;
            b.vy = b.vy / spd * MAX_V;
        }
        if spd < MIN_V && spd > 0.0 {
            b.vx = b.vx / spd * MIN_V;
            b.vy = b.vy / spd * MIN_V;
        }

        b.x = (b.x + b.vx).clamp(0.0, SIZE - 1.0);
        b.y = (b.y + b.vy).clamp(0.0, SIZE - 1.0);
    }

    let colors = [BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for b in boids.iter() {
        let col = colors[b.color_idx % colors.len()];
        let x = b.x as i32;
        let y = b.y as i32;
        // Head dot (2×2)
        for dy in 0..2i32 {
            for dx in 0..2i32 {
                let px = x + dx;
                let py = y + dy;
                if (0..480).contains(&px) && (0..480).contains(&py) {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
        // Dim trail behind
        let tx = (b.x - b.vx * 1.2) as i32;
        let ty = (b.y - b.vy * 1.2) as i32;
        if (0..480).contains(&tx) && (0..480).contains(&ty) {
            img.put_pixel(
                tx as u32,
                ty as u32,
                Rgb([
                    (col[0] as u32 * 3 / 10) as u8,
                    (col[1] as u32 * 3 / 10) as u8,
                    (col[2] as u32 * 3 / 10) as u8,
                ]),
            );
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        462,
        PxScale::from(14.0),
        &font,
        &format!("{N} boids"),
    );
    DynamicImage::ImageRgb8(img)
}

// ─── Metaballs ────────────────────────────────────────────────────────────────

#[derive(Clone)]
#[allow(dead_code)]
pub(super) struct MetaBall {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,
    ci: usize,
}

pub(super) static METABALLS_STATE: std::sync::OnceLock<std::sync::Mutex<Vec<MetaBall>>> =
    std::sync::OnceLock::new();

/// Classic animated metaballs — 8 blobs bouncing around, blending smoothly.
/// Rendered at 240×240 (2×2 px cells) for performance.
pub fn render_metaballs() -> DynamicImage {
    const N: usize = 8;
    const CELL: u32 = 2;
    const GRID: u32 = 240; // 480 / CELL

    let m = METABALLS_STATE.get_or_init(|| {
        let mut s = 0xFEED_CAFEu32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let balls: Vec<MetaBall> = (0..N)
            .map(|i| MetaBall {
                x: lcg(&mut s) * 480.0,
                y: lcg(&mut s) * 480.0,
                vx: (lcg(&mut s) - 0.5) * 4.0,
                vy: (lcg(&mut s) - 0.5) * 4.0,
                r: 55.0 + lcg(&mut s) * 40.0,
                ci: i % 7,
            })
            .collect();
        std::sync::Mutex::new(balls)
    });
    let mut balls = m.lock().unwrap();

    // Move balls, bounce off walls
    for b in balls.iter_mut() {
        b.x += b.vx;
        b.y += b.vy;
        if b.x < 0.0 {
            b.x = 0.0;
            b.vx = b.vx.abs();
        }
        if b.x > 480.0 {
            b.x = 480.0;
            b.vx = -b.vx.abs();
        }
        if b.y < 0.0 {
            b.y = 0.0;
            b.vy = b.vy.abs();
        }
        if b.y > 480.0 {
            b.y = 480.0;
            b.vy = -b.vy.abs();
        }
    }

    let cols = [BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for cy in 0..GRID {
        for cx in 0..GRID {
            let pcx = (cx as f32 + 0.5) * CELL as f32;
            let pcy = (cy as f32 + 0.5) * CELL as f32;
            let mut field = 0.0f32;
            let mut dom_c = BG;
            let mut dom_v = 0.0f32;
            for b in balls.iter() {
                let dx = pcx - b.x;
                let dy = pcy - b.y;
                let d2 = (dx * dx + dy * dy).max(1.0);
                let v = b.r * b.r / d2;
                field += v;
                if v > dom_v {
                    dom_v = v;
                    dom_c = cols[b.ci % cols.len()];
                }
            }

            let col = if field >= 1.0 {
                let br = (field - 1.0).min(1.0) * 0.4 + 0.6;
                Rgb([
                    (dom_c[0] as f32 * br) as u8,
                    (dom_c[1] as f32 * br) as u8,
                    (dom_c[2] as f32 * br) as u8,
                ])
            } else if field > 0.55 {
                let g = (field - 0.55) / 0.45;
                Rgb([
                    (dom_c[0] as f32 * g * 0.28) as u8,
                    (dom_c[1] as f32 * g * 0.28) as u8,
                    (dom_c[2] as f32 * g * 0.28) as u8,
                ])
            } else {
                continue;
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

// ─── Falling Sand ─────────────────────────────────────────────────────────────

pub(super) struct SandState {
    cells: Vec<u8>,
    seed: u32,
    tick: u64,
}

pub(super) static SAND_STATE: std::sync::OnceLock<std::sync::Mutex<SandState>> =
    std::sync::OnceLock::new();

/// Cellular-automaton falling sand simulation.
/// 160×160 grid (3×3 px cells) — sand drains off the bottom; new grains
/// are injected from the top each tick.
pub fn render_sand() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3; // 160×3 = 480

    let m = SAND_STATE.get_or_init(|| {
        std::sync::Mutex::new(SandState {
            cells: vec![0u8; W * H],
            seed: 0xDEAD_C0DEu32,
            tick: 0,
        })
    });
    let mut st = m.lock().unwrap();
    st.tick += 1;

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };

    // Drain bottom row
    for x in 0..W {
        st.cells[(H - 1) * W + x] = 0;
    }

    // Inject a few grains at the top
    for _ in 0..5u32 {
        let x = (lcg(&mut st.seed) % W as u32) as usize;
        let col = 1 + (lcg(&mut st.seed) % 7) as u8;
        if st.cells[x] == 0 {
            st.cells[x] = col;
        }
    }

    // Update: bottom-to-top sweep (so moved grains aren't re-processed)
    for y in (0..(H - 1)).rev() {
        for x in 0..W {
            let c = st.cells[y * W + x];
            if c == 0 {
                continue;
            }
            // Try straight down
            if st.cells[(y + 1) * W + x] == 0 {
                st.cells[(y + 1) * W + x] = c;
                st.cells[y * W + x] = 0;
            } else {
                // Try diagonal (randomly prefer left or right)
                let go_left = x > 0 && st.cells[(y + 1) * W + x - 1] == 0;
                let go_right = x < W - 1 && st.cells[(y + 1) * W + x + 1] == 0;
                let chosen = match (go_left, go_right) {
                    (true, true) => {
                        if lcg(&mut st.seed) % 2 == 0 {
                            Some(x - 1)
                        } else {
                            Some(x + 1)
                        }
                    }
                    (true, _) => Some(x - 1),
                    (_, true) => Some(x + 1),
                    _ => None,
                };
                if let Some(nx) = chosen {
                    st.cells[(y + 1) * W + nx] = c;
                    st.cells[y * W + x] = 0;
                }
            }
        }
    }

    // Color table (0 = empty/BG, 1-7 = Catppuccin sand colours)
    let sand_cols: [Rgb<u8>; 8] = [BG, BLUE, GREEN, YELLOW, RED, MAUVE, TEAL, PEACH];
    // Slightly darken each sand colour for a "grain" look
    let tint = |c: Rgb<u8>| {
        Rgb([
            (c[0] as u32 * 9 / 10) as u8,
            (c[1] as u32 * 9 / 10) as u8,
            (c[2] as u32 * 9 / 10) as u8,
        ])
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for cy in 0..H {
        for cx in 0..W {
            let c = st.cells[cy * W + cx];
            if c == 0 {
                continue;
            }
            let col = tint(sand_cols[c as usize % sand_cols.len()]);
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

// ─── N-body Gravity ───────────────────────────────────────────────────────────

#[derive(Clone)]
#[allow(dead_code)]
pub(super) struct GravBody {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    mass: f32,
    ci: usize,
}

pub(super) struct GravState {
    bodies: Vec<GravBody>,
    trails: Vec<std::collections::VecDeque<(f32, f32)>>,
}

pub(super) static GRAVITY_STATE: std::sync::OnceLock<std::sync::Mutex<GravState>> =
    std::sync::OnceLock::new();

/// N-body gravitational simulation — 10 coloured bodies with trails.
/// Bodies start in a stable circular arrangement and orbit under mutual attraction.
pub fn render_gravity() -> DynamicImage {
    const N: usize = 10;
    const TRAIL: usize = 60; // trail length in ticks
    const G: f32 = 350.0;
    const SOFT: f32 = 8.0; // softening to avoid singularities
    const DT: f32 = 0.45;
    const CENTER_X: f32 = 240.0;
    const CENTER_Y: f32 = 240.0;

    let m = GRAVITY_STATE.get_or_init(|| {
        let mut bodies = Vec::with_capacity(N);
        let mut trails: Vec<std::collections::VecDeque<(f32, f32)>> = (0..N)
            .map(|_| std::collections::VecDeque::with_capacity(TRAIL + 1))
            .collect();

        // One heavy central mass + N-1 orbiting bodies
        bodies.push(GravBody {
            x: CENTER_X,
            y: CENTER_Y,
            vx: 0.0,
            vy: 0.0,
            mass: 200.0,
            ci: 0,
        });
        for i in 1..N {
            let angle = (i as f32) / ((N - 1) as f32) * std::f32::consts::TAU;
            let radius = 60.0 + (i as f32) * 15.0;
            let orbit_v = (G * 200.0 / radius).sqrt(); // circular orbit speed
            bodies.push(GravBody {
                x: CENTER_X + angle.cos() * radius,
                y: CENTER_Y + angle.sin() * radius,
                vx: -angle.sin() * orbit_v,
                vy: angle.cos() * orbit_v,
                mass: 5.0 + (i as f32) * 2.0,
                ci: i % 7,
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
            if i == j {
                continue;
            }
            let dx = bodies_snap[j].x - bodies_snap[i].x;
            let dy = bodies_snap[j].y - bodies_snap[i].y;
            let d2 = dx * dx + dy * dy + SOFT * SOFT;
            let d = d2.sqrt();
            let f = G * bodies_snap[j].mass / d2;
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
        st.bodies[i].x += st.bodies[i].vx * DT;
        st.bodies[i].y += st.bodies[i].vy * DT;
        // Soft wrap
        if st.bodies[i].x < 0.0 {
            st.bodies[i].x += 480.0;
        }
        if st.bodies[i].x > 480.0 {
            st.bodies[i].x -= 480.0;
        }
        if st.bodies[i].y < 0.0 {
            st.bodies[i].y += 480.0;
        }
        if st.bodies[i].y > 480.0 {
            st.bodies[i].y -= 480.0;
        }

        // Record trail (read position first to satisfy borrow checker)
        let pos = (st.bodies[i].x, st.bodies[i].y);
        st.trails[i].push_back(pos);
        while st.trails[i].len() > TRAIL {
            st.trails[i].pop_front();
        }
    }

    let cols = [
        YELLOW, BLUE, GREEN, RED, MAUVE, TEAL, PEACH, TEXT, SUBTEXT, OVERLAY0,
    ];
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Draw trails (fading alpha by position in trail)
    for (i, trail) in st.trails.iter().enumerate() {
        let base = cols[i % cols.len()];
        let len = trail.len();
        for (idx, &(tx, ty)) in trail.iter().enumerate() {
            let frac = idx as f32 / len as f32; // 0=oldest, 1=newest
            let br = frac * frac; // quadratic fade
            let px = tx as i32;
            let py = ty as i32;
            if (0..480).contains(&px) && (0..480).contains(&py) {
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
        let col = cols[i % cols.len()];
        let size = if i == 0 { 3i32 } else { 2i32 };
        let bx = b.x as i32;
        let by = b.y as i32;
        for dy in -size..=size {
            for dx in -size..=size {
                let px = bx + dx;
                let py = by + dy;
                if (0..480).contains(&px) && (0..480).contains(&py) {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Water Ripple ─────────────────────────────────────────────────────────────

pub(super) struct RippleState {
    /// 2D height field, 160×160 (f32 in -1..1)
    field: Vec<f32>,
    prev: Vec<f32>,
    seed: u32,
    tick: u64,
}

pub(super) static RIPPLE_STATE: std::sync::OnceLock<std::sync::Mutex<RippleState>> =
    std::sync::OnceLock::new();

/// Water ripple simulation using a simplified 2D wave equation.
/// New drops are added at random positions every ~15 ticks.
pub fn render_ripple() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3;
    const DAMP: f32 = 0.985; // damping factor

    let m = RIPPLE_STATE.get_or_init(|| {
        std::sync::Mutex::new(RippleState {
            field: vec![0.0f32; W * H],
            prev: vec![0.0f32; W * H],
            seed: 0xBAD_C0DE_u32,
            tick: 0,
        })
    });
    let mut st = m.lock().unwrap();
    st.tick += 1;

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };

    // Drop a new "stone" every 15 ticks
    if st.tick.is_multiple_of(15) {
        // Smooth (gaussian) bump — a hard-edged block rings as checkerboard noise.
        const R: i32 = 4;
        let x = (R + 1) as usize + (lcg(&mut st.seed) % (W as u32 - 2 * (R as u32 + 1))) as usize;
        let y = (R + 1) as usize + (lcg(&mut st.seed) % (H as u32 - 2 * (R as u32 + 1))) as usize;
        for dy in -R..=R {
            for dx in -R..=R {
                let bump = 0.9 * (-((dx * dx + dy * dy) as f32) / 5.0).exp();
                let i = (y as i32 + dy) as usize * W + (x as i32 + dx) as usize;
                st.field[i] = (st.field[i] + bump).min(1.0);
            }
        }
    }

    // Wave propagation: new = 2*cur - prev + c²*(laplacian), then damp
    let cur = st.field.clone();
    let prev = st.prev.clone();
    for y in 1..(H - 1) {
        for x in 1..(W - 1) {
            let lap = cur[(y - 1) * W + x]
                + cur[(y + 1) * W + x]
                + cur[y * W + x - 1]
                + cur[y * W + x + 1]
                - 4.0 * cur[y * W + x];
            st.field[y * W + x] =
                ((2.0 * cur[y * W + x] - prev[y * W + x] + 0.5 * lap) * DAMP).clamp(-1.0, 1.0);
        }
    }
    st.prev = cur;

    // Render: height → color
    // negative = one color, positive = another, zero = BG
    let mut img = RgbImage::new(480, 480);
    for cy in 0..H {
        for cx in 0..W {
            let h = st.field[cy * W + cx];
            let ah = h.abs();
            // Blend from the background towards the crest/trough colour so
            // small amplitudes fade out instead of dropping to black.
            let target: [f32; 3] = if h > 0.0 {
                [80.0, 220.0, 240.0] // crest: teal/cyan
            } else {
                [70.0, 80.0, 255.0] // trough: blue/purple
            };
            let k = (ah * 4.0).min(1.0);
            let col = Rgb([
                (BG[0] as f32 + (target[0] - BG[0] as f32) * k) as u8,
                (BG[1] as f32 + (target[1] - BG[1] as f32) * k) as u8,
                (BG[2] as f32 + (target[2] - BG[2] as f32) * k) as u8,
            ]);
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

// ─────────────────────────────────────────────────────────────────────────────
// render_pendulum_wave  — phase wave of 16 pendulums
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static PENDULUM_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_pendulum_wave() -> DynamicImage {
    let tick = {
        let m = PENDULUM_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.015;
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    const N: usize = 16;
    const PIVOT_Y: f32 = 50.0;
    const L_MIN: f32 = 100.0;
    const L_MAX: f32 = 380.0;

    let pivot_xs: Vec<f32> = (0..N)
        .map(|i| 30.0 + i as f32 * (420.0 / (N - 1) as f32))
        .collect();

    let col_cycle: [Rgb<u8>; 8] = [
        BLUE,
        TEAL,
        GREEN,
        YELLOW,
        PEACH,
        RED,
        MAUVE,
        Rgb([0x89, 0xdc, 0xeb]),
    ];

    for (i, &px) in pivot_xs.iter().enumerate() {
        // Length varies so that periods differ slightly
        let l = L_MIN + (i as f32 / (N - 1) as f32) * (L_MAX - L_MIN);
        // Angular frequency: omega = sqrt(g/l), normalised
        let omega = (9.81_f32 / l).sqrt() * 40.0; // scaling for screen
        let angle = 0.35_f32 * (omega * tf).sin();

        let bob_x = px + l * angle.sin();
        let bob_y = PIVOT_Y + l * angle.cos();

        let col = col_cycle[i % col_cycle.len()];

        // Draw string
        let steps = ((bob_y - PIVOT_Y).abs() as i32 + 1).max(2);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let sx = (px + (bob_x - px) * t) as i32;
            let sy = (PIVOT_Y + (bob_y - PIVOT_Y) * t) as i32;
            if (0..480).contains(&sx) && (0..480).contains(&sy) {
                let dim_col = Rgb([(col[0] / 3), (col[1] / 3), (col[2] / 3)]);
                img.put_pixel(sx as u32, sy as u32, dim_col);
            }
        }

        // Draw bob (circle)
        let bob_r = 8i32;
        for dy in -bob_r..=bob_r {
            for dx in -bob_r..=bob_r {
                if dx * dx + dy * dy <= bob_r * bob_r {
                    let bx = bob_x as i32 + dx;
                    let by = bob_y as i32 + dy;
                    if (0..480).contains(&bx) && (0..480).contains(&by) {
                        let bright = 1.0 - (dx * dx + dy * dy) as f32 / (bob_r * bob_r) as f32;
                        img.put_pixel(
                            bx as u32,
                            by as u32,
                            Rgb([
                                (col[0] as f32 * bright) as u8,
                                (col[1] as f32 * bright) as u8,
                                (col[2] as f32 * bright) as u8,
                            ]),
                        );
                    }
                }
            }
        }

        // Pivot dot
        let pd = PIVOT_Y as i32;
        if px >= 0.0 && (px as u32) < 480 && (0..480).contains(&pd) {
            img.put_pixel(px as u32, pd as u32, OVERLAY0);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Sand Waterfall ────────────────────────────────────────────────────────────
// Falling sand particle simulation — particles fall from a slit at top and
// pile up, creating a shifting heap.
pub fn render_sand_waterfall() -> DynamicImage {
    const W: usize = 120;
    const H: usize = 120;
    const CELL: usize = 4; // each cell = 4×4 pixels
    static SAND_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<u8>, u32, u64)>> =
        std::sync::OnceLock::new();

    let mut guard = SAND_STATE
        .get_or_init(|| std::sync::Mutex::new((vec![0u8; W * H], 0xCAFE_D00Du32, 0u64)))
        .lock()
        .unwrap();
    let (ref mut grid, ref mut seed, ref mut tick) = *guard;
    *tick += 1;

    let mut s = *seed;
    let mut rng = || -> f32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s as f32) / (u32::MAX as f32)
    };

    // Spawn particles from a slit near top center
    let slit_w = 4usize;
    let slit_x = W / 2 - slit_w / 2;
    if *tick % 2 == 0 {
        for cell in &mut grid[slit_x..slit_x + slit_w] {
            if *cell == 0 {
                // Random sand color: 1-8 (different sand shades)
                *cell = 1 + (rng() * 7.0) as u8;
            }
        }
    }

    // Update from bottom to top (so falling works correctly)
    for y in (0..H - 1).rev() {
        for x in 0..W {
            let v = grid[y * W + x];
            if v == 0 {
                continue;
            }
            // Try to fall straight down
            if grid[(y + 1) * W + x] == 0 {
                grid[(y + 1) * W + x] = v;
                grid[y * W + x] = 0;
            } else {
                // Try diagonal (random left or right bias)
                let left = x > 0 && grid[(y + 1) * W + x - 1] == 0;
                let right = x + 1 < W && grid[(y + 1) * W + x + 1] == 0;
                if left && right {
                    if rng() < 0.5 {
                        grid[(y + 1) * W + x - 1] = v;
                    } else {
                        grid[(y + 1) * W + x + 1] = v;
                    }
                    grid[y * W + x] = 0;
                } else if left {
                    grid[(y + 1) * W + x - 1] = v;
                    grid[y * W + x] = 0;
                } else if right {
                    grid[(y + 1) * W + x + 1] = v;
                    grid[y * W + x] = 0;
                }
            }
        }
    }
    *seed = s;

    // Sand color palette (warm sandy tones)
    let palette: [[u8; 3]; 8] = [
        [210, 180, 100], // light sand
        [195, 165, 80],  // medium sand
        [220, 190, 110], // bright sand
        [175, 145, 65],  // dark sand
        [230, 195, 120], // pale gold
        [205, 170, 90],  // amber
        [185, 155, 75],  // tan
        [215, 185, 105], // cream sand
    ];

    let mut img = RgbImage::new(480, 480);
    // Dark background
    for p in img.pixels_mut() {
        *p = Rgb([18u8, 12, 8]);
    }

    // Draw sand grid
    for y in 0..H {
        for x in 0..W {
            let v = grid[y * W + x];
            if v > 0 {
                let col = palette[(v - 1) as usize % 8];
                let px0 = x * CELL;
                let py0 = y * CELL;
                for dy in 0..CELL {
                    for dx in 0..CELL {
                        // Slight shading: top of cell is lighter
                        let shade = if dy == 0 {
                            1.1f32
                        } else if dy == CELL - 1 {
                            0.85
                        } else {
                            1.0
                        };
                        img.put_pixel(
                            (px0 + dx) as u32,
                            (py0 + dy) as u32,
                            Rgb([
                                (col[0] as f32 * shade).min(255.0) as u8,
                                (col[1] as f32 * shade).min(255.0) as u8,
                                (col[2] as f32 * shade).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Magnetic Field Lines ──────────────────────────────────────────────────────
// Two magnetic poles orbiting each other, field lines rendered as streamlines.
pub fn render_magnetic_field() -> DynamicImage {
    static MAG_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = MAG_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([10u8, 10, 20]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Two poles rotating
    let angle = tf * 0.018;
    let orbit_r = 100.0f32;
    let pole1 = (
        240.0f32 + angle.cos() * orbit_r,
        240.0f32 + angle.sin() * orbit_r,
    );
    let pole2 = (
        240.0f32 - angle.cos() * orbit_r,
        240.0f32 - angle.sin() * orbit_r,
    );
    let charge1 = 1.0f32;
    let charge2 = -1.0f32;

    // Field sampling function: sum of dipole contributions
    let field = |x: f32, y: f32| -> (f32, f32) {
        let dx1 = x - pole1.0;
        let dy1 = y - pole1.1;
        let r1sq = (dx1 * dx1 + dy1 * dy1).max(1.0);
        let r1 = r1sq.sqrt();
        let ex1 = charge1 * dx1 / (r1 * r1sq);
        let ey1 = charge1 * dy1 / (r1 * r1sq);
        let dx2 = x - pole2.0;
        let dy2 = y - pole2.1;
        let r2sq = (dx2 * dx2 + dy2 * dy2).max(1.0);
        let r2 = r2sq.sqrt();
        let ex2 = charge2 * dx2 / (r2 * r2sq);
        let ey2 = charge2 * dy2 / (r2 * r2sq);
        (ex1 + ex2, ey1 + ey2)
    };

    // Draw field lines: start from ring around positive pole, trace streamlines
    let n_lines = 20usize;
    for li in 0..n_lines {
        let start_angle = li as f32 / n_lines as f32 * std::f32::consts::TAU;
        let start_r = 18.0f32;
        let mut sx = pole1.0 + start_angle.cos() * start_r;
        let mut sy = pole1.1 + start_angle.sin() * start_r;

        let hue = li as f32 / n_lines as f32;
        let h6 = hue * 6.0;
        let hi = h6 as u32 % 6;
        let f = h6.fract();
        let (lr, lg, lb) = match hi {
            0 => (1.0f32, f, 0.0f32),
            1 => (1.0 - f, 1.0, 0.0),
            2 => (0.0, 1.0, f),
            3 => (0.0, 1.0 - f, 1.0),
            4 => (f, 0.0, 1.0),
            _ => (1.0, 0.0, 1.0 - f),
        };

        let step = 3.0f32;
        let max_steps = 200;
        for si in 0..max_steps {
            if !(0.0..480.0).contains(&sx) || !(0.0..480.0).contains(&sy) {
                break;
            }
            let (fx, fy) = field(sx, sy);
            let fmag = (fx * fx + fy * fy).sqrt().max(1e-6);
            let fxn = fx / fmag;
            let fyn = fy / fmag;

            let alpha = (0.8 - si as f32 / max_steps as f32 * 0.6).max(0.1);
            let px = sx as u32;
            let py = sy as u32;
            let p = img.get_pixel(px, py);
            img.put_pixel(
                px,
                py,
                Rgb([
                    (p[0] as f32 + lr * 180.0 * alpha).min(255.0) as u8,
                    (p[1] as f32 + lg * 180.0 * alpha).min(255.0) as u8,
                    (p[2] as f32 + lb * 180.0 * alpha).min(255.0) as u8,
                ]),
            );

            sx += fxn * step;
            sy += fyn * step;

            // Stop when reaching negative pole
            let to_neg = ((sx - pole2.0).powi(2) + (sy - pole2.1).powi(2)).sqrt();
            if to_neg < 15.0 {
                break;
            }
        }
    }

    // Draw pole markers
    for (pole, is_pos) in &[(pole1, true), (pole2, false)] {
        let (pr, pg, pb) = if *is_pos {
            (255u8, 80, 80)
        } else {
            (80u8, 80, 255)
        };
        let pr_size = 8i32;
        for dy in -pr_size..=pr_size {
            for dx in -pr_size..=pr_size {
                if dx * dx + dy * dy <= pr_size * pr_size {
                    let px = pole.0 as i32 + dx;
                    let py = pole.1 as i32 + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        img.put_pixel(px as u32, py as u32, Rgb([pr, pg, pb]));
                    }
                }
            }
        }
        // Label + / -
        let sym_x = pole.0 as i32;
        let sym_y = pole.1 as i32;
        if sym_x > 0 && sym_x < 479 && sym_y > 0 && sym_y < 479 {
            img.put_pixel(sym_x as u32, sym_y as u32, Rgb([255, 255, 255]));
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Smoke Plume ───────────────────────────────────────────────────────────────
// A wispy smoke column rising from a heat source at the bottom-center.
pub fn render_smoke_plume() -> DynamicImage {
    const W: usize = 80;
    const H: usize = 120;
    const CELL: usize = 4; // scaled
    static SMOKE_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<f32>, u32, u64)>> =
        std::sync::OnceLock::new();

    let mut guard = SMOKE_STATE
        .get_or_init(|| std::sync::Mutex::new((vec![0.0f32; W * H], 0xBABE_F00Du32, 0u64)))
        .lock()
        .unwrap();
    let (ref mut density, ref mut seed, ref mut tick) = *guard;
    *tick += 1;
    let tf = *tick as f32;

    let mut s = *seed;
    let mut rng = || -> f32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s as f32) / (u32::MAX as f32)
    };

    // Source: flickering emitter along the bottom two rows
    let cx = W / 2;
    for y in H - 2..H {
        for x in (cx.saturating_sub(3))..(cx + 4).min(W) {
            density[x + y * W] = (density[x + y * W] + 0.6 + rng() * 0.9).min(3.0);
        }
    }

    // Advect: each cell pulls density from below (buoyant rise), with a
    // swaying sideways drift that grows with height.
    const RISE: f32 = 1.3;
    let sample = |d: &[f32], x: f32, y: f32| -> f32 {
        if x < 0.0 || y < 0.0 || x > (W - 1) as f32 || y > (H - 1) as f32 {
            return 0.0;
        }
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(W - 1), (y0 + 1).min(H - 1));
        let (fx, fy) = (x.fract(), y.fract());
        let top = d[y0 * W + x0] * (1.0 - fx) + d[y0 * W + x1] * fx;
        let bot = d[y1 * W + x0] * (1.0 - fx) + d[y1 * W + x1] * fx;
        top * (1.0 - fy) + bot * fy
    };
    let mut new_d = vec![0.0f32; W * H];
    for y in 0..H - 2 {
        let height = (H - 1 - y) as f32 / H as f32;
        let sway = ((y as f32 * 0.07 + tf * 0.05).sin() * 0.6
            + (y as f32 * 0.19 - tf * 0.11).sin() * 0.4)
            * height
            * 0.9;
        for x in 0..W {
            let jitter = (rng() - 0.5) * 0.5;
            new_d[y * W + x] = sample(density, x as f32 - sway + jitter, y as f32 + RISE) * 0.988;
        }
    }
    for y in H - 2..H {
        for x in 0..W {
            new_d[y * W + x] = density[y * W + x] * 0.6;
        }
    }

    // Diffuse density horizontally (blur)
    for y in 0..H {
        for x in 1..W - 1 {
            new_d[y * W + x] = new_d[y * W + x] * 0.85
                + new_d[y * W + x - 1] * 0.075
                + new_d[y * W + x + 1] * 0.075;
        }
    }

    *density = new_d;
    *seed = s;

    let mut img = RgbImage::new(480, 480);
    // Dark warm background
    for py in 0..480u32 {
        let t = py as f32 / 480.0;
        img.put_pixel(0, py, Rgb([0u8, 0, 0])); // placeholder
        let r = (8.0 + t * 12.0) as u8;
        let g = (8.0 + t * 8.0) as u8;
        let b = (8.0 + t * 5.0) as u8;
        for px in 0..480u32 {
            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    // Render smoke (density → grey-white with blue tint at top)
    let offset_x = (480 - W * CELL) / 2;
    let offset_y = 480 - H * CELL;
    for y in 0..H {
        for x in 0..W {
            let d = density[y * W + x].min(1.5);
            if d < 0.01 {
                continue;
            }
            let height_frac = (H - 1 - y) as f32 / H as f32;
            let alpha = (d / 1.5).min(0.85);
            let base_v = 180.0 + height_frac * 60.0;
            // Smoke colour: warm grey near source, cool white/blue near top
            let (cr, cg, cb) = (
                base_v * (1.0 - height_frac * 0.2),
                base_v * (1.0 - height_frac * 0.05),
                base_v * (1.0 + height_frac * 0.1),
            );
            let px0 = offset_x + x * CELL;
            let py0 = offset_y + y * CELL;
            for dy in 0..CELL {
                for dx in 0..CELL {
                    let px = px0 + dx;
                    let py2 = py0 + dy;
                    if px < 480 && py2 < 480 {
                        let p = img.get_pixel(px as u32, py2 as u32);
                        img.put_pixel(
                            px as u32,
                            py2 as u32,
                            Rgb([
                                (p[0] as f32 * (1.0 - alpha) + cr * alpha).min(255.0) as u8,
                                (p[1] as f32 * (1.0 - alpha) + cg * alpha).min(255.0) as u8,
                                (p[2] as f32 * (1.0 - alpha) + cb * alpha).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    // Heat source glow at bottom
    for dy in -8i32..=8 {
        for dx in -20i32..=20 {
            let dist = ((dx * dx) as f32 * 0.5 + (dy * dy) as f32).sqrt();
            let heat = (1.0 - dist / 14.0).max(0.0);
            if heat > 0.0 {
                let px = (240 + dx) as u32;
                let py = (479 + dy) as u32;
                if px < 480 && py < 480 {
                    let p = img.get_pixel(px, py);
                    img.put_pixel(
                        px,
                        py,
                        Rgb([
                            (p[0] as f32 + 200.0 * heat).min(255.0) as u8,
                            (p[1] as f32 + 80.0 * heat).min(255.0) as u8,
                            (p[2] as f32 + 10.0 * heat).min(255.0) as u8,
                        ]),
                    );
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Flow Field ────────────────────────────────────────────────────────────────
// Hundreds of particles flowing through a hash-noise vector field.
pub fn render_flow_field() -> DynamicImage {
    struct FlowParticle {
        x: f32,
        y: f32,
        age: u32,
        hue: f32,
    }
    type FlowState = (Vec<FlowParticle>, Vec<[u8; 3]>, u32, u64);
    static FF_STATE: std::sync::OnceLock<std::sync::Mutex<FlowState>> = std::sync::OnceLock::new();

    let mut guard = FF_STATE
        .get_or_init(|| {
            std::sync::Mutex::new((
                Vec::new(),
                vec![[8u8, 8, 10]; 480 * 480],
                0xF10F_1E1Du32,
                0u64,
            ))
        })
        .lock()
        .unwrap();
    let (ref mut particles, ref mut trail_buf, ref mut seed, ref mut tick) = *guard;
    *tick += 1;
    let tf = *tick as f32;

    let mut s = *seed;
    let mut rng = || -> f32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s as f32) / (u32::MAX as f32)
    };

    // Smooth, slowly drifting angle field.  (A per-cell hash here gives a
    // piecewise-constant field, which traps particles on cell borders and
    // draws staircases instead of curves.)
    let noise_angle = |x: f32, y: f32, t: f32| -> f32 {
        ((x * 0.011 + t * 0.004).sin()
            + (y * 0.013 - t * 0.003).cos()
            + ((x + y) * 0.007 + t * 0.002).sin()
            + ((x - y) * 0.005).cos())
            * std::f32::consts::PI
    };

    // Spawn new particles
    while particles.len() < 320 {
        particles.push(FlowParticle {
            x: rng() * 480.0,
            y: rng() * 480.0,
            age: 0,
            hue: rng(),
        });
    }

    // Fade trail buffer
    for px in trail_buf.iter_mut() {
        px[0] = (px[0] as f32 * 0.965 + 8.0 * 0.035) as u8;
        px[1] = (px[1] as f32 * 0.965 + 8.0 * 0.035) as u8;
        px[2] = (px[2] as f32 * 0.965 + 10.0 * 0.035) as u8;
    }

    // Update particles
    for p in particles.iter_mut() {
        p.age += 1;
        let angle = noise_angle(p.x, p.y, tf);
        let speed = 1.8f32;
        p.x += angle.cos() * speed;
        p.y += angle.sin() * speed;

        // Wrap around
        p.x = ((p.x % 480.0) + 480.0) % 480.0;
        p.y = ((p.y % 480.0) + 480.0) % 480.0;

        // Die at max age and respawn
        if p.age > 180 {
            p.x = rng() * 480.0;
            p.y = rng() * 480.0;
            p.age = 0;
            p.hue = rng();
        }

        // Write to trail buffer
        let px_int = p.x as usize;
        let py_int = p.y as usize;
        if px_int < 480 && py_int < 480 {
            let alpha = if p.age < 10 {
                p.age as f32 / 10.0 * 0.8
            } else if p.age > 160 {
                (180 - p.age) as f32 / 20.0 * 0.8
            } else {
                0.8
            };
            let col = hsl_to_rgb(p.hue, 0.9, 0.55);
            let idx = py_int * 480 + px_int;
            trail_buf[idx][0] =
                (trail_buf[idx][0] as f32 * (1.0 - alpha) + col[0] as f32 * alpha).min(255.0) as u8;
            trail_buf[idx][1] =
                (trail_buf[idx][1] as f32 * (1.0 - alpha) + col[1] as f32 * alpha).min(255.0) as u8;
            trail_buf[idx][2] =
                (trail_buf[idx][2] as f32 * (1.0 - alpha) + col[2] as f32 * alpha).min(255.0) as u8;
        }
    }
    *seed = s;

    // Build image from trail buffer
    let mut img = RgbImage::new(480, 480);
    for py in 0..480u32 {
        for px in 0..480u32 {
            let tb = &trail_buf[py as usize * 480 + px as usize];
            img.put_pixel(px, py, Rgb([tb[0], tb[1], tb[2]]));
        }
    }

    let _ = tf;
    DynamicImage::ImageRgb8(img)
}

// ── Double Pendulum ───────────────────────────────────────────────────────────
// Chaotic double pendulum with a colourful traced trail.
pub fn render_double_pendulum() -> DynamicImage {
    struct DPState {
        th1: f32,
        th2: f32,
        dth1: f32,
        dth2: f32,
        trail: std::collections::VecDeque<(f32, f32, u64)>,
        tick: u64,
    }
    static DP_STATE: std::sync::OnceLock<std::sync::Mutex<DPState>> = std::sync::OnceLock::new();

    let mut guard = DP_STATE
        .get_or_init(|| {
            std::sync::Mutex::new(DPState {
                th1: std::f32::consts::FRAC_PI_2 + 0.3,
                th2: std::f32::consts::PI - 0.5,
                dth1: 0.0,
                dth2: 0.0,
                trail: std::collections::VecDeque::with_capacity(500),
                tick: 0,
            })
        })
        .lock()
        .unwrap();

    guard.tick += 1;
    let tick = guard.tick;

    let l1 = 1.0f32;
    let l2 = 1.0f32;
    let m1 = 1.0f32;
    let m2 = 1.0f32;
    let g = 9.81f32;
    let dt = 0.04f32;

    // Runge-Kutta 4 step
    let dp_deriv = |th1: f32, th2: f32, dth1: f32, dth2: f32| -> (f32, f32, f32, f32) {
        let delta = th2 - th1;
        let cos_d = delta.cos();
        let sin_d = delta.sin();
        let denom1 = (m1 + m2) * l1 - m2 * l1 * cos_d * cos_d;
        let denom2 = (l2 / l1) * denom1;
        let ddth1 = (m2 * l1 * dth1 * dth1 * sin_d * cos_d
            + m2 * g * th2.sin() * cos_d
            + m2 * l2 * dth2 * dth2 * sin_d
            - (m1 + m2) * g * th1.sin())
            / denom1;
        let ddth2 = (-(l1 / l2) * (m1 + m2) * dth1 * dth1 * sin_d * cos_d
            + (m1 + m2) * g * th1.sin() * cos_d
            - (m1 + m2) * g * th2.sin()
            - m2 * l2 * dth2 * dth2 * sin_d * cos_d)
            / denom2
            * 0.0
            + (-m2 * l2 * dth2 * dth2 * sin_d - (m1 + m2) * g * th2.sin()
                + m2 * l1 * dth1 * dth1 * sin_d
                + (m1 + m2) * g * th1.sin() * cos_d)
                / (l2 * (m1 + m2 - m2 * cos_d * cos_d));
        (dth1, dth2, ddth1, ddth2)
    };

    // Several sub-steps per frame for accuracy
    for _ in 0..3 {
        let (k1a, k1b, k1c, k1d) = dp_deriv(guard.th1, guard.th2, guard.dth1, guard.dth2);
        let (k2a, k2b, k2c, k2d) = dp_deriv(
            guard.th1 + k1a * dt / 2.0,
            guard.th2 + k1b * dt / 2.0,
            guard.dth1 + k1c * dt / 2.0,
            guard.dth2 + k1d * dt / 2.0,
        );
        let (k3a, k3b, k3c, k3d) = dp_deriv(
            guard.th1 + k2a * dt / 2.0,
            guard.th2 + k2b * dt / 2.0,
            guard.dth1 + k2c * dt / 2.0,
            guard.dth2 + k2d * dt / 2.0,
        );
        let (k4a, k4b, k4c, k4d) = dp_deriv(
            guard.th1 + k3a * dt,
            guard.th2 + k3b * dt,
            guard.dth1 + k3c * dt,
            guard.dth2 + k3d * dt,
        );

        guard.th1 += dt / 6.0 * (k1a + 2.0 * k2a + 2.0 * k3a + k4a);
        guard.th2 += dt / 6.0 * (k1b + 2.0 * k2b + 2.0 * k3b + k4b);
        guard.dth1 += dt / 6.0 * (k1c + 2.0 * k2c + 2.0 * k3c + k4c);
        guard.dth2 += dt / 6.0 * (k1d + 2.0 * k2d + 2.0 * k3d + k4d);
    }

    let scale = 100.0f32;
    let ox = 240.0f32;
    let oy = 160.0f32;
    let x1 = ox + guard.th1.sin() * scale * l1;
    let y1 = oy + guard.th1.cos() * scale * l1;
    let x2 = x1 + guard.th2.sin() * scale * l2;
    let y2 = y1 + guard.th2.cos() * scale * l2;

    guard.trail.push_back((x2, y2, tick));
    while guard.trail.len() > 480 {
        guard.trail.pop_front();
    }

    let trail_snapshot: Vec<(f32, f32, u64)> = guard.trail.iter().copied().collect();
    let (x1s, y1s) = (x1, y1);
    let (x2s, y2s) = (x2, y2);
    let th1s = guard.th1;
    let th2s = guard.th2;
    drop(guard);

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([16u8, 12, 20]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Draw trail
    let trail_len = trail_snapshot.len();
    for (i, &(tx, ty, _)) in trail_snapshot.iter().enumerate() {
        let t = i as f32 / trail_len as f32;
        let hue = (t * 0.7 + tick as f32 * 0.003) % 1.0;
        let lit = 0.25 + t * 0.35;
        let col = hsl_to_rgb(hue, 0.9, lit);
        let tpx = tx as i32;
        let tpy = ty as i32;
        if (0..480).contains(&tpx) && (0..480).contains(&tpy) {
            img.put_pixel(tpx as u32, tpy as u32, col);
        }
    }

    // Draw pendulum arms
    let draw_line = |img: &mut RgbImage, ax: f32, ay: f32, bx: f32, by: f32, col: Rgb<u8>| {
        let dx = (bx - ax).abs() as i32;
        let dy = (by - ay).abs() as i32;
        let steps = dx.max(dy).max(1);
        for si in 0..=steps {
            let t = si as f32 / steps as f32;
            let px = (ax + (bx - ax) * t) as i32;
            let py = (ay + (by - ay) * t) as i32;
            if (0..480).contains(&px) && (0..480).contains(&py) {
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    };
    let _ = (th1s, th2s);

    draw_line(&mut img, ox, oy, x1s, y1s, Rgb([180u8, 180, 200]));
    draw_line(&mut img, x1s, y1s, x2s, y2s, Rgb([200u8, 180, 200]));

    // Draw bobs
    for (bx, by, br) in &[
        (ox as i32, oy as i32, 5i32),
        (x1s as i32, y1s as i32, 7),
        (x2s as i32, y2s as i32, 8),
    ] {
        for dy in -br..=*br {
            for dx in -br..=*br {
                if dx * dx + dy * dy <= br * br {
                    let px = bx + dx;
                    let py = by + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        img.put_pixel(px as u32, py as u32, Rgb([220u8, 200, 240]));
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── N-Body Gravity ────────────────────────────────────────────────────────────
// 4 bodies orbiting under mutual gravitational attraction with trails.
pub fn render_nbody_gravity() -> DynamicImage {
    struct Body {
        x: f32,
        y: f32,
        vx: f32,
        vy: f32,
        mass: f32,
        hue: f32,
    }
    /// (bodies, a position trail per body, frame counter)
    type NBodyState = (Vec<Body>, Vec<std::collections::VecDeque<(f32, f32)>>, u64);
    static NBODY_STATE: std::sync::OnceLock<std::sync::Mutex<NBodyState>> =
        std::sync::OnceLock::new();

    let mut guard = NBODY_STATE
        .get_or_init(|| {
            let bodies = vec![
                Body {
                    x: 240.0,
                    y: 160.0,
                    vx: 1.2,
                    vy: 0.3,
                    mass: 3.0,
                    hue: 0.0,
                },
                Body {
                    x: 300.0,
                    y: 280.0,
                    vx: -0.8,
                    vy: 1.1,
                    mass: 2.0,
                    hue: 0.25,
                },
                Body {
                    x: 160.0,
                    y: 300.0,
                    vx: 0.5,
                    vy: -1.4,
                    mass: 2.5,
                    hue: 0.55,
                },
                Body {
                    x: 280.0,
                    y: 380.0,
                    vx: -1.0,
                    vy: -0.2,
                    mass: 1.5,
                    hue: 0.75,
                },
            ];
            let trails = vec![
                std::collections::VecDeque::with_capacity(300),
                std::collections::VecDeque::with_capacity(300),
                std::collections::VecDeque::with_capacity(300),
                std::collections::VecDeque::with_capacity(300),
            ];
            std::sync::Mutex::new((bodies, trails, 0u64))
        })
        .lock()
        .unwrap();
    let (ref mut bodies, ref mut trails, ref mut tick) = *guard;
    *tick += 1;

    let g = 800.0f32;
    let dt = 0.15f32;
    let softening = 18.0f32; // softening radius to avoid singularity

    // Compute forces
    let n = bodies.len();
    let mut forces: Vec<(f32, f32)> = vec![(0.0, 0.0); n];
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = bodies[j].x - bodies[i].x;
            let dy = bodies[j].y - bodies[i].y;
            let dist = (dx * dx + dy * dy + softening * softening).sqrt();
            let force = g * bodies[i].mass * bodies[j].mass / (dist * dist);
            let fx = force * dx / dist;
            let fy = force * dy / dist;
            forces[i].0 += fx;
            forces[i].1 += fy;
            forces[j].0 -= fx;
            forces[j].1 -= fy;
        }
    }

    // Integrate
    for (i, body) in bodies.iter_mut().enumerate() {
        body.vx += forces[i].0 / body.mass * dt;
        body.vy += forces[i].1 / body.mass * dt;
        body.x += body.vx * dt;
        body.y += body.vy * dt;
        // Soft boundary bounce
        if body.x < 20.0 {
            body.vx = body.vx.abs() * 0.9;
            body.x = 20.0;
        }
        if body.x > 460.0 {
            body.vx = -body.vx.abs() * 0.9;
            body.x = 460.0;
        }
        if body.y < 20.0 {
            body.vy = body.vy.abs() * 0.9;
            body.y = 20.0;
        }
        if body.y > 460.0 {
            body.vy = -body.vy.abs() * 0.9;
            body.y = 460.0;
        }
        // Add to trail
        trails[i].push_back((body.x, body.y));
        if trails[i].len() > 300 {
            trails[i].pop_front();
        }
    }

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([8u8, 6, 14]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Background stars (static)
    {
        let mut s: u32 = 0xBA5E_BA11u32;
        for _ in 0..150u32 {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let sx = s % 480;
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let sy = (s >> 16) % 480;
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let b = 60 + (s >> 26) as u8;
            img.put_pixel(sx, sy, Rgb([b, b, b]));
        }
    }

    // Draw trails
    for (i, trail) in trails.iter().enumerate() {
        let hue = bodies[i].hue;
        let tlen = trail.len();
        for (ti, &(tx, ty)) in trail.iter().enumerate() {
            let age_frac = ti as f32 / tlen as f32;
            let alpha = age_frac.powi(2) * 0.8;
            let lit = 0.3 + age_frac * 0.25;
            let col = hsl_to_rgb(hue, 0.9, lit);
            let bx = tx as i32;
            let by = ty as i32;
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

    // Draw bodies
    for body in bodies.iter() {
        let br = (3.0 + body.mass * 2.5) as i32;
        let bx = body.x as i32;
        let by = body.y as i32;
        for dy in -br..=br {
            for dx in -br..=br {
                if dx * dx + dy * dy <= br * br {
                    let px = bx + dx;
                    let py = by + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        // Sphere shading
                        let nx = dx as f32 / br as f32;
                        let ny = dy as f32 / br as f32;
                        let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                        let diff = (-0.6 * nx - 0.5 * ny + 0.6 * nz).max(0.0);
                        let light = 0.4 + 0.6 * diff;
                        let col = hsl_to_rgb(body.hue, 0.9, (0.55 * light).clamp(0.2, 0.8));
                        img.put_pixel(px as u32, py as u32, col);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}
