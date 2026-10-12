//! Shared daemon state and the HTTP server set-up.

use super::*;

// ─── Daemon ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct DaemonState {
    pub(crate) mode: Arc<Mutex<DisplayMode>>,
    /// Mode before transitioning to a transient mode.
    pub(crate) prev_mode: Arc<Mutex<DisplayMode>>,
    pub(crate) pending_jpeg: Arc<Mutex<Option<Vec<u8>>>>,
    pub(crate) last_frame: Arc<Mutex<Option<Vec<u8>>>>,
    /// Monotonically-increasing counter; incremented with each new stored frame.
    pub(crate) frame_seq: Arc<Mutex<u64>>,
    pub(crate) pending_text: Arc<Mutex<Option<CustomTextPayload>>>,
    /// Named presets (in-memory; persisted to ~/.config/ttlcd/presets.json).
    pub(crate) presets: Arc<Mutex<HashMap<String, CustomTextPayload>>>,
    pub(crate) slideshow_paths: Arc<Mutex<Vec<PathBuf>>>,
    pub(crate) slideshow_idx: Arc<Mutex<usize>>,
    pub(crate) slideshow_interval: Arc<Mutex<u64>>,
    /// Ordered list of modes for auto-rotation (empty = disabled).
    pub(crate) rotation_modes: Arc<Mutex<Vec<DisplayMode>>>,
    /// Seconds between auto-rotation advances (0 = disabled).
    pub(crate) rotation_interval: Arc<Mutex<u64>>,
    pub(crate) quality: Arc<Mutex<u8>>,
    pub(crate) vid: u16,
    pub(crate) pid: u16,
    pub(crate) headless: bool,
    pub(crate) rotate_deg: Arc<Mutex<u16>>,
    /// Ring-buffer of the last 50 text/alert notifications sent to the panel.
    pub(crate) history: Arc<Mutex<VecDeque<HistoryEntry>>>,
    /// Countdown target (Unix seconds) + total duration + label.
    pub(crate) countdown: Arc<Mutex<Option<CountdownState>>>,
    /// Auto-clear: if set, CustomText/CustomImage reverts to prev_mode at this Unix time.
    pub(crate) auto_clear_at: Arc<Mutex<Option<u64>>>,
    /// Count of pending scheduled tasks (for status display).
    pub(crate) pending_scheduled: Arc<Mutex<u32>>,
    /// Brightness multiplier 0.0–1.0 applied to every rendered frame (1.0 = full).
    pub(crate) dim_factor: Arc<Mutex<f32>>,
    /// Exponential-moving-average of frames rendered per second.
    pub(crate) render_fps: Arc<Mutex<f32>>,
    /// Timestamp of the last rendered frame (for FPS calc); None before first frame.
    pub(crate) render_fps_last: Arc<Mutex<Option<std::time::Instant>>>,
    /// Hardware panel info (serial, firmware) — None in headless mode.
    pub(crate) panel_info: Arc<Mutex<Option<ttlcd::DeviceInfo>>>,
    /// Unix timestamp (seconds) when this daemon instance started.
    pub(crate) daemon_start_secs: u64,
    /// Stopwatch state (start/stop/lap/reset).
    pub(crate) stopwatch: Arc<Mutex<StopwatchState>>,
    /// If true, the display thread skips rendering (panel shows the last frame).
    pub(crate) frozen: Arc<Mutex<bool>>,
    /// Thresholds for automatic CPU/temperature alerts.
    pub(crate) alert_thresholds: Arc<Mutex<AlertThresholds>>,
    /// Optional night-dim schedule.
    pub(crate) dim_schedule: Arc<Mutex<Option<DimSchedule>>>,
    /// Last Unix-second timestamp when a threshold alert fired (for rate-limiting).
    pub(crate) last_threshold_alert_at: Arc<Mutex<u64>>,
    /// Custom metrics for the Metrics display mode.
    pub(crate) metrics_title: Arc<Mutex<String>>,
    pub(crate) metrics_data: Arc<Mutex<Vec<Metric>>>,
    /// Cached weather snapshot for the Weather display mode.
    pub(crate) weather_data: Arc<Mutex<WeatherData>>,
    /// wttr.in location for auto-fetch (empty = disabled).
    pub(crate) weather_location: Arc<Mutex<String>>,
    /// Auto-fetch interval in seconds (0 = disabled).
    pub(crate) weather_auto_secs: Arc<Mutex<u64>>,
    /// Notes/bulletin board title and list for the Notes display mode.
    pub(crate) notes_title: Arc<Mutex<String>>,
    pub(crate) notes_items: Arc<Mutex<Vec<String>>>,
    /// Ring-buffer of recent render FPS values (last 60 samples) for the FPS sparkline.
    pub(crate) fps_history: Arc<Mutex<VecDeque<f32>>>,
    /// Webhook URL override — if set, outbound webhook notifications are forwarded here.
    pub(crate) webhook_forward_url: Arc<Mutex<Option<String>>>,
    /// QR code mode: payload (text/URL) and optional title.
    pub(crate) qr_payload: Arc<Mutex<String>>,
    pub(crate) qr_title: Arc<Mutex<String>>,
    /// Mode schedule: sorted list of (hour, minute, mode_name) entries.
    /// The scheduler thread checks every 30 s and switches mode when a time slot matches.
    pub(crate) mode_schedule: Arc<Mutex<Vec<ModeScheduleEntry>>>,
    /// Pomodoro timer state.
    pub(crate) pomodoro: Arc<Mutex<PomodoroState>>,
    /// Scrolling ticker text and color.
    pub(crate) ticker_text: Arc<Mutex<String>>,
    pub(crate) ticker_color: Arc<Mutex<String>>,
    /// Current scroll offset in pixels (advances each render tick).
    pub(crate) ticker_pos: Arc<Mutex<i32>>,
    /// World clock: list of (city_name, utc_offset_minutes) pairs.
    pub(crate) world_clock_cities: Arc<Mutex<Vec<(String, i32)>>>,
    /// Monotonically-increasing counter; incremented on every push_history(Alert) call.
    /// Used by the SSE stream to detect and push alert events to clients.
    pub(crate) alert_seq: Arc<Mutex<u64>>,
    /// Summary of the latest alert (seq, summary). None before first alert.
    pub(crate) last_alert_event: Arc<Mutex<Option<(u64, String)>>>,
    /// HTTP status monitor: list of configured monitors (label + URL).
    pub(crate) http_monitors: Arc<Mutex<Vec<HttpMonitorEntry>>>,
    /// Latest HTTP check results.
    pub(crate) http_results: Arc<Mutex<Vec<HttpMonitorResult>>>,
    /// Interval between HTTP health checks (seconds).
    pub(crate) http_check_interval_secs: Arc<Mutex<u64>>,
    /// Shell command for the ShellCmd display mode.
    pub(crate) shell_cmd_text: Arc<Mutex<String>>,
    /// Captured stdout of the last shell command run.
    pub(crate) shell_cmd_output: Arc<Mutex<String>>,
    /// True if the last shell command exited with code 0.
    pub(crate) shell_cmd_exit_ok: Arc<Mutex<bool>>,
    /// Unix-second timestamp of the last shell command run (0 = never).
    pub(crate) shell_cmd_run_at: Arc<Mutex<u64>>,
    /// Refresh interval for the shell command (seconds; 0 = manual only).
    pub(crate) shell_cmd_interval_secs: Arc<Mutex<u64>>,
    /// Ping monitor: list of configured hosts (label + host).
    pub(crate) ping_hosts: Arc<Mutex<Vec<PingHostEntry>>>,
    /// Latest ping results.
    pub(crate) ping_results: Arc<Mutex<Vec<PingResult>>>,
    /// Interval between ping checks (seconds; minimum 5).
    pub(crate) ping_interval_secs: Arc<Mutex<u64>>,
    /// Optional HTTP JSON metrics source (auto-fetches and updates the metrics display).
    pub(crate) metrics_source: Arc<Mutex<Option<MetricsSource>>>,
    /// Path of the file being tailed.
    pub(crate) log_tail_path: Arc<Mutex<String>>,
    /// Cached tail lines (most-recent last).
    pub(crate) log_tail_lines: Arc<Mutex<Vec<String>>>,
    /// Number of tail lines to keep.
    pub(crate) log_tail_n: Arc<Mutex<usize>>,
    /// Seconds between re-reads.
    pub(crate) log_tail_interval: Arc<Mutex<u64>>,
    /// Whether the last read succeeded.
    pub(crate) log_tail_ok: Arc<Mutex<bool>>,
    /// RSS feed: title and items (title, meta).
    pub(crate) rss_feed_title: Arc<Mutex<String>>,
    pub(crate) rss_feed_items: Arc<Mutex<Vec<(String, String)>>>,
    /// Whether the last RSS fetch succeeded.
    pub(crate) rss_feed_ok: Arc<Mutex<bool>>,
    /// RSS feed URL (empty = not configured).
    pub(crate) rss_feed_url: Arc<Mutex<String>>,
    /// Seconds between RSS re-fetches (0 = manual only).
    pub(crate) rss_feed_interval: Arc<Mutex<u64>>,
    /// Systemd units to watch (service names).
    pub(crate) service_units: Arc<Mutex<Vec<String>>>,
    /// Cached service status snapshots.
    pub(crate) service_statuses: Arc<Mutex<Vec<ttlcd::display::ServiceStatus>>>,
    /// Seconds between service status polls (minimum 5).
    pub(crate) service_poll_interval: Arc<Mutex<u64>>,
    /// Optional API key for authentication. When set, all requests must include
    /// `Authorization: Bearer <key>` (except `/static/`, `/api/stream`, and health-check).
    pub(crate) api_key: Arc<Mutex<Option<String>>>,
    /// ntfy.sh topic URL for push notifications. When set, every Alert-kind history entry
    /// will be POSTed to this URL as a plain-text notification.
    /// Example: "https://ntfy.sh/my-ttlcd-topic"
    pub(crate) ntfy_url: Arc<Mutex<Option<String>>>,
    /// Progress bar display state for the Progress mode.
    pub(crate) progress_state: Arc<Mutex<ttlcd::display::ProgressState>>,
    /// Unix-second timestamp of the last non-static API activity.
    /// Updated by the activity middleware on every API request.
    pub(crate) last_activity_secs: Arc<Mutex<u64>>,
    /// If > 0, auto-switch to `screensaver_mode_name` after this many seconds of inactivity.
    pub(crate) screensaver_timeout_secs: Arc<Mutex<u64>>,
    /// Which display mode to use as screensaver (default: "matrix_rain").
    pub(crate) screensaver_mode_name: Arc<Mutex<String>>,
    /// True while the screensaver mode is currently active due to inactivity.
    pub(crate) screensaver_active: Arc<Mutex<bool>>,
    /// True while the USB panel is believed to be connected (False after send errors).
    pub(crate) panel_connected: Arc<Mutex<bool>>,
    /// Flash message (temporary override) set by /api/flash.
    pub(crate) flash_message: Arc<Mutex<Option<FlashMessageState>>>,
    /// Night mode: switch to a dim mode between certain hours automatically.
    pub(crate) night_mode_enabled: Arc<Mutex<bool>>,
    pub(crate) night_mode_start_hour: Arc<Mutex<u8>>,
    pub(crate) night_mode_end_hour: Arc<Mutex<u8>>,
    pub(crate) night_mode_name: Arc<Mutex<String>>,
}

/// Temporary flash message shown instead of the current display mode.
#[derive(Clone, Debug)]
pub(crate) struct FlashMessageState {
    pub(crate) text: String,
    pub(crate) color: String,
    pub(crate) expires_at: std::time::Instant,
}

/// A single entry in the display-mode schedule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ModeScheduleEntry {
    /// Hour of day (0–23).
    pub(crate) hour: u8,
    /// Minute of hour (0–59).
    pub(crate) minute: u8,
    /// Display mode name (must be a valid DisplayMode string).
    pub(crate) mode: String,
    /// Optional human-readable label.
    #[serde(default)]
    pub(crate) label: String,
}

// ─── Pomodoro timer state ─────────────────────────────────────────────────────

pub(crate) fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Which phase the Pomodoro timer is currently in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            PomodoroPhase::Work => "work",
            PomodoroPhase::ShortBreak => "short_break",
            PomodoroPhase::LongBreak => "long_break",
        }
    }
}

/// Runtime state for the Pomodoro mode.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PomodoroState {
    pub(crate) phase: PomodoroPhase,
    /// Unix-ms timestamp when the current phase run was last started (0 = paused).
    pub(crate) phase_started_unix_ms: u64,
    /// Accumulated elapsed ms for the current phase (excluding the running portion).
    pub(crate) phase_elapsed_ms: u64,
    pub(crate) running: bool,
    /// Total completed work sessions (ever).
    pub(crate) completed_sessions: u32,
    // ── Configurable durations ────
    pub(crate) work_secs: u64,
    pub(crate) short_break_secs: u64,
    pub(crate) long_break_secs: u64,
    /// Number of work sessions between long breaks (default 4).
    pub(crate) long_break_interval: u32,
}

impl PomodoroState {
    pub(crate) fn new() -> Self {
        Self {
            phase: PomodoroPhase::Work,
            phase_started_unix_ms: 0,
            phase_elapsed_ms: 0,
            running: false,
            completed_sessions: 0,
            work_secs: 25 * 60,
            short_break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            long_break_interval: 4,
        }
    }

    pub(crate) fn phase_total_secs(&self) -> u64 {
        match self.phase {
            PomodoroPhase::Work => self.work_secs,
            PomodoroPhase::ShortBreak => self.short_break_secs,
            PomodoroPhase::LongBreak => self.long_break_secs,
        }
    }

    pub(crate) fn phase_elapsed_ms_now(&self) -> u64 {
        if self.running && self.phase_started_unix_ms > 0 {
            let now = now_unix_ms();
            self.phase_elapsed_ms + now.saturating_sub(self.phase_started_unix_ms)
        } else {
            self.phase_elapsed_ms
        }
    }

    pub(crate) fn remaining_secs(&self) -> i64 {
        let total_ms = self.phase_total_secs() as i64 * 1000;
        let elapsed = self.phase_elapsed_ms_now() as i64;
        // ceiling so we show "0:01" for the last partial second
        (total_ms - elapsed + 999) / 1000
    }

    /// Advance to the next phase and auto-start it.
    pub(crate) fn advance_phase(&mut self) {
        self.phase_elapsed_ms = 0;
        self.phase_started_unix_ms = 0;

        match self.phase {
            PomodoroPhase::Work => {
                self.completed_sessions += 1;
                if self
                    .completed_sessions
                    .is_multiple_of(self.long_break_interval)
                {
                    self.phase = PomodoroPhase::LongBreak;
                } else {
                    self.phase = PomodoroPhase::ShortBreak;
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                self.phase = PomodoroPhase::Work;
            }
        }

        // Auto-start the new phase
        self.running = true;
        self.phase_started_unix_ms = now_unix_ms();
    }
}
#[tokio::main]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_daemon(
    vid: u16,
    pid: u16,
    port: u16,
    bind: String,
    mode: String,
    quality: u8,
    headless: bool,
    rotate_deg: u16,
    rotation_mode_names: Vec<String>,
    rotation_interval: u64,
) -> Result<()> {
    use actix_web::{web, App, HttpServer};

    let initial_mode: DisplayMode = mode.parse().unwrap_or(DisplayMode::ClockSysinfo);

    // Parse rotation modes (ignore unknown and transient ones)
    let rotation_modes: Vec<DisplayMode> = rotation_mode_names
        .iter()
        .filter_map(|s| DisplayMode::from_str_name(s))
        .filter(|m| !m.is_transient())
        .collect();

    if rotation_interval > 0 && !rotation_modes.is_empty() {
        log::info!(
            "auto-rotation enabled: {} modes, {}s interval",
            rotation_modes.len(),
            rotation_interval
        );
    }

    // Load persisted presets
    let saved_presets = load_presets_from_disk();
    if !saved_presets.is_empty() {
        log::info!("loaded {} preset(s) from disk", saved_presets.len());
    }

    // Load persistent daemon state (last mode, quality, rotate, brightness).
    // CLI / config arguments already take precedence — only apply if those
    // fields still hold their defaults (i.e. the user did not override them).
    let persisted = load_persisted_state();
    let initial_mode = if let Some(ref saved_mode) = persisted.mode {
        // Only restore if the caller passed the compiled default "clock_sysinfo"
        // (i.e. the user did not explicitly choose a mode on the command line).
        if mode == "clock_sysinfo" {
            DisplayMode::from_str_name(saved_mode).unwrap_or(initial_mode)
        } else {
            initial_mode
        }
    } else {
        initial_mode
    };
    let quality = persisted.quality.unwrap_or(quality);
    let rotate_deg = persisted.rotate_deg.unwrap_or(rotate_deg);
    let init_brightness = persisted.brightness.unwrap_or(1.0);
    if let Some(pet) = &persisted.pet {
        pet.apply();
    }
    if persisted.mode.is_some() {
        log::info!(
            "restored persisted state: mode={} quality={} rotate={}°",
            initial_mode.as_str(),
            quality,
            rotate_deg
        );
    }

    let state = DaemonState {
        prev_mode: Arc::new(Mutex::new(initial_mode.clone())),
        mode: Arc::new(Mutex::new(initial_mode)),
        pending_jpeg: Arc::new(Mutex::new(None)),
        last_frame: Arc::new(Mutex::new(None)),
        frame_seq: Arc::new(Mutex::new(0)),
        pending_text: Arc::new(Mutex::new(None)),
        presets: Arc::new(Mutex::new(saved_presets)),
        slideshow_paths: Arc::new(Mutex::new(Vec::new())),
        slideshow_idx: Arc::new(Mutex::new(0)),
        slideshow_interval: Arc::new(Mutex::new(5)),
        rotation_modes: Arc::new(Mutex::new(rotation_modes)),
        rotation_interval: Arc::new(Mutex::new(rotation_interval)),
        quality: Arc::new(Mutex::new(quality)),
        vid,
        pid,
        headless,
        rotate_deg: Arc::new(Mutex::new(rotate_deg)),
        history: Arc::new(Mutex::new(VecDeque::with_capacity(50))),
        countdown: Arc::new(Mutex::new(None)),
        auto_clear_at: Arc::new(Mutex::new(None)),
        pending_scheduled: Arc::new(Mutex::new(0)),
        dim_factor: Arc::new(Mutex::new(init_brightness)),
        render_fps: Arc::new(Mutex::new(0.0)),
        render_fps_last: Arc::new(Mutex::new(None)),
        panel_info: Arc::new(Mutex::new(None)),
        daemon_start_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        stopwatch: Arc::new(Mutex::new(StopwatchState::new())),
        frozen: Arc::new(Mutex::new(false)),
        alert_thresholds: Arc::new(Mutex::new(AlertThresholds::default())),
        dim_schedule: Arc::new(Mutex::new(None)),
        last_threshold_alert_at: Arc::new(Mutex::new(0)),
        metrics_title: Arc::new(Mutex::new(String::new())),
        metrics_data: Arc::new(Mutex::new(Vec::new())),
        weather_data: Arc::new(Mutex::new(WeatherData::default())),
        weather_location: Arc::new(Mutex::new(String::new())),
        weather_auto_secs: Arc::new(Mutex::new(0)),
        notes_title: Arc::new(Mutex::new(String::new())),
        notes_items: Arc::new(Mutex::new(Vec::new())),
        fps_history: Arc::new(Mutex::new(VecDeque::with_capacity(60))),
        webhook_forward_url: Arc::new(Mutex::new(None)),
        qr_payload: Arc::new(Mutex::new(String::new())),
        qr_title: Arc::new(Mutex::new("QR Code".to_string())),
        mode_schedule: Arc::new(Mutex::new(Vec::new())),
        pomodoro: Arc::new(Mutex::new(PomodoroState::new())),
        ticker_text: Arc::new(Mutex::new(String::new())),
        ticker_color: Arc::new(Mutex::new("blue".to_string())),
        ticker_pos: Arc::new(Mutex::new(0i32)),
        world_clock_cities: Arc::new(Mutex::new(Vec::new())),
        alert_seq: Arc::new(Mutex::new(0)),
        last_alert_event: Arc::new(Mutex::new(None)),
        http_monitors: Arc::new(Mutex::new(Vec::new())),
        http_results: Arc::new(Mutex::new(Vec::new())),
        http_check_interval_secs: Arc::new(Mutex::new(30)),
        shell_cmd_text: Arc::new(Mutex::new(String::new())),
        shell_cmd_output: Arc::new(Mutex::new(String::new())),
        shell_cmd_exit_ok: Arc::new(Mutex::new(true)),
        shell_cmd_run_at: Arc::new(Mutex::new(0u64)),
        shell_cmd_interval_secs: Arc::new(Mutex::new(10)),
        ping_hosts: Arc::new(Mutex::new(Vec::new())),
        ping_results: Arc::new(Mutex::new(Vec::new())),
        ping_interval_secs: Arc::new(Mutex::new(10)),
        metrics_source: Arc::new(Mutex::new(None)),
        log_tail_path: Arc::new(Mutex::new(String::new())),
        log_tail_lines: Arc::new(Mutex::new(Vec::new())),
        log_tail_n: Arc::new(Mutex::new(16)),
        log_tail_interval: Arc::new(Mutex::new(3)),
        log_tail_ok: Arc::new(Mutex::new(false)),
        rss_feed_title: Arc::new(Mutex::new(String::new())),
        rss_feed_items: Arc::new(Mutex::new(Vec::new())),
        rss_feed_ok: Arc::new(Mutex::new(false)),
        rss_feed_url: Arc::new(Mutex::new(String::new())),
        rss_feed_interval: Arc::new(Mutex::new(300)),
        service_units: Arc::new(Mutex::new(Vec::new())),
        service_statuses: Arc::new(Mutex::new(Vec::new())),
        service_poll_interval: Arc::new(Mutex::new(10)),
        api_key: Arc::new(Mutex::new(None)),
        ntfy_url: Arc::new(Mutex::new(None)),
        progress_state: Arc::new(Mutex::new(ttlcd::display::ProgressState::default())),
        last_activity_secs: Arc::new(Mutex::new(0u64)),
        screensaver_timeout_secs: Arc::new(Mutex::new(0u64)),
        screensaver_mode_name: Arc::new(Mutex::new("matrix_rain".to_string())),
        screensaver_active: Arc::new(Mutex::new(false)),
        panel_connected: Arc::new(Mutex::new(!headless)),
        flash_message: Arc::new(Mutex::new(None)),
        night_mode_enabled: Arc::new(Mutex::new(false)),
        night_mode_start_hour: Arc::new(Mutex::new(22)), // 10 PM
        night_mode_end_hour: Arc::new(Mutex::new(7)),    // 7 AM
        night_mode_name: Arc::new(Mutex::new("minimal_clock".into())),
    };

    let panel: Arc<Mutex<Option<Panel>>> = if headless {
        log::info!("daemon: headless mode — USB device not opened");
        Arc::new(Mutex::new(None))
    } else {
        match Panel::open(vid, pid) {
            Err(e) => {
                // Auto-fallback: if device not found at startup, continue in headless mode
                // so the web UI is still accessible. The display loop will retry.
                log::warn!(
                    "daemon: USB device {vid:04x}:{pid:04x} not available at startup: {e:#}"
                );
                log::warn!(
                    "daemon: starting in automatic headless mode — display loop will retry USB"
                );
                *state.panel_connected.lock().unwrap() = false;
                push_history(
                    &state,
                    HistoryKind::System,
                    format!("USB device not found at startup — auto headless: {e}"),
                );
                Arc::new(Mutex::new(None))
            }
            Ok(mut p) => {
                p.init()?;
                log::info!(
                    "daemon: panel ready (serial={}); mode={}; rotate={}°; port={}",
                    p.info.serial,
                    state.mode.lock().unwrap().as_str(),
                    rotate_deg,
                    port
                );
                // Cache hardware info in state so web handlers can read it
                *state.panel_info.lock().unwrap() = Some(p.info.clone());
                // Send boot splash as the very first frame
                let splash = render_boot_splash(env!("CARGO_PKG_VERSION"));
                if let Ok(jpeg) = encode_for_panel(&splash, quality) {
                    let _ = p.send_jpeg(&jpeg);
                }
                Arc::new(Mutex::new(Some(p)))
            }
        }
    };

    if headless {
        log::info!(
            "daemon: headless; mode={}; web UI → http://localhost:{port}/",
            state.mode.lock().unwrap().as_str()
        );
    }

    // ── Display thread ────────────────────────────────────────────────────
    {
        let panel2 = Arc::clone(&panel);
        let state2 = state.clone();
        thread::spawn(move || display_loop(panel2, state2));
    }

    // ── Auto-rotation thread ──────────────────────────────────────────────
    {
        let state3 = state.clone();
        thread::spawn(move || rotation_loop(state3));
    }

    // ── Alert threshold monitor thread ───────────────────────────────────
    {
        let state4 = state.clone();
        thread::spawn(move || threshold_monitor_loop(state4));
    }

    // ── Dim schedule thread ───────────────────────────────────────────────
    {
        let state5 = state.clone();
        thread::spawn(move || dim_schedule_loop(state5));
    }

    // ── Mode schedule thread ──────────────────────────────────────────────
    {
        // Load persisted schedule entries from disk before starting the loop
        *state.mode_schedule.lock().unwrap() = load_mode_schedule();
        let state6 = state.clone();
        thread::spawn(move || mode_schedule_loop(state6));
    }

    // ── World clock persistence ────────────────────────────────────────────
    {
        let loaded = load_world_clock();
        if !loaded.is_empty() {
            *state.world_clock_cities.lock().unwrap() = loaded;
        }
    }

    // ── HTTP monitors persistence ──────────────────────────────────────────
    {
        let monitors = load_http_monitors();
        if !monitors.is_empty() {
            *state.http_monitors.lock().unwrap() = monitors;
        }
    }

    // ── Ping hosts persistence ────────────────────────────────────────────
    {
        let hosts = load_ping_hosts();
        if !hosts.is_empty() {
            *state.ping_hosts.lock().unwrap() = hosts;
        }
    }

    // ── Load new settings (weather, log_tail, metrics_source) ────────────
    {
        let ns = load_new_settings();
        if !ns.weather_location.is_empty() {
            *state.weather_location.lock().unwrap() = ns.weather_location;
            *state.weather_auto_secs.lock().unwrap() = ns.weather_auto_secs;
        }
        if !ns.log_tail_path.is_empty() {
            *state.log_tail_path.lock().unwrap() = ns.log_tail_path;
            *state.log_tail_n.lock().unwrap() = ns.log_tail_n.max(1);
            *state.log_tail_interval.lock().unwrap() = ns.log_tail_interval.max(1);
        }
        if let Some(src) = ns.metrics_source {
            *state.metrics_source.lock().unwrap() = Some(src);
        }
        if !ns.rss_feed_url.is_empty() {
            *state.rss_feed_url.lock().unwrap() = ns.rss_feed_url;
            *state.rss_feed_interval.lock().unwrap() = ns.rss_feed_interval.max(30);
        }
        if !ns.service_units.is_empty() {
            *state.service_units.lock().unwrap() = ns.service_units;
            *state.service_poll_interval.lock().unwrap() = ns.service_poll_interval.max(5);
        }
        if !ns.shell_cmd.is_empty() {
            *state.shell_cmd_text.lock().unwrap() = ns.shell_cmd;
            *state.shell_cmd_interval_secs.lock().unwrap() = ns.shell_cmd_interval.max(1);
        }
        if !ns.http_monitors.is_empty() {
            *state.http_monitors.lock().unwrap() = ns.http_monitors;
        }
        if !ns.ping_hosts.is_empty() {
            *state.ping_hosts.lock().unwrap() = ns.ping_hosts;
        }
        if let Some(ntfy) = ns.ntfy_url {
            *state.ntfy_url.lock().unwrap() = Some(ntfy);
        }
        if ns.screensaver_timeout_secs > 0 {
            *state.screensaver_timeout_secs.lock().unwrap() = ns.screensaver_timeout_secs;
        }
        if !ns.screensaver_mode_name.is_empty() {
            *state.screensaver_mode_name.lock().unwrap() = ns.screensaver_mode_name;
        }
        if ns.alert_cpu_pct > 0.0 || ns.alert_temp_c > 0.0 {
            let mut t = state.alert_thresholds.lock().unwrap();
            if ns.alert_cpu_pct >= 0.0 {
                t.cpu_pct = ns.alert_cpu_pct;
            }
            if ns.alert_temp_c >= 0.0 {
                t.temp_c = ns.alert_temp_c;
            }
            if ns.alert_cooldown_secs > 0 {
                t.cooldown_secs = ns.alert_cooldown_secs;
            }
        }
    }

    // ── HTTP health-check background thread ───────────────────────────────
    {
        let state_hc = state.clone();
        tokio::spawn(async move { http_check_loop(state_hc).await });
    }

    // ── Shell-command background thread ───────────────────────────────────
    {
        let state_sc = state.clone();
        tokio::spawn(async move { shell_cmd_loop(state_sc).await });
    }

    // ── Ping monitor background thread ────────────────────────────────────
    {
        let state_pm = state.clone();
        tokio::spawn(async move { ping_monitor_loop(state_pm).await });
    }

    // ── Metrics source auto-fetch background thread ─────────────────────
    {
        let state_ms = state.clone();
        tokio::spawn(async move { metrics_source_loop(state_ms).await });
    }

    // ── Weather auto-fetch background thread ─────────────────────────────
    {
        let state_wf = state.clone();
        tokio::spawn(async move { weather_auto_loop(state_wf).await });
    }

    // ── Log tail background thread ────────────────────────────────────────
    {
        let state_lt = state.clone();
        tokio::spawn(async move { log_tail_loop(state_lt).await });
    }

    // ── RSS feed background thread ────────────────────────────────────────
    {
        let state_rss = state.clone();
        tokio::spawn(async move { rss_feed_loop(state_rss).await });
    }

    // ── Service status background thread ─────────────────────────────────
    {
        let state_svc = state.clone();
        tokio::spawn(async move { service_status_loop(state_svc).await });
    }

    // ── Screensaver idle-detection thread ────────────────────────────────
    {
        // Seed last_activity_secs so screensaver doesn't fire immediately on startup
        *state.last_activity_secs.lock().unwrap() = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let state_ss = state.clone();
        thread::spawn(move || screensaver_loop(state_ss));
    }

    // ── HTTP server ────────────────────────────────────────────────────────
    let web_state = web::Data::new(state.clone());

    let server = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::from_fn(auth_middleware))
            // Registered last, so it runs first.  No CORS headers are sent:
            // the UI is same-origin and nothing else should script this API.
            .wrap(actix_web::middleware::from_fn(origin_guard))
            .app_data(web_state.clone())
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::PayloadConfig::default().limit(16 * 1024 * 1024))
            // Core API
            .route("/api/status", web::get().to(api_status))
            .route("/api/mode", web::post().to(api_set_mode))
            .route("/api/image", web::post().to(api_upload_image))
            .route("/api/image/path", web::post().to(api_image_path))
            .route("/api/image/url", web::post().to(api_image_url))
            .route("/api/render", web::post().to(api_render))
            .route("/api/sysinfo", web::get().to(api_sysinfo_json))
            .route("/api/processes", web::get().to(api_processes_json))
            .route("/api/disks", web::get().to(api_disks_json))
            .route("/api/preview", web::get().to(api_preview_frame))
            .route("/api/preview.png", web::get().to(api_preview_frame_png))
            .route("/api/render/{mode}", web::get().to(api_render_mode_preview))
            .route("/api/text", web::post().to(api_text))
            .route("/api/clear", web::post().to(api_clear))
            // Presets API (full CRUD)
            .route("/api/presets", web::get().to(api_presets_list))
            .route("/api/presets", web::post().to(api_presets_save))
            .route("/api/presets/{name}", web::get().to(api_presets_get))
            .route("/api/presets/{name}", web::post().to(api_presets_apply))
            .route("/api/presets/{name}", web::delete().to(api_presets_delete))
            .route("/api/presets/export", web::get().to(api_presets_export))
            .route("/api/presets/import", web::post().to(api_presets_import))
            // Alert shorthand & screenshot download
            .route("/api/alert", web::post().to(api_alert))
            .route("/api/canvas", web::post().to(api_canvas))
            .route("/api/screenshot", web::get().to(api_screenshot))
            // Mode listing and navigation
            .route("/api/modes", web::get().to(api_modes_list))
            .route("/api/mode/random", web::post().to(api_mode_random))
            .route("/api/mode/next", web::post().to(api_mode_next))
            .route("/api/mode/prev", web::post().to(api_mode_prev))
            // Notification history
            .route("/api/history", web::get().to(api_history))
            .route("/api/history", web::delete().to(api_history_clear))
            // Runtime quality and orientation
            .route("/api/quality", web::post().to(api_set_quality))
            .route("/api/orient", web::post().to(api_set_orient))
            // Health check, blank, countdown
            .route("/health", web::get().to(api_health))
            .route("/api/benchmark", web::get().to(api_benchmark))
            .route("/ping", web::get().to(api_ping))
            .route("/api/ping", web::get().to(api_ping))
            .route("/api/blank", web::post().to(api_blank))
            .route("/api/display/reset", web::post().to(api_display_reset))
            .route("/api/countdown", web::post().to(api_countdown))
            // Prometheus metrics
            .route("/metrics", web::get().to(api_metrics))
            // Clear all presets
            .route("/api/presets", web::delete().to(api_presets_clear))
            // Slideshow & rotation
            .route("/api/slideshow", web::post().to(api_slideshow))
            .route("/api/slideshow/next", web::post().to(api_slideshow_next))
            .route("/api/slideshow/prev", web::post().to(api_slideshow_prev))
            .route("/api/rotation", web::post().to(api_rotation))
            .route("/api/demo", web::post().to(api_start_demo))
            .route("/api/demo", web::delete().to(api_stop_demo))
            // Convenience endpoints
            .route("/api/notify", web::post().to(api_notify))
            .route("/api/webhook", web::post().to(api_webhook))
            .route("/api/config", web::get().to(api_get_config))
            .route("/api/schedule", web::post().to(api_schedule))
            .route("/api/info", web::get().to(api_info))
            // Server-Sent Events
            .route("/api/events", web::get().to(api_events))
            // MJPEG live stream
            .route("/api/stream", web::get().to(api_mjpeg_stream))
            // Test pattern
            .route("/api/test", web::post().to(api_test_pattern))
            // Slideshow extra controls
            .route("/api/slideshow/images", web::get().to(api_slideshow_images))
            .route(
                "/api/slideshow/shuffle",
                web::post().to(api_slideshow_shuffle),
            )
            // Brightness control
            .route("/api/brightness", web::get().to(api_get_brightness))
            .route("/api/brightness", web::post().to(api_set_brightness))
            // Countdown cancel
            .route("/api/countdown", web::delete().to(api_cancel_countdown))
            // Network + temperature JSON
            .route("/api/network", web::get().to(api_network_json))
            .route("/api/temperatures", web::get().to(api_temperatures_json))
            // History ring-buffer snapshots
            .route("/api/cpu/history", web::get().to(api_cpu_history))
            .route("/api/net/history", web::get().to(api_net_history))
            // PNG screenshot
            .route("/api/screenshot.png", web::get().to(api_screenshot_png))
            // Rotation GET
            .route("/api/rotation", web::get().to(api_rotation_get))
            // SVG status badge
            .route("/api/status.svg", web::get().to(api_status_badge))
            // Version and uptime
            .route("/api/version", web::get().to(api_version))
            .route("/api/uptime", web::get().to(api_uptime))
            // Panel hardware info
            .route("/api/panel/info", web::get().to(api_panel_info))
            .route("/api/panel/reconnect", web::post().to(api_panel_reconnect))
            .route("/api/usb/devices", web::get().to(api_usb_devices))
            .route("/api/flash", web::post().to(api_flash_message))
            // Large text banner (POST to display, GET to fetch last banner)
            .route("/api/banner", web::post().to(api_banner))
            // Modes with descriptions
            .route("/api/modes/info", web::get().to(api_modes_info))
            // Reload persisted daemon state from disk
            .route("/api/state/reload", web::post().to(api_state_reload))
            // Stopwatch
            .route("/api/stopwatch", web::get().to(api_stopwatch_get))
            .route("/api/stopwatch/start", web::post().to(api_stopwatch_start))
            .route("/api/stopwatch/stop", web::post().to(api_stopwatch_stop))
            .route("/api/stopwatch/lap", web::post().to(api_stopwatch_lap))
            .route("/api/stopwatch/reset", web::post().to(api_stopwatch_reset))
            // Display freeze / unfreeze
            .route("/api/display/freeze", web::post().to(api_display_freeze))
            .route(
                "/api/display/unfreeze",
                web::post().to(api_display_unfreeze),
            )
            .route("/api/display/frozen", web::get().to(api_display_frozen))
            // Alert thresholds
            .route("/api/alerts/thresholds", web::get().to(api_get_thresholds))
            .route("/api/alerts/thresholds", web::post().to(api_set_thresholds))
            // Night-dim schedule
            .route("/api/dim/schedule", web::get().to(api_get_dim_schedule))
            .route("/api/dim/schedule", web::post().to(api_set_dim_schedule))
            .route(
                "/api/dim/schedule",
                web::delete().to(api_clear_dim_schedule),
            )
            // Custom metrics dashboard
            .route("/api/metrics", web::get().to(api_get_metrics))
            .route("/api/metrics", web::post().to(api_push_metrics))
            .route("/api/metrics", web::delete().to(api_clear_metrics))
            // Display profiles
            .route("/api/profiles", web::get().to(api_profiles_list))
            .route("/api/profiles", web::post().to(api_profiles_save))
            .route(
                "/api/profiles/{name}/apply",
                web::post().to(api_profiles_apply),
            )
            .route(
                "/api/profiles/{name}",
                web::delete().to(api_profiles_delete),
            )
            // Comprehensive system snapshot
            .route("/api/sys/snapshot", web::get().to(api_sys_snapshot))
            .route("/api/sys/os", web::get().to(api_sys_os))
            .route("/api/sys/net", web::get().to(api_sys_net))
            // Weather display
            .route("/api/weather", web::get().to(api_get_weather))
            .route("/api/weather", web::post().to(api_update_weather))
            .route("/api/weather", web::delete().to(api_clear_weather))
            .route("/api/weather/fetch", web::post().to(api_weather_fetch_now))
            .route("/api/weather/auto", web::put().to(api_weather_auto))
            .route("/api/weather/auto", web::delete().to(api_weather_auto_stop))
            // Notes / bulletin board
            .route("/api/notes", web::get().to(api_get_notes))
            .route("/api/notes", web::post().to(api_push_notes))
            .route("/api/notes", web::delete().to(api_clear_notes))
            // QR Code mode
            .route("/api/qr", web::get().to(api_get_qr))
            .route("/api/qr", web::post().to(api_set_qr))
            .route("/api/qr", web::delete().to(api_clear_qr))
            // Mode schedule
            .route("/api/schedule/mode", web::get().to(api_get_mode_schedule))
            .route("/api/schedule/mode", web::post().to(api_add_mode_schedule))
            .route(
                "/api/schedule/mode",
                web::delete().to(api_clear_mode_schedule),
            )
            .route(
                "/api/schedule/mode/{index}",
                web::delete().to(api_remove_mode_schedule_entry),
            )
            // Settings export/import
            .route("/api/settings/export", web::get().to(api_settings_export))
            .route("/api/settings/import", web::post().to(api_settings_import))
            // FPS history sparkline
            .route("/api/fps/history", web::get().to(api_fps_history))
            // Webhook config (GET current URL, POST to set, DELETE to clear)
            .route("/api/webhook/config", web::get().to(api_get_webhook_config))
            .route(
                "/api/webhook/config",
                web::post().to(api_set_webhook_config),
            )
            .route(
                "/api/webhook/config",
                web::delete().to(api_clear_webhook_config),
            )
            // Pomodoro timer
            .route("/api/pomodoro", web::get().to(api_get_pomodoro))
            .route("/api/pomodoro/start", web::post().to(api_pomodoro_start))
            .route("/api/pomodoro/pause", web::post().to(api_pomodoro_pause))
            .route("/api/pomodoro/skip", web::post().to(api_pomodoro_skip))
            .route("/api/pomodoro/reset", web::post().to(api_pomodoro_reset))
            .route("/api/pomodoro/stop", web::post().to(api_pomodoro_stop))
            .route("/api/pomodoro/config", web::post().to(api_pomodoro_config))
            // Scrolling ticker
            .route("/api/ticker", web::get().to(api_get_ticker))
            .route("/api/ticker", web::post().to(api_set_ticker))
            .route("/api/ticker", web::delete().to(api_clear_ticker))
            // World clock
            .route("/api/world_clock", web::get().to(api_get_world_clock))
            .route("/api/world_clock", web::post().to(api_set_world_clock))
            .route("/api/world_clock", web::delete().to(api_clear_world_clock))
            // HTTP JSON metrics source
            .route("/api/metrics/source", web::get().to(api_get_metrics_source))
            .route(
                "/api/metrics/source",
                web::post().to(api_set_metrics_source),
            )
            .route(
                "/api/metrics/source",
                web::delete().to(api_clear_metrics_source),
            )
            .route(
                "/api/metrics/source/fetch",
                web::post().to(api_fetch_metrics_source_now),
            )
            // Battery info
            .route("/api/battery", web::get().to(api_battery_json))
            // HTTP status monitor
            .route("/api/http_status", web::get().to(api_get_http_status))
            .route("/api/http_status", web::post().to(api_add_http_monitor))
            .route(
                "/api/http_status/{index}",
                web::delete().to(api_delete_http_monitor),
            )
            .route(
                "/api/http_status/interval",
                web::put().to(api_set_http_interval),
            )
            // Shell command
            .route("/api/shell_cmd", web::get().to(api_get_shell_cmd))
            .route("/api/shell_cmd", web::post().to(api_set_shell_cmd))
            .route("/api/shell_cmd", web::delete().to(api_clear_shell_cmd))
            .route("/api/shell_cmd/run", web::post().to(api_run_shell_cmd_now))
            // Ping monitor
            .route("/api/ping_monitor", web::get().to(api_get_ping))
            .route("/api/ping_monitor", web::post().to(api_add_ping_host))
            .route(
                "/api/ping_monitor/{index}",
                web::delete().to(api_delete_ping_host),
            )
            .route(
                "/api/ping_monitor/interval",
                web::put().to(api_set_ping_interval),
            )
            // Log tail viewer
            .route("/api/log_tail", web::get().to(api_get_log_tail))
            .route("/api/log_tail", web::post().to(api_set_log_tail))
            .route("/api/log_tail", web::delete().to(api_clear_log_tail))
            // RSS feed
            .route("/api/rss", web::get().to(api_get_rss))
            .route("/api/rss", web::post().to(api_set_rss))
            .route("/api/rss", web::delete().to(api_clear_rss))
            .route("/api/rss/fetch", web::post().to(api_fetch_rss))
            // Service status monitor
            .route("/api/services", web::get().to(api_get_services))
            .route("/api/services", web::post().to(api_set_services))
            .route("/api/services", web::delete().to(api_clear_services))
            // Docker containers
            .route("/api/docker", web::get().to(api_get_docker))
            // Logged-in user sessions
            .route("/api/sessions", web::get().to(api_get_sessions))
            // API key authentication management
            .route("/api/auth/key", web::post().to(api_set_auth_key))
            .route("/api/auth/key", web::delete().to(api_clear_auth_key))
            .route("/api/auth/status", web::get().to(api_get_auth_status))
            // Progress bar display
            .route("/api/progress", web::get().to(api_get_progress))
            .route("/api/progress", web::post().to(api_set_progress))
            .route("/api/progress", web::delete().to(api_clear_progress))
            // ntfy.sh push notification config
            .route("/api/ntfy/config", web::get().to(api_get_ntfy_config))
            .route("/api/ntfy/config", web::post().to(api_set_ntfy_config))
            .route("/api/ntfy/config", web::delete().to(api_clear_ntfy_config))
            .route("/api/ntfy/test", web::post().to(api_ntfy_test))
            // Batch API
            .route("/api/batch", web::post().to(api_batch))
            // Screensaver
            .route("/api/screensaver", web::get().to(api_get_screensaver))
            .route("/api/screensaver", web::post().to(api_set_screensaver))
            .route("/api/screensaver", web::delete().to(api_clear_screensaver))
            .route(
                "/api/screensaver/wake",
                web::post().to(api_screensaver_wake),
            )
            // Pet cat
            .route("/api/pet", web::get().to(api_get_pet))
            .route("/api/pet", web::post().to(api_set_pet))
            .route("/api/night", web::get().to(api_get_night_mode))
            .route("/api/night", web::post().to(api_set_night_mode))
            .route("/api/modes/catalog", web::get().to(api_modes_catalog))
            // API docs
            .route("/api/docs", web::get().to(api_docs))
            // Web UI
            .route("/", web::get().to(web_ui_index))
            .route("/static/app.css", web::get().to(web_ui_css))
            .route("/static/ttlcd_web.js", web::get().to(web_ui_js))
            .route("/static/boot.js", web::get().to(web_ui_boot))
            .route("/static/ttlcd_web_bg.wasm", web::get().to(web_ui_wasm))
            .route("/manifest.json", web::get().to(web_ui_manifest))
    })
    .bind((require_loopback(&bind)?, port))?
    .run();

    let handle = server.handle();
    // ── Graceful shutdown handler ─────────────────────────────────────────────
    // On SIGTERM or Ctrl-C: blank the LCD panel, then stop the HTTP server.
    {
        let panel_sd = Arc::clone(&panel);
        let quality_sd = *state.quality.lock().unwrap();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            log::info!("ttlcd: shutdown signal — blanking panel");
            {
                let mut guard = panel_sd.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    // Render a clean "shutdown" frame
                    let goodbye = render_boot_splash(&format!(
                        "v{} — shutting down",
                        env!("CARGO_PKG_VERSION")
                    ));
                    if let Ok(jpeg) = encode_for_panel(&goodbye, quality_sd) {
                        let _ = p.send_jpeg(&jpeg);
                        // Brief pause so the user sees the goodbye frame
                        std::thread::sleep(std::time::Duration::from_millis(800));
                    }
                    // Then blank the panel (all-black)
                    let blank_img = image::DynamicImage::ImageRgb8(image::RgbImage::new(480, 480));
                    if let Ok(jpeg) = encode_for_panel(&blank_img, 50) {
                        let _ = p.send_jpeg(&jpeg);
                    }
                }
            }
            handle.stop(true).await;
        });
    }

    server.await.context("HTTP server error")?;

    Ok(())
}

// ─── Store a new frame ────────────────────────────────────────────────────────

pub(crate) fn store_frame(state: &DaemonState, jpeg: Vec<u8>) -> Vec<u8> {
    *state.last_frame.lock().unwrap() = Some(jpeg.clone());
    *state.frame_seq.lock().unwrap() += 1;
    jpeg
}

/// Update the EMA render-FPS counter.
/// Uses an `Arc<Mutex<Option<Instant>>>` stored in `DaemonState` so there are
/// no thread-local initialization quirks.
pub(crate) fn update_fps(state: &DaemonState) {
    let now = std::time::Instant::now();
    let mut last_opt = state.render_fps_last.lock().unwrap();
    if let Some(last) = *last_opt {
        let elapsed = now.duration_since(last).as_secs_f32();
        // Sanity bounds: only accept 0.01 s – 60 s intervals
        if (0.01..=60.0).contains(&elapsed) {
            let new_fps = 1.0 / elapsed;
            let mut fps = state.render_fps.lock().unwrap();
            *fps = if *fps == 0.0 {
                new_fps
            } else {
                0.2 * new_fps + 0.8 * (*fps)
            };
            // Append to FPS history ring-buffer (capped at 60 samples)
            let mut fh = state.fps_history.lock().unwrap();
            if fh.len() >= 60 {
                fh.pop_front();
            }
            fh.push_back(*fps);
        }
    }
    *last_opt = Some(now);
}
