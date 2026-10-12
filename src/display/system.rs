//! System information screens: memory hogs, disk I/O, battery, CPU frequency, host and IP, uptime, OS info, temperatures and disk usage.

use super::*;

// ─── Top processes by memory ──────────────────────────────────────────────────

/// Top processes ranked by memory usage.
///
/// Similar to `render_process_top` but sorted by RAM instead of CPU.
pub fn render_process_memory() -> DynamicImage {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.refresh_memory();

    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();

    let mut procs: Vec<(String, u64, f32)> = sys
        .processes()
        .values()
        .map(|p| {
            (
                p.name().to_string_lossy().to_string(),
                p.memory(),
                p.cpu_usage(),
            )
        })
        .collect();
    procs.sort_by_key(|p| std::cmp::Reverse(p.1));
    procs.truncate(8);

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    let mut y: i32 = 14;

    // ── Header ─────────────────────────────────────────────────────────────
    draw_text_mut(
        &mut img,
        MAUVE,
        20,
        y,
        PxScale::from(28.0),
        &bold,
        "TOP PROCESSES",
    );
    let now = Local::now();
    let tw = text_width(
        &mono,
        PxScale::from(20.0),
        &now.format("%H:%M:%S").to_string(),
    );
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        y + 4,
        PxScale::from(20.0),
        &mono,
        &now.format("%H:%M:%S").to_string(),
    );
    y += 36;

    // ── Total RAM bar ───────────────────────────────────────────────────────
    let sm = PxScale::from(18.0);
    let used_gb = used_mem as f32 / 1_073_741_824.0;
    let total_gb = total_mem as f32 / 1_073_741_824.0;
    let ram_frac = if total_mem > 0 {
        used_mem as f32 / total_mem as f32
    } else {
        0.0
    };
    draw_kv(
        &mut img,
        &bold,
        &mono,
        sm,
        y,
        GREEN,
        TEXT,
        "RAM",
        &format!(
            "{:.1}/{:.1} GB ({:.0}%)",
            used_gb,
            total_gb,
            ram_frac * 100.0
        ),
    );
    y += 24;
    draw_bar(&mut img, 20, y, 440, 10, ram_frac, GREEN);
    y += 18;

    draw_divider(&mut img, y);
    y += 10;

    // ── Column headers ──────────────────────────────────────────────────────
    draw_text_mut(&mut img, SUBTEXT, 20, y, sm, &bold, "PROCESS");
    draw_text_mut(&mut img, SUBTEXT, 280, y, sm, &bold, "MEM");
    draw_text_mut(&mut img, SUBTEXT, 370, y, sm, &bold, "CPU%");
    y += 22;
    draw_divider(&mut img, y);
    y += 6;

    // ── Process rows ────────────────────────────────────────────────────────
    for (name, mem, cpu) in &procs {
        if y > 455 {
            break;
        }
        let mem_pct = if total_mem > 0 {
            *mem as f32 / total_mem as f32
        } else {
            0.0
        };
        let color = if mem_pct > 0.20 {
            RED
        } else if mem_pct > 0.10 {
            YELLOW
        } else {
            TEXT
        };
        let n = if name.len() > 20 {
            format!("{}…", &name[..19])
        } else {
            name.clone()
        };
        let mem_s = if *mem >= 1_073_741_824 {
            format!("{:.1}G", *mem as f32 / 1_073_741_824.0)
        } else if *mem >= 1_048_576 {
            format!("{:.0}M", *mem as f32 / 1_048_576.0)
        } else {
            format!("{:.0}K", *mem as f32 / 1024.0)
        };
        let cpu_s = format!("{cpu:.1}%");
        draw_text_mut(&mut img, color, 20, y, sm, &bold, &n);
        draw_text_mut(&mut img, color, 280, y, sm, &mono, &mem_s);
        draw_text_mut(&mut img, OVERLAY0, 370, y, sm, &mono, &cpu_s);
        y += 22;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Disk I/O rates ───────────────────────────────────────────────────────────

/// Current disk read/write throughput for each block device.
///
/// Refreshes disk stats twice with a ~500 ms gap and uses sysinfo's
/// `DiskUsage::read_bytes` / `written_bytes` (delta since last refresh)
/// to compute per-second rates.
pub fn render_disk_io() -> DynamicImage {
    use sysinfo::{DiskRefreshKind, Disks};

    // First refresh to get I/O counters
    let mut disks = Disks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(500));
    // Second refresh — read_bytes / written_bytes are deltas since the previous refresh
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    struct DiskRate {
        name: String,
        read_bps: u64,
        write_bps: u64,
        used_pct: f32,
    }
    let rates: Vec<DiskRate> = disks
        .iter()
        .filter_map(|d| {
            let total = d.total_space();
            if total == 0 {
                return None;
            }
            let free = d.available_space();
            let used = total - free;
            let used_pct = used as f32 / total as f32;
            let io = d.usage();
            // io.read_bytes / written_bytes are deltas over the ~500 ms window;
            // multiply by 2 to get approximate bytes/sec.
            let read_bps = io.read_bytes * 2;
            let write_bps = io.written_bytes * 2;
            Some(DiskRate {
                name: d.name().to_string_lossy().to_string(),
                read_bps,
                write_bps,
                used_pct,
            })
        })
        .take(6)
        .collect();

    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();
    let sm = PxScale::from(18.0);
    let med = PxScale::from(20.0);

    let mut y: i32 = 14;

    // Header
    draw_text_mut(
        &mut img,
        PEACH,
        20,
        y,
        PxScale::from(28.0),
        &bold,
        "DISK I/O",
    );
    let now = Local::now();
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(20.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - 20 - tw,
        y + 4,
        PxScale::from(20.0),
        &mono,
        &ts,
    );
    y += 36;

    draw_divider(&mut img, y);
    y += 10;

    if rates.is_empty() {
        draw_centered(
            &mut img,
            &bold,
            PxScale::from(22.0),
            200,
            SUBTEXT,
            "No disks detected",
        );
        return DynamicImage::ImageRgb8(img);
    }

    // Column headers
    draw_text_mut(&mut img, SUBTEXT, 20, y, sm, &bold, "DEVICE");
    draw_text_mut(&mut img, SUBTEXT, 160, y, sm, &bold, "READ/s");
    draw_text_mut(&mut img, SUBTEXT, 270, y, sm, &bold, "WRITE/s");
    draw_text_mut(&mut img, SUBTEXT, 370, y, sm, &bold, "USE%");
    y += 22;
    draw_divider(&mut img, y);
    y += 8;

    for d in &rates {
        if y > 455 {
            break;
        }
        let n = if d.name.len() > 10 {
            format!("{}…", &d.name[..9])
        } else {
            d.name.clone()
        };
        let r_s = format_bps(d.read_bps);
        let w_s = format_bps(d.write_bps);
        let use_s = format!("{:.0}%", d.used_pct * 100.0);
        let color = if d.used_pct > 0.90 {
            RED
        } else if d.used_pct > 0.75 {
            YELLOW
        } else {
            TEXT
        };
        draw_text_mut(&mut img, TEAL, 20, y, med, &bold, &n);
        draw_text_mut(&mut img, GREEN, 160, y, med, &mono, &r_s);
        draw_text_mut(&mut img, PEACH, 270, y, med, &mono, &w_s);
        draw_text_mut(&mut img, color, 370, y, med, &mono, &use_s);
        // Usage bar
        draw_bar(&mut img, 20, y + 22, 440, 8, d.used_pct, color);
        y += 38;
        draw_divider(&mut img, y);
        y += 8;
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Battery status display ───────────────────────────────────────────────────

pub(super) struct BatteryInfo {
    capacity: u8,
    status: String,
    voltage_uv: Option<u64>,      // microvolts
    current_ua: Option<u64>,      // microamps
    energy_now_uwh: Option<u64>,  // micro-watt-hours
    energy_full_uwh: Option<u64>, // micro-watt-hours
}

/// Read battery info from the Linux sysfs power-supply interface.
/// Returns `None` on desktops or if no battery is found.
pub(super) fn read_battery_info() -> Option<BatteryInfo> {
    for name in &["BAT0", "BAT1", "BAT2", "BAT"] {
        let base = format!("/sys/class/power_supply/{name}");
        if !std::path::Path::new(&base).exists() {
            continue;
        }
        let read_str = |f: &str| -> Option<String> {
            std::fs::read_to_string(format!("{base}/{f}"))
                .ok()
                .map(|s| s.trim().to_string())
        };
        let read_u64 = |f: &str| -> Option<u64> { read_str(f)?.parse::<u64>().ok() };

        let capacity = read_u64("capacity").map_or(0, |v| v.min(100) as u8);
        let status = read_str("status").unwrap_or_default();

        let voltage_uv = read_u64("voltage_now");
        let current_ua = read_u64("current_now");

        // energy_now may be in µWh (energy_now) or µAh (charge_now)
        let energy_now_uwh = read_u64("energy_now").or_else(|| {
            let uah = read_u64("charge_now")?;
            let uv = voltage_uv?;
            Some(uah * uv / 1_000_000)
        });
        let energy_full_uwh = read_u64("energy_full").or_else(|| {
            let uah = read_u64("charge_full")?;
            let uv = voltage_uv?;
            Some(uah * uv / 1_000_000)
        });

        return Some(BatteryInfo {
            capacity,
            status,
            voltage_uv,
            current_ua,
            energy_now_uwh,
            energy_full_uwh,
        });
    }
    None
}

/// Render a battery status display.
///
/// Reads live data from `/sys/class/power_supply/BAT*`.  
/// Shows a large battery icon, percentage, status, voltage, and estimated
/// time remaining on discharge.
pub fn render_battery() -> DynamicImage {
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
        "Battery",
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

    match read_battery_info() {
        None => {
            draw_centered(
                &mut img,
                &bold,
                PxScale::from(22.0),
                200,
                OVERLAY0,
                "No battery detected",
            );
            draw_centered(
                &mut img,
                &mono,
                PxScale::from(16.0),
                232,
                SURFACE0,
                "Desktop or unsupported platform",
            );
        }
        Some(info) => {
            let pct = info.capacity.clamp(0, 100);
            let frac = pct as f32 / 100.0;
            let color = if pct > 50 {
                GREEN
            } else if pct > 20 {
                YELLOW
            } else {
                RED
            };

            // ── Battery icon ─────────────────────────────────────────────────
            // Body at (90, 105), 285×125 px
            let bx = 90i32;
            let by = 105i32;
            let bw = 285i32;
            let bh = 125i32;
            let term_w = 14i32;
            let term_h = 46i32;

            // Outline
            draw_hollow_rect_mut(
                &mut img,
                Rect::at(bx, by).of_size(bw as u32, bh as u32),
                OVERLAY0,
            );
            // Terminal nub
            draw_filled_rect_mut(
                &mut img,
                Rect::at(bx + bw, by + (bh - term_h) / 2).of_size(term_w as u32, term_h as u32),
                OVERLAY0,
            );
            // Fill bar
            let fill_w = ((bw - 4) as f32 * frac) as i32;
            if fill_w > 0 {
                draw_filled_rect_mut(
                    &mut img,
                    Rect::at(bx + 2, by + 2).of_size(fill_w as u32, (bh - 4) as u32),
                    color,
                );
            }

            // ── Percentage centred inside the battery body ───────────────────
            let pct_str = format!("{pct}%");
            let text_col = if frac > 0.35 { BG } else { TEXT };
            draw_centered(
                &mut img,
                &bold,
                PxScale::from(60.0),
                120,
                text_col,
                &pct_str,
            );

            // ── Status line ───────────────────────────────────────────────────
            let status_lower = info.status.to_lowercase();
            let status_str: &str = match status_lower.trim() {
                "charging" => "⚡ Charging",
                "discharging" => "Discharging",
                "full" => "Full",
                "not charging" => "Not Charging",
                other => other,
            };
            draw_centered(&mut img, &bold, PxScale::from(24.0), 250, color, status_str);

            // ── Detail rows ───────────────────────────────────────────────────
            draw_divider(&mut img, 286);
            let mut y = 298i32;
            let scale = PxScale::from(18.0);

            if let Some(v) = info.voltage_uv {
                draw_kv(
                    &mut img,
                    &bold,
                    &mono,
                    scale,
                    y,
                    SUBTEXT,
                    TEXT,
                    "Voltage",
                    &format!("{:.3} V", v as f64 / 1_000_000.0),
                );
                y += 26;
            }
            if let (Some(en), Some(ef)) = (info.energy_now_uwh, info.energy_full_uwh) {
                draw_kv(
                    &mut img,
                    &bold,
                    &mono,
                    scale,
                    y,
                    SUBTEXT,
                    TEXT,
                    "Capacity",
                    &format!(
                        "{:.1}/{:.1} Wh",
                        en as f64 / 1_000_000.0,
                        ef as f64 / 1_000_000.0
                    ),
                );
                y += 26;
            }
            // Estimated time remaining for discharging
            if info.status.to_lowercase() == "discharging" {
                if let (Some(en), Some(cur), Some(v)) =
                    (info.energy_now_uwh, info.current_ua, info.voltage_uv)
                {
                    let power_uw = cur as f64 * v as f64 / 1_000_000.0;
                    if power_uw > 0.0 {
                        let hours_left = en as f64 / power_uw;
                        let h = hours_left as u64;
                        let m = ((hours_left - h as f64) * 60.0) as u64;
                        let est = format!("{h}h {m:02}m");
                        draw_kv(
                            &mut img,
                            &bold,
                            &mono,
                            scale,
                            y,
                            SUBTEXT,
                            TEAL,
                            "Est. left",
                            &est,
                        );
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Public battery data accessor ────────────────────────────────────────────

/// Return battery information as a `serde_json::Value` for the API.
///
/// On systems without a battery, returns `{"present": false}`.
pub fn read_battery_json() -> serde_json::Value {
    use serde_json::json;
    match read_battery_info() {
        None => json!({ "present": false }),
        Some(info) => {
            let power_uw = info
                .current_ua
                .and_then(|c| info.voltage_uv.map(|v| c * v / 1_000_000));
            let hours_remaining = if info.status.to_lowercase() == "discharging" {
                power_uw.and_then(|p| {
                    if p == 0 {
                        return None;
                    }
                    info.energy_now_uwh.map(|e| e as f64 / p as f64)
                })
            } else {
                None
            };

            json!({
                "present":          true,
                "capacity_pct":     info.capacity,
                "status":           info.status,
                "voltage_v":        info.voltage_uv.map(|v| v as f64 / 1_000_000.0),
                "current_a":        info.current_ua.map(|c| c as f64 / 1_000_000.0),
                "power_w":          power_uw.map(|p| p as f64 / 1_000_000.0),
                "energy_now_wh":    info.energy_now_uwh.map(|e|  e as f64 / 1_000_000.0),
                "energy_full_wh":   info.energy_full_uwh.map(|e| e as f64 / 1_000_000.0),
                "hours_remaining":  hours_remaining,
            })
        }
    }
}

// ─── CPU frequency display ────────────────────────────────────────────────────

/// Read current CPU MHz values from /proc/cpuinfo.
/// Returns one entry per logical core (in core order).
pub(super) fn read_cpu_mhz() -> Vec<f32> {
    let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| l.starts_with("cpu MHz"))
        .filter_map(|l| {
            l.split(':')
                .nth(1)
                .and_then(|v| v.trim().parse::<f32>().ok())
        })
        .collect()
}

/// Render a per-core CPU frequency grid.
///
/// Shows each logical CPU's current clock in MHz, colour-coded:
/// - Green ≥ max * 0.90  (boosting / near max)
/// - Teal  ≥ max * 0.60
/// - Yellow ≥ max * 0.30 (moderate)
/// - Blue  < max * 0.30  (light load / power-save)
pub fn render_cpu_freq() -> DynamicImage {
    let freqs = read_cpu_mhz();
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 44), SURFACE0);
    draw_text_mut(
        &mut img,
        TEAL,
        8,
        8,
        PxScale::from(24.0),
        &bold,
        "CPU Frequency",
    );

    let now = Local::now();
    let ts = now.format("%H:%M:%S").to_string();
    let tw = text_width(&mono, PxScale::from(14.0), &ts);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        480 - tw - 8,
        14,
        PxScale::from(14.0),
        &mono,
        &ts,
    );

    draw_filled_rect_mut(&mut img, Rect::at(0, 44).of_size(480, 2), TEAL);

    if freqs.is_empty() {
        draw_text_mut(
            &mut img,
            OVERLAY0,
            12,
            64,
            PxScale::from(18.0),
            &mono,
            "No CPU frequency data available.",
        );
        draw_text_mut(
            &mut img,
            OVERLAY0,
            12,
            88,
            PxScale::from(14.0),
            &mono,
            "(requires /proc/cpuinfo with 'cpu MHz')",
        );
        return DynamicImage::ImageRgb8(img);
    }

    let max_mhz = freqs.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    let n = freqs.len();

    // Summary line
    let avg_mhz = freqs.iter().sum::<f32>() / n as f32;
    let summary = format!("{n} cores   avg {avg_mhz:.0} MHz   max {max_mhz:.0} MHz");
    let sw = text_width(&bold, PxScale::from(16.0), &summary);
    draw_text_mut(
        &mut img,
        SUBTEXT,
        (480 - sw).max(0) / 2,
        50,
        PxScale::from(16.0),
        &bold,
        &summary,
    );

    // Grid layout: up to 4 columns
    let cols: i32 = if n <= 8 { 2 } else { 4 };
    let rows = ((n as i32) + cols - 1) / cols;
    let cell_w = 480 / cols;
    let cell_h: i32 = ((480 - 72) / rows).min(54);
    let start_y: i32 = 72;

    for (i, &mhz) in freqs.iter().enumerate().take((cols * rows) as usize) {
        let col = (i as i32) % cols;
        let row = (i as i32) / cols;
        let x = col * cell_w;
        let y = start_y + row * cell_h;

        let frac = mhz / max_mhz;
        let color = if frac >= 0.90 {
            GREEN
        } else if frac >= 0.60 {
            TEAL
        } else if frac >= 0.30 {
            YELLOW
        } else {
            BLUE
        };

        // Cell background (alternate)
        if (col + row) % 2 == 0 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(x, y).of_size(cell_w as u32, cell_h as u32),
                SURFACE0,
            );
        }

        // Core label + MHz
        let core_label = format!("C{i}");
        let freq_str = format!("{mhz:.0}");
        let unit_str = "MHz";

        let label_scale = PxScale::from(13.0);
        let freq_scale = PxScale::from(if cell_h >= 42 { 22.0 } else { 18.0 });

        draw_text_mut(
            &mut img,
            OVERLAY0,
            x + 4,
            y + 3,
            label_scale,
            &mono,
            &core_label,
        );
        draw_text_mut(&mut img, color, x + 4, y + 18, freq_scale, &mono, &freq_str);
        let unit_y = y + 18 + if cell_h >= 42 { 20 } else { 16 };
        if unit_y + 14 < y + cell_h {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                x + 4,
                unit_y,
                PxScale::from(11.0),
                &mono,
                unit_str,
            );
        }

        // Mini progress bar at bottom of cell
        let bar_y = y + cell_h - 5;
        let bar_w = cell_w - 4;
        draw_filled_rect_mut(
            &mut img,
            Rect::at(x + 2, bar_y).of_size(bar_w as u32, 3),
            OVERLAY0,
        );
        let fill = ((bar_w as f32 * frac) as i32).max(0);
        if fill > 0 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(x + 2, bar_y).of_size(fill as u32, 3),
                color,
            );
        }
    }

    DynamicImage::ImageRgb8(img)
}

// ─── Hostname & IP display ────────────────────────────────────────────────────

/// Read all non-loopback IPv4/IPv6 addresses for all interfaces.
pub(super) fn read_host_addrs() -> Vec<(String, Vec<String>)> {
    // Read from /proc/net/fib_trie for IPv4 and /proc/net/if_inet6 for IPv6.
    // Returns Vec<(ifname, Vec<addr_string>)>
    let mut result: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    // IPv4: /proc/net/fib_trie
    // Alternatively, parse /proc/net/if_inet6 for IPv6, and /proc/net/dev for interface names.
    // Simplest reliable approach: read /sys/class/net/ for interface names, then
    // read /proc/net/fib_trie for LOCAL entries and correlate.
    // Actually use a simpler approach: iterate /proc/net/arp entries + parse /proc/net/fib_trie

    // Read IPv4 addresses from /proc/net/fib_trie
    // Format we care about: lines after "  LOCAL" that contain the address
    if let Ok(_content) = std::fs::read_to_string("/proc/net/fib_trie") {
        // Parse fib_trie: look for "<ifname>" lines in fib_triestat or use another approach
        // Simplest: use ip addresses from /proc/net/if_inet6 for v6 and
        // parse /proc/net/dev for interface list, then read each interface address
    }

    // Use /sys/class/net for interface names
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let ifname = entry.file_name().to_string_lossy().to_string();
            if ifname == "lo" {
                continue;
            }
            let mut addrs = Vec::new();

            // IPv4: read from /proc/net/fib_trie — find LOCAL entries
            // Then match via /proc/net/fib_triestat which interface they belong to
            // Alternative simpler: run `ip addr show <ifname>` — but that requires a subprocess

            // Best no-subprocess approach: read /proc/net/if_inet6 (IPv6) and correlate index with
            // /sys/class/net/<if>/ifindex, then for IPv4 read /proc/net/fib_trie

            // IPv6
            if let Ok(v6data) = std::fs::read_to_string("/proc/net/if_inet6") {
                // Format: addr(32 hex) ifindex prefix_len scope flags ifname
                for line in v6data.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 6 && parts[5] == ifname {
                        let hex = parts[0];
                        // Skip link-local (fe80) and loopback
                        if hex.starts_with("fe80") || hex.starts_with("0000") {
                            continue;
                        }
                        // Format as IPv6 with colons
                        let groups: Vec<String> = (0..8)
                            .map(|i| hex[i * 4..(i + 1) * 4].to_string())
                            .collect();
                        let v6_str = groups.join(":");
                        // prefix length
                        let prefix = u8::from_str_radix(parts[2], 16).unwrap_or(0);
                        addrs.push(format!("{v6_str}/{prefix}"));
                    }
                }
            }

            if !addrs.is_empty() || {
                // Check if interface is up
                let flags_path = format!("/sys/class/net/{ifname}/flags");
                std::fs::read_to_string(&flags_path)
                    .ok()
                    .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
                    .is_some_and(|f| f & 1 != 0)
            } {
                result.entry(ifname.clone()).or_default().extend(addrs);
            }
        }
    }

    // IPv4: parse /proc/net/fib_trie for LOCAL host entries
    // Then determine interface from /proc/net/fib_triestat is complex.
    // Use simpler: read each interface's ipv4 addr from a subprocess-free method.
    // Scan /proc/net/fib_trie for LEAF + LOCAL addresses:
    //   - each leaf has lines like "  +-- <ip> <prefix>" and then children
    //   - LOCAL entries appear under each interface's subtree
    // This is complex to parse correctly. Instead, use /proc/net/dev to list interfaces,
    // then use SIOCGIFADDR via FFI — too complex.
    // Simplest robust approach: run "ip -o -4 addr" as subprocess.
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-4", "addr", "show"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            // Format: <idx>: <ifname>    inet <addr>/<prefix> brd <brd> scope ...
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "inet" {
                let ifname = parts[1].trim_end_matches(':').to_string();
                let addr = parts[3]; // "addr/prefix"
                if ifname != "lo" {
                    result.entry(ifname).or_default().push(addr.to_string());
                }
            }
        }
    }

    result.into_iter().collect()
}

/// Render hostname and all network IP addresses — useful for quick machine identification.
pub fn render_hostname_ip() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Title bar
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 56), SURFACE0);
    draw_centered(&mut img, &bold, PxScale::from(26.0), 14, BLUE, "MACHINE ID");

    // Hostname
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_else(|_| "unknown".to_string());
    let hostname = hostname.trim();

    // Draw big hostname
    let host_scale = if hostname.len() <= 16 {
        38.0
    } else if hostname.len() <= 24 {
        28.0
    } else {
        22.0
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(host_scale),
        68,
        TEXT,
        hostname,
    );

    // Divider
    draw_divider(&mut img, 116);

    // Network interfaces
    let ifaces = read_host_addrs();
    let row_h = 42i32;
    let mut y = 122i32;
    let max_rows = 8;
    let mut count = 0;

    if ifaces.is_empty() {
        draw_centered(
            &mut img,
            &mono,
            PxScale::from(18.0),
            y,
            OVERLAY0,
            "no network interfaces",
        );
    }

    for (ifname, addrs) in &ifaces {
        if count >= max_rows {
            break;
        }

        // Interface name pill background
        let pill_w = 120i32;
        draw_filled_rect_mut(
            &mut img,
            Rect::at(12, y).of_size(pill_w as u32, 28),
            SURFACE0,
        );
        draw_text_mut(
            &mut img,
            TEAL,
            18,
            y + 5,
            PxScale::from(16.0),
            &mono,
            ifname,
        );

        // Addresses
        let mut addr_y = y;
        if addrs.is_empty() {
            draw_text_mut(
                &mut img,
                OVERLAY0,
                pill_w + 22,
                addr_y + 5,
                PxScale::from(15.0),
                &mono,
                "(no addr)",
            );
        }
        for addr in addrs {
            if count >= max_rows {
                break;
            }
            let color = if addr.contains(':') { MAUVE } else { GREEN };
            draw_text_mut(
                &mut img,
                color,
                pill_w + 22,
                addr_y + 5,
                PxScale::from(15.0),
                &mono,
                addr,
            );
            addr_y += row_h / 2;
            count += 1;
        }

        y += row_h.max(addr_y - y + 8);
        count += 1;
    }

    // Footer: uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0) as u64;
    let d = uptime_secs / 86400;
    let h = (uptime_secs % 86400) / 3600;
    let m = (uptime_secs % 3600) / 60;
    let uptime_str = if d > 0 {
        format!("up {d}d {h:02}h {m:02}m")
    } else if h > 0 {
        format!("up {h}h {m:02}m")
    } else {
        format!("up {m}m")
    };
    draw_divider(&mut img, 452);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(16.0),
        456,
        SUBTEXT,
        &uptime_str,
    );

    DynamicImage::ImageRgb8(img)
}

// ─── Uptime / Boot time display ──────────────────────────────────────────────

/// Render a full-screen uptime/boot time display.
/// Shows: big uptime counter, boot timestamp, load averages, and a visual progress bar.
pub fn render_uptime() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // Read uptime from /proc/uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0) as u64;

    let days = uptime_secs / 86400;
    let hours = (uptime_secs % 86400) / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let seconds = uptime_secs % 60;

    // Read load averages from /proc/loadavg
    let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load_parts: Vec<&str> = loadavg.split_whitespace().collect();
    let load1 = load_parts.first().copied().unwrap_or("0.0");
    let load5 = load_parts.get(1).copied().unwrap_or("0.0");
    let load15 = load_parts.get(2).copied().unwrap_or("0.0");

    // Calculate boot time (current time - uptime)
    let now = Local::now();
    let boot_time = now - chrono::Duration::seconds(uptime_secs as i64);
    let boot_str = boot_time.format("%Y-%m-%d %H:%M:%S").to_string();

    // Header
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(24.0),
        12,
        BLUE,
        "SYSTEM UPTIME",
    );
    let now_str = now.format("%H:%M:%S").to_string();
    draw_centered(&mut img, &mono, PxScale::from(13.0), 36, OVERLAY0, &now_str);

    // Big uptime counter
    let day_str = if days > 0 {
        format!("{days}d ")
    } else {
        String::new()
    };
    let time_str = format!("{day_str}{hours:02}:{minutes:02}:{seconds:02}");
    let big_scale = if time_str.len() <= 8 {
        72.0f32
    } else if time_str.len() <= 11 {
        58.0
    } else {
        46.0
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(big_scale),
        80,
        TEAL,
        &time_str,
    );

    // Days / hours / minutes labels
    if days > 0 {
        let labels = format!("{days} days, {hours} hours, {minutes} minutes");
        draw_centered(&mut img, &mono, PxScale::from(15.0), 158, SUBTEXT, &labels);
    } else {
        let labels = format!("{hours} hours, {minutes} minutes, {seconds} seconds");
        draw_centered(&mut img, &mono, PxScale::from(15.0), 158, SUBTEXT, &labels);
    }

    // Divider
    draw_divider(&mut img, 184);

    // Boot time
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        190,
        OVERLAY0,
        "Boot time",
    );
    draw_centered(&mut img, &bold, PxScale::from(22.0), 208, TEXT, &boot_str);

    // Progress bar: fraction of a week elapsed
    let week_secs = 7 * 86400u64;
    let frac = ((uptime_secs % week_secs) as f32 / week_secs as f32).min(1.0);
    draw_divider(&mut img, 254);
    draw_text_mut(
        &mut img,
        OVERLAY0,
        12,
        260,
        PxScale::from(13.0),
        &mono,
        "Week progress",
    );
    let bar_x = 12;
    let bar_y = 278;
    let bar_w = 456;
    draw_bar(&mut img, bar_x, bar_y, bar_w, 14, frac, BLUE);
    let frac_pct = format!("{:.1}%", frac * 100.0);
    let fw = text_width(&mono, PxScale::from(13.0), &frac_pct);
    draw_text_mut(
        &mut img,
        BLUE,
        bar_x + bar_w - fw - 2,
        260,
        PxScale::from(13.0),
        &mono,
        &frac_pct,
    );

    // Load averages
    draw_divider(&mut img, 306);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(14.0),
        312,
        OVERLAY0,
        "Load averages (1m / 5m / 15m)",
    );
    let load_str = format!("{load1} / {load5} / {load15}");
    let load1_f = load1.parse::<f32>().unwrap_or(0.0);
    let load_color = if load1_f > 4.0 {
        RED
    } else if load1_f > 2.0 {
        YELLOW
    } else {
        GREEN
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(28.0),
        332,
        load_color,
        &load_str,
    );

    // CPU count for context
    let cpu_count = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();
    if cpu_count > 0 {
        let cpu_str = format!("{cpu_count} CPU cores");
        draw_text_mut(
            &mut img,
            OVERLAY0,
            12,
            372,
            PxScale::from(13.0),
            &mono,
            &cpu_str,
        );
    }

    // Memory quick stats
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        let mut total_kb = 0u64;
        let mut available_kb = 0u64;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                available_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        }
        if total_kb > 0 {
            let used_kb = total_kb - available_kb;
            let mem_str = format!(
                "RAM: {:.1}/{:.1} GB",
                used_kb as f64 / 1_048_576.0,
                total_kb as f64 / 1_048_576.0
            );
            draw_text_mut(
                &mut img,
                SUBTEXT,
                12,
                392,
                PxScale::from(13.0),
                &mono,
                &mem_str,
            );
        }
    }

    // Footer
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let hostname = hostname.trim();
    draw_divider(&mut img, 452);
    draw_centered(&mut img, &mono, PxScale::from(15.0), 456, SUBTEXT, hostname);

    DynamicImage::ImageRgb8(img)
}

// ─── OS / System info display ─────────────────────────────────────────────────

/// Render a comprehensive OS and system information display.
pub fn render_os_info() -> DynamicImage {
    let mut img = blank();
    let bold = font_bold();
    let mono = font_mono();

    // --- Gather info ---
    // OS name
    let os_pretty = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("PRETTY_NAME=")
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Linux".to_string());

    // Kernel version
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "unknown".to_string());
    let kernel = kernel.trim().to_string();

    // Architecture
    let arch = std::fs::read_to_string("/proc/sys/kernel/arch")
        .or_else(|_| {
            std::process::Command::new("uname")
                .arg("-m")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .map_err(std::io::Error::other)
        })
        .unwrap_or_else(|_| "x86_64".to_string());
    let arch = arch.trim().to_string();

    // Hostname
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let hostname = hostname.trim().to_string();

    // CPU model
    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("model name")
                .map(|r| r.trim_start_matches(['\t', ':', ' ']).to_string())
        })
        .unwrap_or_else(|| "Unknown CPU".to_string());
    // Shorten CPU name
    let cpu_model = cpu_model
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("  ", " ");
    let cpu_short: String = cpu_model.chars().take(32).collect();

    // CPU core count
    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    // Memory total
    let mem_total_kb: u64 = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("MemTotal:")
                .and_then(|r| r.split_whitespace().next().and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0);

    // Uptime
    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0) as u64;
    let up_days = uptime_secs / 86400;
    let up_hours = (uptime_secs % 86400) / 3600;
    let uptime_str = if up_days > 0 {
        format!("{up_days}d {up_hours}h up")
    } else {
        let up_mins = (uptime_secs % 3600) / 60;
        format!("{up_hours}h {up_mins}m up")
    };

    // --- Header ---
    draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(480, 52), SURFACE0);
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(22.0),
        6,
        MAUVE,
        "SYSTEM INFO",
    );
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        32,
        OVERLAY0,
        &Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    );

    // --- OS Name (big) ---
    let os_scale = if os_pretty.len() <= 22 {
        28.0f32
    } else if os_pretty.len() <= 30 {
        22.0
    } else {
        18.0
    };
    draw_centered(
        &mut img,
        &bold,
        PxScale::from(os_scale),
        60,
        TEXT,
        &os_pretty,
    );

    draw_divider(&mut img, 98);

    // --- Info rows ---
    let row_h = 34i32;
    let mut y = 104i32;
    let label_scale = PxScale::from(14.0);
    let value_scale = PxScale::from(17.0);

    let rows: &[(&str, String, Rgb<u8>)] = &[
        ("Hostname", hostname.clone(), TEAL),
        ("Kernel", format!("{kernel} ({arch})"), BLUE),
        ("CPU", cpu_short.clone(), SUBTEXT),
        ("Cores", format!("{cpu_cores} logical cores"), YELLOW),
        (
            "Memory",
            format!("{:.1} GB RAM", mem_total_kb as f64 / 1_048_576.0),
            GREEN,
        ),
        ("Uptime", uptime_str, PEACH),
    ];

    for (label, value, color) in rows {
        // Label
        draw_text_mut(&mut img, OVERLAY0, 12, y + 8, label_scale, &mono, label);
        // Value (right-aligned or after a separator)
        let val_short: String = value.chars().take(30).collect();
        let vw = text_width(&mono, value_scale, &val_short);
        draw_text_mut(
            &mut img,
            *color,
            480 - vw - 12,
            y + 5,
            value_scale,
            &bold,
            &val_short,
        );
        // Light divider
        if y + row_h < 440 {
            draw_filled_rect_mut(
                &mut img,
                Rect::at(0, y + row_h - 1).of_size(480, 1),
                SURFACE0,
            );
        }
        y += row_h;
    }

    // --- Footer ---
    draw_divider(&mut img, 452);
    draw_centered(
        &mut img,
        &mono,
        PxScale::from(13.0),
        457,
        OVERLAY0,
        &format!("ttlcd v{}", env!("CARGO_PKG_VERSION")),
    );

    DynamicImage::ImageRgb8(img)
}
// ─── Disk Pie Chart ───────────────────────────────────────────────────────────

/// Render disk partitions as a pie chart with used/free sectors and labels.
pub fn render_disk_pie() -> DynamicImage {
    use std::f64::consts::PI;
    use sysinfo::{DiskRefreshKind, Disks};

    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh_specifics(false, DiskRefreshKind::everything());

    // Collect up to 5 mounted partitions with real data
    let mut entries: Vec<(String, u64, u64)> = Vec::new(); // (mount, used, total)
    for d in &disks {
        if d.total_space() == 0 {
            continue;
        }
        let mount = d.mount_point().to_string_lossy().to_string();
        let total = d.total_space();
        let avail = d.available_space();
        let used = total.saturating_sub(avail);
        entries.push((mount, used, total));
        if entries.len() >= 5 {
            break;
        }
    }

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Title
    let title = "Disk Usage";
    let tw = text_width(&font, PxScale::from(28.0), title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        10,
        PxScale::from(28.0),
        &font,
        title,
    );

    if entries.is_empty() {
        let msg = "No disk data";
        let mw = text_width(&font, PxScale::from(22.0), msg);
        draw_text_mut(
            &mut img,
            SUBTEXT,
            (480 - mw) / 2,
            220,
            PxScale::from(22.0),
            &font,
            msg,
        );
        return DynamicImage::ImageRgb8(img);
    }

    // Color palette for different partitions
    let slice_colors = [BLUE, MAUVE, TEAL, PEACH, YELLOW];
    let free_color = SURFACE0;

    // Draw up to 5 pies in a grid (2 columns × 3 rows)
    let pie_r = 72_i32;
    let positions = [
        (120i32, 160i32),
        (360, 160),
        (120, 320),
        (360, 320),
        (240, 460),
    ];

    for (idx, (mount, used, total)) in entries.iter().enumerate() {
        if idx >= positions.len() {
            break;
        }
        let (cx, cy) = positions[idx];
        let used_pct = *used as f64 / *total as f64;
        let slice_col = slice_colors[idx % slice_colors.len()];

        // Draw pie background (free)
        for dy in -pie_r..=pie_r {
            for dx in -pie_r..=pie_r {
                if dx * dx + dy * dy <= pie_r * pie_r {
                    let px = (cx + dx) as u32;
                    let py = (cy + dy) as u32;
                    if px < 480 && py < 480 {
                        img.put_pixel(px, py, free_color);
                    }
                }
            }
        }

        // Draw used sector (start from top = -PI/2)
        let used_angle = used_pct * 2.0 * PI;
        for deg_10 in 0..3600_i32 {
            let a = deg_10 as f64 * 0.1_f64.to_radians();
            if a > used_angle {
                break;
            }
            let draw_a = a - PI / 2.0; // Start from 12 o'clock
            for rr in 0..pie_r {
                let px = (cx as f64 + rr as f64 * draw_a.cos()) as i32;
                let py = (cy as f64 + rr as f64 * draw_a.sin()) as i32;
                if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                    img.put_pixel(px as u32, py as u32, slice_col);
                }
            }
        }

        // Outline circle
        for deg in 0..3600_i32 {
            let a = deg as f64 * 0.1_f64.to_radians();
            let px = (cx as f64 + pie_r as f64 * a.cos()) as i32;
            let py = (cy as f64 + pie_r as f64 * a.sin()) as i32;
            if px >= 0 && py >= 0 && (px as u32) < 480 && (py as u32) < 480 {
                img.put_pixel(px as u32, py as u32, OVERLAY0);
            }
        }

        // Labels
        let pct_str = format!("{:.0}%", used_pct * 100.0);
        let pw = text_width(&font, PxScale::from(20.0), &pct_str);
        draw_text_mut(
            &mut img,
            WHITE,
            cx - pw / 2,
            cy - 10,
            PxScale::from(20.0),
            &font,
            &pct_str,
        );

        // Mount point label (truncate at 8 chars)
        let short_mount = if mount.len() > 10 {
            &mount[mount.len() - 10..]
        } else {
            mount.as_str()
        };
        let mw2 = text_width(&font, PxScale::from(15.0), short_mount);
        draw_text_mut(
            &mut img,
            SUBTEXT,
            cx - mw2 / 2,
            cy + pie_r + 4,
            PxScale::from(15.0),
            &font,
            short_mount,
        );

        // Size label
        let sz_str = human_bytes(*total as f64);
        let sw2 = text_width(&font, PxScale::from(14.0), &sz_str);
        draw_text_mut(
            &mut img,
            OVERLAY0,
            cx - sw2 / 2,
            cy + pie_r + 20,
            PxScale::from(14.0),
            &font,
            &sz_str,
        );
    }

    DynamicImage::ImageRgb8(img)
}

pub(super) fn human_bytes(bytes: f64) -> String {
    if bytes >= 1_073_741_824.0 {
        format!("{:.1}G", bytes / 1_073_741_824.0)
    } else if bytes >= 1_048_576.0 {
        format!("{:.0}M", bytes / 1_048_576.0)
    } else if bytes >= 1024.0 {
        format!("{:.0}K", bytes / 1024.0)
    } else {
        format!("{bytes:.0}B")
    }
}

// ─── System Temperatures Dashboard ───────────────────────────────────────────

/// Render a detailed temperature + fan sensor dashboard with bar gauges.
pub fn render_thermal_dashboard() -> DynamicImage {
    use sysinfo::Components;

    let comps = Components::new_with_refreshed_list();

    let font = font_bold();
    let mut img = RgbImage::new(480, 480);
    for p in img.pixels_mut() {
        *p = BG;
    }

    // Title
    let title = "Thermal Dashboard";
    let tw = text_width(&font, PxScale::from(26.0), title);
    draw_text_mut(
        &mut img,
        TEXT,
        (480 - tw) / 2,
        8,
        PxScale::from(26.0),
        &font,
        title,
    );

    // Collect sensor readings
    let mut sensors: Vec<(String, f32, f32)> = Vec::new(); // (label, current, max_observed)
    for c in &comps {
        let label = c.label().to_string();
        if let Some(t) = c.temperature() {
            if t > 0.0 && t < 200.0 {
                let max = c.max().unwrap_or(t);
                sensors.push((label, t, max));
            }
        }
        if sensors.len() >= 10 {
            break;
        }
    }

    if sensors.is_empty() {
        let msg = "No thermal sensors found";
        let mw = text_width(&font, PxScale::from(20.0), msg);
        draw_text_mut(
            &mut img,
            SUBTEXT,
            (480 - mw) / 2,
            220,
            PxScale::from(20.0),
            &font,
            msg,
        );
        return DynamicImage::ImageRgb8(img);
    }

    // ── Draw sensor rows ──────────────────────────────────────────────────
    let row_h = (440 / sensors.len().max(1)) as i32;
    let bar_x = 180_i32;
    let bar_w = 260_i32;
    let bar_h = (row_h - 10).max(14);

    for (i, (label, temp, _max_obs)) in sensors.iter().enumerate() {
        let y = 45 + i as i32 * row_h;

        // Temperature color scale: <50°C=blue, <70°C=green, <85°C=yellow, >=85°C=red
        let col = if *temp < 50.0 {
            BLUE
        } else if *temp < 70.0 {
            GREEN
        } else if *temp < 85.0 {
            YELLOW
        } else {
            RED
        };

        // Sensor label (truncate at 20 chars)
        let short = if label.len() > 20 {
            &label[..20]
        } else {
            label.as_str()
        };
        draw_text_mut(
            &mut img,
            SUBTEXT,
            8,
            y + 2,
            PxScale::from(15.0),
            &font,
            short,
        );

        // Background bar
        let bg_rect = Rect::at(bar_x, y).of_size(bar_w as u32, bar_h as u32);
        draw_filled_rect_mut(&mut img, bg_rect, SURFACE0);

        // Value bar (scale: 0°C=0, 100°C=full bar)
        let bar_fill = (((*temp as f64 / 100.0).clamp(0.0, 1.0)) * bar_w as f64) as i32;
        if bar_fill > 0 {
            let val_rect = Rect::at(bar_x, y).of_size(bar_fill as u32, bar_h as u32);
            draw_filled_rect_mut(&mut img, val_rect, col);
        }

        // Temperature text overlay
        let temp_str = format!("{temp:.0}°C");
        let tsw = text_width(&font, PxScale::from(15.0), &temp_str);
        draw_text_mut(
            &mut img,
            if bar_fill > 30 { BG } else { TEXT },
            bar_x + bar_fill.max(4) - 2,
            y + 2,
            PxScale::from(15.0),
            &font,
            &temp_str,
        );
        let _ = tsw;
    }

    DynamicImage::ImageRgb8(img)
}
