//! Endpoints that push one-off content: notifications, webhooks, banners, slideshows and demo mode.

use super::*;

// ─── Notification history ────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HistoryKind {
    Text,
    Alert,
    Image,
    System,
}

#[derive(Clone, Serialize)]
pub(crate) struct HistoryEntry {
    pub(crate) kind: HistoryKind,
    pub(crate) summary: String,
    /// Unix timestamp (seconds).
    pub(crate) timestamp: u64,
}

/// Push an entry to the ring-buffer (capped at 50 items).
pub(crate) fn push_history(state: &DaemonState, kind: HistoryKind, summary: String) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    // Fire desktop notification for Alert-kind events (if notify-send is available)
    if matches!(kind, HistoryKind::Alert) {
        let sum2 = summary.clone();
        thread::spawn(move || {
            let _ = std::process::Command::new("notify-send")
                // Options first, then `--`: the summary is caller-supplied text
                // and must not be read as an option.
                .args([
                    "--icon",
                    "dialog-warning",
                    "--expire-time",
                    "5000",
                    "--",
                    "ttlcd Alert",
                    &sum2,
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }

    let entry = HistoryEntry {
        kind: kind.clone(),
        summary: summary.clone(),
        timestamp: ts,
    };
    {
        let mut h = state.history.lock().unwrap();
        if h.len() >= 200 {
            h.pop_front();
        }
        h.push_back(entry);
    }
    // Publish alert events for the SSE stream
    if matches!(kind, HistoryKind::Alert) {
        let mut seq = state.alert_seq.lock().unwrap();
        *seq += 1;
        let new_seq = *seq;
        drop(seq);
        *state.last_alert_event.lock().unwrap() = Some((new_seq, summary.clone()));

        // Fire ntfy.sh notification (fire-and-forget)
        let ntfy_opt = state.ntfy_url.lock().unwrap().clone();
        if let Some(ntfy_url) = ntfy_opt {
            let body = summary.clone();
            tokio::spawn(async move {
                match reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                {
                    Ok(client) => {
                        match client
                            .post(&ntfy_url)
                            .header("Title", "ttlcd Alert")
                            .header("Priority", "high")
                            .header("Tags", "warning")
                            .body(body)
                            .send()
                            .await
                        {
                            Ok(r) => {
                                log::info!(
                                    "ntfy notification sent → {} ({})",
                                    ntfy_url,
                                    r.status()
                                );
                            }
                            Err(e) => log::warn!("ntfy notification failed: {e}"),
                        }
                    }
                    Err(e) => log::warn!("ntfy: could not build HTTP client: {e}"),
                }
            });
        }
    }
}

/// GET /api/history — return notifications.
///
/// Query params:
///   `limit=N`    — max entries (default 50, max 50)
///   `kind=alert|text|image|system` — filter by entry kind
///   `format=csv` — CSV output (columns: timestamp_unix,kind,summary)
pub(crate) async fn api_history(
    state: web::Data<DaemonState>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let limit: usize = query
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .min(200);
    let fmt = query
        .get("format")
        .map_or("json", std::string::String::as_str)
        .to_string();
    let kind_filter = query.get("kind").cloned();

    fn kind_str(k: &HistoryKind) -> &'static str {
        match k {
            HistoryKind::Alert => "alert",
            HistoryKind::Text => "text",
            HistoryKind::Image => "image",
            HistoryKind::System => "system",
        }
    }

    let h = state.history.lock().unwrap();
    let items: Vec<&HistoryEntry> = h
        .iter()
        .rev()
        .filter(|e| {
            kind_filter
                .as_ref()
                .is_none_or(|kf| kind_str(&e.kind) == kf.as_str())
        })
        .take(limit)
        .collect();

    if fmt == "csv" {
        let mut csv = String::from("timestamp_unix,kind,summary\n");
        for e in &items {
            let safe = e.summary.replace('"', "\"\"");
            csv.push_str(&format!(
                "{},{},\"{}\"\n",
                e.timestamp,
                kind_str(&e.kind),
                safe
            ));
        }
        return HttpResponse::Ok()
            .content_type("text/csv; charset=utf-8")
            .insert_header((
                "Content-Disposition",
                "attachment; filename=\"ttlcd-history.csv\"",
            ))
            .body(csv);
    }
    HttpResponse::Ok().json(items)
}
/// DELETE /api/history — clear the notification ring-buffer.
pub(crate) async fn api_history_clear(state: web::Data<DaemonState>) -> impl Responder {
    let mut h = state.history.lock().unwrap();
    let count = h.len();
    h.clear();
    log::info!("notification history cleared ({count} entries removed)");
    HttpResponse::Ok().body(format!("cleared {count} notification(s)"))
}

// ─── Clear all presets ────────────────────────────────────────────────────────

/// DELETE /api/presets — remove ALL saved presets (irreversible).
pub(crate) async fn api_presets_clear(state: web::Data<DaemonState>) -> impl Responder {
    let count = {
        let mut guard = state.presets.lock().unwrap();
        let n = guard.len();
        guard.clear();
        save_presets_to_disk(&guard);
        n
    };
    log::info!("all {count} preset(s) cleared");
    HttpResponse::Ok().body(format!("cleared {count} preset(s)"))
}

// ─── Slideshow ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SlideshowRequest {
    dir: String,
    #[serde(default = "default_ss_interval")]
    interval: u64,
}
pub(crate) fn default_ss_interval() -> u64 {
    5
}

pub(crate) async fn api_slideshow(
    state: web::Data<DaemonState>,
    body: web::Json<SlideshowRequest>,
) -> impl Responder {
    let req = body.into_inner();
    let dir = PathBuf::from(&req.dir);
    if !dir.is_dir() {
        return HttpResponse::BadRequest().body(format!("'{}' is not a directory", req.dir));
    }
    let paths = collect_image_paths(&dir);
    if paths.is_empty() {
        return HttpResponse::BadRequest().body(format!("no supported images in '{}'", req.dir));
    }
    let count = paths.len();
    let interval = req.interval.max(1);
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.slideshow_paths.lock().unwrap() = paths;
    *state.slideshow_idx.lock().unwrap() = 0;
    *state.slideshow_interval.lock().unwrap() = interval;
    *state.mode.lock().unwrap() = DisplayMode::Slideshow;
    HttpResponse::Ok().body(format!(
        "slideshow started: {count} images, {interval}s/frame"
    ))
}

// ─── Demo mode (cycle all screensaver modes) ──────────────────────────────────

/// POST /api/demo — start auto-rotation through all "fun" screensaver modes.
/// Optional body: { "interval": 8 }   (default 8 seconds per mode)
/// DELETE /api/demo — stop demo (clears rotation).
pub(crate) async fn api_start_demo(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let interval = body
        .get("interval")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(8)
        .max(2);

    // Collect all "fun" screensaver modes
    let screensaver_names = [
        "game_of_life",
        "matrix_rain",
        "plasma",
        "fireworks",
        "starfield",
        "bounce_ball",
        "snake",
        "cube_3d",
        "dvd_bounce",
        "lissajous",
        "pong",
        "breakout",
        "radar",
        "oscilloscope",
        "doom_fire",
        "boids",
        "tunnel",
        "aurora",
        "metaballs",
        "sand",
        "gravity",
        "voronoi",
        "reaction_diffusion",
        "snowfall",
        "conway_colors",
        "julia_set",
        "confetti",
        "neon_grid",
        "ripple",
        "hypno",
        "lorenz",
        "mandelbrot",
        "kaleidoscope",
        "dna_helix",
        "rain_glass",
        "perlin_terrain",
        "fire_text",
        "disco_floor",
        "city_skyline",
        "neon_sign",
        "gradient_clock",
        "particle_clock",
        "pulse",
        "galaxy",
        "ocean_waves",
        "warp_speed",
        "morphing_blobs",
        "clock_matrix",
        "sierpinski",
        "lava_lamp",
        "pendulum_wave",
        "fiber_optic",
        "network_globe",
        "plasma_ball",
        "moss_growth",
        "asteroid_field",
        "solar_system",
        "lightning_storm",
        "spirograph",
        "fractal_tree",
        "bubble_rise",
        "heartbeat",
        "waveform_bars",
        "lorenz",
        "mandelbrot",
        "aurora",
        "voronoi",
    ];
    let modes: Vec<DisplayMode> = screensaver_names
        .iter()
        .filter_map(|&name| DisplayMode::from_str_name(name))
        .collect();
    let count = modes.len();
    *state.rotation_modes.lock().unwrap() = modes;
    *state.rotation_interval.lock().unwrap() = interval;
    push_history(
        &state,
        HistoryKind::System,
        format!("demo started: {count} modes, {interval}s"),
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": format!("demo rotation started: {count} modes, {interval}s each"),
        "mode_count": count,
        "interval": interval,
    }))
}

pub(crate) async fn api_stop_demo(state: web::Data<DaemonState>) -> impl Responder {
    *state.rotation_modes.lock().unwrap() = vec![];
    *state.rotation_interval.lock().unwrap() = 0;
    push_history(&state, HistoryKind::System, "demo stopped".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "message": "demo stopped" }))
}

pub(crate) async fn api_rotation(
    state: web::Data<DaemonState>,
    body: web::Json<RotationRequest>,
) -> impl Responder {
    let req = body.into_inner();
    let modes: Vec<DisplayMode> = req
        .modes
        .iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .filter(|m| !m.is_transient())
        .collect();

    if req.interval > 0 && modes.is_empty() {
        return HttpResponse::BadRequest().body("no valid (non-transient) modes specified");
    }

    *state.rotation_modes.lock().unwrap() = modes;
    *state.rotation_interval.lock().unwrap() = req.interval;

    if req.interval == 0 {
        HttpResponse::Ok().body("rotation stopped")
    } else {
        let count = state.rotation_modes.lock().unwrap().len();
        HttpResponse::Ok().body(format!(
            "rotation started: {} modes, {}s interval",
            count, req.interval
        ))
    }
}

// ─── /api/notify — simple single-call push ────────────────────────────────────

/// POST /api/notify — convenience endpoint for quick panel notifications.
///
/// Simpler than `/api/text`: accepts a flat JSON with optional fields.
/// Designed for one-liners in shell scripts and CI pipelines.
///
/// ```json
/// { "title": "Build OK", "body": "All 42 tests passed", "color": "green", "progress": 1.0 }
/// ```
#[derive(Deserialize)]
pub(crate) struct NotifyRequest {
    #[serde(default)]
    title: Option<String>,
    /// Single-line body text (shown as `lines[0]`).
    #[serde(default)]
    body: Option<String>,
    #[serde(default = "default_notify_color")]
    color: ttlcd::display::NamedColor,
    #[serde(default)]
    progress: Option<f32>,
    #[serde(default = "default_true")]
    show_time: bool,
}
pub(crate) fn default_notify_color() -> ttlcd::display::NamedColor {
    ttlcd::display::NamedColor::Blue
}
pub(crate) fn default_true() -> bool {
    true
}

pub(crate) async fn api_notify(
    state: web::Data<DaemonState>,
    body: web::Json<NotifyRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.title.is_none() && req.body.is_none() {
        return HttpResponse::BadRequest().body("supply at least 'title' or 'body'");
    }
    let summary = req
        .title
        .clone()
        .unwrap_or_else(|| req.body.clone().unwrap_or_default());
    let payload = CustomTextPayload {
        title: req.title,
        lines: req.body.into_iter().collect(),
        progress: req.progress,
        color: req.color,
        show_time: req.show_time,
        ..Default::default()
    };
    push_history(&state, HistoryKind::Text, summary);
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap() = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

// ─── /api/webhook — generic CI/CD webhook ────────────────────────────────────

/// POST /api/webhook — accept JSON from CI/CD systems and render a panel alert.
///
/// Understands:
///   • GitHub Actions-style: `{ "action": "...", "workflow_run": { "conclusion": "success|failure", "name": "..." } }`
///   • Simple flat: `{ "event": "...", "status": "success|failure|...", "message": "..." }`
///   • Anything else: renders a generic "Webhook" notification.
///
/// Query parameter `?secret=TOKEN` is compared against the `TTLCD_WEBHOOK_SECRET`
/// environment variable when that variable is set.
pub(crate) async fn api_webhook(
    state: web::Data<DaemonState>,
    query: web::Query<std::collections::HashMap<String, String>>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    // Optional shared-secret check
    if let Ok(expected) = std::env::var("TTLCD_WEBHOOK_SECRET") {
        let provided = query.get("secret").map_or("", String::as_str);
        if provided != expected {
            return HttpResponse::Unauthorized().body("invalid webhook secret");
        }
    }

    let v = body.into_inner();

    // ── Outbound forwarding (fire-and-forget) ────────────────────────────
    if let Some(fwd_url) = state.webhook_forward_url.lock().unwrap().clone() {
        let fwd_body = v.clone();
        tokio::spawn(async move {
            match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
            {
                Ok(client) => match client.post(&fwd_url).json(&fwd_body).send().await {
                    Ok(resp) => log::info!(
                        "webhook forwarded to {} — status {}",
                        fwd_url,
                        resp.status()
                    ),
                    Err(e) => log::warn!("webhook forward to {fwd_url} failed: {e}"),
                },
                Err(e) => log::warn!("webhook forward: could not build HTTP client: {e}"),
            }
        });
    }

    // ── GitHub Actions workflow_run ──────────────────────────────────────
    if let Some(wr) = v.get("workflow_run") {
        let conclusion = wr
            .get("conclusion")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");
        let name = wr
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Workflow");
        let (color, emoji) = match conclusion {
            "success" => (ttlcd::display::NamedColor::Green, "✓"),
            "failure" => (ttlcd::display::NamedColor::Red, "✗"),
            "cancelled" => (ttlcd::display::NamedColor::Yellow, "⊘"),
            _ => (ttlcd::display::NamedColor::Blue, "·"),
        };
        let payload = CustomTextPayload {
            title: Some(format!(
                "{emoji} {}",
                name.chars().take(18).collect::<String>()
            )),
            subtitle: Some(conclusion.to_uppercase()),
            progress: if conclusion == "success" {
                Some(1.0)
            } else if conclusion == "failure" {
                Some(0.0)
            } else {
                None
            },
            color,
            show_time: true,
            ..Default::default()
        };
        let summary = format!("GitHub: {name} — {conclusion}");
        push_history(&state, HistoryKind::Alert, summary);
        {
            let cur = state.mode.lock().unwrap().clone();
            if !cur.is_transient() {
                *state.prev_mode.lock().unwrap() = cur;
            }
        }
        *state.pending_text.lock().unwrap() = Some(payload);
        *state.mode.lock().unwrap() = DisplayMode::CustomText;
        return HttpResponse::Ok().body("ok");
    }

    // ── Simple flat { "status": "...", "message": "..." } ────────────────
    let status = v
        .get("status")
        .or_else(|| v.get("event"))
        .and_then(|s| s.as_str())
        .unwrap_or("info");
    let message = v
        .get("message")
        .or_else(|| v.get("text"))
        .and_then(|m| m.as_str())
        .unwrap_or("Webhook received");
    let (color, title) = match status.to_lowercase().as_str() {
        "success" | "passed" | "ok" => (ttlcd::display::NamedColor::Green, "✓ SUCCESS"),
        "failure" | "failed" | "error" => (ttlcd::display::NamedColor::Red, "✗ FAILED"),
        "warning" | "warn" => (ttlcd::display::NamedColor::Yellow, "⚠ WARNING"),
        "running" | "pending" => (ttlcd::display::NamedColor::Blue, "⟳ RUNNING"),
        _ => (ttlcd::display::NamedColor::Teal, "· WEBHOOK"),
    };
    let payload = CustomTextPayload {
        title: Some(title.into()),
        subtitle: Some(message.chars().take(38).collect()),
        color,
        show_time: true,
        ..Default::default()
    };
    push_history(&state, HistoryKind::Alert, format!("{title} — {message}"));
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_text.lock().unwrap() = Some(payload);
    *state.mode.lock().unwrap() = DisplayMode::CustomText;
    HttpResponse::Ok().body("ok")
}

// ─── Scheduled notifications ─────────────────────────────────────────────────

/// POST /api/schedule — schedule a panel notification for a future time.
///
/// The daemon spawns a background task that sleeps until the requested time,
/// then pushes the notification exactly like `POST /api/text`.
///
/// Body (JSON):
/// ```json
/// { "at": "ISO8601 datetime",   // e.g. "2026-05-13T22:00:00Z"
///   "in_secs": 300,             // alternative: fire after this many seconds
///   "payload": { ... },         // CustomTextPayload fields
///   "label": "human label"      // optional; used in log output
/// }
/// ```
/// Supply exactly one of `at` or `in_secs`.
#[derive(Deserialize)]
pub(crate) struct ScheduleRequest {
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    in_secs: Option<u64>,
    payload: CustomTextPayload,
    #[serde(default)]
    label: Option<String>,
}

pub(crate) async fn api_schedule(
    state: web::Data<DaemonState>,
    body: web::Json<ScheduleRequest>,
) -> impl Responder {
    let req = body.into_inner();

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let delay_secs: u64 = if let Some(ref at_str) = req.at {
        match chrono::DateTime::parse_from_rfc3339(at_str) {
            Ok(dt) => {
                let target = dt.timestamp() as u64;
                if target <= now_secs {
                    return HttpResponse::BadRequest().body("'at' must be in the future");
                }
                target - now_secs
            }
            Err(e) => return HttpResponse::BadRequest().body(format!("invalid datetime: {e}")),
        }
    } else if let Some(s) = req.in_secs {
        if s == 0 {
            return HttpResponse::BadRequest().body("in_secs must be > 0");
        }
        s
    } else {
        return HttpResponse::BadRequest().body("supply 'at' (ISO-8601) or 'in_secs'");
    };

    let label = req.label.unwrap_or_else(|| {
        req.payload
            .title
            .clone()
            .unwrap_or_else(|| "(no title)".into())
    });
    let fire_at_str = if delay_secs >= 60 {
        format!("{:.0}m {:.0}s", delay_secs / 60, delay_secs % 60)
    } else {
        format!("{delay_secs}s")
    };

    // Increment pending counter
    *state.pending_scheduled.lock().unwrap() += 1;
    let counter = Arc::clone(&state.pending_scheduled);
    let payload_clone = req.payload;
    let state_clone = state.get_ref().clone();
    let label_clone = label.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;

        log::info!("scheduled notification fired: {label_clone}");

        if let Some(dur) = payload_clone.duration_secs {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            *state_clone.auto_clear_at.lock().unwrap() = Some(now + dur);
        } else {
            *state_clone.auto_clear_at.lock().unwrap() = None;
        }

        let summary = payload_clone
            .title
            .clone()
            .unwrap_or_else(|| label_clone.clone());
        push_history(&state_clone, HistoryKind::Text, summary);
        {
            let cur = state_clone.mode.lock().unwrap().clone();
            if !cur.is_transient() {
                *state_clone.prev_mode.lock().unwrap() = cur;
            }
        }
        *state_clone.pending_text.lock().unwrap() = Some(payload_clone);
        *state_clone.mode.lock().unwrap() = DisplayMode::CustomText;

        // Decrement pending counter
        let mut c = counter.lock().unwrap();
        *c = c.saturating_sub(1);
    });

    log::info!("scheduled notification in {delay_secs}s: {label}");
    HttpResponse::Ok().body(format!("scheduled in {fire_at_str}: \"{label}\""))
}

// ─── Slideshow navigation ─────────────────────────────────────────────────────

/// POST /api/slideshow/next — advance the slideshow to the next image immediately.
pub(crate) async fn api_slideshow_next(state: web::Data<DaemonState>) -> impl Responder {
    if *state.mode.lock().unwrap() != DisplayMode::Slideshow {
        return HttpResponse::Conflict().body("not in slideshow mode");
    }
    let count = state.slideshow_paths.lock().unwrap().len();
    if count == 0 {
        return HttpResponse::BadRequest().body("no slideshow loaded");
    }
    {
        let mut idx = state.slideshow_idx.lock().unwrap();
        *idx = (*idx + 1) % count;
    }
    HttpResponse::Ok().body(format!(
        "advanced to image {}",
        *state.slideshow_idx.lock().unwrap()
    ))
}

/// POST /api/slideshow/prev — go back to the previous slideshow image.
pub(crate) async fn api_slideshow_prev(state: web::Data<DaemonState>) -> impl Responder {
    if *state.mode.lock().unwrap() != DisplayMode::Slideshow {
        return HttpResponse::Conflict().body("not in slideshow mode");
    }
    let count = state.slideshow_paths.lock().unwrap().len();
    if count == 0 {
        return HttpResponse::BadRequest().body("no slideshow loaded");
    }
    {
        let mut idx = state.slideshow_idx.lock().unwrap();
        *idx = idx.checked_sub(1).unwrap_or(count - 1);
    }
    HttpResponse::Ok().body(format!(
        "went back to image {}",
        *state.slideshow_idx.lock().unwrap()
    ))
}

// ─── Test pattern ─────────────────────────────────────────────────────────────

/// POST /api/test — display a SMPTE-style test card, then auto-return.
///
/// Optional body: `{ "duration_secs": 5 }` (default: 5 seconds)
pub(crate) async fn api_test_pattern(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let dur = body
        .get("duration_secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 60);

    let quality = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();

    let img = render_test_card();
    let img = apply_rotation(img, rotate_deg);
    let jpeg = match encode_for_panel(&img, quality) {
        Ok(j) => j,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };

    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;

    // Schedule auto-return
    let state_clone = state.get_ref().clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(dur)).await;
        let prev = state_clone.prev_mode.lock().unwrap().clone();
        *state_clone.mode.lock().unwrap() = prev;
        *state_clone.pending_jpeg.lock().unwrap() = None;
        log::info!("test pattern: auto-returned after {dur}s");
    });

    HttpResponse::Ok().body(format!("test pattern: showing for {dur}s"))
}

// ─── Slideshow image list + shuffle ──────────────────────────────────────────

/// GET /api/slideshow/images — list the paths currently loaded in the slideshow.
pub(crate) async fn api_slideshow_images(state: web::Data<DaemonState>) -> impl Responder {
    let paths = state.slideshow_paths.lock().unwrap();
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    #[derive(Serialize)]
    struct SlideshowImageList {
        count: usize,
        current: usize,
        images: Vec<String>,
    }
    HttpResponse::Ok().json(SlideshowImageList {
        count: names.len(),
        current: *state.slideshow_idx.lock().unwrap(),
        images: names,
    })
}

/// POST /api/slideshow/shuffle — randomise the slideshow order.
pub(crate) async fn api_slideshow_shuffle(state: web::Data<DaemonState>) -> impl Responder {
    let mut paths = state.slideshow_paths.lock().unwrap();
    if paths.is_empty() {
        return HttpResponse::BadRequest().body("no slideshow loaded");
    }
    // Fisher-Yates shuffle using SystemTime as entropy
    let n = paths.len();
    for i in (1..n).rev() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i, |d| d.subsec_nanos() as usize);
        let j = ts % (i + 1);
        paths.swap(i, j);
    }
    *state.slideshow_idx.lock().unwrap() = 0;
    HttpResponse::Ok().body(format!("shuffled {n} images"))
}

// ─── Preset export / import ──────────────────────────────────────────────────

/// GET /api/presets/export — download all presets as a JSON file.
pub(crate) async fn api_presets_export(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.presets.lock().unwrap();
    match serde_json::to_string_pretty(&*guard) {
        Ok(json) => HttpResponse::Ok()
            .content_type("application/json")
            .insert_header((
                "Content-Disposition",
                "attachment; filename=ttlcd-presets.json",
            ))
            .body(json),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

/// POST /api/presets/import — import presets from a JSON body.
///
/// The body must be a JSON object mapping preset names to CustomTextPayload objects.
/// Existing presets with the same name are overwritten.
pub(crate) async fn api_presets_import(
    state: web::Data<DaemonState>,
    body: web::Json<HashMap<String, CustomTextPayload>>,
) -> impl Responder {
    let incoming = body.into_inner();
    let count = incoming.len();
    if count == 0 {
        return HttpResponse::BadRequest().body("no presets in import body");
    }
    {
        let mut guard = state.presets.lock().unwrap();
        for (name, payload) in incoming {
            guard.insert(name, payload);
        }
        save_presets_to_disk(&guard);
    }
    log::info!("imported {count} preset(s)");
    HttpResponse::Ok().body(format!("imported {count} preset(s)"))
}

// ─── POST /api/flash ─────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct FlashRequest {
    text: String,
    #[serde(default = "default_flash_duration")]
    duration: u64,
    #[serde(default = "default_flash_color")]
    color: String,
}
pub(crate) fn default_flash_duration() -> u64 {
    10
}
pub(crate) fn default_flash_color() -> String {
    "#89b4fa".into()
}

pub(crate) async fn api_flash_message(
    state: web::Data<DaemonState>,
    body: web::Json<FlashRequest>,
) -> impl Responder {
    let expires_at =
        std::time::Instant::now() + std::time::Duration::from_secs(body.duration.clamp(1, 300));
    let fm = FlashMessageState {
        text: body.text.clone(),
        color: body.color.clone(),
        expires_at,
    };
    *state.flash_message.lock().unwrap() = Some(fm);
    push_history(
        &state,
        HistoryKind::System,
        format!("Flash message: \"{}\": {}s", body.text, body.duration),
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "text": body.text,
        "duration": body.duration,
        "color": body.color,
    }))
}

// ─── Banner endpoint ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct BannerRequest {
    /// Up to 3 lines of text to display large and centred.
    lines: Vec<String>,
    /// Optional subtitle label at the bottom.
    #[serde(default)]
    label: String,
    /// Accent colour name (blue/green/yellow/red/teal/mauve/peach/white).
    #[serde(default = "default_banner_color")]
    color: String,
    /// Show clock in the top-right corner (default true).
    #[serde(default = "default_true")]
    show_time: bool,
    /// Auto-clear after N seconds (0 = keep until cleared).
    #[serde(default)]
    duration: u64,
}

pub(crate) fn default_banner_color() -> String {
    "blue".to_string()
}

/// POST /api/banner — display up to 3 lines of large centred text on the panel.
///
/// ```json
/// {"lines": ["99.8%", "Uptime"], "label": "SLA", "color": "green"}
/// ```
pub(crate) async fn api_banner(
    state: web::Data<DaemonState>,
    body: web::Json<BannerRequest>,
) -> impl Responder {
    use ttlcd::display::NamedColor;
    let quality = *state.quality.lock().unwrap();
    let rotate = *state.rotate_deg.lock().unwrap();
    let brightness = *state.dim_factor.lock().unwrap();
    let named = NamedColor::from_str_loose(&body.color);
    let rgb = named.to_rgb_pub();
    let refs: Vec<&str> = body.lines.iter().map(String::as_str).take(3).collect();
    let img = render_big_text_banner(&refs, &body.label, rgb, body.show_time);
    let img = apply_rotation(img, rotate);
    let img = apply_brightness(img, brightness);
    let jpeg = match encode_for_panel(&img, quality) {
        Ok(j) => j,
        Err(e) => return HttpResponse::InternalServerError().body(format!("encode error: {e}")),
    };
    let jpeg = store_frame(&state, jpeg.clone());
    *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    if body.duration > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        *state.auto_clear_at.lock().unwrap() = Some(now + body.duration);
    }
    push_history(
        &state,
        HistoryKind::Text,
        format!(
            "banner: {}",
            body.lines.first().map_or("", std::string::String::as_str)
        ),
    );
    HttpResponse::Ok().body("ok")
}

// ─── Gradient test pattern ────────────────────────────────────────────────────

pub(crate) fn test_pattern() -> DynamicImage {
    let mut img = image::RgbImage::new(PANEL_WIDTH, PANEL_HEIGHT);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let r = (x * 255 / (PANEL_WIDTH - 1)) as u8;
        let g = (y * 255 / (PANEL_HEIGHT - 1)) as u8;
        let b = (((x + y) * 255) / (PANEL_WIDTH + PANEL_HEIGHT - 2)) as u8;
        let on_diag = x == y || x + y == PANEL_WIDTH - 1;
        let on_edge = x < 4 || y < 4 || x >= PANEL_WIDTH - 4 || y >= PANEL_HEIGHT - 4;
        *px = if on_diag || on_edge {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([r, g, b])
        };
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Webhook configuration ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct WebhookConfigRequest {
    url: Option<String>,
}

/// GET /api/webhook/config — return the current webhook forward URL (if any).
pub(crate) async fn api_get_webhook_config(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.webhook_forward_url.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "url": url,
        "configured": url.is_some(),
    }))
}

/// POST /api/webhook/config — set the webhook forward URL.
///
/// Body: `{"url": "https://hooks.example.com/..."}`
///
/// When set, any event received at `/api/webhook` (CI/CD pushes, alerts) is
/// also forwarded to this URL as a POST with the same JSON body.
pub(crate) async fn api_set_webhook_config(
    state: web::Data<DaemonState>,
    body: web::Json<WebhookConfigRequest>,
) -> impl Responder {
    let url = match &body.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return HttpResponse::BadRequest().body("url is required and must not be empty"),
    };
    *state.webhook_forward_url.lock().unwrap() = Some(url.clone());
    log::info!("webhook forward URL set to: {url}");
    push_history(
        &state,
        HistoryKind::System,
        format!("Webhook forward URL configured: {url}"),
    );
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "url": url }))
}

/// DELETE /api/webhook/config — clear the webhook forward URL.
pub(crate) async fn api_clear_webhook_config(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.webhook_forward_url.lock().unwrap().take().is_some();
    if was_set {
        log::info!("webhook forward URL cleared");
        push_history(
            &state,
            HistoryKind::System,
            "Webhook forward URL cleared".to_string(),
        );
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "cleared": was_set }))
}
