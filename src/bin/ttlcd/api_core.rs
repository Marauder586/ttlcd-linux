//! Request guards and the core endpoints: mode switching, text, images, presets and countdown.

use super::*;

// ─── Auth middleware ─────────────────────────────────────────────────────────

/// Actix middleware that enforces Bearer-token authentication when an API key
/// has been configured via POST /api/auth/key.
///
/// The following paths are always exempt (no key required):
///   - `/`  (web UI)
///   - `/static/*`
///   - `/health`, `/ping`
///   - `/api/events`, `/api/stream`  (SSE and MJPEG — often used by dashboards)
///   - `/api/auth/status`  (so a client can tell whether a key is needed)
///
/// Changing or clearing the key (`/api/auth/key`) needs the current key.
///
/// All other paths require `Authorization: Bearer <key>`.
pub(crate) async fn auth_middleware(
    req: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, actix_web::Error> {
    let state_opt = req.app_data::<web::Data<DaemonState>>();
    let key_opt: Option<String> = state_opt
        .and_then(|s| s.api_key.lock().ok())
        .and_then(|g| g.clone());

    if let Some(expected) = key_opt {
        let path = req.path();
        let skip = path == "/"
            || path.starts_with("/static/")
            || path == "/health"
            || path == "/ping"
            || path.starts_with("/api/events")
            || path.starts_with("/api/stream")
            || path == "/api/auth/status";

        if !skip {
            let auth_ok = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|t| constant_time_eq(t.trim(), &expected));

            if !auth_ok {
                return Err(actix_web::error::ErrorUnauthorized(
                    r#"{"error":"Unauthorized","message":"Bearer token required. Set via POST /api/auth/key"}"#,
                ));
            }
        }
    }

    // Update last-activity timestamp for screensaver idle detection.
    // Skip updates for pure-static or SSE/stream paths.
    {
        let path = req.path();
        let is_static = path == "/"
            || path.starts_with("/static/")
            || path.starts_with("/api/events")
            || path.starts_with("/api/stream")
            || path == "/api/stream.mjpeg"
            || path == "/metrics";
        if !is_static {
            if let Some(s) = req.app_data::<web::Data<DaemonState>>() {
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                *s.last_activity_secs.lock().unwrap() = now_secs;
                // If screensaver was active, deactivate it and restore the previous mode
                let was_active = {
                    let mut a = s.screensaver_active.lock().unwrap();
                    if *a {
                        *a = false;
                        true
                    } else {
                        false
                    }
                };
                if was_active {
                    let prev = s.prev_mode.lock().unwrap().clone();
                    *s.mode.lock().unwrap() = prev;
                }
            }
        }
    }

    next.call(req).await
}

#[derive(Serialize)]
pub(crate) struct StatusResponse {
    mode: String,
    version: &'static str,
    available_modes: &'static [&'static str],
    headless: bool,
    rotate_deg: u16,
    quality: u8,
    frame_seq: u64,
    can_clear: bool,
    rotation_interval: u64,
    rotation_modes: Vec<String>,
    slideshow_image_count: usize,
    slideshow_interval: u64,
    slideshow_current_idx: usize,
    pending_scheduled: u32,
    /// Seconds remaining in countdown (None if not in countdown mode).
    countdown_remaining_secs: Option<i64>,
    /// Countdown label (None if not in countdown mode).
    countdown_label: Option<String>,
    /// Display brightness 0.0–1.0 (1.0 = full brightness).
    brightness: f32,
    /// Exponential-moving-average of frames rendered per second.
    render_fps: f32,
    /// Seconds since the daemon started.
    daemon_uptime_secs: u64,
    /// Whether the display is currently frozen (updates paused).
    frozen: bool,
    /// Whether the USB panel is currently connected (always false in headless mode).
    panel_connected: bool,
    /// Whether the stopwatch is currently running.
    stopwatch_running: bool,
    /// Current stopwatch elapsed milliseconds.
    stopwatch_elapsed_ms: u64,
}

pub(crate) async fn api_status(state: web::Data<DaemonState>) -> impl Responder {
    let mode = state.mode.lock().unwrap().as_str().to_string();
    let frame_seq = *state.frame_seq.lock().unwrap();
    let can_clear = state.mode.lock().unwrap().is_transient();
    let rot_interval = *state.rotation_interval.lock().unwrap();
    let rot_modes: Vec<String> = state
        .rotation_modes
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.as_str().to_string())
        .collect();
    let ss_count = state.slideshow_paths.lock().unwrap().len();
    let ss_interval = *state.slideshow_interval.lock().unwrap();
    let ss_idx = *state.slideshow_idx.lock().unwrap();
    // Countdown remaining
    let (cd_remaining, cd_label) = {
        let cd = state.countdown.lock().unwrap().clone();
        if let Some(c) = cd {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let rem = (c.target_unix as i64) - (now as i64);
            (Some(rem), Some(c.label))
        } else {
            (None, None)
        }
    };

    // Extract stopwatch fields before building the response to avoid
    // holding two MutexGuard temporaries on the same mutex in one expression.
    let (sw_running, sw_elapsed_ms) = {
        let sw = state.stopwatch.lock().unwrap();
        (sw.running, sw.current_elapsed_ms())
    };
    let rotate_deg = *state.rotate_deg.lock().unwrap();
    let quality = *state.quality.lock().unwrap();
    let pending_scheduled = *state.pending_scheduled.lock().unwrap();
    let brightness = *state.dim_factor.lock().unwrap();
    let render_fps = *state.render_fps.lock().unwrap();
    let frozen = *state.frozen.lock().unwrap();
    let daemon_uptime_secs = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        now.saturating_sub(state.daemon_start_secs)
    };

    HttpResponse::Ok().json(StatusResponse {
        mode,
        version: env!("CARGO_PKG_VERSION"),
        available_modes: DisplayMode::ALL_NAMES,
        headless: state.headless,
        rotate_deg,
        quality,
        frame_seq,
        can_clear,
        rotation_interval: rot_interval,
        rotation_modes: rot_modes,
        slideshow_image_count: ss_count,
        slideshow_interval: ss_interval,
        slideshow_current_idx: ss_idx,
        pending_scheduled,
        countdown_remaining_secs: cd_remaining,
        countdown_label: cd_label,
        brightness,
        render_fps,
        daemon_uptime_secs,
        frozen,
        panel_connected: *state.panel_connected.lock().unwrap(),
        stopwatch_running: sw_running,
        stopwatch_elapsed_ms: sw_elapsed_ms,
    })
}

#[derive(Deserialize)]
pub(crate) struct SetModeRequest {
    mode: String,
}

pub(crate) async fn api_set_mode(
    state: web::Data<DaemonState>,
    body: web::Json<SetModeRequest>,
) -> impl Responder {
    match body.mode.parse::<DisplayMode>() {
        Ok(new_mode) => {
            if !new_mode.is_transient() {
                *state.pending_text.lock().unwrap() = None;
                *state.pending_jpeg.lock().unwrap() = None;
                *state.prev_mode.lock().unwrap() = new_mode.clone();
            }
            *state.mode.lock().unwrap() = new_mode;
            persist_state(&state);
            HttpResponse::Ok().body("ok")
        }
        Err(e) => HttpResponse::BadRequest().body(e),
    }
}

pub(crate) async fn api_upload_image(
    state: web::Data<DaemonState>,
    mut mp: Multipart,
) -> impl Responder {
    let mut raw: Vec<u8> = Vec::new();
    while let Some(field) = mp.next().await {
        let mut field = match field {
            Ok(f) => f,
            Err(e) => return HttpResponse::BadRequest().body(format!("multipart error: {e}")),
        };
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(c) if raw.len() + c.len() > MAX_IMAGE_BYTES => {
                    return HttpResponse::PayloadTooLarge()
                        .body(format!("image is larger than {MAX_IMAGE_BYTES} bytes"));
                }
                Ok(c) => raw.extend_from_slice(&c),
                Err(e) => return HttpResponse::BadRequest().body(format!("chunk error: {e}")),
            }
        }
    }
    if raw.is_empty() {
        return HttpResponse::BadRequest().body("no image data");
    }

    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = ttlcd::decode_image(&raw)?;
        letterbox_for_panel(
            &apply_rotation(img, *state.rotate_deg.lock().unwrap()),
            *state.quality.lock().unwrap(),
        )
    })() {
        Ok(j) => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("decode error: {e}")),
    };

    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    push_history(
        &state,
        HistoryKind::Image,
        format!("Image uploaded ({} bytes)", raw.len()),
    );
    HttpResponse::Ok().body("image queued")
}

pub(crate) async fn api_text(
    state: web::Data<DaemonState>,
    body: web::Json<CustomTextPayload>,
) -> impl Responder {
    let payload = body.into_inner();
    let summary = payload.title.clone().unwrap_or_else(|| "(no title)".into());
    // Set auto-expiry if duration_secs was provided
    if let Some(dur) = payload.duration_secs {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        *state.auto_clear_at.lock().unwrap() = Some(now + dur);
    } else {
        *state.auto_clear_at.lock().unwrap() = None;
    }
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

pub(crate) async fn api_clear(state: web::Data<DaemonState>) -> impl Responder {
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    *state.pending_text.lock().unwrap() = None;
    *state.pending_jpeg.lock().unwrap() = None;
    *state.auto_clear_at.lock().unwrap() = None;
    HttpResponse::Ok().body("ok")
}

// ─── Presets ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct PresetEntry {
    name: String,
    payload: CustomTextPayload,
}

pub(crate) async fn api_presets_list(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.presets.lock().unwrap();
    let mut list: Vec<PresetEntry> = guard
        .iter()
        .map(|(k, v)| PresetEntry {
            name: k.clone(),
            payload: v.clone(),
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    HttpResponse::Ok().json(list)
}

/// GET /api/presets/{name} — return a single preset's payload as JSON.
pub(crate) async fn api_presets_get(
    state: web::Data<DaemonState>,
    name: web::Path<String>,
) -> impl Responder {
    let payload = state.presets.lock().unwrap().get(name.as_str()).cloned();
    match payload {
        Some(p) => HttpResponse::Ok().json(p),
        None => HttpResponse::NotFound().body(format!("preset '{name}' not found")),
    }
}

#[derive(Deserialize)]
pub(crate) struct SavePresetRequest {
    name: String,
    payload: CustomTextPayload,
}

pub(crate) async fn api_presets_save(
    state: web::Data<DaemonState>,
    body: web::Json<SavePresetRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.name.trim().is_empty() {
        return HttpResponse::BadRequest().body("name must not be empty");
    }
    {
        let mut guard = state.presets.lock().unwrap();
        guard.insert(req.name.clone(), req.payload);
        save_presets_to_disk(&guard);
    }
    HttpResponse::Ok().body(format!("saved '{}'", req.name))
}

pub(crate) async fn api_presets_apply(
    state: web::Data<DaemonState>,
    name: web::Path<String>,
) -> impl Responder {
    let payload = state.presets.lock().unwrap().get(name.as_str()).cloned();
    match payload {
        Some(p) => {
            {
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() {
                    *state.prev_mode.lock().unwrap() = cur;
                }
            }
            *state.pending_text.lock().unwrap() = Some(p);
            *state.mode.lock().unwrap() = DisplayMode::CustomText;
            HttpResponse::Ok().body("ok")
        }
        None => HttpResponse::NotFound().body(format!("preset '{name}' not found")),
    }
}

pub(crate) async fn api_presets_delete(
    state: web::Data<DaemonState>,
    name: web::Path<String>,
) -> impl Responder {
    let removed = {
        let mut guard = state.presets.lock().unwrap();
        let r = guard.remove(name.as_str()).is_some();
        if r {
            save_presets_to_disk(&guard);
        }
        r
    };
    if removed {
        HttpResponse::Ok().body("deleted")
    } else {
        HttpResponse::NotFound().body(format!("'{name}' not found"))
    }
}

// ─── Mode listing ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct ModeInfo {
    name: &'static str,
    description: &'static str,
}

pub(crate) async fn api_modes_list() -> impl Responder {
    let modes: Vec<ModeInfo> = vec![
        ModeInfo { name: "clock",          description: "Large digital clock with date and uptime" },
        ModeInfo { name: "big_clock",      description: "Full-screen giant HH:MM with large seconds and progress bar" },
        ModeInfo { name: "clock_sysinfo",  description: "Split-screen: clock above, quick stats below" },
        ModeInfo { name: "sysinfo",        description: "CPU, RAM, temperature, load average, uptime" },
        ModeInfo { name: "cpu_detail",     description: "Per-core CPU usage grid (up to 32 cores)" },
        ModeInfo { name: "network",        description: "Per-interface Rx/Tx rates and totals" },
        ModeInfo { name: "disk_usage",     description: "Disk partition usage bars" },
        ModeInfo { name: "process_top",    description: "Top processes sorted by CPU usage" },
        ModeInfo { name: "temperature",    description: "Thermal sensor readings with color-coded bars" },
        ModeInfo { name: "memory_detail",  description: "Detailed RAM breakdown: used, cached, free, swap" },
        ModeInfo { name: "countdown",      description: "Countdown timer with colour transitions and progress bar" },
        ModeInfo { name: "cpu_graph",      description: "CPU usage history bar chart (last 2 minutes)" },
        ModeInfo { name: "net_graph",      description: "Network Rx/Tx I/O history chart (last 60 seconds)" },
        ModeInfo { name: "recent_alerts",  description: "Last 5 notifications as an on-panel log" },
        ModeInfo { name: "process_memory", description: "Top 8 processes sorted by RAM usage" },
        ModeInfo { name: "disk_io",        description: "Disk read/write throughput per block device" },
        ModeInfo { name: "calendar",       description: "Full-month calendar with today highlighted" },
        ModeInfo { name: "stopwatch",      description: "Running stopwatch with lap times" },
        ModeInfo { name: "metrics",        description: "Custom key/value metrics dashboard (up to 8 items)" },
        ModeInfo { name: "weather",        description: "Weather snapshot — push via /api/weather or web UI" },
        ModeInfo { name: "notes",          description: "Bulletin board — push text items via /api/notes" },
        ModeInfo { name: "qr_code",         description: "Display a scannable QR code — set payload via /api/qr" },
        ModeInfo { name: "pomodoro",        description: "Pomodoro focus timer: 25 min work / 5 min break cycles with visual progress ring" },
        ModeInfo { name: "battery",        description: "Battery level, charging status, voltage and estimated time remaining (Linux laptops)" },
        ModeInfo { name: "world_clock",    description: "World clock showing up to 4 cities with configurable UTC offsets and day/night coloring" },
        ModeInfo { name: "ticker",         description: "Scrolling text ticker — push text via POST /api/ticker" },
        ModeInfo { name: "http_status",    description: "HTTP health monitor dashboard — configure endpoints via /api/http_status" },
        ModeInfo { name: "shell_cmd",      description: "Shell command output viewer — set command via POST /api/shell_cmd" },
        ModeInfo { name: "ping_monitor",   description: "ICMP ping latency dashboard — configure hosts via /api/ping_monitor" },
        ModeInfo { name: "dashboard",      description: "Four-quadrant overview: clock, CPU/RAM, network, temperature" },
        ModeInfo { name: "log_tail",       description: "Live log tail viewer — configure file via POST /api/log_tail" },
        ModeInfo { name: "cpu_freq",        description: "Per-core CPU clock frequency grid (reads /proc/cpuinfo)" },
        ModeInfo { name: "hostname_ip",    description: "Machine identification: hostname and all network IP addresses" },
        ModeInfo { name: "rss_feed",       description: "RSS/Atom headline feed — configure URL via POST /api/rss" },
        ModeInfo { name: "service_status", description: "Systemd unit state monitor — configure units via POST /api/services" },
        ModeInfo { name: "net_connections",description: "Active TCP/UDP socket viewer: listening ports and established connections" },
        ModeInfo { name: "uptime_fancy",   description: "Full-screen uptime display: big counter, boot time, load averages, memory" },
        ModeInfo { name: "os_info",        description: "OS and hardware info: distro, kernel, CPU model, memory, uptime" },
        ModeInfo { name: "docker_status",  description: "Docker containers panel: running/stopped containers with image and status" },
        ModeInfo { name: "logged_in_users",description: "Active user sessions: who is logged in, from where, and idle time" },
        ModeInfo { name: "load_graph",     description: "Load average history graph: 1m/5m/15m load avg as a filled line chart" },
        ModeInfo { name: "mem_graph",      description: "Memory usage history graph: RAM % over time as a filled line chart" },
        ModeInfo { name: "progress",       description: "Full-screen progress bar — configure via POST /api/progress with value/total/label" },
        ModeInfo { name: "matrix_rain",    description: "Matrix digital rain screensaver — animated falling characters" },
        ModeInfo { name: "bounce_ball",      description: "Bouncing ball screensaver — Catppuccin colour cycling on wall bounces" },
        ModeInfo { name: "speedometer_gauge", description: "Analog speedometer gauges for CPU % and RAM %" },
        ModeInfo { name: "disk_pie",          description: "Disk usage as pie charts for each mounted filesystem" },
        ModeInfo { name: "thermal_dashboard", description: "Temperature sensor bar chart dashboard — all CPU/GPU/SSD sensors" },
        ModeInfo { name: "led_clock",        description: "7-segment LED-style clock with date, load average, and hostname — flashing colons" },
        ModeInfo { name: "game_of_life",     description: "Conway's Game of Life — animated cellular automaton, re-seeds when population stagnates" },
        ModeInfo { name: "status_overview",  description: "4-quadrant live overview: clock, CPU/RAM, network Rx/Tx, and temperature/uptime" },
        ModeInfo { name: "ping_graph",      description: "Ping latency history graph — overlaid line charts for up to 4 monitored hosts" },
        ModeInfo { name: "analog_clock",    description: "Analog clock face with hour/minute/second hands and date/load display" },
        ModeInfo { name: "net_speed_graph", description: "Network speed history: overlaid Rx/Tx line chart with per-interface breakdown" },
        ModeInfo { name: "cpu_core_bars",   description: "Per-core CPU utilisation as horizontal bars — good for high core-count systems" },
        ModeInfo { name: "temp_graph",     description: "CPU/component temperature history graph with thermal thresholds" },
        ModeInfo { name: "disk_io_graph",  description: "Disk I/O history graph: total read and write throughput over time" },
        ModeInfo { name: "graphs_overview",description: "4-panel mini-charts: CPU, Load, Memory, Temperature at a glance" },
        ModeInfo { name: "plasma",          description: "Plasma screensaver — animated multi-sine colour field in Catppuccin palette" },
        ModeInfo { name: "fireworks",       description: "Fireworks screensaver — rockets launch and explode with particle trails" },
        ModeInfo { name: "cpu_heatmap",     description: "Per-core CPU utilization heat map — cores on Y axis, time samples on X axis" },
        ModeInfo { name: "starfield",       description: "Starfield screensaver — animated stars flying towards the viewer" },
        ModeInfo { name: "pulse",           description: "Ambient CPU pulse — concentric expanding rings, speed and color reflect CPU load" },
        ModeInfo { name: "minimal_clock",   description: "Ultra-minimal full-screen clock: large time + date, day-of-year progress bar" },
        ModeInfo { name: "particle_clock",  description: "Ambient particle clock — drifting Catppuccin pixels behind large time display" },
        ModeInfo { name: "snake",           description: "Self-playing snake game with greedy AI — Catppuccin colour scheme" },
        ModeInfo { name: "cube_3d",         description: "Spinning 3D wireframe cube — smooth rotation with Catppuccin edge colours" },
        ModeInfo { name: "dvd_bounce",      description: "DVD-style bouncing logo screensaver — colour changes on wall hit" },
        ModeInfo { name: "binary_clock",    description: "Binary clock — hours/minutes/seconds shown as lit/unlit cells in binary" },
        ModeInfo { name: "radar",           description: "CPU radar — rotating sweep with per-core load blips" },
        ModeInfo { name: "oscilloscope",    description: "Oscilloscope — CPU usage history as a phosphor-green waveform" },
        ModeInfo { name: "lissajous",       description: "Lissajous curves screensaver — parametric curves cycling through frequency ratios" },
        ModeInfo { name: "pong",            description: "Self-playing Pong with AI paddles — Catppuccin colours, scores tracked" },
        ModeInfo { name: "neon_clock",      description: "Neon-glow clock — colour-cycling phosphor glow on H:MM:SS with seconds arc" },
        ModeInfo { name: "net_heatmap",     description: "Network heatmap — 60-sample Rx/Tx bar history coloured by throughput" },
        ModeInfo { name: "word_clock",      description: "Word clock — displays time as English words in large type" },
        ModeInfo { name: "breakout",        description: "Self-playing Breakout/Arkanoid — AI paddle, colour-coded bricks, score and lives" },
        ModeInfo { name: "spectrum",        description: "Audio spectrum visualizer — 32 frequency bars driven by per-core CPU load" },
        ModeInfo { name: "doom_fire",       description: "Classic Doom-PSX fire simulation — 120x120 intensity cells with lateral spread and decay" },
        ModeInfo { name: "boids",           description: "Craig Reynolds Boids flocking — 80 coloured agents, separation/alignment/cohesion rules" },
        ModeInfo { name: "tunnel",          description: "Demoscene perspective tunnel — Catppuccin-tinted rings and sectors scroll towards viewer" },
        ModeInfo { name: "aurora",          description: "Aurora Borealis simulation — animated sine bands with Gaussian glow, stars, water reflection" },
        ModeInfo { name: "metaballs",       description: "Animated metaballs — 8 bouncing blobs blending smoothly into organic shapes" },
        ModeInfo { name: "sand",            description: "Falling sand simulation — coloured grains injected at top, pile up and drain" },
        ModeInfo { name: "gravity",         description: "N-body gravity — 10 bodies orbiting a central mass with fading trails" },
        ModeInfo { name: "voronoi",         description: "Animated Voronoi diagram — 20 bouncing seeds, edge-darkened colour regions" },
        ModeInfo { name: "reaction_diffusion", description: "Gray-Scott reaction-diffusion — organic spot/maze patterns at 160x160 cells" },
        ModeInfo { name: "snowfall",         description: "Snowfall screensaver — 180 flakes drifting with wind, wobble, and soft glow" },
        ModeInfo { name: "hex_grid",         description: "Per-core CPU hex grid — each hexagon fill level = core load; colour = load category" },
        ModeInfo { name: "bubble_chart",     description: "Process CPU bubble chart — top processes as circles scaled by CPU%, colour by process name" },
        ModeInfo { name: "conway_colors",   description: "Age-coloured Conway Game of Life — cells cycle through Catppuccin spectrum as they age" },
        ModeInfo { name: "julia_set",        description: "Animated Julia set fractal — parameter c rotates to produce continuously morphing shapes" },
        ModeInfo { name: "confetti",         description: "Celebration confetti — 120 rotating rectangular pieces drifting down with wind" },
        ModeInfo { name: "cpu_3d_bars",      description: "3D isometric CPU bars — per-core load as coloured 3D bars; green/yellow/red by load" },
        ModeInfo { name: "system_health",    description: "System health traffic lights — CPU, RAM, Temp, Disk, Load as colour-coded indicators" },
        ModeInfo { name: "neon_grid",        description: "Tron-inspired neon grid — glowing lines, pulsing nodes, travelling colour pulses" },
        ModeInfo { name: "ripple",           description: "Water ripple simulation — stones dropped at random intervals create circular waves" },
        ModeInfo { name: "hypno",            description: "Hypnotic spiral — overlaid sine patterns modulated by time for a mesmerising effect" },
        ModeInfo { name: "lorenz",          description: "Lorenz attractor — classic chaotic system traced as a 3D trail with rotating view" },
        ModeInfo { name: "mandelbrot",      description: "Mandelbrot zoom — animated fractal zoom into the Seahorse Valley with smooth colouring" },
        ModeInfo { name: "kaleidoscope",    description: "Kaleidoscope — 6-fold symmetry plasma with shifting hues and evolving patterns" },
        ModeInfo { name: "dna_helix",       description: "DNA Helix — double helix animation with coloured base pairs scrolling upward" },
        ModeInfo { name: "rain_glass",      description: "Rain on glass — realistic raindrops with trails running down a dark pane" },
        ModeInfo { name: "perlin_terrain", description: "Procedural terrain — scrolling heightmap with water, beach, grass, rock and snow zones" },
        ModeInfo { name: "fire_text",      description: "Fire text — the word 'ttlcd' burning in Doom-style fire simulation" },
        ModeInfo { name: "disco_floor",    description: "Disco floor — pulsing coloured tiles like a mirror ball dance floor" },
        ModeInfo { name: "heartbeat",      description: "Heartbeat ECG — green ECG waveform with spike height driven by CPU load" },
        ModeInfo { name: "clock_faces",    description: "Clock faces — three analog clock faces showing Local, UTC+5 and UTC-8 time" },
        ModeInfo { name: "waveform_bars",   description: "Waveform bars — per-CPU-core usage displayed as audio-visualiser style bars" },
        ModeInfo { name: "city_skyline",    description: "City skyline — night city silhouette with twinkling windows, moon and stars" },
        ModeInfo { name: "neon_sign",       description: "Neon sign — flickering neon 'ttlcd' letters on a dark brick wall with ground reflection" },
        ModeInfo { name: "gradient_clock",  description: "Gradient clock — large time display on an animated colour-cycling background" },
        ModeInfo { name: "galaxy",          description: "Galaxy — animated spiral galaxy with stellar arms, core glow and orbiting stars" },
        ModeInfo { name: "ocean_waves",     description: "Ocean waves — rolling sine waves with foam crests, sky gradient and caustics" },
        ModeInfo { name: "warp_speed",      description: "Warp speed — hyperspace streaks radiating from centre with acceleration" },
        ModeInfo { name: "morphing_blobs",  description: "Morphing blobs — five metaball orbs with colour mixing orbiting the centre" },
        ModeInfo { name: "clock_matrix",    description: "Clock Matrix — Matrix-style falling digits with the current time revealed in the middle" },
        ModeInfo { name: "sierpinski",      description: "Sierpiński fractal — chaos-game triangle with slow rotation and colour cycling" },
        ModeInfo { name: "lava_lamp",       description: "Lava lamp — warm-coloured metaball blobs slowly rising and falling in a dark bottle" },
        ModeInfo { name: "pendulum_wave",   description: "Pendulum wave — 16 pendulums with varying lengths creating mesmerising wave patterns" },
        ModeInfo { name: "fiber_optic",     description: "Fiber optic — glowing fiber strands with travelling light pulses radiating from centre" },
        ModeInfo { name: "network_globe",   description: "Network globe — wireframe rotating Earth with animated network connection arcs" },
        ModeInfo { name: "plasma_ball",    description: "Plasma ball — swirling electric tendrils inside a glowing sphere with rim lighting" },
        ModeInfo { name: "moss_growth",    description: "Moss growth — organic cellular automaton simulating slow-growing moss or lichen" },
        ModeInfo { name: "torus", description: "Torus — rotating 3D donut rendered with depth buffer and diffuse lighting" },
        ModeInfo { name: "lemniscate", description: "Lemniscate — animated lemniscate of Bernoulli / figure-eight curves" },
        ModeInfo { name: "system_art", description: "System art — CPU, memory, and load as artistic circular ring gauges" },
        ModeInfo { name: "fourier_epicycles", description: "Fourier epicycles — chain of rotating circles tracing a Fourier series approximation" },
        ModeInfo { name: "rule_30", description: "Rule 30 — Wolfram elementary 1D cellular automaton scrolling downward with colour coding" },
        ModeInfo { name: "nbody_gravity", description: "N-body gravity — 4 bodies under mutual gravitational attraction with trail rendering" },
        ModeInfo { name: "double_pendulum", description: "Double pendulum — chaotic double pendulum simulation with colour-traced trail" },
        ModeInfo { name: "rose_curve", description: "Rose curve — animated parametric rose r=cos(k*theta) with evolving petal count" },
        ModeInfo { name: "truchet", description: "Truchet tiles — hash-based quarter-circle tile pattern with flowing curves" },
        ModeInfo { name: "mandala", description: "Mandala — animated 8-fold symmetric mandala with layered petals and spirals" },
        ModeInfo { name: "flow_field", description: "Flow field — hundreds of particles streaming through a hash-noise vector field" },
        ModeInfo { name: "glitch_art", description: "Glitch art — RGB channel separation and scan-line corruption effects" },
        ModeInfo { name: "wave_interference", description: "Wave interference — two-source wave superposition with animated source positions" },
        ModeInfo { name: "clifford_attractor", description: "Clifford attractor — de Jong strange attractor with slowly morphing parameters" },
        ModeInfo { name: "icosahedron", description: "Icosahedron — wireframe rotating icosahedron with z-depth brightness" },
        ModeInfo { name: "neural_net", description: "Neural network — animated 4-layer perceptron with signal pulses flowing between nodes" },
        ModeInfo { name: "ribbon_dance", description: "Ribbon dance — seven colourful flowing ribbons undulating with sinusoidal harmonics" },
        ModeInfo { name: "hex_plasma", description: "Hex plasma — hexagonal grid with independently pulsing plasma-coloured cells" },
        ModeInfo { name: "sand_waterfall", description: "Sand waterfall — falling sand particle simulation, particles pile up in heaps" },
        ModeInfo { name: "magnetic_field", description: "Magnetic field — two orbiting magnetic poles with visualised field lines" },
        ModeInfo { name: "smoke_plume", description: "Smoke plume — wispy smoke rising from a heat source with advection simulation" },
        ModeInfo { name: "color_mosaic", description: "Color mosaic — tiled colour grid with wave interference hue patterns" },
        ModeInfo { name: "solar_system", description: "Solar system — animated orrery with 6 planets, orbits, and Lambertian shading" },
        ModeInfo { name: "lightning_storm", description: "Lightning storm — recursive branching lightning bolts with stormy atmosphere" },
        ModeInfo { name: "spirograph", description: "Spirograph — five rotating hypotrochoid curves in Catppuccin colours" },
        ModeInfo { name: "fractal_tree", description: "Fractal tree — iterative fractal tree with wind sway animation" },
        ModeInfo { name: "bubble_rise", description: "Bubble rise — physics-based coloured bubbles with specular highlights rising in water" },
        ModeInfo { name: "asteroid_field", description: "Asteroid field — tumbling rocky asteroids drifting through a star field" },
        ModeInfo { name: "pet_cat",        description: "Pet cat — pixel-art cat that sleeps, grooms, stretches and plays with a ball; coat and room are configurable" },
        ModeInfo { name: "ekg_strip",      description: "EKG rhythm strip — P wave, QRS complex and T wave on scrolling ECG paper; heart rate follows CPU load (70 bpm idle to 160 bpm)" },
        ModeInfo { name: "idle",           description: "Idle — no new frames; the last one stays on screen" },
    ];
    HttpResponse::Ok().json(modes)
}

/// POST /api/mode/random — switch to a random live mode (excludes idle, custom_text, custom_image, slideshow).
pub(crate) async fn api_mode_random(state: web::Data<DaemonState>) -> impl Responder {
    // Pick from live modes (not transient/idle)
    let live_modes: Vec<&str> = DisplayMode::ALL_NAMES
        .iter()
        .filter(|&&n| !matches!(n, "idle" | "custom_text" | "custom_image" | "slideshow"))
        .copied()
        .collect();
    if live_modes.is_empty() {
        return HttpResponse::InternalServerError().body("no live modes available");
    }
    // Use time-based seed for pseudo-random selection
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let idx = (seed as usize) % live_modes.len();
    let chosen = live_modes[idx];
    if let Some(new_mode) = DisplayMode::from_str_name(chosen) {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
        *state.mode.lock().unwrap() = new_mode.clone();
        push_history(
            &state,
            HistoryKind::System,
            format!("random mode → {chosen}"),
        );
        HttpResponse::Ok().json(serde_json::json!({ "ok": true, "mode": chosen }))
    } else {
        HttpResponse::InternalServerError().body(format!("mode {chosen} not found"))
    }
}

/// POST /api/mode/next — advance to the next live mode (wraps around).
pub(crate) async fn api_mode_next(state: web::Data<DaemonState>) -> impl Responder {
    cycle_mode(&state, 1)
}

/// POST /api/mode/prev — go to the previous live mode (wraps around).
pub(crate) async fn api_mode_prev(state: web::Data<DaemonState>) -> impl Responder {
    cycle_mode(&state, DisplayMode::ALL_NAMES.len() - 1)
}

pub(crate) fn cycle_mode(state: &DaemonState, step: usize) -> HttpResponse {
    let all: Vec<DisplayMode> = DisplayMode::ALL_NAMES
        .iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .collect();
    let cur = state.mode.lock().unwrap().clone();
    let cur_pos = all.iter().position(|m| m == &cur).unwrap_or(0);
    let next = all[(cur_pos + step) % all.len()].clone();
    *state.pending_text.lock().unwrap() = None;
    *state.pending_jpeg.lock().unwrap() = None;
    *state.prev_mode.lock().unwrap() = next.clone();
    *state.mode.lock().unwrap() = next.clone();
    HttpResponse::Ok().body(next.as_str())
}

// ─── Alert shorthand ─────────────────────────────────────────────────────────

/// POST /api/alert — send a severity-keyed notification without building a full
/// CustomTextPayload. Useful for CI pipelines and scripts.
///
/// Body (JSON):
///   { "level": "error"|"warning"|"info"|"success"|"critical",
///     "message": "human-readable line",
///     "detail":  "optional second line"  }
#[derive(Deserialize)]
pub(crate) struct AlertRequest {
    level: AlertLevel,
    message: String,
    #[serde(default)]
    detail: Option<String>,
    /// Optional progress bar value 0.0–1.0.
    #[serde(default)]
    progress: Option<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AlertLevel {
    Error,
    Warning,
    Info,
    Success,
    Critical,
}

pub(crate) async fn api_alert(
    state: web::Data<DaemonState>,
    body: web::Json<AlertRequest>,
) -> impl Responder {
    use ttlcd::display::NamedColor;
    let req = body.into_inner();
    let (title, color): (&str, NamedColor) = match req.level {
        AlertLevel::Error => ("ERROR", NamedColor::Red),
        AlertLevel::Warning => ("WARNING", NamedColor::Yellow),
        AlertLevel::Info => ("INFO", NamedColor::Blue),
        AlertLevel::Success => ("SUCCESS", NamedColor::Green),
        AlertLevel::Critical => ("CRITICAL", NamedColor::Red),
    };
    let msg_summary = req.message.clone();
    let payload = CustomTextPayload {
        title: Some(title.into()),
        subtitle: Some(req.message),
        lines: req.detail.into_iter().collect(),
        progress: req.progress,
        color,
        show_time: true,
        ..Default::default()
    };
    push_history(
        &state,
        HistoryKind::Alert,
        format!("{title} — {msg_summary}"),
    );
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

// ─── Screenshot download ──────────────────────────────────────────────────────

// ─── Canvas push (raw pixel injection) ────────────────────────────────────────

/// POST /api/canvas — push any image directly to the panel display.
///
/// Send the image bytes as the request body (JPEG, PNG, BMP, GIF, WEBP, etc.).
/// Content-Type header should be set to the appropriate image MIME type.
/// The image is resized to 480×480 and displayed immediately.
/// It stays until the next mode change or `POST /api/clear`.
///
/// Example:
///   curl -X POST http://localhost:7171/api/canvas \
///        --data-binary @photo.jpg -H "Content-Type: image/jpeg"
pub(crate) async fn api_canvas(state: web::Data<DaemonState>, body: web::Bytes) -> impl Responder {
    if body.is_empty() {
        return HttpResponse::BadRequest().body("empty body — send image bytes");
    }
    apply_canvas_bytes(state, &body)
}

pub(crate) fn apply_canvas_bytes(state: web::Data<DaemonState>, bytes: &[u8]) -> HttpResponse {
    let img = match ttlcd::decode_image(bytes) {
        Ok(i) => i,
        Err(e) => return HttpResponse::BadRequest().body(format!("image decode error: {e:#}")),
    };

    let rgb = img
        .resize_exact(480, 480, image::imageops::FilterType::Lanczos3)
        .into_rgb8();

    // Encode as JPEG and push as pending frame
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        &mut buf,
        *state.quality.lock().unwrap(),
    );
    if let Err(e) = encoder.encode_image(&image::DynamicImage::ImageRgb8(rgb)) {
        return HttpResponse::InternalServerError().body(format!("encode error: {e}"));
    }

    *state.pending_jpeg.lock().unwrap() = Some(buf.clone());
    *state.last_frame.lock().unwrap() = Some(buf);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    {
        let mut seq = state.frame_seq.lock().unwrap();
        *seq = seq.wrapping_add(1);
    }
    push_history(
        &state,
        HistoryKind::System,
        "canvas frame pushed".to_string(),
    );

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "canvas frame applied (480×480, CustomImage mode)"
    }))
}

/// GET /api/screenshot — like /api/preview but forces a file download.
pub(crate) async fn api_screenshot(state: web::Data<DaemonState>) -> impl Responder {
    let guard = state.last_frame.lock().unwrap();
    match guard.as_ref() {
        Some(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .insert_header(("Content-Disposition", "attachment; filename=panel.jpg"))
            .content_type("image/jpeg")
            .body(jpeg.clone()),
        None => HttpResponse::NoContent()
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
    }
}

// ─── Countdown timer ─────────────────────────────────────────────────────────

/// POST /api/countdown — start a countdown timer on the panel.
///
/// Body (JSON):
///   { "seconds": 300 }                         ← count down from N seconds
///   { "until": "2026-05-13T21:00:00+00:00" }   ← count down to a UTC datetime
///   { "label": "Build cooldown" }              ← optional label
///
/// The label is optional.  Supply `seconds` OR `until`, not both.
#[derive(Deserialize)]
pub(crate) struct CountdownRequest {
    /// Total seconds to count down from.
    #[serde(default)]
    seconds: Option<u64>,
    /// ISO-8601 datetime string to count down to.
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

pub(crate) async fn api_countdown(
    state: web::Data<DaemonState>,
    body: web::Json<CountdownRequest>,
) -> impl Responder {
    let req = body.into_inner();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let (target_unix, total_secs) = if let Some(secs) = req.seconds {
        if secs == 0 {
            return HttpResponse::BadRequest().body("seconds must be > 0");
        }
        (now_secs + secs, secs)
    } else if let Some(ref until_str) = req.until {
        // Parse ISO-8601 with chrono
        match chrono::DateTime::parse_from_rfc3339(until_str) {
            Ok(dt) => {
                let target = dt.timestamp() as u64;
                if target <= now_secs {
                    return HttpResponse::BadRequest().body("'until' is in the past");
                }
                (target, target - now_secs)
            }
            Err(e) => return HttpResponse::BadRequest().body(format!("invalid datetime: {e}")),
        }
    } else {
        return HttpResponse::BadRequest().body("supply 'seconds' or 'until'");
    };

    let label = req.label.unwrap_or_default();
    *state.countdown.lock().unwrap() = Some(CountdownState {
        target_unix,
        total_secs,
        label: label.clone(),
        expired_notified: false,
    });
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.mode.lock().unwrap() = DisplayMode::Countdown;
    push_history(
        &state,
        HistoryKind::Alert,
        format!("Countdown: {total_secs}s — {label}"),
    );
    HttpResponse::Ok().body(format!("countdown started: {total_secs}s"))
}

// ─── Health check ─────────────────────────────────────────────────────────────

/// GET /health — lightweight liveness probe.
/// Returns 200 with a small JSON body even in headless mode.
pub(crate) async fn api_health(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct Health {
        ok: bool,
        version: &'static str,
        mode: String,
        headless: bool,
        frame_seq: u64,
    }
    HttpResponse::Ok().json(Health {
        ok: true,
        version: env!("CARGO_PKG_VERSION"),
        mode: state.mode.lock().unwrap().as_str().to_string(),
        headless: state.headless,
        frame_seq: *state.frame_seq.lock().unwrap(),
    })
}

// ─── Ping ───────────────────────────────────────────────────────────────────────

/// GET /ping or GET /api/ping — returns "pong" instantly.
/// Useful for latency checks and uptime monitoring.
pub(crate) async fn api_ping() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body("pong")
}

// ─── Blank frame ──────────────────────────────────────────────────────────────

/// POST /api/blank — push a solid background-colour frame (effectively
/// "clears" the panel).  The panel stays on CustomImage mode until /api/clear
/// is called.
pub(crate) async fn api_blank(state: web::Data<DaemonState>) -> impl Responder {
    let quality = *state.quality.lock().unwrap();
    let rotate = *state.rotate_deg.lock().unwrap();
    let blank_img = render_solid(30, 30, 46);
    let blank_img = apply_rotation(blank_img, rotate);
    let jpeg = match encode_for_panel(&blank_img, quality) {
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
    HttpResponse::Ok().body("blanked")
}

// ─── Full reset ───────────────────────────────────────────────────────────────

/// POST /api/display/reset — reset daemon to defaults.
///
/// - Stops rotation (sets interval = 0)
/// - Cancels countdown and screensaver
/// - Returns to `clock` mode
/// - Clears pending JPEG
///
/// Does NOT clear presets, history, thresholds, or persisted settings.
pub(crate) async fn api_display_reset(state: web::Data<DaemonState>) -> impl Responder {
    *state.rotation_interval.lock().unwrap() = 0;
    *state.rotation_modes.lock().unwrap() = vec![];
    *state.countdown.lock().unwrap() = None;
    *state.screensaver_active.lock().unwrap() = false;
    *state.pending_jpeg.lock().unwrap() = None;
    *state.mode.lock().unwrap() = DisplayMode::Clock;
    *state.prev_mode.lock().unwrap() = DisplayMode::Clock;
    log::info!("display reset to defaults");
    push_history(&state, HistoryKind::System, "display reset".to_string());
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "mode": "clock",
        "message": "display reset to defaults"
    }))
}

// ─── Text → JPEG preview renderer ───────────────────────────────────────────

/// POST /api/render — render a CustomTextPayload to JPEG and return it as
/// image/jpeg.  Does NOT affect panel or daemon state.  Useful for previewing
/// a notification layout in the web UI before sending it to the device.
pub(crate) async fn api_render(
    state: web::Data<DaemonState>,
    body: web::Json<CustomTextPayload>,
) -> impl Responder {
    let payload = body.into_inner();
    let img = render_custom_text(&payload);
    let img = apply_rotation(img, *state.rotate_deg.lock().unwrap());
    let quality = *state.quality.lock().unwrap();
    match encode_for_panel(&img, quality) {
        Ok(jpeg) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .content_type("image/jpeg")
            .body(jpeg),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// ─── Server-side image load ───────────────────────────────────────────────────

/// POST /api/image/path — load an image from an absolute server-side path and
/// send it to the panel.  Avoids multipart upload overhead for files already on
/// the same machine.
///
/// Body (JSON):
///   { "path": "/absolute/path/to/image.jpg", "letterbox": true }
#[derive(Deserialize)]
pub(crate) struct ImagePathRequest {
    path: String,
    #[serde(default)]
    letterbox: bool,
}

pub(crate) async fn api_image_path(
    state: web::Data<DaemonState>,
    body: web::Json<ImagePathRequest>,
) -> impl Responder {
    let req = body.into_inner();
    let path = PathBuf::from(&req.path);
    if !path.is_absolute() {
        return HttpResponse::BadRequest().body("path must be absolute");
    }
    if !path.exists() {
        return HttpResponse::NotFound().body(format!("'{}' not found", req.path));
    }
    let quality = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();
    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = ttlcd::open_image(&path)?;
        let img = apply_rotation(img, rotate_deg);
        if req.letterbox {
            letterbox_for_panel(&img, quality)
        } else {
            encode_for_panel(&img, quality)
        }
    })() {
        Ok(j) => j,
        Err(e) => return HttpResponse::UnprocessableEntity().body(format!("image error: {e}")),
    };
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    HttpResponse::Ok().body("image queued")
}

#[derive(Deserialize)]
pub(crate) struct ImageUrlRequest {
    /// HTTP/HTTPS URL of the image to fetch and display.
    url: String,
    /// If true, letterbox (fit with black bars) instead of cropping. Defaults to true.
    #[serde(default = "bool_true")]
    letterbox: bool,
}

/// POST /api/image/url — fetch an image from an HTTP/HTTPS URL and display it on the panel.
///
/// Useful for dashboards that want to push live chart images, webcam frames,
/// or any image hosted on the network.
///
/// Body: `{ "url": "https://example.com/chart.png", "letterbox": true }`
pub(crate) async fn api_image_url(
    state: web::Data<DaemonState>,
    body: web::Json<ImageUrlRequest>,
) -> impl Responder {
    let req = body.into_inner();
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return HttpResponse::BadRequest().body("url must start with http:// or https://");
    }

    // Fetch image bytes from URL
    let image_bytes = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => match client.get(&url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return HttpResponse::BadGateway()
                        .body(format!("remote returned {}", resp.status()));
                }
                match read_body_capped(resp, MAX_IMAGE_BYTES).await {
                    Ok(b) => b,
                    Err(e) => return HttpResponse::BadGateway().body(format!("read error: {e}")),
                }
            }
            Err(e) => return HttpResponse::BadGateway().body(format!("fetch error: {e}")),
        },
        Err(e) => return HttpResponse::InternalServerError().body(format!("client error: {e}")),
    };

    let quality = *state.quality.lock().unwrap();
    let rotate_deg = *state.rotate_deg.lock().unwrap();
    let jpeg = match (|| -> anyhow::Result<Vec<u8>> {
        let img = ttlcd::decode_image(&image_bytes)?;
        let img = apply_rotation(img, rotate_deg);
        if req.letterbox {
            letterbox_for_panel(&img, quality)
        } else {
            encode_for_panel(&img, quality)
        }
    })() {
        Ok(j) => j,
        Err(e) => {
            return HttpResponse::UnprocessableEntity().body(format!("image decode error: {e}"))
        }
    };
    push_history(
        &state,
        HistoryKind::Image,
        format!(
            "Image from URL: {}",
            url.chars().take(80).collect::<String>()
        ),
    );
    {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
    }
    *state.pending_jpeg.lock().unwrap() = Some(jpeg);
    *state.mode.lock().unwrap() = DisplayMode::CustomImage;
    HttpResponse::Ok().body("image queued from URL")
}
