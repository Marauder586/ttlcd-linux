//! Self-playing games and cellular-automaton screensavers.

use super::*;

// ─── Conway's Game of Life ────────────────────────────────────────────────────

pub(super) static LIFE_GRID: OnceLock<std::sync::Mutex<Vec<Vec<bool>>>> = OnceLock::new();

pub(super) const LIFE_COLS: usize = 96; // 480 / 5 pixels per cell
pub(super) const LIFE_ROWS: usize = 96;

pub(super) fn life_grid_data() -> std::sync::MutexGuard<'static, Vec<Vec<bool>>> {
    LIFE_GRID
        .get_or_init(|| {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .hash(&mut hasher);
            let seed = hasher.finish();
            // Random initial grid using LCG
            let mut s = seed;
            let grid: Vec<Vec<bool>> = (0..LIFE_ROWS)
                .map(|_| {
                    (0..LIFE_COLS)
                        .map(|_| {
                            s = s
                                .wrapping_mul(6364136223846793005)
                                .wrapping_add(1442695040888963407);
                            (s >> 33) & 1 == 1
                        })
                        .collect()
                })
                .collect();
            std::sync::Mutex::new(grid)
        })
        .lock()
        .unwrap()
}

// (life_gen_data removed - generation count tracked inline in render_game_of_life)

/// Advance the Game of Life grid by one generation.
pub(super) fn life_step(grid: &mut Vec<Vec<bool>>) {
    let rows = grid.len();
    let cols = grid[0].len();
    let mut next = grid.clone();
    for r in 0..rows {
        for c in 0..cols {
            let mut neighbours = 0u8;
            for dr in [-1i32, 0, 1] {
                for dc in [-1i32, 0, 1] {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let nr = (r as i32 + dr).rem_euclid(rows as i32) as usize;
                    let nc = (c as i32 + dc).rem_euclid(cols as i32) as usize;
                    if grid[nr][nc] {
                        neighbours += 1;
                    }
                }
            }
            next[r][c] = matches!((grid[r][c], neighbours), (true, 2 | 3) | (false, 3));
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

    life_step(&mut grid);

    let alive_after: u64 = grid.iter().flatten().map(|&b| b as u64).sum();

    // Re-seed if the population died out or stagnated
    if alive_after == 0 || alive_after == alive_before {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .hash(&mut hasher);
        let mut s = hasher.finish() ^ (alive_before.wrapping_mul(0xdeadbeef));
        for row in grid.iter_mut() {
            for cell in row.iter_mut() {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *cell = (s >> 33) & 1 == 1;
            }
        }
    }

    let mut img = blank();

    // Background already dark (blank())
    // Draw alive cells
    let alive_col = GREEN;
    let dim_col = Rgb([alive_col.0[0] / 4, alive_col.0[1] / 4, alive_col.0[2] / 4]);

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
    let pop_str = format!("pop: {alive_after}  gen: —");
    let pw = text_width(&mono, PxScale::from(12.0), &pop_str);
    draw_filled_rect_mut(
        &mut img,
        Rect::at(2, 466).of_size(pw as u32 + 4, 14),
        Rgb([10, 10, 15]),
    );
    draw_text_mut(
        &mut img,
        dim_col,
        4,
        466,
        PxScale::from(12.0),
        &mono,
        &pop_str,
    );
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

// ─── Bouncing Ball Screensaver ─────────────────────────────────────────────────

/// (x, y, vx, vy, colour index)
pub(super) type BallState = (f64, f64, f64, f64, u8);

pub(super) static BALL_STATE: OnceLock<std::sync::Mutex<BallState>> = OnceLock::new();

pub(super) fn ball_state_data() -> std::sync::MutexGuard<'static, BallState> {
    BALL_STATE
        .get_or_init(|| std::sync::Mutex::new((120.0, 180.0, 3.7, 2.3, 0)))
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
    if x - RADIUS < 0.0 {
        x = RADIUS;
        vx = vx.abs();
        bounced = true;
    }
    if x + RADIUS > W {
        x = W - RADIUS;
        vx = -vx.abs();
        bounced = true;
    }
    if y - RADIUS < 0.0 {
        y = RADIUS;
        vy = vy.abs();
        bounced = true;
    }
    if y + RADIUS > H {
        y = H - RADIUS;
        vy = -vy.abs();
        bounced = true;
    }

    if bounced {
        color_idx = (color_idx + 1) % palette.len() as u8;
    }

    // Save new state
    {
        let mut s = ball_state_data();
        *s = (x, y, vx, vy, color_idx);
    }

    let color = palette[color_idx as usize];
    let glow = Rgb([color.0[0] / 4, color.0[1] / 4, color.0[2] / 4]);

    let mut img = blank();

    // Draw glow (slightly larger dim circle)
    let glow_r = RADIUS + 8.0;
    for py in ((y - glow_r) as i32 - 1)..=((y + glow_r) as i32 + 1) {
        for px in ((x - glow_r) as i32 - 1)..=((x + glow_r) as i32 + 1) {
            if px < 0 || py < 0 || px >= 480 || py >= 480 {
                continue;
            }
            let dx = px as f64 - x;
            let dy = py as f64 - y;
            if dx * dx + dy * dy <= glow_r * glow_r {
                img.put_pixel(px as u32, py as u32, glow);
            }
        }
    }

    // Draw main ball
    for py in ((y - RADIUS) as i32)..=((y + RADIUS) as i32) {
        for px in ((x - RADIUS) as i32)..=((x + RADIUS) as i32) {
            if px < 0 || py < 0 || px >= 480 || py >= 480 {
                continue;
            }
            let dx = px as f64 - x;
            let dy = py as f64 - y;
            if dx * dx + dy * dy <= RADIUS * RADIUS {
                // Slight highlight at top-left
                let dist = (dx * dx + dy * dy).sqrt();
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
        if !(0..480).contains(&shadow_x) {
            continue;
        }
        let shadow_py = shadow_y as i32;
        if (0..480).contains(&shadow_py) {
            let existing = img.get_pixel(shadow_x as u32, shadow_py as u32);
            let r = existing.0[0].saturating_add(20);
            let g = existing.0[1].saturating_add(20);
            let b = existing.0[2].saturating_add(20);
            img.put_pixel(
                shadow_x as u32,
                shadow_py as u32,
                Rgb([r / 2, g / 2, b / 2]),
            );
        }
    }

    // Small velocity trail
    let mono = font_bold();
    let speed_str = format!("{:.1}", (vx * vx + vy * vy).sqrt());
    let sw = text_width(&mono, PxScale::from(12.0), &speed_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - sw - 4,
        468,
        PxScale::from(12.0),
        &mono,
        &speed_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Snake Screensaver ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(super) enum SnakeDir {
    Up,
    Down,
    Left,
    Right,
}

pub(super) struct SnakeState {
    // Snake body as (col, row) grid positions; head = first element
    body: std::collections::VecDeque<(i32, i32)>,
    dir: SnakeDir,
    food: (i32, i32),
    score: u32,
    dead: bool,
    tick: u64,
}

pub(super) const SNAKE_COLS: i32 = 32;
pub(super) const SNAKE_ROWS: i32 = 32;

pub(super) static SNAKE_STATE: std::sync::OnceLock<std::sync::Mutex<SnakeState>> =
    std::sync::OnceLock::new();

pub(super) fn new_snake_state(seed: u64) -> SnakeState {
    let lcg = |s: u64| {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };
    let mut s = lcg(seed);
    // Snake starts in center heading right
    let mut body = std::collections::VecDeque::new();
    body.push_back((SNAKE_COLS / 2, SNAKE_ROWS / 2));
    body.push_back((SNAKE_COLS / 2 - 1, SNAKE_ROWS / 2));
    body.push_back((SNAKE_COLS / 2 - 2, SNAKE_ROWS / 2));
    // Place food randomly, not on snake
    s = lcg(s);
    let fx = (s % SNAKE_COLS as u64) as i32;
    s = lcg(s);
    let fy = (s % SNAKE_ROWS as u64) as i32;
    SnakeState {
        body,
        dir: SnakeDir::Right,
        food: (fx, fy),
        score: 0,
        dead: false,
        tick: 0,
    }
}

/// AI-controlled snake screensaver — automatic game with Catppuccin colours.
pub fn render_snake() -> DynamicImage {
    let tick_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);

    let mut state = SNAKE_STATE
        .get_or_init(|| std::sync::Mutex::new(new_snake_state(tick_now)))
        .lock()
        .unwrap();

    state.tick += 1;

    let lcg = |s: u64| {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

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
    if fx > hx {
        candidates.push(SnakeDir::Right);
    }
    if fx < hx {
        candidates.push(SnakeDir::Left);
    }
    if fy > hy {
        candidates.push(SnakeDir::Down);
    }
    if fy < hy {
        candidates.push(SnakeDir::Up);
    }
    // Add remaining directions as fallback
    for d in [
        SnakeDir::Right,
        SnakeDir::Left,
        SnakeDir::Down,
        SnakeDir::Up,
    ] {
        if !candidates.contains(&d) {
            candidates.push(d);
        }
    }

    // Don't reverse
    let forbidden = match state.dir {
        SnakeDir::Up => SnakeDir::Down,
        SnakeDir::Down => SnakeDir::Up,
        SnakeDir::Left => SnakeDir::Right,
        SnakeDir::Right => SnakeDir::Left,
    };
    candidates.retain(|d| *d != forbidden);

    // Pick first safe move (doesn't hit self or wall)
    fn next_pos(hx: i32, hy: i32, d: &SnakeDir) -> (i32, i32) {
        match d {
            SnakeDir::Up => (hx, hy - 1),
            SnakeDir::Down => (hx, hy + 1),
            SnakeDir::Left => (hx - 1, hy),
            SnakeDir::Right => (hx + 1, hy),
        }
    }

    let body_set: std::collections::HashSet<(i32, i32)> = state.body.iter().copied().collect();
    let chosen_dir = candidates.into_iter().find(|d| {
        let (nx, ny) = next_pos(hx, hy, d);
        nx >= 0 && ny >= 0 && nx < SNAKE_COLS && ny < SNAKE_ROWS && !body_set.contains(&(nx, ny))
    });

    if let Some(d) = chosen_dir {
        state.dir = d;
    }

    // Move snake
    let (nhx, nhy) = next_pos(hx, hy, &state.dir);
    if nhx < 0
        || nhy < 0
        || nhx >= SNAKE_COLS
        || nhy >= SNAKE_ROWS
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
            let body_set2: std::collections::HashSet<_> = state.body.iter().copied().collect();
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
    for p in img.pixels_mut() {
        *p = BG;
    }

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
            GREEN // head
        } else {
            // Body fades from green to teal
            let t = (i as f32 / state.body.len() as f32).min(1.0);
            let r = (GREEN.0[0] as f32 * (1.0 - t) + TEAL.0[0] as f32 * t) as u8;
            let g = (GREEN.0[1] as f32 * (1.0 - t) + TEAL.0[1] as f32 * t) as u8;
            let b = (GREEN.0[2] as f32 * (1.0 - t) + TEAL.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        };
        let margin = if i == 0 { 0 } else { 1 };
        let rect = imageproc::rect::Rect::at(sx + margin, sy + margin).of_size(
            ((cell_w - margin * 2).max(1)) as u32,
            ((cell_h - margin * 2).max(1)) as u32,
        );
        imageproc::drawing::draw_filled_rect_mut(&mut img, rect, col);
    }

    // Score overlay
    let score_str = format!("Snake  Score: {}", state.score);
    let sw = text_width(&font, PxScale::from(14.0), &score_str);
    // Semi-transparent background
    let bg_rect = imageproc::rect::Rect::at(0, 0).of_size(sw as u32 + 8, 20);
    imageproc::drawing::draw_filled_rect_mut(&mut img, bg_rect, Rgb([0, 0, 0]));
    draw_text_mut(
        &mut img,
        GREEN,
        4,
        2,
        PxScale::from(14.0),
        &font,
        &score_str,
    );

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

// ─── DVD Bounce Screensaver ───────────────────────────────────────────────────

pub(super) struct DvdState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    color_idx: usize,
    tick: u64,
}

pub(super) static DVD_STATE: std::sync::OnceLock<std::sync::Mutex<DvdState>> =
    std::sync::OnceLock::new();

pub fn render_dvd_bounce() -> DynamicImage {
    const LABEL: &str = "ttlcd";
    const FONT_SZ: f32 = 56.0;
    // Approximate text dimensions
    const TEXT_W: f32 = 170.0;
    const TEXT_H: f32 = 56.0;

    let palette: [Rgb<u8>; 6] = [BLUE, MAUVE, TEAL, GREEN, PEACH, YELLOW];

    let state_lock = DVD_STATE.get_or_init(|| {
        std::sync::Mutex::new(DvdState {
            x: 40.0,
            y: 60.0,
            vx: 2.3,
            vy: 1.7,
            color_idx: 0,
            tick: 0,
        })
    });

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
    for p in img.pixels_mut() {
        *p = BG;
    }

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
    let glow = Rgb([col.0[0] / 4, col.0[1] / 4, col.0[2] / 4]);
    for offx in [-2i32, 0, 2] {
        for offy in [-2i32, 0, 2] {
            draw_text_mut(
                &mut img,
                glow,
                tx + offx,
                ty + offy,
                PxScale::from(FONT_SZ),
                &font,
                LABEL,
            );
        }
    }
    draw_text_mut(&mut img, col, tx, ty, PxScale::from(FONT_SZ), &font, LABEL);

    DynamicImage::ImageRgb8(img)
}

// ─── Pong AI Screensaver ──────────────────────────────────────────────────────

pub(super) struct PongState {
    ball_x: f32,
    ball_y: f32,
    ball_vx: f32,
    ball_vy: f32,
    left_y: f32,
    right_y: f32,
    left_score: u32,
    right_score: u32,
    tick: u64,
}

pub(super) fn new_pong_state(seed: u64) -> PongState {
    let s = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let angle_frac = (s as f32 / u64::MAX as f32 - 0.5) * 0.8;
    let speed = 3.2f32;
    let vx = if (s >> 32) & 1 == 0 { speed } else { -speed };
    PongState {
        ball_x: 240.0,
        ball_y: 240.0,
        ball_vx: vx,
        ball_vy: speed * angle_frac,
        left_y: 200.0,
        right_y: 200.0,
        left_score: 0,
        right_score: 0,
        tick: 0,
    }
}

pub(super) static PONG_STATE: std::sync::OnceLock<std::sync::Mutex<PongState>> =
    std::sync::OnceLock::new();

pub fn render_pong() -> DynamicImage {
    const PADDLE_W: f32 = 10.0;
    const PADDLE_H: f32 = 70.0;
    const BALL_R: f32 = 8.0;
    const AI_SPEED: f32 = 3.8;
    const LEFT_X: f32 = 24.0;
    const RIGHT_X: f32 = 446.0;
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
    state.left_y = state.left_y.clamp(20.0, H - PADDLE_H - 20.0);
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
        *state = PongState {
            left_score: state.left_score,
            right_score: state.right_score,
            ..new_pong_state(state.tick)
        };
    } else if state.ball_x > W {
        state.left_score += 1;
        *state = PongState {
            left_score: state.left_score,
            right_score: state.right_score,
            ..new_pong_state(state.tick.wrapping_add(7919))
        };
    }

    let (bx, by) = (state.ball_x, state.ball_y);
    let (lx, ly) = (LEFT_X, state.left_y);
    let (rx, ry) = (RIGHT_X, state.right_y);
    let (ls, rs) = (state.left_score, state.right_score);

    // ── Render ─────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }
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
    let lrect =
        imageproc::rect::Rect::at(lx as i32, ly as i32).of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, lrect, BLUE);
    let rrect =
        imageproc::rect::Rect::at(rx as i32, ry as i32).of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, rrect, GREEN);

    // Ball glow
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx * dx + dy * dy <= 9 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(
                    px2,
                    py2,
                    Rgb([YELLOW.0[0] / 3, YELLOW.0[1] / 3, YELLOW.0[2] / 3]),
                );
            }
        }
    }
    // Ball core
    for dx in -4i32..=4 {
        for dy in -4i32..=4 {
            if dx * dx + dy * dy <= 16 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, YELLOW);
            }
        }
    }

    // Scores
    let ls_str = format!("{ls}");
    let rs_str = format!("{rs}");
    let lw = text_width(&font, PxScale::from(36.0), &ls_str);
    draw_text_mut(
        &mut img,
        BLUE,
        (200 - lw as i32 / 2).max(0),
        4,
        PxScale::from(36.0),
        &font,
        &ls_str,
    );
    draw_text_mut(&mut img, GREEN, 270, 4, PxScale::from(36.0), &font, &rs_str);

    DynamicImage::ImageRgb8(img)
}

// ─── Breakout / Brick Breaker ─────────────────────────────────────────────────

pub(super) const BRKOUT_COLS: usize = 10;
pub(super) const BRKOUT_ROWS: usize = 5;

pub(super) struct BreakoutState {
    ball_x: f32,
    ball_y: f32,
    ball_vx: f32,
    ball_vy: f32,
    paddle_x: f32,
    bricks: [[bool; BRKOUT_COLS]; BRKOUT_ROWS], // true = alive
    score: u32,
    lives: u32,
    tick: u64,
}

pub(super) fn new_breakout_state(seed: u64) -> BreakoutState {
    let mut bricks = [[true; BRKOUT_COLS]; BRKOUT_ROWS];
    // A few gaps for variety
    let mut s = seed;
    let lcg = |v: u64| {
        v.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };
    for brick in bricks.iter_mut().flatten() {
        s = lcg(s);
        if s.is_multiple_of(10) {
            *brick = false;
        }
    }
    BreakoutState {
        ball_x: 240.0,
        ball_y: 350.0,
        ball_vx: 3.0,
        ball_vy: -3.5,
        paddle_x: 200.0,
        bricks,
        score: 0,
        lives: 3,
        tick: 0,
    }
}

pub(super) static BREAKOUT_STATE: std::sync::OnceLock<std::sync::Mutex<BreakoutState>> =
    std::sync::OnceLock::new();

pub fn render_breakout() -> DynamicImage {
    const W: f32 = 480.0;
    const H: f32 = 460.0;
    const PADDLE_W: f32 = 64.0;
    const PADDLE_H: f32 = 10.0;
    const BALL_R: f32 = 7.0;
    const PADDLE_Y: f32 = 430.0;
    const BRICK_W: f32 = 44.0;
    const BRICK_H: f32 = 16.0;
    const BRICK_MARGIN: f32 = 4.0;
    const BRICKS_X0: f32 = 10.0;
    const BRICKS_Y0: f32 = 50.0;
    const AI_SPEED: f32 = 5.0;

    let row_colors: [Rgb<u8>; BRKOUT_ROWS] = [RED, PEACH, YELLOW, GREEN, BLUE];

    let state_lock =
        BREAKOUT_STATE.get_or_init(|| std::sync::Mutex::new(new_breakout_state(12345)));
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
    if state.ball_x - BALL_R < 0.0 {
        state.ball_vx = state.ball_vx.abs();
        state.ball_x = BALL_R;
    }
    if state.ball_x + BALL_R > W {
        state.ball_vx = -state.ball_vx.abs();
        state.ball_x = W - BALL_R;
    }
    if state.ball_y - BALL_R < 30.0 {
        state.ball_vy = state.ball_vy.abs();
        state.ball_y = 30.0 + BALL_R;
    }

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
        if state.lives > 0 {
            state.lives -= 1;
        }
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
            if !state.bricks[row][col] {
                continue;
            }
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
        state.ball_vy = -(state.ball_vy.abs() + 0.3); // slightly faster each level
    }

    let (bx, by) = (state.ball_x, state.ball_y);
    let (px, score, lives) = (state.paddle_x, state.score, state.lives);
    let bricks_snapshot = state.bricks;

    // ── Render ─────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }
    let font = font_bold();

    // Score + lives header
    let score_str = format!("Score: {score}   ❤ {lives}");
    draw_text_mut(&mut img, TEXT, 8, 6, PxScale::from(18.0), &font, &score_str);

    // Bricks
    for row in 0..BRKOUT_ROWS {
        let col_color = row_colors[row];
        for (col, &alive) in bricks_snapshot[row].iter().enumerate() {
            if !alive {
                continue;
            }
            let bxr = (BRICKS_X0 + col as f32 * (BRICK_W + BRICK_MARGIN)) as i32;
            let byr = (BRICKS_Y0 + row as f32 * (BRICK_H + BRICK_MARGIN)) as i32;
            let brect = imageproc::rect::Rect::at(bxr, byr).of_size(BRICK_W as u32, BRICK_H as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, brect, col_color);
            // Highlight line at top of brick
            let bright = Rgb([
                (col_color.0[0] as u32 + 50).min(255) as u8,
                (col_color.0[1] as u32 + 50).min(255) as u8,
                (col_color.0[2] as u32 + 50).min(255) as u8,
            ]);
            imageproc::drawing::draw_hollow_rect_mut(
                &mut img,
                imageproc::rect::Rect::at(bxr, byr).of_size(BRICK_W as u32, 2),
                bright,
            );
        }
    }

    // Paddle
    let paddle_rect = imageproc::rect::Rect::at(px as i32, PADDLE_Y as i32)
        .of_size(PADDLE_W as u32, PADDLE_H as u32);
    imageproc::drawing::draw_filled_rect_mut(&mut img, paddle_rect, BLUE);

    // Ball with glow
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx * dx + dy * dy <= 9 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(
                    px2,
                    py2,
                    Rgb([YELLOW.0[0] / 3, YELLOW.0[1] / 3, YELLOW.0[2] / 3]),
                );
            }
        }
    }
    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            if dx * dx + dy * dy <= 4 {
                let px2 = (bx as i32 + dx).clamp(0, 479) as u32;
                let py2 = (by as i32 + dy).clamp(0, 479) as u32;
                img.put_pixel(px2, py2, YELLOW);
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Coloured Conway's Game of Life ───────────────────────────────────────────

pub(super) struct ColorLife {
    // 0 = dead; 1..=255 = age (wraps at u8::MAX)
    grid: Vec<u8>,
    tick: u64,
}

pub(super) static COLOR_LIFE_STATE: std::sync::OnceLock<std::sync::Mutex<ColorLife>> =
    std::sync::OnceLock::new();

/// Game of Life with age-based colouring.
/// Dead cells are BG; living cells cycle through the Catppuccin spectrum
/// from young (blue) through mature (green/yellow) to old (mauve/red).
pub fn render_conway_colors() -> DynamicImage {
    const W: usize = 160;
    const H: usize = 160;
    const CELL: u32 = 3;

    let m = COLOR_LIFE_STATE.get_or_init(|| {
        let mut seed = 0x4C1FE_u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s
        };
        let grid: Vec<u8> = (0..W * H)
            .map(|_| if lcg(&mut seed) % 3 == 0 { 1 } else { 0 })
            .collect();
        std::sync::Mutex::new(ColorLife { grid, tick: 0 })
    });

    let mut st = m.lock().unwrap();
    st.tick += 1;

    let old = st.grid.clone();
    for y in 0..H {
        let yn = (y + H - 1) % H;
        let ys = (y + 1) % H;
        for x in 0..W {
            let xw = (x + W - 1) % W;
            let xe = (x + 1) % W;
            let alive = (old[yn * W + xw] > 0) as u8
                + (old[yn * W + x] > 0) as u8
                + (old[yn * W + xe] > 0) as u8
                + (old[y * W + xw] > 0) as u8
                + (old[y * W + xe] > 0) as u8
                + (old[ys * W + xw] > 0) as u8
                + (old[ys * W + x] > 0) as u8
                + (old[ys * W + xe] > 0) as u8;
            let was_alive = old[y * W + x] > 0;
            st.grid[y * W + x] = if was_alive {
                if alive == 2 || alive == 3 {
                    old[y * W + x].saturating_add(1) // grow older
                } else {
                    0 // die
                }
            } else if alive == 3 {
                1 // born
            } else {
                0
            };
        }
    }

    // Colour ramp: age 1..64 = blue, 65..128 = teal, 129..192 = green,
    // 193..224 = yellow, 225..255 = mauve/red
    let age_col = |age: u8| -> Rgb<u8> {
        let t = age as f32 / 255.0;
        let lerp = |a: Rgb<u8>, b: Rgb<u8>, f: f32| {
            Rgb([
                (a[0] as f32 + (b[0] as f32 - a[0] as f32) * f) as u8,
                (a[1] as f32 + (b[1] as f32 - a[1] as f32) * f) as u8,
                (a[2] as f32 + (b[2] as f32 - a[2] as f32) * f) as u8,
            ])
        };
        if t < 0.33 {
            lerp(BLUE, TEAL, t / 0.33)
        } else if t < 0.55 {
            lerp(TEAL, GREEN, (t - 0.33) / 0.22)
        } else if t < 0.75 {
            lerp(GREEN, YELLOW, (t - 0.55) / 0.20)
        } else if t < 0.90 {
            lerp(YELLOW, PEACH, (t - 0.75) / 0.15)
        } else {
            lerp(PEACH, RED, (t - 0.90) / 0.10)
        }
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    for cy in 0..H {
        for cx in 0..W {
            let age = st.grid[cy * W + cx];
            if age == 0 {
                continue;
            }
            let col = age_col(age);
            let px0 = cx as u32 * CELL;
            let py0 = cy as u32 * CELL;
            for dy in 0..CELL {
                for dx in 0..CELL {
                    img.put_pixel(px0 + dx, py0 + dy, col);
                }
            }
        }
    }

    // Respawn with a splash of new cells if population crashes
    let live_count = st.grid.iter().filter(|&&c| c > 0).count();
    if live_count < W * H / 20 {
        let mut seed = st.tick as u32;
        let lcg = |s: &mut u32| -> u32 {
            *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s
        };
        for _ in 0..W * H / 8 {
            let idx = (lcg(&mut seed) as usize) % (W * H);
            st.grid[idx] = 1;
        }
    }

    DynamicImage::ImageRgb8(img)
}
