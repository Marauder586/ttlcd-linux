//! Multi-panel overviews and gauges.

use super::*;

// ─── Dashboard (4-quadrant overview) ─────────────────────────────────────────

/// Render a four-quadrant overview dashboard:
///
/// ```text
/// ┌─────────────────┬─────────────────┐
/// │  CLOCK + DATE   │  CPU / MEM      │
/// │                 │                 │
/// ├─────────────────┼─────────────────┤
/// │  NETWORK I/O    │  TEMPERATURE    │
/// │                 │                 │
/// └─────────────────┴─────────────────┘
/// ```
///
/// All stats are collected fresh on each call. Useful as a "glance" mode.
pub fn render_dashboard() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Collect all stats ─────────────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %-d").to_string();

    let cpu_pct = sys.global_cpu_usage();
    let used_mem = sys.used_memory();
    let total_mem = sys.total_memory();
    let mem_pct = if total_mem > 0 {
        used_mem as f64 / total_mem as f64 * 100.0
    } else {
        0.0
    };
    let used_gb = used_mem as f64 / 1_073_741_824.0;
    let total_gb = total_mem as f64 / 1_073_741_824.0;

    // Temperature
    let mut comps = sysinfo::Components::new_with_refreshed_list();
    comps.refresh(false);
    let temp_c: Option<f32> = comps
        .iter()
        .filter(|c| {
            let l = c.label().to_lowercase();
            l.contains("cpu") || l.contains("core") || l.contains("package")
        })
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);

    // Network
    use sysinfo::Networks;
    let networks = Networks::new_with_refreshed_list();
    let (rx_b, tx_b): (u64, u64) = networks
        .iter()
        .filter(|(name, _)| !name.starts_with("lo"))
        .fold((0, 0), |(r, t), (_, d)| {
            (r + d.received(), t + d.transmitted())
        });

    let load = System::load_average();
    let uptime_secs = System::uptime();
    let hostname = System::host_name().unwrap_or_else(|| "ttlcd".into());

    // ── Layout constants ──────────────────────────────────────────────────
    let mid_x: i32 = 240;
    let mid_y: i32 = 240;
    let pad: i32 = 10;

    // ── Quadrant dividers ─────────────────────────────────────────────────
    // Vertical centre line
    draw_filled_rect_mut(&mut img, Rect::at(mid_x - 1, 0).of_size(2, 480), SURFACE0);
    // Horizontal centre line
    draw_filled_rect_mut(&mut img, Rect::at(0, mid_y - 1).of_size(480, 2), SURFACE0);

    // ── Top-left: Clock ───────────────────────────────────────────────────
    let tl_col = MAUVE;
    draw_text_mut(
        &mut img,
        tl_col,
        pad,
        pad + 2,
        PxScale::from(18.0),
        &mono,
        &hostname,
    );
    draw_text_mut(
        &mut img,
        TEXT,
        pad,
        pad + 26,
        PxScale::from(48.0),
        &mono,
        &time_str,
    );
    draw_text_mut(
        &mut img,
        SUBTEXT,
        pad,
        pad + 80,
        PxScale::from(20.0),
        &bold,
        &date_str,
    );

    let uptime = format_uptime(uptime_secs);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        pad,
        pad + 106,
        PxScale::from(16.0),
        &mono,
        &format!("up {uptime}"),
    );

    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        pad,
        pad + 126,
        PxScale::from(14.0),
        &mono,
        &load_str,
    );

    // ── Top-right: CPU + Memory ───────────────────────────────────────────
    let tr_x = mid_x + pad;
    let cpu_col = if cpu_pct > 80.0 {
        RED
    } else if cpu_pct > 50.0 {
        YELLOW
    } else {
        GREEN
    };

    draw_text_mut(
        &mut img,
        BLUE,
        tr_x,
        pad + 2,
        PxScale::from(18.0),
        &bold,
        "CPU",
    );
    draw_text_mut(
        &mut img,
        cpu_col,
        tr_x,
        pad + 24,
        PxScale::from(38.0),
        &mono,
        &format!("{cpu_pct:4.1}%"),
    );
    draw_bar(&mut img, tr_x, pad + 70, 220, 14, cpu_pct / 100.0, cpu_col);

    draw_divider(&mut img, pad + 92);

    let mem_col = if mem_pct > 85.0 {
        RED
    } else if mem_pct > 65.0 {
        YELLOW
    } else {
        GREEN
    };
    draw_text_mut(
        &mut img,
        BLUE,
        tr_x,
        pad + 98,
        PxScale::from(18.0),
        &bold,
        "MEM",
    );
    draw_text_mut(
        &mut img,
        mem_col,
        tr_x,
        pad + 120,
        PxScale::from(30.0),
        &mono,
        &format!("{used_gb:.1}/{total_gb:.1}G"),
    );
    draw_bar(
        &mut img,
        tr_x,
        pad + 158,
        220,
        14,
        (mem_pct / 100.0) as f32,
        mem_col,
    );

    draw_text_mut(
        &mut img,
        OVERLAY0,
        tr_x,
        pad + 178,
        PxScale::from(16.0),
        &mono,
        &format!("{mem_pct:.0}%"),
    );

    // ── Bottom-left: Network ──────────────────────────────────────────────
    let bl_y = mid_y + pad;
    draw_text_mut(
        &mut img,
        TEAL,
        pad,
        bl_y,
        PxScale::from(18.0),
        &bold,
        "NETWORK",
    );
    let rx_str = format!("↓ {}", format_bytes_compact(rx_b));
    let tx_str = format!("↑ {}", format_bytes_compact(tx_b));
    draw_text_mut(
        &mut img,
        GREEN,
        pad,
        bl_y + 26,
        PxScale::from(28.0),
        &mono,
        &rx_str,
    );
    draw_text_mut(
        &mut img,
        YELLOW,
        pad,
        bl_y + 60,
        PxScale::from(28.0),
        &mono,
        &tx_str,
    );

    // Active interfaces count
    let if_count = networks
        .iter()
        .filter(|(n, _)| !n.starts_with("lo"))
        .count();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        pad,
        bl_y + 96,
        PxScale::from(16.0),
        &mono,
        &format!(
            "{} interface{}",
            if_count,
            if if_count == 1 { "" } else { "s" }
        ),
    );

    // ── Bottom-right: Temperature ─────────────────────────────────────────
    let br_x = mid_x + pad;
    let br_y = mid_y + pad;
    draw_text_mut(
        &mut img,
        PEACH,
        br_x,
        br_y,
        PxScale::from(18.0),
        &bold,
        "TEMP",
    );

    match temp_c {
        None => {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                br_x,
                br_y + 28,
                PxScale::from(22.0),
                &bold,
                "N/A",
            );
        }
        Some(t) => {
            let t_col = if t > 85.0 {
                RED
            } else if t > 70.0 {
                YELLOW
            } else {
                GREEN
            };
            draw_text_mut(
                &mut img,
                t_col,
                br_x,
                br_y + 24,
                PxScale::from(44.0),
                &mono,
                &format!("{t:.0}°C"),
            );
            let bar_frac = ((t - 20.0) / 80.0).clamp(0.0, 1.0);
            draw_bar(&mut img, br_x, br_y + 76, 220, 14, bar_frac, t_col);

            // Show a few sensor labels
            let mut sensor_y = br_y + 98;
            for c in comps.iter().take(3) {
                if let Some(st) = c.temperature() {
                    if st > 0.0 && !st.is_nan() {
                        let label: String = c.label().chars().take(14).collect();
                        let sc = if st > 80.0 {
                            RED
                        } else if st > 65.0 {
                            YELLOW
                        } else {
                            OVERLAY0
                        };
                        draw_text_mut(
                            &mut img,
                            sc,
                            br_x,
                            sensor_y,
                            PxScale::from(14.0),
                            &mono,
                            &format!("{label:<14} {st:.0}°"),
                        );
                        sensor_y += 18;
                        if sensor_y > 470 {
                            break;
                        }
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

pub(super) fn format_bytes_compact(b: u64) -> String {
    if b >= 1_000_000_000 {
        format!("{:.1}GB", b as f64 / 1_000_000_000.0)
    } else if b >= 1_000_000 {
        format!("{:.1}MB", b as f64 / 1_000_000.0)
    } else if b >= 1_000 {
        format!("{:.0}KB", b as f64 / 1_000.0)
    } else {
        format!("{b}B")
    }
}

/// Remove ANSI/VT100 escape sequences from a string.
/// Handles CSI sequences (ESC [ ... m) and OSC sequences.
pub(super) fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Skip escape sequence
            i += 1;
            if i < bytes.len() {
                match bytes[i] {
                    b'[' => {
                        // CSI sequence: ESC [ ... (letter)
                        i += 1;
                        while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        if i < bytes.len() {
                            i += 1;
                        } // skip the terminating letter
                    }
                    b']' => {
                        // OSC sequence: ESC ] ... ST (BEL or ESC\)
                        i += 1;
                        while i < bytes.len() && bytes[i] != 0x07 && bytes[i] != 0x1b {
                            i += 1;
                        }
                        if i < bytes.len() {
                            i += 1;
                        }
                    }
                    _ => {
                        // Other two-char sequences
                        i += 1;
                    }
                }
            }
        } else if bytes[i] < 0x20 && bytes[i] != b'\t' {
            // Skip other control chars except tab
            i += 1;
        } else {
            // Safe to push this char
            if let Some(c) = s[i..].chars().next() {
                out.push(c);
                i += c.len_utf8();
            } else {
                i += 1;
            }
        }
    }
    out
}

// ─── Status Overview (4-quadrant live status) ────────────────────────────────

/// Render a 4-quadrant live status overview (480×480).
///
/// Top-left: Clock + date.  Top-right: CPU % + Memory.
/// Bottom-left: Network Rx/Tx.  Bottom-right: Temp + Uptime.
pub fn render_status_overview() -> DynamicImage {
    use sysinfo::{Components, CpuRefreshKind, RefreshKind, System};

    let refresh = RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory(sysinfo::MemoryRefreshKind::everything());
    let mut sys = System::new_with_specifics(refresh);
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_specifics(refresh);

    let now = Local::now();
    let cpu = sys.global_cpu_usage();
    let used = sys.used_memory();
    let total = sys.total_memory();
    let mem_pct = if total > 0 {
        used as f32 / total as f32 * 100.0
    } else {
        0.0
    };
    let load = System::load_average();
    let uptime = System::uptime();

    // Network
    use sysinfo::Networks;
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(100));
    nets.refresh(true);
    let (rx_bps, tx_bps) = nets.iter().fold((0u64, 0u64), |(rx, tx), (_, d)| {
        (rx + d.received() * 10, tx + d.transmitted() * 10) // ~100ms × 10 = /s
    });

    // Temperature
    let comps = Components::new_with_refreshed_list();
    let hot_temp = comps
        .iter()
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mid = 240i32;

    // ── Quadrant background tint ──────────────────────────────────────────
    // All 4 quads use the same base background with subtle dividers

    // Divider lines
    for x in 0..480u32 {
        img.put_pixel(x, mid as u32, SURFACE0);
    }
    for y in 0..480u32 {
        img.put_pixel(mid as u32, y, SURFACE0);
    }

    // ── TL: Clock ─────────────────────────────────────────────────────────
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %-e").to_string();
    let wday_str = now.format("%Y").to_string();
    draw_centered_in(
        &mut img,
        &mono,
        PxScale::from(40.0),
        0,
        0,
        mid,
        mid - 10,
        TEAL,
        &time_str,
    );
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(17.0),
        0,
        0,
        mid,
        mid + 60,
        TEXT,
        &date_str,
    );
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(14.0),
        0,
        0,
        mid,
        mid + 85,
        SUBTEXT,
        &wday_str,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        8,
        PxScale::from(12.0),
        &mono,
        "CLOCK",
    );

    // ── TR: CPU + Memory ──────────────────────────────────────────────────
    let cpu_color = if cpu > 80.0 {
        RED
    } else if cpu > 50.0 {
        YELLOW
    } else {
        MAUVE
    };
    let mem_color = if mem_pct > 90.0 {
        RED
    } else if mem_pct > 75.0 {
        YELLOW
    } else {
        BLUE
    };
    let cpu_str = format!("{cpu:.0}%");
    let mem_str = format!("{mem_pct:.0}%");

    draw_text_mut(
        &mut img,
        OVERLAY0,
        mid + 8,
        8,
        PxScale::from(12.0),
        &mono,
        "CPU / MEM",
    );
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(34.0),
        mid,
        0,
        mid,
        mid - 60,
        cpu_color,
        &cpu_str,
    );

    // CPU bar
    let bar_x = mid + 12;
    let bar_y = mid / 2 - 5;
    let bar_w = mid - 24;
    draw_bar(&mut img, bar_x, bar_y, bar_w, 10, cpu / 100.0, cpu_color);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        mid + 8,
        mid / 2 + 10,
        PxScale::from(13.0),
        &mono,
        "CPU",
    );

    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(34.0),
        mid,
        0,
        mid,
        mid + 30,
        mem_color,
        &mem_str,
    );
    draw_bar(
        &mut img,
        bar_x,
        mid - 35,
        bar_w,
        10,
        mem_pct / 100.0,
        mem_color,
    );
    draw_text_mut(
        &mut img,
        SUBTEXT,
        mid + 8,
        mid - 20,
        PxScale::from(13.0),
        &mono,
        "MEM",
    );

    let load_str = format!("load {:.2}", load.one);
    let lw = text_width(&mono, PxScale::from(13.0), &load_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - lw - 8,
        mid - 6,
        PxScale::from(13.0),
        &mono,
        &load_str,
    );

    // ── BL: Network ───────────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        8,
        mid + 8,
        PxScale::from(12.0),
        &mono,
        "NETWORK",
    );
    let rx_str = format!("↓ {}", format_bps(rx_bps));
    let tx_str = format!("↑ {}", format_bps(tx_bps));
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(28.0),
        0,
        mid,
        mid,
        mid + 60,
        GREEN,
        &rx_str,
    );
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(28.0),
        0,
        mid,
        mid,
        mid + 100,
        PEACH,
        &tx_str,
    );
    // Tiny history sparkline
    let rx_hist: Vec<f32> = net_rx_history().iter().map(|&v| v as f32).collect();
    let tx_hist: Vec<f32> = net_tx_history().iter().map(|&v| v as f32).collect();
    let net_max = rx_hist
        .iter()
        .chain(tx_hist.iter())
        .copied()
        .fold(0.0f32, f32::max)
        .max(1024.0);
    let fill_rx = Rgb([GREEN.0[0] / 10, GREEN.0[1] / 10, GREEN.0[2] / 10]);
    let fill_tx = Rgb([PEACH.0[0] / 10, PEACH.0[1] / 10, PEACH.0[2] / 10]);
    draw_line_chart(
        &mut img,
        &tx_hist,
        net_max,
        0,
        mid + 145,
        mid,
        84,
        PEACH,
        fill_tx,
    );
    draw_line_chart(
        &mut img,
        &rx_hist,
        net_max,
        0,
        mid + 145,
        mid,
        84,
        GREEN,
        fill_rx,
    );

    // ── BR: Temp + Uptime ─────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        mid + 8,
        mid + 8,
        PxScale::from(12.0),
        &mono,
        "TEMP / UPTIME",
    );
    let temp_color = if hot_temp >= 85.0 {
        RED
    } else if hot_temp >= 70.0 {
        PEACH
    } else {
        GREEN
    };
    let temp_str = if hot_temp > 0.0 {
        format!("{hot_temp:.0}°C")
    } else {
        "—".to_string()
    };
    draw_centered_in(
        &mut img,
        &bold,
        PxScale::from(40.0),
        mid,
        mid,
        mid,
        mid + 60,
        temp_color,
        &temp_str,
    );

    // Uptime
    let up_str = format_uptime(uptime);
    draw_centered_in(
        &mut img,
        &mono,
        PxScale::from(18.0),
        mid,
        mid,
        mid,
        mid + 115,
        TEAL,
        &up_str,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        mid + 8,
        473,
        PxScale::from(12.0),
        &mono,
        "up",
    );

    // ── Timestamp (bottom center) ─────────────────────────────────────────
    let ts = now.format("%H:%M:%S").to_string();
    let tsw = text_width(&mono, PxScale::from(11.0), &ts);
    draw_filled_rect_mut(
        &mut img,
        Rect::at((480 - tsw) / 2 - 2, mid - 7).of_size(tsw as u32 + 4, 14),
        SURFACE0,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - tsw) / 2,
        mid - 7,
        PxScale::from(11.0),
        &mono,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

/// Draw text centered horizontally within a sub-rectangle [ox, ox+pw] and
/// at a given absolute y coordinate.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_centered_in(
    img: &mut RgbImage,
    font: &ab_glyph::FontRef<'_>,
    scale: PxScale,
    ox: i32,
    _oy: i32,
    pw: i32,
    y: i32,
    color: Rgb<u8>,
    text: &str,
) {
    let w = text_width(font, scale, text);
    let x = ox + (pw - w) / 2;
    draw_text_mut(img, color, x.max(ox), y, scale, font, text);
}

// ─── Speedometer Gauge ───────────────────────────────────────────────────────

/// Render two large analog speedometer gauges side-by-side: CPU% (left) and RAM% (right).
/// Each gauge is a filled arc with needle, tick marks, labels, and value display.
pub fn render_speedometer_gauge() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, System};

    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    sys.refresh_memory_specifics(MemoryRefreshKind::everything());

    let cpu_pct = sys.global_cpu_usage() as f64;
    let ram_pct = if sys.total_memory() > 0 {
        sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0
    } else {
        0.0
    };

    // Also get temperature for subtitle
    let temp: f32 = {
        use sysinfo::Components;
        let comps = Components::new_with_refreshed_list();
        comps
            .iter()
            .filter_map(sysinfo::Component::temperature)
            .filter(|t| t.is_finite() && *t > 0.0)
            .fold(0.0_f32, f32::max)
    };

    let font = font_bold();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Title bar
    let title = "System Gauges";
    let tw = text_width(&font, PxScale::from(24.0), title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        6,
        PxScale::from(24.0),
        &font,
        title,
    );

    // Subtitle: temp + timestamp
    let now = chrono::Local::now();
    let sub = if temp > 0.0 {
        format!(
            "{:.0}°C  {:02}:{:02}:{:02}",
            temp,
            now.hour(),
            now.minute(),
            now.second()
        )
    } else {
        format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
    };
    let sw = text_width(&font, PxScale::from(16.0), &sub);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - sw) / 2,
        34,
        PxScale::from(16.0),
        &font,
        &sub,
    );

    // ── Core draw_gauge closure ────────────────────────────────────────────────
    // Gauge spans 225° to 315° (clockwise, i.e. -135° to +135° from the bottom)
    // In standard screen coords (y down): start at bottom-left, end at bottom-right
    let draw_gauge = |img: &mut RgbImage,
                      cx: i32,
                      cy: i32,
                      r: i32,
                      value: f64,
                      label: &str,
                      unit: &str,
                      max_val: f64| {
        // Arc from 225° to 315° going clockwise = 270 degrees total
        // In math angles (y up), 225° = bottom-left, 315° = bottom-right
        // In screen angles (y down), same numbers but sin/cos are affected
        // We'll use screen coords: angle 0 = right, increases clockwise
        // Start at 225° screen (SW), end at 315° screen (SE)
        let start_deg = 225.0_f64;
        let _end_deg = 315.0_f64;
        // Arc goes clockwise from start to end via top (0° at right, going up through 270°)
        // Arc span = 270 degrees total (225→360→45→135→315 via top, but that's 360-225+315=450... no)
        // Let's do it simply: start 225°, end 315°, span=270° going counterclockwise
        // (i.e., decreasing degrees from 225 → 180 → 90 → 0 → 315)
        // In screen coords angle a: x = cx + r*cos(a), y = cy + r*sin(a)
        // We want: 0% at 225° (bottom-left), 100% at 315° (bottom-right)
        // Going counterclockwise (decreasing angle) from 225° to 315°:
        // that's 225 → 180 → 90 → 0/360 → 315 = 270° span counterclockwise
        let arc_span_deg = 270.0_f64;

        // Convert angle function: 0..100% → screen angle
        let angle_for = |pct: f64| -> f64 {
            // Start at 225°, go counterclockwise (decreasing) by arc_span_deg * pct/100
            let a_deg = start_deg - (arc_span_deg * (pct / 100.0).clamp(0.0, 1.0));
            a_deg.to_radians()
        };

        // ── Background circle (behind gauge) ─────────────────────────────
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= (r - 1) * (r - 1) && dx * dx + dy * dy > (r - 30) * (r - 30)
                {
                    // Only draw in the arc region
                    let a_deg = (dy as f64).atan2(dx as f64).to_degrees();
                    // Normalize to 0..360
                    let a_norm = if a_deg < 0.0 { a_deg + 360.0 } else { a_deg };
                    // Check if in arc (225° → CCW → 315°, covers top half + small bottom bits)
                    // Arc covers: from 315° going up to 225° = 315°..360°, 0°..225°
                    let in_arc = a_norm <= 225.0 || a_norm >= 315.0;
                    if in_arc {
                        let px = (cx + dx) as u32;
                        let py = (cy + dy) as u32;
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, SURFACE0);
                        }
                    }
                }
            }
        }

        // ── Coloured value arc ───────────────────────────────────────────
        let val_pct = (value / max_val).clamp(0.0, 1.0) * 100.0;
        let arc_color = if val_pct < 50.0 {
            GREEN
        } else if val_pct < 75.0 {
            YELLOW
        } else {
            RED
        };

        // Draw arc by sampling angles
        let steps = 2700_i32; // 0.1° increments
        for step in 0..=steps {
            // From 225° down to 315° counterclockwise
            let a_deg = start_deg - (arc_span_deg * step as f64 / steps as f64);
            if a_deg < (start_deg - arc_span_deg * (val_pct / 100.0)) {
                break;
            }
            let a = a_deg.to_radians();
            for rr in (r - 28)..(r - 2) {
                let px = (cx as f64 + rr as f64 * a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, arc_color);
                }
            }
        }

        // ── Tick marks (0, 25, 50, 75, 100%) ────────────────────────────
        for tick_pct_10 in 0..=10 {
            let tick_pct = tick_pct_10 as f64 * 10.0;
            let a = angle_for(tick_pct);
            let (is_major, tick_inner) = if tick_pct_10 % 5 == 0 {
                (true, r - 44)
            } else {
                (false, r - 36)
            };
            for rr in tick_inner..(r + 3) {
                let px = (cx as f64 + rr as f64 * a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(
                        px as u32,
                        py as u32,
                        if is_major { OVERLAY0 } else { SURFACE0 },
                    );
                }
            }
        }

        // ── Needle ───────────────────────────────────────────────────────
        let na = angle_for(val_pct);
        let nl = r as f64 - 32.0;
        let nx = cx as f32 + (nl * na.cos()) as f32;
        let ny = cy as f32 + (nl * na.sin()) as f32;
        for offset in -1_i32..=1 {
            let perp = na + PI / 2.0;
            let sx = cx as f32 + (offset as f64 * perp.cos()) as f32;
            let sy = cy as f32 + (offset as f64 * perp.sin()) as f32;
            let lx = nx + (offset as f64 * perp.cos()) as f32;
            let ly = ny + (offset as f64 * perp.sin()) as f32;
            draw_antialiased_line_segment_mut(
                img,
                (sx as i32, sy as i32),
                (lx as i32, ly as i32),
                WHITE,
                interpolate,
            );
        }

        // ── Pivot dot ────────────────────────────────────────────────────
        for dy in -5_i32..=5 {
            for dx in -5_i32..=5 {
                if dx * dx + dy * dy <= 25 {
                    let px = (cx + dx) as u32;
                    let py = (cy + dy) as u32;
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, py, arc_color);
                    }
                }
            }
        }

        // ── Label ────────────────────────────────────────────────────────
        let lw = text_width(&font, PxScale::from(22.0), label);
        draw_text_mut(
            img,
            TEXT,
            cx - lw / 2,
            cy - r - 8,
            PxScale::from(22.0),
            &font,
            label,
        );

        // ── Value text ───────────────────────────────────────────────────
        let val_str = format!("{value:.0}{unit}");
        let vw = text_width(&font, PxScale::from(30.0), &val_str);
        draw_text_mut(
            img,
            arc_color,
            cx - vw / 2,
            cy + 14,
            PxScale::from(30.0),
            &font,
            &val_str,
        );

        // ── 0 and max labels ─────────────────────────────────────────────
        let a0 = angle_for(0.0);
        let a100 = angle_for(100.0);
        let lr = r + 18;
        let x0 = cx + (lr as f64 * a0.cos()) as i32 - 4;
        let y0 = cy + (lr as f64 * a0.sin()) as i32 - 6;
        draw_text_mut(img, OVERLAY0, x0, y0, PxScale::from(15.0), &font, "0");
        let x100 = cx + (lr as f64 * a100.cos()) as i32 - 14;
        let y100 = cy + (lr as f64 * a100.sin()) as i32 - 6;
        let max_str = format!("{max_val:.0}");
        draw_text_mut(
            img,
            OVERLAY0,
            x100,
            y100,
            PxScale::from(15.0),
            &font,
            &max_str,
        );
    };

    let gauge_r = 170_i32;
    // CPU gauge: left side
    draw_gauge(&mut img, 120, 310, gauge_r, cpu_pct, "CPU", "%", 100.0);
    // RAM gauge: right side
    draw_gauge(&mut img, 360, 310, gauge_r, ram_pct, "RAM", "%", 100.0);

    DynamicImage::ImageRgb8(img)
}

// ─── Radar Display ────────────────────────────────────────────────────────────

pub(super) static RADAR_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_radar() -> DynamicImage {
    use std::f64::consts::PI;

    let tick = {
        let t = RADAR_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let cpus = sys.cpus();
    let n_cpus = cpus.len().max(1);
    let cpu_loads: Vec<f32> = cpus.iter().map(sysinfo::Cpu::cpu_usage).collect();

    let cx = 240i32;
    let cy = 240i32;
    let r_max = 210.0f64;

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Draw concentric rings
    for ri in 1..=5u32 {
        let rr = (r_max * ri as f64 / 5.0) as f32;
        let ring_col = Rgb([20u8, 30, 45]);
        // Draw circle manually with line segments
        let segs = 120;
        for s in 0..segs {
            let a0 = s as f64 * 2.0 * PI / segs as f64;
            let a1 = (s + 1) as f64 * 2.0 * PI / segs as f64;
            let x0 = cx as f32 + rr * a0.cos() as f32;
            let y0 = cy as f32 + rr * a0.sin() as f32;
            let x1 = cx as f32 + rr * a1.cos() as f32;
            let y1 = cy as f32 + rr * a1.sin() as f32;
            imageproc::drawing::draw_line_segment_mut(&mut img, (x0, y0), (x1, y1), ring_col);
        }
    }

    // Draw grid cross
    let grid_col = Rgb([20u8, 30, 45]);
    imageproc::drawing::draw_line_segment_mut(
        &mut img,
        (cx as f32, (cy - 215) as f32),
        (cx as f32, (cy + 215) as f32),
        grid_col,
    );
    imageproc::drawing::draw_line_segment_mut(
        &mut img,
        ((cx - 215) as f32, cy as f32),
        ((cx + 215) as f32, cy as f32),
        grid_col,
    );

    // Sweep arc: 20° wide, decaying trail
    let sweep_angle = (tick as f64 * 0.05) % (2.0 * PI);
    let trail_len = std::f64::consts::PI * 0.4; // 72° trail

    for trail_step in 0..60u32 {
        let a = sweep_angle - (trail_step as f64 / 60.0) * trail_len;
        let fade = 1.0 - trail_step as f64 / 60.0;
        let alpha = (fade * 40.0) as u8;
        let col = Rgb([0u8, (40.0 * fade) as u8, (alpha).min(50)]);

        // Fill a thin wedge by drawing many radii
        for ri in 0..=(r_max as u32) {
            let px = (cx as f64 + ri as f64 * a.cos()) as i32;
            let py = (cy as f64 + ri as f64 * a.sin()) as i32;
            if (0..480).contains(&px) && (0..480).contains(&py) {
                let existing = img.get_pixel(px as u32, py as u32);
                img.put_pixel(
                    px as u32,
                    py as u32,
                    Rgb([
                        existing.0[0].saturating_add(col.0[0]),
                        existing.0[1].saturating_add(col.0[1]),
                        existing.0[2].saturating_add(col.0[2]),
                    ]),
                );
            }
        }
    }

    // Draw sweep line (bright green)
    let sweep_col = Rgb([0u8, 200, 80]);
    for ri in 0..=(r_max as u32) {
        let px = (cx as f64 + ri as f64 * sweep_angle.cos()) as i32;
        let py = (cy as f64 + ri as f64 * sweep_angle.sin()) as i32;
        if (0..480).contains(&px) && (0..480).contains(&py) {
            img.put_pixel(px as u32, py as u32, sweep_col);
        }
    }

    // Plot CPU blips as dots on the radar
    // Each CPU is placed at an angle proportional to its index, radius proportional to load
    let mut seed = tick.wrapping_mul(2654435761);
    let lcg_r = |s: u64| {
        s.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

    for (i, &load) in cpu_loads.iter().enumerate() {
        let angle = (i as f64 / n_cpus as f64) * 2.0 * PI;
        let rad = (load as f64 / 100.0) * r_max * 0.9;

        // Jitter (same each frame per core)
        seed = lcg_r(seed);
        let jitter_a = (seed as f64 / u64::MAX as f64 - 0.5) * 0.15;
        seed = lcg_r(seed);
        let jitter_r = (seed as f64 / u64::MAX as f64) * 8.0;

        let final_angle = angle + jitter_a;
        let final_rad = (rad + jitter_r).min(r_max);

        let bx = (cx as f64 + final_rad * final_angle.cos()) as i32;
        let by = (cy as f64 + final_rad * final_angle.sin()) as i32;

        // Glow
        let blip_col = if load > 85.0 {
            RED
        } else if load > 60.0 {
            YELLOW
        } else {
            GREEN
        };

        // Proximity to sweep determines brightness
        let angle_diff = {
            let d = (final_angle - sweep_angle).rem_euclid(2.0 * PI);
            d.min(2.0 * PI - d)
        };
        let glow = if angle_diff < trail_len {
            (1.0 - angle_diff / trail_len).powf(0.5)
        } else {
            0.0
        };

        if glow > 0.1 {
            for dx in -3i32..=3 {
                for dy in -3i32..=3 {
                    let dist2 = dx * dx + dy * dy;
                    if dist2 <= 9 {
                        let px2 = (bx + dx).clamp(0, 479) as u32;
                        let py2 = (by + dy).clamp(0, 479) as u32;
                        let bright = (glow * 255.0 * (1.0 - dist2 as f64 / 9.0)) as u8;
                        let existing = img.get_pixel(px2, py2);
                        img.put_pixel(
                            px2,
                            py2,
                            Rgb([
                                existing.0[0].saturating_add(blip_col.0[0] / 4 * bright / 64),
                                existing.0[1].saturating_add(blip_col.0[1] / 4 * bright / 64),
                                existing.0[2].saturating_add(blip_col.0[2] / 4 * bright / 64),
                            ]),
                        );
                    }
                }
            }
        }

        // Always draw a faint dot
        if (0..480).contains(&bx) && (0..480).contains(&by) {
            let faint = Rgb([blip_col.0[0] / 4, blip_col.0[1] / 4, blip_col.0[2] / 4]);
            img.put_pixel(bx as u32, by as u32, faint);
        }
    }

    // Center dot
    for dx in -3i32..=3 {
        for dy in -3i32..=3 {
            if dx * dx + dy * dy <= 9 {
                img.put_pixel((cx + dx) as u32, (cy + dy) as u32, GREEN);
            }
        }
    }

    let font = font_bold();
    // Ring labels
    for ri in 1..=5u32 {
        let pct = ri * 20;
        let label = format!("{pct}%");
        let lx = cx as f32 + (r_max * ri as f64 / 5.0 + 3.0) as f32;
        draw_text_mut(
            &mut img,
            OVERLAY0,
            lx as i32,
            cy + 2,
            PxScale::from(11.0),
            &font,
            &label,
        );
    }
    draw_text_mut(
        &mut img,
        GREEN,
        4,
        4,
        PxScale::from(14.0),
        &font,
        "CPU RADAR",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        320,
        4,
        PxScale::from(12.0),
        &font,
        &format!("{n_cpus} cores"),
    );

    DynamicImage::ImageRgb8(img)
}

// ─── System Health Dashboard ──────────────────────────────────────────────────

/// 5 large traffic-light indicators: CPU · RAM · Temp · Disk · Network.
/// Green = ok, yellow = warning, red = critical.
pub fn render_system_health() -> DynamicImage {
    use sysinfo::{Components, Disks};

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let disks = Disks::new_with_refreshed_list();
    let components = Components::new_with_refreshed_list();

    let font = font_bold();
    let small = font_mono();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // ── Helpers ──────────────────────────────────────────────────────────────
    let draw_circle = |img: &mut RgbImage, cx: i32, cy: i32, r: i32, col: Rgb<u8>| {
        let glow = Rgb([
            (col[0] as u32 * 5 / 10) as u8,
            (col[1] as u32 * 5 / 10) as u8,
            (col[2] as u32 * 5 / 10) as u8,
        ]);
        let r2 = r * r;
        let rg = r + 8;
        for dy in -(rg)..(rg) {
            for dx in -(rg)..(rg) {
                let d2 = dx * dx + dy * dy;
                if d2 > rg * rg {
                    continue;
                }
                let px = cx + dx;
                let py = cy + dy;
                if !(0..480).contains(&px) || !(0..480).contains(&py) {
                    continue;
                }
                if d2 <= r2 {
                    img.put_pixel(px as u32, py as u32, col);
                } else {
                    // Soft glow ring
                    let fr = (dx * dx + dy * dy) as f32;
                    let alpha = (1.0 - (fr.sqrt() - r as f32) / 8.0).max(0.0);
                    let existing = img.get_pixel(px as u32, py as u32);
                    img.put_pixel(
                        px as u32,
                        py as u32,
                        Rgb([
                            (existing[0] as f32 + glow[0] as f32 * alpha).min(255.0) as u8,
                            (existing[1] as f32 + glow[1] as f32 * alpha).min(255.0) as u8,
                            (existing[2] as f32 + glow[2] as f32 * alpha).min(255.0) as u8,
                        ]),
                    );
                }
            }
        }
    };

    let traffic = |pct: f32, warn: f32, crit: f32| -> Rgb<u8> {
        if pct >= crit {
            RED
        } else if pct >= warn {
            YELLOW
        } else {
            GREEN
        }
    };

    // ── Compute metrics ───────────────────────────────────────────────────────
    let n_cores = sys.cpus().len().max(1);
    let cpu_pct = sys.cpus().iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / n_cores as f32;

    let mem_used = sys.used_memory();
    let mem_total = sys.total_memory().max(1);
    let mem_pct = mem_used as f32 / mem_total as f32 * 100.0;

    let temp_max: f32 = components
        .iter()
        .filter_map(sysinfo::Component::temperature)
        .fold(0.0f32, f32::max);

    let (disk_pct, disk_used_gb, disk_total_gb) = {
        let (mut used, mut total) = (0u64, 0u64);
        for d in &disks {
            total += d.total_space();
            used += d.total_space() - d.available_space();
        }
        let pct = if total > 0 {
            used as f32 / total as f32 * 100.0
        } else {
            0.0
        };
        (
            pct,
            used as f32 / 1_073_741_824.0,
            total as f32 / 1_073_741_824.0,
        )
    };

    // ── Layout: 5 items in a 2-column + 1-column grid ─────────────────────────
    // Row 1: CPU (left) | RAM (right)
    // Row 2: Temp(left) | Disk (right)
    // Row 3: centred Network status
    let positions = [(120, 130), (360, 130), (120, 310), (360, 310), (240, 430)];
    let r = 55i32;

    // CPU
    let (cx, cy) = positions[0];
    draw_circle(&mut img, cx, cy, r, traffic(cpu_pct, 70.0, 90.0));
    let lbl = format!("{cpu_pct:.0}%");
    let lw = text_width(&font, PxScale::from(26.0), &lbl);
    draw_text_mut(
        &mut img,
        BG,
        cx - lw / 2,
        cy - 14,
        PxScale::from(26.0),
        &font,
        &lbl,
    );
    let tw = text_width(&small, PxScale::from(14.0), "CPU");
    draw_text_mut(
        &mut img,
        TEXT,
        cx - tw / 2,
        cy + 15,
        PxScale::from(14.0),
        &small,
        "CPU",
    );

    // RAM
    let (cx, cy) = positions[1];
    let ram_str = format!(
        "{:.1}/{:.1}G",
        mem_used as f32 / 1_073_741_824.0,
        mem_total as f32 / 1_073_741_824.0
    );
    draw_circle(&mut img, cx, cy, r, traffic(mem_pct, 70.0, 90.0));
    let lbl2 = format!("{mem_pct:.0}%");
    let lw2 = text_width(&font, PxScale::from(26.0), &lbl2);
    draw_text_mut(
        &mut img,
        BG,
        cx - lw2 / 2,
        cy - 18,
        PxScale::from(26.0),
        &font,
        &lbl2,
    );
    let rw = text_width(&small, PxScale::from(12.0), &ram_str);
    draw_text_mut(
        &mut img,
        BG,
        cx - rw / 2,
        cy + 4,
        PxScale::from(12.0),
        &small,
        &ram_str,
    );
    let tw2 = text_width(&small, PxScale::from(14.0), "RAM");
    draw_text_mut(
        &mut img,
        TEXT,
        cx - tw2 / 2,
        cy + 20,
        PxScale::from(14.0),
        &small,
        "RAM",
    );

    // Temp
    let (cx, cy) = positions[2];
    let temp_col = if temp_max <= 0.0 {
        OVERLAY0
    } else {
        traffic(temp_max, 70.0, 85.0)
    };
    draw_circle(&mut img, cx, cy, r, temp_col);
    let tlbl = if temp_max > 0.0 {
        format!("{temp_max:.0}°C")
    } else {
        "N/A".to_string()
    };
    let tlw = text_width(&font, PxScale::from(24.0), &tlbl);
    draw_text_mut(
        &mut img,
        BG,
        cx - tlw / 2,
        cy - 14,
        PxScale::from(24.0),
        &font,
        &tlbl,
    );
    let ttw = text_width(&small, PxScale::from(14.0), "Temp");
    draw_text_mut(
        &mut img,
        TEXT,
        cx - ttw / 2,
        cy + 15,
        PxScale::from(14.0),
        &small,
        "Temp",
    );

    // Disk
    let (cx, cy) = positions[3];
    draw_circle(&mut img, cx, cy, r, traffic(disk_pct, 80.0, 93.0));
    let dlbl = format!("{disk_pct:.0}%");
    let dlw = text_width(&font, PxScale::from(26.0), &dlbl);
    draw_text_mut(
        &mut img,
        BG,
        cx - dlw / 2,
        cy - 18,
        PxScale::from(26.0),
        &font,
        &dlbl,
    );
    let dsub = format!("{disk_used_gb:.0}/{disk_total_gb:.0}G");
    let dsw = text_width(&small, PxScale::from(12.0), &dsub);
    draw_text_mut(
        &mut img,
        BG,
        cx - dsw / 2,
        cy + 3,
        PxScale::from(12.0),
        &small,
        &dsub,
    );
    let dtw = text_width(&small, PxScale::from(14.0), "Disk");
    draw_text_mut(
        &mut img,
        TEXT,
        cx - dtw / 2,
        cy + 20,
        PxScale::from(14.0),
        &small,
        "Disk",
    );

    // Load average (replaces "network" — always available)
    let (cx, cy) = positions[4];
    let uptime = System::uptime();
    let load1 = System::load_average().one as f32;
    let load_col = traffic(load1 / n_cores as f32 * 100.0, 70.0, 90.0);
    draw_circle(&mut img, cx, cy, r - 10, load_col);
    let lload = format!("{load1:.2}");
    let llw = text_width(&font, PxScale::from(22.0), &lload);
    draw_text_mut(
        &mut img,
        BG,
        cx - llw / 2,
        cy - 12,
        PxScale::from(22.0),
        &font,
        &lload,
    );
    let ltw = text_width(&small, PxScale::from(12.0), "Load");
    draw_text_mut(
        &mut img,
        TEXT,
        cx - ltw / 2,
        cy + 10,
        PxScale::from(12.0),
        &small,
        "Load",
    );

    // Title + uptime
    draw_text_mut(
        &mut img,
        TEXT,
        8,
        6,
        PxScale::from(20.0),
        &font,
        "System Health",
    );
    let up_str = format!("up {}d {:02}h", uptime / 86400, (uptime % 86400) / 3600);
    let uw = text_width(&small, PxScale::from(14.0), &up_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - uw - 6,
        10,
        PxScale::from(14.0),
        &small,
        &up_str,
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod status_overview_tests {
    use super::*;

    #[test]
    fn test_render_status_overview_size() {
        let img = render_status_overview();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}
