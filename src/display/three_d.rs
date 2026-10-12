//! 3-D and space screensavers.

use super::*;

// ─── Starfield Screensaver ────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Star {
    x: f32, // -1..1 in projected space
    y: f32,
    z: f32, // 0..1, 0=far, 1=close
    speed: f32,
}

pub(super) static STARFIELD: std::sync::OnceLock<std::sync::Mutex<Vec<Star>>> =
    std::sync::OnceLock::new();
pub(super) static STARFIELD_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub(super) const N_STARS: usize = 200;

/// Animated starfield screensaver — stars fly towards the viewer from a central vanishing point.
pub fn render_starfield() -> DynamicImage {
    let tick = {
        let m = STARFIELD_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut t = m.lock().unwrap();
        let v = *t;
        *t = t.wrapping_add(1);
        v
    };

    let lcg = |s: u64| {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

    let mut stars = STARFIELD
        .get_or_init(|| {
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
                v.push(Star {
                    x,
                    y,
                    z,
                    speed: spd,
                });
            }
            std::sync::Mutex::new(v)
        })
        .lock()
        .unwrap();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

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

        if sx < 0.0 || sy < 0.0 || sx >= 480.0 || sy >= 480.0 {
            continue;
        }

        // Brightness based on closeness
        let bright = (star.z * 255.0).min(255.0) as u8;
        // Size based on closeness
        let size = if star.z > 0.7 { 2_i32 } else { 1_i32 };

        // Color: far = dim blue, close = bright white/yellow
        let r = bright;
        let g = bright;
        let b = if star.z < 0.5 {
            bright
        } else {
            (bright as f32 * 0.8) as u8
        };
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
    draw_text_mut(
        &mut img,
        Rgb([0, 0, 0]),
        (480 - tw) / 2 + 1,
        453,
        PxScale::from(22.0),
        &font,
        &ts,
    );
    draw_text_mut(
        &mut img,
        SUBTEXT,
        (480 - tw) / 2,
        452,
        PxScale::from(22.0),
        &font,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── 3D Spinning Cube ────────────────────────────────────────────────────────

pub(super) static CUBE_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Project a 3D point through X/Y rotation matrices, then perspective-divide.
pub(super) fn project_cube_vertex(x: f64, y: f64, z: f64, rx: f64, ry: f64) -> (f64, f64) {
    // Rotate around Y axis
    let x1 = x * ry.cos() + z * ry.sin();
    let y1 = y;
    let z1 = -x * ry.sin() + z * ry.cos();
    // Rotate around X axis
    let x2 = x1;
    let y2 = y1 * rx.cos() - z1 * rx.sin();
    let z2 = y1 * rx.sin() + z1 * rx.cos();
    // Perspective
    // Camera far enough back that the whole cube (corner radius ≈ 2.42)
    // stays inside the 480×480 frame at every rotation.
    let fov = 440.0_f64;
    let dist = 6.0_f64;
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
    let verts: [(f64, f64, f64); 8] = [
        (-s, -s, -s),
        (s, -s, -s),
        (s, s, -s),
        (-s, s, -s),
        (-s, -s, s),
        (s, -s, s),
        (s, s, s),
        (-s, s, s),
    ];

    // 12 edges (pairs of vertex indices)
    let edges: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // back face
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // front face
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7), // connecting edges
    ];

    // Face sets for depth-based coloring (front-facing face indices)
    let face_colors: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let projected: Vec<(f64, f64)> = verts
        .iter()
        .map(|&(x, y, z)| project_cube_vertex(x, y, z, rx, ry))
        .collect();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Draw subtle grid floor
    for i in 0..=8i32 {
        let t = i as f64 / 8.0;
        let x1 = (40.0 + t * 400.0) as i32;
        let x2 = (40.0 + t * 400.0) as i32;
        let fade = Rgb([25u8, 25, 40]);
        imageproc::drawing::draw_line_segment_mut(
            &mut img,
            (40.0, 380.0),
            (x1 as f32, 440.0),
            fade,
        );
        imageproc::drawing::draw_line_segment_mut(
            &mut img,
            (x2 as f32, 380.0),
            (440.0, 440.0),
            fade,
        );
    }

    // Draw edges, colored by which face they belong to
    let edge_colors: [Rgb<u8>; 12] = [
        BLUE, BLUE, BLUE, BLUE, TEAL, TEAL, TEAL, TEAL, MAUVE, MAUVE, GREEN, GREEN,
    ];

    for (ei, &(a, b)) in edges.iter().enumerate() {
        let (x0, y0) = projected[a];
        let (x1, y1) = projected[b];
        let col = edge_colors[ei];
        // Draw thick line by drawing a few offset copies
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                imageproc::drawing::draw_line_segment_mut(
                    &mut img,
                    (x0 as f32 + dx as f32, y0 as f32 + dy as f32),
                    (x1 as f32 + dx as f32, y1 as f32 + dy as f32),
                    Rgb([col.0[0] / 3, col.0[1] / 3, col.0[2] / 3]),
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
                if (dx * dx + dy * dy) <= 4 {
                    let ix = (px as i32 + dx).clamp(0, 479) as u32;
                    let iy = (py as i32 + dy).clamp(0, 479) as u32;
                    img.put_pixel(ix, iy, col);
                }
            }
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        458,
        PxScale::from(14.0),
        &font,
        "3D Cube",
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Perspective Tunnel ───────────────────────────────────────────────────────

pub(super) static TUNNEL_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// Classic demoscene perspective tunnel.  2×2-pixel cells → 240×240 iters.
/// Catppuccin-tinted rings and sectors scroll towards the viewer.
pub fn render_tunnel() -> DynamicImage {
    let tick = {
        let mut t = TUNNEL_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *t += 1;
        *t
    };
    let tf = tick as f32;

    let stripes: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, PEACH, GREEN, RED];
    let mut img = RgbImage::new(480, 480);

    for cy in 0..240u32 {
        for cx in 0..240u32 {
            let dx = cx as f32 - 120.0;
            let dy = cy as f32 - 120.0;
            let dist = (dx * dx + dy * dy).sqrt().max(0.5);

            // Depth fog: the centre is far away, the rim is close.
            let br = (dist / 150.0).min(1.0).powf(0.8);

            // Texture coords: v runs down the tunnel (∝ 1/dist, scrolling
            // towards the viewer), u runs around it in 12 sectors.
            let v = 640.0 / dist + tf * 0.12;
            let ring = v.floor() as i32;
            let angle = dy.atan2(dx) + tf * 0.004; // slow roll
            let sector = (angle / (std::f32::consts::PI / 6.0)).floor() as i32;

            // Checkerboard of rings × sectors; colour cycles per ring.
            let base = stripes[ring.rem_euclid(6) as usize];
            let shade = if (ring + sector).rem_euclid(2) == 0 {
                1.0
            } else {
                0.35
            };
            let col = Rgb([
                (base[0] as f32 * br * shade) as u8,
                (base[1] as f32 * br * shade) as u8,
                (base[2] as f32 * br * shade) as u8,
            ]);

            let px = cx * 2;
            let py = cy * 2;
            img.put_pixel(px, py, col);
            img.put_pixel(px + 1, py, col);
            img.put_pixel(px, py + 1, col);
            img.put_pixel(px + 1, py + 1, col);
        }
    }
    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_perlin_terrain  — scrolling procedural terrain heightmap
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static TERRAIN_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

/// 2D value noise using a hashed grid.
pub(super) fn vnoise2(ix: i32, iy: i32) -> f32 {
    let h = ix
        .wrapping_mul(1619)
        .wrapping_add(iy.wrapping_mul(31337))
        .wrapping_mul(6791);
    let h2 = h ^ (h >> 14) ^ (h << 5);
    (h2 as u32 as f32) / u32::MAX as f32
}

pub(super) fn smooth_noise(x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    // Smooth step
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let v00 = vnoise2(x0, y0);
    let v10 = vnoise2(x0 + 1, y0);
    let v01 = vnoise2(x0, y0 + 1);
    let v11 = vnoise2(x0 + 1, y0 + 1);
    v00 * (1.0 - ux) * (1.0 - uy) + v10 * ux * (1.0 - uy) + v01 * (1.0 - ux) * uy + v11 * ux * uy
}

pub(super) fn fbm(x: f32, y: f32, octaves: u32) -> f32 {
    let mut val = 0.0f32;
    let mut amp = 0.5f32;
    let mut freq = 1.0f32;
    for _ in 0..octaves {
        val += smooth_noise(x * freq, y * freq) * amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    val
}

pub fn render_perlin_terrain() -> DynamicImage {
    let tick = {
        let m = TERRAIN_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let mut img = RgbImage::new(480, 480);
    let scroll = tick as f32 * 0.005;

    for py in 0u32..480 {
        for px in 0u32..480 {
            let nx = px as f32 / 480.0 * 4.0 + scroll;
            let ny = py as f32 / 480.0 * 4.0;
            let h = fbm(nx, ny, 5);

            // Height→colour: deep water, shallow, beach, grass, rock, snow
            let col = if h < 0.35 {
                // Deep water
                let t = h / 0.35;
                Rgb([
                    (0x17_u8 as f32 + t * (0x1e - 0x17) as f32) as u8,
                    (0x3d_u8 as f32 + t * (0x5a - 0x3d) as f32) as u8,
                    (0x72_u8 as f32 + t * (0x8c - 0x72) as f32) as u8,
                ])
            } else if h < 0.42 {
                // Shallow water / coast
                Rgb([0x74, 0xc7, 0xec])
            } else if h < 0.46 {
                // Beach (yellow)
                Rgb([0xf9, 0xe2, 0xaf])
            } else if h < 0.62 {
                // Grass
                let t = (h - 0.46) / 0.16;
                Rgb([
                    (GREEN[0] as f32 * (1.0 - t) + TEAL[0] as f32 * t) as u8,
                    (GREEN[1] as f32 * (1.0 - t) + TEAL[1] as f32 * t) as u8,
                    (GREEN[2] as f32 * (1.0 - t) + TEAL[2] as f32 * t) as u8,
                ])
            } else if h < 0.80 {
                // Rock / stone
                let br = (h - 0.62) / 0.18;
                let base = (100.0 + br * 50.0) as u8;
                Rgb([base, base, base])
            } else {
                // Snow cap
                Rgb([0xcd, 0xd6, 0xf4])
            };
            img.put_pixel(px, py, col);
        }
    }

    // Overlay label
    let font = font_bold();
    draw_text_mut(
        &mut img,
        TEXT,
        8,
        458,
        PxScale::from(14.0),
        &font,
        "Terrain",
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_galaxy  — spiral galaxy with stars and core glow
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static GALAXY_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_galaxy() -> DynamicImage {
    let tick = {
        let m = GALAXY_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.008;
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x05, 0x05, 0x12]);
    }

    // Core glow
    let cx = 240.0_f32;
    let cy = 240.0_f32;
    for py in 0..480u32 {
        for px in 0..480u32 {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let glow = (-r * 0.018).exp();
            if glow > 0.01 {
                let cur = *img.get_pixel(px, py);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (cur[0] as f32 + 0x89 as f32 * glow * 0.6).min(255.0) as u8,
                        (cur[1] as f32 + 0xb4 as f32 * glow * 0.5).min(255.0) as u8,
                        (cur[2] as f32 + 0xff as f32 * glow * 0.4).min(255.0) as u8,
                    ]),
                );
            }
        }
    }

    // Spiral arms with stars
    let n_arms = 2i32;
    let stars_per_arm = 600;
    let mut star_seed = 0x6A1A_A1A5u32;
    let star_lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    for arm in 0..n_arms {
        for _ in 0..stars_per_arm {
            let t_star = star_lcg(&mut star_seed); // 0..1 along arm
            let r = t_star * 220.0_f32;
            // Spiral: angle = arm offset + r * tightness + rotation
            let angle =
                arm as f32 * std::f32::consts::PI + r * 0.04 + tf + star_lcg(&mut star_seed) * 0.3; // scatter
            let scatter = star_lcg(&mut star_seed) * 20.0 * t_star;

            let (sa, ca) = angle.sin_cos();
            let sx = cx + (ca * r + sa * scatter);
            let sy = cy + (sa * r - ca * scatter);

            if !(0.0..480.0).contains(&sx) || !(0.0..480.0).contains(&sy) {
                continue;
            }

            // Star colour: blue near centre, yellow/white at edges
            let heat = 1.0 - t_star;
            let col = Rgb([
                (0x89 as f32 + heat * (0xff - 0x89) as f32) as u8,
                (0xb4 as f32 + heat * (0xff - 0xb4) as f32) as u8,
                (0xfa as f32) as u8,
            ]);
            let brightness = star_lcg(&mut star_seed) * 0.5 + 0.5;
            let alpha = (brightness * 200.0) as u8;
            let pi = sx as u32;
            let qi = sy as u32;
            let bg = *img.get_pixel(pi, qi);
            let fa = alpha as f32 / 255.0;
            img.put_pixel(
                pi,
                qi,
                Rgb([
                    (bg[0] as f32 * (1.0 - fa) + col[0] as f32 * fa).min(255.0) as u8,
                    (bg[1] as f32 * (1.0 - fa) + col[1] as f32 * fa).min(255.0) as u8,
                    (bg[2] as f32 * (1.0 - fa) + col[2] as f32 * fa).min(255.0) as u8,
                ]),
            );
        }
    }

    // Bright core stars
    for i in 0..80i32 {
        let r = (i as f32 / 80.0).powi(2) * 40.0;
        let angle = i as f32 * 2.399963; // golden angle
        let (sa, ca) = (angle + tf * 0.3).sin_cos();
        let px_i = (cx + ca * r) as u32;
        let py_i = (cy + sa * r) as u32;
        if px_i < 480 && py_i < 480 {
            img.put_pixel(px_i, py_i, TEXT);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_warp_speed  — hyperspace warp streaks
// ─────────────────────────────────────────────────────────────────────────────
pub(super) struct WarpStar {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    len: f32,
    col_idx: usize,
}
pub(super) static WARP_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<WarpStar>, u32, u64)>> =
    std::sync::OnceLock::new();

pub fn render_warp_speed() -> DynamicImage {
    const N_STARS: usize = 200;

    let state = WARP_STATE.get_or_init(|| {
        let mut s = 0x0A45_5EEDu32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let stars = (0..N_STARS)
            .map(|i| {
                let angle = lcg(&mut s) * std::f32::consts::TAU;
                let speed = 1.0 + lcg(&mut s) * 8.0;
                WarpStar {
                    x: lcg(&mut s) * 480.0,
                    y: lcg(&mut s) * 480.0,
                    vx: angle.cos() * speed,
                    vy: angle.sin() * speed,
                    len: 1.0 + lcg(&mut s) * 3.0,
                    col_idx: i % 8,
                }
            })
            .collect();
        std::sync::Mutex::new((stars, s, 0u64))
    });
    let mut st = state.lock().unwrap();
    let (ref mut stars, ref mut seed, ref mut tick) = *st;
    *tick += 1;

    let mut img = RgbImage::new(480, 480);
    // Motion blur: darken existing frame rather than clearing
    for p in img.pixels_mut() {
        *p = BG;
    }

    let cols: [Rgb<u8>; 8] = [
        TEXT,
        BLUE,
        TEAL,
        SUBTEXT,
        Rgb([0xff, 0xff, 0xff]),
        GREEN,
        MAUVE,
        PEACH,
    ];

    let lcg_spawn = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    for star in stars.iter_mut() {
        // Accelerate away from center
        let dx = star.x - 240.0;
        let dy = star.y - 240.0;
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        star.vx += dx / dist * 0.4;
        star.vy += dy / dist * 0.4;

        // Speed limit
        let spd = (star.vx * star.vx + star.vy * star.vy).sqrt();
        let max_spd = 30.0_f32;
        if spd > max_spd {
            star.vx = star.vx / spd * max_spd;
            star.vy = star.vy / spd * max_spd;
        }

        let x0 = star.x as i32;
        let y0 = star.y as i32;
        star.x += star.vx;
        star.y += star.vy;
        let x1 = star.x as i32;
        let y1 = star.y as i32;
        let trail_len = (star.len * spd * 0.3) as i32;

        // Draw streak
        let col = cols[star.col_idx % cols.len()];
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for ti in 0..=steps.max(trail_len) {
            let t = ti as f32 / steps.max(trail_len) as f32;
            let px = x0 as f32 + (x1 - x0) as f32 * t;
            let py = y0 as f32 + (y1 - y0) as f32 * t;
            let alpha = (200.0 * (1.0 - t * 0.7)) as u8;
            if (0.0..480.0).contains(&px) && (0.0..480.0).contains(&py) {
                let pxi = px as u32;
                let pyi = py as u32;
                let bg = *img.get_pixel(pxi, pyi);
                let fa = alpha as f32 / 255.0;
                img.put_pixel(
                    pxi,
                    pyi,
                    Rgb([
                        (bg[0] as f32 + col[0] as f32 * fa).min(255.0) as u8,
                        (bg[1] as f32 + col[1] as f32 * fa).min(255.0) as u8,
                        (bg[2] as f32 + col[2] as f32 * fa).min(255.0) as u8,
                    ]),
                );
            }
        }

        // Respawn if off-screen
        if star.x < -50.0 || star.x > 530.0 || star.y < -50.0 || star.y > 530.0 {
            let angle = lcg_spawn(seed) * std::f32::consts::TAU;
            let r_init = lcg_spawn(seed) * 30.0;
            star.x = 240.0 + angle.cos() * r_init;
            star.y = 240.0 + angle.sin() * r_init;
            star.vx = angle.cos() * (0.5 + lcg_spawn(seed));
            star.vy = angle.sin() * (0.5 + lcg_spawn(seed));
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_network_globe  — wireframe globe with network activity arcs
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static GLOBE_TICK: std::sync::OnceLock<std::sync::Mutex<(u64, u32)>> =
    std::sync::OnceLock::new();

pub fn render_network_globe() -> DynamicImage {
    let state = GLOBE_TICK.get_or_init(|| std::sync::Mutex::new((0u64, 0xBABE_F00Du32)));
    let mut st = state.lock().unwrap();
    let (ref mut tick, ref mut _seed) = *st;
    *tick += 1;
    let tf = *tick as f32 * 0.012;

    let lcg = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s as f32) / u32::MAX as f32
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    const CX: f32 = 240.0;
    const CY: f32 = 240.0;
    const R: f32 = 180.0;

    // Grid colour: fades from the background (limb) to a soft blue (centre).
    let grid_col = |z: f32| -> Rgb<u8> {
        let k = (z / R * 0.75 + 0.15).clamp(0.0, 1.0);
        Rgb([
            (BG[0] as f32 + (110.0 - BG[0] as f32) * k) as u8,
            (BG[1] as f32 + (135.0 - BG[1] as f32) * k) as u8,
            (BG[2] as f32 + (200.0 - BG[2] as f32) * k) as u8,
        ])
    };

    // Rotation: slowly spin the globe
    let rot_y = tf;

    // Project a 3D point on the sphere onto 2D
    let project = |lat: f32, lon: f32| -> Option<(u32, u32, f32)> {
        // Rotate around Y axis
        let lon_r = lon + rot_y;
        let x = R * lat.cos() * lon_r.cos();
        let y = R * lat.sin();
        let z = R * lat.cos() * lon_r.sin();
        if z < 0.0 {
            return None;
        } // back-face cull
        Some(((CX + x) as u32, (CY - y) as u32, z))
    };

    // Draw latitude lines
    for lat_deg in (-75i32..=75).step_by(15) {
        let lat = lat_deg as f32 * std::f32::consts::PI / 180.0;
        let steps = 120;
        for i in 0..steps {
            let lon1 = i as f32 / steps as f32 * std::f32::consts::TAU;
            let lon2 = (i + 1) as f32 / steps as f32 * std::f32::consts::TAU;
            if let (Some((x1, y1, _z1)), Some((x2, y2, z2))) =
                (project(lat, lon1), project(lat, lon2))
            {
                if x1 < 480 && y1 < 480 && x2 < 480 && y2 < 480 {
                    imageproc::drawing::draw_line_segment_mut(
                        &mut img,
                        (x1 as f32, y1 as f32),
                        (x2 as f32, y2 as f32),
                        grid_col(z2),
                    );
                }
            }
        }
    }

    // Draw longitude lines
    for lon_deg in (0i32..360).step_by(20) {
        let lon = lon_deg as f32 * std::f32::consts::PI / 180.0;
        let steps = 60;
        for i in 0..steps {
            let lat1 = (i as f32 / steps as f32 - 0.5) * std::f32::consts::PI;
            let lat2 = ((i + 1) as f32 / steps as f32 - 0.5) * std::f32::consts::PI;
            if let (Some((x1, y1, _)), Some((x2, y2, z2))) =
                (project(lat1, lon), project(lat2, lon))
            {
                if x1 < 480 && y1 < 480 && x2 < 480 && y2 < 480 {
                    imageproc::drawing::draw_line_segment_mut(
                        &mut img,
                        (x1 as f32, y1 as f32),
                        (x2 as f32, y2 as f32),
                        grid_col(z2),
                    );
                }
            }
        }
    }

    // Network activity arcs (4-6 active connections)
    let n_arcs = 5usize;
    let arc_cols: [Rgb<u8>; 5] = [GREEN, TEAL, BLUE, MAUVE, YELLOW];
    let mut arc_seed = 0x0A4C_5EEDu32;

    for i in 0..n_arcs {
        // Randomise source and dest on the sphere
        let lat1 = (lcg(&mut arc_seed) - 0.5) * std::f32::consts::PI * 0.8;
        let lon1 = lcg(&mut arc_seed) * std::f32::consts::TAU;
        let lat2 = (lcg(&mut arc_seed) - 0.5) * std::f32::consts::PI * 0.8;
        let lon2 = lcg(&mut arc_seed) * std::f32::consts::TAU;
        let col = arc_cols[i % arc_cols.len()];

        // Animate the arc drawing progress (pulse)
        let arc_phase = (tf * 0.5 + i as f32 * 1.3).rem_euclid(2.0);
        let progress = arc_phase.min(1.0);

        // Endpoint nodes
        for (lat, lon) in [(lat1, lon1), (lat2, lon2)] {
            if let Some((px, py, _)) = project(lat, lon) {
                imageproc::drawing::draw_filled_circle_mut(
                    &mut img,
                    (px as i32, py as i32),
                    3,
                    col,
                );
            }
        }

        let steps = 120;
        for s in 0..(steps as f32 * progress) as i32 {
            let t = s as f32 / steps as f32;
            let lat = lat1 + (lat2 - lat1) * t;
            let lon = lon1 + (lon2 - lon1) * t;
            // Lift the arc above the surface
            let lift = (t * std::f32::consts::PI).sin() * 0.3;
            let lat_lifted = lat + lift;
            if let Some((px, py, z)) = project(lat_lifted, lon) {
                // Bright head, dimmer tail; 2×2 dots so the arc reads as a line.
                let head = (t / progress.max(0.01)).powi(3);
                let fa = (0.35 + 0.65 * head) * (0.4 + 0.6 * z / R);
                for (ox, oy) in [(0u32, 0u32), (1, 0), (0, 1), (1, 1)] {
                    let (qx, qy) = (px + ox, py + oy);
                    if qx < 480 && qy < 480 {
                        let bg = *img.get_pixel(qx, qy);
                        img.put_pixel(
                            qx,
                            qy,
                            Rgb([
                                (bg[0] as f32 * (1.0 - fa) + col[0] as f32 * fa) as u8,
                                (bg[1] as f32 * (1.0 - fa) + col[1] as f32 * fa) as u8,
                                (bg[2] as f32 * (1.0 - fa) + col[2] as f32 * fa) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }

    // Globe outline
    let steps = 360;
    for i in 0..steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let px = (CX + R * a.cos()) as u32;
        let py = (CY + R * a.sin()) as u32;
        if px < 480 && py < 480 {
            img.put_pixel(px, py, OVERLAY0);
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        SUBTEXT,
        150,
        458,
        PxScale::from(14.0),
        &font,
        "Network Globe",
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_asteroid_field  — rotating asteroids in space
// ─────────────────────────────────────────────────────────────────────────────
pub(super) struct Asteroid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,     // radius
    rot: f32,   // rotation angle
    rot_v: f32, // rotation speed
    ci: usize,
}

pub(super) static ASTEROID_STATE: std::sync::OnceLock<std::sync::Mutex<(Vec<Asteroid>, u32, u64)>> =
    std::sync::OnceLock::new();

pub fn render_asteroid_field() -> DynamicImage {
    const N: usize = 22;

    let state = ASTEROID_STATE.get_or_init(|| {
        let mut s = 0xA57E_4010u32;
        let lcg = |r: &mut u32| -> f32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (*r as f32) / u32::MAX as f32
        };
        let asteroids = (0..N)
            .map(|i| Asteroid {
                x: lcg(&mut s) * 480.0,
                y: lcg(&mut s) * 480.0,
                vx: (lcg(&mut s) - 0.5) * 1.2,
                vy: (lcg(&mut s) - 0.5) * 1.2,
                r: 5.0 + lcg(&mut s) * 22.0,
                rot: lcg(&mut s) * std::f32::consts::TAU,
                rot_v: (lcg(&mut s) - 0.5) * 0.04,
                ci: i % 4,
            })
            .collect();
        std::sync::Mutex::new((asteroids, s, 0u64))
    });

    let mut st = state.lock().unwrap();
    let (ref mut asteroids, ref mut _seed, ref mut tick) = *st;
    *tick += 1;
    let tf = *tick as f32;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0x04, 0x04, 0x10]);
    }

    // Stars
    let mut star_s = 0xA5A5_A5A5u32;
    let sclg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };
    for _ in 0..150 {
        let sx = sclg(&mut star_s) % 480;
        let sy = sclg(&mut star_s) % 480;
        let br = (sclg(&mut star_s) % 200 + 55) as u8;
        img.put_pixel(sx, sy, Rgb([br, br, br]));
    }

    // Rock colours: warm grey tones
    let rock_cols: [[u8; 3]; 4] = [
        [0x7f, 0x78, 0x6e],
        [0x6a, 0x65, 0x60],
        [0x95, 0x8e, 0x84],
        [0x55, 0x52, 0x4e],
    ];

    // Update and draw asteroids
    for ast in asteroids.iter_mut() {
        ast.x = (ast.x + ast.vx).rem_euclid(490.0) - 5.0;
        ast.y = (ast.y + ast.vy).rem_euclid(490.0) - 5.0;
        ast.rot += ast.rot_v;

        // Draw a rough polygon (7 vertices, perturbed)
        let n_verts = 7;
        let base_col = rock_cols[ast.ci % rock_cols.len()];
        let cx = ast.x;
        let cy = ast.y;
        let r = ast.r;

        // Scan fill within bounding box
        let x0 = (cx - r - 2.0) as i32;
        let x1 = (cx + r + 2.0) as i32;
        let y0 = (cy - r - 2.0) as i32;
        let y1 = (cy + r + 2.0) as i32;

        let vert_rs: Vec<f32> = (0..n_verts)
            .map(|v| {
                let h = (v as u32)
                    .wrapping_mul(2654435761)
                    .wrapping_add(cx as u32 * 17);
                r * (0.65 + (h as f32 / u32::MAX as f32) * 0.35)
            })
            .collect();

        for py in y0..=y1 {
            for px in x0..=x1 {
                if !(0..480).contains(&px) || !(0..480).contains(&py) {
                    continue;
                }
                let dx = px as f32 - cx;
                let dy = py as f32 - cy;
                let pt_angle = dy.atan2(dx) - ast.rot;
                let pt_r = (dx * dx + dy * dy).sqrt();

                // Interpolate radius at this angle
                let frac =
                    (pt_angle + std::f32::consts::PI) / std::f32::consts::TAU * n_verts as f32;
                let vi = frac as usize % n_verts;
                let vi2 = (vi + 1) % n_verts;
                let t = frac.fract();
                let poly_r = vert_rs[vi] * (1.0 - t) + vert_rs[vi2] * t;

                if pt_r <= poly_r {
                    // Shading: lit from upper-left
                    let light = ((-dx - dy) / (r * 2.0)).clamp(0.0, 1.0) * 0.6 + 0.4;
                    img.put_pixel(
                        px as u32,
                        py as u32,
                        Rgb([
                            (base_col[0] as f32 * light) as u8,
                            (base_col[1] as f32 * light) as u8,
                            (base_col[2] as f32 * light) as u8,
                        ]),
                    );
                }
            }
        }
    }

    let _ = tf;
    DynamicImage::ImageRgb8(img)
}

// ── Solar System ─────────────────────────────────────────────────────────────
pub fn render_solar_system() -> DynamicImage {
    static SOLAR_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = SOLAR_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([4u8, 4, 14]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Background stars (seeded, static)
    let mut s: u32 = 0xA5EE_D123u32;
    for _ in 0..250u32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let sx = (s >> 23) % 480;
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let sy = (s >> 23) % 480;
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let b = 80u8 + (s >> 26) as u8;
        img.put_pixel(sx, sy, Rgb([b, b, b]));
    }

    let cx = 240.0f32;
    let cy = 240.0f32;

    // Sun glow
    for py in 180u32..300 {
        for px in 180u32..300 {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            if r <= 26.0 {
                let t = ((26.0 - r) / 26.0).powi(2);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (220.0 + 35.0 * t) as u8,
                        (160.0 + 60.0 * t) as u8,
                        (40.0 * t) as u8,
                    ]),
                );
            } else if r < 60.0 {
                let t = ((60.0 - r) / 34.0) * 0.18;
                let p = img.get_pixel(px, py);
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (p[0] as f32 + (220.0 - p[0] as f32) * t).min(255.0) as u8,
                        (p[1] as f32 + (160.0 - p[1] as f32) * t).min(255.0) as u8,
                        p[2],
                    ]),
                );
            }
        }
    }

    // Planets: (orbital_r, period, planet_r, R, G, B, initial_phase)
    let planets: &[(f32, f32, f32, u8, u8, u8, f32)] = &[
        (55.0, 18.0, 4.0, 180, 140, 120, 0.0),     // Mercury
        (88.0, 46.0, 7.0, 210, 175, 60, 1.0),      // Venus
        (120.0, 75.0, 7.5, 60, 120, 220, 2.1),     // Earth
        (160.0, 140.0, 6.0, 200, 80, 60, 3.5),     // Mars
        (205.0, 470.0, 14.0, 195, 155, 100, 0.7),  // Jupiter
        (235.0, 1170.0, 11.0, 195, 175, 115, 4.2), // Saturn
    ];

    // Draw orbits first
    for &(orb_r, _, _, _, _, _, _) in planets {
        for a in 0..720u32 {
            let ang = a as f32 * std::f32::consts::PI / 360.0;
            let ox = (cx + ang.cos() * orb_r) as i32;
            let oy = (cy + ang.sin() * orb_r) as i32;
            if (0..480).contains(&ox) && (0..480).contains(&oy) {
                let p = img.get_pixel(ox as u32, oy as u32);
                if p[0] < 30 {
                    img.put_pixel(ox as u32, oy as u32, Rgb([22, 22, 35]));
                }
            }
        }
    }

    // Draw planet bodies
    for &(orb_r, period, psize, cr, cg, cb, phase0) in planets {
        let angle = tf / period * std::f32::consts::TAU + phase0;
        let px = cx + angle.cos() * orb_r;
        let py = cy + angle.sin() * orb_r;

        let ir = psize as i32 + 2;
        for dy in -ir..=ir {
            for dx in -ir..=ir {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist <= psize {
                    let bx = px as i32 + dx;
                    let by = py as i32 + dy;
                    if (0..480).contains(&bx) && (0..480).contains(&by) {
                        // Lambert diffuse shading (light from upper-left)
                        let nx = dx as f32 / psize;
                        let ny = dy as f32 / psize;
                        let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                        let diff = (-0.6 * nx - 0.6 * ny + 0.5 * nz).max(0.0);
                        let light = 0.3 + 0.7 * diff;
                        img.put_pixel(
                            bx as u32,
                            by as u32,
                            Rgb([
                                (cr as f32 * light).min(255.0) as u8,
                                (cg as f32 * light).min(255.0) as u8,
                                (cb as f32 * light).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }

        // Saturn rings (index 5 = Saturn, orbital_r == 235)
        if (orb_r - 235.0).abs() < 1.0 {
            let rstart = psize + 3.0;
            let rend = psize + 10.0;
            for ra in 0..720u32 {
                let rang = ra as f32 * std::f32::consts::PI / 360.0;
                // Elliptical projection
                let rx = rang.cos() * rend;
                let ry = rang.sin() * rend * 0.28;
                let rd = (rx * rx + ry * ry / (0.28 * 0.28)).sqrt();
                if rd >= rstart && rd <= rend {
                    let rbx = (px + rx) as i32;
                    let rby = (py + ry) as i32;
                    if (0..480).contains(&rbx) && (0..480).contains(&rby) {
                        img.put_pixel(rbx as u32, rby as u32, Rgb([175, 155, 95]));
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Rotating Icosahedron ──────────────────────────────────────────────────────
// Wireframe 3D icosahedron rotating on all three axes.
pub fn render_icosahedron() -> DynamicImage {
    static ICO_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = ICO_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([18u8, 14, 24]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Icosahedron vertices (circumradius 1)
    let phi = (1.0f32 + 5.0f32.sqrt()) / 2.0; // golden ratio ≈ 1.618
    let norm = (1.0 + phi * phi).sqrt();
    let a = 1.0 / norm;
    let b = phi / norm;
    let verts_raw: &[(f32, f32, f32)] = &[
        (0.0, a, b),
        (0.0, -a, b),
        (0.0, a, -b),
        (0.0, -a, -b),
        (a, b, 0.0),
        (-a, b, 0.0),
        (a, -b, 0.0),
        (-a, -b, 0.0),
        (b, 0.0, a),
        (-b, 0.0, a),
        (b, 0.0, -a),
        (-b, 0.0, -a),
    ];

    // Icosahedron edges (30 edges total)
    let edges: &[(usize, usize)] = &[
        (0, 1),
        (0, 4),
        (0, 5),
        (0, 8),
        (0, 9),
        (1, 6),
        (1, 7),
        (1, 8),
        (1, 9),
        (2, 3),
        (2, 4),
        (2, 5),
        (2, 10),
        (2, 11),
        (3, 6),
        (3, 7),
        (3, 10),
        (3, 11),
        (4, 8),
        (4, 10),
        (5, 9),
        (5, 11),
        (6, 8),
        (6, 10),
        (7, 9),
        (7, 11),
        (8, 9),
        (10, 11),
        (4, 5),
        (6, 7),
    ];

    // Rotation angles
    let rx = tf * 0.013;
    let ry = tf * 0.019;
    let rz = tf * 0.007;

    // Rotation matrices (Rx * Ry * Rz)
    let (srx, crx) = rx.sin_cos();
    let (sry, cry) = ry.sin_cos();
    let (srz, crz) = rz.sin_cos();

    let rotate = |(x, y, z): (f32, f32, f32)| -> (f32, f32, f32) {
        // Rz
        let (x1, y1, z1) = (x * crz - y * srz, x * srz + y * crz, z);
        // Ry
        let (x2, y2, z2) = (x1 * cry + z1 * sry, y1, -x1 * sry + z1 * cry);
        // Rx
        let (x3, y3, z3) = (x2, y2 * crx - z2 * srx, y2 * srx + z2 * crx);
        (x3, y3, z3)
    };

    let scale = 185.0f32;
    let cx = 240.0f32;
    let cy = 240.0f32;

    // Project vertices
    let projected: Vec<(f32, f32, f32)> = verts_raw.iter().map(|&v| rotate(v)).collect();

    // Draw edges
    for &(i, j) in edges {
        let (x1, y1, z1) = projected[i];
        let (x2, y2, z2) = projected[j];

        // Back-face dimming: average z of endpoints
        let avg_z = (z1 + z2) / 2.0;
        let brightness = 0.35 + avg_z * 0.65;
        let alpha = brightness.clamp(0.1, 1.0);

        let px1 = cx + x1 * scale;
        let py1 = cy - y1 * scale; // y flipped
        let px2 = cx + x2 * scale;
        let py2 = cy - y2 * scale;

        let dx = (px2 - px1).abs() as i32;
        let dy = (py2 - py1).abs() as i32;
        let steps = dx.max(dy).max(1);

        // Edge colour: cyan-to-purple gradient along z
        let hue = (z1 * 0.5 + 0.5) * 0.5 + 0.5; // 0.5..1.0 → blue to magenta

        for si in 0..=steps {
            let t = si as f32 / steps as f32;
            let epx = (px1 + (px2 - px1) * t) as i32;
            let epy = (py1 + (py2 - py1) * t) as i32;
            if (0..480).contains(&epx) && (0..480).contains(&epy) {
                let ecol = hsl_to_rgb(hue % 1.0, 0.9, 0.55 * alpha);
                let p = img.get_pixel(epx as u32, epy as u32);
                img.put_pixel(
                    epx as u32,
                    epy as u32,
                    Rgb([
                        (p[0] as f32 + (ecol[0] as f32 - p[0] as f32) * alpha * 0.85).min(255.0)
                            as u8,
                        (p[1] as f32 + (ecol[1] as f32 - p[1] as f32) * alpha * 0.85).min(255.0)
                            as u8,
                        (p[2] as f32 + (ecol[2] as f32 - p[2] as f32) * alpha * 0.85).min(255.0)
                            as u8,
                    ]),
                );
            }
        }
    }

    // Draw vertex dots
    for (i, &(x, y, z)) in projected.iter().enumerate() {
        let _ = i;
        let vbright = (z + 1.0) / 2.0;
        let vx = (cx + x * scale) as i32;
        let vy = (cy - y * scale) as i32;
        if !(2..478).contains(&vx) || !(2..478).contains(&vy) {
            continue;
        }
        let col = hsl_to_rgb(
            (z * 0.5 + 0.5) * 0.5 + 0.5,
            0.9,
            (0.3 + vbright * 0.4).clamp(0.2, 0.8),
        );
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx * dx + dy * dy <= 4 {
                    img.put_pixel((vx + dx) as u32, (vy + dy) as u32, col);
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ── Torus (Donut) ─────────────────────────────────────────────────────────────
// A rotating 3D torus rendered with depth and normal-based shading.
pub fn render_torus() -> DynamicImage {
    static TORUS_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> = std::sync::OnceLock::new();
    let tick = {
        let mut g = TORUS_TICK
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        *g += 1;
        *g
    };
    let tf = tick as f32;

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([10u8, 8, 16]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Z-buffer for depth sorting
    let mut zbuf = vec![f32::NEG_INFINITY; 480 * 480];

    let cx = 240.0f32;
    let cy = 240.0f32;
    let scale = 95.0f32;

    let big_r = 1.6f32; // major radius
    let small_r = 0.75f32; // tube radius

    // Two rotation angles
    let a = tf * 0.025; // rotation around Y
    let b = tf * 0.035; // rotation around Z

    let (sa, ca) = a.sin_cos();
    let (sb, cb) = b.sin_cos();

    // Sample torus surface
    let n_theta = 120usize;
    let n_phi = 60usize;

    for ti in 0..n_theta {
        let theta = ti as f32 / n_theta as f32 * std::f32::consts::TAU;
        for pi in 0..n_phi {
            let phi = pi as f32 / n_phi as f32 * std::f32::consts::TAU;

            // Torus position before rotation
            let x0 = (big_r + small_r * phi.cos()) * theta.cos();
            let y0 = (big_r + small_r * phi.cos()) * theta.sin();
            let z0 = small_r * phi.sin();

            // Rotate around Y axis by `a`
            let x1 = x0 * ca + z0 * sa;
            let z1 = -x0 * sa + z0 * ca;
            let y1 = y0;

            // Rotate around Z axis by `b`
            let x2 = x1 * cb - y1 * sb;
            let y2 = x1 * sb + y1 * cb;
            let z2 = z1;

            // Perspective projection
            let persp = 1.0 / (z2 + 4.0);
            let px = (cx + x2 * scale * persp) as i32;
            let py = (cy - y2 * scale * persp) as i32;
            if !(0..480).contains(&px) || !(0..480).contains(&py) {
                continue;
            }

            let depth = z2;
            let idx = py as usize * 480 + px as usize;
            if depth > zbuf[idx] {
                zbuf[idx] = depth;

                // Normal: derivative of torus surface (phi component)
                let nx0 = phi.cos() * theta.cos();
                let ny0 = phi.cos() * theta.sin();
                let nz0 = phi.sin();

                // Rotate normal same way
                let nx1 = nx0 * ca + nz0 * sa;
                let nz1 = -nx0 * sa + nz0 * ca;
                let ny1 = ny0;
                let nx2 = nx1 * cb - ny1 * sb;
                let ny2 = nx1 * sb + ny1 * cb;
                let nz2 = nz1;

                // Lighting: key from upper-left-front
                let dot = (-0.6 * nx2 - 0.4 * ny2 + 0.7 * nz2).max(0.0);
                let ambient = 0.2f32;
                let light = ambient + (1.0 - ambient) * dot;

                // Colour: hue from theta angle, brightness from light
                let hue = (theta / std::f32::consts::TAU + tf * 0.005) % 1.0;
                let col = hsl_to_rgb(hue, 0.85, (light * 0.65).clamp(0.1, 0.75));
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}
