//! Endpoints for the feature cards: metrics, profiles, weather, notes, QR, Pomodoro, world clock, ticker, progress, pet cat, settings backup and batch.

use super::*;

// ─── Custom metrics dashboard ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct PushMetricsRequest {
    /// Optional dashboard title (shown at top of panel).
    #[serde(default)]
    title: String,
    /// Key/value metrics to display (up to 8 shown; extras ignored).
    metrics: Vec<Metric>,
    /// If true, switch the display to Metrics mode immediately.
    #[serde(default = "default_true_metrics")]
    show: bool,
}
pub(crate) fn default_true_metrics() -> bool {
    true
}

/// POST /api/metrics — push new metric values and optionally switch to Metrics mode.
///
/// Example body:
/// ```json
/// { "title": "CI Dashboard",
///   "metrics": [
///     { "label": "Build", "value": "OK",     "color": "green" },
///     { "label": "Tests", "value": "1234",   "color": "blue"  },
///     { "label": "Cover", "value": "92.3%",  "color": "teal"  }
///   ]
/// }
/// ```
pub(crate) async fn api_push_metrics(
    state: web::Data<DaemonState>,
    body: web::Json<PushMetricsRequest>,
) -> impl Responder {
    let req = body.into_inner();
    *state.metrics_title.lock().unwrap() = req.title;
    *state.metrics_data.lock().unwrap() = req.metrics;
    if req.show {
        let prev = state.mode.lock().unwrap().clone();
        if !prev.is_transient() {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::Metrics;
    }
    HttpResponse::Ok().json(serde_json::json!({
        "count": state.metrics_data.lock().unwrap().len()
    }))
}

/// GET /api/metrics — return current metric data.
pub(crate) async fn api_get_metrics(state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "title":   *state.metrics_title.lock().unwrap(),
        "metrics": *state.metrics_data.lock().unwrap(),
    }))
}

/// DELETE /api/metrics — clear all metric data.
pub(crate) async fn api_clear_metrics(state: web::Data<DaemonState>) -> impl Responder {
    state.metrics_title.lock().unwrap().clear();
    state.metrics_data.lock().unwrap().clear();
    HttpResponse::Ok().body("cleared")
}

// ─── HTTP JSON Metrics Source API ────────────────────────────────────────────

/// GET /api/metrics/source — return current metrics source configuration.
pub(crate) async fn api_get_metrics_source(state: web::Data<DaemonState>) -> impl Responder {
    let src = state.metrics_source.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "source": src }))
}

/// POST /api/metrics/source — configure an HTTP JSON metrics source and enable auto-fetch.
///
/// Body: `{"url":"https://…","title":"My Server","interval_secs":30,
///        "fields":[{"label":"CPU","path":"cpu_pct","format":"{:.1}%","color":"green"}]}`
pub(crate) async fn api_set_metrics_source(
    state: web::Data<DaemonState>,
    body: web::Json<MetricsSource>,
) -> impl Responder {
    let src = body.into_inner();
    if src.url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if src.fields.is_empty() {
        return HttpResponse::BadRequest().body("fields must not be empty");
    }
    let interval = src.interval_secs.max(5);
    let src_stored = MetricsSource {
        interval_secs: interval,
        ..src.clone()
    };
    log::info!(
        "metrics_source configured: url='{}' interval={}s fields={}",
        src.url,
        interval,
        src.fields.len()
    );
    *state.metrics_source.lock().unwrap() = Some(src_stored);
    save_new_settings(&state);
    HttpResponse::Ok()
        .json(serde_json::json!({ "ok": true, "url": src.url, "interval_secs": interval }))
}

/// DELETE /api/metrics/source — remove the metrics source (auto-fetch stops).
pub(crate) async fn api_clear_metrics_source(state: web::Data<DaemonState>) -> impl Responder {
    *state.metrics_source.lock().unwrap() = None;
    save_new_settings(&state);
    log::info!("metrics_source cleared");
    HttpResponse::Ok().body("metrics source cleared")
}

/// POST /api/metrics/source/fetch — trigger an immediate fetch from the configured source.
pub(crate) async fn api_fetch_metrics_source_now(state: web::Data<DaemonState>) -> impl Responder {
    let src = state.metrics_source.lock().unwrap().clone();
    match src {
        None => HttpResponse::BadRequest().body("no metrics source configured"),
        Some(s) => match fetch_metrics_source(&state, &s).await {
            Ok(()) => {
                // Optionally switch to metrics mode
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() {
                    *state.prev_mode.lock().unwrap() = cur;
                }
                *state.mode.lock().unwrap() = DisplayMode::Metrics;
                HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": state.metrics_data.lock().unwrap().len() }))
            }
            Err(e) => HttpResponse::BadGateway().body(format!("fetch failed: {e}")),
        },
    }
}

// ─── Display profiles ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DisplayProfile {
    name: String,
    mode: String,
    quality: u8,
    rotate_deg: u16,
    brightness: f32,
    #[serde(default)]
    rotation_modes: Vec<String>,
    #[serde(default)]
    rotation_interval: u64,
}

pub(crate) fn profiles_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("profiles.json"))
}

pub(crate) fn load_profiles() -> Vec<DisplayProfile> {
    (|| -> Option<Vec<DisplayProfile>> {
        let path = profiles_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_profiles(profiles: &[DisplayProfile]) {
    let Some(path) = profiles_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(profiles) {
        let _ = std::fs::write(&path, text);
    }
}

/// GET /api/profiles — list all saved display profiles.
pub(crate) async fn api_profiles_list(_state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(load_profiles())
}

/// POST /api/profiles — save current daemon state as a named profile.
/// Body: `{"name":"my-profile"}`
#[derive(Deserialize)]
pub(crate) struct SaveProfileRequest {
    name: String,
}

pub(crate) async fn api_profiles_save(
    state: web::Data<DaemonState>,
    body: web::Json<SaveProfileRequest>,
) -> impl Responder {
    let profile = DisplayProfile {
        name: body.name.clone(),
        mode: state.mode.lock().unwrap().as_str().to_string(),
        quality: *state.quality.lock().unwrap(),
        rotate_deg: *state.rotate_deg.lock().unwrap(),
        brightness: *state.dim_factor.lock().unwrap(),
        rotation_modes: state
            .rotation_modes
            .lock()
            .unwrap()
            .iter()
            .map(|m| m.as_str().to_string())
            .collect(),
        rotation_interval: *state.rotation_interval.lock().unwrap(),
    };
    let mut profiles = load_profiles();
    // Replace if name already exists, else append
    if let Some(existing) = profiles.iter_mut().find(|p| p.name == body.name) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }
    save_profiles(&profiles);
    log::info!("profile '{}' saved", body.name);
    HttpResponse::Ok().json(&profile)
}

/// POST /api/profiles/{name}/apply — load and apply a saved profile.
pub(crate) async fn api_profiles_apply(
    state: web::Data<DaemonState>,
    name: web::Path<String>,
) -> impl Responder {
    let profiles = load_profiles();
    let Some(profile) = profiles.iter().find(|p| p.name == name.as_str()) else {
        return HttpResponse::NotFound().body(format!("profile '{name}' not found"));
    };
    // Apply settings
    if let Some(mode) = DisplayMode::from_str_name(&profile.mode) {
        *state.mode.lock().unwrap() = mode.clone();
        *state.prev_mode.lock().unwrap() = mode;
    }
    *state.quality.lock().unwrap() = profile.quality;
    *state.rotate_deg.lock().unwrap() = profile.rotate_deg;
    *state.dim_factor.lock().unwrap() = profile.brightness;
    if !profile.rotation_modes.is_empty() {
        let modes: Vec<DisplayMode> = profile
            .rotation_modes
            .iter()
            .filter_map(|s| DisplayMode::from_str_name(s))
            .collect();
        *state.rotation_modes.lock().unwrap() = modes;
        *state.rotation_interval.lock().unwrap() = profile.rotation_interval;
    }
    persist_state(&state);
    log::info!("profile '{name}' applied");
    HttpResponse::Ok().json(profile)
}

/// DELETE /api/profiles/{name} — delete a saved profile.
pub(crate) async fn api_profiles_delete(
    _state: web::Data<DaemonState>,
    name: web::Path<String>,
) -> impl Responder {
    let mut profiles = load_profiles();
    let before = profiles.len();
    profiles.retain(|p| p.name != name.as_str());
    if profiles.len() < before {
        save_profiles(&profiles);
        HttpResponse::Ok().body(format!("deleted '{name}'"))
    } else {
        HttpResponse::NotFound().body(format!("profile '{name}' not found"))
    }
}

// ─── Weather API ──────────────────────────────────────────────────────────────

/// GET /api/weather — return the currently cached weather data.
pub(crate) async fn api_get_weather(state: web::Data<DaemonState>) -> impl Responder {
    let wd = state.weather_data.lock().unwrap().clone();
    HttpResponse::Ok().json(wd)
}

/// POST /api/weather — push new weather data and optionally switch to Weather mode.
///
/// Body follows the [`WeatherData`] schema.  An extra `show` boolean field (default true)
/// controls whether the display switches to weather mode immediately.
pub(crate) async fn api_update_weather(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let show = body
        .get("show")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    // Deserialise into WeatherData (ignore unknown fields via Value → serde)
    let mut wd: WeatherData = match serde_json::from_value(body.into_inner()) {
        Ok(d) => d,
        Err(e) => return HttpResponse::BadRequest().body(format!("invalid body: {e}")),
    };
    // Stamp the update time
    wd.updated_at = Some(now_secs);
    *state.weather_data.lock().unwrap() = wd;

    if show {
        let prev = state.mode.lock().unwrap().clone();
        if !prev.is_transient() {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::Weather;
    }

    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// DELETE /api/weather — clear cached weather data and switch back to previous mode.
pub(crate) async fn api_clear_weather(state: web::Data<DaemonState>) -> impl Responder {
    *state.weather_data.lock().unwrap() = WeatherData::default();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().body("weather cleared")
}
/// POST /api/weather/fetch — trigger an immediate wttr.in fetch for the configured location.
/// Body (optional): `{"location": "London", "switch_mode": true}`
pub(crate) async fn api_weather_fetch_now(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let location = body
        .get("location")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let loc = state.weather_location.lock().unwrap().clone();
            if loc.is_empty() {
                None
            } else {
                Some(loc)
            }
        });

    match location {
        None => {
            HttpResponse::BadRequest().body(r#"no location configured; pass {"location":"City"}"#)
        }
        Some(loc) => {
            // Update stored location
            *state.weather_location.lock().unwrap() = loc.clone();

            match fetch_wttr(&state, &loc).await {
                Ok(()) => {
                    let switch = body
                        .get("switch_mode")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true);
                    if switch {
                        let cur = state.mode.lock().unwrap().clone();
                        if !cur.is_transient() {
                            *state.prev_mode.lock().unwrap() = cur;
                        }
                        *state.mode.lock().unwrap() = DisplayMode::Weather;
                    }
                    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "location": loc }))
                }
                Err(e) => HttpResponse::BadGateway()
                    .body(format!("wttr.in fetch failed for '{loc}': {e}")),
            }
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct WeatherAutoRequest {
    location: String,
    #[serde(default = "default_weather_auto_secs")]
    interval_secs: u64,
}
pub(crate) fn default_weather_auto_secs() -> u64 {
    1800
} // 30 minutes

/// PUT /api/weather/auto — configure location and auto-refresh interval.
/// Body: `{"location":"London","interval_secs":1800}`
pub(crate) async fn api_weather_auto(
    state: web::Data<DaemonState>,
    body: web::Json<WeatherAutoRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.location.is_empty() {
        return HttpResponse::BadRequest().body("location must not be empty");
    }
    let secs = req.interval_secs.max(60); // minimum 1 minute
    *state.weather_location.lock().unwrap() = req.location.clone();
    *state.weather_auto_secs.lock().unwrap() = secs;
    save_new_settings(&state);
    log::info!(
        "weather auto-fetch enabled: location='{}' interval={}s",
        req.location,
        secs
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok":           true,
        "location":     req.location,
        "interval_secs": secs,
    }))
}

/// DELETE /api/weather/auto — disable auto-fetch (clears location).
pub(crate) async fn api_weather_auto_stop(state: web::Data<DaemonState>) -> impl Responder {
    *state.weather_location.lock().unwrap() = String::new();
    *state.weather_auto_secs.lock().unwrap() = 0;
    save_new_settings(&state);
    log::info!("weather auto-fetch disabled");
    HttpResponse::Ok().body("weather auto-fetch disabled")
}

// ─── Notes / bulletin board API ──────────────────────────────────────────────

/// GET /api/notes — return current notes title and items.
pub(crate) async fn api_get_notes(state: web::Data<DaemonState>) -> impl Responder {
    let title = state.notes_title.lock().unwrap().clone();
    let items = state.notes_items.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "title": title, "items": items }))
}

/// POST /api/notes — replace or append notes.
///
/// Body JSON schema:
/// ```json
/// { "title": "Sprint 42", "items": ["Task A done", "Task B in progress"], "append": false, "show": true }
/// ```
/// - `append: true` → add to existing list; `false` (default) → replace.
/// - `show: true` (default) → switch to Notes mode immediately.
pub(crate) async fn api_push_notes(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let append = body
        .get("append")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let show = body
        .get("show")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    let new_items: Vec<String> = body
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Update title if provided
    if !title.is_empty() {
        *state.notes_title.lock().unwrap() = title;
    }

    // Replace or append items
    {
        let mut guard = state.notes_items.lock().unwrap();
        if append {
            guard.extend(new_items.iter().cloned());
        } else {
            *guard = new_items;
        }
        let count = guard.len();
        drop(guard);

        if show {
            let prev = state.mode.lock().unwrap().clone();
            if !prev.is_transient() {
                *state.prev_mode.lock().unwrap() = prev;
            }
            *state.mode.lock().unwrap() = DisplayMode::Notes;
        }

        HttpResponse::Ok().json(serde_json::json!({ "count": count }))
    }
}

/// DELETE /api/notes — clear all notes and return to previous mode.
pub(crate) async fn api_clear_notes(state: web::Data<DaemonState>) -> impl Responder {
    state.notes_title.lock().unwrap().clear();
    state.notes_items.lock().unwrap().clear();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().body("notes cleared")
}

// ─── QR Code mode ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct QrSetRequest {
    /// The text or URL to encode.
    payload: String,
    /// Optional title shown in the header bar (defaults to "QR Code").
    #[serde(default)]
    title: String,
    /// If true, automatically switch to qr_code mode after setting.
    #[serde(default = "bool_true")]
    show: bool,
}

pub(crate) fn bool_true() -> bool {
    true
}

/// GET /api/qr — return the current QR payload and title.
pub(crate) async fn api_get_qr(state: web::Data<DaemonState>) -> impl Responder {
    let payload = state.qr_payload.lock().unwrap().clone();
    let title = state.qr_title.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({ "payload": payload, "title": title }))
}

/// POST /api/qr — set the QR code payload (and optionally title), switch to qr_code mode.
///
/// Body: `{ "payload": "https://...", "title": "My App", "show": true }`
pub(crate) async fn api_set_qr(
    state: web::Data<DaemonState>,
    body: web::Json<QrSetRequest>,
) -> impl Responder {
    let payload = body.payload.trim().to_string();
    if payload.is_empty() {
        return HttpResponse::BadRequest().body("payload must not be empty");
    }
    let title = if body.title.is_empty() {
        "QR Code".to_string()
    } else {
        body.title.clone()
    };
    *state.qr_payload.lock().unwrap() = payload.clone();
    *state.qr_title.lock().unwrap() = title.clone();
    push_history(
        &state,
        HistoryKind::System,
        format!(
            "QR code set: {}",
            &payload.chars().take(60).collect::<String>()
        ),
    );
    if body.show {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
        *state.mode.lock().unwrap() = DisplayMode::QrCode;
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "payload": payload, "title": title }))
}

/// DELETE /api/qr — clear the QR code and return to the previous mode.
pub(crate) async fn api_clear_qr(state: web::Data<DaemonState>) -> impl Responder {
    state.qr_payload.lock().unwrap().clear();
    *state.qr_title.lock().unwrap() = "QR Code".to_string();
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    push_history(&state, HistoryKind::System, "QR code cleared".to_string());
    HttpResponse::Ok().body("qr cleared")
}

// ─── Mode schedule API ───────────────────────────────────────────────────────

/// GET /api/schedule/mode — return all mode schedule entries.
pub(crate) async fn api_get_mode_schedule(state: web::Data<DaemonState>) -> impl Responder {
    let entries = state.mode_schedule.lock().unwrap().clone();
    HttpResponse::Ok().json(entries)
}

/// POST /api/schedule/mode — add or replace a schedule entry.
///
/// Body: `{ "hour": 9, "minute": 0, "mode": "clock_sysinfo", "label": "Work hours" }`
/// Entries are kept sorted by (hour, minute). Adding an entry with the same (hour, minute)
/// replaces the existing one.
pub(crate) async fn api_add_mode_schedule(
    state: web::Data<DaemonState>,
    body: web::Json<ModeScheduleEntry>,
) -> impl Responder {
    // Validate mode
    if body.mode.parse::<DisplayMode>().is_err() {
        return HttpResponse::BadRequest().body(format!("unknown mode: {}", body.mode));
    }
    if body.hour > 23 || body.minute > 59 {
        return HttpResponse::BadRequest().body("hour must be 0-23, minute must be 0-59");
    }
    let entry = body.into_inner();
    let mut entries = state.mode_schedule.lock().unwrap();
    // Replace if same time slot exists, otherwise push
    if let Some(pos) = entries
        .iter()
        .position(|e| e.hour == entry.hour && e.minute == entry.minute)
    {
        entries[pos] = entry.clone();
    } else {
        entries.push(entry.clone());
    }
    entries.sort_by_key(|e| (e.hour, e.minute));
    save_mode_schedule(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": entries.len() }))
}

/// DELETE /api/schedule/mode — remove all schedule entries.
pub(crate) async fn api_clear_mode_schedule(state: web::Data<DaemonState>) -> impl Responder {
    state.mode_schedule.lock().unwrap().clear();
    save_mode_schedule(&[]);
    HttpResponse::Ok().body("schedule cleared")
}

/// DELETE /api/schedule/mode/{{index}} — remove entry at zero-based index.
pub(crate) async fn api_remove_mode_schedule_entry(
    state: web::Data<DaemonState>,
    path: web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut entries = state.mode_schedule.lock().unwrap();
    if idx >= entries.len() {
        return HttpResponse::BadRequest().body("index out of range");
    }
    entries.remove(idx);
    save_mode_schedule(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": entries.len() }))
}

// ─── Settings export / import ─────────────────────────────────────────────────

/// GET /api/settings/export — export complete daemon state as a JSON bundle.
///
/// Returns a JSON object containing:
/// - `daemon_state`: mode, quality, brightness, rotation
/// - `presets`: all saved text presets
/// - `profiles`: all saved display profiles
/// - `alert_thresholds`: CPU/temp thresholds
/// - `dim_schedule`: night dim configuration
/// - `notes`: current notes title and items
/// - `metrics`: current metrics title and items
///
/// The bundle can be re-imported via `POST /api/settings/import`.
pub(crate) async fn api_settings_export(state: web::Data<DaemonState>) -> impl Responder {
    let presets: HashMap<String, CustomTextPayload> = state.presets.lock().unwrap().clone();
    let profiles: Vec<DisplayProfile> = load_profiles();
    let thresholds: AlertThresholds = state.alert_thresholds.lock().unwrap().clone();
    let dim_sched: Option<DimSchedule> = state.dim_schedule.lock().unwrap().clone();

    let bundle = serde_json::json!({
        "export_version": 1,
        "exported_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        "daemon_state": {
            "mode":       state.mode.lock().unwrap().as_str(),
            "quality":    *state.quality.lock().unwrap(),
            "brightness": *state.dim_factor.lock().unwrap(),
            "rotate_deg": *state.rotate_deg.lock().unwrap(),
        },
        "presets":   presets,
        "profiles":  profiles,
        "alert_thresholds": thresholds,
        "dim_schedule": dim_sched,
        "notes": {
            "title": *state.notes_title.lock().unwrap(),
            "items": *state.notes_items.lock().unwrap(),
        },
        "metrics": {
            "title":   *state.metrics_title.lock().unwrap(),
            "metrics": *state.metrics_data.lock().unwrap(),
        },
        "http_monitors":      state.http_monitors.lock().unwrap().clone(),
        "ping_hosts":         state.ping_hosts.lock().unwrap().clone(),
        "world_clock":        state.world_clock_cities.lock().unwrap().clone(),
        "mode_schedule":      state.mode_schedule.lock().unwrap().clone(),
        "weather_location":   state.weather_location.lock().unwrap().clone(),
        "weather_auto_secs":  *state.weather_auto_secs.lock().unwrap(),
        "log_tail_path":      state.log_tail_path.lock().unwrap().clone(),
        "log_tail_n":         *state.log_tail_n.lock().unwrap(),
        "log_tail_interval":  *state.log_tail_interval.lock().unwrap(),
        "shell_cmd":          state.shell_cmd_text.lock().unwrap().clone(),
        "shell_cmd_interval": *state.shell_cmd_interval_secs.lock().unwrap(),
        "metrics_source":     state.metrics_source.lock().unwrap().clone(),
        "rss_feed_url":       state.rss_feed_url.lock().unwrap().clone(),
        "rss_feed_interval":  *state.rss_feed_interval.lock().unwrap(),
        "service_units":      state.service_units.lock().unwrap().clone(),
        "service_poll_interval": *state.service_poll_interval.lock().unwrap(),
    });

    HttpResponse::Ok()
        .insert_header((
            "Content-Disposition",
            "attachment; filename=ttlcd-settings.json",
        ))
        .insert_header(("Cache-Control", "no-store"))
        .json(bundle)
}

/// POST /api/settings/import — import a previously exported settings bundle.
///
/// Accepts the JSON format produced by `GET /api/settings/export`.
/// Only non-null fields are applied; missing sections are skipped.
pub(crate) async fn api_settings_import(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let mut applied: Vec<&str> = Vec::new();

    // daemon_state
    if let Some(ds) = body.get("daemon_state") {
        if let Some(mode_str) = ds.get("mode").and_then(|v| v.as_str()) {
            if let Some(m) = DisplayMode::from_str_name(mode_str) {
                *state.mode.lock().unwrap() = m.clone();
                *state.prev_mode.lock().unwrap() = m;
                applied.push("mode");
            }
        }
        if let Some(q) = ds.get("quality").and_then(serde_json::Value::as_u64) {
            *state.quality.lock().unwrap() = q.clamp(1, 100) as u8;
            applied.push("quality");
        }
        if let Some(b) = ds.get("brightness").and_then(serde_json::Value::as_f64) {
            *state.dim_factor.lock().unwrap() = (b as f32).clamp(0.0, 1.0);
            applied.push("brightness");
        }
        if let Some(r) = ds.get("rotate_deg").and_then(serde_json::Value::as_u64) {
            let deg = match r {
                90 => 90u16,
                180 => 180,
                270 => 270,
                _ => 0,
            };
            *state.rotate_deg.lock().unwrap() = deg;
            applied.push("rotate_deg");
        }
        persist_state(&state);
    }

    // presets
    if let Some(presets_val) = body.get("presets") {
        if let Ok(new_presets) =
            serde_json::from_value::<HashMap<String, CustomTextPayload>>(presets_val.clone())
        {
            let mut guard = state.presets.lock().unwrap();
            for (k, v) in new_presets {
                guard.insert(k, v);
            }
            save_presets_to_disk(&guard);
            applied.push("presets");
        }
    }

    // profiles
    if let Some(profiles_val) = body.get("profiles") {
        if let Ok(new_profiles) =
            serde_json::from_value::<Vec<DisplayProfile>>(profiles_val.clone())
        {
            let mut existing = load_profiles();
            for p in new_profiles {
                if let Some(e) = existing.iter_mut().find(|e| e.name == p.name) {
                    *e = p;
                } else {
                    existing.push(p);
                }
            }
            save_profiles(&existing);
            applied.push("profiles");
        }
    }

    // alert_thresholds
    if let Some(t_val) = body.get("alert_thresholds") {
        if let Ok(t) = serde_json::from_value::<AlertThresholds>(t_val.clone()) {
            *state.alert_thresholds.lock().unwrap() = t;
            applied.push("alert_thresholds");
        }
    }

    // dim_schedule
    if let Some(d_val) = body.get("dim_schedule") {
        if let Ok(sched) = serde_json::from_value::<Option<DimSchedule>>(d_val.clone()) {
            *state.dim_schedule.lock().unwrap() = sched;
            applied.push("dim_schedule");
        }
    }

    // notes
    if let Some(n_val) = body.get("notes") {
        if let Some(title) = n_val.get("title").and_then(|v| v.as_str()) {
            *state.notes_title.lock().unwrap() = title.to_string();
        }
        if let Some(items) = n_val.get("items").and_then(|v| v.as_array()) {
            let items: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            *state.notes_items.lock().unwrap() = items;
        }
        applied.push("notes");
    }

    // metrics
    if let Some(m_val) = body.get("metrics") {
        if let Some(title) = m_val.get("title").and_then(|v| v.as_str()) {
            *state.metrics_title.lock().unwrap() = title.to_string();
        }
        if let Ok(metrics) =
            serde_json::from_value::<Vec<Metric>>(m_val.get("metrics").cloned().unwrap_or_default())
        {
            *state.metrics_data.lock().unwrap() = metrics;
        }
        applied.push("metrics");
    }

    // http_monitors
    if let Some(hm_val) = body.get("http_monitors") {
        if let Ok(monitors) = serde_json::from_value::<Vec<HttpMonitorEntry>>(hm_val.clone()) {
            *state.http_monitors.lock().unwrap() = monitors.clone();
            save_http_monitors(&monitors);
            applied.push("http_monitors");
        }
    }

    // ping_hosts
    if let Some(ph_val) = body.get("ping_hosts") {
        if let Ok(hosts) = serde_json::from_value::<Vec<PingHostEntry>>(ph_val.clone()) {
            *state.ping_hosts.lock().unwrap() = hosts.clone();
            save_ping_hosts(&hosts);
            applied.push("ping_hosts");
        }
    }

    // world_clock
    if let Some(wc_val) = body.get("world_clock") {
        if let Ok(cities) = serde_json::from_value::<Vec<(String, i32)>>(wc_val.clone()) {
            *state.world_clock_cities.lock().unwrap() = cities.clone();
            save_world_clock(&cities);
            applied.push("world_clock");
        }
    }

    // mode_schedule
    if let Some(ms_val) = body.get("mode_schedule") {
        if let Ok(entries) = serde_json::from_value::<Vec<ModeScheduleEntry>>(ms_val.clone()) {
            *state.mode_schedule.lock().unwrap() = entries.clone();
            save_mode_schedule(&entries);
            applied.push("mode_schedule");
        }
    }
    // weather auto-fetch
    if let Some(loc) = body.get("weather_location").and_then(|v| v.as_str()) {
        if !loc.is_empty() {
            *state.weather_location.lock().unwrap() = loc.to_string();
            applied.push("weather_location");
        }
    }
    if let Some(secs) = body
        .get("weather_auto_secs")
        .and_then(serde_json::Value::as_u64)
    {
        *state.weather_auto_secs.lock().unwrap() = secs;
        applied.push("weather_auto_secs");
    }
    // log tail
    if let Some(path) = body.get("log_tail_path").and_then(|v| v.as_str()) {
        if !path.is_empty() {
            *state.log_tail_path.lock().unwrap() = path.to_string();
            *state.log_tail_interval.lock().unwrap() = body
                .get("log_tail_interval")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3)
                .max(1);
            *state.log_tail_n.lock().unwrap() = body
                .get("log_tail_n")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(16) as usize;
            applied.push("log_tail");
        }
    }
    // shell_cmd
    if let Some(cmd) = body.get("shell_cmd").and_then(|v| v.as_str()) {
        if !cmd.is_empty() {
            *state.shell_cmd_text.lock().unwrap() = cmd.to_string();
            *state.shell_cmd_interval_secs.lock().unwrap() = body
                .get("shell_cmd_interval")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(10)
                .max(1);
            applied.push("shell_cmd");
        }
    }
    // metrics_source
    if let Some(ms_val) = body.get("metrics_source") {
        if let Ok(src) = serde_json::from_value::<MetricsSource>(ms_val.clone()) {
            *state.metrics_source.lock().unwrap() = Some(src);
            applied.push("metrics_source");
        }
    }
    // rss feed
    if let Some(url) = body.get("rss_feed_url").and_then(|v| v.as_str()) {
        if !url.is_empty() {
            *state.rss_feed_url.lock().unwrap() = url.to_string();
            *state.rss_feed_interval.lock().unwrap() = body
                .get("rss_feed_interval")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(300)
                .max(30);
            applied.push("rss_feed");
        }
    }
    // service status
    if let Some(sv_val) = body.get("service_units") {
        if let Ok(units) = serde_json::from_value::<Vec<String>>(sv_val.clone()) {
            if !units.is_empty() {
                *state.service_units.lock().unwrap() = units;
                *state.service_poll_interval.lock().unwrap() = body
                    .get("service_poll_interval")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(10)
                    .max(5);
                applied.push("service_units");
            }
        }
    }
    save_new_settings(&state);

    log::info!("settings imported: {} section(s) applied", applied.len());
    HttpResponse::Ok().json(serde_json::json!({
        "applied": applied,
        "count":   applied.len(),
    }))
}

// ─── Pomodoro API ─────────────────────────────────────────────────────────────

/// GET /api/pomodoro — current Pomodoro timer state.
pub(crate) async fn api_get_pomodoro(state: web::Data<DaemonState>) -> impl Responder {
    let p = state.pomodoro.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "phase":               p.phase.as_str(),
        "remaining_secs":      p.remaining_secs(),
        "total_secs":          p.phase_total_secs(),
        "running":             p.running,
        "completed_sessions":  p.completed_sessions,
        "sessions_in_set":     p.completed_sessions % p.long_break_interval,
        "work_secs":           p.work_secs,
        "short_break_secs":    p.short_break_secs,
        "long_break_secs":     p.long_break_secs,
        "long_break_interval": p.long_break_interval,
    }))
}

/// POST /api/pomodoro/start — start or resume the Pomodoro timer.
pub(crate) async fn api_pomodoro_start(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if !p.running {
            p.running = true;
            p.phase_started_unix_ms = now_unix_ms();
        }
    }
    *state.mode.lock().unwrap() = DisplayMode::Pomodoro;
    push_history(&state, HistoryKind::System, "Pomodoro started".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// POST /api/pomodoro/pause — pause the running timer.
pub(crate) async fn api_pomodoro_pause(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if p.running {
            p.phase_elapsed_ms = p.phase_elapsed_ms_now();
            p.running = false;
            p.phase_started_unix_ms = 0;
        }
    }
    push_history(&state, HistoryKind::System, "Pomodoro paused".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// POST /api/pomodoro/skip — skip the current phase and advance to the next.
pub(crate) async fn api_pomodoro_skip(state: web::Data<DaemonState>) -> impl Responder {
    let next_phase = {
        let mut p = state.pomodoro.lock().unwrap();
        p.advance_phase();
        p.phase.as_str().to_string()
    };
    *state.mode.lock().unwrap() = DisplayMode::Pomodoro;
    push_history(
        &state,
        HistoryKind::System,
        format!("Pomodoro skipped → {next_phase}"),
    );
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "phase": next_phase }))
}

/// POST /api/pomodoro/reset — reset the timer to the beginning of the current phase.
pub(crate) async fn api_pomodoro_reset(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        p.phase = PomodoroPhase::Work;
        p.phase_elapsed_ms = 0;
        p.phase_started_unix_ms = 0;
        p.running = false;
        p.completed_sessions = 0;
    }
    push_history(&state, HistoryKind::System, "Pomodoro reset".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[derive(serde::Deserialize)]
pub(crate) struct PomodoroConfigRequest {
    work_mins: Option<u64>,
    short_break_mins: Option<u64>,
    long_break_mins: Option<u64>,
    long_break_interval: Option<u32>,
}

/// POST /api/pomodoro/config — configure work/break durations.
///
/// Body (all fields optional):
/// ```json
/// { "work_mins": 25, "short_break_mins": 5, "long_break_mins": 15, "long_break_interval": 4 }
/// ```
pub(crate) async fn api_pomodoro_config(
    state: web::Data<DaemonState>,
    body: web::Json<PomodoroConfigRequest>,
) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        if let Some(m) = body.work_mins {
            p.work_secs = m * 60;
        }
        if let Some(m) = body.short_break_mins {
            p.short_break_secs = m * 60;
        }
        if let Some(m) = body.long_break_mins {
            p.long_break_secs = m * 60;
        }
        if let Some(n) = body.long_break_interval {
            p.long_break_interval = n.max(1);
        }
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── World clock API ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct WorldClockCity {
    city: String,
    utc_offset_mins: i32,
}

/// GET /api/world_clock — return the current list of cities.
pub(crate) async fn api_get_world_clock(state: web::Data<DaemonState>) -> impl Responder {
    let cities = state
        .world_clock_cities
        .lock()
        .unwrap()
        .iter()
        .map(|(c, o)| serde_json::json!({ "city": c, "utc_offset_mins": o }))
        .collect::<Vec<_>>();
    HttpResponse::Ok().json(serde_json::json!({ "cities": cities }))
}

/// POST /api/world_clock — set city list (replaces all).
///
/// Body: `[{"city":"New York","utc_offset_mins":-300}, ...]`  (up to 4 entries)
pub(crate) async fn api_set_world_clock(
    state: web::Data<DaemonState>,
    body: web::Json<Vec<WorldClockCity>>,
) -> impl Responder {
    let entries: Vec<(String, i32)> = body
        .0
        .into_iter()
        .take(4)
        .map(|c| (c.city, c.utc_offset_mins.clamp(-14 * 60, 14 * 60)))
        .collect();
    let count = entries.len();
    *state.world_clock_cities.lock().unwrap() = entries.clone();
    save_world_clock(&entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "count": count }))
}

/// DELETE /api/world_clock — reset to default cities.
pub(crate) async fn api_clear_world_clock(state: web::Data<DaemonState>) -> impl Responder {
    state.world_clock_cities.lock().unwrap().clear();
    save_world_clock(&[]);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "cleared": true }))
}

/// POST /api/pomodoro/stop — stop the timer and return to the previous mode.
pub(crate) async fn api_pomodoro_stop(state: web::Data<DaemonState>) -> impl Responder {
    {
        let mut p = state.pomodoro.lock().unwrap();
        p.phase_elapsed_ms = p.phase_elapsed_ms_now();
        p.running = false;
        p.phase_started_unix_ms = 0;
    }
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    push_history(&state, HistoryKind::System, "Pomodoro stopped".to_string());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Ticker API ───────────────────────────────────────────────────────────────

/// GET /api/ticker — current ticker text and color.
pub(crate) async fn api_get_ticker(state: web::Data<DaemonState>) -> impl Responder {
    let text = state.ticker_text.lock().unwrap().clone();
    let color = state.ticker_color.lock().unwrap().clone();
    let pos = *state.ticker_pos.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({ "text": text, "color": color, "scroll_pos": pos }))
}

#[derive(serde::Deserialize)]
pub(crate) struct TickerRequest {
    text: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    switch_mode: Option<bool>,
}

/// POST /api/ticker — set the ticker text (and optionally switch to ticker mode).
///
/// Body: `{"text": "Hello World!", "color": "blue", "switch_mode": true}`
pub(crate) async fn api_set_ticker(
    state: web::Data<DaemonState>,
    body: web::Json<TickerRequest>,
) -> impl Responder {
    let text = body.text.clone();
    let color = body.color.clone().unwrap_or_else(|| "blue".to_string());

    *state.ticker_text.lock().unwrap() = text.clone();
    *state.ticker_color.lock().unwrap() = color.clone();
    *state.ticker_pos.lock().unwrap() = 0; // reset scroll to start

    if body.switch_mode.unwrap_or(true) {
        *state.mode.lock().unwrap() = DisplayMode::Ticker;
    }

    let summary = format!("Ticker: {}", text.chars().take(60).collect::<String>());
    push_history(&state, HistoryKind::Text, summary);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "text": text, "color": color }))
}

/// DELETE /api/ticker — clear ticker text and return to previous mode.
pub(crate) async fn api_clear_ticker(state: web::Data<DaemonState>) -> impl Responder {
    *state.ticker_text.lock().unwrap() = String::new();
    *state.ticker_pos.lock().unwrap() = 0;
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Progress display API ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SetProgressPayload {
    #[serde(default)]
    label: String,
    #[serde(default)]
    sublabel: String,
    #[serde(default)]
    value: f64,
    #[serde(default = "default_total")]
    total: f64,
    #[serde(default)]
    units: String,
    #[serde(default)]
    color: String,
    /// If true, record current time as started_at (enables ETA calculation).
    #[serde(default)]
    start_timer: bool,
}

pub(crate) fn default_total() -> f64 {
    100.0
}

/// GET /api/progress — return current progress state.
pub(crate) async fn api_get_progress(state: web::Data<DaemonState>) -> impl Responder {
    let ps = state.progress_state.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "label":      ps.label,
        "sublabel":   ps.sublabel,
        "value":      ps.value,
        "total":      ps.total,
        "units":      ps.units,
        "color":      format!("{:?}", ps.color).to_lowercase(),
        "pct":        if ps.total > 0.0 { ps.value / ps.total * 100.0 } else { 0.0 },
        "started_at": ps.started_at,
    }))
}

/// POST /api/progress — update the progress state.
///
/// Body fields (all optional):
/// - `label`       — main heading (default "Progress")
/// - `sublabel`    — secondary description line
/// - `value`       — current value (default 0)
/// - `total`       — maximum value (default 100)
/// - `units`       — unit suffix (e.g. "MB", "%")
/// - `color`       — accent: blue | green | yellow | red | mauve | teal | peach
/// - `start_timer` — true to record start time for ETA calculation
///
/// Also switches the display to Progress mode.
pub(crate) async fn api_set_progress(
    state: web::Data<DaemonState>,
    payload: web::Json<SetProgressPayload>,
) -> impl Responder {
    use ttlcd::display::NamedColor;
    let p = payload.into_inner();
    let color = match p.color.to_lowercase().as_str() {
        "green" => NamedColor::Green,
        "yellow" => NamedColor::Yellow,
        "red" => NamedColor::Red,
        "mauve" => NamedColor::Mauve,
        "teal" => NamedColor::Teal,
        "peach" => NamedColor::Peach,
        "white" => NamedColor::White,
        _ => NamedColor::Blue,
    };
    let started_at = if p.start_timer {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    } else {
        state.progress_state.lock().unwrap().started_at
    };

    let total = if p.total > 0.0 { p.total } else { 100.0 };
    let new_state = ProgressState {
        label: if p.label.is_empty() {
            "Progress".into()
        } else {
            p.label
        },
        sublabel: p.sublabel,
        value: p.value.clamp(0.0, total),
        total,
        units: p.units,
        color,
        started_at,
    };
    let pct = new_state.value / new_state.total * 100.0;
    *state.progress_state.lock().unwrap() = new_state;

    // Auto-switch to Progress mode
    let cur = state.mode.lock().unwrap().clone();
    if !cur.is_transient() && cur != DisplayMode::Progress {
        *state.prev_mode.lock().unwrap() = cur;
    }
    *state.mode.lock().unwrap() = DisplayMode::Progress;

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "pct": pct,
    }))
}

/// DELETE /api/progress — clear progress and return to previous mode.
pub(crate) async fn api_clear_progress(state: web::Data<DaemonState>) -> impl Responder {
    *state.progress_state.lock().unwrap() = ProgressState::default();
    if matches!(*state.mode.lock().unwrap(), DisplayMode::Progress) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().body("progress cleared")
}

// ── /api/pet ─────────────────────────────────────────────────────────────────

pub(crate) fn pet_json() -> serde_json::Value {
    let cur = PetSettings::current();
    serde_json::json!({
        "coat": cur.coat,
        "environment": cur.environment,
        "background": cur.background,
        "coats": ttlcd::pet::Coat::ALL.map(ttlcd::pet::Coat::as_str),
        "environments": ttlcd::pet::Environment::ALL.map(ttlcd::pet::Environment::as_str),
    })
}

/// GET /api/pet — current pet cat settings plus the allowed values.
pub(crate) async fn api_get_pet() -> impl Responder {
    HttpResponse::Ok().json(pet_json())
}

#[derive(Deserialize)]
pub(crate) struct PetUpdate {
    coat: Option<String>,
    environment: Option<String>,
    background: Option<String>,
}

/// POST /api/pet — update any subset of `coat`, `environment`, `background`
/// (`#rrggbb`).  Unknown values are rejected and nothing is changed.
pub(crate) async fn api_set_pet(
    state: web::Data<DaemonState>,
    body: web::Json<PetUpdate>,
) -> impl Responder {
    let bad = |msg: String| HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }));
    let mut cfg = ttlcd::pet::config();
    if let Some(v) = &body.coat {
        match ttlcd::pet::Coat::from_str_name(v) {
            Some(c) => cfg.coat = c,
            None => return bad(format!("unknown coat '{v}'")),
        }
    }
    if let Some(v) = &body.environment {
        match ttlcd::pet::Environment::from_str_name(v) {
            Some(e) => cfg.environment = e,
            None => return bad(format!("unknown environment '{v}'")),
        }
    }
    if let Some(v) = &body.background {
        match ttlcd::pet::parse_hex_color(v) {
            Some(b) => cfg.background = b,
            None => return bad(format!("background must be #rrggbb, got '{v}'")),
        }
    }
    ttlcd::pet::set_config(cfg);
    persist_state(&state);
    HttpResponse::Ok().json(pet_json())
}

#[derive(Deserialize)]
pub(crate) struct ScreensaverConfig {
    /// Timeout in seconds before screensaver activates. Set to 0 to disable.
    #[serde(default)]
    timeout_secs: Option<u64>,
    /// Display mode name to use as screensaver (e.g. "matrix_rain", "idle").
    #[serde(default)]
    mode: Option<String>,
}

/// POST /api/screensaver — configure the screensaver.
///
/// Body (JSON):
/// ```json
/// { "timeout_secs": 300, "mode": "matrix_rain" }
/// ```
///
/// Set `timeout_secs` to 0 (or DELETE) to disable. Omit either field to keep
/// its current value.
pub(crate) async fn api_set_screensaver(
    state: web::Data<DaemonState>,
    body: web::Json<ScreensaverConfig>,
) -> impl Responder {
    let req = body.into_inner();

    if let Some(timeout) = req.timeout_secs {
        *state.screensaver_timeout_secs.lock().unwrap() = timeout;
        if timeout == 0 {
            // Also deactivate if currently active
            let was_active = {
                let mut a = state.screensaver_active.lock().unwrap();
                if *a {
                    *a = false;
                    true
                } else {
                    false
                }
            };
            if was_active {
                let prev = state.prev_mode.lock().unwrap().clone();
                *state.mode.lock().unwrap() = prev;
            }
        }
    }

    if let Some(ref mode_name) = req.mode {
        if DisplayMode::from_str_name(mode_name).is_none() {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "unknown mode",
                "available": DisplayMode::ALL_NAMES,
            }));
        }
        *state.screensaver_mode_name.lock().unwrap() = mode_name.clone();
    }

    let timeout = *state.screensaver_timeout_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "enabled": timeout > 0,
        "timeout_secs": timeout,
        "mode": *state.screensaver_mode_name.lock().unwrap(),
    }))
}

/// DELETE /api/screensaver — disable the screensaver.
pub(crate) async fn api_clear_screensaver(state: web::Data<DaemonState>) -> impl Responder {
    *state.screensaver_timeout_secs.lock().unwrap() = 0;
    // Deactivate if currently running
    let was_active = {
        let mut a = state.screensaver_active.lock().unwrap();
        if *a {
            *a = false;
            true
        } else {
            false
        }
    };
    if was_active {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().body("screensaver disabled")
}

/// POST /api/screensaver/wake — immediately deactivate the screensaver
/// and restore the previous mode (useful for hardware button integrations).
pub(crate) async fn api_screensaver_wake(state: web::Data<DaemonState>) -> impl Responder {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    *state.last_activity_secs.lock().unwrap() = now;
    let was_active = {
        let mut a = state.screensaver_active.lock().unwrap();
        if *a {
            *a = false;
            true
        } else {
            false
        }
    };
    if was_active {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "was_active": was_active }))
}

// ─── Batch API ────────────────────────────────────────────────────────────────

/// POST /api/batch — execute multiple API operations in a single HTTP request.
///
/// This is useful for CI/CD pipelines, automation scripts, and any client that
/// needs to atomically configure several aspects of the daemon (e.g. set mode,
/// update metrics, and trigger an alert) without the latency of multiple round-trips.
///
/// Request body:
/// ```json
/// [
///   { "method": "POST", "path": "/api/mode",  "body": { "mode": "sysinfo" } },
///   { "method": "POST", "path": "/api/alert", "body": { "text": "Deploy started" } },
///   { "method": "POST", "path": "/api/progress", "body": { "label": "Deploy", "value": 0, "total": 100 } }
/// ]
/// ```
///
/// Returns an array of per-operation results:
/// ```json
/// [
///   { "index": 0, "status": 200, "body": "..." },
///   { "index": 1, "status": 200, "body": "..." },
///   { "index": 2, "status": 200, "body": "..." }
/// ]
/// ```
///
/// Operations are executed in order. On failure, processing continues for the
/// remaining operations (fail-soft). Up to 20 operations per batch.
///
/// Supported methods: GET, POST, DELETE.
/// Supported paths: any path that has a corresponding handler.
///
/// Note: GET operations return a simplified summary, not the full response body,
/// since this endpoint is primarily intended for write operations.
pub(crate) async fn api_batch(
    state: web::Data<DaemonState>,
    body: web::Json<Vec<serde_json::Value>>,
) -> impl Responder {
    let ops = body.into_inner();
    if ops.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "empty batch" }));
    }
    if ops.len() > 20 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "batch too large",
            "max": 20,
            "got": ops.len(),
        }));
    }

    let mut results: Vec<serde_json::Value> = Vec::with_capacity(ops.len());

    for (idx, op) in ops.into_iter().enumerate() {
        let method = op
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_uppercase();
        let path = op
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let op_body = op.get("body").cloned().unwrap_or(serde_json::Value::Null);

        let result = process_batch_op(&state, &method, &path, op_body).await;
        results.push(serde_json::json!({
            "index":  idx,
            "status": result.0,
            "body":   result.1,
        }));
    }

    let all_ok = results
        .iter()
        .all(|r| r["status"].as_u64().unwrap_or(500) < 400);
    HttpResponse::Ok().json(serde_json::json!({
        "ok":      all_ok,
        "count":   results.len(),
        "results": results,
    }))
}

/// Execute a single operation within a batch request.
/// Returns (http_status_code, response_body_string).
pub(crate) async fn process_batch_op(
    state: &web::Data<DaemonState>,
    method: &str,
    path: &str,
    body: serde_json::Value,
) -> (u16, String) {
    match (method, path) {
        // ── Mode ──
        ("POST", "/api/mode") => {
            let mode_name = body.get("mode").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(mode) = DisplayMode::from_str_name(mode_name) {
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() {
                    *state.prev_mode.lock().unwrap() = cur;
                }
                *state.mode.lock().unwrap() = mode;
                (200, format!("mode set to {mode_name}"))
            } else {
                (400, format!("unknown mode: {mode_name}"))
            }
        }
        ("POST", "/api/mode/next") => {
            let modes = DisplayMode::ALL_NAMES;
            let cur = state.mode.lock().unwrap().as_str().to_string();
            let pos = modes.iter().position(|&m| m == cur).unwrap_or(0);
            let next_name = modes[(pos + 1) % modes.len()];
            if let Some(next) = DisplayMode::from_str_name(next_name) {
                *state.mode.lock().unwrap() = next;
                (200, format!("mode → {next_name}"))
            } else {
                (500, "internal error".into())
            }
        }
        // ── Alert ──
        ("POST", "/api/alert") => {
            let text = body
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("batch alert");
            push_history(state, HistoryKind::Alert, text.to_string());
            (200, "alert queued".into())
        }
        // ── Text ──
        ("POST", "/api/text") => {
            if let Ok(payload) = serde_json::from_value::<CustomTextPayload>(body) {
                push_history(
                    state,
                    HistoryKind::Text,
                    format!("text: {}", payload.title.as_deref().unwrap_or("")),
                );
                *state.pending_text.lock().unwrap() = Some(payload.clone());
                let prev = state.mode.lock().unwrap().clone();
                if !prev.is_transient() {
                    *state.prev_mode.lock().unwrap() = prev;
                }
                *state.mode.lock().unwrap() = DisplayMode::CustomText;
                (200, "text set".into())
            } else {
                (400, "invalid text payload".into())
            }
        }
        // ── Progress ──
        ("POST", "/api/progress") => {
            let label = body
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = body
                .get("value")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let total = body
                .get("total")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(100.0);
            let sublabel = body
                .get("sublabel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            {
                let mut ps = state.progress_state.lock().unwrap();
                if !label.is_empty() {
                    ps.label = label.clone();
                }
                if !sublabel.is_empty() {
                    ps.sublabel = sublabel;
                }
                ps.value = value;
                ps.total = total;
            }
            let cur = state.mode.lock().unwrap().clone();
            if !cur.is_transient() && cur != DisplayMode::Progress {
                *state.prev_mode.lock().unwrap() = cur;
                *state.mode.lock().unwrap() = DisplayMode::Progress;
            }
            (200, format!("progress {value}/{total}"))
        }
        ("DELETE", "/api/progress") => {
            {
                let mut ps = state.progress_state.lock().unwrap();
                *ps = ttlcd::display::ProgressState::default();
            }
            let prev = state.prev_mode.lock().unwrap().clone();
            *state.mode.lock().unwrap() = prev;
            (200, "progress cleared".into())
        }
        // ── Quality ──
        ("POST", "/api/quality") => {
            if let Some(q) = body.get("quality").and_then(serde_json::Value::as_u64) {
                *state.quality.lock().unwrap() = (q as u8).clamp(1, 100);
                (200, format!("quality={q}"))
            } else {
                (400, "missing quality".into())
            }
        }
        // ── Blank ──
        ("POST", "/api/blank") => {
            *state.mode.lock().unwrap() = DisplayMode::Idle;
            (200, "blanked".into())
        }
        // ── Clear ──
        ("POST", "/api/clear") => {
            let prev = state.prev_mode.lock().unwrap().clone();
            *state.mode.lock().unwrap() = prev;
            (200, "cleared".into())
        }
        // ── GET status (informational) ──
        ("GET", "/api/status") => {
            let mode = state.mode.lock().unwrap().as_str().to_string();
            let seq = *state.frame_seq.lock().unwrap();
            (200, format!("{{\"mode\":\"{mode}\",\"frame_seq\":{seq}}}"))
        }
        // ── Unknown ──
        _ => (404, format!("unsupported batch operation: {method} {path}")),
    }
}
