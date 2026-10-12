//! Shared page state and the pieces every section uses: the activity log,
//! status messages, toasts, the live event stream and the status poll.

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use web_sys::{EventSource, MessageEvent};

use crate::api;
use crate::dom::{self, El};
use crate::js::Js;
use crate::obj;
use crate::util::{fmt_sw_time, fmt_uptime, mode_label};

pub struct State {
    pub last_frame_seq: f64,
    pub last_mode: String,
    pub frozen: bool,
    /// While true the preview shows the MJPEG stream and must not be
    /// replaced by polled frames.
    pub mjpeg: bool,
    sse_retry_ms: f64,
}

thread_local! {
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            last_frame_seq: -1.0,
            last_mode: String::new(),
            frozen: false,
            mjpeg: false,
            sse_retry_ms: SSE_RETRY_MIN_MS,
        })
    };
}

const SSE_RETRY_MIN_MS: f64 = 2000.0;
const SSE_RETRY_MAX_MS: f64 = 30000.0;
const LOG_MAX_LINES: u32 = 100;

/// Modes that take over the panel temporarily and offer a Return button.
const TRANSIENT_MODES: [&str; 3] = ["custom_text", "custom_image", "countdown"];

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

// ─── Activity log ────────────────────────────────────────────────────────────

fn log_line(msg: &str, class: &str) {
    let list = El::id("log");
    let li = El::new("li");
    li.set_text(&format!("[{}] {msg}", dom::time_str()));
    if !class.is_empty() {
        li.set_class(class);
    }
    list.prepend(&li);
    while list.child_count() > LOG_MAX_LINES {
        list.find(":scope > :last-child").remove();
    }
}

pub fn log(msg: &str) {
    log_line(msg, "");
}

pub fn log_err(msg: &str) {
    log_line(msg, "err");
}

// ─── Status messages and toasts ──────────────────────────────────────────────

/// Show a green or red message that clears itself after `timeout_ms`.
pub fn status_for(el: &El, msg: &str, ok: bool, timeout_ms: i32) {
    if !el.ok() {
        return;
    }
    el.set_text(msg)
        .color(if ok { "var(--green)" } else { "var(--red)" });
    let (el, msg) = (el.clone(), msg.to_string());
    dom::set_timeout(timeout_ms, move || {
        if el.text() == msg {
            el.set_text("");
        }
    });
}

pub fn status(el: &El, msg: &str, ok: bool) {
    status_for(el, msg, ok, 3500);
}

/// Report the outcome of a request in a status element. Returns whether it
/// succeeded.
pub fn report<T>(el: &El, result: &Result<T, String>, ok_msg: &str) -> bool {
    match result {
        Ok(_) => status(el, ok_msg, true),
        Err(e) => status(el, &format!("Error: {e}"), false),
    }
    result.is_ok()
}

/// Write the outcome of a request as plain text. Returns whether it succeeded.
pub fn report_text<T>(el: &El, result: &Result<T, String>, ok_msg: &str) -> bool {
    match result {
        Ok(_) => el.set_text(ok_msg),
        Err(e) => el.set_text(&format!("Error: {e}")),
    };
    result.is_ok()
}

pub fn toast(msg: &str, kind: &str, duration_ms: i32) {
    let mut container = El::id("toast-container");
    if !container.ok() {
        container = El::new("div");
        container.set_id("toast-container");
        El::body().append(&container);
    }
    let toast = El::new("div");
    toast
        .set_class(&format!("toast toast-{kind}"))
        .set_text(msg);
    container.append(&toast);

    let t = toast.clone();
    dom::next_frame(move || {
        t.class("toast-visible", true);
    });
    dom::set_timeout(duration_ms, move || {
        toast.class("toast-visible", false);
        if let Some(e) = &toast.0 {
            let t = toast.clone();
            dom::listen_once(e, "transitionend", move |_| t.remove());
        }
    });
}

/// Toast the error of a failed request. Returns the value on success.
pub fn or_toast<T>(result: Result<T, String>) -> Option<T> {
    result
        .map_err(|e| toast(&format!("Error: {e}"), "alert", 3000))
        .ok()
}

// ─── Mode switching ──────────────────────────────────────────────────────────

pub fn set_active_mode(mode: &str) {
    for btn in El::all(".mode-btn") {
        btn.class("active", btn.data("mode") == mode);
    }
    El::id("current-mode").set_text(&mode_label(mode));
    with(|s| s.last_mode = mode.to_string());
}

/// Ask the daemon to switch mode, then highlight it.
pub async fn switch_mode(mode: &str) {
    match api::post_only("/api/mode", obj! { "mode": mode }).await {
        Ok(()) => {
            log(&format!("Mode → {mode}"));
            set_active_mode(mode);
        }
        Err(e) => log_err(&format!("Mode change failed: {e}")),
    }
}

/// Switch mode and refresh the status display; errors are ignored.
pub async fn go_to_mode(mode: &str) {
    if api::post_only("/api/mode", obj! { "mode": mode })
        .await
        .is_ok()
    {
        fetch_status().await;
    }
}

pub fn update_freeze_button() {
    let frozen = with(|s| s.frozen);
    El::id("freeze-btn")
        .set_text(if frozen { "▶ Unfreeze" } else { "❄ Freeze" })
        .class("active", frozen);
}

// ─── Connection indicator ────────────────────────────────────────────────────

fn set_connected(online: bool) {
    El::id("conn-dot")
        .set_class(if online {
            "conn-dot"
        } else {
            "conn-dot offline"
        })
        .set_title(if online { "Connected" } else { "Offline" });
}

pub fn init_conn_dot() {
    let bar = El::id("status-bar");
    let dot = El::new("span");
    dot.set_id("conn-dot")
        .set_class("conn-dot offline")
        .set_title("Connecting…");
    bar.prepend(&dot);
}

// ─── Live events ─────────────────────────────────────────────────────────────

fn on_sse(source: &EventSource, event: &str, f: impl Fn(Result<Js, String>) + 'static) {
    dom::listen(source, event, move |e| {
        set_connected(true);
        let data = e
            .dyn_ref::<MessageEvent>()
            .and_then(|m| m.data().as_string());
        f(Js::parse(&data.unwrap_or_default()));
    });
}

/// Subscribe to the daemon's event stream, reconnecting with backoff.
pub fn init_sse() {
    let Ok(source) = EventSource::new("/api/events") else {
        return;
    };

    on_sse(&source, "frame", |data| {
        let Ok(data) = data else {
            return refresh_preview();
        };
        let seq = data.get("seq").num_or(-1.0);
        let changed = with(|s| {
            s.frozen = data.get("frozen").truthy();
            let changed = s.last_frame_seq != seq;
            s.last_frame_seq = seq;
            changed
        });
        update_freeze_button();
        if changed {
            refresh_preview();
        }
    });

    on_sse(&source, "mode", |data| {
        if let Some(mode) = data
            .ok()
            .and_then(|d| d.get("mode").str())
            .filter(|m| !m.is_empty())
        {
            set_active_mode(&mode);
            toast(&format!("Mode → {}", mode_label(&mode)), "info", 2500);
        }
    });

    on_sse(&source, "alert", |data| {
        if let Some(summary) = data
            .ok()
            .and_then(|d| d.get("summary").str())
            .filter(|s| !s.is_empty())
        {
            toast(&summary, "alert", 6000);
        }
    });

    dom::listen(&source, "open", |_| {
        with(|s| s.sse_retry_ms = SSE_RETRY_MIN_MS);
        set_connected(true);
    });

    let failed = source.clone();
    dom::listen(&source, "error", move |_| {
        set_connected(false);
        failed.close();
        let delay = with(|s| {
            let delay = s.sse_retry_ms;
            s.sse_retry_ms = (delay * 1.5).min(SSE_RETRY_MAX_MS);
            delay
        });
        dom::set_timeout(delay as i32, init_sse);
    });
}

/// Load the latest frame off-screen and swap it in once it has arrived, so
/// the preview never shows a half-loaded image.
pub fn refresh_preview() {
    if with(|s| s.mjpeg) {
        return;
    }
    let url = format!("/api/preview?t={}", dom::now_ms());
    let loader = El::new("img");
    let src = url.clone();
    loader.on("load", move |_| {
        if with(|s| s.mjpeg) {
            return;
        }
        El::id("panel-preview-img")
            .set_attr("src", &src)
            .display("block");
        El::id("panel-preview-empty").hide();
    });
    loader.set_attr("src", &url);
}

// ─── Status poll ─────────────────────────────────────────────────────────────

/// Sync a settings control from the daemon once; later polls leave the
/// user's edits alone until `resync_settings` is called.
fn sync_once(el: &El, value: &str) -> bool {
    if el.data("synced") == "1" {
        return false;
    }
    el.set_val(value).set_data("synced", "1");
    true
}

/// Let the next status poll overwrite the settings controls again.
pub fn resync_settings() {
    for id in ["set-quality", "set-orient", "set-brightness"] {
        El::id(id).set_data("synced", "");
    }
}

pub async fn fetch_status() {
    // Silent on failure: the daemon may still be starting.
    let Ok(d) = api::get("/api/status").await else {
        return;
    };
    let mode = d.get("mode").string();
    let headless = d.get("headless").truthy();
    let fps = d.get("render_fps").num();
    let brightness_pct = (d.get("brightness").num_or(0.0) * 100.0).round();

    El::id("hdr-version").set_text(&format!("v{}", d.get("version").show()));

    let (dot_class, dot_title) = if headless {
        (
            "usb-dot usb-dot-headless",
            "Headless mode — USB panel not connected",
        )
    } else if d.get("panel_connected").0.as_bool() == Some(false) {
        (
            "usb-dot usb-dot-error",
            "USB panel disconnected — reconnecting…",
        )
    } else {
        ("usb-dot usb-dot-ok", "USB panel connected")
    };
    El::id("usb-status-dot")
        .set_class(dot_class)
        .set_title(dot_title);

    if let Some(fps) = fps {
        El::id("fps-badge")
            .set_text(&format!("{fps:.1} fps"))
            .show();
    }

    if mode != with(|s| s.last_mode.clone()) {
        set_active_mode(&mode);
    }
    // Checked on every poll: the event stream usually reports a mode change
    // first, so the mode alone does not say whether this is up to date.
    let can_return = d.get("can_clear").truthy() && TRANSIENT_MODES.contains(&mode.as_str());
    El::id("return-btn").display(if can_return { "" } else { "none" });

    let rotation_modes: Vec<String> = d.get("rotation_modes").arr().iter().map(Js::show).collect();
    let rotation_interval = d.get("rotation_interval").num_or(0.0);
    if rotation_interval > 0.0 && !rotation_modes.is_empty() {
        El::id("rot-indicator").display("flex");
        El::id("rot-mode-list").set_text(&rotation_modes.join(", "));
        El::id("rot-interval-display").set_text(&rotation_interval.to_string());
    } else {
        El::id("rot-indicator").hide();
    }

    let sw_ms = d.get("stopwatch_elapsed_ms").num_or(0.0);
    let stopwatch = if d.get("stopwatch_running").truthy() {
        format!("▶ {}", fmt_sw_time(sw_ms))
    } else if sw_ms > 0.0 {
        format!("⏸ {}", fmt_sw_time(sw_ms))
    } else {
        "Ready".to_string()
    };
    let frozen = d.get("frozen").truthy();

    El::id("info-version").set_text(&d.get("version").show());
    El::id("info-mode").set_text(&mode);
    El::id("info-rotate").set_text(&format!("{}°", d.get("rotate_deg").show()));
    El::id("info-quality").set_text(&d.get("quality").show());
    El::id("info-headless").set_text(if headless { "Yes" } else { "No" });
    El::id("info-pending").set_text(&d.get("pending_scheduled").num_or(0.0).to_string());
    El::id("info-brightness").set_text(&format!("{brightness_pct}%"));
    El::id("info-fps").set_text(&fps.map_or("—".to_string(), |f| format!("{f:.1}")));
    El::id("info-daemon-uptime").set_text(&fmt_uptime(d.get("daemon_uptime_secs").num_or(0.0)));
    El::id("info-frame-seq").set_text(&d.get("frame_seq").show());
    El::id("info-frozen").set_text(if frozen { "❄ Yes" } else { "No" });
    El::id("info-sw").set_text(&stopwatch);

    with(|s| s.frozen = frozen);
    update_freeze_button();

    if sync_once(&El::id("set-quality"), &d.get("quality").show()) {
        El::id("set-quality-lbl").set_text(&d.get("quality").show());
    }
    sync_once(&El::id("set-orient"), &d.get("rotate_deg").show());
    if sync_once(&El::id("set-brightness"), &brightness_pct.to_string()) {
        El::id("set-brightness-lbl").set_text(&format!("{brightness_pct}%"));
    }
}

// ─── Header controls ─────────────────────────────────────────────────────────

pub fn init_theme() {
    let root = El(dom::doc().document_element());
    let toggle = El::id("theme-toggle-btn");
    if dom::storage_get("ttlcd-theme").as_deref() == Some("latte") {
        root.set_attr("data-theme", "latte");
        toggle.set_text("☀️");
    }
    let btn = toggle.clone();
    toggle.on("click", move |_| {
        let Some(root) = dom::doc().document_element() else {
            return;
        };
        if root.get_attribute("data-theme").as_deref() == Some("latte") {
            let _ = root.remove_attribute("data-theme");
            dom::storage_set("ttlcd-theme", "dark");
            btn.set_text("🌙");
        } else {
            let _ = root.set_attribute("data-theme", "latte");
            dom::storage_set("ttlcd-theme", "latte");
            btn.set_text("☀️");
        }
    });
}

pub fn init_return_button() {
    El::id("return-btn").on_async("click", |_| async {
        match api::post_empty("/api/clear").await {
            Ok(()) => {
                log("Returned to previous mode");
                El::id("return-btn").hide();
            }
            Err(e) => log_err(&format!("Clear failed: {e}")),
        }
    });
}
