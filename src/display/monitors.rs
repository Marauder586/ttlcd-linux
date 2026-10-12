//! Monitoring screens: alerts, HTTP checks, shell output, ping, log tail, RSS, services, listening ports, Docker and logged-in users.

use super::*;

// ─── Recent alerts display ───────────────────────────────────────────────────

/// Show the last N notification summaries from the history ring-buffer.
///
/// Rendered as a compact log on the panel, with timestamps and kind icons.
/// This mode is driven by the caller passing in `entries` to avoid the
/// display module depending on daemon state directly.
pub fn render_recent_alerts(entries: &[(u64, bool, &str)]) -> DynamicImage {
    // entries: Vec of (timestamp_unix, is_alert: bool, summary)
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let now = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(22.0), &time_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        18,
        PxScale::from(22.0),
        &mono,
        &time_str,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "RECENT ALERTS",
    );
    draw_filled_rect_mut(&mut img, Rect::at(0, 48).of_size(480, 1), SURFACE0);

    if entries.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(26.0),
            200,
            OVERLAY0,
            "No notifications yet",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let row_h: i32 = 82;
    let mut y: i32 = 55;

    for (ts, is_alert, summary) in entries.iter().take(5) {
        // Timestamp
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(*ts as i64, 0).map_or_else(
            || "??:??:??".to_string(),
            |d| {
                d.with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
                    .to_string()
            },
        );
        let kind_str = if *is_alert { "🔔" } else { "📝" };
        let header = format!("{kind_str} {dt}");
        draw_text_mut(
            &mut img,
            OVERLAY0,
            20,
            y,
            PxScale::from(17.0),
            &mono,
            &header,
        );

        // Summary (truncate to fit)
        let max_chars = 44;
        let display = if summary.len() > max_chars {
            format!("{}…", &summary[..max_chars - 1])
        } else {
            summary.to_string()
        };
        let color = if *is_alert { PEACH } else { TEXT };
        draw_text_mut(
            &mut img,
            color,
            20,
            y + 22,
            PxScale::from(18.0),
            &bold,
            &display,
        );

        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, y + row_h - 4).of_size(480, 1),
            SURFACE0,
        );
        y += row_h;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── HTTP Status Monitor display ──────────────────────────────────────────────

/// One HTTP monitor result. `status_code` = None means unreachable / timed out.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HttpMonitorResult {
    pub label: String,
    pub url: String,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub ok: bool,
}

/// Render an HTTP health-monitor dashboard.
///
/// Shows up to 6 configured endpoints with status code, latency, and a
/// green (ok) / red (error) indicator badge.
pub fn render_http_status(results: &[HttpMonitorResult]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(
        &mut img,
        TEAL,
        20,
        14,
        PxScale::from(22.0),
        &bold,
        "HTTP MONITOR",
    );
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(16.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 16 - tw,
        18,
        PxScale::from(16.0),
        &mono,
        &ts,
    );
    draw_divider(&mut img, 52);

    let show: Vec<&HttpMonitorResult> = results.iter().take(6).collect();
    let n = show.len();

    if n == 0 {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(22.0),
            200,
            OVERLAY0,
            "No monitors configured.",
        );
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(18.0),
            234,
            SURFACE0,
            "POST /api/http_status to add one",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let row_h = (480 - 60) / n.max(1) as i32;

    for (i, r) in show.iter().enumerate() {
        let y = 60 + i as i32 * row_h;

        // Alternate row background
        if i % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, row_h as u32),
                SURFACE0,
            );
        }

        // Status badge (coloured rectangle on left)
        let badge_col = if r.ok { GREEN } else { RED };
        draw_filled_rect_mut(
            &mut img,
            Rect::at(12, y + 10).of_size(6, (row_h - 20).max(6) as u32),
            badge_col,
        );

        // Label
        let label_disp: String = r.label.chars().take(20).collect();
        draw_text_mut(
            &mut img,
            TEXT,
            28,
            y + 8,
            PxScale::from(20.0),
            &bold,
            &label_disp,
        );

        // Status code / timeout
        let code_str = match r.status_code {
            Some(c) => format!("{c}"),
            None => "TIMEOUT".into(),
        };
        let code_col = if r.ok {
            GREEN
        } else if r.status_code.is_some_and(|c| c < 500) {
            YELLOW
        } else {
            RED
        };
        let cw = text_width(&mono, PxScale::from(18.0), &code_str);
        draw_text_mut(
            &mut img,
            code_col,
            480 - 16 - cw,
            y + 10,
            PxScale::from(18.0),
            &mono,
            &code_str,
        );

        // Latency
        let lat_str = match r.latency_ms {
            Some(ms) if ms < 1000 => format!("{ms}ms"),
            Some(ms) => format!("{:.1}s", ms as f64 / 1000.0),
            None => String::new(),
        };
        if !lat_str.is_empty() {
            let lw = text_width(&mono, PxScale::from(15.0), &lat_str);
            draw_text_mut(
                &mut img,
                OVERLAY0,
                480 - 16 - lw,
                y + 32,
                PxScale::from(15.0),
                &mono,
                &lat_str,
            );
        }

        // URL (truncated, small)
        let url_disp: String = r.url.chars().take(36).collect();
        draw_text_mut(
            &mut img,
            OVERLAY0,
            28,
            y + 32,
            PxScale::from(15.0),
            &mono,
            &url_disp,
        );

        // Divider
        if i + 1 < n {
            draw_divider(&mut img, y + row_h - 1);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Shell command output display ─────────────────────────────────────────────

/// Render a shell-command output viewer.
///
/// - `cmd`      — display-only short description of the command
/// - `output`   — captured stdout (newline-delimited); up to 12 lines rendered
/// - `exit_ok`  — whether the last run exited with code 0
/// - `run_secs` — Unix seconds of the last successful run (0 = never)
pub fn render_shell_cmd(cmd: &str, output: &str, exit_ok: bool, run_secs: u64) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let hdr_col = if exit_ok { TEAL } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(
        &mut img,
        hdr_col,
        20,
        14,
        PxScale::from(20.0),
        &bold,
        "SHELL CMD",
    );
    let status_str = if exit_ok { "OK" } else { "ERR" };
    let sw = text_width(&bold, PxScale::from(18.0), status_str);
    draw_text_mut(
        &mut img,
        hdr_col,
        480 - 16 - sw,
        16,
        PxScale::from(18.0),
        &bold,
        status_str,
    );
    draw_divider(&mut img, 52);

    // Command label
    let cmd_disp: String = cmd.chars().take(44).collect();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        12,
        58,
        PxScale::from(16.0),
        &mono,
        &cmd_disp,
    );

    // Last-run timestamp
    if run_secs > 0 {
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(run_secs as i64, 0)
            .map(|d| d.with_timezone(&Local).format("%H:%M:%S").to_string())
            .unwrap_or_default();
        let rw = text_width(&mono, PxScale::from(14.0), &dt);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 14 - rw,
            60,
            PxScale::from(14.0),
            &mono,
            &dt,
        );
    }

    draw_divider(&mut img, 78);

    // Output lines (monospaced, up to 12 lines)
    let line_h: i32 = 32;
    let max_lines: usize = 12;
    let start_y: i32 = 84;

    let lines: Vec<&str> = if output.is_empty() {
        vec!["(no output)"]
    } else {
        output.lines().take(max_lines).collect()
    };

    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as i32 * line_h;
        if y + line_h > 480 {
            break;
        }
        // Truncate to fit panel width
        let disp: String = line.chars().take(42).collect();
        let text_col = if i == 0 { TEXT } else { SUBTEXT };
        draw_text_mut(&mut img, text_col, 12, y, PxScale::from(18.0), &mono, &disp);
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Ping latency monitor display ────────────────────────────────────────────

/// One ping result for a configured host.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PingResult {
    pub host: String,
    pub label: String,
    pub latency_ms: Option<f64>, // None = unreachable / timeout
    pub loss_pct: f32,           // packet-loss percentage 0–100
}

/// Render a ping-latency monitor dashboard.
///
/// Shows up to 8 configured hosts with round-trip latency and a
/// colour-coded quality indicator.
pub fn render_ping_monitor(results: &[PingResult]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(22.0),
        &bold,
        "PING MONITOR",
    );
    let ts = Local::now().format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(16.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 16 - tw,
        18,
        PxScale::from(16.0),
        &mono,
        &ts,
    );
    draw_divider(&mut img, 52);

    let show: Vec<&PingResult> = results.iter().take(8).collect();
    let n = show.len();

    if n == 0 {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(22.0),
            200,
            OVERLAY0,
            "No hosts configured.",
        );
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(18.0),
            234,
            SURFACE0,
            "POST /api/ping to add one",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let row_h = (480 - 60) / n.max(1) as i32;

    for (i, r) in show.iter().enumerate() {
        let y = 60 + i as i32 * row_h;

        // Alternate row background
        if i % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, row_h as u32),
                SURFACE0,
            );
        }

        // Latency quality color
        let (qual_col, bar_frac) = match r.latency_ms {
            None => (RED, 0.0f32),
            Some(ms) if ms < 20.0 => (GREEN, (ms / 200.0) as f32),
            Some(ms) if ms < 80.0 => (TEAL, (ms / 200.0) as f32),
            Some(ms) if ms < 200.0 => (YELLOW, (ms / 200.0) as f32),
            Some(ms) => (RED, 1.0f32.min(ms as f32 / 500.0)),
        };

        // Left bar accent
        draw_filled_rect_mut(
            &mut img,
            Rect::at(12, y + 8).of_size(4, (row_h - 16).max(4) as u32),
            qual_col,
        );

        // Label
        let label_disp: String = r.label.chars().take(18).collect();
        draw_text_mut(
            &mut img,
            TEXT,
            26,
            y + 8,
            PxScale::from(20.0),
            &bold,
            &label_disp,
        );

        // Host (small, right-aligned)
        let host_disp: String = r.host.chars().take(24).collect();
        let hdisp_w = text_width(&mono, PxScale::from(14.0), &host_disp);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 14 - hdisp_w,
            y + 10,
            PxScale::from(14.0),
            &mono,
            &host_disp,
        );

        // Latency value
        let lat_str = match r.latency_ms {
            Some(ms) => format!("{ms:.1} ms"),
            None => "TIMEOUT".into(),
        };
        draw_text_mut(
            &mut img,
            qual_col,
            26,
            y + 30,
            PxScale::from(22.0),
            &mono,
            &lat_str,
        );

        // Loss indicator
        if r.loss_pct > 0.0 {
            let loss_str = format!("{:.0}% loss", r.loss_pct);
            let lw = text_width(&mono, PxScale::from(14.0), &loss_str);
            draw_text_mut(
                &mut img,
                YELLOW,
                480 - 14 - lw,
                y + 30,
                PxScale::from(14.0),
                &mono,
                &loss_str,
            );
        }

        // Latency mini-bar
        if row_h > 56 {
            draw_bar(&mut img, 26, y + row_h - 14, 200, 6, bar_frac, qual_col);
        }

        // Divider
        if i + 1 < n {
            draw_divider(&mut img, y + row_h - 1);
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Log tail viewer ──────────────────────────────────────────────────────────

/// Render the last N lines of a log file.
///
/// - `path`  — file path shown in the header (just the filename portion)
/// - `lines` — already-read tail lines (most recent last)
/// - `ok`    — true if the file was readable; false means show error state
pub fn render_log_tail(path: &str, lines: &[String], ok: bool) -> DynamicImage {
    let mut img = RgbImage::from_pixel(480, 480, BG);
    let mono = font_mono();
    let bold = font_bold();

    // Header bar
    let hdr_color = if ok { TEAL } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 44), SURFACE0);

    // File icon + short filename
    let fname: String = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .chars()
        .take(30)
        .collect();
    let hdr_text = if ok {
        format!("≡ {fname}")
    } else {
        format!("✕ {fname}")
    };
    draw_text_mut(
        &mut img,
        hdr_color,
        8,
        8,
        PxScale::from(24.0),
        &bold,
        &hdr_text,
    );

    // Line count badge
    let badge = format!("{} lines", lines.len());
    let bw = text_width(&mono, PxScale::from(14.0), &badge);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - bw - 8,
        14,
        PxScale::from(14.0),
        &mono,
        &badge,
    );

    // Divider
    draw_filled_rect_mut(&mut img, Rect::at(0, 44).of_size(480, 2), hdr_color);

    if !ok {
        let msg = if path.is_empty() {
            "No file configured.".to_string()
        } else {
            format!("Cannot read:\n{path}")
        };
        let y = 48 + 20;
        for (i, l) in msg.lines().enumerate() {
            draw_text_mut(
                &mut img,
                SUBTEXT,
                12,
                y + i as i32 * 20,
                PxScale::from(18.0),
                &mono,
                l,
            );
        }
        draw_text_mut(
            &mut img,
            OVERLAY0,
            12,
            y + 60,
            PxScale::from(14.0),
            &mono,
            "Use POST /api/log_tail to set a path.",
        );
        return DynamicImage::ImageRgb8(img);
    }

    // Content area — 14px mono, max ~16 visible lines
    const LINE_H: i32 = 26;
    const MAX_LINES: usize = 16;
    // const CHAR_W: i32 = 8;  // approx chars at 14px mono
    const MAX_CHARS: usize = 58;

    let content_y_start: i32 = 50;

    // Show up to MAX_LINES most-recent lines
    let display_lines: Vec<&str> = if lines.len() > MAX_LINES {
        lines[lines.len() - MAX_LINES..]
            .iter()
            .map(std::string::String::as_str)
            .collect()
    } else {
        lines.iter().map(std::string::String::as_str).collect()
    };

    for (i, line) in display_lines.iter().enumerate() {
        let y = content_y_start + i as i32 * LINE_H;
        if y + LINE_H > 480 {
            break;
        }

        // Strip ANSI escape sequences
        let clean: String = strip_ansi(line);
        // Truncate long lines
        let truncated: String = if clean.chars().count() > MAX_CHARS {
            let t: String = clean.chars().take(MAX_CHARS - 1).collect();
            format!("{t}…")
        } else {
            clean.clone()
        };

        // Alternate row backgrounds for readability
        if i % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, LINE_H as u32),
                SURFACE0,
            );
        }

        // Highlight lines with common log level keywords
        let color = if clean.contains("ERROR")
            || clean.contains("error")
            || clean.contains("CRIT")
            || clean.contains("crit")
        {
            RED
        } else if clean.contains("WARN") || clean.contains("warn") {
            YELLOW
        } else if clean.contains("INFO") || clean.contains("info") {
            TEXT
        } else if clean.contains("DEBUG") || clean.contains("debug") || clean.contains("TRACE") {
            OVERLAY0
        } else {
            SUBTEXT
        };

        draw_text_mut(
            &mut img,
            color,
            4,
            y + 5,
            PxScale::from(14.0),
            &mono,
            &truncated,
        );
    }

    DynamicImage::ImageRgb8(img)
}

// ─── RSS / Atom headline feed ─────────────────────────────────────────────────

/// Render a list of RSS headlines.
/// `feed_title`: channel/feed title
/// `items`: Vec<(title, pub_date_or_author)>
/// `ok`: false = error state (feed unavailable)
pub fn render_rss_feed(feed_title: &str, items: &[(String, String)], ok: bool) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    let header_color = if ok { BLUE } else { RED };
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    let title_display = if feed_title.is_empty() {
        "RSS FEED"
    } else {
        feed_title
    };
    let title_scale = if title_display.len() <= 20 {
        22.0f32
    } else {
        16.0
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(title_scale),
        7,
        header_color,
        title_display,
    );
    let status_str = if ok { "● LIVE" } else { "✕ OFFLINE" };
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        32,
        if ok { GREEN } else { RED },
        status_str,
    );

    if items.is_empty() {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(18.0),
            220,
            OVERLAY0,
            if ok { "no items" } else { "feed unavailable" },
        );
        return DynamicImage::ImageRgb8(img);
    }

    // Draw items
    const ITEM_H: i32 = 68;
    const MAX_ITEMS: usize = 6;
    let colors = [BLUE, MAUVE, GREEN, TEAL, PEACH, YELLOW];

    for (idx, (title, meta)) in items.iter().take(MAX_ITEMS).enumerate() {
        let y = 56 + idx as i32 * ITEM_H;
        let accent = colors[idx % colors.len()];

        // Left accent bar
        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, y).of_size(4, ITEM_H as u32 - 2),
            accent,
        );

        // Divider between items
        if idx > 0 {
            draw_filled_rect_mut(&mut img, Rect::at(4, y - 1).of_size(476, 1), SURFACE0);
        }

        // Title — word-wrapped to ~44 chars across 2 lines
        let title_clean: String = title.chars().take(88).collect();
        let (line1, line2) = if title_clean.len() <= 44 {
            (title_clean.as_str(), "")
        } else {
            // find last space before char 44
            let break_at = title_clean[..44].rfind(' ').unwrap_or(44);
            (
                &title_clean[..break_at],
                title_clean[break_at..].trim_start(),
            )
        };
        draw_text_mut(&mut img, TEXT, 10, y + 4, PxScale::from(16.0), &bold, line1);
        if !line2.is_empty() {
            let line2_trim: String = line2.chars().take(44).collect();
            draw_text_mut(
                &mut img,
                SUBTEXT,
                10,
                y + 22,
                PxScale::from(15.0),
                &mono,
                &line2_trim,
            );
        }

        // Meta (date/author)
        if !meta.is_empty() {
            let meta_short: String = meta.chars().take(40).collect();
            draw_text_mut(
                &mut img,
                OVERLAY0,
                10,
                y + ITEM_H - 18,
                PxScale::from(13.0),
                &mono,
                &meta_short,
            );
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Systemd / service status ─────────────────────────────────────────────────

/// A snapshot of one systemd unit.
#[derive(Clone, Debug)]
pub struct ServiceStatus {
    pub name: String,
    pub state: String, // "active", "inactive", "failed", ...
    pub sub: String,   // "running", "dead", "exited", ...
}

/// Render a grid of systemd service statuses.
pub fn render_service_status(units: &[ServiceStatus]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(24.0),
        12,
        BLUE,
        "SERVICE STATUS",
    );

    let now_str = Local::now().format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    if units.is_empty() {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(18.0),
            220,
            OVERLAY0,
            "no units configured",
        );
        return DynamicImage::ImageRgb8(img);
    }

    const ROW_H: i32 = 36;
    const MAX_ROWS: usize = 11;

    for (idx, svc) in units.iter().take(MAX_ROWS).enumerate() {
        let y = 54 + idx as i32 * ROW_H;

        // Alternating row background
        if idx % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, ROW_H as u32),
                SURFACE0,
            );
        }

        // Status indicator
        let (indicator, ind_color) = match svc.state.as_str() {
            "active" => ("●", GREEN),
            "failed" => ("✖", RED),
            "inactive" => ("○", OVERLAY0),
            _ => ("?", YELLOW),
        };
        draw_text_mut(
            &mut img,
            ind_color,
            8,
            y + 8,
            PxScale::from(18.0),
            &bold,
            indicator,
        );

        // Service name (truncated)
        let name_short: String = svc
            .name
            .trim_end_matches(".service")
            .chars()
            .take(24)
            .collect();
        draw_text_mut(
            &mut img,
            TEXT,
            30,
            y + 9,
            PxScale::from(16.0),
            &mono,
            &name_short,
        );

        // Sub-state
        let sub_color = match svc.sub.as_str() {
            "running" => GREEN,
            "exited" => SUBTEXT,
            "dead" => OVERLAY0,
            "failed" => RED,
            _ => YELLOW,
        };
        let sub_x = 480 - 80;
        draw_text_mut(
            &mut img,
            sub_color,
            sub_x,
            y + 9,
            PxScale::from(15.0),
            &mono,
            &svc.sub,
        );
    }

    // Footer count
    draw_divider(&mut img, 452);
    let active_count = units.iter().filter(|u| u.state == "active").count();
    let failed_count = units.iter().filter(|u| u.state == "failed").count();
    let summary = format!(
        "{} active  {} failed  {} total",
        active_count,
        failed_count,
        units.len()
    );
    draw_centered(&mut img, &mono, PxScale::from(14.0), 456, SUBTEXT, &summary);

    DynamicImage::ImageRgb8(img)
}

// ─── Network connections (listening ports + established) ──────────────────────

/// A parsed TCP/UDP socket entry from /proc/net/tcp or /proc/net/udp.
#[derive(Clone, Debug)]
pub struct NetConn {
    pub proto: String, // "tcp" / "tcp6" / "udp"
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    /// TCP state name (e.g. "LISTEN", "ESTABLISHED") or empty for UDP.
    pub state: String,
}

/// Parse /proc/net/tcp (or tcp6/udp) format into a list of socket entries.
/// Only returns LISTEN and ESTABLISHED connections by default.
pub fn read_net_connections() -> Vec<NetConn> {
    fn parse_hex_addr(hex: &str) -> (String, u16) {
        if let Some((addr_h, port_h)) = hex.split_once(':') {
            let port = u16::from_str_radix(port_h, 16).unwrap_or(0);
            // IPv4 little-endian 32-bit hex
            if addr_h.len() == 8 {
                let a = u32::from_str_radix(addr_h, 16).unwrap_or(0);
                let ip = std::net::Ipv4Addr::from(u32::from_be(a.swap_bytes()));
                let s = if ip.is_unspecified() {
                    "0.0.0.0".to_string()
                } else {
                    ip.to_string()
                };
                return (s, port);
            }
            // IPv6: 32 hex chars
            if addr_h.len() == 32 {
                // 4 groups of 4 bytes, each group reversed
                let mut bytes = [0u8; 16];
                for g in 0..4 {
                    let grp = &addr_h[g * 8..(g + 1) * 8];
                    let val = u32::from_str_radix(grp, 16).unwrap_or(0).swap_bytes();
                    bytes[g * 4..(g + 1) * 4].copy_from_slice(&val.to_be_bytes());
                }
                let ip6 = std::net::Ipv6Addr::from(bytes);
                if let Some(v4) = ip6.to_ipv4() {
                    return (v4.to_string(), port);
                }
                return (ip6.to_string(), port);
            }
        }
        ("?".to_string(), 0)
    }

    const TCP_STATES: &[&str] = &[
        "",
        "ESTABLISHED",
        "SYN_SENT",
        "SYN_RECV",
        "FIN_WAIT1",
        "FIN_WAIT2",
        "TIME_WAIT",
        "CLOSE",
        "CLOSE_WAIT",
        "LAST_ACK",
        "LISTEN",
        "CLOSING",
    ];

    let mut out = Vec::new();

    let files = [
        ("/proc/net/tcp", "tcp", true),
        ("/proc/net/tcp6", "tcp6", true),
        ("/proc/net/udp", "udp", false),
    ];

    for (path, proto, is_tcp) in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 4 {
                continue;
            }
            let (local_addr, local_port) = parse_hex_addr(cols[1]);
            let (remote_addr, remote_port) = parse_hex_addr(cols[2]);
            let state_hex = u8::from_str_radix(cols[3], 16).unwrap_or(0) as usize;
            let state = if *is_tcp {
                TCP_STATES.get(state_hex).unwrap_or(&"").to_string()
            } else {
                String::new()
            };
            // Only show LISTEN and ESTABLISHED for TCP; all for UDP (but limit)
            if *is_tcp && state != "LISTEN" && state != "ESTABLISHED" {
                continue;
            }
            // Skip pure-loopback established connections
            if state == "ESTABLISHED"
                && local_addr.starts_with("127.")
                && remote_addr.starts_with("127.")
            {
                continue;
            }
            out.push(NetConn {
                proto: proto.to_string(),
                local_addr,
                local_port,
                remote_addr,
                remote_port,
                state,
            });
        }
    }
    out
}

/// Render active network connections: listening ports and established sessions.
pub fn render_net_connections() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let conns = read_net_connections();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(24.0),
        12,
        TEAL,
        "CONNECTIONS",
    );
    let now_str = Local::now().format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    if conns.is_empty() {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(18.0),
            220,
            OVERLAY0,
            "no connections",
        );
        return DynamicImage::ImageRgb8(img);
    }

    // Column header
    let hdr_y = 54i32;
    draw_filled_rect_mut(&mut img, Rect::at(0, hdr_y).of_size(480, 20), SURFACE0);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        hdr_y + 3,
        PxScale::from(12.0),
        &mono,
        "PROTO",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        60,
        hdr_y + 3,
        PxScale::from(12.0),
        &mono,
        "LOCAL:PORT",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        220,
        hdr_y + 3,
        PxScale::from(12.0),
        &mono,
        "REMOTE:PORT",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        370,
        hdr_y + 3,
        PxScale::from(12.0),
        &mono,
        "STATE",
    );

    const ROW_H: i32 = 30;
    const MAX_ROWS: usize = 13;
    let listen_first: Vec<_> = {
        let mut v: Vec<_> = conns.into_iter().collect();
        v.sort_by(|a, b| {
            // LISTEN first, then ESTABLISHED, then UDP
            let rank = |c: &NetConn| match c.state.as_str() {
                "LISTEN" => 0,
                "ESTABLISHED" => 1,
                _ => 2,
            };
            rank(a).cmp(&rank(b)).then(a.local_port.cmp(&b.local_port))
        });
        v
    };

    for (idx, conn) in listen_first.iter().take(MAX_ROWS).enumerate() {
        let y = hdr_y + 22 + idx as i32 * ROW_H;

        if idx % 2 == 1 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y).of_size(480, ROW_H as u32),
                SURFACE0,
            );
        }

        let (state_color, proto_color) = match conn.state.as_str() {
            "LISTEN" => (GREEN, TEAL),
            "ESTABLISHED" => (BLUE, BLUE),
            _ => (OVERLAY0, OVERLAY0), // UDP
        };

        // Proto
        draw_text_mut(
            &mut img,
            proto_color,
            8,
            y + 7,
            PxScale::from(13.0),
            &mono,
            &conn.proto,
        );

        // Local addr:port
        let local_str = if conn.local_addr == "0.0.0.0" || conn.local_addr == "::" {
            format!("*:{}", conn.local_port)
        } else {
            // Shorten long IPv6
            let addr = if conn.local_addr.len() > 15 {
                "…".to_string()
            } else {
                conn.local_addr.clone()
            };
            format!("{}:{}", addr, conn.local_port)
        };
        draw_text_mut(
            &mut img,
            TEXT,
            60,
            y + 7,
            PxScale::from(13.0),
            &mono,
            &local_str,
        );

        // Remote addr:port (only for ESTABLISHED)
        if conn.state == "ESTABLISHED" {
            let rem_str = format!("{}:{}", conn.remote_addr, conn.remote_port);
            let rem_short: String = rem_str.chars().take(20).collect();
            draw_text_mut(
                &mut img,
                SUBTEXT,
                220,
                y + 7,
                PxScale::from(13.0),
                &mono,
                &rem_short,
            );
        } else {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                220,
                y + 7,
                PxScale::from(13.0),
                &mono,
                "—",
            );
        }

        // State
        draw_text_mut(
            &mut img,
            state_color,
            370,
            y + 7,
            PxScale::from(12.0),
            &mono,
            &conn.state,
        );
    }

    // Footer
    let listen_count = listen_first.iter().filter(|c| c.state == "LISTEN").count();
    let estab_count = listen_first
        .iter()
        .filter(|c| c.state == "ESTABLISHED")
        .count();
    draw_divider(&mut img, 452);
    let summary = format!("{listen_count} listening  {estab_count} established");
    draw_centered(&mut img, &mono, PxScale::from(14.0), 456, SUBTEXT, &summary);

    DynamicImage::ImageRgb8(img)
}

// ─── Docker containers display ───────────────────────────────────────────────

/// One Docker container as returned by `docker ps`.
#[derive(Debug, Clone)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    pub running: bool,
}

/// Query `docker ps -a --format '{{json .}}'` and parse the output.
/// Returns containers sorted running-first.
pub fn read_docker_containers() -> Vec<DockerContainer> {
    let out = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&out);

    let mut containers: Vec<DockerContainer> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            // Parse JSON object fields without serde dependency
            let field = |key: &str| -> Option<String> {
                let needle = format!("\"{key}\":\"");
                let start = line.find(&needle)?;
                let rest = &line[start + needle.len()..];
                let end = rest.find('"')?;
                Some(rest[..end].to_string())
            };
            let id = field("ID").unwrap_or_default();
            let name = field("Names").unwrap_or_default();
            let image = field("Image").unwrap_or_default();
            let status = field("Status").unwrap_or_default();
            let ports = field("Ports").unwrap_or_default();
            let running = status.to_lowercase().starts_with("up");
            if id.is_empty() && name.is_empty() {
                return None;
            }
            Some(DockerContainer {
                id,
                name,
                image,
                status,
                ports,
                running,
            })
        })
        .collect();

    // Running first, then by name
    containers.sort_by(|a, b| b.running.cmp(&a.running).then(a.name.cmp(&b.name)));
    containers
}

/// Render a Docker containers overview (480×480).
///
/// Shows up to 10 containers with name, image, and Up/Exited status.
pub fn render_docker_status(containers: &[DockerContainer]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 6, TEAL, "DOCKER");
    let up_count = containers.iter().filter(|c| c.running).count();
    let total_count = containers.len();
    let sub = if total_count == 0 {
        "docker not available".to_string()
    } else {
        format!("{up_count}/{total_count} running")
    };
    draw_centered(&mut img, &mono, PxScale::from(14.0), 32, OVERLAY0, &sub);

    if containers.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(18.0),
            220,
            SUBTEXT,
            "No containers found",
        );
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(13.0),
            248,
            OVERLAY0,
            "Is Docker running?",
        );
        draw_divider(&mut img, 452);
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(13.0),
            457,
            OVERLAY0,
            &Local::now().format("%H:%M:%S").to_string(),
        );
        return DynamicImage::ImageRgb8(img);
    }

    // ── Column headers ────────────────────────────────────────────────────────
    let header_y = 56i32;
    let col_name_x = 8i32;
    let col_img_x = 200i32;
    let col_stat_x = 360i32;
    let hscale = PxScale::from(12.0);
    draw_text_mut(
        &mut img, OVERLAY0, col_name_x, header_y, hscale, &mono, "NAME",
    );
    draw_text_mut(
        &mut img, OVERLAY0, col_img_x, header_y, hscale, &mono, "IMAGE",
    );
    draw_text_mut(
        &mut img, OVERLAY0, col_stat_x, header_y, hscale, &mono, "STATUS",
    );
    draw_filled_rect_mut(
        &mut img,
        Rect::at(0, header_y + 14).of_size(480, 1),
        SURFACE0,
    );

    // ── Rows ─────────────────────────────────────────────────────────────────
    let row_h = 40i32;
    let max_rows = 10usize;
    let mut y = header_y + 18;
    let rscale = PxScale::from(14.0);
    let sscale = PxScale::from(12.0);

    for c in containers.iter().take(max_rows) {
        let status_color = if c.running { GREEN } else { RED };
        let status_short = if c.running {
            // "Up 3 hours" → "Up 3h"
            let s = c.status.as_str();
            if s.contains("hour") {
                let n: String = s
                    .chars()
                    .skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}h", n.trim())
            } else if s.contains("minute") {
                let n: String = s
                    .chars()
                    .skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}m", n.trim())
            } else if s.contains("second") {
                "Up <1m".to_string()
            } else if s.contains("day") {
                let n: String = s
                    .chars()
                    .skip(3)
                    .take_while(|ch| ch.is_numeric() || *ch == ' ')
                    .collect();
                format!("Up{}d", n.trim())
            } else {
                s.chars().take(10).collect()
            }
        } else {
            "Exited".to_string()
        };

        let name_short: String = c.name.trim_start_matches('/').chars().take(22).collect();
        let image_short: String = {
            // strip registry prefix if present
            let img_name = c.image.rsplit('/').next().unwrap_or(&c.image);
            // strip tag after ':'
            let base = img_name.split(':').next().unwrap_or(img_name);
            base.chars().take(18).collect()
        };

        // Row background for running containers
        if c.running {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y - 2).of_size(480, row_h as u32 - 2),
                Rgb([40, 50, 44]),
            );
        }

        draw_text_mut(&mut img, TEXT, col_name_x, y, rscale, &bold, &name_short);
        draw_text_mut(&mut img, SUBTEXT, col_img_x, y, sscale, &mono, &image_short);
        draw_text_mut(
            &mut img,
            status_color,
            col_stat_x,
            y,
            sscale,
            &mono,
            &status_short,
        );

        // Indicator dot
        let dot_color = if c.running { GREEN } else { Rgb([80, 80, 80]) };
        draw_filled_rect_mut(&mut img, Rect::at(470, y + 4).of_size(6, 6), dot_color);

        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, y + row_h - 4).of_size(480, 1),
            SURFACE0,
        );
        y += row_h;
    }

    if containers.len() > max_rows {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(12.0),
            y + 2,
            OVERLAY0,
            &format!("… {} more", containers.len() - max_rows),
        );
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        457,
        OVERLAY0,
        &Local::now().format("%H:%M:%S").to_string(),
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Logged-in users display ─────────────────────────────────────────────────

/// A user session as parsed from `who -u`.
#[derive(Debug, Clone)]
pub struct UserSession {
    pub username: String,
    pub tty: String,
    pub login_at: String,
    pub idle: String,
    pub pid: String,
    pub from: String,
}

/// Read active user sessions from `who -u`.
pub fn read_user_sessions() -> Vec<UserSession> {
    let out = std::process::Command::new("who").arg("-u").output();
    let out = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&out);

    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let username = parts.next()?.to_string();
            let tty = parts.next().unwrap_or("?").to_string();
            // next two tokens are date and time
            let date = parts.next().unwrap_or("").to_string();
            let time = parts.next().unwrap_or("").to_string();
            let login_at = format!("{date} {time}");
            let idle = parts.next().unwrap_or(".").to_string();
            let pid = parts.next().unwrap_or("?").to_string();
            // rest is FROM (may be empty for local sessions)
            let from: String = parts.collect::<Vec<_>>().join(" ");
            Some(UserSession {
                username,
                tty,
                login_at,
                idle,
                pid,
                from,
            })
        })
        .collect()
}

/// Render a logged-in users panel (480×480).
pub fn render_logged_in_users(sessions: &[UserSession]) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(22.0),
        6,
        MAUVE,
        "ACTIVE SESSIONS",
    );
    let count_str = match sessions.len() {
        0 => "no users logged in".to_string(),
        1 => "1 user".to_string(),
        n => format!("{n} users"),
    };
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        32,
        OVERLAY0,
        &count_str,
    );

    if sessions.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(18.0),
            230,
            SUBTEXT,
            "No active sessions",
        );
        draw_divider(&mut img, 452);
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(13.0),
            457,
            OVERLAY0,
            &Local::now().format("%H:%M:%S").to_string(),
        );
        return DynamicImage::ImageRgb8(img);
    }

    // ── Column headers ────────────────────────────────────────────────────────
    let header_y = 58i32;
    let hscale = PxScale::from(12.0);
    draw_text_mut(&mut img, OVERLAY0, 8, header_y, hscale, &mono, "USER");
    draw_text_mut(&mut img, OVERLAY0, 160, header_y, hscale, &mono, "TTY");
    draw_text_mut(
        &mut img,
        OVERLAY0,
        230,
        header_y,
        hscale,
        &mono,
        "LOGGED IN",
    );
    draw_text_mut(&mut img, OVERLAY0, 380, header_y, hscale, &mono, "IDLE");
    draw_filled_rect_mut(
        &mut img,
        Rect::at(0, header_y + 14).of_size(480, 1),
        SURFACE0,
    );

    // ── Rows ─────────────────────────────────────────────────────────────────
    let row_h = 38i32;
    let max_rows = 10usize;
    let mut y = header_y + 18;
    let rscale = PxScale::from(15.0);
    let sscale = PxScale::from(13.0);

    for sess in sessions.iter().take(max_rows) {
        let user_short: String = sess.username.chars().take(14).collect();
        let tty_short: String = sess.tty.chars().take(8).collect();
        // Login time: keep only HH:MM part
        let login_short: String = sess
            .login_at
            .split_whitespace()
            .last()
            .unwrap_or(&sess.login_at)
            .chars()
            .take(10)
            .collect();
        // Idle: '.' means active
        let idle_short: String = if sess.idle == "." {
            "active".to_string()
        } else {
            sess.idle.chars().take(7).collect()
        };
        let idle_color = if sess.idle == "." { GREEN } else { YELLOW };

        // From annotation (SSH source IP / console)
        let from_short: String = if sess.from.is_empty() {
            "local".to_string()
        } else {
            sess.from.chars().take(18).collect()
        };

        draw_text_mut(&mut img, BLUE, 8, y, rscale, &bold, &user_short);
        draw_text_mut(&mut img, SUBTEXT, 160, y, sscale, &mono, &tty_short);
        draw_text_mut(&mut img, TEXT, 230, y, sscale, &mono, &login_short);
        draw_text_mut(&mut img, idle_color, 380, y, sscale, &mono, &idle_short);

        // Source on second line if present
        if !from_short.is_empty() && from_short != "local" {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                160,
                y + 18,
                PxScale::from(11.0),
                &mono,
                &format!("← {from_short}"),
            );
        } else if from_short == "local" {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                160,
                y + 18,
                PxScale::from(11.0),
                &mono,
                "local",
            );
        }

        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, y + row_h - 4).of_size(480, 1),
            SURFACE0,
        );
        y += row_h;
    }

    if sessions.len() > max_rows {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(12.0),
            y + 2,
            OVERLAY0,
            &format!("… {} more", sessions.len() - max_rows),
        );
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    draw_divider(&mut img, 452);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        457,
        OVERLAY0,
        &Local::now().format("%H:%M:%S").to_string(),
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod docker_session_tests {
    use super::*;

    #[test]
    fn test_docker_status_empty_size() {
        let img = render_docker_status(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_docker_status_with_containers() {
        let containers = vec![
            DockerContainer {
                id: "abc123".into(),
                name: "web-server".into(),
                image: "nginx:latest".into(),
                status: "Up 3 hours".into(),
                ports: "0.0.0.0:80->80/tcp".into(),
                running: true,
            },
            DockerContainer {
                id: "def456".into(),
                name: "database".into(),
                image: "postgres:15".into(),
                status: "Exited (0) 2 days ago".into(),
                ports: "".into(),
                running: false,
            },
        ];
        let img = render_docker_status(&containers);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_logged_in_users_empty_size() {
        let img = render_logged_in_users(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_logged_in_users_with_sessions() {
        let sessions = vec![
            UserSession {
                username: "alice".into(),
                tty: "pts/0".into(),
                login_at: "May12 09:30".into(),
                idle: ".".into(),
                pid: "12345".into(),
                from: "192.168.1.50".into(),
            },
            UserSession {
                username: "bob".into(),
                tty: "tty1".into(),
                login_at: "May13 07:00".into(),
                idle: "00:05".into(),
                pid: "11111".into(),
                from: "".into(),
            },
        ];
        let img = render_logged_in_users(&sessions);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}
