//! The original screens: system summary, clock, test card, CPU, network, disk, processes, temperature, memory, countdown and custom text.

use super::*;

// ─── Public renderers ─────────────────────────────────────────────────────────

/// Live system-stats dashboard: CPU · RAM · temperature · uptime.
pub fn render_sysinfo() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // ── Collect stats ──────────────────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let hostname = System::host_name().unwrap_or_else(|| "unknown".into());

    // CPU
    let cpu_usage = sys.global_cpu_usage();
    let cpu_name = sys
        .cpus()
        .first()
        .map_or_else(|| "CPU".into(), |c| c.brand().to_string());
    let cpu_name = if cpu_name.len() > 28 {
        format!("{}…", &cpu_name[..27])
    } else {
        cpu_name
    };

    // Memory
    let used_mem = sys.used_memory();
    let total_mem = sys.total_memory();
    let mem_frac = if total_mem > 0 {
        used_mem as f32 / total_mem as f32
    } else {
        0.0
    };
    let used_gb = used_mem as f32 / 1_073_741_824.0;
    let total_gb = total_mem as f32 / 1_073_741_824.0;

    // Load average
    let load = System::load_average();
    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);

    // Uptime
    let uptime_str = format_uptime(System::uptime());

    // Time
    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %d, %Y").to_string();

    // Temperature
    let mut comps = sysinfo::Components::new_with_refreshed_list();
    comps.refresh(false);
    let temp_opt: Option<f32> = comps
        .iter()
        .filter(|c| {
            let lbl = c.label().to_lowercase();
            lbl.contains("cpu") || lbl.contains("core") || lbl.contains("package")
        })
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);

    // ── Render ────────────────────────────────────────────────────────────
    let mut y: i32 = 14;

    let big = PxScale::from(32.0);
    draw_text_mut(&mut img, MAUVE, 20, y, big, &bold, &hostname);
    let tw = text_width(&mono, big, &time_str);
    draw_text_mut(&mut img, TEAL, 480 - 20 - tw, y, big, &mono, &time_str);
    y += 38;

    let sm = PxScale::from(20.0);
    draw_text_mut(&mut img, SUBTEXT, 20, y, sm, &bold, &date_str);
    y += 28;

    draw_divider(&mut img, y);
    y += 10;

    let lbl = PxScale::from(22.0);
    let val = PxScale::from(22.0);

    // ── CPU ────────────────────────────────────────────────────────────────
    draw_kv(
        &mut img,
        &bold,
        &mono,
        lbl,
        y,
        BLUE,
        TEXT,
        "CPU",
        &format!("{cpu_usage:.1}%"),
    );
    y += 28;
    draw_bar(&mut img, 20, y, 440, 18, cpu_usage / 100.0, BLUE);
    y += 24;
    draw_text_mut(
        &mut img,
        OVERLAY0,
        20,
        y,
        PxScale::from(17.0),
        &bold,
        &cpu_name,
    );
    y += 24;

    draw_divider(&mut img, y);
    y += 10;

    // ── RAM ────────────────────────────────────────────────────────────────
    draw_kv(
        &mut img,
        &bold,
        &mono,
        val,
        y,
        GREEN,
        TEXT,
        "RAM",
        &format!("{used_gb:.1} / {total_gb:.1} GB"),
    );
    y += 28;
    draw_bar(&mut img, 20, y, 440, 18, mem_frac, GREEN);
    y += 28;

    draw_divider(&mut img, y);
    y += 10;

    // ── Temperature ────────────────────────────────────────────────────────
    if let Some(temp) = temp_opt {
        let col = if temp > 80.0 {
            RED
        } else if temp > 65.0 {
            YELLOW
        } else {
            GREEN
        };
        draw_kv(
            &mut img,
            &bold,
            &mono,
            lbl,
            y,
            YELLOW,
            TEXT,
            "TEMP",
            &format!("{temp:.0}°C"),
        );
        y += 28;
        draw_bar(&mut img, 20, y, 440, 18, (temp / 100.0).min(1.0), col);
    } else {
        draw_kv(
            &mut img, &bold, &mono, lbl, y, YELLOW, OVERLAY0, "TEMP", "N/A",
        );
        y += 28;
        draw_bar(&mut img, 20, y, 440, 18, 0.0, OVERLAY0);
    }
    y += 28;

    draw_divider(&mut img, y);
    y += 12;

    // ── Load average + uptime ──────────────────────────────────────────────
    let s2 = PxScale::from(18.0);
    draw_text_mut(&mut img, OVERLAY0, 20, y, s2, &bold, "Load avg");
    let lw = text_width(&mono, s2, &load_str);
    draw_text_mut(&mut img, SUBTEXT, 480 - 20 - lw, y, s2, &mono, &load_str);
    y += 24;
    draw_text_mut(
        &mut img,
        SUBTEXT,
        20,
        y,
        PxScale::from(18.0),
        &bold,
        &format!("Uptime  {uptime_str}"),
    );

    DynamicImage::ImageRgb8(img)
}

/// Large digital clock with date and uptime.
pub fn render_clock() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let wday_str = now.format("%A").to_string();
    let date_str = now.format("%B %-e, %Y").to_string();
    let up_str = format!("up {}", format_uptime(System::uptime()));

    // Load avg
    let load = System::load_average();
    let load_str = format!(
        "load  {:.2}  {:.2}  {:.2}",
        load.one, load.five, load.fifteen
    );

    // Large clock
    let clock_scale = PxScale::from(96.0);
    let tw = text_width(&mono, clock_scale, &time_str);
    let tx = (480 - tw).max(0) / 2;
    draw_text_mut(&mut img, TEAL, tx, 130, clock_scale, &mono, &time_str);

    draw_divider(&mut img, 244);

    draw_centered(&mut img, &bold, PxScale::from(36.0), 258, MAUVE, &wday_str);
    draw_centered(&mut img, &bold, PxScale::from(28.0), 302, TEXT, &date_str);
    draw_centered(&mut img, &bold, PxScale::from(22.0), 348, SUBTEXT, &up_str);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(20.0),
        382,
        OVERLAY0,
        &load_str,
    );

    // Corner bracket decorations
    draw_brackets(&mut img);

    DynamicImage::ImageRgb8(img)
}

/// SMPTE-style colour bars + cross-hatch overlay.
pub fn render_test_card() -> DynamicImage {
    let mut img = RgbImage::new(480, 480);

    let bars: &[Rgb<u8>] = &[
        Rgb([192, 192, 192]),
        Rgb([192, 192, 0]),
        Rgb([0, 192, 192]),
        Rgb([0, 192, 0]),
        Rgb([192, 0, 192]),
        Rgb([192, 0, 0]),
        Rgb([0, 0, 192]),
    ];
    let bar_w = 480 / bars.len() as u32;
    let bar_h = 380u32;

    for (i, &color) in bars.iter().enumerate() {
        let x0 = i as u32 * bar_w;
        let x1 = (x0 + bar_w).min(480);
        for x in x0..x1 {
            for y in 0..bar_h {
                img.put_pixel(x, y, color);
            }
        }
    }

    for x in 0..480u32 {
        let v = if x < 160 {
            0u8
        } else if x < 320 {
            ((x - 160) * 255 / 160) as u8
        } else {
            255u8.saturating_sub(((x - 320) * 255 / 160) as u8)
        };
        for y in bar_h..480 {
            img.put_pixel(x, y, Rgb([v, v, v]));
        }
    }

    for i in (0u32..960).step_by(40) {
        for t in 0..2u32 {
            let xi = (i + t) as i32 - 480;
            let x0 = xi.max(0) as u32;
            let x1 = (xi + 480).min(480) as u32;
            for x in x0..x1 {
                let y = (xi + x as i32) as u32;
                if y < bar_h {
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                }
            }
        }
    }

    let font = font_bold();
    draw_text_mut(
        &mut img,
        Rgb([255, 255, 255]),
        100,
        390,
        PxScale::from(20.0),
        &font,
        "480 × 480  TEST CARD",
    );

    DynamicImage::ImageRgb8(img)
}

/// Solid colour frame.
pub fn render_solid(r: u8, g: u8, b: u8) -> DynamicImage {
    let mut img = RgbImage::new(480, 480);
    for px in img.pixels_mut() {
        *px = Rgb([r, g, b]);
    }
    DynamicImage::ImageRgb8(img)
}

/// Load all images from a directory (sorted) for slideshow use.
pub fn load_slideshow_frames(dir: &Path) -> Vec<DynamicImage> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("Cannot read '{}': {}", dir.display(), e))
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "gif"
            )
        })
        .collect();
    paths.sort();

    paths
        .iter()
        .filter_map(|p| match crate::open_image(p) {
            Ok(img) => {
                log::debug!("slideshow loaded {}", p.display());
                Some(img)
            }
            Err(e) => {
                log::warn!("slideshow skipped {} ({})", p.display(), e);
                None
            }
        })
        .collect()
}

// ─── Extended renderers ───────────────────────────────────────────────────────

/// Combined clock + mini sysinfo panel (split-screen).
pub fn render_clock_sysinfo() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%a %b %-e").to_string();

    // ── Top half: clock ───────────────────────────────────────────────────
    let clock_scale = PxScale::from(88.0);
    let tw = text_width(&mono, clock_scale, &time_str);
    let tx = (480 - tw).max(0) / 2;
    draw_text_mut(&mut img, TEAL, tx, 50, clock_scale, &mono, &time_str);
    draw_centered(&mut img, &bold, PxScale::from(26.0), 150, MAUVE, &date_str);

    draw_divider(&mut img, 192);

    // ── Bottom half: quick stats ──────────────────────────────────────────
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu = sys.global_cpu_usage();
    let used = sys.used_memory() as f32 / 1_073_741_824.0;
    let tot = sys.total_memory() as f32 / 1_073_741_824.0;
    let mem = if tot > 0.0 { used / tot } else { 0.0 };
    let up = format_uptime(System::uptime());
    let load = System::load_average();

    let lbl = PxScale::from(20.0);
    let val = PxScale::from(20.0);

    let mut y: i32 = 202;

    draw_kv(
        &mut img,
        &bold,
        &mono,
        lbl,
        y,
        BLUE,
        TEXT,
        "CPU",
        &format!("{cpu:.0}%"),
    );
    y += 24;
    draw_bar(&mut img, 20, y, 440, 14, cpu / 100.0, BLUE);
    y += 22;

    draw_kv(
        &mut img,
        &bold,
        &mono,
        val,
        y,
        GREEN,
        TEXT,
        "RAM",
        &format!("{used:.1}/{tot:.1}G"),
    );
    y += 24;
    draw_bar(&mut img, 20, y, 440, 14, mem, GREEN);
    y += 24;

    draw_divider(&mut img, y);
    y += 10;

    // Load avg row
    let s2 = PxScale::from(17.0);
    let load_s = format!(
        "load {:.2} {:.2} {:.2}  up {}",
        load.one, load.five, load.fifteen, up
    );
    draw_centered(&mut img, &bold, s2, y, SUBTEXT, &load_s);

    DynamicImage::ImageRgb8(img)
}

/// Per-CPU core usage grid (up to 32 cores, 4-wide layout).
pub fn render_cpu_detail() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mut sys = System::new_all();
    sys.refresh_all();

    let cpus: Vec<(String, f32)> = sys
        .cpus()
        .iter()
        .map(|c| (c.name().to_string(), c.cpu_usage()))
        .collect();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "CPU Cores",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time_str,
    );
    draw_divider(&mut img, 52);

    let global = sys.global_cpu_usage();
    let load = System::load_average();
    let gs = format!(
        "Total {:.1}%   load {:.2}/{:.2}/{:.2}",
        global, load.one, load.five, load.fifteen
    );
    draw_text_mut(&mut img, TEXT, 20, 60, PxScale::from(17.0), &bold, &gs);

    let cols = 4usize;
    let cell_w = 440 / cols as i32;
    let cell_h = 56i32;
    let start_y = 86i32;
    let bar_h = 12i32;

    for (i, (_name, usage)) in cpus.iter().enumerate().take(32) {
        let col = (i % cols) as i32;
        let row = (i / cols) as i32;
        let x = 20 + col * cell_w;
        let y = start_y + row * cell_h;

        let label = format!("C{i}");
        draw_text_mut(&mut img, SUBTEXT, x, y, PxScale::from(14.0), &bold, &label);

        let bar_color = if *usage > 80.0 {
            RED
        } else if *usage > 50.0 {
            YELLOW
        } else {
            BLUE
        };
        draw_bar(
            &mut img,
            x,
            y + 16,
            cell_w - 6,
            bar_h,
            usage / 100.0,
            bar_color,
        );

        let pct = format!("{usage:.0}%");
        let pw = text_width(&mono, PxScale::from(13.0), &pct);
        let px_ = x + (cell_w - 6 - pw) / 2;
        if px_ >= x {
            draw_text_mut(
                &mut img,
                TEXT,
                px_,
                y + 30,
                PxScale::from(13.0),
                &mono,
                &pct,
            );
        }
    }

    DynamicImage::ImageRgb8(img)
}

/// Network I/O stats — shows per-interface Rx/Tx rates (bytes/sec).
///
/// This function sleeps for 1 second internally to compute the rate delta.
pub fn render_network() -> DynamicImage {
    use sysinfo::Networks;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time = now.format("%H:%M:%S").to_string();

    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "Network",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time,
    );
    draw_divider(&mut img, 52);

    // Two-sample measurement for Rx/Tx rates
    let mut nets = Networks::new_with_refreshed_list();
    std::thread::sleep(Duration::from_secs(1));
    nets.refresh(false);

    let mut y: i32 = 62;
    let nm_scale = PxScale::from(18.0);
    let val_scale = PxScale::from(16.0);

    let mut showed = 0usize;
    for (name, data) in &nets {
        if showed >= 6 {
            break;
        }
        if name == "lo" {
            continue;
        }

        let rx_ps = data.received(); // bytes since last refresh ≈ bytes/sec
        let tx_ps = data.transmitted();
        let rx_tot = data.total_received();
        let tx_tot = data.total_transmitted();

        let title = if name.len() > 14 {
            format!("{}…", &name[..13])
        } else {
            name.clone()
        };
        draw_text_mut(&mut img, BLUE, 20, y, nm_scale, &bold, &title);
        y += 20;

        // Rate row
        let rx_rate = format!("↓ {}/s", format_bytes(rx_ps));
        let tx_rate = format!("↑ {}/s", format_bytes(tx_ps));
        draw_text_mut(&mut img, GREEN, 30, y, val_scale, &mono, &rx_rate);
        let tw_ = text_width(&mono, val_scale, &tx_rate);
        draw_text_mut(
            &mut img,
            YELLOW,
            480 - 20 - tw_,
            y,
            val_scale,
            &mono,
            &tx_rate,
        );
        y += 18;

        // Totals row (smaller)
        let rx_t = format!("tot ↓ {}", format_bytes(rx_tot));
        let tx_t = format!("↑ {}", format_bytes(tx_tot));
        draw_text_mut(&mut img, OVERLAY0, 30, y, PxScale::from(14.0), &mono, &rx_t);
        let tw2 = text_width(&mono, PxScale::from(14.0), &tx_t);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 20 - tw2,
            y,
            PxScale::from(14.0),
            &mono,
            &tx_t,
        );
        y += 18;

        draw_divider(&mut img, y);
        y += 8;
        showed += 1;
    }

    if showed == 0 {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(20.0),
            200,
            OVERLAY0,
            "No network interfaces",
        );
    }

    DynamicImage::ImageRgb8(img)
}

/// Disk usage display — shows each mounted partition with usage bars.
pub fn render_disk_usage() -> DynamicImage {
    use sysinfo::Disks;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "Disk Usage",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time_str,
    );
    draw_divider(&mut img, 52);

    let disks = Disks::new_with_refreshed_list();

    let mut y: i32 = 62;
    let mut showed = 0usize;

    for disk in &disks {
        if showed >= 6 {
            break;
        }

        let total = disk.total_space();
        let avail = disk.available_space();
        if total == 0 {
            continue;
        }

        let used = total.saturating_sub(avail);
        let frac = used as f32 / total as f32;
        let color = if frac > 0.85 {
            RED
        } else if frac > 0.70 {
            YELLOW
        } else {
            GREEN
        };

        let mount = disk.mount_point().to_string_lossy();
        let mount = if mount.len() > 18 {
            format!("{}…", &mount[..17])
        } else {
            mount.to_string()
        };

        let size_str = format!("{} / {}", format_bytes_si(used), format_bytes_si(total));

        // Mount + size on same line
        draw_text_mut(&mut img, BLUE, 20, y, PxScale::from(18.0), &bold, &mount);
        let sw = text_width(&mono, PxScale::from(16.0), &size_str);
        draw_text_mut(
            &mut img,
            TEXT,
            480 - 20 - sw,
            y + 2,
            PxScale::from(16.0),
            &mono,
            &size_str,
        );
        y += 22;

        // Bar (400px wide) + percentage
        draw_bar(&mut img, 20, y, 400, 14, frac, color);
        let pct = format!("{:.0}%", frac * 100.0);
        let pw = text_width(&mono, PxScale::from(14.0), &pct);
        draw_text_mut(
            &mut img,
            color,
            460 - pw,
            y,
            PxScale::from(14.0),
            &mono,
            &pct,
        );
        y += 18;

        // Filesystem type (small, greyed out)
        let fs = disk.file_system().to_string_lossy();
        let fs = format!("{fs}");
        draw_text_mut(&mut img, OVERLAY0, 20, y, PxScale::from(14.0), &mono, &fs);
        y += 16;

        draw_divider(&mut img, y);
        y += 8;
        showed += 1;
    }

    if showed == 0 {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(20.0),
            200,
            OVERLAY0,
            "No disks detected",
        );
    }

    DynamicImage::ImageRgb8(img)
}

/// Top processes by CPU usage.
///
/// Performs two CPU samples 200 ms apart for accurate per-process CPU%.
pub fn render_process_top() -> DynamicImage {
    use std::cmp::Ordering;
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "Top Processes",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time_str,
    );
    draw_divider(&mut img, 52);

    // Two-sample CPU measurement
    let mut sys = System::new_all();
    sys.refresh_all();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        false,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    let cpu_count = sys.cpus().len().max(1) as f32;

    let mut procs: Vec<_> = sys.processes().values().collect();
    procs.sort_by(|a, b| {
        b.cpu_usage()
            .partial_cmp(&a.cpu_usage())
            .unwrap_or(Ordering::Equal)
    });

    // Column headers
    let hdr = PxScale::from(14.0);
    draw_text_mut(&mut img, OVERLAY0, 20, 58, hdr, &bold, "PROCESS");
    draw_text_mut(&mut img, OVERLAY0, 290, 58, hdr, &bold, "CPU%");
    draw_text_mut(&mut img, OVERLAY0, 380, 58, hdr, &bold, "MEM");
    draw_divider(&mut img, 74);

    let mut y: i32 = 78;
    let row_h: i32 = 40;
    let name_scale = PxScale::from(15.0);
    let val_scale = PxScale::from(14.0);

    for proc in procs.iter().take(9) {
        if y + row_h > 480 {
            break;
        }

        let raw_name = proc.name().to_string_lossy();
        let name = if raw_name.len() > 20 {
            format!("{}…", &raw_name[..19])
        } else {
            raw_name.to_string()
        };

        // CPU% normalised to 0–100 across all cores
        let cpu_pct = (proc.cpu_usage() / cpu_count).clamp(0.0, 100.0);
        let cpu_col = if cpu_pct > 50.0 {
            RED
        } else if cpu_pct > 20.0 {
            YELLOW
        } else {
            GREEN
        };

        // Memory
        let mem_mb = proc.memory() as f64 / 1_048_576.0;
        let mem_str = if mem_mb >= 1024.0 {
            format!("{:.1}G", mem_mb / 1024.0)
        } else {
            format!("{mem_mb:.0}M")
        };

        // Name
        draw_text_mut(&mut img, TEXT, 20, y + 2, name_scale, &bold, &name);

        // CPU bar + value
        draw_bar(&mut img, 280, y + 4, 80, 10, cpu_pct / 100.0, cpu_col);
        let cs = format!("{cpu_pct:.1}%");
        draw_text_mut(&mut img, cpu_col, 368, y + 2, val_scale, &mono, &cs);

        // Memory
        let mw = text_width(&mono, val_scale, &mem_str);
        draw_text_mut(&mut img, PEACH, 460 - mw, y + 2, val_scale, &mono, &mem_str);

        draw_divider(&mut img, y + row_h - 2);
        y += row_h;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Custom text display ─────────────────────────────────────────────────────

/// Named colour for [`CustomTextPayload`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NamedColor {
    #[default]
    Blue,
    Green,
    Yellow,
    Red,
    Teal,
    Mauve,
    Peach,
    White,
}

impl NamedColor {
    pub(super) fn to_rgb(&self) -> Rgb<u8> {
        match self {
            NamedColor::Blue => BLUE,
            NamedColor::Green => GREEN,
            NamedColor::Yellow => YELLOW,
            NamedColor::Red => RED,
            NamedColor::Teal => TEAL,
            NamedColor::Mauve => MAUVE,
            NamedColor::Peach => PEACH,
            NamedColor::White => TEXT,
        }
    }

    /// Public alias for `to_rgb`, usable from other crates/modules.
    pub fn to_rgb_pub(&self) -> Rgb<u8> {
        self.to_rgb()
    }

    /// Parse a colour name case-insensitively; falls back to Blue.
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "green" => NamedColor::Green,
            "yellow" => NamedColor::Yellow,
            "red" => NamedColor::Red,
            "teal" => NamedColor::Teal,
            "mauve" => NamedColor::Mauve,
            "peach" => NamedColor::Peach,
            "white" => NamedColor::White,
            _ => NamedColor::Blue,
        }
    }
}

/// Content pushed via `POST /api/text`.
///
/// All fields are optional; omitted fields use sensible defaults.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct CustomTextPayload {
    /// Large title at top of screen.
    pub title: Option<String>,
    /// Smaller subtitle line below the title.
    pub subtitle: Option<String>,
    /// Up to 8 body lines (shown in monospace).
    #[serde(default)]
    pub lines: Vec<String>,
    /// Progress bar value 0.0–1.0 (absent = no bar).
    pub progress: Option<f32>,
    /// Colour for the progress bar and title (default: blue).
    #[serde(default)]
    pub color: NamedColor,
    /// If true, show current time in the top-right corner.
    #[serde(default = "bool_true")]
    pub show_time: bool,
    /// Auto-clear after this many seconds (absent = stay until manually cleared).
    /// The daemon returns to the previous live mode when the timer expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

pub(super) fn bool_true() -> bool {
    true
}

/// Render a [`CustomTextPayload`] to a 480×480 frame.
pub fn render_custom_text(payload: &CustomTextPayload) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let accent = payload.color.to_rgb();
    let now = Local::now();
    let time_s = now.format("%H:%M:%S").to_string();

    let mut y: i32 = 14;

    // ── Timestamp in top-right ─────────────────────────────────────────────
    if payload.show_time {
        let tw = text_width(&mono, PxScale::from(22.0), &time_s);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            480 - 20 - tw,
            y,
            PxScale::from(22.0),
            &mono,
            &time_s,
        );
    }

    // ── Title ──────────────────────────────────────────────────────────────
    if let Some(title) = &payload.title {
        let t = if title.len() > 22 {
            format!("{}…", &title[..21])
        } else {
            title.clone()
        };
        draw_centered(&mut img, &bold, PxScale::from(34.0), y, accent, &t);
        y += 42;
    } else {
        y += 14;
    }

    // ── Subtitle ───────────────────────────────────────────────────────────
    if let Some(sub) = &payload.subtitle {
        let s = if sub.len() > 38 {
            format!("{}…", &sub[..37])
        } else {
            sub.clone()
        };
        draw_centered(&mut img, &bold, PxScale::from(20.0), y, SUBTEXT, &s);
        y += 26;
    }

    draw_divider(&mut img, y);
    y += 12;

    // ── Body lines ─────────────────────────────────────────────────────────
    let line_scale = PxScale::from(20.0);
    for line in payload.lines.iter().take(10) {
        if y > 420 {
            break;
        }
        let l = if line.len() > 38 {
            format!("{}…", &line[..37])
        } else {
            line.clone()
        };
        draw_text_mut(&mut img, TEXT, 20, y, line_scale, &mono, &l);
        y += 26;
    }

    // ── Progress bar ───────────────────────────────────────────────────────
    if let Some(pct) = payload.progress {
        let pct = pct.clamp(0.0, 1.0);
        let bar_y = 420i32;
        draw_bar(&mut img, 20, bar_y, 400, 22, pct, accent);
        let pct_s = format!("{:.0}%", pct * 100.0);
        let pw = text_width(&mono, PxScale::from(18.0), &pct_s);
        draw_text_mut(
            &mut img,
            accent,
            460 - pw,
            bar_y + 2,
            PxScale::from(18.0),
            &mono,
            &pct_s,
        );
    }

    DynamicImage::ImageRgb8(img)
}

/// Thermal sensor detail — all available temperature components with color-coded bars.
///
/// Shows up to 10 sensors. If no sensors are available (VM, no hwmon driver),
/// a placeholder message is shown.
pub fn render_temperature() -> DynamicImage {
    use sysinfo::Components;

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "Temperatures",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time_str,
    );
    draw_divider(&mut img, 52);

    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);

    // Collect all valid readings, sorted by label
    let mut readings: Vec<(String, f32, Option<f32>)> = comps
        .iter()
        .filter_map(|c| {
            let t = c.temperature()?;
            if t <= 0.0 || t.is_nan() {
                return None;
            }
            let label = c.label().to_string();
            let crit = c.critical();
            Some((label, t, crit))
        })
        .collect();
    readings.sort_by(|a, b| a.0.cmp(&b.0));

    if readings.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(20.0),
            200,
            OVERLAY0,
            "No sensors detected",
        );
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(16.0),
            230,
            SURFACE0,
            "Load hwmon kernel module or run on bare metal",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let mut y: i32 = 62;
    let lbl_scale = PxScale::from(15.0);
    let val_scale = PxScale::from(15.0);
    let bar_scale = PxScale::from(13.0);

    for (label, temp, crit) in readings.iter().take(10) {
        if y + 44 > 480 {
            break;
        }

        // Determine color based on fraction of critical (or 100 °C if unknown)
        let max_temp = crit.filter(|&c| c > 0.0).unwrap_or(100.0);
        let frac = (temp / max_temp).clamp(0.0, 1.0);
        let col = if frac > 0.85 {
            RED
        } else if frac > 0.65 {
            YELLOW
        } else {
            GREEN
        };

        // Label
        let lbl = if label.len() > 26 {
            format!("{}…", &label[..25])
        } else {
            label.clone()
        };
        draw_text_mut(&mut img, SUBTEXT, 20, y, lbl_scale, &bold, &lbl);

        // Temperature value right-aligned
        let val_s = format!("{temp:.1}°C");
        let vw = text_width(&mono, val_scale, &val_s);
        draw_text_mut(&mut img, col, 480 - 20 - vw, y, val_scale, &mono, &val_s);
        y += 18;

        // Bar (350 px wide) + optional critical marker
        draw_bar(&mut img, 20, y, 350, 10, frac, col);
        if let Some(c) = crit.filter(|&c| c > 0.0) {
            let crit_s = format!("crit {c:.0}°");
            let cw = text_width(&mono, bar_scale, &crit_s);
            draw_text_mut(
                &mut img,
                OVERLAY0,
                480 - 20 - cw,
                y,
                bar_scale,
                &mono,
                &crit_s,
            );
        }
        y += 14;

        draw_divider(&mut img, y);
        y += 8;
    }

    if readings.len() > 10 {
        let more = format!("…and {} more sensors", readings.len() - 10);
        draw_centered(&mut img, &bold, PxScale::from(15.0), y, OVERLAY0, &more);
    }

    DynamicImage::ImageRgb8(img)
}

/// Detailed memory breakdown: RAM used/cached/available/free, swap, load.
pub fn render_memory_detail() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let time_str = now.format("%H:%M:%S").to_string();
    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        14,
        PxScale::from(28.0),
        &bold,
        "Memory",
    );
    let tw = text_width(&mono, PxScale::from(24.0), &time_str);
    draw_text_mut(
        &mut img,
        TEAL,
        480 - 20 - tw,
        18,
        PxScale::from(24.0),
        &mono,
        &time_str,
    );
    draw_divider(&mut img, 52);

    let mut sys = System::new_all();
    sys.refresh_memory();
    let load = System::load_average();

    let total = sys.total_memory();
    let used = sys.used_memory();
    let avail = sys.available_memory();
    let free = sys.free_memory();
    let cached = total.saturating_sub(used).saturating_sub(free);

    let sw_total = sys.total_swap();
    let sw_used = sys.used_swap();

    let mem_frac = if total > 0 {
        used as f32 / total as f32
    } else {
        0.0
    };
    let swap_frac = if sw_total > 0 {
        sw_used as f32 / sw_total as f32
    } else {
        0.0
    };

    let lbl = PxScale::from(20.0);
    let val = PxScale::from(18.0);

    let fmt_gb = |b: u64| -> String {
        let gb = b as f64 / 1_073_741_824.0;
        if gb >= 1.0 {
            format!("{gb:.2} GB")
        } else {
            format!("{:.0} MB", b as f64 / 1_048_576.0)
        }
    };

    let mut y: i32 = 60;

    // ── RAM ───────────────────────────────────────────────────────────────────
    let ram_col = if mem_frac > 0.85 {
        RED
    } else if mem_frac > 0.65 {
        YELLOW
    } else {
        GREEN
    };
    draw_kv(
        &mut img,
        &bold,
        &mono,
        lbl,
        y,
        GREEN,
        TEXT,
        "RAM Used",
        &format!(
            "{} / {} ({:.0}%)",
            fmt_gb(used),
            fmt_gb(total),
            mem_frac * 100.0
        ),
    );
    y += 26;
    draw_bar(&mut img, 20, y, 440, 16, mem_frac, ram_col);
    y += 24;

    draw_kv(
        &mut img,
        &bold,
        &mono,
        val,
        y,
        SUBTEXT,
        SUBTEXT,
        "  Avail.",
        &fmt_gb(avail),
    );
    y += 22;
    draw_kv(
        &mut img,
        &bold,
        &mono,
        val,
        y,
        SUBTEXT,
        SUBTEXT,
        "  Free",
        &fmt_gb(free),
    );
    y += 22;
    draw_kv(
        &mut img,
        &bold,
        &mono,
        val,
        y,
        SUBTEXT,
        SUBTEXT,
        "  Cached",
        &fmt_gb(cached),
    );
    y += 24;

    draw_divider(&mut img, y);
    y += 10;

    // ── Swap ─────────────────────────────────────────────────────────────────
    if sw_total > 0 {
        let sw_col = if swap_frac > 0.5 {
            RED
        } else if swap_frac > 0.2 {
            YELLOW
        } else {
            TEAL
        };
        draw_kv(
            &mut img,
            &bold,
            &mono,
            lbl,
            y,
            TEAL,
            TEXT,
            "Swap",
            &format!(
                "{} / {} ({:.0}%)",
                fmt_gb(sw_used),
                fmt_gb(sw_total),
                swap_frac * 100.0
            ),
        );
        y += 26;
        draw_bar(&mut img, 20, y, 440, 16, swap_frac, sw_col);
        y += 24;
        draw_divider(&mut img, y);
        y += 10;
    } else {
        draw_kv(
            &mut img,
            &bold,
            &mono,
            lbl,
            y,
            TEAL,
            OVERLAY0,
            "Swap",
            "not configured",
        );
        y += 30;
        draw_divider(&mut img, y);
        y += 10;
    }

    // ── Load + uptime ─────────────────────────────────────────────────────────
    let s = PxScale::from(18.0);
    let load_str = format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen);
    draw_text_mut(&mut img, OVERLAY0, 20, y, s, &bold, "Load avg");
    let lw = text_width(&mono, s, &load_str);
    draw_text_mut(&mut img, SUBTEXT, 480 - 20 - lw, y, s, &mono, &load_str);
    y += 24;
    let uptime = format_uptime(System::uptime());
    draw_text_mut(
        &mut img,
        SUBTEXT,
        20,
        y,
        s,
        &bold,
        &format!("Uptime  {uptime}"),
    );

    DynamicImage::ImageRgb8(img)
}

/// Countdown timer display.
///
/// Shows remaining time until `target_unix_secs` as a large clock.  The accent
/// colour transitions green → yellow → red as the deadline approaches (based on
/// the original total duration `total_secs`).
///
/// When `remaining_secs == 0` the panel shows a "TIME'S UP" message in red.
pub fn render_countdown(remaining_secs: i64, total_secs: i64, label: &str) -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let now = Local::now();
    let ts = now.format("%H:%M:%S").to_string();

    // Header — label left, clock right
    let header_label = if label.is_empty() { "COUNTDOWN" } else { label };
    let header_label = if header_label.len() > 22 {
        format!("{}…", &header_label[..21])
    } else {
        header_label.to_string()
    };
    draw_text_mut(
        &mut img,
        OVERLAY0,
        16,
        12,
        PxScale::from(20.0),
        &bold,
        &header_label,
    );
    let tw_ts = text_width(&mono, PxScale::from(18.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 12 - tw_ts,
        14,
        PxScale::from(18.0),
        &mono,
        &ts,
    );
    draw_divider(&mut img, 38);

    // ── Finished state ────────────────────────────────────────────────────────
    if remaining_secs <= 0 {
        // Pulsing TIME'S UP — alternate between RED and PEACH based on 1-second clock
        let flash = now.second().is_multiple_of(2);
        let finish_color = if flash { RED } else { PEACH };
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(72.0),
            150,
            finish_color,
            "TIME'S UP!",
        );
        // Draw a full red ring
        let cx = 240_i32;
        let cy = 300_i32;
        for py in (cy - 80)..(cy + 80) {
            for px in (cx - 80)..(cx + 80) {
                let dist = (((px - cx).pow(2) + (py - cy).pow(2)) as f64).sqrt();
                if (62.0..=78.0).contains(&dist) && (0..480).contains(&px) && (0..480).contains(&py)
                {
                    img.put_pixel(px as u32, py as u32, if flash { RED } else { PEACH });
                }
            }
        }
        draw_centered(&mut img, &bold, PxScale::from(22.0), 420, OVERLAY0, label);
        return DynamicImage::ImageRgb8(img);
    }

    // ── Progress and color ────────────────────────────────────────────────────
    let frac_left = if total_secs > 0 {
        (remaining_secs as f32 / total_secs as f32).clamp(0.0, 1.0)
    } else {
        1.0
    };
    // Urgency: last 10 seconds blink between accent and SURFACE1
    let is_urgent = remaining_secs <= 10;
    let flash_off = is_urgent && (now.second() % 2 == 1 || (now.nanosecond() / 500_000_000) == 1);
    let accent = if frac_left > 0.5 {
        GREEN
    } else if frac_left > 0.2 {
        YELLOW
    } else {
        RED
    };
    let display_color = if flash_off { SURFACE0 } else { accent };

    // ── Circular arc ring ────────────────────────────────────────────────────
    let cx = 240_i32;
    let cy = 220_i32;
    let r_outer = 130_i32;
    let r_inner = 96_i32;

    let two_pi = std::f64::consts::PI * 2.0;
    let start_angle = -std::f64::consts::FRAC_PI_2; // top (12 o'clock)
    let sweep = frac_left as f64 * two_pi;

    // Background ring
    for py in (cy - r_outer - 2)..(cy + r_outer + 2) {
        for px in (cx - r_outer - 2)..(cx + r_outer + 2) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r_inner as f64
                && dist <= r_outer as f64
                && (0..480).contains(&px)
                && (0..480).contains(&py)
            {
                img.put_pixel(px as u32, py as u32, SURFACE0);
            }
        }
    }

    // Progress arc (remaining portion)
    for py in (cy - r_outer - 2)..(cy + r_outer + 2) {
        for px in (cx - r_outer - 2)..(cx + r_outer + 2) {
            let dx = (px - cx) as f64;
            let dy = (py - cy) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r_inner as f64 && dist <= r_outer as f64 {
                let angle = dy.atan2(dx);
                let mut rel = angle - start_angle;
                if rel < 0.0 {
                    rel += two_pi;
                }
                if rel <= sweep && (0..480).contains(&px) && (0..480).contains(&py) {
                    img.put_pixel(px as u32, py as u32, display_color);
                }
            }
        }
    }

    // ── Central time display ─────────────────────────────────────────────────
    let h = remaining_secs / 3600;
    let m = (remaining_secs % 3600) / 60;
    let s = remaining_secs % 60;
    let (time_display, time_scale) = if h > 0 {
        (format!("{h:02}:{m:02}:{s:02}"), PxScale::from(60.0))
    } else {
        (format!("{m:02}:{s:02}"), PxScale::from(76.0))
    };

    let tw_time = text_width(&mono, time_scale, &time_display);
    let tx = cx - tw_time / 2;
    let ty = cy - (time_scale.y / 2.0) as i32;
    draw_text_mut(
        &mut img,
        display_color,
        tx,
        ty,
        time_scale,
        &mono,
        &time_display,
    );

    // Elapsed % inside ring
    let pct_str = format!("{:.0}%", frac_left * 100.0);
    let pw = text_width(&bold, PxScale::from(18.0), &pct_str);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        cx - pw / 2,
        cy + (time_scale.y * 0.5) as i32 + 6,
        PxScale::from(18.0),
        &bold,
        &pct_str,
    );

    // ── Stats row ────────────────────────────────────────────────────────────
    draw_divider(&mut img, 365);

    let remaining_str = format_uptime(remaining_secs as u64);
    let elapsed_secs = (total_secs - remaining_secs).max(0) as u64;
    let elapsed_str = format_uptime(elapsed_secs);

    let left_str = format!("Remaining: {remaining_str}");
    let right_str = format!("Elapsed: {elapsed_str}");
    draw_text_mut(
        &mut img,
        SUBTEXT,
        16,
        376,
        PxScale::from(16.0),
        &mono,
        &left_str,
    );
    let rw = text_width(&mono, PxScale::from(16.0), &right_str);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        480 - 16 - rw,
        376,
        PxScale::from(16.0),
        &mono,
        &right_str,
    );

    // ── Thin progress bar at bottom ───────────────────────────────────────────
    let bar_y = 404_i32;
    let bar_h = 14_i32;
    draw_filled_rect_mut(
        &mut img,
        Rect::at(16, bar_y).of_size(448, bar_h as u32),
        SURFACE0,
    );
    let bar_fill = ((frac_left * 448.0) as i32).clamp(0, 448);
    if bar_fill > 0 {
        draw_filled_rect_mut(
            &mut img,
            Rect::at(16, bar_y).of_size(bar_fill as u32, bar_h as u32),
            display_color,
        );
    }

    // ── Urgency indicator ─────────────────────────────────────────────────────
    if is_urgent && !flash_off {
        let warn_str = format!("⚠ {remaining_secs}s REMAINING");
        let ww = text_width(&bold, PxScale::from(20.0), &warn_str);
        draw_text_mut(
            &mut img,
            RED,
            (480 - ww) / 2,
            430,
            PxScale::from(20.0),
            &bold,
            &warn_str,
        );
    }

    DynamicImage::ImageRgb8(img)
}
