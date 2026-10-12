//! Cards that are built at start-up rather than written in `index.html`:
//! stopwatch, alert thresholds, dim and mode schedules, metrics, profiles,
//! weather, notes, QR codes, webhooks, Pomodoro, ticker and world clock.
//!
//! Each card is placed relative to another one, so `init` runs them in a
//! fixed order.

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::api;
use crate::core::{self, log, log_err, or_toast, report, status, status_for, toast};
use crate::dom::{self, card, place_after, placeholder_item, El};
use crate::js::{arr, Js};
use crate::obj;
use crate::util::{
    esc, fmt_sw_time, int_or, mode_label, non_empty_or, parse_float, parse_int, utc_offset_label,
};

/// Colour names the daemon accepts for metrics and tickers.
const COLORS: [&str; 8] = [
    "blue", "green", "red", "yellow", "teal", "mauve", "peach", "white",
];
const MAX_METRICS: u32 = 8;
const MAX_NOTES: u32 = 10;
const MAX_WORLD_CITIES: usize = 4;
const MAX_POMODORO_SET: u32 = 12;
const FALLBACK_MODES: [&str; 5] = ["clock", "sysinfo", "cpu_detail", "network", "idle"];

pub fn init() {
    inject_stopwatch();
    inject_thresholds();
    inject_dim();
    inject_metrics();
    inject_metrics_source();
    inject_profiles();
    inject_weather();
    inject_notes();
    inject_qr();
    inject_webhook();
    inject_mode_schedule();
    inject_pomodoro();
    inject_world_clock();
    inject_ticker();
}

/// `<option>` tags for the colour names, with `selected` marked.
fn color_options(selected: &str) -> String {
    COLORS
        .iter()
        .map(|c| {
            format!(
                r#"<option value="{c}"{}>{c}</option>"#,
                if *c == selected { " selected" } else { "" }
            )
        })
        .collect()
}

// ─── Stopwatch ───────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Stopwatch {
    running: bool,
    base_elapsed_ms: f64,
    started_at_unix_ms: f64,
    elapsed_ms: f64,
    /// Interval id of the local display tick while running.
    timer: Option<i32>,
}

thread_local! {
    static STOPWATCH: RefCell<Stopwatch> = RefCell::new(Stopwatch::default());
    static WEATHER_TIMER: RefCell<Option<i32>> = const { RefCell::new(None) };
}

fn inject_stopwatch() {
    let section = card(
        "",
        r#"
    <h2>Stopwatch</h2>
    <div class="sw-main">
      <div class="stopwatch-display" id="sw-display">00:00.00</div>
      <div class="stopwatch-status" id="sw-status-label">READY</div>
    </div>
    <div class="form-actions" style="margin-top:0.75rem">
      <button id="sw-start-btn" class="primary">▶ Start</button>
      <button id="sw-lap-btn">⊙ Lap</button>
      <button id="sw-reset-btn">↺ Reset</button>
    </div>
    <table id="sw-laps-table" class="sw-laps-table" style="margin-top:0.75rem;width:100%;display:none">
      <thead><tr><th>#</th><th>Cumulative</th><th>Split</th></tr></thead>
      <tbody></tbody>
    </table>
  "#,
    );
    El::id("freeze-btn")
        .closest("section")
        .insert_after(&section);

    El::id("sw-start-btn").on_async("click", |_| async {
        let running = STOPWATCH.with(|s| s.borrow().running);
        let path = if running {
            "/api/stopwatch/stop"
        } else {
            "/api/stopwatch/start"
        };
        match api::post_empty(path).await {
            Ok(()) => fetch_stopwatch().await,
            Err(e) => log_err(&format!("Stopwatch error: {e}")),
        }
    });
    El::id("sw-lap-btn").on_async("click", |_| async {
        match api::json("POST", "/api/stopwatch/lap", None).await {
            Ok(r) => {
                log(&format!(
                    "Lap {}: {}",
                    r.get("lap").show(),
                    fmt_sw_time(r.get("elapsed_ms").num_or(0.0))
                ));
                fetch_stopwatch().await;
            }
            Err(e) => log_err(&format!("Lap error: {e}")),
        }
    });
    El::id("sw-reset-btn").on_async("click", |_| async {
        match api::post_empty("/api/stopwatch/reset").await {
            Ok(()) => {
                apply_stopwatch(Stopwatch::default(), &[]);
                log("Stopwatch reset");
            }
            Err(e) => log_err(&format!("Reset error: {e}")),
        }
    });
}

fn update_stopwatch_display() {
    let (ms, running, paused) = STOPWATCH.with(|s| {
        let s = s.borrow();
        let ms = if s.running && s.started_at_unix_ms > 0.0 {
            s.base_elapsed_ms + (dom::now_ms() - s.started_at_unix_ms)
        } else {
            s.elapsed_ms
        };
        (ms, s.running, s.elapsed_ms > 0.0)
    });
    El::id("sw-display").set_text(&fmt_sw_time(ms));
    let (text, class) = if running {
        ("RUNNING", "sw-status-label sw-running")
    } else if paused {
        ("PAUSED", "sw-status-label sw-paused")
    } else {
        ("READY", "sw-status-label ")
    };
    El::id("sw-status-label").set_text(text).set_class(class);
}

/// Adopt the daemon's stopwatch state and run the local tick only while it
/// is running.
fn apply_stopwatch(mut state: Stopwatch, laps: &[f64]) {
    STOPWATCH.with(|s| {
        let old_timer = s.borrow().timer;
        state.timer = match (state.running, old_timer) {
            (true, None) => Some(dom::set_interval(50, update_stopwatch_display)),
            (false, Some(id)) => {
                dom::clear_interval(id);
                None
            }
            (_, timer) => timer,
        };
        *s.borrow_mut() = state;
    });
    update_stopwatch_display();

    // Newest lap first.
    let rows: String = laps
        .iter()
        .enumerate()
        .rev()
        .map(|(i, total)| {
            let split = total - if i > 0 { laps[i - 1] } else { 0.0 };
            let class = if i + 1 == laps.len() { r#" class="sw-latest-lap""# } else { "" };
            format!(
                r#"<tr{class}><td>{}</td><td class="mono">{}</td><td class="mono sw-delta">+{}</td></tr>"#,
                i + 1,
                fmt_sw_time(*total),
                fmt_sw_time(split),
            )
        })
        .collect();
    let table = El::id("sw-laps-table");
    table.find("tbody").set_html(&rows);
    table.display(if laps.is_empty() { "none" } else { "" });
}

pub async fn fetch_stopwatch() {
    let Ok(d) = api::get("/api/stopwatch").await else {
        return;
    };
    let laps: Vec<f64> = d.get("laps").arr().iter().map(|l| l.num_or(0.0)).collect();
    let state = Stopwatch {
        running: d.get("running").truthy(),
        base_elapsed_ms: d.get("base_elapsed_ms").num_or(0.0),
        started_at_unix_ms: d.get("started_at_unix_ms").num_or(0.0),
        elapsed_ms: d.get("elapsed_ms").num_or(0.0),
        timer: None,
    };
    apply_stopwatch(state, &laps);
}

// ─── Alert thresholds ────────────────────────────────────────────────────────

fn inject_thresholds() {
    let section = card(
        "",
        r#"
    <h2>Alert Thresholds</h2>
    <p class="card-hint">Panel flashes a custom text alert when CPU or temperature exceeds these limits.</p>
    <div class="form-row three-col">
      <div class="form-group">
        <label for="thr-cpu">CPU % trigger <span class="hint-inline">(0 = off)</span></label>
        <input type="number" id="thr-cpu" min="0" max="100" value="0" step="1">
      </div>
      <div class="form-group">
        <label for="thr-temp">Temp °C trigger <span class="hint-inline">(0 = off)</span></label>
        <input type="number" id="thr-temp" min="0" max="200" value="0" step="1">
      </div>
      <div class="form-group">
        <label for="thr-cooldown">Cooldown (s)</label>
        <input type="number" id="thr-cooldown" min="0" value="300" step="30">
      </div>
    </div>
    <div class="form-actions">
      <button id="thr-save-btn" class="primary">💾 Save Thresholds</button>
      <span id="thr-status"></span>
    </div>
  "#,
    );
    El::id("set-apply-btn")
        .closest("section")
        .insert_after(&section);

    El::id("thr-save-btn").on_async("click", |_| async {
        let result = save_thresholds("thr-cpu", "thr-temp", "thr-cooldown", 300).await;
        if report(&El::id("thr-status"), &result, "✓ Saved") {
            log("Alert thresholds updated");
            fetch_thresholds().await;
        }
    });
}

/// Post the alert thresholds read from the three named inputs.
pub async fn save_thresholds(
    cpu: &str,
    temp: &str,
    cooldown: &str,
    default_cooldown: i64,
) -> Result<(), String> {
    let body = obj! {
        "cpu_pct": parse_float(&El::id(cpu).val()).unwrap_or(0.0),
        "temp_c": parse_float(&El::id(temp).val()).unwrap_or(0.0),
        "cooldown_secs": parse_int(&El::id(cooldown).val()).unwrap_or(default_cooldown) as f64,
    };
    api::post_only("/api/alerts/thresholds", body).await
}

/// Load the thresholds into both threshold forms and the summary line.
pub async fn fetch_thresholds() {
    let Ok(d) = api::get("/api/alerts/thresholds").await else {
        return;
    };
    let (cpu, temp) = (d.get("cpu_pct").num_or(0.0), d.get("temp_c").num_or(0.0));
    let cooldown = d.get("cooldown_secs").num_or(300.0);

    for (id, value) in [
        ("thr-cpu", cpu),
        ("thresh-cpu", cpu),
        ("thr-temp", temp),
        ("thresh-temp", temp),
        ("thr-cooldown", cooldown),
        ("thresh-cooldown", cooldown),
    ] {
        El::id(id).set_val(&value.to_string());
    }

    let mut active = Vec::new();
    if cpu > 0.0 {
        active.push(format!("CPU > {cpu}%"));
    }
    if temp > 0.0 {
        active.push(format!("Temp > {temp}°C"));
    }
    let line = El::id("threshold-status-line");
    if active.is_empty() {
        line.set_text("✓ No thresholds configured (all disabled)")
            .color("var(--overlay0)");
    } else {
        line.set_text(&format!(
            "⚠ Active thresholds: {} — cooldown {cooldown}s",
            active.join(", ")
        ))
        .color("var(--yellow)");
    }
}

// ─── Night dim schedule ──────────────────────────────────────────────────────

fn inject_dim() {
    let section = card(
        "",
        r#"
    <h2>Night Dim Schedule</h2>
    <p class="card-hint">Automatically dim the panel during a specified hour range (e.g. 22:00 – 07:00).</p>
    <div class="form-row three-col">
      <div class="form-group">
        <label for="dim-start">Start hour <span class="hint-inline">(0–23)</span></label>
        <input type="number" id="dim-start" min="0" max="23" value="22">
      </div>
      <div class="form-group">
        <label for="dim-end">End hour <span class="hint-inline">(0–23)</span></label>
        <input type="number" id="dim-end" min="0" max="23" value="7">
      </div>
      <div class="form-group">
        <label for="dim-level">Dim level <span class="hint-inline">(0–100%)</span></label>
        <input type="number" id="dim-level" min="0" max="100" value="20">
      </div>
    </div>
    <div class="form-actions">
      <button id="dim-set-btn" class="primary">🌙 Set Schedule</button>
      <button id="dim-clear-btn">✕ Clear</button>
      <span id="dim-status"></span>
    </div>
    <div id="dim-active-info" style="display:none;margin-top:0.5rem;font-size:0.85rem;color:var(--subtext)"></div>
  "#,
    );
    El::id("thr-save-btn")
        .closest("section")
        .insert_after(&section);

    El::id("dim-set-btn").on_async("click", |_| async {
        // Hour 0 is midnight, so only a blank field falls back to the default.
        let hour = |id: &str, default: i64| parse_int(&El::id(id).val()).unwrap_or(default) as f64;
        let body = obj! {
            "start_hour": hour("dim-start", 22),
            "end_hour": hour("dim-end", 7),
            "dim_level": hour("dim-level", 20) / 100.0,
        };
        if report(
            &El::id("dim-status"),
            &api::post_only("/api/dim/schedule", body).await,
            "✓ Schedule set",
        ) {
            log("Dim schedule updated");
            fetch_dim_schedule().await;
        }
    });
    El::id("dim-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("dim-status"),
            &api::delete("/api/dim/schedule").await,
            "✓ Cleared",
        ) {
            log("Dim schedule cleared");
            fetch_dim_schedule().await;
        }
    });
}

pub async fn fetch_dim_schedule() {
    let Ok(d) = api::get("/api/dim/schedule").await else {
        return;
    };
    let schedule = d.get("schedule");
    let info = El::id("dim-active-info");
    if !schedule.truthy() {
        info.hide();
        return;
    }
    let (start, end) = (
        schedule.get("start_hour").show(),
        schedule.get("end_hour").show(),
    );
    let level = (schedule.get("dim_level").num_or(0.0) * 100.0).round();
    El::id("dim-start").set_val(&start);
    El::id("dim-end").set_val(&end);
    El::id("dim-level").set_val(&level.to_string());
    info.show()
        .set_text(&format!("Active: {start}:00 → {end}:00 @ {level}%"));
}

// ─── Custom metrics ──────────────────────────────────────────────────────────

fn inject_metrics() {
    let section = card(
        "",
        r#"
    <h2>Custom Metrics Dashboard</h2>
    <p class="card-hint">Push up to 8 key/value metrics to the panel's Metrics display mode.</p>
    <div class="form-group" style="margin-bottom:0.6rem">
      <label for="met-title">Dashboard title</label>
      <input type="text" id="met-title" placeholder="CI Dashboard" maxlength="28">
    </div>
    <div id="met-rows"></div>
    <div class="form-actions" style="margin-top:0.5rem">
      <button id="met-add-btn">+ Add metric</button>
      <button id="met-push-btn" class="primary">📊 Push to panel</button>
      <button id="met-clear-btn" class="danger-btn">🗑 Clear</button>
      <span id="met-status"></span>
    </div>
  "#,
    );
    El::id("dim-set-btn")
        .closest("section")
        .insert_after(&section);

    El::id("met-add-btn").on("click", |_| add_metric_row("", "", "blue"));
    El::id("met-push-btn").on_async("click", |_| async {
        let metrics: Vec<JsValue> = El::id("met-rows")
            .find_all(".metric-row")
            .iter()
            .filter_map(|row| {
                let (label, value) = (row.find(".met-label").trimmed(), row.find(".met-value").trimmed());
                (!label.is_empty() && !value.is_empty())
                    .then(|| obj! { "label": label, "value": value, "color": row.find(".met-color").val() })
            })
            .collect();
        let count = metrics.len();
        let body = obj! { "title": El::id("met-title").trimmed(), "metrics": arr(metrics), "show": true };
        let result = api::post_only("/api/metrics", body).await;
        if report(&El::id("met-status"), &result, &format!("✓ {count} metric(s) pushed")) {
            log(&format!("Metrics pushed: {count} items"));
        }
    });
    El::id("met-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("met-status"),
            &api::delete("/api/metrics").await,
            "✓ Cleared",
        ) {
            El::id("met-rows").clear();
            log("Metrics cleared");
        }
    });
}

fn add_metric_row(label: &str, value: &str, color: &str) {
    let rows = El::id("met-rows");
    if !rows.ok() || rows.child_count() >= MAX_METRICS {
        return;
    }
    let row = El::build(
        "div",
        "metric-row",
        &format!(
            r#"
    <input type="text"  class="met-label" placeholder="Label" value="{}" maxlength="14" style="flex:1">
    <input type="text"  class="met-value" placeholder="Value" value="{}" maxlength="14" style="flex:1">
    <select class="met-color">{}</select>
    <button class="met-del-btn danger-btn" title="Remove">✕</button>
  "#,
            esc(label),
            esc(value),
            color_options(color),
        ),
    );
    let doomed = row.clone();
    row.find(".met-del-btn")
        .on("click", move |_| doomed.remove());
    rows.append(&row);
}

pub async fn fetch_metrics() {
    let Ok(d) = api::get("/api/metrics").await else {
        return;
    };
    // Leave rows the user is editing alone.
    if El::id("met-rows").child_count() > 0 {
        return;
    }
    El::id("met-title").set_val(&d.get("title").string());
    for m in d.get("metrics").arr() {
        let color = m
            .get("color")
            .str()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "blue".into());
        add_metric_row(&m.get("label").show(), &m.get("value").show(), &color);
    }
}

// ─── HTTP JSON metrics source ────────────────────────────────────────────────

fn inject_metrics_source() {
    let section = card(
        "metrics-source-section",
        r#"<h2>📡 HTTP JSON Metrics Source</h2>
<p class="card-hint">Automatically fetch metrics from any JSON HTTP endpoint and display them on the panel.
 Use dot-notation JSON paths: <code>cpu.usage_pct</code>, <code>data.0.value</code>.</p>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:3"><label>URL</label>
    <input type="url" id="ms-url-input" placeholder="https://myserver/api/stats" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>Interval (s)</label>
    <input type="number" id="ms-interval-input" value="30" min="5" max="3600" style="width:100%"></div>
</div>
<div class="form-group" style="margin:0.4rem 0">
  <label>Dashboard title (optional)</label>
  <input type="text" id="ms-title-input" placeholder="My Server" maxlength="28" style="width:100%">
</div>
<div id="ms-fields-list" style="margin-bottom:0.5rem"></div>
<div class="form-row" style="gap:0.5rem;margin-bottom:0.5rem">
  <button id="ms-add-field-btn" class="btn-secondary">+ Add field</button>
</div>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap">
  <button id="ms-set-btn"   class="btn-primary"    style="flex:2">Set &amp; Start Auto-fetch</button>
  <button id="ms-fetch-btn" class="btn-secondary"  style="flex:1">Fetch Now</button>
  <button id="ms-clear-btn" class="btn-danger"     style="flex:1">Clear</button>
</div>
<div id="ms-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>"#,
    );
    place_after(&El::id("met-push-btn").closest("section"), &section);

    El::id("ms-add-field-btn").on("click", |_| add_source_field("", "", "{}", "blue"));

    El::id("ms-set-btn").on_async("click", |_| async {
        let url = El::id("ms-url-input").trimmed();
        if url.is_empty() {
            return toast("Enter a URL first", "alert", 2000);
        }
        let interval = int_or(&El::id("ms-interval-input").val(), 30);
        let fields: Vec<JsValue> = El::all("#ms-fields-list .form-row")
            .iter()
            .filter_map(|row| {
                let (label, path) = (row.find(".ms-field-label").trimmed(), row.find(".ms-field-path").trimmed());
                let format = non_empty_or(row.find(".ms-field-fmt").trimmed(), "{}");
                (!label.is_empty() && !path.is_empty()).then(|| {
                    obj! { "label": label, "path": path, "format": format, "color": row.find(".ms-field-color").val() }
                })
            })
            .collect();
        if fields.is_empty() {
            return toast("Add at least one field", "alert", 2000);
        }
        let body = obj! {
            "url": url.as_str(),
            "title": El::id("ms-title-input").trimmed(),
            "interval_secs": interval as f64,
            "fields": arr(fields),
        };
        if or_toast(api::post("/api/metrics/source", body).await).is_some() {
            toast(&format!("Metrics source started: {url}"), "ok", 3000);
            El::id("ms-status").set_text(&format!("Auto-fetch active every {interval}s"));
        }
    });

    El::id("ms-fetch-btn").on_async("click", |_| async {
        if let Some(d) = or_toast(api::json("POST", "/api/metrics/source/fetch", None).await) {
            toast(
                &format!("Fetched {} metric(s)", d.get("count").num_or(0.0)),
                "ok",
                2500,
            );
            El::id("ms-status").set_text(&format!("Last fetch: {}", dom::time_str()));
        }
    });

    El::id("ms-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/metrics/source").await).is_some() {
            El::id("ms-status").set_text("Cleared.");
            toast("Metrics source cleared", "ok", 2000);
        }
    });

    dom::spawn(async {
        let Ok(d) = api::get("/api/metrics/source").await else {
            return;
        };
        let source = d.get("source");
        if !source.truthy() {
            return;
        }
        let interval = source
            .get("interval_secs")
            .num()
            .filter(|i| *i > 0.0)
            .unwrap_or(30.0);
        El::id("ms-url-input").set_val(&source.get("url").string());
        El::id("ms-title-input").set_val(&source.get("title").string());
        El::id("ms-interval-input").set_val(&interval.to_string());
        for f in source.get("fields").arr() {
            add_source_field(
                &f.get("label").string(),
                &f.get("path").string(),
                &f.get("format").string(),
                &f.get("color").string(),
            );
        }
        if !source.get("url").string().is_empty() {
            El::id("ms-status").set_text(&format!("Auto-fetch active every {interval}s"));
        }
    });
}

fn add_source_field(label: &str, path: &str, format: &str, color: &str) {
    let list = El::id("ms-fields-list");
    if !list.ok() || list.child_count() >= MAX_METRICS {
        return;
    }
    let format = if format.is_empty() { "{}" } else { format };
    let color = if color.is_empty() { "blue" } else { color };
    let row = El::build(
        "div",
        "form-row",
        &format!(
            r#"<input type="text" class="ms-field-label" placeholder="Label" value="{}" maxlength="14" style="flex:1">
<input type="text" class="ms-field-path"  placeholder="JSON path (e.g. cpu.pct)" value="{}" style="flex:2">
<input type="text" class="ms-field-fmt"   placeholder="{{}}" value="{}" style="flex:1;min-width:50px">
<select class="ms-field-color" style="flex:1">{}</select>
<button class="ms-remove-btn btn-danger" title="Remove">✕</button>"#,
            esc(label),
            esc(path),
            esc(format),
            color_options(color),
        ),
    );
    row.css("gap", "0.4rem").css("margin-bottom", "0.3rem");
    let doomed = row.clone();
    row.find(".ms-remove-btn")
        .on("click", move |_| doomed.remove());
    list.append(&row);
}

// ─── Display profiles ────────────────────────────────────────────────────────

fn inject_profiles() {
    let section = card(
        "",
        r#"
    <h2>Display Profiles</h2>
    <p class="card-hint">Save and restore complete panel configurations (mode, quality, rotation, brightness).</p>
    <div class="form-row two-col">
      <div class="form-group">
        <label for="prof-name">Profile name</label>
        <input type="text" id="prof-name" placeholder="e.g. night-mode" maxlength="40">
      </div>
      <div class="form-group">
        <label for="prof-select">Saved profiles</label>
        <select id="prof-select"><option value="">— select profile —</option></select>
      </div>
    </div>
    <div class="form-actions">
      <button id="prof-save-btn" class="primary">💾 Save Current</button>
      <button id="prof-apply-btn">▶ Apply</button>
      <button id="prof-del-btn" class="danger-btn">🗑 Delete</button>
      <span id="prof-status"></span>
    </div>
  "#,
    );
    El::id("met-push-btn")
        .closest("section")
        .insert_after(&section);

    El::id("prof-save-btn").on_async("click", |_| async {
        let typed = El::id("prof-name").trimmed();
        let name = if typed.is_empty() {
            dom::prompt("Profile name:").unwrap_or_default()
        } else {
            typed
        };
        if name.is_empty() {
            return;
        }
        let result = api::post_only("/api/profiles", obj! { "name": name.as_str() }).await;
        if report(
            &El::id("prof-status"),
            &result,
            &format!("✓ Saved \"{name}\""),
        ) {
            log(&format!("Profile saved: {name}"));
            fetch_profiles().await;
            El::id("prof-select").set_val(&name);
        }
    });

    El::id("prof-apply-btn").on_async("click", |_| async {
        let status_el = El::id("prof-status");
        let name = El::id("prof-select").val();
        if name.is_empty() {
            return status(&status_el, "Select a profile first", false);
        }
        let result = api::post_empty(&format!("/api/profiles/{}/apply", api::enc(&name))).await;
        if report(&status_el, &result, &format!("✓ Applied \"{name}\"")) {
            log(&format!("Profile applied: {name}"));
            core::resync_settings();
        }
    });

    El::id("prof-del-btn").on_async("click", |_| async {
        let name = El::id("prof-select").val();
        if name.is_empty() || !dom::confirm(&format!("Delete profile \"{name}\"?")) {
            return;
        }
        let result = api::delete(&format!("/api/profiles/{}", api::enc(&name))).await;
        if report(
            &El::id("prof-status"),
            &result,
            &format!("✓ Deleted \"{name}\""),
        ) {
            log(&format!("Profile deleted: {name}"));
            fetch_profiles().await;
        }
    });
}

pub async fn fetch_profiles() {
    let Ok(profiles) = api::get("/api/profiles").await else {
        return;
    };
    let select = El::id("prof-select");
    let previous = select.val();
    select.set_html(r#"<option value="">— select profile —</option>"#);
    for p in profiles.arr() {
        let name = p.get("name").string();
        let option = El::new("option");
        option.set_val(&name).set_text(&format!(
            "{name} ({}, q{})",
            p.get("mode").show(),
            p.get("quality").show()
        ));
        select.append(&option);
    }
    if !previous.is_empty() {
        select.set_val(&previous);
    }
}

// ─── Weather ─────────────────────────────────────────────────────────────────

fn inject_weather() {
    let section = card(
        "",
        r#"
    <h2>Weather Dashboard</h2>
    <p class="card-hint">Fetches current conditions from <a href="https://wttr.in" target="_blank" rel="noopener">wttr.in</a> (public API, no key needed) and pushes them to the panel.</p>
    <div class="form-row two-col">
      <div class="form-group">
        <label for="weather-city">City / location</label>
        <input type="text" id="weather-city" placeholder="London, Paris, Tokyo…" value="">
      </div>
      <div class="form-group">
        <label>Auto-refresh</label>
        <select id="weather-refresh-interval">
          <option value="0">Manual only</option>
          <option value="300">Every 5 min</option>
          <option value="600" selected>Every 10 min</option>
          <option value="1800">Every 30 min</option>
          <option value="3600">Every 1 hour</option>
        </select>
      </div>
    </div>
    <div class="form-actions" style="margin-top:0.6rem;flex-wrap:wrap;gap:0.5rem">
      <button id="weather-fetch-btn" class="primary">🌤 Fetch &amp; Push Weather</button>
      <button id="weather-server-auto-btn" title="Enable server-side auto-fetch (persists when browser is closed)">🔄 Server Auto-fetch</button>
      <button id="weather-auto-stop-btn" title="Stop server-side auto-fetch">⏹ Stop Server Auto</button>
      <button id="weather-clear-btn">✕ Clear</button>
      <span id="weather-status"></span>
    </div>
    <div id="weather-display" class="weather-display" style="display:none;margin-top:0.75rem">
      <div class="weather-temp" id="weather-temp">—</div>
      <div class="weather-cond" id="weather-cond">—</div>
      <div class="weather-details" id="weather-details"></div>
    </div>
  "#,
    );
    El::id("prof-save-btn")
        .closest("section")
        .insert_after(&section);

    El::id("weather-fetch-btn").on_async("click", |_| async {
        fetch_and_push_weather(false).await;
        stop_weather_timer();
        let interval = parse_int(&El::id("weather-refresh-interval").val()).unwrap_or(0);
        if interval > 0 {
            let id = dom::set_interval(interval as i32 * 1000, || {
                dom::spawn(fetch_and_push_weather(true));
            });
            WEATHER_TIMER.with(|t| *t.borrow_mut() = Some(id));
            log(&format!("Weather auto-refresh every {interval}s"));
        }
    });

    El::id("weather-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("weather-status"),
            &api::delete("/api/weather").await,
            "✓ Weather cleared",
        ) {
            stop_weather_timer();
            El::id("weather-display").hide();
            log("Weather cleared");
        }
    });

    El::id("weather-server-auto-btn").on_async("click", |_| async {
        let status_el = El::id("weather-status");
        let city = El::id("weather-city").trimmed();
        if city.is_empty() {
            return status(&status_el, "Enter a city first", false);
        }
        // "Manual only" (0) falls back to half an hour; the daemon's minimum is a minute.
        let secs = int_or(&El::id("weather-refresh-interval").val(), 1800).max(60);
        let result = async {
            api::json(
                "PUT",
                "/api/weather/auto",
                Some(obj! { "location": city.as_str(), "interval_secs": secs as f64 }),
            )
            .await?;
            api::post(
                "/api/weather/fetch",
                obj! { "location": city.as_str(), "switch_mode": true },
            )
            .await
        }
        .await;
        if report(
            &status_el,
            &result,
            &format!("✓ Server auto-fetch enabled for {city} every {secs}s"),
        ) {
            core::fetch_status().await;
        }
    });

    El::id("weather-auto-stop-btn").on_async("click", |_| async {
        report(
            &El::id("weather-status"),
            &api::delete("/api/weather/auto").await,
            "✓ Server auto-fetch stopped",
        );
    });
}

fn stop_weather_timer() {
    if let Some(id) = WEATHER_TIMER.with(|t| t.borrow_mut().take()) {
        dom::clear_interval(id);
    }
}

/// Fetch JSON from another origin (wttr.in), bypassing the daemon.
async fn fetch_external_json(url: &str) -> Result<Js, String> {
    let resp = JsFuture::from(dom::win().fetch_with_str(url))
        .await
        .map_err(|e| dom::err_msg(&e))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "no response".to_string())?;
    if !resp.ok() {
        return Err(format!("wttr.in returned {}", resp.status()));
    }
    let promise = resp.json().map_err(|e| dom::err_msg(&e))?;
    JsFuture::from(promise)
        .await
        .map(Js)
        .map_err(|e| dom::err_msg(&e))
}

/// Read current conditions from wttr.in in the browser and push them to the
/// panel. `silent` suppresses status messages for background refreshes.
async fn fetch_and_push_weather(silent: bool) {
    let status_el = El::id("weather-status");
    let city = non_empty_or(El::id("weather-city").trimmed(), "London");
    if !silent {
        status_for(&status_el, "⏳ Fetching weather…", true, 60000);
    }

    let result = async {
        let data =
            fetch_external_json(&format!("https://wttr.in/{}?format=j1", api::enc(&city))).await?;
        let now = data.get("current_condition").at(0);
        if !now.truthy() {
            return Err("No current_condition in response".to_string());
        }
        // wttr.in sends numbers as strings.
        let field = |v: Js| parse_float(&v.show());
        let today = data.get("weather").at(0);
        let temp = field(now.get("temp_C"));
        let condition = now.get("weatherDesc").at(0).get("value").string();
        let humidity = field(now.get("humidity"));
        let wind = field(now.get("windspeedKmph"));
        let wind_dir = now.get("winddir16Point").string();
        let (high, low) = (field(today.get("maxtempC")), field(today.get("mintempC")));

        let body = obj! {
            "city": city.as_str(),
            "temp_c": temp,
            "feels_like_c": field(now.get("FeelsLikeC")),
            "condition": condition.as_str(),
            "humidity_pct": humidity,
            "wind_kmh": wind,
            "wind_dir": wind_dir.as_str(),
            "visibility_km": field(now.get("visibility")),
            "high_c": high,
            "low_c": low,
            "show": true,
        };
        api::post_only("/api/weather", body).await?;

        let whole = |v: Option<f64>| v.map_or("—".to_string(), |v| format!("{v:.0}"));
        let details = [
            format!("💧 {}%", whole(humidity)),
            format!("💨 {} km/h {wind_dir}", whole(wind)),
            format!("▲ {}° ▼ {}°", whole(high), whole(low)),
        ];
        El::id("weather-temp").set_text(&format!("{}°C", whole(temp)));
        El::id("weather-cond").set_text(&condition);
        El::id("weather-details").set_html(
            &details
                .iter()
                .map(|s| format!("<span>{}</span>", esc(s)))
                .collect::<String>(),
        );
        El::id("weather-display").show();
        Ok(format!("Weather: {city} {}°C {condition}", whole(temp)))
    }
    .await;

    if silent {
        return;
    }
    if report(&status_el, &result, &format!("✓ Weather pushed for {city}")) {
        log(&result.unwrap_or_default());
    }
}

pub async fn fetch_cached_weather() {
    let Ok(d) = api::get("/api/weather").await else {
        return;
    };
    if !d.get("city").truthy() && !d.get("temp_c").truthy() {
        return;
    }
    let city = El::id("weather-city");
    if city.val().is_empty() {
        city.set_val(&d.get("city").string());
    }
    El::id("weather-temp").set_text(
        &d.get("temp_c")
            .num()
            .map_or("—".to_string(), |t| format!("{t:.0}°C")),
    );
    El::id("weather-cond").set_text(
        &d.get("condition")
            .str()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "—".into()),
    );
    El::id("weather-display").show();
}

// ─── Notes ───────────────────────────────────────────────────────────────────

fn inject_notes() {
    let section = card(
        "",
        r#"
    <h2>Notes / Bulletin Board</h2>
    <p class="card-hint">Push a list of text items to the panel. Useful for sprint boards, checklists, or reminders.</p>
    <div class="form-group" style="margin-bottom:0.6rem">
      <label for="notes-title-input">Board title</label>
      <input type="text" id="notes-title-input" placeholder="Sprint 42, TODO, Meeting Agenda…" maxlength="28">
    </div>
    <div id="notes-items-wrap"></div>
    <div class="form-actions" style="margin-top:0.5rem">
      <button id="notes-add-btn">+ Add note</button>
      <button id="notes-push-btn" class="primary">📝 Push to panel</button>
      <button id="notes-append-btn">➕ Append</button>
      <button id="notes-clear-btn" class="danger-btn">🗑 Clear</button>
      <span id="notes-status"></span>
    </div>
  "#,
    );
    El::id("weather-fetch-btn")
        .closest("section")
        .insert_after(&section);

    El::id("notes-add-btn").on("click", |_| add_note_row(""));
    for (id, append) in [("notes-push-btn", false), ("notes-append-btn", true)] {
        El::id(id).on_async("click", move |_| async move {
            let items: Vec<String> = El::id("notes-items-wrap")
                .find_all(".note-text")
                .iter()
                .map(El::trimmed)
                .filter(|t| !t.is_empty())
                .collect();
            let count = items.len();
            let body = obj! {
                "title": El::id("notes-title-input").trimmed(),
                "items": arr(items),
                "append": append,
                "show": true,
            };
            let result = api::post_only("/api/notes", body).await;
            if report(
                &El::id("notes-status"),
                &result,
                &format!("✓ {count} note(s) pushed"),
            ) {
                log(&format!(
                    "Notes: {count} item(s){}",
                    if append { " (appended)" } else { "" }
                ));
            }
        });
    }
    El::id("notes-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("notes-status"),
            &api::delete("/api/notes").await,
            "✓ Cleared",
        ) {
            El::id("notes-items-wrap").clear();
            El::id("notes-title-input").set_val("");
            log("Notes cleared");
        }
    });
}

fn add_note_row(text: &str) {
    let wrap = El::id("notes-items-wrap");
    if !wrap.ok() || wrap.child_count() >= MAX_NOTES {
        return;
    }
    let row = El::build(
        "div",
        "notes-item-row",
        &format!(
            r#"
    <input type="text" class="note-text" placeholder="Note text…" value="{}" maxlength="40"
           style="flex:1;padding:0.3rem 0.5rem;font-size:0.85rem">
    <button class="note-del danger-btn" title="Remove">✕</button>
  "#,
            esc(text)
        ),
    );
    row.css_text("display:flex;gap:0.4rem;align-items:center;margin-bottom:0.3rem");
    let doomed = row.clone();
    row.find(".note-del").on("click", move |_| doomed.remove());
    wrap.append(&row);
}

pub async fn fetch_cached_notes() {
    let Ok(d) = api::get("/api/notes").await else {
        return;
    };
    let items = d.get("items").arr();
    if items.is_empty() {
        return;
    }
    let title = El::id("notes-title-input");
    if title.val().is_empty() {
        title.set_val(&d.get("title").string());
    }
    if El::id("notes-items-wrap").child_count() == 0 {
        for item in items {
            add_note_row(&item.show());
        }
    }
}

// ─── QR code ─────────────────────────────────────────────────────────────────

fn inject_qr() {
    let section = card(
        "qr-section",
        r#"<h2>QR Code</h2>
<p class="card-hint">Generate a scannable QR code on the panel — great for sharing URLs, Wi-Fi passwords, or any short text.</p>
<div class="form-row two-col">
  <div class="form-group"><label for="qr-payload">Payload (URL or text)</label>
    <input type="text" id="qr-payload" placeholder="https://example.com" maxlength="300"></div>
  <div class="form-group"><label for="qr-title">Title <span class="hint-inline">(opt.)</span></label>
    <input type="text" id="qr-title" placeholder="QR Code" maxlength="32"></div>
</div>
<div class="form-actions">
  <button id="qr-show-btn" class="primary">🔲 Show QR Code</button>
  <button id="qr-clear-btn">✕ Clear</button>
  <span id="qr-status"></span>
</div>
<div id="qr-preview-wrap" style="display:none;margin-top:0.75rem">
  <div class="sub-label">Last payload</div>
  <div id="qr-payload-display" class="mono" style="font-size:0.82rem;color:var(--teal);word-break:break-all;margin-top:0.3rem"></div>
</div>"#,
    );
    if !El::id("notes-add-btn")
        .closest("section")
        .insert_after(&section)
    {
        El::id("history-list")
            .closest(".card")
            .insert_before(&section);
    }

    El::id("qr-show-btn").on_async("click", |_| async {
        let status_el = El::id("qr-status");
        let payload = El::id("qr-payload").trimmed();
        if payload.is_empty() {
            return status(&status_el, "Enter a payload first", false);
        }
        let body = obj! { "payload": payload.as_str(), "title": El::id("qr-title").trimmed(), "show": true };
        if report(&status_el, &api::post_only("/api/qr", body).await, "✓ QR code on panel") {
            show_qr_payload(&payload);
            log(&format!("QR code: {}", crate::util::prefix(&payload, 60)));
        }
    });

    El::id("qr-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("qr-status"),
            &api::delete("/api/qr").await,
            "✓ Cleared",
        ) {
            El::id("qr-preview-wrap").hide();
            El::id("qr-payload").set_val("");
        }
    });
}

fn show_qr_payload(payload: &str) {
    El::id("qr-preview-wrap").show();
    El::id("qr-payload-display").set_text(payload);
}

pub async fn fetch_qr_state() {
    let Ok(d) = api::get("/api/qr").await else {
        return;
    };
    let Some(payload) = d.get("payload").str().filter(|p| !p.is_empty()) else {
        return;
    };
    let input = El::id("qr-payload");
    if input.val().is_empty() {
        input.set_val(&payload);
    }
    let title = d.get("title").string();
    let title_input = El::id("qr-title");
    if title_input.val().is_empty() && title != "QR Code" {
        title_input.set_val(&title);
    }
    show_qr_payload(&payload);
}

// ─── Webhook ─────────────────────────────────────────────────────────────────

fn inject_webhook() {
    let section = card(
        "wh-section",
        r#"<h2>Webhook</h2>
<p class="card-hint">Test the webhook endpoint or configure a forward URL for outbound delivery.</p>
<div class="form-row two-col">
  <div class="form-group"><label for="wh-status">Status</label>
    <select id="wh-status">
      <option value="success">success</option>
      <option value="failure">failure</option>
      <option value="warning">warning</option>
      <option value="running">running</option>
      <option value="info">info</option>
    </select></div>
  <div class="form-group"><label for="wh-message">Message</label>
    <input type="text" id="wh-message" placeholder="Test webhook message" maxlength="80"></div>
</div>
<div class="form-actions">
  <button id="wh-test-btn" class="primary">⚡ Send Test Webhook</button>
  <span id="wh-test-status"></span>
</div>
<hr style="border-color:var(--surface0);margin:0.75rem 0">
<p class="card-hint">Forward URL — if set, all incoming webhooks are also forwarded here.</p>
<div class="form-row two-col">
  <div class="form-group"><label for="wh-forward-url">Forward URL</label>
    <input type="text" id="wh-forward-url" placeholder="https://hooks.example.com/..." maxlength="200"></div>
</div>
<div class="form-actions">
  <button id="wh-url-save-btn" class="primary">💾 Save URL</button>
  <button id="wh-url-clear-btn" class="danger-btn">✕ Clear</button>
  <span id="wh-url-status"></span>
</div>
<div id="wh-url-display" style="display:none;margin-top:0.4rem;font-size:0.82rem;color:var(--overlay0)">
  Forwarding to: <span id="wh-url-current" class="mono" style="color:var(--teal)"></span>
</div>
<p class="card-hint" style="margin-top:0.75rem">Endpoint: <span class="mono" id="wh-endpoint-url" style="color:var(--blue)"></span></p>"#,
    );
    El::id("history-list")
        .closest(".card")
        .insert_before(&section);
    let origin = dom::win().location().origin().unwrap_or_default();
    El::id("wh-endpoint-url").set_text(&format!("{origin}/api/webhook"));

    El::id("wh-test-btn").on_async("click", |_| async {
        let kind = El::id("wh-status").val();
        let message = non_empty_or(El::id("wh-message").val(), "Test from ttlcd web UI");
        let body = obj! { "status": kind.as_str(), "message": message.as_str() };
        if report(
            &El::id("wh-test-status"),
            &api::post_only("/api/webhook", body).await,
            "✓ Webhook sent",
        ) {
            log(&format!("Webhook test: {kind} — {message}"));
        }
    });

    El::id("wh-url-save-btn").on_async("click", |_| async {
        let status_el = El::id("wh-url-status");
        let url = El::id("wh-forward-url").trimmed();
        if url.is_empty() {
            return status(&status_el, "Enter a URL first", false);
        }
        let result = api::post_only("/api/webhook/config", obj! { "url": url.as_str() }).await;
        if report(&status_el, &result, "✓ URL saved") {
            show_webhook_url(&url);
        }
    });

    El::id("wh-url-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("wh-url-status"),
            &api::delete("/api/webhook/config").await,
            "✓ Cleared",
        ) {
            show_webhook_url("");
            El::id("wh-forward-url").set_val("");
        }
    });
}

fn show_webhook_url(url: &str) {
    El::id("wh-url-current").set_text(url);
    El::id("wh-url-display").display(if url.is_empty() { "none" } else { "" });
}

pub async fn fetch_webhook_config() {
    let Ok(d) = api::get("/api/webhook/config").await else {
        return;
    };
    let Some(url) = d.get("url").str().filter(|u| !u.is_empty()) else {
        return;
    };
    let input = El::id("wh-forward-url");
    if input.val().is_empty() {
        input.set_val(&url);
    }
    show_webhook_url(&url);
}

// ─── Mode schedule ───────────────────────────────────────────────────────────

fn inject_mode_schedule() {
    // The label and status carry a `msched-` prefix: the scheduled-notification
    // card in index.html already owns `sched-label` and `sched-status`.
    let section = card(
        "mode-schedule-section",
        r#"<h2>Mode Schedule</h2>
<p class="card-hint">Automatically switch display modes at specific times of day. The daemon applies the most recently past slot on startup and every 30 seconds.</p>
<div class="form-row three-col">
  <div class="form-group"><label for="sched-hour">Hour <span class="hint-inline">(0–23)</span></label>
    <input type="number" id="sched-hour" min="0" max="23" value="9"></div>
  <div class="form-group"><label for="sched-minute">Minute <span class="hint-inline">(0–59)</span></label>
    <input type="number" id="sched-minute" min="0" max="59" value="0"></div>
  <div class="form-group"><label for="sched-mode">Mode</label>
    <select id="sched-mode"></select></div>
</div>
<div class="form-row two-col">
  <div class="form-group"><label for="msched-label">Label <span class="hint-inline">(opt.)</span></label>
    <input type="text" id="msched-label" placeholder="Work hours" maxlength="40"></div>
</div>
<div class="form-actions">
  <button id="sched-add-btn" class="primary">+ Add Slot</button>
  <button id="sched-clear-btn" class="danger-btn">✕ Clear All</button>
  <span id="msched-status"></span>
</div>
<ul id="sched-list" class="sched-list" style="margin-top:0.75rem;list-style:none;padding:0"></ul>"#,
    );
    if !El::id("dim-start")
        .closest("section")
        .insert_after(&section)
    {
        El::body().append(&section);
    }

    dom::spawn(async {
        let mut modes: Vec<String> = match api::get("/api/modes").await {
            Ok(data) => data
                .arr()
                .iter()
                .map(|m| m.get("name").str().unwrap_or_else(|| m.show()))
                .collect(),
            Err(_) => Vec::new(),
        };
        modes.retain(|m| !m.is_empty());
        if modes.is_empty() {
            modes = FALLBACK_MODES
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
        }
        let select = El::id("sched-mode");
        for mode in modes {
            let option = El::new("option");
            option.set_val(&mode).set_text(&mode_label(&mode));
            select.append(&option);
        }
    });

    El::id("sched-add-btn").on_async("click", |_| async {
        let status_el = El::id("msched-status");
        let hour = parse_int(&El::id("sched-hour").val()).unwrap_or(0);
        let minute = parse_int(&El::id("sched-minute").val()).unwrap_or(0);
        let mode = El::id("sched-mode").val();
        if mode.is_empty() {
            return status(&status_el, "Select a mode", false);
        }
        let body = obj! {
            "hour": hour as f64,
            "minute": minute as f64,
            "mode": mode.as_str(),
            "label": El::id("msched-label").trimmed(),
        };
        if report(
            &status_el,
            &api::post_only("/api/schedule/mode", body).await,
            "✓ Slot added",
        ) {
            log(&format!("Schedule: {hour:02}:{minute:02} → {mode}"));
            fetch_mode_schedule().await;
        }
    });

    El::id("sched-clear-btn").on_async("click", |_| async {
        if report(
            &El::id("msched-status"),
            &api::delete("/api/schedule/mode").await,
            "✓ Cleared",
        ) {
            fetch_mode_schedule().await;
        }
    });

    El::id("sched-list").on_async("click", |e| async move {
        let btn = dom::target(&e).closest("[data-sched-remove]");
        if btn.ok()
            && api::delete(&format!("/api/schedule/mode/{}", btn.data("sched-remove")))
                .await
                .is_ok()
        {
            fetch_mode_schedule().await;
        }
    });
}

pub async fn fetch_mode_schedule() {
    let Ok(entries) = api::get("/api/schedule/mode").await else {
        return;
    };
    let entries = entries.arr();
    let list = El::id("sched-list");
    if entries.is_empty() {
        list.set_html(&placeholder_item("No schedule entries yet."));
        return;
    }
    let rows: String = entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let label = entry.get("label").string();
            let label = if label.is_empty() { String::new() } else { format!(r#"<span class="sched-label">{}</span>"#, esc(&label)) };
            format!(
                r#"<li class="sched-entry"><span class="sched-time mono">{:02}:{:02}</span><span class="sched-mode">{}</span>{label}<button class="sched-remove small-btn" data-sched-remove="{idx}" title="Remove">✕</button></li>"#,
                entry.get("hour").num_or(0.0) as u32,
                entry.get("minute").num_or(0.0) as u32,
                esc(&mode_label(&entry.get("mode").string())),
            )
        })
        .collect();
    list.set_html(&rows);
}

// ─── Pomodoro ────────────────────────────────────────────────────────────────

fn inject_pomodoro() {
    let section = card(
        "pomodoro-section",
        r#"<h2>🍅 Pomodoro Timer</h2>
<p class="card-hint">Focus timer: 25 min work → 5 min break → repeat. After 4 sessions, take a 15 min long break.</p>
<div id="pomo-status-row" style="display:flex;align-items:center;gap:1rem;flex-wrap:wrap;margin-bottom:0.75rem">
  <div id="pomo-phase-badge" class="badge" style="font-size:1rem;padding:0.3rem 0.7rem">—</div>
  <div id="pomo-time" style="font-family:var(--mono-font);font-size:2rem;color:var(--teal);min-width:5rem">--:--</div>
  <div id="pomo-sessions" style="font-size:0.85rem;color:var(--overlay0)"></div>
</div>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap">
  <button id="pomo-start-btn"  class="btn-primary" style="min-width:6rem">▶ Start</button>
  <button id="pomo-stop-btn"   class="btn-secondary" style="min-width:6rem" title="Stop and return to previous mode">⏹ Stop</button>
  <button id="pomo-pause-btn"  class="btn-secondary" style="min-width:6rem">⏸ Pause</button>
  <button id="pomo-skip-btn"   class="btn-secondary" style="min-width:6rem" title="Skip to next phase">⏭ Skip</button>
  <button id="pomo-reset-btn"  class="btn-danger"    style="min-width:6rem" title="Reset all sessions">↺ Reset</button>
</div>
<details style="margin-top:0.75rem">
  <summary style="cursor:pointer;color:var(--subtext0);font-size:0.85rem">Configure durations</summary>
  <div class="form-row two-col" style="margin-top:0.5rem">
    <div class="form-group"><label>Work (min)</label><input type="number" id="pomo-work-mins"  value="25" min="1" max="120" style="width:80px"></div>
    <div class="form-group"><label>Short break (min)</label><input type="number" id="pomo-short-mins" value="5"  min="1" max="60"  style="width:80px"></div>
    <div class="form-group"><label>Long break (min)</label><input type="number"  id="pomo-long-mins"  value="15" min="1" max="120" style="width:80px"></div>
    <div class="form-group"><label>Sessions / set</label><input type="number"    id="pomo-interval"   value="4"  min="1" max="12"  style="width:80px"></div>
  </div>
  <button id="pomo-config-btn" class="btn-secondary" style="margin-top:0.4rem">Apply Config</button>
</details>"#,
    );
    if !El::id("mode-schedule-section").insert_before(&section) {
        El::q("main").append(&section);
    }

    for action in ["start", "stop", "pause", "skip", "reset"] {
        El::id(&format!("pomo-{action}-btn")).on_async("click", move |_| async move {
            if action == "reset" && !dom::confirm("Reset all Pomodoro sessions?") {
                return;
            }
            if or_toast(api::post_empty(&format!("/api/pomodoro/{action}")).await).is_some() {
                fetch_pomodoro().await;
                if action == "stop" {
                    core::fetch_status().await;
                }
            }
        });
    }

    El::id("pomo-config-btn").on_async("click", |_| async {
        let mins = |id: &str, default: i64| int_or(&El::id(id).val(), default) as f64;
        let body = obj! {
            "work_mins": mins("pomo-work-mins", 25),
            "short_break_mins": mins("pomo-short-mins", 5),
            "long_break_mins": mins("pomo-long-mins", 15),
            "long_break_interval": mins("pomo-interval", 4),
        };
        if or_toast(api::post_only("/api/pomodoro/config", body).await).is_some() {
            fetch_pomodoro().await;
        }
    });
}

pub async fn fetch_pomodoro() {
    let Ok(d) = api::get("/api/pomodoro").await else {
        return;
    };
    let phase = d.get("phase").string();
    let (label, color) = match phase.as_str() {
        "work" => ("🍅 Focus", "var(--red)"),
        "short_break" => ("☕ Short Break", "var(--green)"),
        "long_break" => ("🌿 Long Break", "var(--teal)"),
        other => (other, "var(--teal)"),
    };
    El::id("pomo-phase-badge")
        .set_text(label)
        .css("background", color)
        .color("var(--base)");

    let remaining = d.get("remaining_secs").num_or(0.0).max(0.0) as u64;
    El::id("pomo-time")
        .set_text(&format!("{:02}:{:02}", remaining / 60, remaining % 60))
        .color(if d.get("running").truthy() {
            "var(--teal)"
        } else {
            "var(--overlay0)"
        });

    // Bounded like the "Sessions / set" input, so a bad value cannot make
    // the page draw millions of dots.
    let per_set = (d.get("long_break_interval").num_or(0.0) as u32).min(MAX_POMODORO_SET);
    let done = d.get("sessions_in_set").num_or(0.0) as u32;
    let dots: String = (0..per_set)
        .map(|i| {
            let color = if i < done {
                "var(--red)"
            } else if phase == "work" && i == done {
                "var(--overlay0)"
            } else {
                "var(--surface0)"
            };
            format!(r#"<span style="display:inline-block;width:12px;height:12px;border-radius:50%;margin:0 3px;background:{color}"></span>"#)
        })
        .collect();
    El::id("pomo-sessions").set_html(&format!(
        "{dots} &nbsp;{} completed",
        d.get("completed_sessions").num_or(0.0)
    ));

    // Mirror the configuration, except in a field the user is editing.
    let focused = El(dom::doc().active_element());
    for (id, value) in [
        ("pomo-work-mins", d.get("work_secs").num_or(0.0) / 60.0),
        (
            "pomo-short-mins",
            d.get("short_break_secs").num_or(0.0) / 60.0,
        ),
        (
            "pomo-long-mins",
            d.get("long_break_secs").num_or(0.0) / 60.0,
        ),
        ("pomo-interval", per_set as f64),
    ] {
        let input = El::id(id);
        if !input.same(&focused) {
            input.set_val(&value.to_string());
        }
    }
}

// ─── Ticker ──────────────────────────────────────────────────────────────────

fn inject_ticker() {
    let options: String = COLORS
        .iter()
        .map(|c| {
            format!(
                r#"<option value="{c}">{}{}</option>"#,
                c[..1].to_uppercase(),
                &c[1..]
            )
        })
        .collect();
    let section = card(
        "ticker-section",
        &format!(
            r#"<h2>📡 Scrolling Ticker</h2>
<p class="card-hint">Display a scrolling text message on the panel. Useful for long announcements or notifications.</p>
<div class="form-row two-col" style="align-items:flex-end">
  <div class="form-group" style="flex:3"><label>Ticker Text</label>
    <input type="text" id="ticker-text" placeholder="Your scrolling message here…" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>Color</label>
    <select id="ticker-color">{options}</select></div>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">
  <button id="ticker-send-btn" class="btn-primary">Send to Panel</button>
  <button id="ticker-stop-btn" class="btn-secondary">Stop Ticker</button>
</div>"#
        ),
    );
    if !El::id("world-clock-section").insert_before(&section) {
        El::q("main").append(&section);
    }

    El::id("ticker-send-btn").on_async("click", |_| async {
        let text = El::id("ticker-text").trimmed();
        if text.is_empty() {
            return;
        }
        let body = obj! { "text": text.as_str(), "color": El::id("ticker-color").val(), "switch_mode": true };
        if or_toast(api::post_only("/api/ticker", body).await).is_some() {
            core::fetch_status().await;
            let ellipsis = if text.chars().count() > 40 { "…" } else { "" };
            toast(&format!("Ticker set: {}{ellipsis}", crate::util::prefix(&text, 40)), "ok", 2500);
        }
    });

    El::id("ticker-stop-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/ticker").await).is_some() {
            core::fetch_status().await;
        }
    });
}

// ─── World clock ─────────────────────────────────────────────────────────────

fn inject_world_clock() {
    let section = card(
        "world-clock-section",
        r#"<h2>🌍 World Clock</h2>
<p class="card-hint">Show up to 4 cities with configurable UTC offsets. Leave empty for defaults (UTC, New York, London, Tokyo).</p>
<ul id="wc-list" class="sched-list" style="margin-bottom:0.75rem"></ul>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:2"><label>City name</label>
    <input type="text" id="wc-city" placeholder="e.g. Berlin" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>UTC offset (min)</label>
    <input type="number" id="wc-offset" value="0" min="-840" max="840" style="width:100%"></div>
  <button id="wc-add-btn" class="btn-primary">Add City</button>
  <button id="wc-clear-btn" class="btn-danger" title="Reset to defaults">Reset</button>
</div>"#,
    );
    place_after(&El::id("pomodoro-section"), &section);

    El::id("wc-add-btn").on_async("click", |_| async {
        let city = El::id("wc-city").trimmed();
        if city.is_empty() {
            return;
        }
        let offset = parse_int(&El::id("wc-offset").val()).unwrap_or(0);
        let result = async {
            let existing = api::get("/api/world_clock").await?;
            // Keep room for the new city: the panel shows at most four.
            let mut cities: Vec<JsValue> = existing
                .get("cities")
                .arr()
                .into_iter()
                .take(MAX_WORLD_CITIES - 1)
                .map(|c| c.0)
                .collect();
            cities.push(obj! { "city": city.as_str(), "utc_offset_mins": offset as f64 });
            api::post_only("/api/world_clock", arr(cities)).await
        }
        .await;
        if or_toast(result).is_some() {
            El::id("wc-city").set_val("");
            fetch_world_clock().await;
        }
    });

    El::id("wc-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/world_clock").await).is_some() {
            fetch_world_clock().await;
        }
    });

    El::id("wc-list").on_async("click", |e| async move {
        let btn = dom::target(&e).closest("[data-wc-remove]");
        let Some(remove) = parse_int(&btn.data("wc-remove")) else {
            return;
        };
        let result = async {
            let existing = api::get("/api/world_clock").await?;
            let cities: Vec<JsValue> = existing
                .get("cities")
                .arr()
                .into_iter()
                .enumerate()
                .filter(|(i, _)| *i as i64 != remove)
                .map(|(_, c)| c.0)
                .collect();
            api::post_only("/api/world_clock", arr(cities)).await
        }
        .await;
        if or_toast(result).is_some() {
            fetch_world_clock().await;
        }
    });
}

pub async fn fetch_world_clock() {
    let Ok(d) = api::get("/api/world_clock").await else {
        return;
    };
    let cities = d.get("cities").arr();
    let list = El::id("wc-list");
    if cities.is_empty() {
        list.set_html(&placeholder_item(
            "Using defaults: UTC, New York, London, Tokyo",
        ));
        return;
    }
    let rows: String = cities
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            format!(
                r#"<li class="sched-entry"><span class="sched-mode" style="color:var(--mauve)">{}</span><span class="sched-time">{}</span><button class="sched-remove" data-wc-remove="{idx}" title="Remove">✕</button></li>"#,
                esc(&c.get("city").string()),
                utc_offset_label(c.get("utc_offset_mins").num_or(0.0) as i64),
            )
        })
        .collect();
    list.set_html(&rows);
}
