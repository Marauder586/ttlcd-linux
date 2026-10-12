//! Screensaver-style visualisations driven by live system data, including the EKG strip.

use super::*;

// ─── CPU Core Heatmap ─────────────────────────────────────────────────────────

/// Per-core CPU utilization history (rows = cores, cols = time samples).
/// Stored as a Vec<VecDeque<f32>> where each VecDeque is one core's history.
pub(super) static CPU_CORE_HISTORY: std::sync::OnceLock<
    std::sync::Mutex<Vec<std::collections::VecDeque<f32>>>,
> = std::sync::OnceLock::new();

pub(super) const CPU_HEATMAP_COLS: usize = 60; // 60 time samples

/// Update per-core history. Call on each display tick.
pub fn push_cpu_core_history() {
    use sysinfo::{CpuRefreshKind, System};
    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(100));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());

    let cpus = sys.cpus();
    let n = cpus.len();

    let mut history = CPU_CORE_HISTORY
        .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n.max(1)]))
        .lock()
        .unwrap();

    // Grow if CPU count changed
    while history.len() < n {
        history.push(std::collections::VecDeque::new());
    }

    for (i, cpu) in cpus.iter().enumerate() {
        if i >= history.len() {
            break;
        }
        history[i].push_back(cpu.cpu_usage());
        while history[i].len() > CPU_HEATMAP_COLS {
            history[i].pop_front();
        }
    }
}

/// Render the CPU core heatmap: cores on Y axis, time on X axis.
/// Hot = red, warm = yellow, cool = green, idle = dark teal.
pub fn render_cpu_heatmap() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, System};

    // Refresh and push a sample
    let mut sys = System::new();
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(150));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());

    let cpus = sys.cpus();
    let n_cores = cpus.len().max(1);
    let cpu_usage = sys.global_cpu_usage();

    {
        let mut history = CPU_CORE_HISTORY
            .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n_cores]))
            .lock()
            .unwrap();

        while history.len() < n_cores {
            history.push(std::collections::VecDeque::new());
        }
        for (i, cpu) in cpus.iter().enumerate() {
            if i >= history.len() {
                break;
            }
            history[i].push_back(cpu.cpu_usage());
            while history[i].len() > CPU_HEATMAP_COLS {
                history[i].pop_front();
            }
        }
    }

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Title
    let title = format!("CPU Heatmap — {cpu_usage:.0}% avg");
    let tw = text_width(&font, PxScale::from(22.0), &title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        6,
        PxScale::from(22.0),
        &font,
        &title,
    );

    // Core count label
    let cstr = format!("{n_cores} cores × {CPU_HEATMAP_COLS} samples");
    let cw = text_width(&font, PxScale::from(14.0), &cstr);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        (480 - cw) / 2,
        32,
        PxScale::from(14.0),
        &font,
        &cstr,
    );

    // Heat map grid
    let grid_x = 40_i32;
    let grid_y = 50_i32;
    let grid_w = 430_i32;
    let grid_h = 400_i32;

    let cell_w = (grid_w as f32 / CPU_HEATMAP_COLS as f32).max(1.0);
    let cell_h = (grid_h as f32 / n_cores as f32).max(1.0);

    let history = CPU_CORE_HISTORY
        .get_or_init(|| std::sync::Mutex::new(vec![std::collections::VecDeque::new(); n_cores]))
        .lock()
        .unwrap();

    for (core_idx, core_history) in history.iter().enumerate() {
        let y0 = grid_y + (core_idx as f32 * cell_h) as i32;
        let y1 = grid_y + ((core_idx + 1) as f32 * cell_h) as i32;

        // Core label on Y axis
        if cell_h >= 10.0 {
            let label = format!("C{core_idx}");
            draw_text_mut(
                &mut img,
                OVERLAY0,
                4,
                y0 + 1,
                PxScale::from(cell_h.min(14.0)),
                &font,
                &label,
            );
        }

        for (col_idx, &usage) in core_history.iter().enumerate() {
            let x0 = grid_x + (col_idx as f32 * cell_w) as i32;
            let x1 = grid_x + ((col_idx + 1) as f32 * cell_w) as i32;

            // Map usage 0..100 → heatmap colour
            let u = (usage / 100.0).clamp(0.0, 1.0) as f64;
            let (r, g, b) = if u < 0.25 {
                // Dark blue/teal (idle)
                let t = u / 0.25;
                (
                    (30.0 + (148.0 - 30.0) * t) as u8,
                    (30.0 + (226.0 - 30.0) * t) as u8,
                    (46.0 + (213.0 - 46.0) * t) as u8,
                )
            } else if u < 0.5 {
                // Teal to green
                let t = (u - 0.25) / 0.25;
                (
                    (148.0 + (166.0 - 148.0) * t) as u8,
                    (226.0 + (227.0 - 226.0) * t) as u8,
                    (213.0 + (161.0 - 213.0) * t) as u8,
                )
            } else if u < 0.75 {
                // Green to yellow
                let t = (u - 0.5) / 0.25;
                (
                    (166.0 + (249.0 - 166.0) * t) as u8,
                    (227.0 + (226.0 - 227.0) * t) as u8,
                    (161.0 + (175.0 - 161.0) * t) as u8,
                )
            } else {
                // Yellow to red
                let t = (u - 0.75) / 0.25;
                (
                    (249.0 + (243.0 - 249.0) * t) as u8,
                    (226.0 + (139.0 - 226.0) * t) as u8,
                    (175.0 + (168.0 - 175.0) * t) as u8,
                )
            };

            let col = Rgb([r, g, b]);
            for py in y0..y1 {
                for px in x0..x1 {
                    if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                        img.put_pixel(px as u32, py as u32, col);
                    }
                }
            }
        }
    }

    // Time axis labels
    draw_text_mut(
        &mut img,
        OVERLAY0,
        grid_x,
        grid_y + grid_h + 4,
        PxScale::from(13.0),
        &font,
        "← older",
    );
    let now_str = {
        let n = chrono::Local::now();
        format!("{:02}:{:02}:{:02} →", n.hour(), n.minute(), n.second())
    };
    let nw = text_width(&font, PxScale::from(13.0), &now_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        grid_x + grid_w - nw,
        grid_y + grid_h + 4,
        PxScale::from(13.0),
        &font,
        &now_str,
    );

    // Color legend bar
    let legend_y = grid_y + grid_h + 22;
    let legend_w = grid_w;
    for lx in 0..legend_w {
        let u = lx as f64 / legend_w as f64;
        let (lr, lg, lb) = if u < 0.25 {
            let t = u / 0.25;
            (
                (30.0 + 118.0 * t) as u8,
                (30.0 + 196.0 * t) as u8,
                (46.0 + 167.0 * t) as u8,
            )
        } else if u < 0.5 {
            let t = (u - 0.25) / 0.25;
            (
                (148.0 + 18.0 * t) as u8,
                (226.0 + 1.0 * t) as u8,
                (213.0 - 52.0 * t) as u8,
            )
        } else if u < 0.75 {
            let t = (u - 0.5) / 0.25;
            (
                (166.0 + 83.0 * t) as u8,
                (227.0 - 1.0 * t) as u8,
                (161.0 + 14.0 * t) as u8,
            )
        } else {
            let t = (u - 0.75) / 0.25;
            (
                (249.0 - 6.0 * t) as u8,
                (226.0 - 87.0 * t) as u8,
                (175.0 - 7.0 * t) as u8,
            )
        };
        let px = grid_x + lx;
        for dy in 0..6 {
            let py_leg = legend_y + dy;
            if py_leg < 480 && (px as u32) < 480 {
                img.put_pixel(px as u32, py_leg as u32, Rgb([lr, lg, lb]));
            }
        }
    }
    draw_text_mut(
        &mut img,
        OVERLAY0,
        grid_x,
        legend_y + 7,
        PxScale::from(12.0),
        &font,
        "0%",
    );
    let h_str = "100%";
    let hw = text_width(&font, PxScale::from(12.0), h_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        grid_x + grid_w - hw,
        legend_y + 7,
        PxScale::from(12.0),
        &font,
        h_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Oscilloscope CPU Waveform ────────────────────────────────────────────────

pub(super) static OSCILLO_HISTORY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::VecDeque<f32>>,
> = std::sync::OnceLock::new();

pub fn render_oscilloscope() -> DynamicImage {
    const SAMPLES: usize = 240;
    const GREEN_PHOSPHOR: Rgb<u8> = Rgb([0, 220, 100]);
    const DIM_PHOSPHOR: Rgb<u8> = Rgb([0, 55, 25]);

    let history_lock = OSCILLO_HISTORY.get_or_init(|| {
        let mut dq = std::collections::VecDeque::with_capacity(SAMPLES + 1);
        for _ in 0..SAMPLES {
            dq.push_back(0.0f32);
        }
        std::sync::Mutex::new(dq)
    });

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let cpu_pct: f32 = {
        let cpus = sys.cpus();
        if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / cpus.len() as f32
        }
    };
    let mem_pct: f32 = if sys.total_memory() > 0 {
        sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
    } else {
        0.0
    };

    {
        let mut hist = history_lock.lock().unwrap();
        hist.push_back(cpu_pct);
        if hist.len() > SAMPLES {
            hist.pop_front();
        }
    }

    let hist = history_lock.lock().unwrap();
    let samples: Vec<f32> = hist.iter().copied().collect();
    drop(hist);

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([0, 8, 4]);
    } // deep green-black

    let font = font_bold();

    // Horizontal grid lines at 25%, 50%, 75%
    for pct_level in [25u32, 50, 75] {
        let y = (480 - (pct_level as f32 / 100.0 * 400.0) as u32 - 40).min(479);
        for x in (0..480u32).step_by(6) {
            if x + 2 < 480 {
                img.put_pixel(x, y, Rgb([0, 30, 15]));
                img.put_pixel(x + 1, y, Rgb([0, 30, 15]));
            }
        }
        let label = format!("{pct_level}%");
        draw_text_mut(
            &mut img,
            Rgb([0, 60, 30]),
            2,
            y as i32 - 8,
            PxScale::from(12.0),
            &font,
            &label,
        );
    }

    // Vertical time markers every 60 samples (2-second marks)
    for i in (0..SAMPLES).step_by(60) {
        let x = (i as f32 / SAMPLES as f32 * 480.0) as u32;
        for y in (40..440u32).step_by(4) {
            if y + 1 < 480 {
                img.put_pixel(x, y, Rgb([0, 25, 12]));
            }
        }
    }

    // Draw CPU waveform with vertical phosphor glow
    for (i, &val) in samples.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let x0 = ((i - 1) as f32 / SAMPLES as f32 * 478.0) as i32;
        let x1 = (i as f32 / SAMPLES as f32 * 478.0) as i32;
        let y0 = (440 - (samples[i - 1] / 100.0 * 400.0) as i32).clamp(40, 440);
        let y1 = (440 - (val / 100.0 * 400.0) as i32).clamp(40, 440);

        // Glow (dim, wider)
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, (y0 - 1).max(0)),
            (x1, (y1 - 1).max(0)),
            DIM_PHOSPHOR,
            interpolate,
        );
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, (y0 + 1).min(479)),
            (x1, (y1 + 1).min(479)),
            DIM_PHOSPHOR,
            interpolate,
        );
        // Main line
        imageproc::drawing::draw_antialiased_line_segment_mut(
            &mut img,
            (x0, y0),
            (x1, y1),
            GREEN_PHOSPHOR,
            interpolate,
        );
    }

    // RAM usage bar at bottom (thin, blue)
    let ram_w = (mem_pct / 100.0 * 480.0) as u32;
    let ram_rect = imageproc::rect::Rect::at(0, 462).of_size(ram_w.clamp(1, 480), 6);
    imageproc::drawing::draw_filled_rect_mut(&mut img, ram_rect, BLUE);

    // Scanline overlay (subtle)
    for row in (0u32..480).step_by(2) {
        for col in 0u32..480 {
            if let Some(p) = img.get_pixel_mut_checked(col, row) {
                p.0[1] = p.0[1].saturating_sub(4);
            }
        }
    }

    // Labels
    draw_text_mut(
        &mut img,
        GREEN_PHOSPHOR,
        180,
        6,
        PxScale::from(20.0),
        &font,
        "CPU OSCILLOSCOPE",
    );
    let cpu_str = format!("CPU {cpu_pct:5.1}%  RAM {mem_pct:5.1}%");
    let tw = text_width(&font, PxScale::from(16.0), &cpu_str);
    draw_text_mut(
        &mut img,
        GREEN_PHOSPHOR,
        (480 - tw as i32) / 2,
        452,
        PxScale::from(16.0),
        &font,
        &cpu_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Network Heatmap ─────────────────────────────────────────────────────────

pub(super) const NET_HEATMAP_COLS: usize = 60;

pub(super) static NET_HM_RX_HISTORY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::VecDeque<f64>>,
> = std::sync::OnceLock::new();
pub(super) static NET_HM_TX_HISTORY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::VecDeque<f64>>,
> = std::sync::OnceLock::new();
pub(super) static NET_HM_PREV_RX: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();
pub(super) static NET_HM_PREV_TX: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_net_heatmap() -> DynamicImage {
    // ── Gather data ───────────────────────────────────────────────────────────
    let (total_rx, total_tx): (u64, u64) = {
        let mut rx = 0u64;
        let mut tx = 0u64;
        if let Ok(content) = std::fs::read_to_string("/proc/net/dev") {
            for line in content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let iface = parts[0].trim_end_matches(':');
                    if iface == "lo" {
                        continue;
                    }
                    rx += parts[1].parse::<u64>().unwrap_or(0);
                    tx += parts[9].parse::<u64>().unwrap_or(0);
                }
            }
        }
        (rx, tx)
    };

    let delta_rx: f64 = {
        let mut prev = NET_HM_PREV_RX
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        let d = if total_rx > *prev {
            (total_rx - *prev) as f64
        } else {
            0.0
        };
        *prev = total_rx;
        d
    };
    let delta_tx: f64 = {
        let mut prev = NET_HM_PREV_TX
            .get_or_init(|| std::sync::Mutex::new(0))
            .lock()
            .unwrap();
        let d = if total_tx > *prev {
            (total_tx - *prev) as f64
        } else {
            0.0
        };
        *prev = total_tx;
        d
    };

    let init_dq = || {
        let mut dq = std::collections::VecDeque::with_capacity(NET_HEATMAP_COLS + 1);
        for _ in 0..NET_HEATMAP_COLS {
            dq.push_back(0.0f64);
        }
        std::sync::Mutex::new(dq)
    };
    {
        let mut rx_hist = NET_HM_RX_HISTORY.get_or_init(init_dq).lock().unwrap();
        rx_hist.push_back(delta_rx);
        if rx_hist.len() > NET_HEATMAP_COLS {
            rx_hist.pop_front();
        }
    }
    {
        let mut tx_hist = NET_HM_TX_HISTORY.get_or_init(init_dq).lock().unwrap();
        tx_hist.push_back(delta_tx);
        if tx_hist.len() > NET_HEATMAP_COLS {
            tx_hist.pop_front();
        }
    }

    let rx_samples: Vec<f64> = NET_HM_RX_HISTORY
        .get_or_init(init_dq)
        .lock()
        .unwrap()
        .iter()
        .copied()
        .collect();
    let tx_samples: Vec<f64> = NET_HM_TX_HISTORY
        .get_or_init(init_dq)
        .lock()
        .unwrap()
        .iter()
        .copied()
        .collect();

    let max_rx = rx_samples.iter().copied().fold(1.0f64, f64::max);
    let max_tx = tx_samples.iter().copied().fold(1.0f64, f64::max);
    let global_max = max_rx.max(max_tx).max(1024.0); // at least 1 KB

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }
    let font = font_bold();

    draw_text_mut(
        &mut img,
        TEXT,
        8,
        8,
        PxScale::from(22.0),
        &font,
        "Network Heatmap",
    );

    let grid_x = 80i32;
    let grid_w = 380i32;
    let cell_w = grid_w as f32 / NET_HEATMAP_COLS as f32;

    let legend_labels = ["Rx (↓)", "Tx (↑)"];
    let section_ys = [60i32, 265i32];
    let samples_list = [&rx_samples, &tx_samples];

    for (si, &samples) in samples_list.iter().enumerate() {
        let base_y = section_ys[si];
        draw_text_mut(
            &mut img,
            SUBTEXT,
            4,
            base_y,
            PxScale::from(14.0),
            &font,
            legend_labels[si],
        );

        let grid_h = 180i32;

        for (col_idx, &val) in samples.iter().enumerate() {
            let frac = (val / global_max).min(1.0) as f32;
            let x0 = (grid_x as f32 + col_idx as f32 * cell_w) as i32;
            let x1 = (grid_x as f32 + (col_idx + 1) as f32 * cell_w).ceil() as i32;
            let bar_h = (frac * grid_h as f32) as i32;
            if bar_h == 0 {
                continue;
            }

            // Colour: blue(low) → teal(mid) → green(high)
            let col = if frac < 0.33 {
                let t = frac / 0.33;
                let r = (BLUE.0[0] as f32 * (1.0 - t) + TEAL.0[0] as f32 * t) as u8;
                let g = (BLUE.0[1] as f32 * (1.0 - t) + TEAL.0[1] as f32 * t) as u8;
                let b = (BLUE.0[2] as f32 * (1.0 - t) + TEAL.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            } else if frac < 0.66 {
                let t = (frac - 0.33) / 0.33;
                let r = (TEAL.0[0] as f32 * (1.0 - t) + GREEN.0[0] as f32 * t) as u8;
                let g = (TEAL.0[1] as f32 * (1.0 - t) + GREEN.0[1] as f32 * t) as u8;
                let b = (TEAL.0[2] as f32 * (1.0 - t) + GREEN.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            } else {
                let t = (frac - 0.66) / 0.34;
                let r = (GREEN.0[0] as f32 * (1.0 - t) + YELLOW.0[0] as f32 * t) as u8;
                let g = (GREEN.0[1] as f32 * (1.0 - t) + YELLOW.0[1] as f32 * t) as u8;
                let b = (GREEN.0[2] as f32 * (1.0 - t) + YELLOW.0[2] as f32 * t) as u8;
                Rgb([r, g, b])
            };

            let y0 = base_y + grid_h - bar_h;
            let rect =
                imageproc::rect::Rect::at(x0, y0).of_size((x1 - x0).max(1) as u32, bar_h as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, rect, col);
        }

        // Baseline
        for x in grid_x..(grid_x + grid_w) {
            let by = base_y + grid_h;
            if by < 480 {
                img.put_pixel(x as u32, by as u32, SURFACE0);
            }
        }

        // Current value label
        let cur_rx = delta_rx;
        let cur_tx = delta_tx;
        let cur = if si == 0 { cur_rx } else { cur_tx };
        let label = human_bytes(cur) + "/s";
        draw_text_mut(
            &mut img,
            SUBTEXT,
            grid_x + grid_w - text_width(&font, PxScale::from(14.0), &label) + 4,
            base_y,
            PxScale::from(14.0),
            &font,
            &label,
        );

        // Max label
        let max_label = human_bytes(global_max) + " max";
        draw_text_mut(
            &mut img,
            OVERLAY0,
            grid_x,
            base_y + grid_h - 20,
            PxScale::from(12.0),
            &font,
            &max_label,
        );
    }

    // Time axis labels
    draw_text_mut(
        &mut img,
        OVERLAY0,
        grid_x,
        456,
        PxScale::from(11.0),
        &font,
        "← older",
    );
    let newer = format!("{} newer →", chrono::Local::now().format("%H:%M:%S"));
    let nw = text_width(&font, PxScale::from(11.0), &newer);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - nw as i32 - 4,
        456,
        PxScale::from(11.0),
        &font,
        &newer,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Audio Spectrum Visualizer (CPU-driven) ───────────────────────────────────

pub(super) const SPECTRUM_BARS: usize = 32;

pub(super) static SPECTRUM_HEIGHTS: std::sync::OnceLock<std::sync::Mutex<Vec<f32>>> =
    std::sync::OnceLock::new();
pub(super) static SPECTRUM_PEAKS: std::sync::OnceLock<std::sync::Mutex<Vec<f32>>> =
    std::sync::OnceLock::new();
pub(super) static SPECTRUM_TICK: std::sync::OnceLock<std::sync::Mutex<u64>> =
    std::sync::OnceLock::new();

pub fn render_spectrum() -> DynamicImage {
    let tick = {
        let t = SPECTRUM_TICK.get_or_init(|| std::sync::Mutex::new(0));
        let mut g = t.lock().unwrap();
        let v = *g;
        *g = g.wrapping_add(1);
        v
    };

    let heights_lock =
        SPECTRUM_HEIGHTS.get_or_init(|| std::sync::Mutex::new(vec![0.0f32; SPECTRUM_BARS]));
    let peaks_lock =
        SPECTRUM_PEAKS.get_or_init(|| std::sync::Mutex::new(vec![0.0f32; SPECTRUM_BARS]));

    // Get CPU usage per core, map to spectrum bars
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let cpus = sys.cpus();
    let n = cpus.len().max(1);
    let cpu_loads: Vec<f32> = cpus.iter().map(|c| c.cpu_usage() / 100.0).collect();

    // Pseudo-random seed for this tick
    let mut seed = tick.wrapping_mul(2654435761);
    let lcg = |v: u64| {
        v.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

    // Distribute CPU loads across spectrum bars with noise
    let mut heights = heights_lock.lock().unwrap();
    let mut peaks = peaks_lock.lock().unwrap();

    for i in 0..SPECTRUM_BARS {
        // Map bar to a CPU core
        let core_idx = (i * n / SPECTRUM_BARS).min(n - 1);
        let base_load = cpu_loads[core_idx];

        // Add frequency-like envelope: louder in bass/mids, quieter at extremes
        let freq_frac = i as f32 / SPECTRUM_BARS as f32;
        let envelope = 1.0 - (freq_frac * 2.0 - 1.0).powi(2) * 0.4; // slight U-dip at edges

        seed = lcg(seed);
        let noise = (seed as f32 / u64::MAX as f32 - 0.5) * 0.25;

        // Target height: CPU load × envelope + noise + slight boost for bass
        let bass_boost = if i < 4 { 0.1 } else { 0.0 };
        let target = ((base_load * envelope + noise + bass_boost) * 0.9 + 0.05).clamp(0.0, 1.0);

        // Smooth: rise fast, decay slow
        if target > heights[i] {
            heights[i] += (target - heights[i]) * 0.35;
        } else {
            heights[i] -= (heights[i] - target) * 0.12;
        }
        heights[i] = heights[i].clamp(0.02, 1.0);

        // Peak hold + decay
        if heights[i] > peaks[i] {
            peaks[i] = heights[i];
        } else {
            peaks[i] -= 0.008;
        }
        peaks[i] = peaks[i].clamp(0.0, 1.0);
    }

    let heights_snap: Vec<f32> = heights.clone();
    let peaks_snap: Vec<f32> = peaks.clone();
    drop(heights);
    drop(peaks);

    // ── Render ────────────────────────────────────────────────────────────────
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = Rgb([4, 4, 16]);
    }
    let font = font_bold();

    let bar_area_h = 380i32;
    let bar_area_y = 60i32;
    let bar_total_w = 480i32;
    let bar_w = (bar_total_w as f32 / SPECTRUM_BARS as f32).floor() as i32;
    let bar_gap = 2i32;
    let bar_inner = (bar_w - bar_gap).max(1);

    // Frequency labels at bottom
    let freq_labels = [
        "32", "64", "128", "256", "512", "1k", "2k", "4k", "8k", "16k",
    ];
    let label_step = SPECTRUM_BARS / freq_labels.len();

    for (i, &h) in heights_snap.iter().enumerate() {
        let _freq_frac = i as f32 / SPECTRUM_BARS as f32;
        let bx = i as i32 * bar_w;
        let bar_h = (h * bar_area_h as f32) as i32;
        let by = bar_area_y + bar_area_h - bar_h;

        // Colour gradient: BLUE → TEAL → GREEN → YELLOW → RED (low → high energy)
        let col = if h < 0.3 {
            let t = h / 0.3;
            let r = (BLUE.0[0] as f32 * (1.0 - t) + TEAL.0[0] as f32 * t) as u8;
            let g = (BLUE.0[1] as f32 * (1.0 - t) + TEAL.0[1] as f32 * t) as u8;
            let b = (BLUE.0[2] as f32 * (1.0 - t) + TEAL.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else if h < 0.6 {
            let t = (h - 0.3) / 0.3;
            let r = (TEAL.0[0] as f32 * (1.0 - t) + GREEN.0[0] as f32 * t) as u8;
            let g = (TEAL.0[1] as f32 * (1.0 - t) + GREEN.0[1] as f32 * t) as u8;
            let b = (TEAL.0[2] as f32 * (1.0 - t) + GREEN.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else if h < 0.85 {
            let t = (h - 0.6) / 0.25;
            let r = (GREEN.0[0] as f32 * (1.0 - t) + YELLOW.0[0] as f32 * t) as u8;
            let g = (GREEN.0[1] as f32 * (1.0 - t) + YELLOW.0[1] as f32 * t) as u8;
            let b = (GREEN.0[2] as f32 * (1.0 - t) + YELLOW.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        } else {
            let t = (h - 0.85) / 0.15;
            let r = (YELLOW.0[0] as f32 * (1.0 - t) + RED.0[0] as f32 * t) as u8;
            let g = (YELLOW.0[1] as f32 * (1.0 - t) + RED.0[1] as f32 * t) as u8;
            let b = (YELLOW.0[2] as f32 * (1.0 - t) + RED.0[2] as f32 * t) as u8;
            Rgb([r, g, b])
        };

        // Draw bar
        if bar_h > 0 {
            let bar_rect =
                imageproc::rect::Rect::at(bx, by).of_size(bar_inner as u32, bar_h as u32);
            imageproc::drawing::draw_filled_rect_mut(&mut img, bar_rect, col);

            // Bright top pixel line
            let bright = Rgb([
                (col.0[0] as u32 + 80).min(255) as u8,
                (col.0[1] as u32 + 80).min(255) as u8,
                (col.0[2] as u32 + 80).min(255) as u8,
            ]);
            for dx in 0..bar_inner {
                let px = (bx + dx).clamp(0, 479) as u32;
                if (0..480).contains(&by) {
                    img.put_pixel(px, by as u32, bright);
                }
            }
        }

        // Peak dot
        let peak_h = (peaks_snap[i] * bar_area_h as f32) as i32;
        let peak_y = bar_area_y + bar_area_h - peak_h;
        if peak_y >= bar_area_y && peak_y < bar_area_y + bar_area_h {
            for dx in 0..bar_inner {
                let px = (bx + dx).clamp(0, 479) as u32;
                img.put_pixel(px, peak_y as u32, TEXT);
            }
        }

        // Freq label
        if label_step > 0 && i % label_step == 0 {
            let li = i / label_step;
            if li < freq_labels.len() {
                let label = freq_labels[li];
                draw_text_mut(
                    &mut img,
                    OVERLAY0,
                    bx,
                    450,
                    PxScale::from(11.0),
                    &font,
                    label,
                );
            }
        }
    }

    // Baseline
    for x in 0u32..480 {
        img.put_pixel(x, (bar_area_y + bar_area_h) as u32, SURFACE0);
    }

    // dB scale labels on left
    for db in &[0i32, -6, -12, -24] {
        let frac = 1.0 - (db.abs() as f32 / 24.0);
        let y = bar_area_y + ((1.0 - frac) * bar_area_h as f32) as i32;
        let label = format!("{db}dB");
        draw_text_mut(&mut img, OVERLAY0, 0, y, PxScale::from(10.0), &font, &label);
    }

    // Title
    let cpu_avg: f32 = {
        let mut sys2 = System::new();
        sys2.refresh_cpu_all();
        let cpus = sys2.cpus();
        if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / cpus.len() as f32
        }
    };
    let title = format!("Spectrum Analyzer — CPU {cpu_avg:.0}%");
    let tw = text_width(&font, PxScale::from(20.0), &title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw as i32) / 2,
        10,
        PxScale::from(20.0),
        &font,
        &title,
    );

    // Frequency label at bottom
    draw_text_mut(
        &mut img,
        OVERLAY0,
        200,
        465,
        PxScale::from(11.0),
        &font,
        "Hz →",
    );

    DynamicImage::ImageRgb8(img)
}

// ─── CPU Hex Grid ─────────────────────────────────────────────────────────────

/// Per-core CPU-load visualised as a hexagonal grid.
/// Each hexagon's fill level = core load; colour = load category
/// (green → yellow → orange → red).
pub fn render_hex_grid() -> DynamicImage {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let cpus = sys.cpus();
    let n_cores = cpus.len().max(1);

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Layout: determine hex size so all cores fit in a grid
    // Hex flat-top layout: hex width=2r, height=sqrt(3)*r
    // Try to fit in roughly square grid
    let cols = ((n_cores as f32).sqrt().ceil() as usize).max(1);
    let rows = n_cores.div_ceil(cols);

    let pad = 6.0f32;
    let hex_r = ((480.0 - pad * (cols + 1) as f32) / (cols as f32 * 2.0))
        .min((480.0 - pad * (rows + 1) as f32) / (rows as f32 * 1.732))
        .min(48.0);
    let hex_w = hex_r * 2.0;
    let hex_h = hex_r * 1.732;

    let total_w = cols as f32 * hex_w + (cols + 1) as f32 * pad;
    let total_h = rows as f32 * hex_h + (rows + 1) as f32 * pad;
    let ox = (480.0 - total_w) / 2.0 + pad;
    let oy = (480.0 - total_h) / 2.0 + pad + 30.0; // +30 for title

    for (i, cpu) in cpus.iter().enumerate() {
        let load = cpu.cpu_usage() / 100.0;
        let col = i % cols;
        let row = i / cols;

        let cx = ox + col as f32 * (hex_w + pad) + hex_r;
        let cy = oy + row as f32 * (hex_h + pad) + hex_r;

        // Flat-top hexagon: 6 vertices at angles 0°, 60°, 120°, 180°, 240°, 300°
        let verts: Vec<(f32, f32)> = (0..6)
            .map(|k| {
                let angle = (k as f32 * 60.0 - 30.0).to_radians();
                (cx + hex_r * angle.cos(), cy + hex_r * angle.sin())
            })
            .collect();

        // Fill colour based on load
        let fill_col = if load < 0.4 {
            GREEN
        } else if load < 0.7 {
            YELLOW
        } else if load < 0.9 {
            PEACH
        } else {
            RED
        };

        // Draw filled hexagon by scanning bounding box
        let min_x = verts.iter().map(|v| v.0).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_x = verts
            .iter()
            .map(|v| v.0)
            .fold(f32::MIN, f32::max)
            .min(479.0) as u32;
        let min_y = verts.iter().map(|v| v.1).fold(f32::MAX, f32::min).max(0.0) as u32;
        let max_y = verts
            .iter()
            .map(|v| v.1)
            .fold(f32::MIN, f32::max)
            .min(479.0) as u32;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                // Point-in-polygon test
                let inside = verts
                    .windows(2)
                    .chain(std::iter::once([verts[5], verts[0]].as_slice()))
                    .fold(false, |inside, pair| {
                        let (ax, ay) = (pair[0].0, pair[0].1);
                        let (bx, by) = (pair[1].0, pair[1].1);
                        if (ay > fy) != (by > fy) && fx < (bx - ax) * (fy - ay) / (by - ay) + ax {
                            !inside
                        } else {
                            inside
                        }
                    });
                if inside {
                    // Fill based on load: bottom portion lit, top dimmer
                    let rel_y = (fy - (cy - hex_r)) / (hex_r * 2.0); // 0=top, 1=bottom
                    let lit = rel_y >= (1.0 - load);
                    let col = if lit { fill_col } else { SURFACE0 };
                    img.put_pixel(px, py, col);
                }
            }
        }

        // Hexagon outline
        for k in 0..6 {
            let (x0, y0) = verts[k];
            let (x1, y1) = verts[(k + 1) % 6];
            draw_antialiased_line_segment_mut(
                &mut img,
                (x0 as i32, y0 as i32),
                (x1 as i32, y1 as i32),
                OVERLAY0,
                interpolate,
            );
        }

        // Core number (small)
        if hex_r > 18.0 {
            let lbl = format!("{i}");
            let lw = text_width(&font, PxScale::from(11.0), &lbl);
            draw_text_mut(
                &mut img,
                TEXT,
                cx as i32 - lw / 2,
                cy as i32 - 6,
                PxScale::from(11.0),
                &font,
                &lbl,
            );
        }
    }

    // Title
    let avg_load: f32 = cpus.iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / n_cores as f32;
    let title = format!("CPU Hex Grid — {n_cores} cores  avg {avg_load:.0}%");
    let tw = text_width(&font, PxScale::from(18.0), &title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        8,
        PxScale::from(18.0),
        &font,
        &title,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Process Bubble Chart ─────────────────────────────────────────────────────

/// Top processes by CPU usage visualised as bubbles.
/// Bubble area ∝ CPU%, colour = hash of process name.
pub fn render_bubble_chart() -> DynamicImage {
    use sysinfo::ProcessesToUpdate;

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Collect top-N processes by CPU%
    let mut procs: Vec<(String, f32)> = sys
        .processes()
        .values()
        .filter(|p| p.cpu_usage() > 0.1)
        .map(|p| (p.name().to_string_lossy().to_string(), p.cpu_usage()))
        .collect();
    procs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(20);

    if procs.is_empty() {
        draw_centered(
            &mut img,
            &font,
            PxScale::from(22.0),
            220,
            OVERLAY0,
            "No active processes",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let max_cpu = procs[0].1.max(1.0);
    let pal: [Rgb<u8>; 7] = [BLUE, GREEN, MAUVE, TEAL, PEACH, RED, YELLOW];

    // Pack bubbles using a simple grid layout with radius ∝ sqrt(cpu)
    let title_h = 30i32;
    let area_y = title_h;
    let _area_h = 480 - title_h - 10;

    // Scale max bubble to ~80px radius
    let scale = 80.0f32 / max_cpu.sqrt();

    // Sort and place bubbles left-to-right, top-to-bottom
    let mut x_cursor = 0i32;
    let mut y_cursor = area_y;
    let mut row_max_r = 0i32;

    for (idx, (name, cpu)) in procs.iter().enumerate() {
        let r = ((cpu.sqrt() * scale) as i32).max(10);
        if x_cursor + r * 2 > 480 {
            x_cursor = 0;
            y_cursor += row_max_r * 2 + 8;
            row_max_r = 0;
        }
        if y_cursor + r * 2 > 480 {
            break;
        }

        let cx = (x_cursor + r) as f32;
        let cy = (y_cursor + r) as f32;
        x_cursor += r * 2 + 6;
        row_max_r = row_max_r.max(r);

        // Colour by name hash
        let hash: usize = name
            .bytes()
            .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
        let fill = pal[hash % pal.len()];

        // Draw filled circle
        let r2 = r * r;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if !(0..480).contains(&px) || !(0..480).contains(&py) {
                    continue;
                }
                // Radial brightness: centre bright, edge dim
                let dist_frac = ((dx * dx + dy * dy) as f32).sqrt() / r as f32;
                let br = 1.0 - dist_frac * 0.35;
                img.put_pixel(
                    px as u32,
                    py as u32,
                    Rgb([
                        (fill[0] as f32 * br) as u8,
                        (fill[1] as f32 * br) as u8,
                        (fill[2] as f32 * br) as u8,
                    ]),
                );
            }
        }

        // Label inside large bubbles
        if r >= 24 {
            let short = if name.len() > 8 {
                &name[..8]
            } else {
                name.as_str()
            };
            let lw = text_width(&font, PxScale::from(11.0), short);
            draw_text_mut(
                &mut img,
                BG,
                cx as i32 - lw / 2,
                cy as i32 - 12,
                PxScale::from(11.0),
                &font,
                short,
            );
            let pct = format!("{cpu:.0}%");
            let pw = text_width(&font, PxScale::from(11.0), &pct);
            draw_text_mut(
                &mut img,
                BG,
                cx as i32 - pw / 2,
                cy as i32 + 2,
                PxScale::from(11.0),
                &font,
                &pct,
            );
        }

        // Rank badge top-left if idx < 3
        if idx < 3 {
            let badge = ["①", "②", "③"][idx];
            draw_text_mut(
                &mut img,
                TEXT,
                cx as i32 - r + 2,
                cy as i32 - r + 2,
                PxScale::from(12.0),
                &font,
                badge,
            );
        }
    }

    // Title bar
    draw_filled_rect_mut(
        &mut img,
        Rect::at(0, 0).of_size(480, title_h as u32),
        SURFACE0,
    );
    draw_text_mut(
        &mut img,
        TEXT,
        8,
        6,
        PxScale::from(18.0),
        &font,
        "Process CPU Bubbles",
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        320,
        8,
        PxScale::from(14.0),
        &font,
        &format!("{} procs", procs.len()),
    );

    DynamicImage::ImageRgb8(img)
}

// ─── 3-D Isometric CPU Bars ───────────────────────────────────────────────────

/// Per-core CPU usage displayed as 3D isometric bars.
/// Green = low, yellow = mid, orange = high, red = critical.
pub fn render_cpu_3d_bars() -> DynamicImage {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let cpus = sys.cpus();
    let n = cpus.len().max(1);
    let font = font_bold();

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Isometric projection helpers
    // Grid layout: up to 16 bars across, multiple rows
    let cols = ((n as f32).sqrt().ceil() as usize).clamp(1, 12);
    let rows = n.div_ceil(cols);

    let pad_x = 16.0f32;
    let pad_y = 60.0f32; // top margin for title
    let cell_w = (480.0 - pad_x * 2.0) / cols as f32;
    let cell_d = (480.0 - pad_y - 30.0) / (rows as f32 + 0.5) * 0.65; // depth per row
    let max_h = cell_d * 2.0; // max bar height in pixels

    // Iso unit vectors: right (+X), back (+Y), up (+Z)
    let iso_x = |gx: f32, gy: f32| -> f32 { pad_x + gx * cell_w + gy * cell_w * 0.4 };
    let iso_y = |_gx: f32, gy: f32, gz: f32| -> f32 {
        pad_y + gy * cell_d - gz * (max_h / 1.0) + (rows as f32) * cell_d * 0.3
    };

    // Helper: draw a filled parallelogram (quad) top/side face
    let draw_quad = |img: &mut RgbImage, pts: [(f32, f32); 4], col: Rgb<u8>| {
        // Scan-fill between the two edge pairs (0-1 and 3-2)
        let (y_min, y_max) = {
            let ys = [pts[0].1, pts[1].1, pts[2].1, pts[3].1];
            (
                ys.iter().copied().fold(f32::MAX, f32::min) as i32,
                ys.iter().copied().fold(f32::MIN, f32::max) as i32,
            )
        };
        for py in y_min.max(0)..=y_max.min(479) {
            let yf = py as f32;
            // Find x-intersections with all 4 edges
            let mut xs: Vec<f32> = Vec::new();
            for k in 0..4 {
                let (ax, ay) = pts[k];
                let (bx, by) = pts[(k + 1) % 4];
                if (ay <= yf && by > yf) || (by <= yf && ay > yf) {
                    xs.push(ax + (bx - ax) * (yf - ay) / (by - ay));
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if xs.len() >= 2 {
                let x0 = xs[0] as i32;
                let x1 = xs[xs.len() - 1] as i32;
                for px in x0.max(0)..=x1.min(479) {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    };

    for (i, cpu) in cpus.iter().enumerate() {
        let load = cpu.cpu_usage() / 100.0;
        let col_ = i % cols;
        let row_ = i / cols;

        let gx = col_ as f32;
        let gy = row_ as f32;
        let bar_h = (load * max_h).max(2.0);

        // Four 3D corners of the bar:
        //   bl=back-left, br=back-right, fl=front-left, fr=front-right
        let w = cell_w * 0.70;
        let d = cell_d * 0.65;
        let fx0 = iso_x(gx + 0.15, gy);
        let fx1 = iso_x(gx + 0.15 + w / cell_w, gy);
        let bx0 = iso_x(gx + 0.15, gy + d / cell_d);
        let bx1 = iso_x(gx + 0.15 + w / cell_w, gy + d / cell_d);
        let fy0 = iso_y(gx, gy, 0.0);
        let fy1 = iso_y(gx, gy, bar_h / max_h);
        let by0 = iso_y(gx, gy + d / cell_d, 0.0);
        let by1 = iso_y(gx, gy + d / cell_d, bar_h / max_h);

        let fill = if load < 0.4 {
            GREEN
        } else if load < 0.7 {
            YELLOW
        } else if load < 0.9 {
            PEACH
        } else {
            RED
        };
        let dark = Rgb([
            (fill[0] as u32 * 6 / 10) as u8,
            (fill[1] as u32 * 6 / 10) as u8,
            (fill[2] as u32 * 6 / 10) as u8,
        ]);
        let side = Rgb([
            (fill[0] as u32 * 8 / 10) as u8,
            (fill[1] as u32 * 8 / 10) as u8,
            (fill[2] as u32 * 8 / 10) as u8,
        ]);

        // Draw front face (brightest)
        draw_quad(
            &mut img,
            [(fx0, fy0), (fx1, fy0), (fx1, fy1), (fx0, fy1)],
            fill,
        );
        // Draw top face (medium)
        draw_quad(
            &mut img,
            [(fx0, fy1), (fx1, fy1), (bx1, by1), (bx0, by1)],
            side,
        );
        // Draw right face (darkest)
        draw_quad(
            &mut img,
            [(fx1, fy0), (bx1, by0), (bx1, by1), (fx1, fy1)],
            dark,
        );

        // Core label
        if w > 18.0 {
            let lbl = format!("{i}");
            let lw = text_width(&font, PxScale::from(10.0), &lbl);
            draw_text_mut(
                &mut img,
                BG,
                (fx0 + (fx1 - fx0) / 2.0 - lw as f32 / 2.0) as i32,
                (fy0 + 2.0) as i32,
                PxScale::from(10.0),
                &font,
                &lbl,
            );
        }
    }

    // Title
    let avg: f32 = cpus.iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / n as f32;
    let title = format!("3D CPU Bars — {n} cores  avg {avg:.0}%");
    let tw = text_width(&font, PxScale::from(18.0), &title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        8,
        PxScale::from(18.0),
        &font,
        &title,
    );

    DynamicImage::ImageRgb8(img)
}

// ─────────────────────────────────────────────────────────────────────────────
// render_waveform_bars  — CPU-core frequency bars like an audio visualiser
// ─────────────────────────────────────────────────────────────────────────────
pub(super) static WAVEFORM_HISTORY: std::sync::OnceLock<
    std::sync::Mutex<Vec<std::collections::VecDeque<f32>>>,
> = std::sync::OnceLock::new();

pub fn render_waveform_bars() -> DynamicImage {
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    let cpus = sys.cpus();
    let n_cores = cpus.len().clamp(1, 16);
    let core_loads: Vec<f32> = cpus
        .iter()
        .take(n_cores)
        .map(|c| c.cpu_usage() / 100.0)
        .collect();

    let state = WAVEFORM_HISTORY.get_or_init(|| {
        std::sync::Mutex::new(vec![std::collections::VecDeque::with_capacity(240); 16])
    });
    let mut hist = state.lock().unwrap();
    // Ensure we have enough histories
    while hist.len() < n_cores {
        hist.push(std::collections::VecDeque::with_capacity(240));
    }

    for (i, &load) in core_loads.iter().enumerate() {
        if hist[i].len() >= 240 {
            hist[i].pop_front();
        }
        hist[i].push_back(load);
    }

    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    let cols: [Rgb<u8>; 8] = [
        BLUE,
        GREEN,
        MAUVE,
        TEAL,
        YELLOW,
        PEACH,
        RED,
        Rgb([0x89, 0xdc, 0xeb]),
    ];

    let bar_area_h = 360u32;
    let bar_area_top = 60u32;
    let bar_w = (480 / n_cores as u32).min(60);
    let gap = 4u32;

    // Draw scrolling history for each core
    for (ci, core_hist) in hist.iter().take(n_cores).enumerate() {
        let col = cols[ci % cols.len()];
        let x_start = ci as u32 * bar_w;

        // Bar fill (current value)
        let load = core_hist.back().copied().unwrap_or(0.0);
        let bar_h = (load * bar_area_h as f32) as u32;
        for py in (bar_area_top + bar_area_h - bar_h)..(bar_area_top + bar_area_h) {
            for px in (x_start + gap)..(x_start + bar_w - gap) {
                // Gradient: brighter at top
                let t = (py - (bar_area_top + bar_area_h - bar_h)) as f32 / bar_h.max(1) as f32;
                let brightness = 1.0 - t * 0.5;
                img.put_pixel(
                    px,
                    py,
                    Rgb([
                        (col[0] as f32 * brightness) as u8,
                        (col[1] as f32 * brightness) as u8,
                        (col[2] as f32 * brightness) as u8,
                    ]),
                );
            }
        }

        // Mini history waveform above the bar
        let wave_h = 50u32;
        let wave_y_base = bar_area_top - 4;
        let wave_len = core_hist.len() as u32;
        if wave_len > 1 {
            for i in 1..wave_len {
                let v_prev = core_hist[i as usize - 1];
                let v_curr = core_hist[i as usize];
                let x0 = x_start + gap + (i - 1) * (bar_w - gap * 2) / wave_len;
                let x1 = x_start + gap + i * (bar_w - gap * 2) / wave_len;
                let y0 = wave_y_base - (v_prev * wave_h as f32) as u32;
                let y1 = wave_y_base - (v_curr * wave_h as f32) as u32;
                if x0 < 480 && x1 < 480 && y0 < 480 && y1 < 480 {
                    img.put_pixel(x0, y0, Rgb([(col[0] / 2), (col[1] / 2), (col[2] / 2)]));
                    img.put_pixel(x1, y1, col);
                }
            }
        }

        // Core label
        let font = font_bold();
        let label = format!("C{ci}");
        let lx = (x_start + 4) as i32;
        draw_text_mut(
            &mut img,
            SUBTEXT,
            lx,
            (bar_area_top + bar_area_h + 4) as i32,
            PxScale::from(12.0),
            &font,
            &label,
        );
        let pct = (load * 100.0) as u8;
        draw_text_mut(
            &mut img,
            col,
            lx,
            (bar_area_top + bar_area_h + 18) as i32,
            PxScale::from(12.0),
            &font,
            &format!("{pct}%"),
        );
    }

    // Title
    let font = font_bold();
    draw_text_mut(
        &mut img,
        TEXT,
        8,
        8,
        PxScale::from(22.0),
        &font,
        &format!("CPU Cores ×{n_cores}"),
    );

    DynamicImage::ImageRgb8(img)
}

// ── System Art ────────────────────────────────────────────────────────────────
// CPU, memory, and load usage rendered as artistic circular gauges.
pub fn render_system_art() -> DynamicImage {
    use sysinfo::{CpuRefreshKind, RefreshKind, System};

    let cpu_pct = {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
        );
        sys.refresh_cpu_usage();
        // Small sleep to get accurate reading would need async context; use cached value
        sys.global_cpu_usage()
    };
    let (mem_pct, load_1) = {
        let mut sys = System::new_all();
        sys.refresh_memory();
        let mem = if sys.total_memory() > 0 {
            sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
        } else {
            0.0
        };
        let load = System::load_average();
        (mem, load.one as f32)
    };

    let mut img = RgbImage::new(480, 480);
    let bg = Rgb([14u8, 12, 20]);
    for p in img.pixels_mut() {
        *p = bg;
    }

    // Draw circular gauge helper
    let draw_ring =
        |img: &mut RgbImage, cx: f32, cy: f32, r: f32, fill: f32, hue: f32, label: &str| {
            let _ = label;
            let arc_steps = 400usize;
            let full_angle = std::f32::consts::TAU;
            let filled_angle = full_angle * fill.clamp(0.0, 1.0);
            let start_a = -std::f32::consts::FRAC_PI_2;

            // Background ring (dim)
            for si in 0..arc_steps {
                let a = start_a + si as f32 / arc_steps as f32 * full_angle;
                for ri in -4i32..=4 {
                    let r2 = r + ri as f32;
                    let bx = (cx + a.cos() * r2) as i32;
                    let by = (cy + a.sin() * r2) as i32;
                    if (0..480).contains(&bx) && (0..480).contains(&by) {
                        let p = img.get_pixel(bx as u32, by as u32);
                        img.put_pixel(
                            bx as u32,
                            by as u32,
                            Rgb([
                                (p[0] as f32 + 20.0).min(255.0) as u8,
                                (p[1] as f32 + 20.0).min(255.0) as u8,
                                (p[2] as f32 + 25.0).min(255.0) as u8,
                            ]),
                        );
                    }
                }
            }

            // Filled arc
            let filled_steps = (arc_steps as f32 * fill.clamp(0.0, 1.0)) as usize;
            for si in 0..filled_steps {
                let a = start_a + si as f32 / arc_steps as f32 * filled_angle / fill.max(0.001);
                let t = si as f32 / filled_steps.max(1) as f32;
                let lit = 0.45 + t * 0.2;
                let col = hsl_to_rgb((hue + t * 0.15) % 1.0, 0.9, lit);
                for ri in -4i32..=4 {
                    let r2 = r + ri as f32;
                    let bx = (cx + a.cos() * r2) as i32;
                    let by = (cy + a.sin() * r2) as i32;
                    if (0..480).contains(&bx) && (0..480).contains(&by) {
                        img.put_pixel(bx as u32, by as u32, col);
                    }
                }
            }
        };

    // Three gauges: CPU (left), MEM (right), Load (bottom)
    draw_ring(&mut img, 160.0, 180.0, 90.0, cpu_pct / 100.0, 0.0, "CPU");
    draw_ring(&mut img, 320.0, 180.0, 90.0, mem_pct / 100.0, 0.3, "MEM");
    draw_ring(
        &mut img,
        240.0,
        360.0,
        80.0,
        (load_1 / 4.0).min(1.0),
        0.55,
        "LOAD",
    );

    // Labels using draw_text_mut
    use imageproc::drawing::draw_text_mut;
    let font = font_bold();
    let scale_sm = ab_glyph::PxScale { x: 18.0, y: 18.0 };
    let scale_val = ab_glyph::PxScale { x: 22.0, y: 22.0 };

    let white = Rgb([220u8, 215, 235]);
    let grey = Rgb([130u8, 125, 150]);

    // CPU
    draw_text_mut(&mut img, grey, 130, 160, scale_sm, &font, "CPU");
    draw_text_mut(
        &mut img,
        white,
        128,
        178,
        scale_val,
        &font,
        &format!("{cpu_pct:.0}%"),
    );

    // MEM
    draw_text_mut(&mut img, grey, 293, 160, scale_sm, &font, "MEM");
    draw_text_mut(
        &mut img,
        white,
        288,
        178,
        scale_val,
        &font,
        &format!("{mem_pct:.0}%"),
    );

    // LOAD
    draw_text_mut(&mut img, grey, 212, 345, scale_sm, &font, "LOAD");
    draw_text_mut(
        &mut img,
        white,
        208,
        363,
        scale_val,
        &font,
        &format!("{load_1:.2}"),
    );

    DynamicImage::ImageRgb8(img)
}

// ─── EKG rhythm strip ────────────────────────────────────────────────────────

/// Heart rate shown by the EKG strip with the CPU idle.
pub const EKG_REST_BPM: f64 = 70.0;
/// Heart rate shown by the EKG strip with every core fully loaded.
pub const EKG_LOAD_BPM: f64 = 160.0;

/// Lead-II style ECG voltage (mV) at `dt` seconds from the R peak of a beat
/// whose R-R interval is `rr` seconds: P wave, QRS complex and T wave as a
/// sum of Gaussians.  As in a real heart, the QT and PR intervals shorten at
/// faster rates so the T wave doesn't run into the next P wave.
pub(super) fn ekg_beat_mv(dt: f64, rr: f64) -> f64 {
    let qt = (rr / 0.857).sqrt().clamp(0.6, 1.1); // Bazett-style scaling
    let pr = 0.7 + 0.3 * qt;
    // (amplitude mV, centre s, width s)
    let waves = [
        (0.16, -0.180 * pr, 0.026 * pr), // P
        (-0.11, -0.036, 0.009),          // Q
        (1.15, 0.000, 0.011),            // R
        (-0.24, 0.034, 0.011),           // S
        (0.32, 0.250 * qt, 0.052 * qt),  // T
    ];
    waves
        .iter()
        .map(|&(amp, centre, width)| amp * (-((dt - centre) / width).powi(2) / 2.0).exp())
        .sum()
}

/// Heart rate the EKG strip aims for at `cpu_pct` percent overall CPU usage:
/// linear from [`EKG_REST_BPM`] at 0 % to [`EKG_LOAD_BPM`] at 100 %.
pub(super) fn ekg_target_bpm(cpu_pct: f64) -> f64 {
    EKG_REST_BPM + (EKG_LOAD_BPM - EKG_REST_BPM) * cpu_pct.clamp(0.0, 100.0) / 100.0
}

/// EKG rhythm strip screensaver: a sinus rhythm (P wave, QRS complex, T wave)
/// scrolling across ECG paper at the standard 25 mm/s, 10 mm/mV.  The heart
/// rate follows system load, from [`EKG_REST_BPM`] at 0 % CPU to
/// [`EKG_LOAD_BPM`] at 100 %.
pub fn render_ekg_strip() -> DynamicImage {
    const MM: f64 = 6.0; // pixels per millimetre of paper
    const PX_PER_SEC: f64 = 25.0 * MM;
    const PX_PER_MV: f64 = 10.0 * MM;
    const BASELINE: f64 = 300.0;

    struct EkgState {
        start: std::time::Instant,
        /// (R-peak time in seconds since `start`, R-R interval), oldest first.
        beats: std::collections::VecDeque<(f64, f64)>,
        seed: u32,
        /// Kept alive between frames: CPU usage is a delta between refreshes.
        sys: System,
        cpu_sampled_at: f64,
        cpu_pct: f64,
        /// Current heart rate, easing towards the rate the load calls for.
        bpm: f64,
        last_frame: f64,
    }
    static EKG_STATE: OnceLock<std::sync::Mutex<EkgState>> = OnceLock::new();
    let mut guard = EKG_STATE
        .get_or_init(|| {
            let mut sys = System::new();
            sys.refresh_cpu_usage();
            std::sync::Mutex::new(EkgState {
                start: std::time::Instant::now(),
                beats: std::collections::VecDeque::from([(0.4, 60.0 / EKG_REST_BPM)]),
                seed: 0x0EC6_BEA7,
                sys,
                cpu_sampled_at: 0.0,
                cpu_pct: 0.0,
                bpm: EKG_REST_BPM,
                last_frame: 0.0,
            })
        })
        .lock()
        .unwrap();
    let st = &mut *guard;

    let now = st.start.elapsed().as_secs_f64();
    let window = 480.0 / PX_PER_SEC;

    // Sample CPU load twice a second and ease the heart rate towards its
    // target over a few seconds, the way a real pulse ramps up and settles.
    if now - st.cpu_sampled_at >= 0.5 {
        st.sys.refresh_cpu_usage();
        st.cpu_pct = st.sys.global_cpu_usage().clamp(0.0, 100.0) as f64;
        st.cpu_sampled_at = now;
    }
    let target = ekg_target_bpm(st.cpu_pct);
    let dt = (now - st.last_frame).clamp(0.0, 1.0);
    st.last_frame = now;
    st.bpm += (target - st.bpm) * (1.0 - (-dt / 2.5).exp());

    // Schedule beats a little past "now", with mild sinus arrhythmia.
    while st.beats.back().unwrap().0 < now + 1.0 {
        st.seed = st.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let jitter = (st.seed >> 8) as f64 / (1u32 << 24) as f64 - 0.5;
        let last = st.beats.back().unwrap().0;
        let rr = 60.0 / st.bpm
            * (1.0 + 0.03 * (last * 0.25 * std::f64::consts::TAU).sin() + 0.02 * jitter);
        st.beats.push_back((last + rr, rr));
    }
    while st.beats.len() > 2 && st.beats[1].0 < now - window - 1.0 {
        st.beats.pop_front();
    }

    let beats = &st.beats;
    let mv_at = |t: f64| -> f64 {
        beats
            .iter()
            .filter(|(b, _)| (t - b).abs() < 0.6)
            .map(|&(b, rr)| ekg_beat_mv(t - b, rr))
            .sum()
    };

    // Paper: 1 mm minor and 5 mm major grid, scrolling with the trace.
    const PAPER: Rgb<u8> = Rgb([255, 245, 240]);
    const MINOR: Rgb<u8> = Rgb([248, 208, 204]);
    const MAJOR: Rgb<u8> = Rgb([234, 150, 150]);
    const INK: Rgb<u8> = Rgb([28, 28, 36]);
    let scroll = (now * PX_PER_SEC) as i64;
    let mut img = RgbImage::from_pixel(480, 480, PAPER);
    for (step, color) in [(MM as i64, MINOR), (5 * MM as i64, MAJOR)] {
        for x in 0..480i64 {
            if (x + scroll).rem_euclid(step) == 0 {
                for y in 0..480 {
                    img.put_pixel(x as u32, y, color);
                }
            }
        }
        for y in (0..480i64).filter(|y| y % step == 0) {
            for x in 0..480 {
                img.put_pixel(x, y as u32, color);
            }
        }
    }

    // Trace: the newest sample is at the right-hand edge.
    let y_at = |x: i32| (BASELINE - mv_at(now - (479 - x) as f64 / PX_PER_SEC) * PX_PER_MV) as f32;
    let mut prev = y_at(0);
    for x in 1..480 {
        let y = y_at(x);
        for (ox, oy) in [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (0.0, -1.0)] {
            imageproc::drawing::draw_line_segment_mut(
                &mut img,
                ((x - 1) as f32 + ox, prev + oy),
                (x as f32 + ox, y + oy),
                INK,
            );
        }
        prev = y;
    }

    // Labels, as printed along the top of a rhythm strip.
    let bpm = beats
        .iter()
        .rev()
        .find(|(b, _)| *b <= now)
        .map_or(st.bpm, |&(_, rr)| 60.0 / rr)
        .round() as u32;
    let font = font_bold();
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 44), PAPER);
    draw_text_mut(&mut img, INK, 14, 10, PxScale::from(26.0), &font, "II");
    draw_text_mut(
        &mut img,
        INK,
        62,
        15,
        PxScale::from(17.0),
        &font,
        "25 mm/s   10 mm/mV",
    );
    draw_text_mut(
        &mut img,
        INK,
        262,
        15,
        PxScale::from(17.0),
        &font,
        &format!("CPU {:.0}%", st.cpu_pct),
    );
    draw_text_mut(
        &mut img,
        Rgb([200, 40, 60]),
        356,
        10,
        PxScale::from(26.0),
        &font,
        &format!("HR {bpm}"),
    );

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod ekg_tests {
    use super::*;

    /// Resting R-R interval.
    const REST_RR: f64 = 60.0 / EKG_REST_BPM;

    #[test]
    fn heart_rate_tracks_cpu_load_between_rest_and_load_rates() {
        assert_eq!(ekg_target_bpm(0.0), 70.0);
        assert_eq!(ekg_target_bpm(100.0), 160.0);
        assert_eq!(ekg_target_bpm(50.0), 115.0);
        // Out-of-range readings are clamped rather than extrapolated.
        assert_eq!(ekg_target_bpm(-5.0), 70.0);
        assert_eq!(ekg_target_bpm(250.0), 160.0);
        assert!((0..100).all(|p| ekg_target_bpm(p as f64) < ekg_target_bpm(p as f64 + 1.0)));
    }

    #[test]
    fn beat_has_p_wave_qrs_complex_and_t_wave_in_order() {
        let mv = |dt: f64| ekg_beat_mv(dt, REST_RR);
        // R is the tall spike at the reference point…
        assert!((mv(0.0) - 1.15).abs() < 0.05);
        // …flanked by the small negative Q and S deflections.
        assert!(mv(-0.036) < -0.05);
        assert!(mv(0.034) < -0.15);
        // P is a small positive bump before the QRS, T a broader one after it.
        let p = mv(-0.18);
        let t = mv(0.25);
        assert!((0.1..0.25).contains(&p), "P = {p}");
        assert!((0.25..0.4).contains(&t), "T = {t}");
        // Flat baseline between T and the next P, and between P and Q.
        for dt in [-0.4, -0.09, 0.45, 0.55] {
            assert!(mv(dt).abs() < 0.01, "baseline at {dt} is {}", mv(dt));
        }
    }

    #[test]
    fn r_wave_is_the_largest_deflection_of_the_beat() {
        let peak = (-400..600)
            .map(|ms| ms as f64 / 1000.0)
            .max_by(|a, b| ekg_beat_mv(*a, REST_RR).total_cmp(&ekg_beat_mv(*b, REST_RR)))
            .unwrap();
        assert!(
            peak.abs() < 0.002,
            "largest deflection at {peak}s, expected the R peak at 0"
        );
    }

    #[test]
    fn intervals_shorten_at_fast_rates_so_t_clears_the_next_p() {
        let fast_rr = 60.0 / EKG_LOAD_BPM;
        // The T wave peaks earlier when the heart is racing.
        let t_peak = |rr: f64| {
            (100..400)
                .map(|ms| ms as f64 / 1000.0)
                .max_by(|a, b| ekg_beat_mv(*a, rr).total_cmp(&ekg_beat_mv(*b, rr)))
                .unwrap()
        };
        assert!(t_peak(fast_rr) < t_peak(REST_RR) - 0.05);
        // Two consecutive fast beats: the trace returns close to baseline
        // somewhere between this beat's T and the next beat's P.
        let two_beats = |t: f64| ekg_beat_mv(t, fast_rr) + ekg_beat_mv(t - fast_rr, fast_rr);
        let trough = (0..=((fast_rr - 0.1) * 1000.0) as i32)
            .map(|ms| two_beats(0.1 + ms as f64 / 1000.0))
            .fold(f64::MAX, f64::min);
        assert!(
            trough < 0.12,
            "T and next P merge: minimum between them is {trough} mV"
        );
    }

    #[test]
    fn strip_renders_a_panel_sized_frame_with_a_visible_trace() {
        let img = render_ekg_strip().to_rgb8();
        assert_eq!((img.width(), img.height()), (480, 480));
        let ink = img.pixels().filter(|p| p.0.iter().all(|&v| v < 60)).count();
        assert!(
            ink > 480,
            "expected a trace across the strip, found {ink} dark pixels"
        );
    }
}
