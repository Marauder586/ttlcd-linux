//! Endpoints for the panel itself: preview and streams, quality, brightness, stopwatch, schedules and the embedded web UI.

use super::*;

// ─── Runtime quality / orient controls ───────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct QualityRequest {
    /// New JPEG quality (1–100).
    quality: u8,
}

/// POST /api/quality — change JPEG quality at runtime (1–100).
pub(crate) async fn api_set_quality(
    state: web::Data<DaemonState>,
    body: web::Json<QualityRequest>,
) -> impl Responder {
    let q = body.quality.clamp(1, 100);
    *state.quality.lock().unwrap() = q;
    log::info!("quality set to {q}");
    persist_state(&state);
    // Persist to config file in background
    tokio::spawn(async move {
        persist_setting("quality", &q.to_string());
    });
    HttpResponse::Ok().body(format!("{q}"))
}

#[derive(Deserialize)]
pub(crate) struct OrientRequest {
    /// Rotation in degrees: 0, 90, 180, or 270.
    rotate: u16,
}

/// POST /api/orient — rotate all output at runtime (0/90/180/270).
pub(crate) async fn api_set_orient(
    state: web::Data<DaemonState>,
    body: web::Json<OrientRequest>,
) -> impl Responder {
    let deg = match body.rotate {
        0 | 90 | 180 | 270 => body.rotate,
        other => {
            return HttpResponse::BadRequest()
                .body(format!("invalid rotation {other}; use 0, 90, 180, or 270"))
        }
    };
    *state.rotate_deg.lock().unwrap() = deg;
    log::info!("rotation set to {deg}°");
    persist_state(&state);
    HttpResponse::Ok().body(format!("{deg}°"))
}

// ─── Auto-rotation API ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct RotationRequest {
    pub(crate) modes: Vec<String>,
    /// Seconds between advances. 0 = stop rotation.
    pub(crate) interval: u64,
}

// ─── Preview & sysinfo ────────────────────────────────────────────────────────

pub(crate) async fn api_preview_frame(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .content_type("image/jpeg")
            .body(jpeg.clone()),
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
    }
}

/// GET /api/preview.png — current panel frame as lossless PNG.
///
/// Decodes the stored JPEG and re-encodes as PNG. Useful for high-quality
/// screenshots, image processing, and clients that prefer lossless format.
pub(crate) async fn api_preview_frame_png(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => {
            // Decode JPEG → re-encode as PNG
            match image::load_from_memory(jpeg) {
                Ok(img) => {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    match img.write_to(&mut buf, image::ImageFormat::Png) {
                        Ok(()) => HttpResponse::Ok()
                            .insert_header(("Cache-Control", "no-store"))
                            .content_type("image/png")
                            .body(buf.into_inner()),
                        Err(e) => HttpResponse::InternalServerError()
                            .body(format!("PNG encode error: {e}")),
                    }
                }
                Err(e) => {
                    HttpResponse::InternalServerError().body(format!("JPEG decode error: {e}"))
                }
            }
        }
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
    }
}

#[derive(Serialize)]
pub(crate) struct SysinfoJson {
    cpu_pct: f32,
    mem_used: u64,
    mem_total: u64,
    uptime: u64,
    load1: f64,
    load5: f64,
    load15: f64,
    hostname: String,
    cpu_temp: Option<f32>,
}

pub(crate) async fn api_sysinfo_json() -> impl Responder {
    use sysinfo::{Components, System};
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let load = System::load_average();
    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);
    let cpu_temp: Option<f32> = comps
        .iter()
        .filter(|c| {
            let l = c.label().to_lowercase();
            l.contains("cpu") || l.contains("core") || l.contains("package")
        })
        .filter_map(sysinfo::Component::temperature)
        .filter(|t| *t > 0.0 && !t.is_nan())
        .reduce(f32::max);
    HttpResponse::Ok().json(SysinfoJson {
        cpu_pct: sys.global_cpu_usage(),
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
        uptime: System::uptime(),
        load1: load.one,
        load5: load.five,
        load15: load.fifteen,
        hostname: System::host_name().unwrap_or_default(),
        cpu_temp,
    })
}

// ─── Server-Sent Events stream ────────────────────────────────────────────────

/// GET /api/events — Server-Sent Events stream for real-time frame updates.
///
/// Sends `event: frame\ndata: {seq}\n\n` whenever a new panel frame is stored.
/// Sends a `: heartbeat` comment every 15 s to keep the TCP connection alive.
/// The browser can use this to trigger a fresh `/api/preview` fetch without
/// polling on a fixed timer.
pub(crate) async fn api_events(state: web::Data<DaemonState>) -> impl Responder {
    let frame_seq = Arc::clone(&state.frame_seq);
    let mode_arc = Arc::clone(&state.mode);
    let frozen_arc = Arc::clone(&state.frozen);
    let alert_seq_arc = Arc::clone(&state.alert_seq);
    let last_alert_arc = Arc::clone(&state.last_alert_event);

    let init_mode = mode_arc.lock().unwrap().as_str().to_string();
    let init_alert_seq = *alert_seq_arc.lock().unwrap();

    // State tuple: (frame_seq, last_frame_seq, last_heartbeat, last_mode,
    //               mode_arc, frozen_arc, alert_seq_arc, last_alert_arc, last_alert_seq)
    let stream = futures_util::stream::unfold(
        (
            frame_seq,
            u64::MAX,
            std::time::Instant::now(),
            init_mode,
            mode_arc,
            frozen_arc,
            alert_seq_arc,
            last_alert_arc,
            init_alert_seq,
        ),
        |(
            seq,
            last_seq,
            last_hb,
            last_mode,
            mode_arc,
            frozen_arc,
            alert_seq_arc,
            last_alert_arc,
            last_alert_seq,
        )| async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let cur = *seq.lock().unwrap();
                let mode = mode_arc.lock().unwrap().as_str().to_string();
                let alert_seq = *alert_seq_arc.lock().unwrap();
                let now = std::time::Instant::now();

                // Alert event (new alert pushed to history)
                if alert_seq != last_alert_seq {
                    let summary = last_alert_arc
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|(_, s)| s.replace('"', "\'").replace('\n', " "))
                        .unwrap_or_default();
                    let event = format!(
                        "event: alert\ndata: {{\"seq\":{alert_seq},\"summary\":\"{summary}\"}}\n\n"
                    );
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (
                            seq,
                            last_seq,
                            last_hb,
                            last_mode,
                            mode_arc,
                            frozen_arc,
                            alert_seq_arc,
                            last_alert_arc,
                            alert_seq,
                        ),
                    ));
                }

                // Frame event
                if cur != last_seq {
                    let frozen = *frozen_arc.lock().unwrap();
                    let event =
                        format!("event: frame\ndata: {{\"seq\":{cur},\"frozen\":{frozen}}}\n\n");
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (
                            seq,
                            cur,
                            last_hb,
                            last_mode,
                            mode_arc,
                            frozen_arc,
                            alert_seq_arc,
                            last_alert_arc,
                            last_alert_seq,
                        ),
                    ));
                }

                // Mode change event
                if mode != last_mode {
                    let event = format!("event: mode\ndata: {{\"mode\":\"{mode}\"}}\n\n");
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(event)),
                        (
                            seq,
                            last_seq,
                            last_hb,
                            mode,
                            mode_arc,
                            frozen_arc,
                            alert_seq_arc,
                            last_alert_arc,
                            last_alert_seq,
                        ),
                    ));
                }

                // Heartbeat every 15 s
                if now.duration_since(last_hb).as_secs() >= 15 {
                    return Some((
                        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from_static(
                            b": heartbeat\n\n",
                        )),
                        (
                            seq,
                            last_seq,
                            now,
                            last_mode,
                            mode_arc,
                            frozen_arc,
                            alert_seq_arc,
                            last_alert_arc,
                            last_alert_seq,
                        ),
                    ));
                }
            }
        },
    );

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

// ─── MJPEG live stream ────────────────────────────────────────────────────────

/// GET /api/stream — MJPEG live stream of the panel.
///
/// Standard Motion JPEG format: browsers and VLC can display it directly.
/// Stream a new JPEG whenever frame_seq changes, otherwise wait.
/// Sends at most ~4fps to limit CPU/bandwidth usage.
///
/// Usage in HTML: `<img src="/api/stream" style="width:480px">`
pub(crate) async fn api_mjpeg_stream(state: web::Data<DaemonState>) -> impl Responder {
    let frame_seq_lock = Arc::clone(&state.frame_seq);
    let last_frame_lock = Arc::clone(&state.last_frame);

    let stream = futures_util::stream::unfold(
        (frame_seq_lock, last_frame_lock, u64::MAX),
        |(seq_lock, frame_lock, last_seq)| async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let cur = *seq_lock.lock().unwrap();
                if cur != last_seq {
                    let maybe_jpeg = frame_lock.lock().unwrap().clone();
                    if let Some(jpeg) = maybe_jpeg {
                        let header = format!(
                            "--frame
Content-Type: image/jpeg
Content-Length: {}

",
                            jpeg.len()
                        );
                        let mut part = header.into_bytes();
                        part.extend_from_slice(&jpeg);
                        part.extend_from_slice(
                            b"
",
                        );
                        return Some((
                            Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(part)),
                            (seq_lock, frame_lock, cur),
                        ));
                    }
                }
            }
        },
    );

    HttpResponse::Ok()
        .content_type("multipart/x-mixed-replace; boundary=frame")
        .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

// ─── Brightness control ──────────────────────────────────────────────────────

/// GET /api/brightness — return current brightness as JSON.
pub(crate) async fn api_get_brightness(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct BrightnessResp {
        brightness: f32,
    }
    HttpResponse::Ok().json(BrightnessResp {
        brightness: *state.dim_factor.lock().unwrap(),
    })
}

#[derive(Deserialize)]
pub(crate) struct BrightnessRequest {
    /// Brightness fraction 0.0 (off) to 1.0 (full).  Also accepts 0–100 as a
    /// convenience — values > 1 are treated as a percentage and divided by 100.
    brightness: f32,
}

/// POST /api/brightness — set display brightness (0.0–1.0 or 0–100).
pub(crate) async fn api_set_brightness(
    state: web::Data<DaemonState>,
    body: web::Json<BrightnessRequest>,
) -> impl Responder {
    let raw = body.brightness;
    let factor = if raw > 1.0 {
        (raw / 100.0).clamp(0.0, 1.0)
    } else {
        raw.clamp(0.0, 1.0)
    };
    *state.dim_factor.lock().unwrap() = factor;
    log::info!("brightness set to {factor:.2}");
    persist_state(&state);
    HttpResponse::Ok().body(format!("{factor:.2}"))
}

// ─── Countdown cancel ─────────────────────────────────────────────────────────

/// DELETE /api/countdown — cancel the running countdown and return to the
/// previous live mode.
pub(crate) async fn api_cancel_countdown(state: web::Data<DaemonState>) -> impl Responder {
    let was_active = state.countdown.lock().unwrap().is_some();
    *state.countdown.lock().unwrap() = None;
    // Only revert if currently showing countdown
    if *state.mode.lock().unwrap() == DisplayMode::Countdown {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    if was_active {
        log::info!("countdown cancelled");
        HttpResponse::Ok().body("countdown cancelled")
    } else {
        HttpResponse::Ok().body("no active countdown")
    }
}

// ─── PNG screenshot ───────────────────────────────────────────────────────────

/// GET /api/screenshot.png — return the current panel frame losslessly as PNG.
///
/// Useful for embedding in dashboards that require lossless images.  Note that
/// this decodes and re-encodes the stored JPEG so quality may not exceed the
/// JPEG encoder's original quality.
pub(crate) async fn api_screenshot_png(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
        Some(jpeg) => match image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg) {
            Ok(img) => {
                let mut buf = Vec::new();
                match img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png) {
                    Ok(_) => HttpResponse::Ok()
                        .insert_header(("Cache-Control", "no-store"))
                        .insert_header(("Content-Disposition", "attachment; filename=panel.png"))
                        .content_type("image/png")
                        .body(buf),
                    Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
                }
            }
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        },
    }
}

// ─── GET /api/rotation ────────────────────────────────────────────────────────

/// GET /api/rotation — return the current auto-rotation configuration.
pub(crate) async fn api_rotation_get(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct RotationConfig {
        modes: Vec<String>,
        interval: u64,
        enabled: bool,
    }
    let modes: Vec<String> = state
        .rotation_modes
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.as_str().to_string())
        .collect();
    let interval = *state.rotation_interval.lock().unwrap();
    HttpResponse::Ok().json(RotationConfig {
        enabled: interval > 0 && !modes.is_empty(),
        modes,
        interval,
    })
}

// ─── Panel reconnect endpoint ────────────────────────────────────────────────

/// POST /api/panel/reconnect — trigger an immediate USB reconnect attempt.
///
/// This does not interrupt the display loop; it simply clears the consecutive_errors
/// count by signalling a reconnect intent through panel_connected=false so the
/// display loop's next iteration will attempt to reopen the device.
///
/// In headless mode this is a no-op.
pub(crate) async fn api_panel_reconnect(state: web::Data<DaemonState>) -> impl Responder {
    if state.headless {
        return HttpResponse::Ok().json(serde_json::json!({
            "ok": true,
            "message": "headless mode — no USB device to reconnect"
        }));
    }
    // Force panel_connected=false so the display loop retries on next tick
    *state.panel_connected.lock().unwrap() = false;
    push_history(
        &state,
        HistoryKind::System,
        "panel reconnect requested via API".to_string(),
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "reconnect attempt scheduled — check /api/panel/info in a few seconds"
    }))
}

// ─── USB device listing endpoint ─────────────────────────────────────────────

/// GET /api/usb/devices — list all USB devices visible to the system via rusb.
///
/// Returns a JSON array of `{ bus, device, vid, pid, manufacturer, product, serial }`.
/// Useful for diagnosing why the panel isn't found (wrong VID:PID, permissions, etc.).
pub(crate) async fn api_usb_devices() -> impl Responder {
    #[derive(serde::Serialize)]
    struct UsbDev {
        bus: u8,
        device: u8,
        vid: String,
        pid: String,
        manufacturer: Option<String>,
        product: Option<String>,
        serial: Option<String>,
    }

    let devices = tokio::task::spawn_blocking(|| -> Vec<UsbDev> {
        use rusb::UsbContext as _;
        let Ok(ctx) = rusb::Context::new() else {
            return Vec::new();
        };
        let Ok(devs) = ctx.devices() else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for dev in devs.iter() {
            let Ok(desc) = dev.device_descriptor() else {
                continue;
            };
            let bus = dev.bus_number();
            let device = dev.address();
            let vid = format!("{:04x}", desc.vendor_id());
            let pid = format!("{:04x}", desc.product_id());

            // Try to open and read string descriptors (may fail without permissions)
            // Try to open and read string descriptors (may fail without permissions)
            let (manufacturer, product, serial) = if let Ok(handle) = dev.open() {
                let m = handle.read_manufacturer_string_ascii(&desc).ok();
                let p = handle.read_product_string_ascii(&desc).ok();
                let s = handle.read_serial_number_string_ascii(&desc).ok();
                (m, p, s)
            } else {
                (None, None, None)
            };

            result.push(UsbDev {
                bus,
                device,
                vid,
                pid,
                manufacturer,
                product,
                serial,
            });
        }
        result
    })
    .await
    .unwrap_or_default();

    HttpResponse::Ok().json(serde_json::json!({
        "count": devices.len(),
        "devices": devices,
    }))
}

// ─── GET/POST /api/night ─────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub(crate) struct NightModeStatus {
    enabled: bool,
    start_hour: u8,
    end_hour: u8,
    mode: String,
    currently_active: bool,
}

pub(crate) async fn api_get_night_mode(state: web::Data<DaemonState>) -> impl Responder {
    let enabled = *state.night_mode_enabled.lock().unwrap();
    let start_hour = *state.night_mode_start_hour.lock().unwrap();
    let end_hour = *state.night_mode_end_hour.lock().unwrap();
    let mode = state.night_mode_name.lock().unwrap().clone();
    let current_h = chrono::Local::now().hour() as u8;
    let in_night = if start_hour > end_hour {
        current_h >= start_hour || current_h < end_hour
    } else {
        current_h >= start_hour && current_h < end_hour
    };
    HttpResponse::Ok().json(NightModeStatus {
        enabled,
        start_hour,
        end_hour,
        mode,
        currently_active: enabled && in_night,
    })
}

#[derive(serde::Deserialize)]
pub(crate) struct SetNightModeRequest {
    enabled: Option<bool>,
    start_hour: Option<u8>,
    end_hour: Option<u8>,
    mode: Option<String>,
}

pub(crate) async fn api_set_night_mode(
    state: web::Data<DaemonState>,
    body: web::Json<SetNightModeRequest>,
) -> impl Responder {
    if let Some(v) = body.enabled {
        *state.night_mode_enabled.lock().unwrap() = v;
    }
    if let Some(v) = body.start_hour {
        *state.night_mode_start_hour.lock().unwrap() = v.min(23);
    }
    if let Some(v) = body.end_hour {
        *state.night_mode_end_hour.lock().unwrap() = v.min(23);
    }
    if let Some(ref m) = body.mode {
        *state.night_mode_name.lock().unwrap() = m.clone();
    }
    push_history(
        &state,
        HistoryKind::System,
        "Night mode updated".to_string(),
    );
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── State reload ────────────────────────────────────────────────────────────

/// POST /api/state/reload — re-read the persisted daemon state from disk and
/// apply quality, rotate_deg, and brightness.  Mode is NOT changed (to avoid
/// interrupting an active session).
pub(crate) async fn api_state_reload(state: web::Data<DaemonState>) -> impl Responder {
    let ps = load_persisted_state();
    let mut changed: Vec<&str> = Vec::new();
    if let Some(q) = ps.quality {
        *state.quality.lock().unwrap() = q;
        changed.push("quality");
    }
    if let Some(r) = ps.rotate_deg {
        *state.rotate_deg.lock().unwrap() = r;
        changed.push("rotate_deg");
    }
    if let Some(b) = ps.brightness {
        *state.dim_factor.lock().unwrap() = b;
        changed.push("brightness");
    }
    if changed.is_empty() {
        HttpResponse::Ok().body("no persisted state found")
    } else {
        log::info!(
            "reloaded persisted state: {} field(s) applied",
            changed.len()
        );
        HttpResponse::Ok().body(format!("reloaded: {}", changed.join(", ")))
    }
}

// ─── Embedded web UI ─────────────────────────────────────────────────────────

/// Content security policy for the web UI page. Scripts may only come from
/// this server (plus WebAssembly compilation), so markup injected into the
/// page cannot run code; the page may not be framed; and the only other host
/// it may contact is wttr.in, which the weather card queries directly.
/// Inline styles stay allowed because the page uses `style` attributes
/// throughout.
pub(crate) const WEB_UI_CSP: &str = "default-src 'self'; \
    script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: blob:; \
    connect-src 'self' https://wttr.in; \
    object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// Starts the WebAssembly UI. A file of its own, rather than an inline
/// script, so the page's content security policy can forbid inline scripts.
pub(crate) const WEB_UI_BOOT_JS: &str = "import init from './ttlcd_web.js';\ninit();\n";

pub(crate) async fn web_ui_index() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Content-Security-Policy", WEB_UI_CSP))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .body(include_str!("../../web/index.html"))
}
pub(crate) async fn web_ui_boot() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/javascript")
        .body(WEB_UI_BOOT_JS)
}
pub(crate) async fn web_ui_css() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/css")
        .body(include_str!("../../web/app.css"))
}
pub(crate) async fn web_ui_js() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/javascript")
        .body(include_str!("../../web/pkg/ttlcd_web.js"))
}
/// The web UI itself: the `web/` crate compiled to WebAssembly by
/// `scripts/build-web.sh`. `ttlcd_web.js` is its generated loader.
pub(crate) async fn web_ui_wasm() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/wasm")
        .body(&include_bytes!("../../web/pkg/ttlcd_web_bg.wasm")[..])
}
pub(crate) async fn web_ui_manifest() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/manifest+json")
        .body(include_str!("../../web/manifest.json"))
}

// ─── Stopwatch API ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct StopwatchResponse {
    running: bool,
    elapsed_ms: u64,
    /// Accumulated milliseconds before the current running period.
    /// When running: current_elapsed = elapsed_ms + (now_ms - started_at_unix_ms)
    base_elapsed_ms: u64,
    /// Unix-millisecond timestamp when the stopwatch was last started (0 = not running).
    started_at_unix_ms: u64,
    laps: Vec<u64>,
    display: String,
    lap_count: usize,
}

/// GET /api/stopwatch — current stopwatch state.
pub(crate) async fn api_stopwatch_get(state: web::Data<DaemonState>) -> impl Responder {
    let sw = state.stopwatch.lock().unwrap();
    let elapsed = sw.current_elapsed_ms();
    let lap_count = sw.laps.len();
    let display = ttlcd::display::format_sw_time(elapsed);
    HttpResponse::Ok().json(StopwatchResponse {
        running: sw.running,
        elapsed_ms: elapsed,
        base_elapsed_ms: sw.elapsed_ms,
        started_at_unix_ms: sw.started_at_unix_ms,
        laps: sw.laps.clone(),
        display,
        lap_count,
    })
}

/// POST /api/stopwatch/start — start or resume the stopwatch.
pub(crate) async fn api_stopwatch_start(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    if !sw.running {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        sw.started_at_unix_ms = now_ms;
        sw.running = true;
    }
    drop(sw);
    // Switch to stopwatch mode
    *state.mode.lock().unwrap() = DisplayMode::Stopwatch;
    *state.prev_mode.lock().unwrap() = DisplayMode::Stopwatch;
    HttpResponse::Ok().body("started")
}

/// POST /api/stopwatch/stop — pause the stopwatch.
pub(crate) async fn api_stopwatch_stop(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    if sw.running {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        sw.elapsed_ms += now_ms.saturating_sub(sw.started_at_unix_ms);
        sw.running = false;
        sw.started_at_unix_ms = 0;
    }
    HttpResponse::Ok().body("stopped")
}

/// POST /api/stopwatch/lap — record a lap at the current elapsed time.
pub(crate) async fn api_stopwatch_lap(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    let elapsed = sw.current_elapsed_ms();
    if elapsed > 0 {
        sw.laps.push(elapsed);
        let n = sw.laps.len();
        drop(sw);
        HttpResponse::Ok().json(serde_json::json!({ "lap": n, "elapsed_ms": elapsed }))
    } else {
        HttpResponse::BadRequest().body("stopwatch not started")
    }
}

/// POST /api/stopwatch/reset — stop and zero the stopwatch.
pub(crate) async fn api_stopwatch_reset(state: web::Data<DaemonState>) -> impl Responder {
    let mut sw = state.stopwatch.lock().unwrap();
    *sw = StopwatchState::new();
    HttpResponse::Ok().body("reset")
}

// ─── Display freeze / unfreeze ────────────────────────────────────────────────

/// POST /api/display/freeze — stop the display from updating (last frame stays).
pub(crate) async fn api_display_freeze(state: web::Data<DaemonState>) -> impl Responder {
    *state.frozen.lock().unwrap() = true;
    HttpResponse::Ok().body("frozen")
}

/// POST /api/display/unfreeze — resume normal display updates.
pub(crate) async fn api_display_unfreeze(state: web::Data<DaemonState>) -> impl Responder {
    *state.frozen.lock().unwrap() = false;
    HttpResponse::Ok().body("unfrozen")
}

/// GET /api/display/frozen — returns {"frozen":true|false}.
pub(crate) async fn api_display_frozen(state: web::Data<DaemonState>) -> impl Responder {
    let frozen = *state.frozen.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({ "frozen": frozen }))
}

// ─── Alert thresholds ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SetThresholdsRequest {
    cpu_pct: Option<f32>,
    temp_c: Option<f32>,
    cooldown_secs: Option<u64>,
}

/// GET /api/alerts/thresholds — current threshold configuration.
pub(crate) async fn api_get_thresholds(state: web::Data<DaemonState>) -> impl Responder {
    let t = state.alert_thresholds.lock().unwrap().clone();
    HttpResponse::Ok().json(&t)
}

/// POST /api/alerts/thresholds — update one or more threshold values.
/// Any omitted fields keep their current value.
pub(crate) async fn api_set_thresholds(
    state: web::Data<DaemonState>,
    body: web::Json<SetThresholdsRequest>,
) -> impl Responder {
    let mut t = state.alert_thresholds.lock().unwrap();
    if let Some(v) = body.cpu_pct {
        t.cpu_pct = v.clamp(0.0, 100.0);
    }
    if let Some(v) = body.temp_c {
        t.temp_c = v.clamp(0.0, 200.0);
    }
    if let Some(v) = body.cooldown_secs {
        t.cooldown_secs = v;
    }
    HttpResponse::Ok().json(&*t)
}

// ─── Night-dim schedule ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SetDimScheduleRequest {
    start_hour: u8,
    end_hour: u8,
    #[serde(default = "default_dim_level")]
    dim_level: f32,
}
pub(crate) fn default_dim_level() -> f32 {
    0.2
}

/// GET /api/dim/schedule — current dim schedule (null if not set).
pub(crate) async fn api_get_dim_schedule(state: web::Data<DaemonState>) -> impl Responder {
    let s = state.dim_schedule.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "schedule": s }))
}

/// POST /api/dim/schedule — set a night-dim schedule.
/// Example: {"start_hour":22,"end_hour":7,"dim_level":0.15}
pub(crate) async fn api_set_dim_schedule(
    state: web::Data<DaemonState>,
    body: web::Json<SetDimScheduleRequest>,
) -> impl Responder {
    let sched = DimSchedule {
        start_hour: body.start_hour % 24,
        end_hour: body.end_hour % 24,
        dim_level: body.dim_level.clamp(0.0, 1.0),
    };
    *state.dim_schedule.lock().unwrap() = Some(sched.clone());
    log::info!(
        "dim schedule set: {}:00 → {}:00 @ {:.0}%",
        sched.start_hour,
        sched.end_hour,
        sched.dim_level * 100.0
    );
    HttpResponse::Ok().json(serde_json::json!({ "schedule": sched }))
}

/// DELETE /api/dim/schedule — remove the dim schedule.
pub(crate) async fn api_clear_dim_schedule(state: web::Data<DaemonState>) -> impl Responder {
    *state.dim_schedule.lock().unwrap() = None;
    HttpResponse::Ok().body("schedule cleared")
}

// ─── Screensaver API handlers ─────────────────────────────────────────────────

/// GET /api/screensaver — get current screensaver configuration and status.
///
/// Response:
/// ```json
/// {
///   "enabled": true,
///   "timeout_secs": 300,
///   "mode": "matrix_rain",
///   "active": false,
///   "seconds_since_activity": 42
/// }
/// ```
pub(crate) async fn api_get_screensaver(state: web::Data<DaemonState>) -> impl Responder {
    let timeout = *state.screensaver_timeout_secs.lock().unwrap();
    let mode = state.screensaver_mode_name.lock().unwrap().clone();
    let active = *state.screensaver_active.lock().unwrap();
    let last = *state.last_activity_secs.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let since_activity = now.saturating_sub(last);

    HttpResponse::Ok().json(serde_json::json!({
        "enabled": timeout > 0,
        "timeout_secs": timeout,
        "mode": mode,
        "active": active,
        "seconds_since_activity": since_activity,
    }))
}
