//! Cards that push content to the panel on demand: notifications, text,
//! images, timers, banners, rotation and the panel settings.

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{DragEvent, File, FormData, Url};

use crate::api;
use crate::core::{self, log, log_err, report, report_text, status, status_for};
use crate::dom::{self, El};
use crate::js::{arr, opt, Js};
use crate::obj;
use crate::util::{
    esc, fmt_uptime, int_or, non_empty_or, parse_float, parse_int, prefix, split_list,
};

thread_local! {
    /// The image chosen in the upload card, waiting to be sent.
    static IMAGE: RefCell<Option<File>> = const { RefCell::new(None) };
    /// Which kind of history entry the list is filtered to (`""` = all).
    static HISTORY_KIND: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn init() {
    init_quick_notify();
    init_custom_text();
    init_presets();
    init_image_upload();
    init_slideshow();
    init_countdown();
    init_banner();
    init_rotation();
    init_settings();
    init_freeze();
    init_schedule();
    init_history();
    init_flash();
    init_progress();
    init_mjpeg_toggle();
    El::id("log-clear-btn").on("click", |_| {
        El::id("log").clear();
    });
}

// ─── Quick notify ────────────────────────────────────────────────────────────

/// (key, title, line, colour, seconds shown; 0 = until replaced)
const QUICK_PRESETS: [(&str, &str, &str, &str, u32); 8] = [
    ("ok", "✓ OK", "All systems operational", "green", 8),
    ("fail", "✗ FAILED", "Operation failed", "red", 15),
    ("warn", "⚠ Warning", "Check required", "yellow", 12),
    ("run", "⟳ Running", "Operation in progress…", "teal", 0),
    ("deploy", "↑ Deploying", "Deploy in progress…", "blue", 0),
    ("done", "✓ Done", "Task completed", "green", 10),
    ("info", "ℹ Info", "FYI", "blue", 8),
    (
        "reboot",
        "↺ Rebooting",
        "System restart initiated",
        "peach",
        0,
    ),
];

fn init_quick_notify() {
    for btn in El::all(".quick-btn") {
        let key = btn.data("quick");
        let Some(&(_, title, line, color, secs)) = QUICK_PRESETS.iter().find(|p| p.0 == key) else {
            continue;
        };
        btn.on_async("click", move |_| async move {
            let body = obj! {
                "title": title,
                "lines": arr([line]),
                "color": color,
                "duration_secs": (secs > 0).then_some(secs),
                "show_time": true,
            };
            let result = api::post_only("/api/text", body).await;
            if report(&El::id("quick-status"), &result, &format!("Sent: {title}")) {
                log(&format!("Quick: {title}"));
            }
        });
    }

    El::id("test-pattern-btn").on_async("click", |_| async {
        let result = api::post_empty("/api/test").await;
        if report(&El::id("quick-status"), &result, "🎨 Test card sent (5s)") {
            log("Test pattern sent");
        }
    });
}

// ─── Custom text ─────────────────────────────────────────────────────────────

fn progress_label(value: i64) -> String {
    if value < 0 {
        "off".into()
    } else {
        format!("{value}%")
    }
}

/// The custom-text form as a `/api/text` body.
pub fn text_payload() -> JsValue {
    let lines: Vec<String> = El::id("txt-lines")
        .val()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let progress = parse_int(&El::id("txt-progress").val()).filter(|p| *p >= 0);
    let duration = parse_int(&El::id("txt-duration").val()).filter(|d| *d > 0);
    obj! {
        "title": opt(&El::id("txt-title").val()),
        "subtitle": opt(&El::id("txt-subtitle").val()),
        "lines": (!lines.is_empty()).then(|| arr(lines)),
        "progress": progress.map(|p| p as f64),
        "color": El::id("txt-color").val(),
        "show_time": El::id("txt-show-time").checked(),
        "duration_secs": duration.map(|d| d as f64),
    }
}

fn init_custom_text() {
    El::id("txt-progress").on("input", |_| {
        let value = parse_int(&El::id("txt-progress").val()).unwrap_or(-1);
        El::id("prog-pct-label").set_text(&progress_label(value));
    });

    El::id("txt-send-btn").on_async("click", |_| async {
        let result = api::post_only("/api/text", text_payload()).await;
        if report(&El::id("txt-status"), &result, "✓ Sent") {
            let title = opt(&El::id("txt-title").val()).unwrap_or_else(|| "(no title)".into());
            log(&format!("Text push: \"{title}\""));
        }
    });

    El::id("txt-preview-btn").on_async("click", |_| async {
        let result = async {
            let resp = api::call("POST", "/api/render", Some(text_payload())).await?;
            let blob = api::blob(&resp).await?;
            El::id("txt-preview-img").set_blob_src(&blob);
            El::id("txt-preview-wrap").show();
            Ok(())
        }
        .await;
        report(&El::id("txt-status"), &result, "👁 Preview rendered");
    });
}

// ─── Presets ─────────────────────────────────────────────────────────────────

pub async fn fetch_presets() {
    let Ok(list) = api::get("/api/presets").await else {
        return;
    };
    let select = El::id("preset-select");
    select.set_html(r#"<option value="">— saved presets —</option>"#);
    for preset in list.arr() {
        let name = preset.get("name").string();
        let option = El::new("option");
        option.set_val(&name).set_text(&name);
        select.append(&option);
    }
}

/// Log the outcome of a preset operation and reload the list on success.
async fn preset_done(result: Result<(), String>, done: &str, failed: &str) {
    match result {
        Ok(()) => {
            log(done);
            fetch_presets().await;
        }
        Err(e) => log_err(&format!("{failed}: {e}")),
    }
}

fn init_presets() {
    El::id("preset-apply-btn").on_async("click", |_| async {
        let name = El::id("preset-select").val();
        if name.is_empty() {
            return;
        }
        let result = async {
            let p = api::get(&format!("/api/presets/{}", api::enc(&name))).await?;
            let lines: Vec<String> = p.get("lines").arr().iter().map(Js::show).collect();
            let progress = p.get("progress").num_or(-1.0) as i64;
            El::id("txt-title").set_val(&p.get("title").string());
            El::id("txt-subtitle").set_val(&p.get("subtitle").string());
            El::id("txt-lines").set_val(&lines.join("\n"));
            El::id("txt-progress").set_val(&progress.to_string());
            El::id("prog-pct-label").set_text(&progress_label(progress));
            if let Some(color) = p.get("color").str().filter(|c| !c.is_empty()) {
                El::id("txt-color").set_val(&color);
            }
            El::id("txt-show-time").set("checked", p.get("show_time").0.as_bool() != Some(false));
            api::post_empty(&format!("/api/presets/{}/apply", api::enc(&name))).await
        }
        .await;
        match result {
            Ok(()) => log(&format!("Preset applied: {name}")),
            Err(e) => log_err(&format!("Preset apply error: {e}")),
        }
    });

    El::id("preset-save-btn").on_async("click", |_| async {
        let selected = El::id("preset-select").val();
        let name = if selected.is_empty() {
            dom::prompt("Preset name:").unwrap_or_default()
        } else {
            selected
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let body = obj! { "name": name.as_str(), "payload": text_payload() };
        let result = api::post_only("/api/presets", body).await;
        preset_done(
            result,
            &format!("Preset saved: {name}"),
            "Preset save error",
        )
        .await;
        El::id("preset-select").set_val(&name);
    });

    El::id("preset-del-btn").on_async("click", |_| async {
        let name = El::id("preset-select").val();
        if name.is_empty() || !dom::confirm(&format!("Delete preset \"{name}\"?")) {
            return;
        }
        let result = api::delete(&format!("/api/presets/{}", api::enc(&name))).await;
        preset_done(
            result,
            &format!("Preset deleted: {name}"),
            "Preset delete error",
        )
        .await;
    });

    El::id("preset-clear-btn").on_async("click", |_| async {
        if !dom::confirm("Delete ALL presets?") {
            return;
        }
        preset_done(
            api::delete("/api/presets").await,
            "All presets cleared",
            "Preset clear error",
        )
        .await;
    });

    El::id("preset-import-input").on_async("change", |_| async {
        let input = El::id("preset-import-input");
        let Some(file) = input.first_file() else {
            return;
        };
        let result = async {
            let form = FormData::new().map_err(|e| dom::err_msg(&e))?;
            form.append_with_blob_and_filename("file", &file, &file.name())
                .map_err(|e| dom::err_msg(&e))?;
            api::post_form("/api/presets/import", &form).await.map(drop)
        }
        .await;
        preset_done(
            result,
            &format!("Presets imported from {}", file.name()),
            "Preset import error",
        )
        .await;
        input.set_val("");
    });
}

// ─── Image upload ────────────────────────────────────────────────────────────

fn choose_image(file: Option<File>) {
    let Some(file) = file else { return };
    El::id("preview").set_blob_src(&file);
    El::id("preview-wrap").class("hidden", false).show();
    IMAGE.with(|i| *i.borrow_mut() = Some(file));
}

fn init_image_upload() {
    let drop_zone = El::id("drop-zone");
    drop_zone.on("click", |_| El::id("file-input").click());
    El::id("file-input").on("change", |_| {
        choose_image(El::id("file-input").first_file());
    });

    drop_zone.on("dragover", |e| {
        e.prevent_default();
        El::id("drop-zone").class("drag-over", true);
    });
    drop_zone.on("dragleave", |_| {
        El::id("drop-zone").class("drag-over", false);
    });
    drop_zone.on("drop", |e| {
        e.prevent_default();
        El::id("drop-zone").class("drag-over", false);
        let files = e
            .dyn_ref::<DragEvent>()
            .and_then(web_sys::DragEvent::data_transfer)
            .and_then(|t| t.files());
        choose_image(files.and_then(|f| f.get(0)));
    });

    El::id("send-btn").on_async("click", |_| async {
        let Some(file) = IMAGE.with(|i| i.borrow().clone()) else {
            return;
        };
        let status = El::id("upload-status");
        status.set_text("Uploading…");
        let result = async {
            let form = FormData::new().map_err(|e| dom::err_msg(&e))?;
            form.append_with_blob("file", &file)
                .map_err(|e| dom::err_msg(&e))?;
            api::post_form("/api/image", &form).await
        }
        .await;
        match result {
            Ok(_) => {
                status.set_text("✓ Image sent");
                log(&format!("Image sent: {}", file.name()));
                dom::set_timeout(3000, move || {
                    status.set_text("");
                });
            }
            Err(e) => {
                status.set_text(&format!("Error: {e}"));
                log_err(&format!("Image upload error: {e}"));
            }
        }
    });

    El::id("cancel-btn").on("click", |_| {
        IMAGE.with(|i| *i.borrow_mut() = None);
        El::id("preview-wrap").class("hidden", true).hide();
        El::id("file-input").set_val("");
    });

    El::id("img-url-send-btn").on_async("click", |_| async {
        let status_el = El::id("img-url-status");
        let url = El::id("img-url-input").trimmed();
        if url.is_empty() {
            return status(&status_el, "Enter a URL first", false);
        }
        let letterbox = El::id("img-url-letterbox").val() != "false";
        status_for(&status_el, "⏳ Fetching image…", true, 30000);
        let result = api::post_only(
            "/api/image/url",
            obj! { "url": url.as_str(), "letterbox": letterbox },
        )
        .await;
        match &result {
            Ok(()) => log(&format!("Image from URL: {}", prefix(&url, 60))),
            Err(e) => log_err(&format!("Image URL error: {e}")),
        }
        report(&status_el, &result, "✓ Image displayed");
    });
}

// ─── Slideshow ───────────────────────────────────────────────────────────────

fn init_slideshow() {
    El::id("ss-start-btn").on_async("click", |_| async {
        let status_el = El::id("ss-status");
        let dir = El::id("ss-dir").trimmed();
        if dir.is_empty() {
            return status(&status_el, "Enter a directory path", false);
        }
        let interval = int_or(&El::id("ss-interval").val(), 5) as f64;
        let result = api::post_only(
            "/api/slideshow",
            obj! { "dir": dir.as_str(), "interval": interval },
        )
        .await;
        if report(&status_el, &result, "▶ Slideshow started") {
            log(&format!("Slideshow started: {dir}"));
        }
    });
    El::id("ss-prev-btn").on_async("click", |_| async {
        let _ = api::post_empty("/api/slideshow/prev").await;
    });
    El::id("ss-next-btn").on_async("click", |_| async {
        let _ = api::post_empty("/api/slideshow/next").await;
    });
    El::id("ss-shuffle-btn").on_async("click", |_| async {
        report(
            &El::id("ss-status"),
            &api::post_empty("/api/slideshow/shuffle").await,
            "🔀 Shuffled",
        );
    });
}

// ─── Countdown ───────────────────────────────────────────────────────────────

const COUNTDOWN_PRESETS: [(&str, u32); 5] = [
    ("5m", 300),
    ("10m", 600),
    ("15m", 900),
    ("25m 🍅", 1500),
    ("1h", 3600),
];

/// Total seconds from the hour/minute/second pickers, mirrored into the
/// hidden `cd-seconds` field.
fn countdown_seconds() -> i64 {
    let part = |id: &str| parse_int(&El::id(id).val()).unwrap_or(0);
    let total = part("cd-hours") * 3600 + part("cd-minutes") * 60 + part("cd-secs-i");
    El::id("cd-seconds").set_val(&total.to_string());
    total
}

fn init_countdown() {
    for id in ["cd-hours", "cd-minutes", "cd-secs-i"] {
        El::id(id).on("input", |_| {
            countdown_seconds();
        });
    }

    El::id("cd-start-btn").on_async("click", |_| async {
        let status_el = El::id("cd-status");
        let secs = countdown_seconds();
        if secs <= 0 {
            return status(&status_el, "Set a time > 0", false);
        }
        let body = obj! { "seconds": secs as f64, "label": opt(&El::id("cd-label").val()) };
        let result = api::post_only("/api/countdown", body).await;
        if report(
            &status_el,
            &result,
            &format!("⏳ {} countdown started", fmt_uptime(secs as f64)),
        ) {
            log(&format!("Countdown started: {secs}s"));
        }
    });

    El::id("cd-cancel-btn").on_async("click", |_| async {
        if report(
            &El::id("cd-status"),
            &api::delete("/api/countdown").await,
            "✕ Countdown cancelled",
        ) {
            log("Countdown cancelled");
        }
    });

    let bar = El::new("div");
    bar.set_class("cd-preset-bar");
    for (label, secs) in COUNTDOWN_PRESETS {
        let btn = El::new("button");
        btn.set_class("cd-preset-btn")
            .set_text(label)
            .set_title(&format!("Start {label} countdown"));
        btn.on_async("click", move |_| async move {
            let result = api::post_only("/api/countdown", obj! { "seconds": secs }).await;
            if report(
                &El::id("cd-status"),
                &result,
                &format!("⏳ {label} countdown started"),
            ) {
                log(&format!("Preset countdown started: {label}"));
            }
        });
        bar.append(&btn);
    }
    El::id("cd-cancel-btn").parent().insert_after(&bar);
}

// ─── Banner ──────────────────────────────────────────────────────────────────

fn init_banner() {
    El::id("ban-send-btn").on_async("click", |_| async {
        let status_el = El::id("ban-status");
        let lines: Vec<String> = ["ban-line1", "ban-line2", "ban-line3"]
            .iter()
            .map(|id| El::id(id).val())
            .filter(|l| !l.is_empty())
            .collect();
        let Some(first) = lines.first().cloned() else {
            return status(&status_el, "Enter at least one line", false);
        };
        let body = obj! {
            "lines": arr(lines),
            "label": opt(&El::id("ban-label").val()),
            "color": El::id("ban-color").val(),
            "show_time": true,
            "duration": 0,
        };
        if report(
            &status_el,
            &api::post_only("/api/banner", body).await,
            "✓ Banner shown",
        ) {
            log(&format!("Banner: {first}"));
        }
    });
}

// ─── Auto-rotation ───────────────────────────────────────────────────────────

fn init_rotation() {
    El::id("rot-start-btn").on_async("click", |_| async {
        let status_el = El::id("rot-status");
        let modes = split_list(&El::id("rot-modes").val(), false);
        let interval = int_or(&El::id("rot-interval").val(), 30);
        if modes.is_empty() {
            return status(&status_el, "Enter at least one mode", false);
        }
        let summary = modes.join(", ");
        let body = obj! { "modes": arr(modes), "interval": interval as f64 };
        if report(
            &status_el,
            &api::post_only("/api/rotation", body).await,
            "▶ Rotation active",
        ) {
            log(&format!(
                "Auto-rotation started: {summary} every {interval}s"
            ));
        }
    });

    El::id("rot-stop-btn").on_async("click", |_| async {
        let body = obj! { "modes": js_sys::Array::new(), "interval": 0 };
        if report(
            &El::id("rot-status"),
            &api::post_only("/api/rotation", body).await,
            "■ Rotation stopped",
        ) {
            log("Auto-rotation stopped");
        }
    });
}

// ─── Panel settings ──────────────────────────────────────────────────────────

fn init_settings() {
    El::id("set-quality").on("input", |_| {
        El::id("set-quality-lbl").set_text(&El::id("set-quality").val());
    });
    El::id("set-brightness").on("input", |_| {
        El::id("set-brightness-lbl").set_text(&format!("{}%", El::id("set-brightness").val()));
    });

    El::id("set-apply-btn").on_async("click", |_| async {
        let quality = parse_float(&El::id("set-quality").val()).unwrap_or(85.0);
        let degrees = parse_float(&El::id("set-orient").val()).unwrap_or(0.0);
        let brightness = parse_float(&El::id("set-brightness").val()).unwrap_or(100.0);
        let result = async {
            api::post_only("/api/quality", obj! { "quality": quality }).await?;
            api::post_only("/api/orient", obj! { "degrees": degrees }).await?;
            api::post_only("/api/brightness", obj! { "brightness": brightness / 100.0 }).await
        }
        .await;
        if report(&El::id("set-status"), &result, "✓ Settings applied") {
            log(&format!(
                "Settings: quality={quality} rotate={degrees}° brightness={brightness}%"
            ));
        }
    });
}

// ─── Freeze ──────────────────────────────────────────────────────────────────

fn init_freeze() {
    El::id("freeze-btn").on_async("click", |_| async {
        let frozen = core::with(|s| s.frozen);
        let path = if frozen {
            "/api/display/unfreeze"
        } else {
            "/api/display/freeze"
        };
        match api::post_empty(path).await {
            Ok(()) => {
                core::with(|s| s.frozen = !frozen);
                log(if frozen {
                    "Display unfrozen"
                } else {
                    "Display frozen"
                });
                core::update_freeze_button();
            }
            Err(e) => log_err(&format!("Freeze toggle error: {e}")),
        }
    });
}

// ─── Scheduled notification ──────────────────────────────────────────────────

fn init_schedule() {
    El::id("sched-btn").on_async("click", |_| async {
        let status_el = El::id("sched-status");
        let secs = parse_int(&El::id("sched-in-secs").val()).unwrap_or(0);
        if secs < 1 {
            return status(&status_el, "Enter delay > 0", false);
        }
        let body = obj! {
            "delay_secs": secs as f64,
            "label": opt(&El::id("sched-label").val()),
            "payload": text_payload(),
        };
        let result = api::post_only("/api/schedule", body).await;
        if report(
            &status_el,
            &result,
            &format!("✓ Scheduled in {}", fmt_uptime(secs as f64)),
        ) {
            log(&format!("Notification scheduled: +{secs}s"));
        }
    });
}

// ─── History ─────────────────────────────────────────────────────────────────

const HISTORY_KINDS: [&str; 5] = ["", "text", "alert", "image", "system"];

fn history_kind() -> String {
    HISTORY_KIND.with(|k| k.borrow().clone())
}

pub async fn fetch_history() {
    let kind = history_kind();
    let filter = if kind.is_empty() {
        String::new()
    } else {
        format!("&kind={}", api::enc(&kind))
    };
    let Ok(d) = api::get(&format!("/api/history?limit=30{filter}")).await else {
        return;
    };
    let entries = if d.get("entries").is_nullish() {
        d.arr()
    } else {
        d.get("entries").arr()
    };

    let list = El::id("history-list");
    if entries.is_empty() {
        let message = if kind.is_empty() {
            "No history yet".to_string()
        } else {
            format!("No {kind} entries")
        };
        list.set_html(&format!(
            r#"<li style="color:var(--overlay0)">{}</li>"#,
            esc(&message)
        ));
    } else {
        let rows: String = entries
            .iter()
            .map(|entry| {
                let secs = entry.get("timestamp").num().or(entry.get("ts_secs").num()).unwrap_or(0.0);
                let entry_kind = entry.get("kind").str().filter(|k| !k.is_empty()).unwrap_or_else(|| "text".into());
                let message = entry.get("summary").str().unwrap_or_else(|| entry.get("message").string());
                format!(
                    r#"<li><span class="hist-ts">{}</span><span class="hist-kind hist-{kind}">{kind}</span><span class="hist-msg">{}</span></li>"#,
                    esc(&dom::locale_time(secs * 1000.0)),
                    esc(&message),
                    kind = esc(&entry_kind),
                )
            })
            .collect();
        list.set_html(&rows);
    }

    for btn in El::all(".hist-filter-btn") {
        btn.class("active", btn.data("kind") == kind);
    }
    El::q(r#"a[download="ttlcd-history.csv"]"#).set_attr(
        "href",
        &format!("/api/history?format=csv&limit=500{filter}"),
    );
}

fn init_history() {
    let list = El::id("history-list");

    let buttons: String = HISTORY_KINDS
        .iter()
        .map(|k| {
            let label = if k.is_empty() { "all" } else { k };
            format!(r#"<button class="hist-filter-btn small-btn" data-kind="{k}">{label}</button>"#)
        })
        .collect();
    let bar = El::build(
        "div",
        "hist-filter-bar",
        &format!(r#"<span class="hist-filter-label">Filter:</span>{buttons}"#),
    );
    bar.set_id("hist-filter-bar");
    list.insert_before(&bar);
    bar.on_async("click", |e| async move {
        let btn = dom::target(&e).closest(".hist-filter-btn");
        if btn.ok() {
            HISTORY_KIND.with(|k| *k.borrow_mut() = btn.data("kind"));
            fetch_history().await;
        }
    });

    El::id("hist-refresh-btn").on_async("click", |_| fetch_history());
    El::id("hist-clear-btn").on_async("click", |_| async {
        match api::delete("/api/history").await {
            Ok(()) => {
                log("History cleared");
                fetch_history().await;
            }
            Err(e) => log_err(&format!("History clear error: {e}")),
        }
    });

    let export = El::new("button");
    export
        .set_class("btn-secondary hist-export-btn")
        .set_text("⬇ Export")
        .set_title("Download history as JSON")
        .css_text("font-size:0.75rem;padding:0.2rem 0.5rem;margin-left:0.5rem");
    export.on_async("click", |_| async {
        if let Err(e) = export_history().await {
            dom::alert(&format!("Export failed: {e}"));
        }
    });
    list.closest(".card").find("h2").append(&export);
}

/// Download the full history as a dated JSON file.
async fn export_history() -> Result<(), String> {
    let data = api::get("/api/history").await?;
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let parts = arr([data.to_json(true)]);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)
        .map_err(|e| dom::err_msg(&e))?;
    let url = Url::create_object_url_with_blob(&blob).map_err(|e| dom::err_msg(&e))?;
    let date: String = js_sys::Date::new_0().to_iso_string().into();
    let link = El::new("a");
    link.set_attr("href", &url).set_attr(
        "download",
        &format!("ttlcd-history-{}.json", prefix(&date, 10)),
    );
    link.click();
    let _ = Url::revoke_object_url(&url);
    Ok(())
}

// ─── Flash message ───────────────────────────────────────────────────────────

fn init_flash() {
    El::id("flash-send-btn").on_async("click", |_| async {
        let (btn, status_el) = (El::id("flash-send-btn"), El::id("flash-status"));
        let text = El::id("flash-text").trimmed();
        if text.is_empty() {
            status_el
                .set_text("Enter a message first")
                .color("var(--red)");
            return;
        }
        let duration = parse_int(&El::id("flash-duration").val()).unwrap_or(10);
        let color = non_empty_or(El::id("flash-color").val(), "#89b4fa");

        btn.set_disabled(true);
        status_el.set_text("Sending…").color("var(--overlay0)");
        let body = obj! { "text": text, "duration": duration as f64, "color": color };
        match api::post_only("/api/flash", body).await {
            Ok(()) => status_el
                .set_text(&format!("✓ Showing for {duration}s"))
                .color("var(--green)"),
            Err(e) => status_el
                .set_text(&format!("Error: {e}"))
                .color("var(--red)"),
        };
        btn.set_disabled(false);
        dom::set_timeout(4000, move || {
            status_el.set_text("");
        });
    });
}

// ─── Progress display ────────────────────────────────────────────────────────

fn init_progress() {
    El::id("prog-set-btn").on_async("click", |_| async {
        let value_of = |id: &str, default: &str| non_empty_or(El::id(id).val(), default);
        let body = obj! {
            "label": value_of("prog-label", "Progress"),
            "sublabel": El::id("prog-sublabel").val(),
            "value": parse_float(&El::id("prog-value").val()).unwrap_or(0.0),
            "total": parse_float(&El::id("prog-total").val()).unwrap_or(100.0),
            "color": value_of("prog-color", "blue"),
            "start_timer": El::id("prog-timer-check").checked(),
        };
        let result = api::post("/api/progress", body).await;
        let pct = result.as_ref().map_or(0.0, |r| r.get("pct").num_or(0.0));
        if report_text(
            &El::id("prog-status"),
            &result,
            &format!("{pct:.1}% — showing on panel"),
        ) {
            core::fetch_status().await;
        }
    });

    El::id("prog-clear-btn").on_async("click", |_| async {
        if report_text(
            &El::id("prog-status"),
            &api::delete("/api/progress").await,
            "Cleared",
        ) {
            core::fetch_status().await;
        }
    });
}

// ─── MJPEG preview ───────────────────────────────────────────────────────────

fn init_mjpeg_toggle() {
    El::id("mjpeg-toggle-btn").on("click", |_| {
        let btn = El::id("mjpeg-toggle-btn");
        let active = core::with(|s| {
            s.mjpeg = !s.mjpeg;
            s.mjpeg
        });
        if active {
            El::id("panel-preview-img")
                .set_attr("src", &format!("/api/stream?t={}", dom::now_ms()))
                .display("block");
            El::id("panel-preview-empty").hide();
            btn.set_text("📷 Poll")
                .set_title("Switch back to JPEG polling mode")
                .color("var(--green)");
            log("Switched to embedded MJPEG stream");
        } else {
            El::id("panel-preview-img").set_attr("src", "");
            btn.set_text("📺 MJPEG")
                .set_title("Toggle between JPEG polling and embedded MJPEG stream")
                .color("");
            log("Switched back to JPEG polling");
            core::refresh_preview();
        }
    });
}
