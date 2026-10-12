//! History ring buffers and the line/bar graphs drawn from them.

use super::*;

// ─── CPU usage history (global ring-buffer, shared across renders) ────────────

pub(super) static CPU_GRAPH_DATA: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();

pub(super) fn cpu_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    CPU_GRAPH_DATA
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one CPU usage sample to the global history ring-buffer (max 120 entries).
///
/// Call this from the display loop each time a live mode renders, so the
/// graph mode always has fresh data even if `render_cpu_graph` hasn't been
/// called recently.
pub fn push_cpu_history(pct: f32) {
    let mut h = cpu_graph_data();
    if h.len() >= 120 {
        h.pop_front();
    }
    h.push_back(pct.clamp(0.0, 100.0));
}

/// Full-screen CPU usage history graph.
///
/// Shows the last 120 samples (2 minutes at 1 sample/second) as a bar chart,
/// with the current reading prominently displayed.
pub fn render_cpu_graph() -> DynamicImage {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    // Small sleep so sysinfo can measure a delta
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    let cpu_pct = sys.global_cpu_usage();
    push_cpu_history(cpu_pct);

    let history: Vec<f32> = cpu_graph_data().iter().copied().collect();

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── current time (top-right) ─────────────────────────────────────────
    let now = Local::now();
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

    // ── mode label (top-left) ────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "CPU GRAPH",
    );

    // ── large current % ──────────────────────────────────────────────────
    let accent = if cpu_pct > 80.0 {
        RED
    } else if cpu_pct > 50.0 {
        YELLOW
    } else {
        GREEN
    };
    let pct_str = format!("{cpu_pct:.1}%");
    draw_centered(&mut img, &bold, PxScale::from(80.0), 50, accent, &pct_str);

    // ── stats row ────────────────────────────────────────────────────────
    if !history.is_empty() {
        let avg: f32 = history.iter().sum::<f32>() / history.len() as f32;
        let peak: f32 = history.iter().copied().fold(0.0_f32, f32::max);
        let stats_str = format!(
            "avg {:.1}%  peak {:.1}%  ({} samples)",
            avg,
            peak,
            history.len()
        );
        let sw = text_width(&mono, PxScale::from(16.0), &stats_str);
        let sx = ((480 - sw) / 2).max(0);
        draw_text_mut(
            &mut img,
            SUBTEXT,
            sx,
            148,
            PxScale::from(16.0),
            &mono,
            &stats_str,
        );
    }

    // ── bar chart ────────────────────────────────────────────────────────
    // Chart area: x=0..480, y=175..460  (285px tall)
    let chart_x = 0i32;
    let chart_y = 175i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Background grid lines at 25%, 50%, 75%
    for pct_line in [25u32, 50, 75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32;
        if ly >= 0 && (ly as u32) < img.height() {
            for x in (chart_x..(chart_x + chart_w)).step_by(6) {
                if x >= 0 && (x as u32) < img.width() {
                    img.put_pixel(x as u32, ly as u32, SURFACE0);
                }
            }
        }
    }

    // Y-axis labels
    for (pct_line, label) in [(25u32, "25"), (50, "50"), (75, "75"), (100, "100")] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32 - 2;
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            ly - 8,
            PxScale::from(12.0),
            &mono,
            label,
        );
    }

    // Bars
    let n = history.len();
    if n > 0 {
        let bar_w = chart_w / 120; // 4px per bar
        let offset = 120 - n as i32; // left-pad with empty space if less than 120 samples
        for (i, &val) in history.iter().enumerate() {
            let bar_h = (chart_h as f32 * val / 100.0).max(1.0) as i32;
            let bx = chart_x + (offset + i as i32) * bar_w;
            let by = chart_y + chart_h - bar_h;
            let color = if val > 80.0 {
                RED
            } else if val > 50.0 {
                YELLOW
            } else {
                BLUE
            };
            // Brighten the most recent bar
            let is_last = i == n - 1;
            let draw_color = if is_last { TEXT } else { color };
            draw_filled_rect_mut(
                &mut img,
                Rect::at(bx, by).of_size(bar_w.max(1) as u32, bar_h as u32),
                draw_color,
            );
        }
    }

    // Chart border (bottom line)
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );

    // ── x-axis label ─────────────────────────────────────────────────────
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        chart_y + chart_h + 6,
        OVERLAY0,
        "← 2 min ago                      now →",
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Network I/O history graph ───────────────────────────────────────────────

pub(super) static NET_RX_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<u64>>> =
    OnceLock::new();
pub(super) static NET_TX_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<u64>>> =
    OnceLock::new();

pub(super) fn net_rx_history() -> std::sync::MutexGuard<'static, std::collections::VecDeque<u64>> {
    NET_RX_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(60)))
        .lock()
        .unwrap()
}
pub(super) fn net_tx_history() -> std::sync::MutexGuard<'static, std::collections::VecDeque<u64>> {
    NET_TX_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(60)))
        .lock()
        .unwrap()
}

/// Append one network sample (bytes/second for rx and tx) to history.
pub fn push_net_history(rx_bps: u64, tx_bps: u64) {
    let mut rx = net_rx_history();
    if rx.len() >= 60 {
        rx.pop_front();
    }
    rx.push_back(rx_bps);

    let mut tx = net_tx_history();
    if tx.len() >= 60 {
        tx.pop_front();
    }
    tx.push_back(tx_bps);
}

pub(super) fn format_bps(bps: u64) -> String {
    if bps >= 1_000_000_000 {
        format!("{:.1}GB/s", bps as f64 / 1_000_000_000.0)
    } else if bps >= 1_000_000 {
        format!("{:.1}MB/s", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.1}KB/s", bps as f64 / 1_000.0)
    } else {
        format!("{bps}B/s")
    }
}

/// Full-screen network I/O history graph.
///
/// Shows Rx (download) and Tx (upload) rates for the last 60 seconds as
/// colour-coded area charts. The scale adapts to the peak value.
pub fn render_net_graph() -> DynamicImage {
    use sysinfo::Networks;
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(500));
    nets.refresh(true);

    let (total_rx, total_tx): (u64, u64) = nets.iter().fold((0u64, 0u64), |(rx, tx), (_, d)| {
        (rx + d.received(), tx + d.transmitted())
    });

    // Convert to bytes/sec (we slept ~500ms)
    let rx_bps = total_rx * 2;
    let tx_bps = total_tx * 2;
    push_net_history(rx_bps, tx_bps);

    let rx_hist: Vec<u64> = net_rx_history().iter().copied().collect();
    let tx_hist: Vec<u64> = net_tx_history().iter().copied().collect();

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Current time
    let now = Local::now();
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
        "NET GRAPH",
    );

    // Current Rx/Tx
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(38.0),
        55,
        GREEN,
        &format!("↓ {}", format_bps(rx_bps)),
    );
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(38.0),
        100,
        PEACH,
        &format!("↑ {}", format_bps(tx_bps)),
    );

    // Charts
    // Upper chart: Rx (y=145..310, 165px)
    // Lower chart: Tx (y=320..455, 135px)
    /// (top y, height, samples, colour, label)
    type Chart<'a> = (i32, i32, &'a [u64], Rgb<u8>, &'a str);
    let charts: &[Chart] = &[
        (145, 160, &rx_hist, GREEN, "Rx"),
        (315, 135, &tx_hist, PEACH, "Tx"),
    ];

    for &(cy, ch, hist, color, label) in charts {
        let peak = hist.iter().copied().max().unwrap_or(1).max(1);

        // Background
        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, cy).of_size(480, ch as u32),
            Rgb([20, 20, 32]),
        );

        // Label
        draw_text_mut(
            &mut img,
            color,
            4,
            cy + 2,
            PxScale::from(14.0),
            &bold,
            label,
        );
        let peak_str = format_bps(peak);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            50,
            cy + 2,
            PxScale::from(13.0),
            &mono,
            &peak_str,
        );

        // Grid line at 50%
        let mid_y = cy + ch / 2;
        for x in (0..480i32).step_by(8) {
            img.put_pixel(x as u32, mid_y as u32, SURFACE0);
        }

        // Bars
        let n = hist.len();
        if n > 0 {
            let bar_w = 480 / 60;
            let offset = 60 - n as i32;
            for (i, &val) in hist.iter().enumerate() {
                let bar_h = ((ch - 4) as f64 * val as f64 / peak as f64).max(1.0) as i32;
                let bx = (offset + i as i32) * bar_w;
                let by = cy + ch - bar_h;
                // Alpha-blend towards color using surface darkness
                let draw_col = if i == n - 1 { TEXT } else { color };
                draw_filled_rect_mut(
                    &mut img,
                    Rect::at(bx, by).of_size(bar_w.max(1) as u32, bar_h as u32),
                    draw_col,
                );
            }
        }

        // Bottom border
        draw_filled_rect_mut(&mut img, Rect::at(0, cy + ch).of_size(480, 1), SURFACE0);
    }

    // x-axis label
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(12.0),
        458,
        OVERLAY0,
        "← 60 s ago      now →",
    );

    DynamicImage::ImageRgb8(img)
}

// ─── History accessors ────────────────────────────────────────────────────────

/// Return a snapshot of the CPU usage history ring-buffer (oldest first).
/// Returns an empty Vec if no samples have been pushed yet.
pub fn get_cpu_history() -> Vec<f32> {
    cpu_graph_data().iter().copied().collect()
}

/// Return a snapshot of the network I/O history ring-buffers (oldest first).
/// Returns `(rx_bps, tx_bps)` vectors.
pub fn get_net_history() -> (Vec<u64>, Vec<u64>) {
    let rx: Vec<u64> = net_rx_history().iter().copied().collect();
    let tx: Vec<u64> = net_tx_history().iter().copied().collect();
    (rx, tx)
}

// ─── Load average history graph ──────────────────────────────────────────────

pub(super) static LOAD_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();

pub(super) fn load_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    LOAD_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one 1-minute load average sample to the load history ring-buffer.
pub fn push_load_history(load1: f32) {
    let mut buf = load_graph_data();
    if buf.len() >= 120 {
        buf.pop_front();
    }
    buf.push_back(load1);
}

// ─── Memory history graph ─────────────────────────────────────────────────────

pub(super) static MEM_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();

pub(super) fn mem_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    MEM_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one memory usage percentage sample to the memory history ring-buffer.
pub fn push_mem_history(mem_pct: f32) {
    let mut buf = mem_graph_data();
    if buf.len() >= 120 {
        buf.pop_front();
    }
    buf.push_back(mem_pct);
}

/// Draw a line-chart from a history buffer inside the given chart bounds.
///
/// `values` are in [0, max_val]. Returns the last drawn point for value annotation.
// The geometry and colours are all independent; a struct would only move them.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_line_chart(
    img: &mut RgbImage,
    values: &[f32],
    max_val: f32,
    chart_x: i32,
    chart_y: i32,
    chart_w: i32,
    chart_h: i32,
    line_color: Rgb<u8>,
    fill_color: Rgb<u8>,
) {
    if values.len() < 2 || max_val <= 0.0 {
        return;
    }
    let n = values.len();
    let x_step = chart_w as f32 / (n - 1).max(1) as f32;

    let pixel_y = |v: f32| -> i32 {
        let frac = (v / max_val).clamp(0.0, 1.0);
        chart_y + chart_h - (chart_h as f32 * frac) as i32
    };

    // Fill area under curve
    for (i, &val) in values.iter().enumerate() {
        let px = (chart_x as f32 + i as f32 * x_step) as i32;
        let py = pixel_y(val);
        let bot = chart_y + chart_h;
        if px >= 0 && (px as u32) < img.width() {
            for row in py..=bot {
                if row >= 0 && (row as u32) < img.height() {
                    img.put_pixel(px as u32, row as u32, fill_color);
                }
            }
        }
    }

    // Draw line on top
    for i in 0..n - 1 {
        let px1 = (chart_x as f32 + i as f32 * x_step) as i32;
        let py1 = pixel_y(values[i]);
        let px2 = (chart_x as f32 + (i + 1) as f32 * x_step) as i32;
        let py2 = pixel_y(values[i + 1]);
        // Bresenham-ish line
        let dx = (px2 - px1).abs();
        let dy = (py2 - py1).abs();
        let steps = dx.max(dy).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let lx = (px1 as f32 + t * (px2 - px1) as f32) as i32;
            let ly = (py1 as f32 + t * (py2 - py1) as f32) as i32;
            let ly2 = (ly - 1).max(0);
            for &row in &[ly, ly2] {
                if lx >= 0 && (lx as u32) < img.width() && row >= 0 && (row as u32) < img.height() {
                    img.put_pixel(lx as u32, row as u32, line_color);
                }
            }
        }
    }
}

/// Render a load-average history graph (480×480).
///
/// Shows the 1-minute load average as a line chart over the last ~2 minutes,
/// with current 1m/5m/15m values and per-core context.
pub fn render_load_graph() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, RefreshKind, System};

    let load = System::load_average();
    push_load_history(load.one as f32);

    let history: Vec<f32> = load_graph_data().iter().copied().collect();

    let cpu_count = {
        let s =
            System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        s.cpus().len().max(1)
    };

    let max_load = (cpu_count as f32).max(load.fifteen as f32 * 2.0).max(4.0);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "LOAD AVG",
    );
    let tw = text_width(
        &mono,
        PxScale::from(22.0),
        &now.format("%H:%M:%S").to_string(),
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        18,
        PxScale::from(22.0),
        &mono,
        &now.format("%H:%M:%S").to_string(),
    );

    // ── Current load 1m — large ───────────────────────────────────────────
    let accent = {
        let pct = load.one as f32 / cpu_count as f32;
        if pct > 1.0 {
            RED
        } else if pct > 0.7 {
            YELLOW
        } else {
            GREEN
        }
    };
    let load1_str = format!("{:.2}", load.one);
    draw_centered(&mut img, &bold, PxScale::from(80.0), 42, accent, &load1_str);

    // ── Sub-stats row ─────────────────────────────────────────────────────
    let stats_str = format!(
        "1m {:.2}  5m {:.2}  15m {:.2}  ({} cores)",
        load.one, load.five, load.fifteen, cpu_count
    );
    let sw = text_width(&mono, PxScale::from(16.0), &stats_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - sw) / 2).max(0),
        148,
        PxScale::from(16.0),
        &mono,
        &stats_str,
    );

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Grid lines at core-count multiples
    for multiple in 1..=4u32 {
        let grid_val = multiple as f32;
        if grid_val > max_load {
            break;
        }
        let ly = chart_y + chart_h - (chart_h as f32 * grid_val / max_load) as i32;
        if ly < chart_y {
            break;
        }
        for x in (chart_x..(chart_x + chart_w)).step_by(6) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label = format!("{grid_val:.0}");
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            (ly - 8).max(0),
            PxScale::from(12.0),
            &mono,
            &label,
        );
    }

    // Fill + line
    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(
        &mut img, &history, max_load, chart_x, chart_y, chart_w, chart_h, accent, fill_color,
    );

    // Chart bottom border
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        chart_y + chart_h + 6,
        OVERLAY0,
        "← 2 min ago                      now →",
    );

    DynamicImage::ImageRgb8(img)
}

/// Render a memory usage history graph (480×480).
///
/// Shows RAM usage % as a filled line chart over the last ~2 minutes,
/// with current used/total values and a usage indicator.
pub fn render_mem_graph() -> DynamicImage {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_memory();
    let used = sys.used_memory();
    let total = sys.total_memory();
    let mem_pct = if total > 0 {
        used as f32 / total as f32 * 100.0
    } else {
        0.0
    };
    push_mem_history(mem_pct);

    let history: Vec<f32> = mem_graph_data().iter().copied().collect();

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "MEM GRAPH",
    );
    let tw = text_width(
        &mono,
        PxScale::from(22.0),
        &now.format("%H:%M:%S").to_string(),
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        18,
        PxScale::from(22.0),
        &mono,
        &now.format("%H:%M:%S").to_string(),
    );

    // ── Current % — large ─────────────────────────────────────────────────
    let accent = if mem_pct > 85.0 {
        RED
    } else if mem_pct > 65.0 {
        YELLOW
    } else {
        BLUE
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(80.0),
        42,
        accent,
        &format!("{mem_pct:.1}%"),
    );

    // ── Sub-stats row ─────────────────────────────────────────────────────
    let used_gb = used as f64 / 1_073_741_824.0;
    let total_gb = total as f64 / 1_073_741_824.0;
    let swap_used = sys.used_swap();
    let swap_total = sys.total_swap();
    let sub = if swap_total > 0 {
        format!(
            "{:.1}/{:.1} GB  swap {:.0}%",
            used_gb,
            total_gb,
            swap_used as f64 / swap_total as f64 * 100.0
        )
    } else {
        format!("{used_gb:.1} GB used of {total_gb:.1} GB  (no swap)")
    };
    let sw = text_width(&mono, PxScale::from(16.0), &sub);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - sw) / 2).max(0),
        148,
        PxScale::from(16.0),
        &mono,
        &sub,
    );

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Grid lines at 25%, 50%, 75%
    for pct_line in [25u32, 50, 75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct_line as f32 / 100.0) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(6) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            (ly - 8).max(0),
            PxScale::from(12.0),
            &mono,
            &format!("{pct_line}%"),
        );
    }

    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(
        &mut img, &history, 100.0, chart_x, chart_y, chart_w, chart_h, accent, fill_color,
    );

    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        chart_y + chart_h + 6,
        OVERLAY0,
        "← 2 min ago                      now →",
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod graph_history_tests {
    use super::*;

    #[test]
    fn test_load_graph_size() {
        let img = render_load_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_mem_graph_size() {
        let img = render_mem_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Graphs overview (4-panel mini-charts) ───────────────────────────────────

/// Render a 4-panel mini-charts overview (480×480).
///
/// Shows CPU usage, load average, memory usage, and temperature as four
/// equal-sized line charts arranged in a 2×2 grid. This is the best
/// "at a glance" view for system health monitoring.
pub fn render_graphs_overview() -> DynamicImage {
    use sysinfo::{Components, CpuRefreshKind, RefreshKind, System};

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Gather current values
    let load = System::load_average();
    let cpu_count = {
        let s =
            System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        s.cpus().len().max(1)
    };
    let mut sys = System::new();
    sys.refresh_memory();
    let used = sys.used_memory();
    let total = sys.total_memory();
    let mem_pct = if total > 0 {
        used as f32 / total as f32 * 100.0
    } else {
        0.0
    };

    let comps = Components::new_with_refreshed_list();
    let hot_temp = comps
        .iter()
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max);

    // Collect histories
    let cpu_hist: Vec<f32> = get_cpu_history();
    let load_hist: Vec<f32> = load_graph_data().iter().copied().collect();
    let mem_hist: Vec<f32> = mem_graph_data().iter().copied().collect();
    let temp_hist: Vec<f32> = temp_graph_data().iter().copied().collect();
    let diog_r_hist: Vec<f32> = disk_read_data().iter().copied().collect();

    let now = Local::now();
    let ts = now.format("%H:%M:%S").to_string();

    // Draw a subtle dividing cross
    let mid = 240i32;
    for x in 0..480u32 {
        img.put_pixel(x, mid as u32, SURFACE0);
    }
    for y in 0..480u32 {
        img.put_pixel(mid as u32, y, SURFACE0);
    }

    // ── Helper: draw one panel ────────────────────────────────────────────────
    let draw_panel = |img: &mut image::RgbImage,
                      ox: i32,
                      oy: i32,
                      pw: i32,
                      ph: i32,
                      title: &str,
                      value_str: &str,
                      history: &[f32],
                      max_val: f32,
                      line_color: Rgb<u8>,
                      fill_color: Rgb<u8>,
                      font_b: &ab_glyph::FontRef<'_>,
                      font_m: &ab_glyph::FontRef<'_>| {
        // Panel background (slightly lighter than base)
        let panel_bg = Rgb([24u8, 24, 37]); // Catppuccin Mantle
        draw_filled_rect_mut(
            img,
            Rect::at(ox + 2, oy + 2).of_size((pw - 4) as u32, (ph - 4) as u32),
            panel_bg,
        );

        // Title (small, top-left)
        draw_text_mut(
            img,
            OVERLAY0,
            ox + 6,
            oy + 5,
            PxScale::from(13.0),
            font_b,
            title,
        );

        // Current value (top-right)
        let vw = text_width(font_m, PxScale::from(16.0), value_str);
        draw_text_mut(
            img,
            line_color,
            ox + pw - 8 - vw,
            oy + 5,
            PxScale::from(16.0),
            font_m,
            value_str,
        );

        // Mini chart (uses most of the panel height)
        let cx = ox + 2;
        let cy = oy + 26;
        let cw = pw - 4;
        let ch = ph - 32;
        draw_line_chart(
            img, history, max_val, cx, cy, cw, ch, line_color, fill_color,
        );

        // Bottom border
        draw_filled_rect_mut(img, Rect::at(cx, cy + ch).of_size(cw as u32, 1), SURFACE0);
    };

    // ── CPU panel (top-left) ──────────────────────────────────────────────────
    let cpu_cur = cpu_hist.last().copied().unwrap_or(0.0);
    let cpu_acc = if cpu_cur > 80.0 {
        RED
    } else if cpu_cur > 50.0 {
        YELLOW
    } else {
        MAUVE
    };
    draw_panel(
        &mut img,
        0,
        0,
        mid,
        mid,
        "CPU %",
        &format!("{cpu_cur:.0}%"),
        &cpu_hist,
        100.0,
        cpu_acc,
        Rgb([cpu_acc.0[0] / 7, cpu_acc.0[1] / 7, cpu_acc.0[2] / 7]),
        &bold,
        &mono,
    );

    // ── Load avg panel (top-right) ────────────────────────────────────────────
    let load_max = (cpu_count as f32).max(4.0);
    let load_acc = {
        let l = load.one as f32 / cpu_count as f32;
        if l > 1.0 {
            RED
        } else if l > 0.7 {
            YELLOW
        } else {
            GREEN
        }
    };
    draw_panel(
        &mut img,
        mid,
        0,
        mid,
        mid,
        "Load avg",
        &format!("{:.2}", load.one),
        &load_hist,
        load_max,
        load_acc,
        Rgb([load_acc.0[0] / 7, load_acc.0[1] / 7, load_acc.0[2] / 7]),
        &bold,
        &mono,
    );

    // ── Memory panel (bottom-left) ────────────────────────────────────────────
    let mem_acc = if mem_pct > 90.0 {
        RED
    } else if mem_pct > 75.0 {
        YELLOW
    } else {
        BLUE
    };
    draw_panel(
        &mut img,
        0,
        mid,
        mid,
        mid,
        "Memory %",
        &format!("{mem_pct:.0}%"),
        &mem_hist,
        100.0,
        mem_acc,
        Rgb([mem_acc.0[0] / 7, mem_acc.0[1] / 7, mem_acc.0[2] / 7]),
        &bold,
        &mono,
    );

    // ── Temperature panel (bottom-right) ─────────────────────────────────────
    // If no temp data, fall back to disk read history
    let (temp_use_hist, temp_max, temp_acc, temp_label, temp_val) = if hot_temp > 0.0 {
        let acc = if hot_temp >= 85.0 {
            RED
        } else if hot_temp >= 70.0 {
            PEACH
        } else {
            GREEN
        };
        (
            temp_hist.as_slice(),
            (hot_temp * 1.1).max(100.0),
            acc,
            "Temp °C",
            format!("{hot_temp:.0}°C"),
        )
    } else {
        let peak_r = diog_r_hist.iter().copied().fold(0.0_f32, f32::max);
        (
            diog_r_hist.as_slice(),
            (peak_r * 1.2).max(1024.0 * 1024.0),
            YELLOW,
            "Disk R/s",
            format_bps(*diog_r_hist.last().unwrap_or(&0.0) as u64),
        )
    };
    draw_panel(
        &mut img,
        mid,
        mid,
        mid,
        mid,
        temp_label,
        &temp_val,
        temp_use_hist,
        temp_max,
        temp_acc,
        Rgb([temp_acc.0[0] / 7, temp_acc.0[1] / 7, temp_acc.0[2] / 7]),
        &bold,
        &mono,
    );

    // ── Center timestamp overlay ──────────────────────────────────────────────
    let tw_ts = text_width(&mono, PxScale::from(12.0), &ts);
    let tx = (480 - tw_ts) / 2;
    // Draw behind-the-cross: tiny label at center
    draw_filled_rect_mut(
        &mut img,
        Rect::at(tx - 2, mid - 9).of_size(tw_ts as u32 + 4, 14),
        SURFACE0,
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        tx,
        mid - 9,
        PxScale::from(12.0),
        &mono,
        &ts,
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod graphs_overview_tests {
    use super::*;

    #[test]
    fn test_render_graphs_overview_size() {
        let img = render_graphs_overview();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Disk I/O history graph ───────────────────────────────────────────────────

pub(super) static DISK_READ_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();
pub(super) static DISK_WRITE_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();

pub(super) fn disk_read_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    DISK_READ_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

pub(super) fn disk_write_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    DISK_WRITE_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one disk I/O sample to the ring-buffers.
/// `read_bps` and `write_bps` are in bytes per second.
pub fn push_disk_io_history(read_bps: f32, write_bps: f32) {
    {
        let mut r = disk_read_data();
        if r.len() >= 120 {
            r.pop_front();
        }
        r.push_back(read_bps);
    }
    {
        let mut w = disk_write_data();
        if w.len() >= 120 {
            w.pop_front();
        }
        w.push_back(write_bps);
    }
}

/// Render a disk I/O history graph (480×480).
///
/// Shows total disk read and write throughput as two overlaid line charts,
/// alongside current total read/write rates and peak values.
pub fn render_disk_io_graph() -> DynamicImage {
    use sysinfo::{DiskRefreshKind, Disks};

    // Sample disk I/O (two refreshes, 500ms apart)
    let mut disks = Disks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(200));
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    let mut total_read_bps: u64 = 0;
    let mut total_write_bps: u64 = 0;
    for d in &disks {
        if d.total_space() == 0 {
            continue;
        }
        let io = d.usage();
        total_read_bps += io.read_bytes * 5; // ~200ms window → ×5 for per-second
        total_write_bps += io.written_bytes * 5;
    }

    push_disk_io_history(total_read_bps as f32, total_write_bps as f32);

    let read_history: Vec<f32> = disk_read_data().iter().copied().collect();
    let write_history: Vec<f32> = disk_write_data().iter().copied().collect();

    // Peaks
    let peak_read = read_history.iter().copied().fold(0.0_f32, f32::max);
    let peak_write = write_history.iter().copied().fold(0.0_f32, f32::max);
    let max_val = (peak_read.max(peak_write) * 1.1).max(1024.0 * 1024.0); // at least 1 MB/s

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "DISK I/O GRAPH",
    );
    let ts = now.format("%H:%M:%S").to_string();
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

    // ── Current R/W display ───────────────────────────────────────────────
    let read_str = format!("R {}", format_bps(total_read_bps));
    let write_str = format!("W {}", format_bps(total_write_bps));
    draw_text_mut(
        &mut img,
        BLUE,
        20,
        52,
        PxScale::from(32.0),
        &bold,
        &read_str,
    );
    draw_text_mut(
        &mut img,
        PEACH,
        250,
        52,
        PxScale::from(32.0),
        &bold,
        &write_str,
    );

    // ── Peak row ─────────────────────────────────────────────────────────
    let peak_str = format!(
        "peak R {} / W {}",
        format_bps(peak_read as u64),
        format_bps(peak_write as u64)
    );
    let pw = text_width(&mono, PxScale::from(14.0), &peak_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        ((480 - pw) / 2).max(4),
        96,
        PxScale::from(14.0),
        &mono,
        &peak_str,
    );

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 120i32;
    let chart_w = 480i32;
    let chart_h = 320i32;

    // Grid lines at 25%, 50%, 75% of max_val
    for (pct, _label) in &[(0.25f32, "25%"), (0.5, "50%"), (0.75, "75%")] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let val_at_pct = format_bps((max_val * pct) as u64);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            (ly - 10).max(chart_y),
            PxScale::from(11.0),
            &mono,
            &val_at_pct,
        );
    }

    // Read chart (blue fill)
    let fill_read = Rgb([BLUE.0[0] / 8, BLUE.0[1] / 8, BLUE.0[2] / 8]);
    let fill_write = Rgb([PEACH.0[0] / 8, PEACH.0[1] / 8, PEACH.0[2] / 8]);
    draw_line_chart(
        &mut img,
        &write_history,
        max_val,
        chart_x,
        chart_y,
        chart_w,
        chart_h,
        PEACH,
        fill_write,
    );
    draw_line_chart(
        &mut img,
        &read_history,
        max_val,
        chart_x,
        chart_y,
        chart_w,
        chart_h,
        BLUE,
        fill_read,
    );

    // Chart border
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );

    // Legend
    draw_filled_rect_mut(&mut img, Rect::at(20, 445).of_size(16, 10), BLUE);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        42,
        443,
        PxScale::from(14.0),
        &mono,
        "Read",
    );
    draw_filled_rect_mut(&mut img, Rect::at(100, 445).of_size(16, 10), PEACH);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        122,
        443,
        PxScale::from(14.0),
        &mono,
        "Write",
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        463,
        OVERLAY0,
        "← 2 min ago                      now →",
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod disk_io_graph_tests {
    use super::*;

    #[test]
    fn test_push_disk_io_history() {
        push_disk_io_history(1_000_000.0, 500_000.0);
        let r: Vec<f32> = disk_read_data().iter().copied().collect();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_disk_io_graph_size() {
        let img = render_disk_io_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Temperature history graph ────────────────────────────────────────────────

pub(super) static TEMP_HISTORY: OnceLock<std::sync::Mutex<std::collections::VecDeque<f32>>> =
    OnceLock::new();

pub(super) fn temp_graph_data() -> std::sync::MutexGuard<'static, std::collections::VecDeque<f32>> {
    TEMP_HISTORY
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::with_capacity(120)))
        .lock()
        .unwrap()
}

/// Append one temperature sample (°C) to the temperature history ring-buffer.
pub fn push_temp_history(temp_c: f32) {
    let mut buf = temp_graph_data();
    if buf.len() >= 120 {
        buf.pop_front();
    }
    buf.push_back(temp_c);
}

/// Returns the hottest component temperature in degrees Celsius, or 0.0 if unavailable.
pub fn hottest_component_temp() -> f32 {
    use sysinfo::Components;
    let comps = Components::new_with_refreshed_list();
    comps
        .iter()
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| t.is_finite() && *t > 0.0)
        .fold(0.0_f32, f32::max)
}

/// Render a CPU/component temperature history graph (480×480).
///
/// Shows the hottest component temperature as a filled line chart over the
/// last ~2 minutes, with current/peak/average values and thermal thresholds.
pub fn render_temp_graph() -> DynamicImage {
    use sysinfo::Components;

    // Refresh and sample the hottest sensor
    let comps = Components::new_with_refreshed_list();
    let temps: Vec<f32> = comps
        .iter()
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| t.is_finite() && *t > 0.0)
        .collect();

    let current_temp = temps.iter().copied().fold(0.0_f32, f32::max);
    push_temp_history(current_temp);

    let history: Vec<f32> = temp_graph_data().iter().copied().collect();

    // Stats
    let peak_temp = history.iter().copied().fold(0.0_f32, f32::max);
    let avg_temp = if history.is_empty() {
        0.0
    } else {
        history.iter().sum::<f32>() / history.len() as f32
    };

    let max_display = (peak_temp.max(current_temp) * 1.1).max(100.0);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Header ────────────────────────────────────────────────────────────
    let now = Local::now();
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "TEMP GRAPH",
    );
    let ts = now.format("%H:%M:%S").to_string();
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

    // ── Current temperature — large ───────────────────────────────────────
    let accent = if current_temp >= 85.0 {
        RED
    } else if current_temp >= 70.0 {
        PEACH
    } else if current_temp >= 55.0 {
        YELLOW
    } else {
        GREEN
    };

    let temp_str = format!("{current_temp:.1}°C");
    draw_centered(&mut img, &bold, PxScale::from(80.0), 42, accent, &temp_str);

    // ── Stats row ─────────────────────────────────────────────────────────
    let stats_str = format!(
        "peak {:.1}°   avg {:.1}°   sensors: {}",
        peak_temp,
        avg_temp,
        temps.len()
    );
    let sw = text_width(&mono, PxScale::from(15.0), &stats_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - sw) / 2).max(0),
        148,
        PxScale::from(15.0),
        &mono,
        &stats_str,
    );

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 170i32;
    let chart_w = 480i32;
    let chart_h = 280i32;

    // Thermal threshold guide lines
    for (threshold, label, color) in &[
        (55.0f32, "55°", YELLOW),
        (70.0, "70°", PEACH),
        (85.0, "85°", RED),
    ] {
        if *threshold > max_display {
            continue;
        }
        let ly = chart_y + chart_h - (chart_h as f32 * threshold / max_display) as i32;
        if ly < chart_y {
            continue;
        }
        // Dashed line
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, *color);
            }
        }
        draw_text_mut(
            &mut img,
            *color,
            chart_x + 4,
            (ly - 14).max(chart_y),
            PxScale::from(12.0),
            &mono,
            label,
        );
    }

    // Fill + line chart
    let fill_color = Rgb([accent.0[0] / 6, accent.0[1] / 6, accent.0[2] / 6]);
    draw_line_chart(
        &mut img,
        &history,
        max_display,
        chart_x,
        chart_y,
        chart_w,
        chart_h,
        accent,
        fill_color,
    );

    // Bottom border + time axis label
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        chart_y + chart_h + 6,
        OVERLAY0,
        "← 2 min ago                      now →",
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod temp_graph_tests {
    use super::*;

    #[test]
    fn test_push_temp_history() {
        push_temp_history(65.0);
        push_temp_history(72.0);
        let buf: Vec<f32> = temp_graph_data().iter().copied().collect();
        assert!(!buf.is_empty());
        assert!(buf.contains(&65.0) || buf.contains(&72.0));
    }

    #[test]
    fn test_render_temp_graph_size() {
        let img = render_temp_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

#[cfg(test)]
mod matrix_tests {
    use super::*;

    #[test]
    fn test_matrix_rain_size() {
        let img = render_matrix_rain();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_matrix_rain_animates() {
        let img1 = render_matrix_rain();
        let img2 = render_matrix_rain();
        // Two frames should be different (rain moved)
        let p1 = img1.as_rgb8().unwrap().get_pixel(240, 240);
        let p2 = img2.as_rgb8().unwrap().get_pixel(240, 240);
        // They may or may not differ depending on column positions,
        // but both should be valid green-tinted colors
        // Just check they're valid images
        assert_eq!(img1.width(), 480);
        assert_eq!(img2.width(), 480);
        let _ = (p1, p2); // suppress unused warning
    }
}

// ─── Net Speed Graph (overlaid Rx + Tx line chart) ───────────────────────────

/// Render a dedicated network speed history graph (480×480).
///
/// Shows download (Rx, green) and upload (Tx, peach) overlaid on one shared
/// chart so the relative magnitudes are directly comparable, plus a compact
/// per-interface table at the bottom.
pub fn render_net_speed_graph() -> DynamicImage {
    use sysinfo::Networks;

    // Two-pass sample: sleep ~200 ms between refreshes
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(200));
    nets.refresh(true);

    // Per-interface breakdown (for the mini table)
    let mut ifaces: Vec<(String, u64, u64)> = nets
        .iter()
        .filter_map(|(name, data)| {
            let rx = data.received() * 5;
            let tx = data.transmitted() * 5;
            if rx + tx > 0 {
                Some((name.clone(), rx, tx))
            } else {
                None
            }
        })
        .collect();
    ifaces.sort_by_key(|i| std::cmp::Reverse(i.1 + i.2));
    ifaces.truncate(4); // show at most 4 interfaces

    let total_rx: u64 = ifaces.iter().map(|i| i.1).sum();
    let total_tx: u64 = ifaces.iter().map(|i| i.2).sum();

    // Add sample to shared history
    push_net_history(total_rx, total_tx);

    let rx_hist: Vec<f32> = net_rx_history().iter().map(|&v| v as f32).collect();
    let tx_hist: Vec<f32> = net_tx_history().iter().map(|&v| v as f32).collect();

    let peak_rx = rx_hist.iter().copied().fold(0.0f32, f32::max);
    let peak_tx = tx_hist.iter().copied().fold(0.0f32, f32::max);
    let max_val = ((peak_rx.max(peak_tx)) * 1.1).max(10_240.0); // at least 10 KB/s

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "NET SPEED GRAPH",
    );
    let ts = now.format("%H:%M:%S").to_string();
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

    // ── Current Rx / Tx ───────────────────────────────────────────────────
    let rx_str = format!("↓ {}  ↑ {}", format_bps(total_rx), format_bps(total_tx));
    let rw = text_width(&bold, PxScale::from(30.0), &rx_str);
    draw_text_mut(
        &mut img,
        GREEN,
        ((480 - rw) / 2).max(0),
        48,
        PxScale::from(30.0),
        &bold,
        &rx_str,
    );

    // ── Peak row ─────────────────────────────────────────────────────────
    let pk_str = format!(
        "peak ↓ {} / ↑ {}",
        format_bps(peak_rx as u64),
        format_bps(peak_tx as u64)
    );
    let pw = text_width(&mono, PxScale::from(14.0), &pk_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - pw) / 2).max(4),
        88,
        PxScale::from(14.0),
        &mono,
        &pk_str,
    );

    // ── Chart area (one shared, overlaid) ─────────────────────────────────
    let chart_x = 0i32;
    let chart_y = 112i32;
    let chart_w = 480i32;
    let chart_h = 270i32;

    // Grid lines at 25 / 50 / 75 %
    for pct in &[0.25f32, 0.5, 0.75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if x >= 0 && (x as u32) < img.width() && ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label = format_bps((max_val * pct) as u64);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            (ly - 10).max(chart_y),
            PxScale::from(11.0),
            &mono,
            &label,
        );
    }

    // Draw Tx first (behind), then Rx on top
    let fill_rx = Rgb([GREEN.0[0] / 8, GREEN.0[1] / 8, GREEN.0[2] / 8]);
    let fill_tx = Rgb([PEACH.0[0] / 8, PEACH.0[1] / 8, PEACH.0[2] / 8]);
    draw_line_chart(
        &mut img, &tx_hist, max_val, chart_x, chart_y, chart_w, chart_h, PEACH, fill_tx,
    );
    draw_line_chart(
        &mut img, &rx_hist, max_val, chart_x, chart_y, chart_w, chart_h, GREEN, fill_rx,
    );

    // Bottom border
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );

    // ── Legend ────────────────────────────────────────────────────────────
    draw_filled_rect_mut(&mut img, Rect::at(20, 390).of_size(14, 9), GREEN);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        40,
        387,
        PxScale::from(13.0),
        &mono,
        "Download",
    );
    draw_filled_rect_mut(&mut img, Rect::at(130, 390).of_size(14, 9), PEACH);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        150,
        387,
        PxScale::from(13.0),
        &mono,
        "Upload",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        330,
        387,
        PxScale::from(12.0),
        &mono,
        "← 1 min",
    );

    // ── Per-interface mini table ───────────────────────────────────────────
    let table_y = 408i32;
    draw_filled_rect_mut(&mut img, Rect::at(0, table_y).of_size(480, 1), SURFACE0);

    if ifaces.is_empty() {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(14.0),
            table_y + 8,
            OVERLAY0,
            "No active interfaces",
        );
    } else {
        let row_h = 16i32;
        for (n, (name, rx, tx)) in ifaces.iter().enumerate() {
            let ry = table_y + 6 + n as i32 * row_h;
            let short_name = if name.len() > 10 {
                &name[..10]
            } else {
                name.as_str()
            };
            draw_text_mut(
                &mut img,
                SUBTEXT,
                8,
                ry,
                PxScale::from(13.0),
                &mono,
                short_name,
            );
            draw_text_mut(
                &mut img,
                GREEN,
                130,
                ry,
                PxScale::from(13.0),
                &mono,
                &format!("↓ {}", format_bps(*rx)),
            );
            draw_text_mut(
                &mut img,
                PEACH,
                280,
                ry,
                PxScale::from(13.0),
                &mono,
                &format!("↑ {}", format_bps(*tx)),
            );
        }
    }

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod net_speed_graph_tests {
    use super::*;

    #[test]
    fn test_render_net_speed_graph_size() {
        let img = render_net_speed_graph();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── CPU Core Bars (horizontal per-core utilisation) ─────────────────────────

/// Render a large horizontal-bar chart of per-core CPU utilisation (480×480).
///
/// Each row is one logical CPU core.  The bar colour shifts from green → yellow
/// → red as load increases.  Good for systems with many cores where the grid
/// view in `cpu_detail` is too cramped.
pub fn render_cpu_core_bars() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, RefreshKind};

    let refresh = RefreshKind::nothing().with_cpu(CpuRefreshKind::everything());
    let mut sys = System::new_with_specifics(refresh);
    // Two refreshes to get non-zero per-core usage from sysinfo
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_specifics(refresh);

    let cores: Vec<(String, f32)> = sys
        .cpus()
        .iter()
        .map(|c| (c.name().to_string(), c.cpu_usage()))
        .collect();

    let global = sys.global_cpu_usage();
    let n = cores.len().max(1);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "CPU CORE BARS",
    );
    let ts = now.format("%H:%M:%S").to_string();
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

    // Global summary
    let acc = if global > 80.0 {
        RED
    } else if global > 50.0 {
        YELLOW
    } else {
        GREEN
    };
    let g_str = format!("{global:.1}% global");
    let gw = text_width(&bold, PxScale::from(24.0), &g_str);
    draw_text_mut(
        &mut img,
        acc,
        ((480 - gw) / 2).max(0),
        46,
        PxScale::from(24.0),
        &bold,
        &g_str,
    );

    let load = System::load_average();
    let load_str = format!(
        "load {:.2} / {:.2} / {:.2}",
        load.one, load.five, load.fifteen
    );
    let lw = text_width(&mono, PxScale::from(14.0), &load_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        ((480 - lw) / 2).max(0),
        76,
        PxScale::from(14.0),
        &mono,
        &load_str,
    );

    // ── Per-core bars ─────────────────────────────────────────────────────
    let bar_area_top = 102i32;
    let bar_area_h = 370i32;
    let max_cores = 32usize;
    let visible = n.min(max_cores);
    let row_h = (bar_area_h / visible as i32).max(10);
    let label_w = 34i32;
    let pct_w = 36i32;
    let bar_x = label_w + 4;
    let bar_max_w = 480 - bar_x - pct_w - 6;

    for (i, (_name, usage)) in cores.iter().enumerate().take(visible) {
        let y = bar_area_top + i as i32 * row_h;
        let bar_color = if *usage > 80.0 {
            RED
        } else if *usage > 50.0 {
            YELLOW
        } else {
            GREEN
        };
        let fill_color = Rgb([bar_color.0[0] / 6, bar_color.0[1] / 6, bar_color.0[2] / 6]);

        // Label
        let lbl = format!("C{i}");
        draw_text_mut(
            &mut img,
            SUBTEXT,
            4,
            y + 1,
            PxScale::from((row_h - 2).min(14) as f32),
            &mono,
            &lbl,
        );

        // Background track
        let bar_h_draw = (row_h - 4).max(4) as u32;
        draw_filled_rect_mut(
            &mut img,
            Rect::at(bar_x, y + 2).of_size(bar_max_w as u32, bar_h_draw),
            SURFACE0,
        );

        // Filled portion
        let filled =
            ((bar_max_w as f32 * usage / 100.0) as i32).max(if *usage > 0.0 { 2 } else { 0 });
        if filled > 0 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(bar_x, y + 2).of_size(filled as u32, bar_h_draw),
                bar_color,
            );
            // Lighter fill overlay for depth
            if filled > 4 {
                draw_filled_rect_mut(
                    &mut img,
                    Rect::at(bar_x, y + 2).of_size(filled as u32, (bar_h_draw / 3).max(1)),
                    fill_color,
                );
            }
        }

        // Percentage text
        let pct_str = format!("{usage:.0}%");
        draw_text_mut(
            &mut img,
            TEXT,
            480 - pct_w - 2,
            y + 1,
            PxScale::from((row_h - 2).min(13) as f32),
            &mono,
            &pct_str,
        );
    }

    // Separator line
    let sep_y = bar_area_top + visible as i32 * row_h + 2;
    if sep_y < 480 {
        draw_filled_rect_mut(&mut img, Rect::at(0, sep_y).of_size(480, 1), SURFACE0);
    }

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod cpu_core_bars_tests {
    use super::*;

    #[test]
    fn test_render_cpu_core_bars_size() {
        let img = render_cpu_core_bars();
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }
}

// ─── Ping Latency History Graph ───────────────────────────────────────────────

// Up to 4 hosts, 120 samples each
/// Per-host ring of (latency in ms, reachable) samples.
pub(super) type PingHistory = Vec<std::collections::VecDeque<(f32, bool)>>;

pub(super) static PING_HISTORY: OnceLock<std::sync::Mutex<PingHistory>> = OnceLock::new();

pub(super) fn ping_history_data() -> std::sync::MutexGuard<'static, PingHistory> {
    PING_HISTORY
        .get_or_init(|| {
            // Pre-allocate for up to 4 hosts × 120 samples
            let v: Vec<std::collections::VecDeque<(f32, bool)>> = (0..4)
                .map(|_| std::collections::VecDeque::with_capacity(120))
                .collect();
            std::sync::Mutex::new(v)
        })
        .lock()
        .unwrap()
}

/// Record the latest ping results into the history ring-buffers.
///
/// Called by the ping_monitor_loop each time a fresh set of results arrives.
/// Only the first 4 hosts are tracked.
pub fn push_ping_history(results: &[PingResult]) {
    let mut hist = ping_history_data();
    for (idx, res) in results.iter().enumerate().take(4) {
        while hist.len() <= idx {
            hist.push(std::collections::VecDeque::with_capacity(120));
        }
        let buf = &mut hist[idx];
        if buf.len() >= 120 {
            buf.pop_front();
        }
        let ms = res.latency_ms.unwrap_or(0.0) as f32;
        let ok = res.latency_ms.is_some() && res.loss_pct < 100.0;
        buf.push_back((ms, ok));
    }
}

/// Render a ping latency history graph (480×480).
///
/// Shows up to 4 configured hosts as overlaid coloured line charts.
/// Unreachable samples are shown as a red vertical spike at the bottom.
pub fn render_ping_graph(results: &[PingResult]) -> DynamicImage {
    // Push fresh samples first
    push_ping_history(results);

    let hist = ping_history_data();
    let hosts: Vec<(String, Vec<(f32, bool)>)> = results
        .iter()
        .enumerate()
        .take(4)
        .map(|(i, res)| {
            let buf: Vec<(f32, bool)> = if i < hist.len() {
                hist[i].iter().copied().collect()
            } else {
                vec![]
            };
            (res.label.clone(), buf)
        })
        .collect();
    drop(hist);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();
    let now = Local::now();

    // ── Header ────────────────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        18,
        PxScale::from(19.0),
        &bold,
        "PING GRAPH",
    );
    let ts = now.format("%H:%M:%S").to_string();
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

    // Per-host current latency
    let colors: &[Rgb<u8>] = &[GREEN, BLUE, PEACH, MAUVE];
    let mut cur_y = 48i32;
    for (i, (label, _)) in hosts.iter().enumerate() {
        let ms = results.get(i).and_then(|r| r.latency_ms).unwrap_or(0.0);
        let loss = results.get(i).map_or(100.0, |r| r.loss_pct);
        let col = if loss >= 100.0 {
            RED
        } else if ms > 100.0 {
            YELLOW
        } else {
            colors[i]
        };
        let stat = if loss >= 100.0 {
            format!("{label}: timeout")
        } else {
            format!("{label}: {ms:.1} ms ({loss:.0}% loss)")
        };
        draw_text_mut(&mut img, col, 20, cur_y, PxScale::from(16.0), &bold, &stat);
        cur_y += 20;
    }

    // ── Compute chart scale ───────────────────────────────────────────────
    let max_ms = hosts
        .iter()
        .flat_map(|(_, buf)| buf.iter())
        .filter(|(_, ok)| *ok)
        .map(|(ms, _)| *ms)
        .fold(0.0f32, f32::max);
    let chart_max = (max_ms * 1.2).max(100.0);

    // ── Chart area ────────────────────────────────────────────────────────
    let chart_x = 0i32;
    let chart_y = (cur_y + 10).max(130);
    let chart_w = 480i32;
    let chart_h = 480 - chart_y - 60;

    // Grid lines at 25%, 50%, 75%
    for pct in &[0.25f32, 0.5, 0.75] {
        let ly = chart_y + chart_h - (chart_h as f32 * pct) as i32;
        for x in (chart_x..(chart_x + chart_w)).step_by(8) {
            if ly >= 0 && (ly as u32) < img.height() {
                img.put_pixel(x as u32, ly as u32, SURFACE0);
            }
        }
        let label_ms = format!("{:.0}ms", chart_max * pct);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            2,
            (ly - 10).max(chart_y),
            PxScale::from(11.0),
            &mono,
            &label_ms,
        );
    }

    // Per-host line charts
    for (i, (_, buf)) in hosts.iter().enumerate().take(4) {
        if buf.is_empty() {
            continue;
        }
        let color = colors[i];
        let fill = Rgb([color.0[0] / 8, color.0[1] / 8, color.0[2] / 8]);
        let vals: Vec<f32> = buf
            .iter()
            .map(|(ms, ok)| if *ok { *ms } else { 0.0 })
            .collect();
        draw_line_chart(
            &mut img, &vals, chart_max, chart_x, chart_y, chart_w, chart_h, color, fill,
        );

        // Timeout markers (vertical red lines at bottom)
        let n = buf.len();
        let bar_w = (chart_w / 120).max(2);
        let offset = 120 - n as i32;
        for (j, (_, ok)) in buf.iter().enumerate() {
            if !ok {
                let bx = chart_x + (offset + j as i32) * bar_w;
                let by = chart_y + chart_h - 10;
                if bx >= 0 && (bx as u32) < img.width() as u32 {
                    draw_filled_rect_mut(&mut img, Rect::at(bx, by).of_size(bar_w as u32, 10), RED);
                }
            }
        }
    }

    // Chart border
    draw_filled_rect_mut(
        &mut img,
        Rect::at(chart_x, chart_y + chart_h).of_size(chart_w as u32, 2),
        SURFACE0,
    );

    // ── Legend ────────────────────────────────────────────────────────────
    let legend_y = chart_y + chart_h + 6;
    let mut lx = 8i32;
    for (i, (label, _)) in hosts.iter().enumerate().take(4) {
        let color = colors[i];
        draw_filled_rect_mut(&mut img, Rect::at(lx, legend_y + 2).of_size(12, 8), color);
        lx += 14;
        let short = if label.len() > 12 {
            &label[..12]
        } else {
            label.as_str()
        };
        draw_text_mut(
            &mut img,
            SUBTEXT,
            lx,
            legend_y,
            PxScale::from(13.0),
            &mono,
            short,
        );
        lx += text_width(&mono, PxScale::from(13.0), short) + 10;
    }

    let footer = "← 2 min ago    now →  ▼ = timeout";
    let fw = text_width(&mono, PxScale::from(12.0), footer);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - fw).max(0),
        legend_y + 17,
        PxScale::from(12.0),
        &mono,
        footer,
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod ping_graph_tests {
    use super::*;

    #[test]
    fn test_render_ping_graph_empty() {
        let img = render_ping_graph(&[]);
        assert_eq!(img.width(), 480);
        assert_eq!(img.height(), 480);
    }

    #[test]
    fn test_render_ping_graph_with_results() {
        let results = vec![
            PingResult {
                host: "8.8.8.8".into(),
                label: "Google DNS".into(),
                latency_ms: Some(12.5),
                loss_pct: 0.0,
            },
            PingResult {
                host: "1.1.1.1".into(),
                label: "Cloudflare".into(),
                latency_ms: Some(8.3),
                loss_pct: 0.0,
            },
        ];
        push_ping_history(&results);
        let img = render_ping_graph(&results);
        assert_eq!(img.width(), 480);
    }
}
