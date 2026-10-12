//! The system snapshot card and the panel hardware info table.

use web_sys::CanvasRenderingContext2d;

use crate::api;
use crate::dom::{self, El};
use crate::js::Js;
use crate::util::{esc, fmt_bytes, fmt_uptime, load_color, plural, prefix};

/// USB ids of the panel this driver talks to.
const PANEL_VID: &str = "264a";
const PANEL_PID: &str = "2343";

pub fn init() {
    inject_fps_sparkline();
    inject_net_sparkline();
    El::id("list-usb-btn").on_async("click", |_| async {
        let btn = El::id("list-usb-btn");
        btn.set_disabled(true);
        fetch_usb_devices().await;
        btn.set_disabled(false);
    });
}

// ─── Panel info ──────────────────────────────────────────────────────────────

pub async fn fetch_panel_info() {
    let Ok(d) = api::get("/api/panel/info").await else {
        return;
    };
    El::id("info-host").set_text(&dom::win().location().host().unwrap_or_default());
    let serial = d.get("serial").str().unwrap_or_else(|| {
        if d.get("headless").truthy() {
            "(headless)"
        } else {
            "—"
        }
        .to_string()
    });
    El::id("info-serial").set_text(&serial);
    let firmware = d
        .get("firmware_byte")
        .num()
        .map_or("—".to_string(), |b| format!("0x{:02x}", b as u32));
    El::id("info-firmware").set_text(&firmware);
}

async fn fetch_usb_devices() {
    let Ok(d) = api::get("/api/usb/devices").await else {
        return;
    };
    let count = d.get("count").num_or(0.0) as usize;
    El::id("info-usb-count").set_text(&format!("{count} device{}", plural(count)));
    let rows: String = d
        .get("devices")
        .arr()
        .iter()
        .map(|dev| {
            let (vid, pid) = (dev.get("vid").string(), dev.get("pid").string());
            let label = dev
                .get("product")
                .str()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| format!("{vid}:{pid}"));
            let serial = dev
                .get("serial")
                .str()
                .filter(|s| !s.is_empty())
                .map_or(String::new(), |s| format!(" ({s})"));
            let style = if vid == PANEL_VID && pid == PANEL_PID {
                "color:var(--green);font-weight:600"
            } else {
                ""
            };
            format!(
                r#"<div style="{style}">{}:{} — {}{}</div>"#,
                esc(&vid),
                esc(&pid),
                esc(&label),
                esc(&serial)
            )
        })
        .collect();
    El::id("info-usb-list").set_html(&rows);
}

// ─── Snapshot ────────────────────────────────────────────────────────────────

pub async fn fetch_snapshot() {
    let Ok(d) = api::get("/api/sys/snapshot").await else {
        return;
    };

    let cpu = d.get("cpu_pct").num_or(0.0);
    let ram = match (d.get("mem_used").num(), d.get("mem_total").num()) {
        (Some(used), Some(total)) if total > 0.0 => used / total * 100.0,
        _ => 0.0,
    };
    El::id("stat-cpu").set_text(&format!("{cpu:.1}%"));
    El::id("stat-ram").set_text(&format!("{ram:.1}%"));
    El::id("bar-cpu")
        .css("width", &format!("{cpu}%"))
        .css("background", load_color(cpu, "var(--blue)"));
    El::id("bar-ram")
        .css("width", &format!("{ram}%"))
        .css("background", load_color(ram, "var(--green)"));

    let load: Vec<String> = d.get("load_avg").arr().iter().map(Js::show).collect();
    if !load.is_empty() {
        El::id("stat-load").set_text(&load.join(" / "));
    }
    if let Some(up) = d.get("uptime_secs").num() {
        El::id("stat-uptime").set_text(&fmt_uptime(up));
    }

    let hottest = d.get("temperatures").arr().into_iter().max_by(|a, b| {
        a.get("current")
            .num_or(0.0)
            .total_cmp(&b.get("current").num_or(0.0))
    });
    if let Some(t) = hottest {
        let current = t.get("current").num_or(0.0);
        let critical = t
            .get("critical")
            .num()
            .filter(|c| *c > 0.0)
            .unwrap_or(105.0);
        let pct = (current / critical * 100.0).min(100.0);
        let color = if pct > 80.0 {
            "var(--red)"
        } else if pct > 60.0 {
            "var(--peach)"
        } else {
            "var(--teal)"
        };
        El::id("stat-temp-block").show();
        El::id("stat-temp").set_text(&format!("{current:.1}°C"));
        El::id("bar-temp")
            .css("width", &format!("{pct}%"))
            .css("background", color);
    }

    let network = d.get("network").arr();
    if !network.is_empty() {
        let total = |key: &str| network.iter().map(|n| n.get(key).num_or(0.0)).sum::<f64>();
        El::id("stat-net-block").show();
        El::id("stat-net").set_text(&format!(
            "↓{}/s ↑{}/s",
            fmt_bytes(total("rx_bps")),
            fmt_bytes(total("tx_bps"))
        ));
        El::id("net-ifaces-wrap").show();
        let rows: String = network
            .iter()
            .map(|n| {
                format!(
                    r#"<tr><td class="mono">{}</td><td class="mono">{}/s</td><td class="mono">{}/s</td><td class="mono">{}</td><td class="mono">{}</td></tr>"#,
                    esc(&n.get("name").string()),
                    fmt_bytes(n.get("rx_bps").num_or(0.0)),
                    fmt_bytes(n.get("tx_bps").num_or(0.0)),
                    fmt_bytes(n.get("total_rx").num_or(0.0)),
                    fmt_bytes(n.get("total_tx").num_or(0.0)),
                )
            })
            .collect();
        El::id("net-ifaces-table").find("tbody").set_html(&rows);
    }

    let disks = d.get("disks").arr();
    if !disks.is_empty() {
        update_disk_table(&disks);
    }
    let procs = d.get("processes").arr();
    if !procs.is_empty() {
        update_process_table(&procs);
    }
    update_cpu_sparkline().await;
}

fn update_disk_table(disks: &[Js]) {
    let mut wrap = El::id("disk-usage-wrap");
    if !wrap.ok() {
        wrap = El::build(
            "div",
            "",
            r#"<div class="sub-label">Disk Usage</div>
      <table class="iface-table" id="disk-table">
        <thead><tr><th>Mount</th><th>Used</th><th>Total</th><th>Used%</th></tr></thead>
        <tbody></tbody>
      </table>"#,
        );
        wrap.set_id("disk-usage-wrap").css("margin-top", "0.75rem");
        El::id("net-ifaces-wrap").insert_before(&wrap);
    }
    wrap.show();
    let rows: String = disks
        .iter()
        .filter_map(|disk| {
            let total = disk.get("total_bytes").num().filter(|t| *t > 0.0)?;
            let used = disk.get("used_bytes").num_or(0.0);
            let pct = used / total * 100.0;
            let color = if pct > 85.0 {
                "var(--red)"
            } else if pct > 70.0 {
                "var(--yellow)"
            } else {
                "var(--teal)"
            };
            let name = [disk.get("name").string(), disk.get("mount_point").string()]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| "?".into());
            Some(format!(
                r#"<tr>
      <td class="mono">{}</td>
      <td class="mono">{}</td>
      <td class="mono">{}</td>
      <td>
        <div class="mini-bar-track" style="width:80px;display:inline-block">
          <div class="mini-bar" style="width:{pct:.0}%;background:{color}"></div>
        </div>
        <span class="mono" style="font-size:0.75rem;margin-left:4px">{pct:.0}%</span>
      </td>
    </tr>"#,
                esc(&name),
                fmt_bytes(used),
                fmt_bytes(total),
            ))
        })
        .collect();
    wrap.find("tbody").set_html(&rows);
}

fn update_process_table(procs: &[Js]) {
    let mut wrap = El::id("proc-wrap");
    if !wrap.ok() {
        wrap = El::build(
            "div",
            "",
            r#"<div class="sub-label">Top Processes <span class="hint-inline">(CPU)</span></div>
      <table class="iface-table" id="proc-table">
        <thead><tr><th>PID</th><th>Name</th><th>CPU%</th><th>RAM</th></tr></thead>
        <tbody></tbody>
      </table>"#,
        );
        wrap.set_id("proc-wrap").css("margin-top", "0.75rem");
        El::id("sparkline-wrap").insert_after(&wrap);
    }
    wrap.show();
    let rows: String = procs
        .iter()
        .take(6)
        .map(|p| {
            let cpu = p.get("cpu_pct").num_or(0.0);
            let color = if cpu > 50.0 {
                "var(--red)"
            } else if cpu > 20.0 {
                "var(--yellow)"
            } else {
                "var(--green)"
            };
            format!(
                r#"<tr>
      <td class="mono" style="color:var(--overlay0)">{}</td>
      <td class="mono">{}</td>
      <td class="mono" style="color:{color}">{cpu:.1}%</td>
      <td class="mono">{}</td>
    </tr>"#,
                esc(&p.get("pid").show()),
                esc(&prefix(&p.get("name").string(), 20)),
                fmt_bytes(p.get("mem_bytes").num_or(0.0)),
            )
        })
        .collect();
    wrap.find("tbody").set_html(&rows);
}

// ─── Sparklines ──────────────────────────────────────────────────────────────

fn samples(d: &Js) -> Vec<f64> {
    d.get("samples")
        .arr()
        .iter()
        .map(|v| v.num_or(0.0))
        .collect()
}

/// Clear a canvas to the shared translucent background.
fn clear(ctx: &CanvasRenderingContext2d, w: f64, h: f64, alpha: &str) {
    ctx.clear_rect(0.0, 0.0, w, h);
    ctx.set_fill_style_str(&format!("rgba(49,50,68,{alpha})"));
    ctx.fill_rect(0.0, 0.0, w, h);
}

async fn update_cpu_sparkline() {
    let Ok(d) = api::get("/api/cpu/history").await else {
        return;
    };
    let data = samples(&d);
    if data.is_empty() {
        return;
    }
    El::id("sparkline-wrap").show();
    let Some((ctx, w, h)) = El::id("cpu-sparkline").canvas() else {
        return;
    };
    clear(&ctx, w, h, "0.4");

    // Gridlines at 25 %, 50 % and 75 %.
    ctx.set_stroke_style_str("rgba(108,112,134,0.25)");
    ctx.set_line_width(1.0);
    for fraction in [0.25, 0.5, 0.75] {
        let y = h - h * fraction;
        ctx.begin_path();
        ctx.move_to(0.0, y);
        ctx.line_to(w, y);
        ctx.stroke();
    }

    let bar_w = w / data.len() as f64;
    for (i, v) in data.iter().enumerate() {
        let bar_h = (h * (v / 100.0).min(1.0)).round();
        ctx.set_fill_style_str(if *v > 85.0 {
            "#f38ba8"
        } else if *v > 60.0 {
            "#f9e2af"
        } else {
            "#89b4fa"
        });
        ctx.fill_rect(i as f64 * bar_w, h - bar_h, bar_w - 1.0, bar_h);
    }
}

fn inject_fps_sparkline() {
    let row = El::build(
        "tr",
        "",
        r#"<td class="info-label">FPS graph</td><td><canvas id="fps-sparkline-canvas" width="200" height="28" style="vertical-align:middle;border-radius:3px"></canvas></td>"#,
    );
    row.set_id("fps-sparkline-row");
    El::id("info-fps").closest("tr").insert_after(&row);
}

pub async fn fetch_fps_history() {
    let Ok(d) = api::get("/api/fps/history").await else {
        return;
    };
    let data = samples(&d);
    let Some((ctx, w, h)) = El::id("fps-sparkline-canvas").canvas() else {
        return;
    };
    if data.is_empty() {
        return;
    }
    clear(&ctx, w, h, "0.6");
    let max = data.iter().copied().fold(1.0, f64::max);
    let bar_w = w / data.len() as f64;
    for (i, v) in data.iter().enumerate() {
        let bar_h = (h * (v / max)).round();
        ctx.set_fill_style_str(if *v < 0.5 {
            "#f38ba8"
        } else if *v < 1.5 {
            "#f9e2af"
        } else {
            "#a6e3a1"
        });
        ctx.fill_rect(i as f64 * bar_w, h - bar_h, (bar_w - 1.0).max(1.0), bar_h);
    }
}

fn inject_net_sparkline() {
    let wrap = El::build(
        "div",
        "",
        r#"<div class="sub-label">Net history <span class="hint-inline">(↓ Rx | ↑ Tx, last 60 s)</span></div>
<div style="display:flex;gap:4px">
<div style="flex:1"><div class="sub-label" style="font-size:0.7rem">↓ Rx</div>
  <canvas id="net-rx-sparkline" class="cpu-sparkline" width="210" height="45"></canvas></div>
<div style="flex:1"><div class="sub-label" style="font-size:0.7rem">↑ Tx</div>
  <canvas id="net-tx-sparkline" class="cpu-sparkline" width="210" height="45"></canvas></div>
</div>"#,
    );
    wrap.set_id("net-sparkline-wrap")
        .css_text("margin-top:0.5rem;display:none");
    El::id("sparkline-wrap").insert_after(&wrap);
}

pub async fn update_net_sparkline() {
    let Ok(d) = api::get("/api/net/history").await else {
        return;
    };
    let samples = d.get("samples").arr();
    if samples.is_empty() {
        return;
    }
    El::id("net-sparkline-wrap").show();
    let series = |key: &str| {
        samples
            .iter()
            .map(|s| s.get(key).num_or(0.0))
            .collect::<Vec<_>>()
    };
    draw_line(&El::id("net-rx-sparkline"), &series("rx_bps"), "#89b4fa");
    draw_line(&El::id("net-tx-sparkline"), &series("tx_bps"), "#a6e3a1");
}

fn draw_line(canvas: &El, data: &[f64], color: &str) {
    let Some((ctx, w, h)) = canvas.canvas() else {
        return;
    };
    clear(&ctx, w, h, "0.4");
    let max = data.iter().copied().fold(1.0, f64::max);
    ctx.begin_path();
    ctx.set_stroke_style_str(color);
    ctx.set_line_width(1.5);
    for (i, v) in data.iter().enumerate() {
        let x = if data.len() > 1 {
            i as f64 / (data.len() - 1) as f64 * w
        } else {
            w / 2.0
        };
        let y = h - v / max * (h - 4.0) - 2.0;
        if i == 0 {
            ctx.move_to(x, y);
        } else {
            ctx.line_to(x, y);
        }
    }
    ctx.stroke();
}
