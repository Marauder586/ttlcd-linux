//! Clock faces.

use super::*;

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

    let now = Local::now();
    let h = now.hour();
    let m = now.minute();
    let s = now.second();

    let hm_str = format!("{h:02}:{m:02}");
    let ss_str = format!("{s:02}");
    let wday_str = now.format("%A").to_string().to_uppercase();
    let date_str = now.format("%B %-e,  %Y").to_string();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_centered(&mut img, &bold, PxScale::from(26.0), 12, MAUVE, &wday_str);
    draw_centered(&mut img, &bold, PxScale::from(20.0), 44, SUBTEXT, &date_str);
    draw_divider(&mut img, 72);

    // ── Giant HH:MM ───────────────────────────────────────────────────────────
    let hm_scale = PxScale::from(144.0);
    let hm_w = text_width(&mono, hm_scale, &hm_str);
    let hm_x = (480 - hm_w).max(0) / 2;
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
    let ss_w = text_width(&mono, ss_scale, &ss_str);
    let ss_x = (480 - ss_w).max(0) / 2;
    draw_text_mut(&mut img, sec_color, ss_x, 242, ss_scale, &mono, &ss_str);

    // Tiny "s" unit label to the right of the seconds digits
    draw_text_mut(
        &mut img,
        OVERLAY0,
        ss_x + ss_w + 6,
        300,
        PxScale::from(20.0),
        &bold,
        "s",
    );

    // ── Seconds progress bar ──────────────────────────────────────────────────
    let sec_frac = s as f32 / 59.0;
    draw_bar(&mut img, 20, 350, 440, 10, sec_frac, sec_color);

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 372);
    let up_str = format!("up {}", format_uptime(System::uptime()));
    let load = System::load_average();
    let load_str = format!(
        "load  {:.2}  {:.2}  {:.2}",
        load.one, load.five, load.fifteen
    );
    draw_centered(&mut img, &bold, PxScale::from(18.0), 382, SUBTEXT, &up_str);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(16.0),
        408,
        OVERLAY0,
        &load_str,
    );

    // ── Corner accent bars ────────────────────────────────────────────────────
    // Teal bars flanking the header for visual polish
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(6, 72), TEAL);
    draw_filled_rect_mut(&mut img, Rect::at(474, 0).of_size(6, 72), TEAL);

    DynamicImage::ImageRgb8(img)
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

    let now = Local::now();
    let h = now.hour() as f64;
    let m = now.minute() as f64;
    let s = now.second() as f64;

    let cx = 240i32;
    let cy = 220i32;
    let r = 185i32; // outer radius of clock face

    // ── Clock face ────────────────────────────────────────────────────────
    // Fill circle background (slightly lighter than base)
    let face_bg = Rgb([30u8, 30, 46]);
    for py in (cy - r)..(cy + r) {
        for px in (cx - r)..(cx + r) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            if dx * dx + dy * dy <= (r as f64) * (r as f64)
                && py >= 0
                && px >= 0
                && (py as u32) < img.height()
                && (px as u32) < img.width()
            {
                img.put_pixel(px as u32, py as u32, face_bg);
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
        if i % 5 == 0 {
            continue;
        } // skip hour marks
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
    let draw_hand =
        |img: &mut image::RgbImage, angle: f64, length: f64, thickness: i32, color: Rgb<u8>| {
            let (sin_a, cos_a) = (angle.sin(), angle.cos());
            let perp_sin = cos_a; // perpendicular to hand direction
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
                draw_antialiased_line_segment_mut(
                    img,
                    (sx as i32, sy as i32),
                    (lx as i32, ly as i32),
                    color,
                    interpolate,
                );
            }
        };

    // ── Hour hand ─────────────────────────────────────────────────────────
    let h_angle = (h + m / 60.0) * PI / 6.0;
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
        let label_r = r as f64 - 38.0;
        let lx = cx + (label_r * angle_rad.sin()) as i32;
        let ly = cy - (label_r * angle_rad.cos()) as i32;
        let lw = text_width(&bold, PxScale::from(18.0), label);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            lx - lw / 2,
            ly - 9,
            PxScale::from(18.0),
            &bold,
            label,
        );
    }

    // ── Digital time (small, below face) ─────────────────────────────────
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%A, %B %-e").to_string();
    let load = System::load_average();
    let load_str = format!(
        "load {:.2} / {:.2} / {:.2}",
        load.one, load.five, load.fifteen
    );

    draw_centered(&mut img, &mono, PxScale::from(26.0), 420, TEAL, &time_str);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(18.0),
        450,
        SUBTEXT,
        &date_str,
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        470,
        OVERLAY0,
        &load_str,
    );

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
    let h = now.hour();
    let m = now.minute();
    let s = now.second();

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
        [true, true, true, true, true, true, false],     // 0
        [false, true, true, false, false, false, false], // 1
        [true, true, false, true, true, false, true],    // 2
        [true, true, true, true, false, false, true],    // 3
        [false, true, true, false, false, true, true],   // 4
        [true, false, true, true, false, true, true],    // 5
        [true, false, true, true, true, true, true],     // 6
        [true, true, true, false, false, false, false],  // 7
        [true, true, true, true, true, true, true],      // 8
        [true, true, true, true, false, true, true],     // 9
    ];

    let on_color = PEACH;
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
                        x: i32,
                        y: i32,
                        w: i32,
                        h: i32,
                        segs: &[bool; 7],
                        on_c: Rgb<u8>,
                        off_c: Rgb<u8>| {
        let st = seg_thick;
        // a: top horizontal
        {
            let c = if segs[0] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x + st, y).of_size((w - 2 * st) as u32, st as u32),
                c,
            );
        }
        // b: top-right vertical
        {
            let c = if segs[1] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x + w - st, y + st).of_size(st as u32, (h / 2 - st) as u32),
                c,
            );
        }
        // c: bottom-right vertical
        {
            let c = if segs[2] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x + w - st, y + h / 2).of_size(st as u32, (h / 2 - st) as u32),
                c,
            );
        }
        // d: bottom horizontal
        {
            let c = if segs[3] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x + st, y + h - st).of_size((w - 2 * st) as u32, st as u32),
                c,
            );
        }
        // e: bottom-left vertical
        {
            let c = if segs[4] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x, y + h / 2).of_size(st as u32, (h / 2 - st) as u32),
                c,
            );
        }
        // f: top-left vertical
        {
            let c = if segs[5] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x, y + st).of_size(st as u32, (h / 2 - st) as u32),
                c,
            );
        }
        // g: middle horizontal
        {
            let c = if segs[6] { on_c } else { off_c };
            draw_filled_rect_mut(
                img,
                Rect::at(x + st, y + h / 2 - st / 2).of_size((w - 2 * st) as u32, st as u32),
                c,
            );
        }
    };

    let draw_colon = |img: &mut image::RgbImage, x: i32, y: i32, h: i32, flash: bool| {
        let c = if flash { on_color } else { off_color };
        let dot_size = 7i32;
        draw_filled_rect_mut(
            img,
            Rect::at(x, y + h / 4 - dot_size / 2).of_size(dot_size as u32, dot_size as u32),
            c,
        );
        draw_filled_rect_mut(
            img,
            Rect::at(x, y + 3 * h / 4 - dot_size / 2).of_size(dot_size as u32, dot_size as u32),
            c,
        );
    };

    // Draw HH:MM:SS
    let colon_w = 18i32;
    let mut cx = start_x;
    let flash = s.is_multiple_of(2);

    let digits = [h / 10, h % 10, m / 10, m % 10, s / 10, s % 10];
    for (i, &d) in digits.iter().enumerate() {
        // Colon after HH and MM
        if i == 2 || i == 4 {
            draw_colon(&mut img, cx, start_y, dh, flash);
            cx += colon_w;
        }
        if (d as usize) < 10 {
            draw_segment(
                &mut img,
                cx,
                start_y,
                dw,
                dh,
                &segments[d as usize],
                on_color,
                off_color,
            );
        }
        cx += dw + gap;
    }

    // Glow effect: draw a subtle halo under digits using darker peach rects
    // (already handled by the display itself)

    // Date + day below
    let date_str = now.format("%A, %B %-e, %Y").to_string();
    let date_w = text_width(&bold, PxScale::from(22.0), &date_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - date_w) / 2).max(0),
        260,
        PxScale::from(22.0),
        &bold,
        &date_str,
    );

    // Load average and uptime
    let load = System::load_average();
    let uptime = System::uptime();
    let stat_str = format!(
        "load {:.2}/{:.2}/{:.2}   up {}",
        load.one,
        load.five,
        load.fifteen,
        format_uptime(uptime)
    );
    let sw = text_width(&mono, PxScale::from(15.0), &stat_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        ((480 - sw) / 2).max(0),
        294,
        PxScale::from(15.0),
        &mono,
        &stat_str,
    );

    // Hostname
    let hostname = System::host_name().unwrap_or_else(|| "ttlcd".into());
    let hw = text_width(&mono, PxScale::from(18.0), &hostname);
    draw_text_mut(
        &mut img,
        MAUVE,
        ((480 - hw) / 2).max(0),
        326,
        PxScale::from(18.0),
        &mono,
        &hostname,
    );

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
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Large time in center
    let time_scale = PxScale::from(110.0);
    let tw = text_width(&mono, time_scale, &time_str);
    let secs_scale = PxScale::from(42.0);
    let sw = text_width(&mono, secs_scale, &secs_str);
    let total_w = tw + sw;
    let tx = (480 - total_w) / 2;
    let ty = 140;

    // Drop shadow
    draw_text_mut(
        &mut img,
        SURFACE0,
        tx + 3,
        ty + 3,
        time_scale,
        &mono,
        &time_str,
    );
    draw_text_mut(
        &mut img,
        SURFACE0,
        tx + tw + 3,
        ty + 68 + 3,
        secs_scale,
        &mono,
        &secs_str,
    );

    // Main time
    draw_text_mut(&mut img, TEXT, tx, ty, time_scale, &mono, &time_str);
    // Seconds (smaller, aligned to baseline of H:MM)
    draw_text_mut(
        &mut img,
        OVERLAY0,
        tx + tw,
        ty + 68,
        secs_scale,
        &mono,
        &secs_str,
    );

    // Date line
    let date_scale = PxScale::from(28.0);
    let dw = text_width(&font, date_scale, &date_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        (480 - dw) / 2,
        280,
        date_scale,
        &font,
        &date_str,
    );

    // Year line
    let year_scale = PxScale::from(20.0);
    let yw = text_width(&font, year_scale, &year_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - yw) / 2,
        314,
        year_scale,
        &font,
        &year_str,
    );

    // Subtle bottom bar: day-of-year progress
    let day_of_year = now.ordinal() as f32;
    let days_in_year = if now.date_naive().leap_year() {
        366.0
    } else {
        365.0
    };
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
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - plw) / 2,
        450,
        PxScale::from(13.0),
        &font,
        &prog_label,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Ambient Particle Clock ────────────────────────────────────────────────────

pub(super) static PARTICLE_CLOCK_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

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
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Particle layer (behind text): drifting pixels
    let lcg = |s: u64| {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };
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
            0 => Rgb([137u8, 180, 250]), // BLUE
            1 => Rgb([203, 166, 247]),   // MAUVE
            2 => Rgb([148, 226, 213]),   // TEAL
            3 => Rgb([250, 179, 135]),   // PEACH
            4 => Rgb([166, 227, 161]),   // GREEN
            _ => Rgb([249, 226, 175]),   // YELLOW
        };
        // Dim to 30% so text is readable
        let col = Rgb([col.0[0] / 4, col.0[1] / 4, col.0[2] / 4]);
        img.put_pixel(x, y, col);
    }

    // Time text (large, centered)
    let time_scale = PxScale::from(80.0);
    let tw = text_width(&font, time_scale, &time_str);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        168,
        time_scale,
        &font,
        &time_str,
    );

    // Date text
    let date_scale = PxScale::from(28.0);
    let dw = text_width(&font, date_scale, &date_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        (480 - dw) / 2,
        266,
        date_scale,
        &font,
        &date_str,
    );

    // Horizontal dividers
    for x in 80..400_u32 {
        img.put_pixel(x, 158, SURFACE0);
        img.put_pixel(x, 298, SURFACE0);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Binary Clock ─────────────────────────────────────────────────────────────

pub fn render_binary_clock() -> DynamicImage {
    use chrono::Timelike;

    let now = chrono::Local::now();
    let h = now.hour();
    let m = now.minute();
    let s = now.second();

    // Binary clock shows 6 columns: H tens, H units, M tens, M units, S tens, S units
    // Rows: bit 3 (top) to bit 0 (bottom) = 4 rows
    let digits: [u8; 6] = [
        (h / 10) as u8,
        (h % 10) as u8,
        (m / 10) as u8,
        (m % 10) as u8,
        (s / 10) as u8,
        (s % 10) as u8,
    ];

    let col_colors: [Rgb<u8>; 6] = [BLUE, BLUE, GREEN, GREEN, MAUVE, MAUVE];

    let cell = 52i32; // cell size
    let gap = 16i32; // gap between cells
                     // Total width for 6 columns: 6*52 + 5*16 = 312 + 80 = 392, centered: (480-392)/2 = 44
    let start_x = 44i32;
    let start_y = 100i32; // vertical center area

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let font = font_bold();

    // Title
    draw_text_mut(
        &mut img,
        TEXT,
        160,
        30,
        PxScale::from(28.0),
        &font,
        "Binary Clock",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        155,
        62,
        PxScale::from(18.0),
        &font,
        "H  H  M  M  S  S",
    );

    for (col_idx, &digit) in digits.iter().enumerate() {
        let cx = start_x + col_idx as i32 * (cell + gap);
        let color = col_colors[col_idx];

        for row in 0..4u8 {
            let bit = 3 - row; // bit 3 at top, bit 0 at bottom
            let on = (digit >> bit) & 1 == 1;
            let cy = start_y + row as i32 * (cell + gap);

            let rect = imageproc::rect::Rect::at(cx, cy).of_size(cell as u32, cell as u32);
            let fill = if on { color } else { SURFACE0 };
            imageproc::drawing::draw_filled_rect_mut(&mut img, rect, fill);

            // Inner lighter border for ON cells
            if on {
                let bright = Rgb([
                    (color.0[0] as u32 + 60).min(255) as u8,
                    (color.0[1] as u32 + 60).min(255) as u8,
                    (color.0[2] as u32 + 60).min(255) as u8,
                ]);
                imageproc::drawing::draw_hollow_rect_mut(
                    &mut img,
                    imageproc::rect::Rect::at(cx + 1, cy + 1)
                        .of_size(cell as u32 - 2, cell as u32 - 2),
                    bright,
                );
            }

            // Bit power label
            let label = format!("{}", 1 << bit);
            let lw = text_width(&font, PxScale::from(14.0), &label);
            let lx = cx + (cell - lw as i32) / 2;
            let ly = cy + (cell - 14) / 2;
            let label_col = if on { BG } else { OVERLAY0 };
            draw_text_mut(
                &mut img,
                label_col,
                lx,
                ly,
                PxScale::from(14.0),
                &font,
                &label,
            );
        }
    }

    // Decimal time readout at the bottom
    let time_str = format!("{h:02}:{m:02}:{s:02}");
    let tw = text_width(&font, PxScale::from(48.0), &time_str);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw as i32) / 2 + 1,
        431,
        PxScale::from(48.0),
        &font,
        &time_str,
    );
    draw_text_mut(
        &mut img,
        BLUE,
        (480 - tw as i32) / 2,
        430,
        PxScale::from(48.0),
        &font,
        &time_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Neon Clock ───────────────────────────────────────────────────────────────

/// Clock with heavy neon-phosphor glow effect — coloured outer glow layers + sharp inner text.
pub fn render_neon_clock() -> DynamicImage {
    let now = chrono::Local::now();
    let h = now.hour();
    let m = now.minute();
    let s = now.second();

    let time_str = format!("{h:02}:{m:02}:{s:02}");
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
    for p in img.pixels_mut() {
        *p = Rgb([2, 2, 10]);
    } // near-black with slight blue tint

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
                draw_text_mut(
                    &mut img,
                    dim,
                    tx + dx,
                    ty + dy,
                    PxScale::from(sz),
                    &font,
                    &time_str,
                );
            }
        }
    }

    // Medium glow (neon colour at 50%)
    let mid_col = Rgb([neon_col.0[0] / 2, neon_col.0[1] / 2, neon_col.0[2] / 2]);
    for (dx, dy) in [(-2i32, -2i32), (-2, 2), (2, -2), (2, 2)] {
        draw_text_mut(
            &mut img,
            mid_col,
            tx + dx,
            ty + dy,
            PxScale::from(sz),
            &font,
            &time_str,
        );
    }

    // Bright core
    draw_text_mut(
        &mut img,
        neon_col,
        tx,
        ty,
        PxScale::from(sz),
        &font,
        &time_str,
    );

    // ── Date ──────────────────────────────────────────────────────────────────
    let dw = text_width(&font, PxScale::from(24.0), &date_str);
    let dx_pos = (480 - dw) / 2;
    // Date with smaller glow
    let dim_date = Rgb([dark_col.0[0] * 2, dark_col.0[1] * 2, dark_col.0[2] * 2]);
    for offset in [(-1i32, -1), (1, -1), (-1, 1), (1, 1)] {
        draw_text_mut(
            &mut img,
            dim_date,
            dx_pos + offset.0,
            275 + offset.1,
            PxScale::from(24.0),
            &font,
            &date_str,
        );
    }
    draw_text_mut(
        &mut img,
        neon_col,
        dx_pos,
        275,
        PxScale::from(24.0),
        &font,
        &date_str,
    );

    // ── Decorative separator line with glow ───────────────────────────────────
    let y_sep = 305i32;
    for x in 60..420i32 {
        let t = (x - 60) as f32 / 360.0;
        let alpha = ((-((t - 0.5) * 4.0).powi(2)).exp() * 255.0) as u8;
        let px = *img.get_pixel(x as u32, y_sep as u32);
        img.put_pixel(
            x as u32,
            y_sep as u32,
            Rgb([
                px.0[0].saturating_add(neon_col.0[0].saturating_mul(alpha) / 255),
                px.0[1].saturating_add(neon_col.0[1].saturating_mul(alpha) / 255),
                px.0[2].saturating_add(neon_col.0[2].saturating_mul(alpha) / 255),
            ]),
        );
    }

    // ── Seconds arc ───────────────────────────────────────────────────────────
    let cx = 240i32;
    let cy = 420i32;
    let arc_r = 55.0f64;
    let sec_frac = s as f64 / 60.0;
    use std::f64::consts::PI;
    let start_a = -PI / 2.0;
    let _end_a = start_a + sec_frac * 2.0 * PI;

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
            &mut img,
            (x0 as i32, y0 as i32),
            (x1 as i32, y1 as i32),
            neon_col,
            interpolate,
        );
    }

    // Seconds number in arc
    let sstr = format!("{s:02}s");
    let sw = text_width(&font, PxScale::from(18.0), &sstr);
    draw_text_mut(
        &mut img,
        neon_col,
        cx - sw as i32 / 2,
        cy - 9,
        PxScale::from(18.0),
        &font,
        &sstr,
    );

    DynamicImage::ImageRgb8(img)
}

/// HSL → RGB colour conversion (h, s, l all 0..1).
pub(super) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Rgb<u8> {
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
    Rgb([
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    ])
}

// ─── Word Clock ───────────────────────────────────────────────────────────────

/// Convert a time to English words.  Returns a Vec of lines (2-4 words each).
pub(super) fn time_to_words(h: u32, m: u32) -> Vec<String> {
    let mins_to_word = |n: u32| match n {
        5 => "FIVE",
        10 => "TEN",
        15 => "QUARTER",
        20 => "TWENTY",
        25 => "TWENTY FIVE",
        30 => "HALF",
        _ => "",
    };
    let hour_word = |h: u32| match h % 12 {
        0 => "TWELVE",
        1 => "ONE",
        2 => "TWO",
        3 => "THREE",
        4 => "FOUR",
        5 => "FIVE",
        6 => "SIX",
        7 => "SEVEN",
        8 => "EIGHT",
        9 => "NINE",
        10 => "TEN",
        11 => "ELEVEN",
        _ => "TWELVE",
    };

    let rounded = (m / 5) * 5;
    let h_display = if rounded > 30 { (h + 1) % 24 } else { h };

    match rounded {
        0 => vec![
            "IT IS".into(),
            hour_word(h_display).into(),
            "O'CLOCK".into(),
        ],
        30 => vec!["HALF".into(), "PAST".into(), hour_word(h_display).into()],
        15 => vec!["QUARTER".into(), "PAST".into(), hour_word(h_display).into()],
        45 => vec!["QUARTER".into(), "TO".into(), hour_word(h_display).into()],
        n if n <= 30 => {
            let mw = mins_to_word(n);
            if mw.is_empty() {
                vec![hour_word(h_display).into(), "O'CLOCK".into()]
            } else if mw.contains(' ') {
                let parts: Vec<&str> = mw.splitn(2, ' ').collect();
                vec![
                    parts[0].into(),
                    parts[1].into(),
                    "PAST".into(),
                    hour_word(h_display).into(),
                ]
            } else {
                vec![mw.into(), "PAST".into(), hour_word(h_display).into()]
            }
        }
        n => {
            let to_min = 60 - n;
            let mw = mins_to_word(to_min);
            if mw.is_empty() {
                vec![hour_word(h_display).into(), "O'CLOCK".into()]
            } else if mw.contains(' ') {
                let parts: Vec<&str> = mw.splitn(2, ' ').collect();
                vec![
                    parts[0].into(),
                    parts[1].into(),
                    "TO".into(),
                    hour_word(h_display).into(),
                ]
            } else {
                vec![mw.into(), "TO".into(), hour_word(h_display).into()]
            }
        }
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
    for p in img.pixels_mut() {
        *p = BG;
    }
    let font = font_bold();

    // Choose font size based on word count and longest line
    let max_len = words
        .iter()
        .map(std::string::String::len)
        .max()
        .unwrap_or(1);
    let font_sz: f32 = if max_len <= 7 && n <= 3 {
        72.0
    } else if max_len <= 10 {
        56.0
    } else {
        44.0
    };

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
        let tx = (480 - tw) / 2;
        let ty = base_y + i as i32 * line_h;

        // Shadow
        draw_text_mut(
            &mut img,
            SURFACE0,
            tx + 2,
            ty + 2,
            PxScale::from(font_sz),
            &font,
            word,
        );

        // Accent: first word in BLUE, last in MAUVE, middle in text colour
        let col = if i == 0 {
            BLUE
        } else if i as i32 == n - 1 {
            MAUVE
        } else {
            text_col
        };
        draw_text_mut(&mut img, col, tx, ty, PxScale::from(font_sz), &font, word);
    }

    // Thin progress bar at bottom (minute progress)
    let bar_w = (m as f32 / 60.0 * 480.0) as u32;
    let bar = imageproc::rect::Rect::at(0, 474).of_size(bar_w.max(1), 6);
    imageproc::drawing::draw_filled_rect_mut(&mut img, bar, BLUE);

    // Date in corner
    let date_str = now.format("%-d %b %Y").to_string();
    let dw = text_width(&font, PxScale::from(14.0), &date_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - dw - 8,
        8,
        PxScale::from(14.0),
        &font,
        &date_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_clock_faces  — three analog clock faces (local + UTC ± offsets)
// ─────────────────────────────────────────────────────────────────────────────
pub fn render_clock_faces() -> DynamicImage {
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let now = Local::now();
    let font = font_bold();
    let font_sm = font_bold();

    // Three clock centres
    let clocks: [(i32, i32, f32, &str); 3] = [
        (120, 160, 0.0, "Local"),
        (360, 160, 5.0, "UTC+5"),
        (240, 360, -8.0, "UTC-8"),
    ];

    let draw_clock = |img: &mut RgbImage, cx: i32, cy: i32, h_off: f32, label: &str| {
        let hour_f: f32 = now.hour() as f32 + now.minute() as f32 / 60.0 + h_off;
        let min_f: f32 = now.minute() as f32 + now.second() as f32 / 60.0;
        let sec_f: f32 = now.second() as f32;

        let r = 100.0_f32;

        // Clock face
        for py in (cy - r as i32 - 5)..(cy + r as i32 + 5) {
            for px in (cx - r as i32 - 5)..(cx + r as i32 + 5) {
                if !(0..480).contains(&px) || !(0..480).contains(&py) {
                    continue;
                }
                let dx = px as f32 - cx as f32;
                let dy = py as f32 - cy as f32;
                let d = (dx * dx + dy * dy).sqrt();
                if d < r {
                    // Interior
                    let shade = if d < r - 3.0 { SURFACE0 } else { OVERLAY0 };
                    img.put_pixel(px as u32, py as u32, shade);
                }
            }
        }

        // Hour marks
        for i in 0..12u32 {
            let angle = i as f32 * std::f32::consts::TAU / 12.0 - std::f32::consts::PI / 2.0;
            let (s, c) = angle.sin_cos();
            let tick_len = if i % 3 == 0 { 14.0_f32 } else { 7.0_f32 };
            let x0 = cx as f32 + c * (r - tick_len);
            let y0 = cy as f32 + s * (r - tick_len);
            let x1 = cx as f32 + c * (r - 4.0);
            let y1 = cy as f32 + s * (r - 4.0);
            // Draw tick
            let steps = ((x1 - x0).abs() + (y1 - y0).abs()) as i32;
            for t in 0..=steps {
                let t_f = t as f32 / steps.max(1) as f32;
                let px = (x0 + (x1 - x0) * t_f) as i32;
                let py = (y0 + (y1 - y0) * t_f) as i32;
                if (0..480).contains(&px) && (0..480).contains(&py) {
                    img.put_pixel(px as u32, py as u32, OVERLAY0);
                }
            }
        }

        // Draw hand helper
        let draw_hand = |img: &mut RgbImage, angle: f32, length: f32, col: Rgb<u8>, thick: i32| {
            let (s, c) = angle.sin_cos();
            let _x1 = cx as f32 + c * length;
            let _y1 = cy as f32 + s * length;
            let steps = (length * 2.0) as i32;
            for ti in 0..=steps {
                let t = ti as f32 / steps as f32;
                for off in -thick..=thick {
                    let px = (cx as f32 + c * length * t - s * off as f32) as i32;
                    let py = (cy as f32 + s * length * t + c * off as f32) as i32;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        img.put_pixel(px as u32, py as u32, col);
                    }
                }
            }
        };

        // Hour hand
        let ha =
            (hour_f.rem_euclid(12.0) / 12.0) * std::f32::consts::TAU - std::f32::consts::PI / 2.0;
        draw_hand(img, ha, r * 0.55, TEXT, 3);
        // Minute hand
        let ma = (min_f / 60.0) * std::f32::consts::TAU - std::f32::consts::PI / 2.0;
        draw_hand(img, ma, r * 0.80, SUBTEXT, 2);
        // Second hand
        let sa = (sec_f / 60.0) * std::f32::consts::TAU - std::f32::consts::PI / 2.0;
        draw_hand(img, sa, r * 0.85, RED, 1);

        // Centre dot
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                if dx * dx + dy * dy <= 16 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if (0..480).contains(&px) && (0..480).contains(&py) {
                        img.put_pixel(px as u32, py as u32, RED);
                    }
                }
            }
        }

        // Label
        let lx = (cx - 22) as u32;
        let ly = (cy + r as i32 + 8) as u32;
        if lx < 460 && ly < 470 {
            draw_text_mut(
                img,
                SUBTEXT,
                lx as i32,
                ly as i32,
                PxScale::from(14.0),
                &font_sm,
                label,
            );
        }
    };

    for &(cx, cy, h_off, label) in &clocks {
        draw_clock(&mut img, cx, cy, h_off, label);
    }

    let time_str = now.format("%H:%M:%S").to_string();
    draw_text_mut(
        &mut img,
        TEXT,
        170,
        458,
        PxScale::from(16.0),
        &font,
        &time_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_gradient_clock  — big clock on a flowing gradient background
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static GRAD_CLOCK_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_gradient_clock() -> DynamicImage {
    let tick = {
        let m = GRAD_CLOCK_TICK.get_or_init(|| std::sync::Mutex::new(0u64));
        let mut t = m.lock().unwrap();
        *t += 1;
        *t
    };

    let tf = tick as f32 * 0.006;
    let mut img = RgbImage::new(480, 480);

    // Animated gradient background
    for py in 0u32..480 {
        for px in 0u32..480 {
            let nx = px as f32 / 480.0;
            let ny = py as f32 / 480.0;
            let wave1 = (nx * 2.0 + tf).sin() * 0.3;
            let wave2 = (ny * 2.0 - tf * 0.7).sin() * 0.3;
            let wave3 = ((nx + ny) * 1.5 + tf * 0.5).cos() * 0.2;
            let h = ((wave1 + wave2 + wave3) * 0.5 + 0.5) % 1.0;
            img.put_pixel(px, py, hsl_to_rgb(h, 0.6, 0.12));
        }
    }

    // Clock text
    let now = Local::now();
    let h = now.hour();
    let m = now.minute();
    let s = now.second();
    let font = font_bold();

    let time_str = format!("{h:02}:{m:02}:{s:02}");
    let date_str = now.format("%A, %d %b %Y").to_string();

    // Time — giant text
    draw_text_mut(
        &mut img,
        TEXT,
        40,
        160,
        PxScale::from(100.0),
        &font,
        &time_str,
    );
    // Date below
    draw_text_mut(
        &mut img,
        SUBTEXT,
        80,
        290,
        PxScale::from(28.0),
        &font,
        &date_str,
    );

    // Progress ring: seconds around the border
    let sec_f = s as f32 + now.nanosecond() as f32 / 1e9;
    let progress = sec_f / 60.0;
    let cx = 240.0_f32;
    let cy = 390.0_f32;
    let r = 60.0_f32;

    for i in 0..360u32 {
        let angle = i as f32 / 360.0;
        let a = angle * std::f32::consts::TAU - std::f32::consts::PI / 2.0;
        let (sa, ca) = a.sin_cos();
        let col = if angle <= progress {
            hsl_to_rgb(progress * 0.3 + 0.55, 0.9, 0.65)
        } else {
            SURFACE0
        };
        // Draw a thick arc segment (2 radii)
        for ri in 0..5u32 {
            let r2 = r + ri as f32;
            let px_f = cx + ca * r2;
            let py_f = cy + sa * r2;
            let px_i = px_f as u32;
            let py_i = py_f as u32;
            if px_i < 480 && py_i < 480 {
                img.put_pixel(px_i, py_i, col);
            }
        }
    }

    // Centre second dot
    let sdot_x =
        (cx + (sec_f / 60.0 * std::f32::consts::TAU - std::f32::consts::PI / 2.0).cos() * r) as i32;
    let sdot_y =
        (cy + (sec_f / 60.0 * std::f32::consts::TAU - std::f32::consts::PI / 2.0).sin() * r) as i32;
    for dy in -4i32..=4 {
        for dx in -4i32..=4 {
            if dx * dx + dy * dy <= 16 {
                let px = (sdot_x + dx) as u32;
                let py_d = (sdot_y + dy) as u32;
                if px < 480 && py_d < 480 {
                    img.put_pixel(px, py_d, TEXT);
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_clock_matrix  — Matrix-style falling digits that resolve to the time
// ─────────────────────────────────────────────────────────────────────────────
pub(super) struct MatrixClockCol {
    head_y: f32,
    speed: f32,
    chars: Vec<char>,
    bright_head: bool,
}
pub(super) static CLOCK_MATRIX_STATE: std::sync::OnceLock<
    std::sync::Mutex<(Vec<MatrixClockCol>, u32, u64)>,
> = std::sync::OnceLock::new();

pub fn render_clock_matrix() -> DynamicImage {
    const COLS: usize = 40; // number of columns
    const ROWS: usize = 40; // character rows
    const CW: u32 = 12; // cell width
    const CH: u32 = 12; // cell height

    let now = Local::now();
    let time_str: Vec<char> = now.format("%H:%M:%S").to_string().chars().collect();

    let state = CLOCK_MATRIX_STATE.get_or_init(|| {
        let mut s = 0xC10C_C0DEu32;
        let lcg = |r: &mut u32| -> u32 {
            *r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *r
        };
        let cols = (0..COLS)
            .map(|_| MatrixClockCol {
                head_y: (lcg(&mut s) % ROWS as u32) as f32,
                speed: 0.3 + (lcg(&mut s) as f32 / u32::MAX as f32) * 0.9,
                chars: (0..ROWS)
                    .map(|_| {
                        let c = lcg(&mut s) % 10;
                        (b'0' + c as u8) as char
                    })
                    .collect(),
                bright_head: lcg(&mut s) % 3 != 0,
            })
            .collect();
        std::sync::Mutex::new((cols, s, 0u64))
    });

    let mut st = state.lock().unwrap();
    let (ref mut cols, ref mut seed, ref mut tick) = *st;
    *tick += 1;

    let lcg = |s: &mut u32| -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    };

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let font = font_mono();

    // Update columns
    for col in cols.iter_mut() {
        col.head_y += col.speed;
        if col.head_y >= ROWS as f32 + 5.0 {
            col.head_y = -5.0;
            // Randomise chars
            for c in &mut col.chars {
                let r = lcg(seed) % 10;
                *c = (b'0' + r as u8) as char;
            }
        }
        // Occasionally flicker a char
        if lcg(seed) % 20 == 0 {
            let ri = lcg(seed) as usize % ROWS;
            let r = lcg(seed) % 10;
            col.chars[ri] = (b'0' + r as u8) as char;
        }
    }

    // Time overlay position (centre 8 characters)
    let time_start_col = (COLS / 2).saturating_sub(4);
    let time_row = ROWS / 2;

    // Draw columns
    for (ci, col) in cols.iter().enumerate() {
        let px_base = ci as u32 * CW;
        let head = col.head_y as i32;

        for ri in 0..ROWS {
            let py_base = ri as u32 * CH;
            let dist_from_head = head - ri as i32;

            // Is this cell part of the time display?
            let time_idx = ci as i32 - time_start_col as i32;
            let is_time_cell =
                ri == time_row && time_idx >= 0 && (time_idx as usize) < time_str.len();

            let (display_char, col_rgb) = if is_time_cell {
                (time_str[time_idx as usize], TEXT)
            } else {
                let c = col.chars[ri % ROWS];
                let col_rgb = if dist_from_head == 0 && col.bright_head {
                    TEXT // bright head
                } else if dist_from_head > 0 && dist_from_head < 12 {
                    let alpha = 255 - dist_from_head as u32 * 20;
                    Rgb([0, (GREEN[1] as u32 * alpha / 255) as u8, 0])
                } else if dist_from_head >= 0 {
                    Rgb([0, (GREEN[1] / 3), 0])
                } else {
                    continue;
                };
                (c, col_rgb)
            };

            if px_base < 480 && py_base < 480 {
                draw_text_mut(
                    &mut img,
                    col_rgb,
                    px_base as i32,
                    py_base as i32,
                    PxScale::from(11.0),
                    &font,
                    &display_char.to_string(),
                );
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}
