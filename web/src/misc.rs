//! Everything else: keyboard shortcuts, swipe gestures, the toolbar buttons,
//! the settings-backup card, and the cards written in `index.html` (Docker,
//! sessions, ntfy, API key, thresholds, screensaver, night mode, pet cat).

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::TouchEvent;

use crate::api;
use crate::core::{self, log, log_err, report_text, status, toast};
use crate::dom::{self, card, El};
use crate::js::Js;
use crate::obj;
use crate::util::{esc, non_empty_or, parse_int, plural};
use crate::{modes, sections};

pub fn init() {
    init_keyboard();
    init_shortcuts_help();
    init_swipe();
    init_toolbar();
    init_fullscreen();
    init_settings_backup();
    init_docker();
    init_sessions();
    init_ntfy();
    init_auth();
    init_threshold_form();
    init_screensaver();
    init_night_mode();
    init_pet();
    init_lights();
}

// ─── Keyboard shortcuts ──────────────────────────────────────────────────────

/// Shift+key → mode. The help overlay is generated from this table.
const MODE_KEYS: [(&str, &str); 20] = [
    ("A", "analog_clock"),
    ("B", "cpu_core_bars"),
    ("C", "net_connections"),
    ("D", "dashboard"),
    ("E", "game_of_life"),
    ("F", "cpu_freq"),
    ("G", "ping_graph"),
    ("H", "http_status"),
    ("I", "hostname_ip"),
    ("K", "docker_status"),
    ("L", "log_tail"),
    ("N", "net_speed_graph"),
    ("O", "os_info"),
    ("R", "rss_feed"),
    ("T", "temp_graph"),
    ("U", "uptime_fancy"),
    ("V", "service_status"),
    ("W", "logged_in_users"),
    ("X", "led_clock"),
    ("Z", "status_overview"),
];

/// (keys, what they do) for the shortcuts that are not plain mode switches.
const OTHER_KEYS: [(&str, &str); 13] = [
    ("/", "Focus mode search"),
    ("Ctrl+K", "Open command palette"),
    ("?", "Toggle this overlay"),
    (
        "Esc",
        "Clear search / close overlay / return to previous mode",
    ),
    ("1 – 9", "Switch to the first nine modes in the grid"),
    ("→ or N", "Next mode"),
    ("← or P", "Previous mode"),
    ("F", "Freeze / unfreeze display"),
    ("Space", "Start / stop stopwatch"),
    ("L", "Stopwatch lap"),
    ("W", "Fetch and push weather"),
    ("Shift+P", "Start / pause Pomodoro"),
    ("Shift+S", "Skip Pomodoro phase"),
];

async fn step_mode(direction: &str) {
    if api::post_empty(&format!("/api/mode/{direction}"))
        .await
        .is_ok()
    {
        core::fetch_status().await;
    }
}

async fn random_mode() {
    let status_el = El::id("quick-status");
    match api::json("POST", "/api/mode/random", None).await {
        Ok(d) => {
            let mode = d.get("mode").string();
            log(&format!("Random mode → {mode}"));
            core::set_active_mode(&mode);
            status(&status_el, &format!("🎲 Switched to: {mode}"), true);
            core::fetch_status().await;
        }
        Err(e) => status(&status_el, &format!("Random mode error: {e}"), false),
    }
}

async fn toggle_pomodoro() {
    let Ok(d) = api::get("/api/pomodoro").await else {
        return;
    };
    let action = if d.get("running").truthy() {
        "pause"
    } else {
        "start"
    };
    if api::post_empty(&format!("/api/pomodoro/{action}"))
        .await
        .is_ok()
    {
        sections::fetch_pomodoro().await;
    }
}

fn init_keyboard() {
    dom::doc_on("keydown", |e| {
        if dom::typing() || dom::flag(&e, "metaKey") || dom::flag(&e, "ctrlKey") {
            return;
        }
        let key = dom::key(&e);
        if let Some(&(_, mode)) = MODE_KEYS.iter().find(|(k, _)| *k == key) {
            return dom::spawn(core::go_to_mode(mode));
        }
        match key.as_str() {
            " " => {
                e.prevent_default();
                El::id("sw-start-btn").click();
            }
            "l" => El::id("sw-lap-btn").click(),
            "f" => El::id("freeze-btn").click(),
            "w" => El::id("weather-fetch-btn").click(),
            "P" => dom::spawn(toggle_pomodoro()),
            "S" => dom::spawn(async {
                if api::post_empty("/api/pomodoro/skip").await.is_ok() {
                    sections::fetch_pomodoro().await;
                    toast("Pomodoro phase skipped", "ok", 2000);
                }
            }),
            "Escape" => {
                let btn = El::id("return-btn");
                if !btn.is_hidden() {
                    btn.click();
                }
            }
            "ArrowRight" | "n" => {
                e.prevent_default();
                dom::spawn(step_mode("next"));
            }
            "ArrowLeft" | "p" => {
                e.prevent_default();
                dom::spawn(step_mode("prev"));
            }
            digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
                let idx = digit.parse::<usize>().unwrap_or(1) - 1;
                if let Some(btn) = El::all("#mode-grid .mode-btn[data-mode]").get(idx) {
                    let mode = btn.data("mode");
                    modes::remember(&mode);
                    dom::spawn(async move { core::switch_mode(&mode).await });
                }
            }
            _ => {}
        }
    });
}

fn init_shortcuts_help() {
    let row = |keys: &str, action: &str| {
        format!(
            "<tr><td><kbd>{}</kbd></td><td>{}</td></tr>",
            esc(keys),
            esc(action)
        )
    };
    let rows: String = OTHER_KEYS
        .iter()
        .map(|(keys, action)| row(keys, action))
        .chain(
            MODE_KEYS
                .iter()
                .map(|(key, mode)| row(&format!("Shift+{key}"), &crate::util::mode_label(mode))),
        )
        .collect();
    let overlay = El::build(
        "div",
        "",
        &format!(
            r#"
      <div class="shortcuts-card">
        <div class="shortcuts-header">
          <h2 style="margin:0">⌨ Keyboard Shortcuts</h2>
          <button id="shortcuts-close-btn" class="icon-btn" title="Close">✕</button>
        </div>
        <div style="overflow-y:auto;max-height:70vh">
        <table class="shortcuts-table">
          <thead><tr><th>Key</th><th>Action</th></tr></thead>
          <tbody>{rows}</tbody>
        </table>
        </div>
      </div>
    "#
        ),
    );
    overlay.set_id("shortcuts-overlay");
    El::body().append(&overlay);

    let backdrop = overlay.clone();
    overlay.on("click", move |e| {
        if dom::target(&e).same(&backdrop) {
            backdrop.class("visible", false);
        }
    });
    El::id("shortcuts-close-btn").on("click", |_| {
        El::id("shortcuts-overlay").class("visible", false);
    });

    dom::doc_on("keydown", |e| {
        if dom::typing() {
            return;
        }
        let overlay = El::id("shortcuts-overlay");
        let visible = overlay.has_class("visible");
        match dom::key(&e).as_str() {
            "?" => {
                e.prevent_default();
                overlay.class("visible", !visible);
            }
            "Escape" if visible => {
                overlay.class("visible", false);
            }
            _ => {}
        }
    });
}

// ─── Swipe gestures ──────────────────────────────────────────────────────────

/// Minimum horizontal travel, in pixels, for a swipe.
const SWIPE_MIN_PX: f64 = 60.0;
/// A swipe must be at least this many times wider than it is tall.
const SWIPE_RATIO: f64 = 2.0;

thread_local! {
    static TOUCH_START: RefCell<(f64, f64)> = const { RefCell::new((0.0, 0.0)) };
}

fn touch_point(e: &web_sys::Event) -> Option<(f64, f64)> {
    let touch = e.dyn_ref::<TouchEvent>()?.changed_touches().get(0)?;
    Some((touch.client_x() as f64, touch.client_y() as f64))
}

fn init_swipe() {
    dom::doc_on("touchstart", |e| {
        if let Some(point) = touch_point(&e) {
            TOUCH_START.with(|t| *t.borrow_mut() = point);
        }
    });
    dom::doc_on("touchend", |e| {
        let Some((x, y)) = touch_point(&e) else {
            return;
        };
        let (start_x, start_y) = TOUCH_START.with(|t| *t.borrow());
        let (dx, dy) = (x - start_x, y - start_y);
        if dx.abs() < SWIPE_MIN_PX || dx.abs() < dy.abs() * SWIPE_RATIO {
            return;
        }
        // Swipes that start on a control belong to that control.
        if dom::target(&e)
            .closest("input,textarea,select,button,.mode-btn,a")
            .ok()
        {
            return;
        }
        if dx > 0.0 {
            dom::spawn(step_mode("prev"));
            toast("◀ Previous mode", "info", 1800);
        } else {
            dom::spawn(step_mode("next"));
            toast("Next mode ▶", "info", 1800);
        }
    });
}

// ─── Toolbar buttons ─────────────────────────────────────────────────────────

/// Disable a button and show `busy` while `work` runs, then restore `idle`.
async fn while_busy<T>(
    btn: &El,
    busy: &str,
    idle: &str,
    work: impl std::future::Future<Output = T>,
) -> T {
    btn.set_disabled(true).set_text(busy);
    let out = work.await;
    btn.set_disabled(false).set_text(idle);
    out
}

fn init_toolbar() {
    El::id("web-qr-btn").on_async("click", |_| async {
        let location = dom::win().location();
        let url = format!("http://{}", location.host().unwrap_or_default());
        let body = obj! { "payload": url.as_str(), "title": "ttlcd Web UI", "show": true };
        match api::post_only("/api/qr", body).await {
            Ok(()) => {
                log(&format!("QR code shown: {url}"));
                status(&El::id("quick-status"), "✓ Web UI QR shown", true);
            }
            Err(e) => {
                log_err(&format!("QR code failed: {e}"));
                status(&El::id("quick-status"), &format!("QR error: {e}"), false);
            }
        }
    });

    El::id("reset-panel-btn").on_async("click", |_| async {
        if !dom::confirm("Reset panel to clock mode and stop rotation / screensaver?") {
            return;
        }
        match api::json("POST", "/api/display/reset", None).await {
            Ok(d) => {
                log(&format!(
                    "Panel reset: {}",
                    d.get("message").str().unwrap_or_else(|| "ok".into())
                ));
                status(&El::id("quick-status"), "✓ Panel reset to clock", true);
                core::fetch_status().await;
            }
            Err(e) => {
                log_err(&format!("Reset failed: {e}"));
                status(&El::id("quick-status"), &format!("Reset error: {e}"), false);
            }
        }
    });

    El::id("random-mode-btn").on_async("click", |_| random_mode());

    El::id("reconnect-usb-btn").on_async("click", |_| async {
        let btn = El::id("reconnect-usb-btn");
        let result = while_busy(
            &btn,
            "⏳ Reconnecting…",
            "🔌 Reconnect",
            api::json("POST", "/api/panel/reconnect", None),
        )
        .await;
        match result {
            Ok(d) => {
                log(&format!(
                    "USB reconnect: {}",
                    d.get("message").str().unwrap_or_else(|| "ok".into())
                ));
                status(
                    &El::id("quick-status"),
                    "✓ Reconnect triggered — check status dot",
                    true,
                );
                // Reconnecting takes a few seconds, so check the status again later.
                for delay in [3000, 6000] {
                    dom::set_timeout(delay, || dom::spawn(core::fetch_status()));
                }
            }
            Err(e) => {
                log_err(&format!("Reconnect failed: {e}"));
                status(
                    &El::id("quick-status"),
                    &format!("Reconnect error: {e}"),
                    false,
                );
            }
        }
    });

    El::id("demo-start-btn").on_async("click", |_| async {
        let btn = El::id("demo-start-btn");
        let result = while_busy(
            &btn,
            "⏳ Starting…",
            "🎬 Demo",
            api::post("/api/demo", obj! { "interval": 8 }),
        )
        .await;
        match result {
            Ok(d) => {
                let message = d
                    .get("message")
                    .str()
                    .unwrap_or_else(|| "Demo started".into());
                log(&format!("Demo started: {message}"));
                status(&El::id("quick-status"), &format!("🎬 {message}"), true);
                core::fetch_status().await;
            }
            Err(e) => {
                log_err(&format!("Demo start failed: {e}"));
                status(&El::id("quick-status"), &format!("Demo error: {e}"), false);
            }
        }
    });

    El::id("demo-stop-btn").on_async("click", |_| async {
        let btn = El::id("demo-stop-btn");
        match while_busy(
            &btn,
            "⏳ Stopping…",
            "⏹ Stop Demo",
            api::delete("/api/demo"),
        )
        .await
        {
            Ok(()) => {
                log("Demo stopped");
                status(&El::id("quick-status"), "⏹ Demo stopped", true);
                core::fetch_status().await;
            }
            Err(e) => {
                log_err(&format!("Demo stop failed: {e}"));
                status(
                    &El::id("quick-status"),
                    &format!("Demo stop error: {e}"),
                    false,
                );
            }
        }
    });

    El::id("benchmark-btn").on_async("click", |_| async {
        let (btn, status_el) = (El::id("benchmark-btn"), El::id("quick-status"));
        status(&status_el, "⏱ Running benchmark (may take ~10s)…", true);
        match while_busy(&btn, "⏳ Running…", "⏱ Bench", api::get("/api/benchmark")).await {
            Ok(d) => {
                let slowest: Vec<String> = d
                    .get("modes")
                    .arr()
                    .iter()
                    .take(3)
                    .map(|m| format!("{}:{}ms", m.get("name").show(), m.get("ms").show()))
                    .collect();
                let (count, avg) = (d.get("count").show(), d.get("avg_ms").show());
                let message = format!(
                    "✓ {count} modes, avg {avg}ms, total {}ms | Slowest: {}",
                    d.get("total_ms").show(),
                    slowest.join(", ")
                );
                log(&format!("Benchmark: {message}"));
                status(&status_el, &message, true);
                toast(
                    &format!("⏱ Benchmark: avg {avg}ms/frame over {count} modes"),
                    "ok",
                    6000,
                );
            }
            Err(e) => {
                log_err(&format!("Benchmark error: {e}"));
                status(&status_el, &format!("Benchmark failed: {e}"), false);
            }
        }
    });
}

// ─── Fullscreen preview ──────────────────────────────────────────────────────

thread_local! {
    static FULLSCREEN_TIMER: RefCell<Option<i32>> = const { RefCell::new(None) };
}

fn hide_fullscreen() {
    El::id("fullscreen-overlay").hide();
    if let Some(id) = FULLSCREEN_TIMER.with(|t| t.borrow_mut().take()) {
        dom::clear_interval(id);
    }
}

fn init_fullscreen() {
    let overlay = El::id("fullscreen-overlay");
    if !overlay.ok() {
        return;
    }
    El::id("fullscreen-preview-btn").on("click", |_| {
        hide_fullscreen();
        El::id("fullscreen-overlay").display("flex");
        El::id("fullscreen-img").set_attr("src", &El::id("panel-preview-img").attr("src"));
        let id = dom::set_interval(1000, || {
            El::id("fullscreen-img").set_attr("src", &format!("/api/preview?_={}", dom::now_ms()));
        });
        FULLSCREEN_TIMER.with(|t| *t.borrow_mut() = Some(id));
    });
    El::id("fullscreen-close").on("click", |e| {
        e.stop_propagation();
        hide_fullscreen();
    });
    overlay.on("click", |_| hide_fullscreen());
    dom::doc_on("keydown", |e| {
        if dom::key(&e) == "Escape" {
            hide_fullscreen();
        }
    });
}

// ─── Settings backup ─────────────────────────────────────────────────────────

fn init_settings_backup() {
    let section = card(
        "",
        r#"
    <h2>Settings Backup</h2>
    <p class="card-hint">Export or import all daemon settings: presets, profiles, thresholds, dim schedule, notes, and metrics.</p>
    <div class="form-actions">
      <a id="settings-export-btn" href="/api/settings/export" download="ttlcd-settings.json"
         class="btn-link" style="text-decoration:none;display:inline-block">⬇ Export All Settings</a>
      <label class="btn-link" style="cursor:pointer;display:inline-block">
        ⬆ Import Settings
        <input type="file" class="settings-import-input" accept=".json" hidden>
      </label>
      <span id="settings-io-status"></span>
    </div>
  "#,
    );
    El::q(".log-card").insert_before(&section);

    // index.html has its own import input next to the panel settings.
    for input in El::all("#settings-import-input, .settings-import-input") {
        let source = input.clone();
        input.on_async("change", move |_| {
            let input = source.clone();
            async move {
                let Some(file) = input.first_file() else {
                    return;
                };
                let result = import_settings(&file).await;
                input.set_val("");
                match result {
                    Ok(d) => {
                        let applied: Vec<String> =
                            d.get("applied").arr().iter().map(Js::show).collect();
                        let message = format!("✓ Imported: {}", applied.join(", "));
                        status(&El::id("settings-io-status"), &message, true);
                        status(&El::id("set-status"), &message, true);
                        log(&format!(
                            "Settings imported from {}: {} section(s)",
                            file.name(),
                            d.get("count").show()
                        ));
                        core::resync_settings();
                        crate::refresh_settings().await;
                    }
                    Err(e) => {
                        status(&El::id("settings-io-status"), &format!("Error: {e}"), false);
                        status(&El::id("set-status"), &format!("Import failed: {e}"), false);
                        log_err(&format!("Settings import error: {e}"));
                    }
                }
            }
        });
    }
}

async fn import_settings(file: &web_sys::File) -> Result<Js, String> {
    let text = JsFuture::from(file.text())
        .await
        .map_err(|e| dom::err_msg(&e))?;
    let settings = Js::parse(&text.as_string().unwrap_or_default())?;
    api::post("/api/settings/import", settings.0).await
}

// ─── Docker and sessions ─────────────────────────────────────────────────────

fn error_row(columns: u32, error: &str) -> String {
    format!(
        r#"<tr><td colspan="{columns}" style="color:var(--red)">Error: {}</td></tr>"#,
        esc(error)
    )
}

async fn fetch_docker() {
    let tbody = El::id("docker-tbody");
    let data = match api::get("/api/docker").await {
        Ok(d) => d,
        Err(e) => {
            tbody.set_html(&error_row(3, &e));
            return;
        }
    };
    let containers = data.get("containers").arr();
    if containers.is_empty() {
        El::id("docker-status-line").set_text("Docker not available or no containers");
        tbody.set_html(r#"<tr><td colspan="3" style="opacity:0.5">No containers found</td></tr>"#);
        return;
    }
    El::id("docker-status-line").set_text(&format!(
        "{}/{} running",
        data.get("running").show(),
        data.get("total").show()
    ));
    let rows: String = containers
        .iter()
        .map(|c| {
            let running = c.get("running").truthy();
            let name = c.get("name").string();
            // "registry/org/app:tag" → "app"
            let image = c.get("image").string();
            let image = image.rsplit('/').next().unwrap_or_default().split(':').next().unwrap_or_default();
            let state = c.get("status").str().filter(|s| !s.is_empty()).unwrap_or_else(|| "unknown".into());
            format!(
                r#"<tr><td>{} <strong>{}</strong></td><td style="opacity:0.75">{}</td><td style="color:{}">{}</td></tr>"#,
                if running { "🟢" } else { "🔴" },
                esc(name.trim_start_matches('/')),
                esc(image),
                if running { "var(--green)" } else { "var(--red)" },
                esc(&state),
            )
        })
        .collect();
    tbody.set_html(&rows);
}

fn init_docker() {
    El::id("docker-refresh-btn").on_async("click", |_| fetch_docker());
    dom::poll(60000, fetch_docker);
}

async fn fetch_sessions() {
    let tbody = El::id("sessions-tbody");
    let data = match api::get("/api/sessions").await {
        Ok(d) => d,
        Err(e) => {
            tbody.set_html(&error_row(5, &e));
            return;
        }
    };
    let sessions = data.get("sessions").arr();
    if sessions.is_empty() {
        El::id("sessions-status-line").set_text("No active sessions");
        tbody.set_html(r#"<tr><td colspan="5" style="opacity:0.5">No users logged in</td></tr>"#);
        return;
    }
    El::id("sessions-status-line").set_text(&format!(
        "{} session{}",
        sessions.len(),
        plural(sessions.len())
    ));
    let rows: String = sessions
        .iter()
        .map(|s| {
            // `who` reports an active terminal as an idle time of ".".
            let idle = s.get("idle").string();
            let (idle, color) = if idle == "." { ("active".to_string(), "var(--green)") } else { (idle, "var(--yellow)") };
            let from = s.get("from").str().filter(|f| !f.is_empty()).unwrap_or_else(|| "local".into());
            format!(
                r#"<tr><td><strong>{}</strong></td><td style="font-family:monospace">{}</td><td>{}</td><td style="color:{color}">{}</td><td style="opacity:0.75">{}</td></tr>"#,
                esc(&s.get("username").string()),
                esc(&s.get("tty").string()),
                esc(&s.get("login_at").string()),
                esc(&idle),
                esc(&from),
            )
        })
        .collect();
    tbody.set_html(&rows);
}

fn init_sessions() {
    El::id("sessions-refresh-btn").on_async("click", |_| fetch_sessions());
    dom::poll(30000, fetch_sessions);
}

// ─── ntfy and API key ────────────────────────────────────────────────────────

/// The `message` field of a response, or `fallback`.
fn message_or(result: &Result<Js, String>, fallback: &str) -> String {
    result
        .as_ref()
        .ok()
        .and_then(|r| r.get("message").str())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

async fn fetch_ntfy_status() {
    let Ok(data) = api::get("/api/ntfy/config").await else {
        return;
    };
    let line = El::id("ntfy-status-line");
    if data.get("configured").truthy() {
        let url = data.get("url").string();
        line.set_html(&format!(
            "🔔 <strong>Active</strong> — sending to <code>{}</code>",
            esc(&url)
        ))
        .color("var(--green)");
        let input = El::id("ntfy-url-input");
        if input.val().is_empty() {
            input.set_val(&url);
        }
    } else {
        line.set_text("🔕 Not configured — no push notifications will be sent.")
            .color("var(--overlay0)");
    }
}

fn init_ntfy() {
    El::id("ntfy-set-btn").on_async("click", |_| async {
        let status_el = El::id("ntfy-action-status");
        let url = El::id("ntfy-url-input").trimmed();
        if url.is_empty() {
            status_el.set_text("Enter a URL first");
            return;
        }
        let result = api::post("/api/ntfy/config", obj! { "url": url }).await;
        if report_text(&status_el, &result, &message_or(&result, "Saved")) {
            fetch_ntfy_status().await;
        }
    });
    El::id("ntfy-test-btn").on_async("click", |_| async {
        let result = api::json("POST", "/api/ntfy/test", None).await;
        report_text(
            &El::id("ntfy-action-status"),
            &result,
            &message_or(&result, "Test sent"),
        );
    });
    El::id("ntfy-clear-btn").on_async("click", |_| async {
        if report_text(
            &El::id("ntfy-action-status"),
            &api::delete("/api/ntfy/config").await,
            "Cleared",
        ) {
            El::id("ntfy-url-input").set_val("");
            fetch_ntfy_status().await;
        }
    });
    dom::spawn(fetch_ntfy_status());
}

async fn fetch_auth_status() {
    let Ok(data) = api::get("/api/auth/status").await else {
        return;
    };
    let line = El::id("auth-status-line");
    if data.get("auth_required").truthy() {
        line.set_html("🔒 <strong>Authentication is active</strong> — Bearer token required for all API calls.")
            .color("var(--green)");
    } else {
        line.set_html("🔓 Authentication is <em>disabled</em> — API is open.")
            .color("var(--yellow)");
    }
}

fn init_auth() {
    El::id("auth-set-btn").on_async("click", |_| async {
        let (status_el, input) = (El::id("auth-action-status"), El::id("auth-key-input"));
        let key = input.trimmed();
        if key.is_empty() {
            status_el.set_text("Enter a key first");
            return;
        }
        let result = api::post("/api/auth/key", obj! { "key": key }).await;
        if report_text(&status_el, &result, &message_or(&result, "Key set")) {
            input.set_val("");
            fetch_auth_status().await;
        }
    });
    El::id("auth-clear-btn").on_async("click", |_| async {
        let result = api::delete("/api/auth/key").await;
        if report_text(
            &El::id("auth-action-status"),
            &result,
            "Key cleared — auth disabled",
        ) {
            fetch_auth_status().await;
        }
    });
    dom::spawn(fetch_auth_status());
}

// ─── Threshold form in index.html ────────────────────────────────────────────

fn init_threshold_form() {
    El::id("thresh-save-btn").on_async("click", |_| async {
        let result =
            sections::save_thresholds("thresh-cpu", "thresh-temp", "thresh-cooldown", 120).await;
        if report_text(&El::id("thresh-action-status"), &result, "Thresholds saved") {
            sections::fetch_thresholds().await;
        }
    });
}

// ─── Screensaver ─────────────────────────────────────────────────────────────

async fn fetch_screensaver() {
    let Ok(data) = api::get("/api/screensaver").await else {
        return;
    };
    let line = El::id("screensaver-status-line");
    let mode = data.get("mode").string();
    let timeout = data.get("timeout_secs").num_or(0.0);
    if !data.get("enabled").truthy() {
        line.set_html("⚪ Screensaver is <em>disabled</em>.")
            .color("var(--overlay0)");
    } else if data.get("active").truthy() {
        line.set_html(&format!(
            "💤 <strong>Screensaver active</strong> — showing <em>{}</em>. Any API call wakes the display.",
            esc(&mode)
        ))
        .color("var(--blue)");
    } else {
        let remaining = (timeout - data.get("seconds_since_activity").num_or(0.0)).max(0.0);
        line.set_html(&format!(
            "✅ Screensaver enabled — activates after <strong>{timeout}s</strong> of inactivity (mode: <em>{}</em>, {remaining}s remaining).",
            esc(&mode)
        ))
        .color("var(--green)");
    }
    // Leave a field the user is editing alone.
    let focused = El(dom::doc().active_element());
    for (id, value) in [
        ("ss-timeout-input", timeout.to_string()),
        ("ss-mode-select", mode),
    ] {
        let field = El::id(id);
        if !field.same(&focused) {
            field.set_val(&value);
        }
    }
}

fn init_screensaver() {
    El::id("ss-save-btn").on_async("click", |_| async {
        let timeout = parse_int(&El::id("ss-timeout-input").val())
            .unwrap_or(0)
            .max(0);
        let mode = non_empty_or(El::id("ss-mode-select").val(), "matrix_rain");
        let result = api::post_only(
            "/api/screensaver",
            obj! { "timeout_secs": timeout as f64, "mode": mode },
        )
        .await;
        let saved = if timeout > 0 {
            format!("Saved — activates after {timeout}s idle")
        } else {
            "Disabled".to_string()
        };
        if report_text(&El::id("ss-action-status"), &result, &saved) {
            fetch_screensaver().await;
        }
    });
    El::id("ss-wake-btn").on_async("click", |_| async {
        let result = api::json("POST", "/api/screensaver/wake", None).await;
        let was_active = result.as_ref().is_ok_and(|r| r.get("was_active").truthy());
        let message = if was_active {
            "Woke from screensaver"
        } else {
            "Not in screensaver"
        };
        if report_text(&El::id("ss-action-status"), &result, message) {
            fetch_screensaver().await;
        }
    });
    El::id("ss-disable-btn").on_async("click", |_| async {
        if report_text(
            &El::id("ss-action-status"),
            &api::delete("/api/screensaver").await,
            "Screensaver disabled",
        ) {
            fetch_screensaver().await;
        }
    });
    dom::poll(15000, fetch_screensaver);
}

// ─── Night mode ──────────────────────────────────────────────────────────────

async fn load_night_mode() {
    let Ok(d) = api::get("/api/night").await else {
        return;
    };
    let (enabled, mode) = (d.get("enabled").truthy(), d.get("mode").string());
    let (start, end) = (d.get("start_hour").show(), d.get("end_hour").show());
    El::id("night-enabled-cb").set("checked", enabled);
    El::id("night-start-hour").set_val(&start);
    El::id("night-end-hour").set_val(&end);
    El::id("night-mode-select").set_val(&mode);

    let line = El::id("night-mode-status-line");
    if enabled && d.get("currently_active").truthy() {
        line.set_text(&format!("🌙 Night mode active — showing \"{mode}\""))
            .color("var(--blue)");
    } else if enabled {
        line.set_text(&format!("Night mode enabled ({start}:00–{end}:00)"))
            .color("var(--subtext0)");
    } else {
        line.set_text("Night mode disabled")
            .color("var(--overlay0)");
    }
}

fn init_night_mode() {
    let section = El::id("night-mode-section");
    if !section.ok() {
        return;
    }
    El::id("night-save-btn").on_async("click", |_| async {
        let (btn, status_el) = (El::id("night-save-btn"), El::id("night-status"));
        // Hour 0 is midnight, so only a blank field falls back to the default.
        let hour = |id: &str, default: i64| parse_int(&El::id(id).val()).unwrap_or(default) as f64;
        let mode = non_empty_or(El::id("night-mode-select").val(), "minimal_clock");
        let body = obj! {
            "enabled": El::id("night-enabled-cb").checked(),
            "start_hour": hour("night-start-hour", 22),
            "end_hour": hour("night-end-hour", 7),
            "mode": mode,
        };
        btn.set_disabled(true);
        match api::post_only("/api/night", body).await {
            Ok(()) => {
                status_el.set_text("✓ Saved").color("var(--green)");
                load_night_mode().await;
            }
            Err(e) => {
                status_el
                    .set_text(&format!("Error: {e}"))
                    .color("var(--red)");
            }
        }
        btn.set_disabled(false);
        dom::set_timeout(3000, move || {
            status_el.set_text("");
        });
    });

    // The card is a <details>; reload whenever it is opened.
    let details = section.clone();
    section.on("toggle", move |_| {
        if details.get("open").is_truthy() {
            dom::spawn(load_night_mode());
        }
    });
    dom::spawn(load_night_mode());
}

// ─── Pet cat ─────────────────────────────────────────────────────────────────

fn render_pet(data: &Js) {
    let environment = data.get("environment").string();
    El::id("pet-coat-select").set_val(&data.get("coat").string());
    El::id("pet-env-select").set_val(&environment);
    El::id("pet-bg-input").set_val(&data.get("background").string());
    // The background colour only applies to the solid environment.
    El::id("pet-bg-group").display(if environment == "solid" { "" } else { "none" });
}

fn init_pet() {
    if !El::id("pet-coat-select").ok() {
        return;
    }
    // 'change' rather than 'input': the colour picker fires 'input'
    // continuously while dragging.
    for (id, field) in [
        ("pet-coat-select", "coat"),
        ("pet-env-select", "environment"),
        ("pet-bg-input", "background"),
    ] {
        El::id(id).on_async("change", move |_| async move {
            let change = js_sys::Object::new();
            crate::js::set_field(&change, field, El::id(id).val());
            let result = api::post("/api/pet", change.into()).await;
            if let Ok(data) = &result {
                render_pet(data);
            }
            report_text(&El::id("pet-action-status"), &result, "Saved");
        });
    }

    El::id("pet-show-btn").on_async("click", |_| async {
        let result = api::post_only("/api/mode", obj! { "mode": "pet_cat" }).await;
        report_text(&El::id("pet-action-status"), &result, "Showing the cat");
    });

    dom::spawn(async {
        if let Ok(data) = api::get("/api/pet").await {
            render_pet(&data);
        }
    });
}

// ─── System lights ───────────────────────────────────────────────────────────

fn render_lights(data: &Js) {
    let enabled = data.get("enabled").truthy();
    El::id("lights-enabled-cb").set("checked", enabled);

    let line = El::id("lights-status-line");
    let devices: Vec<String> = data.get("devices").arr().iter().map(Js::show).collect();
    let error = data.get("error").string();
    if !enabled {
        line.set_text("Off. The lights keep whatever they were last showing.")
            .color("var(--overlay0)");
    } else if !devices.is_empty() {
        // A device may appear several times (two RAM sticks, say).
        let mut names = devices.clone();
        names.dedup();
        let swatch = data.get("color").str().map_or(String::new(), |c| {
            format!(
                r#" <span style="display:inline-block;width:0.9em;height:0.9em;border-radius:3px;vertical-align:middle;background:{}"></span>"#,
                esc(&c)
            )
        });
        line.set_html(&format!(
            "Driving {} device{}: {}{swatch}",
            devices.len(),
            plural(devices.len()),
            esc(&names.join(", "))
        ))
        .color("var(--green)");
    } else if error.is_empty() {
        line.set_text("Connecting to OpenRGB…")
            .color("var(--overlay0)");
    } else {
        line.set_text(&format!("OpenRGB unavailable: {error}"))
            .color("var(--red)");
    }
}

async fn fetch_lights() {
    if let Ok(data) = api::get("/api/lights").await {
        render_lights(&data);
    }
}

fn init_lights() {
    let toggle = El::id("lights-enabled-cb");
    if !toggle.ok() {
        return;
    }
    toggle.on_async("change", |_| async {
        let enabled = El::id("lights-enabled-cb").checked();
        match api::post("/api/lights", obj! { "enabled": enabled }).await {
            Ok(data) => render_lights(&data),
            Err(e) => {
                El::id("lights-status-line")
                    .set_text(&format!("Error: {e}"))
                    .color("var(--red)");
            }
        }
    });
    dom::poll(5000, fetch_lights);
}
