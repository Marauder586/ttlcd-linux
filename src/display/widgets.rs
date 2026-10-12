//! Screens showing content pushed by the user: calendar, banner, stopwatch, metrics, weather, notes, QR code, Pomodoro, ticker, progress and world clock.

use super::*;

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

    let now = Local::now();
    let today = now.date_naive();
    let year = today.year();
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
    const LEFT: i32 = 4; // left margin so 7×68=476 fits with 4+476+0=480
    const DOW_Y: i32 = 72;
    const GRID_Y: i32 = 96;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header: Month + Year ─────────────────────────────────────────────────
    let month_name = match month {
        1 => "JANUARY",
        2 => "FEBRUARY",
        3 => "MARCH",
        4 => "APRIL",
        5 => "MAY",
        6 => "JUNE",
        7 => "JULY",
        8 => "AUGUST",
        9 => "SEPTEMBER",
        10 => "OCTOBER",
        11 => "NOVEMBER",
        12 => "DECEMBER",
        _ => "?",
    };
    let header = format!("{month_name} {year}");
    draw_centered(&mut img, &bold, PxScale::from(36.0), 14, BLUE, &header);

    // Time top-right
    let ts = now.format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        18,
        PxScale::from(22.0),
        &mono,
        &ts,
    );

    // ── Day-of-week row ───────────────────────────────────────────────────────
    let dow_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    for (col, &label) in dow_labels.iter().enumerate() {
        let cx = LEFT + col as i32 * CELL_W + CELL_W / 2;
        let lw = text_width(&bold, PxScale::from(20.0), label);
        let col_color = if col >= 5 { PEACH } else { SUBTEXT };
        draw_text_mut(
            &mut img,
            col_color,
            cx - lw / 2,
            DOW_Y,
            PxScale::from(20.0),
            &bold,
            label,
        );
    }

    draw_divider(&mut img, GRID_Y - 2);

    // ── Day grid ──────────────────────────────────────────────────────────────
    for day in 1..=days_in_month {
        let slot = start_offset + day - 1;
        let col = slot % 7;
        let row = slot / 7;
        let cx = LEFT + col * CELL_W;
        let cy = GRID_Y + row * CELL_H;

        let is_today = day == today.day() as i32;
        let is_weekend = col >= 5;

        // Highlight today
        if is_today {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(cx + 4, cy + 2).of_size((CELL_W - 8) as u32, (CELL_H - 6) as u32),
                BLUE,
            );
        }

        let label = format!("{day:2}");
        let font_scale = PxScale::from(if is_today { 28.0 } else { 24.0 });
        let lw = text_width(&bold, font_scale, label.trim());
        let text_x = cx + (CELL_W - lw) / 2;
        let text_y = cy + (CELL_H - 24) / 2;

        let color = if is_today {
            Rgb([30u8, 30, 46]) // dark bg text on blue highlight
        } else if is_weekend {
            PEACH
        } else {
            TEXT
        };

        draw_text_mut(
            &mut img,
            color,
            text_x,
            text_y,
            font_scale,
            &bold,
            label.trim(),
        );
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
    lines: &[&str],
    label: &str,
    color: Rgb<u8>,
    show_time: bool,
) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    if show_time {
        let ts = Local::now().format("%H:%M:%S").to_string();
        let tw = text_width(&mono, PxScale::from(20.0), &ts);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 20 - tw,
            14,
            PxScale::from(20.0),
            &mono,
            &ts,
        );
    }

    let non_empty: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|s| !s.is_empty())
        .take(3)
        .collect();
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
        let lw = text_width(&bold, scale, line);
        let x = ((480 - lw) / 2).max(10);
        let col = if i == 0 { color } else { TEXT };
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
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(28.0),
        228,
        BLUE,
        "USB LCD Panel Daemon",
    );

    // Version
    let ver_str = format!("v{version}");
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(22.0),
        270,
        OVERLAY0,
        &ver_str,
    );

    // Divider
    draw_divider(&mut img, 310);

    // Animated dots placeholder — just static "Starting…" for now
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(20.0),
        326,
        SUBTEXT,
        "Starting…",
    );

    // Date/time
    let ts = Local::now().format("%Y-%m-%d  %H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(20.0), 440, OVERLAY0, &ts);

    DynamicImage::ImageRgb8(img)
}

// ─── Stopwatch renderer ───────────────────────────────────────────────────────

/// Format milliseconds as a stopwatch string: "MM:SS.cc" (or "H:MM:SS.cc" for ≥1 h).
pub fn format_sw_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let cs = (ms % 1_000) / 10;
    if h > 0 {
        format!("{h:01}:{m:02}:{s:02}.{cs:02}")
    } else {
        format!("{m:02}:{s:02}.{cs:02}")
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
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(20.0),
        8,
        OVERLAY0,
        "STOPWATCH",
    );
    // Current wall-clock time top-right
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 12 - tw,
        12,
        PxScale::from(18.0),
        &mono,
        &ts,
    );
    draw_divider(&mut img, 36);

    // ── Main time display ────────────────────────────────────────────────────
    let time_str = format_sw_time(elapsed_ms);
    // Split at last '.' so the fractional part can be styled differently
    let dot_pos = time_str.rfind('.');
    let main_part: &str = if let Some(p) = dot_pos {
        &time_str[..p]
    } else {
        &time_str
    };
    let frac_part: &str = if let Some(p) = dot_pos {
        &time_str[p..]
    } else {
        ""
    };

    // Use a smaller scale for hour+ times so they still fit in 480px
    let main_scale: f32 = if elapsed_ms >= 3_600_000 { 72.0 } else { 90.0 };
    let frac_scale: f32 = main_scale * 0.55;

    let mw = text_width(&mono, PxScale::from(main_scale), main_part);
    let fw = text_width(&mono, PxScale::from(frac_scale), frac_part);
    let total_w = mw + fw;
    let start_x = ((480 - total_w) / 2).max(8);
    let main_y = 58_i32;
    // Align fraction baseline with main
    let frac_y = main_y + (main_scale - frac_scale) as i32;

    let time_color = if running {
        TEAL
    } else if elapsed_ms == 0 {
        OVERLAY0
    } else {
        SUBTEXT
    };
    draw_text_mut(
        &mut img,
        time_color,
        start_x,
        main_y,
        PxScale::from(main_scale),
        &mono,
        main_part,
    );
    if !frac_part.is_empty() {
        draw_text_mut(
            &mut img,
            OVERLAY0,
            start_x + mw,
            frac_y,
            PxScale::from(frac_scale),
            &mono,
            frac_part,
        );
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
    draw_filled_rect_mut(
        &mut img,
        Rect::at(sx - 14, status_y + 7).of_size(10, 10),
        status_color,
    );
    draw_text_mut(
        &mut img,
        status_color,
        sx,
        status_y,
        PxScale::from(22.0),
        &bold,
        status_str,
    );

    // ── Lap list ─────────────────────────────────────────────────────────────
    let lap_section_y = status_y + 42;
    draw_divider(&mut img, lap_section_y);

    if laps.is_empty() {
        if !running && elapsed_ms == 0 {
            draw_centered(
                &mut img,
                &bold,
                PxScale::from(18.0),
                lap_section_y + 28,
                OVERLAY0,
                "Use Stopwatch controls",
            );
            draw_centered(
                &mut img,
                &bold,
                PxScale::from(16.0),
                lap_section_y + 56,
                OVERLAY0,
                "to start, lap, and reset.",
            );
        } else {
            draw_centered(
                &mut img,
                &mono,
                PxScale::from(18.0),
                lap_section_y + 28,
                OVERLAY0,
                "no laps recorded",
            );
        }
    } else {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(15.0),
            lap_section_y + 6,
            OVERLAY0,
            "LAPS",
        );

        // Show up to 4 most-recent laps (most recent first)
        let show_laps: Vec<(usize, u64)> = laps
            .iter()
            .enumerate()
            .rev()
            .take(4)
            .map(|(i, &ms)| (i, ms))
            .collect();

        let row_h = 52_i32;
        let base_y = lap_section_y + 26;

        for (idx_in_list, (lap_idx, lap_ms)) in show_laps.into_iter().enumerate() {
            let lap_num = lap_idx + 1; // 1-based
            let row_y = base_y + idx_in_list as i32 * row_h;

            // Subtle background for the most-recent lap
            if idx_in_list == 0 {
                draw_filled_rect_mut(
                    &mut img,
                    Rect::at(8, row_y - 2).of_size(464, (row_h - 4) as u32),
                    SURFACE0,
                );
            }

            // Lap number (left)
            let num_str = format!("#{lap_num}");
            draw_text_mut(
                &mut img,
                OVERLAY0,
                16,
                row_y + 8,
                PxScale::from(20.0),
                &bold,
                &num_str,
            );

            // Cumulative lap time (centre)
            let cum_str = format_sw_time(lap_ms);
            let cw = text_width(&mono, PxScale::from(22.0), &cum_str);
            draw_text_mut(
                &mut img,
                TEXT,
                (480 - cw) / 2,
                row_y + 7,
                PxScale::from(22.0),
                &mono,
                &cum_str,
            );

            // Split delta (right)
            let prev_ms = if lap_idx > 0 { laps[lap_idx - 1] } else { 0 };
            let delta_ms = lap_ms.saturating_sub(prev_ms);
            let delta_str = format!("+{}", format_sw_time(delta_ms));
            let dw = text_width(&mono, PxScale::from(18.0), &delta_str);
            let delta_color = if idx_in_list == 0 { PEACH } else { SUBTEXT };
            draw_text_mut(
                &mut img,
                delta_color,
                480 - dw - 12,
                row_y + 11,
                PxScale::from(18.0),
                &mono,
                &delta_str,
            );
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

pub(super) fn default_metric_color() -> String {
    "blue".into()
}

impl Metric {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            color: default_metric_color(),
        }
    }
}

/// Render up to 8 key/value metrics in a 2-column grid.
///
/// # Arguments
/// * `title`   — optional panel heading (shown at top, truncated to ~28 chars).
/// * `metrics` — key/value pairs; first 8 are shown.
pub fn render_metrics(title: &str, metrics: &[Metric]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    let header = if title.is_empty() {
        "METRICS".to_string()
    } else {
        title.to_uppercase()
    };
    draw_centered(&mut img, &bold, PxScale::from(22.0), 8, BLUE, &header);

    // Current time top-right
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 10 - tw,
        12,
        PxScale::from(18.0),
        &mono,
        &ts,
    );

    draw_divider(&mut img, 38);

    if metrics.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(22.0),
            200,
            OVERLAY0,
            "No metrics set",
        );
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(16.0),
            236,
            OVERLAY0,
            "POST /api/metrics to push data",
        );
        return DynamicImage::ImageRgb8(img);
    }

    // ── 2-column grid ─────────────────────────────────────────────────────────
    // Each cell: 240px wide, 110px tall.  Up to 4 rows = 8 metrics.
    const COLS: i32 = 2;
    const CELL_W: i32 = 240;
    const CELL_H: i32 = 108;
    const START_Y: i32 = 46;

    for (i, m) in metrics.iter().take(8).enumerate() {
        let col = (i as i32) % COLS;
        let row = (i as i32) / COLS;
        let x0 = col * CELL_W;
        let y0 = START_Y + row * CELL_H;

        // Cell border
        draw_hollow_rect_mut(
            &mut img,
            Rect::at(x0 + 4, y0 + 2).of_size((CELL_W - 8) as u32, (CELL_H - 6) as u32),
            SURFACE0,
        );

        // Label (small, dim)
        let label_str: String = m.label.chars().take(20).collect();
        draw_text_mut(
            &mut img,
            OVERLAY0,
            x0 + 10,
            y0 + 8,
            PxScale::from(15.0),
            &bold,
            &label_str,
        );

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
        draw_text_mut(
            &mut img,
            color,
            val_x,
            val_y,
            PxScale::from(val_scale),
            &mono,
            &val_str,
        );
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
pub(super) fn weather_icon(condition: &str) -> &'static [&'static str] {
    let c = condition.to_lowercase();
    if c.contains("thunder") || c.contains("storm") {
        &["  .-.  ", " (   ). ", "(___(__)", "   /_  ", "  /(__) "]
    } else if c.contains("snow") || c.contains("blizzard") || c.contains("ice") {
        &["  .-.  ", " (   ). ", "(___(__)", "  * * * ", "  * * * "]
    } else if c.contains("sleet") || c.contains("drizzle") || c.contains("shower") {
        &["  .-.  ", " (   ). ", "(___(__)", "  ' ' ' ", "  ' ' ' "]
    } else if c.contains("rain") {
        &["  .-.  ", " (   ). ", "(___(__)", "  / / / ", "  / / / "]
    } else if c.contains("fog") || c.contains("mist") || c.contains("haze") {
        &["_______", "_______", "_______", "_______", "_______"]
    } else if c.contains("overcast") || c.contains("cloud") {
        &["       ", " .---. ", "( .--. )", "'------'", "       "]
    } else if c.contains("partly") || c.contains("broken") || c.contains("scatter") {
        &[" \\  /  ", "  \\/   ", " .---.  ", "( sun  )", "'-----' "]
    } else if c.contains("clear") || c.contains("sun") || c.contains("fair") || c.contains("bright")
    {
        &["  \\  / ", "  .--. ", "- |  | -", "  `--' ", "  /  \\ "]
    } else {
        &[" ????  ", " ????  ", " ????  ", " ????  ", " ????  "]
    }
}

/// Pick a colour for a temperature value.
pub(super) fn temp_color(c: f32) -> Rgb<u8> {
    if c >= 38.0 {
        RED
    } else if c >= 28.0 {
        PEACH
    } else if c >= 18.0 {
        YELLOW
    } else if c >= 8.0 {
        GREEN
    } else if c >= -2.0 {
        TEAL
    } else {
        BLUE
    }
}

/// Render a 480×480 weather snapshot panel.
pub fn render_weather(data: &WeatherData) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header bar ────────────────────────────────────────────────────────────
    // "WEATHER" label + city name
    let city_disp = if data.city.is_empty() {
        "WEATHER".to_string()
    } else {
        data.city.to_uppercase()
    };
    draw_centered(&mut img, &bold, PxScale::from(26.0), 8, BLUE, &city_disp);

    // Current time top-right
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 10 - tw,
        12,
        PxScale::from(18.0),
        &mono,
        &ts,
    );

    draw_divider(&mut img, 42);

    // ── Main temperature ──────────────────────────────────────────────────────
    let temp_str = data
        .temp_c
        .map_or_else(|| "—".to_string(), |t| format!("{t:.0}°C"));
    let t_color = data.temp_c.map_or(SUBTEXT, temp_color);
    let t_scale = if temp_str.len() <= 5 { 86.0 } else { 64.0 };
    let tw_main = text_width(&mono, PxScale::from(t_scale), &temp_str);
    // Offset left to make room for the icon
    let temp_x = (240 - tw_main / 2).max(0);
    draw_text_mut(
        &mut img,
        t_color,
        temp_x,
        50,
        PxScale::from(t_scale),
        &mono,
        &temp_str,
    );

    // ── ASCII icon (right side) ───────────────────────────────────────────────
    let icon_lines = weather_icon(&data.condition);
    for (row, line) in icon_lines.iter().enumerate() {
        draw_text_mut(
            &mut img,
            TEAL,
            320,
            56 + (row as i32 * 18),
            PxScale::from(17.0),
            &mono,
            line,
        );
    }

    // ── Condition text ────────────────────────────────────────────────────────
    let cond_str: String = if data.condition.is_empty() {
        "—".to_string()
    } else {
        data.condition.chars().take(28).collect()
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(20.0),
        150,
        SUBTEXT,
        &cond_str,
    );

    draw_divider(&mut img, 180);

    // ── Detail grid: 2 columns ────────────────────────────────────────────────
    // Left col: feels-like, humidity; Right col: wind, visibility
    let fields: &[(&str, String)] = &[
        (
            "Feels like",
            data.feels_like_c
                .map_or_else(|| "—".to_string(), |t| format!("{t:.0}°C")),
        ),
        (
            "Humidity",
            data.humidity_pct
                .map_or_else(|| "—".to_string(), |h| format!("{h}%")),
        ),
        ("Wind", {
            let spd = data
                .wind_kmh
                .map_or_else(|| "—".to_string(), |w| format!("{w:.0}km/h"));
            if data.wind_dir.is_empty() {
                spd
            } else {
                format!("{} {}", spd, data.wind_dir)
            }
        }),
        (
            "Visibility",
            data.visibility_km
                .map_or_else(|| "—".to_string(), |v| format!("{v:.0}km")),
        ),
        (
            "High",
            data.high_c
                .map_or_else(|| "—".to_string(), |t| format!("{t:.0}°C")),
        ),
        (
            "Low",
            data.low_c
                .map_or_else(|| "—".to_string(), |t| format!("{t:.0}°C")),
        ),
    ];

    let start_y = 192;
    let row_h = 42;
    let col_w = 240;

    for (i, (label, value)) in fields.iter().enumerate() {
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        let bx = col * col_w;
        let by = start_y + row * row_h;

        // Cell background
        if row % 2 == 0 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(bx, by).of_size(col_w as u32, row_h as u32),
                Rgb([38, 38, 58]),
            );
        }

        // Label
        draw_text_mut(
            &mut img,
            OVERLAY0,
            bx + 8,
            by + 5,
            PxScale::from(14.0),
            &bold,
            label,
        );
        // Value
        let vw = text_width(&mono, PxScale::from(20.0), value);
        let vx = bx + col_w - vw - 8;
        draw_text_mut(
            &mut img,
            TEXT,
            vx,
            by + 17,
            PxScale::from(20.0),
            &mono,
            value,
        );
    }

    draw_divider(&mut img, start_y + 3 * row_h + 2);

    // ── Updated timestamp ─────────────────────────────────────────────────────
    let upd_str = if let Some(ts) = data.updated_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let age = now.saturating_sub(ts);
        if age < 120 {
            "Updated just now".to_string()
        } else if age < 3600 {
            format!("Updated {}m ago", age / 60)
        } else {
            format!("Updated {}h ago", age / 3600)
        }
    } else {
        "Push weather data via API".to_string()
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(14.0),
        452,
        OVERLAY0,
        &upd_str,
    );

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
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    let header = if title.is_empty() {
        "NOTES".to_string()
    } else {
        title.to_uppercase()
    };
    draw_centered(&mut img, &bold, PxScale::from(24.0), 8, YELLOW, &header);

    // Current time
    let ts = Local::now().format("%H:%M").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 10 - tw,
        12,
        PxScale::from(18.0),
        &mono,
        &ts,
    );

    draw_divider(&mut img, 40);

    if notes.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(20.0),
            200,
            OVERLAY0,
            "No notes",
        );
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(15.0),
            232,
            OVERLAY0,
            "POST /api/notes to add items",
        );
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
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, ROW_H as u32),
                Rgb([38, 38, 56]),
            );
        }

        // Bullet point
        let bullet_x = PADDING_X;
        let col = if i % 4 == 0 {
            MAUVE
        } else if i % 4 == 1 {
            BLUE
        } else if i % 4 == 2 {
            TEAL
        } else {
            PEACH
        };
        draw_text_mut(
            &mut img,
            col,
            bullet_x,
            y + 9,
            PxScale::from(20.0),
            &bold,
            "•",
        );

        // Note text
        let text: String = note.chars().take(38).collect();
        draw_text_mut(
            &mut img,
            TEXT,
            bullet_x + 22,
            y + 10,
            PxScale::from(18.0),
            &bold,
            &text,
        );
    }

    // Count indicator if there are more than MAX_NOTES
    if notes.len() > MAX_NOTES {
        let more = format!("+{} more…", notes.len() - MAX_NOTES);
        draw_centered(&mut img, &bold, PxScale::from(14.0), 458, OVERLAY0, &more);
    }

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
            draw_centered(
                &mut img,
                &mono,
                PxScale::from(16.0),
                230,
                OVERLAY0,
                "Payload too large or invalid",
            );
        }
        Ok(code) => {
            // Draw QR modules onto the image
            let modules = code.width();
            // Target QR area: 360px square, centred horizontally between header and footer
            let footer_h: i32 = 60;
            let qr_area_top = header_h + 12;
            let qr_area_bot = 480 - footer_h - 12;
            let qr_area_h = qr_area_bot - qr_area_top;
            let qr_area_w = 380_i32;
            let cell = ((qr_area_w.min(qr_area_h)) / (modules as i32 + 4)).max(1) as u32;
            let qr_px = cell * (modules as u32 + 4);
            let qr_x = (480 - qr_px as i32) / 2;
            let qr_y = qr_area_top + (qr_area_h - qr_px as i32) / 2;

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
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(15.0),
        428,
        OVERLAY0,
        &display_payload,
    );

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
        "long_break" => (TEAL, "🌿", "Long Break"),
        _ => (RED, "🍅", "Focus"),
    };

    // ── Header bar ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 56), phase_color);
    let sessions_in_set = completed_sessions % 4;
    let header_text = if phase == "work" {
        format!("{} Focus  Round {}", phase_emoji, sessions_in_set + 1)
    } else {
        format!("{phase_emoji} {phase_label}")
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
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(18.0),
        276,
        status_color,
        status_text,
    );

    // ── Cycle dots (4 tomatoes = one set) ────────────────────────────────────
    draw_divider(&mut img, 415);
    let n_dots = 4i32;
    let dot_r = 8i32;
    let dot_spacing = 30i32;
    let total_dot_w = (n_dots - 1) * dot_spacing + dot_r * 2;
    let dot_x0 = (480 - total_dot_w) / 2 + dot_r;
    let dot_y = 440i32;

    for i in 0..n_dots {
        let dot_x = dot_x0 + i * dot_spacing;
        let is_complete = i < sessions_in_set as i32;
        let is_active = phase == "work" && i == sessions_in_set as i32;
        let col = if is_complete {
            phase_color
        } else if is_active {
            SUBTEXT
        } else {
            SURFACE0
        };
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
        ("UTC".into(), 0),
        ("New York".into(), -300), // EST (UTC-5); does not track DST
        ("London".into(), 0),
        ("Tokyo".into(), 540), // JST (UTC+9)
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
        let h12 = if h.is_multiple_of(12) { 12 } else { h % 12 };

        let time_24 = format!("{h:02}:{m:02}:{s:02}");
        let time_12 = format!("{h12:02}:{m:02} {ampm}");
        let date_str = local_dt.format("%b %-d").to_string();

        let y_base = 56 + i as i32 * row_h;

        // Background stripe for even rows
        if i % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y_base).of_size(480, row_h as u32),
                SURFACE0,
            );
        }

        // City name
        let city_disp: String = city.chars().take(18).collect();
        draw_text_mut(
            &mut img,
            TEXT,
            20,
            y_base + 10,
            PxScale::from(20.0),
            &bold,
            &city_disp,
        );

        // UTC offset label
        let offset_h = offset_mins / 60;
        let offset_m = (offset_mins % 60).abs();
        let offset_str = if offset_m == 0 {
            format!("UTC{offset_h:+}")
        } else {
            format!("UTC{offset_h:+}:{offset_m:02}")
        };
        let ow = text_width(&mono, PxScale::from(14.0), &offset_str);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 20 - ow,
            y_base + 10,
            PxScale::from(14.0),
            &mono,
            &offset_str,
        );

        // Day/night color: golden for day (6–18), blue for evening (18–22), dim for night
        let time_color = if (6..18).contains(&h) {
            YELLOW
        } else if h < 22 {
            BLUE
        } else {
            OVERLAY0
        };
        // Day/night dot indicator
        let dot_color = if (6..18).contains(&h) {
            YELLOW
        } else {
            SURFACE0
        };
        let dot_x = (480 - 20 - 8) as u32;
        let dot_y_px = (y_base + 44) as u32;
        for dr in -5i32..=5 {
            for dc in -5i32..=5 {
                if dr * dr + dc * dc <= 25 {
                    let px = (dot_x as i32 + dc).max(0) as u32;
                    let py = (dot_y_px as i32 + dr).max(0) as u32;
                    if px < 480 && py < 480 {
                        img.put_pixel(px, py, dot_color);
                    }
                }
            }
        }

        // Large time
        draw_text_mut(
            &mut img,
            time_color,
            20,
            y_base + 32,
            PxScale::from(36.0),
            &mono,
            &time_24,
        );
        // 12-hour + date right-aligned
        let tw12 = text_width(&mono, PxScale::from(18.0), &time_12);
        draw_text_mut(
            &mut img,
            SUBTEXT,
            480 - 20 - tw12,
            y_base + 36,
            PxScale::from(18.0),
            &mono,
            &time_12,
        );
        let dw = text_width(&mono, PxScale::from(16.0), &date_str);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 20 - dw,
            y_base + 58,
            PxScale::from(16.0),
            &mono,
            &date_str,
        );

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
        "blue" => BLUE,
        "green" => GREEN,
        "yellow" => YELLOW,
        "red" => RED,
        "mauve" => MAUVE,
        "teal" => TEAL,
        "peach" => PEACH,
        _ => TEXT,
    };

    // Header bar
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 48), SURFACE0);
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 12 - tw,
        16,
        PxScale::from(18.0),
        &mono,
        &ts,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        12,
        16,
        PxScale::from(18.0),
        &bold,
        "TICKER",
    );
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

// ─── Progress display ─────────────────────────────────────────────────────────

/// State for the configurable progress display mode.
#[derive(Clone, Debug)]
pub struct ProgressState {
    pub label: String,
    pub sublabel: String,
    pub value: f64,
    pub total: f64,
    pub units: String,
    pub color: NamedColor,
    pub started_at: u64, // unix seconds; 0 = no ETA
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            label: "Progress".into(),
            sublabel: String::new(),
            value: 0.0,
            total: 100.0,
            units: String::new(),
            color: NamedColor::Blue,
            started_at: 0,
        }
    }
}

/// Render a full-screen progress display (480×480).
///
/// Shows a large progress bar with percentage, label, optional sublabel,
/// and an ETA derived from `started_at` if provided.
pub fn render_progress(state: &ProgressState) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let frac = if state.total > 0.0 {
        (state.value / state.total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let pct = frac * 100.0;

    let color = state.color.to_rgb();

    // ── Header ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    let label_short: String = state.label.chars().take(26).collect();
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, color, &label_short);
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(13.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        35,
        PxScale::from(13.0),
        &mono,
        &ts,
    );

    // ── Large percentage ──────────────────────────────────────────────────
    let pct_str = format!("{pct:.1}%");
    draw_centered(&mut img, &bold, PxScale::from(100.0), 60, color, &pct_str);

    // ── Sub-label ─────────────────────────────────────────────────────────
    if !state.sublabel.is_empty() {
        let sub: String = state.sublabel.chars().take(40).collect();
        draw_centered(&mut img, &mono, PxScale::from(16.0), 175, SUBTEXT, &sub);
    }

    // ── Value / total ─────────────────────────────────────────────────────
    if state.total > 0.0 {
        let units = if state.units.is_empty() {
            ""
        } else {
            state.units.as_str()
        };
        let vt_str = format!("{:.1}{} / {:.1}{}", state.value, units, state.total, units);
        draw_centered(&mut img, &mono, PxScale::from(18.0), 198, SUBTEXT, &vt_str);
    }

    // ── Progress bar ──────────────────────────────────────────────────────
    let bar_x = 30i32;
    let bar_y = 230i32;
    let bar_w = 420i32;
    let bar_h = 60i32;

    // Outer border
    draw_hollow_rect_mut(
        &mut img,
        Rect::at(bar_x, bar_y).of_size(bar_w as u32, bar_h as u32),
        SURFACE0,
    );

    // Fill
    let inner_w = ((bar_w - 4) as f64 * frac) as i32;
    if inner_w > 0 {
        draw_filled_rect_mut(
            &mut img,
            Rect::at(bar_x + 2, bar_y + 2).of_size(inner_w.max(0) as u32, (bar_h - 4) as u32),
            color,
        );
    }

    // Percentage text inside bar
    let inside_pct = format!("{pct:.0}%");
    let iw = text_width(&bold, PxScale::from(28.0), &inside_pct);
    let text_color = if frac > 0.5 { BG } else { color };
    draw_text_mut(
        &mut img,
        text_color,
        (bar_x + (bar_w - iw) / 2).max(0),
        bar_y + 16,
        PxScale::from(28.0),
        &bold,
        &inside_pct,
    );

    // ── ETA ───────────────────────────────────────────────────────────────
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let eta_y = bar_y + bar_h + 18;
    if state.started_at > 0 && frac > 0.01 && frac < 1.0 {
        let elapsed = now_unix.saturating_sub(state.started_at);
        let total_est = (elapsed as f64 / frac) as u64;
        let remaining = total_est.saturating_sub(elapsed);
        let eta_str = if remaining > 3600 {
            format!("ETA ~{}h {}m", remaining / 3600, (remaining % 3600) / 60)
        } else if remaining > 60 {
            format!("ETA ~{}m {}s", remaining / 60, remaining % 60)
        } else {
            format!("ETA ~{remaining}s")
        };
        draw_centered(&mut img, &mono, PxScale::from(18.0), eta_y, PEACH, &eta_str);
    } else if frac >= 1.0 {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(20.0),
            eta_y,
            GREEN,
            "✓ Complete!",
        );
    }

    // ── Elapsed time ──────────────────────────────────────────────────────
    if state.started_at > 0 {
        let elapsed = now_unix.saturating_sub(state.started_at);
        let elapsed_str = if elapsed > 3600 {
            format!("Elapsed: {}h {}m", elapsed / 3600, (elapsed % 3600) / 60)
        } else if elapsed > 60 {
            format!("Elapsed: {}m {}s", elapsed / 60, elapsed % 60)
        } else {
            format!("Elapsed: {elapsed}s")
        };
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(15.0),
            eta_y + 26,
            OVERLAY0,
            &elapsed_str,
        );
    }

    // ── Footer ────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        457,
        OVERLAY0,
        "ttlcd progress",
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Flash Message Renderer (used by /api/flash endpoint) ────────────────────

/// Render a full-screen text overlay.  `text` may contain `\n` for newlines.
pub fn render_flash_message(text: &str, hex_color: &str) -> DynamicImage {
    let col = parse_hex_color(hex_color).unwrap_or(TEXT);
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }
    let font = font_bold();

    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len() as i32;
    let line_h = 40i32;
    let base_y = 240 - n * line_h / 2;

    // Draw a subtle card background
    let card_w = 440u32;
    let card_h = (n as u32 * line_h as u32 + 40).max(80);
    let card_x = (480 - card_w as i32) / 2;
    let card_y = base_y - 20;
    let card_rect = imageproc::rect::Rect::at(card_x, card_y).of_size(card_w, card_h);
    imageproc::drawing::draw_filled_rect_mut(&mut img, card_rect, SURFACE0);
    imageproc::drawing::draw_hollow_rect_mut(&mut img, card_rect, col);

    for (i, line) in lines.iter().enumerate() {
        let y = base_y + i as i32 * line_h;
        let sz = if n == 1 { 48.0 } else { 36.0 };
        let tw = text_width(&font, PxScale::from(sz), line);
        let tx = (480 - tw) / 2;
        // Shadow
        draw_text_mut(&mut img, BG, tx + 1, y + 1, PxScale::from(sz), &font, line);
        draw_text_mut(&mut img, col, tx, y, PxScale::from(sz), &font, line);
    }

    // Timestamp
    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
    let tsw = text_width(&font, PxScale::from(14.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - tsw - 8,
        462,
        PxScale::from(14.0),
        &font,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

pub(super) fn parse_hex_color(hex: &str) -> Option<Rgb<u8>> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Rgb([r, g, b]))
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn test_progress_default_size() {
        let state = ProgressState::default();
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_progress_half() {
        let state = ProgressState {
            label: "Building release…".into(),
            sublabel: "linking step".into(),
            value: 50.0,
            total: 100.0,
            units: "%".into(),
            color: NamedColor::Blue,
            started_at: 1_700_000_000,
        };
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_progress_complete() {
        let state = ProgressState {
            value: 100.0,
            total: 100.0,
            color: NamedColor::Green,
            ..Default::default()
        };
        let img = render_progress(&state);
        assert_eq!(img.width(), 480);
    }
}
