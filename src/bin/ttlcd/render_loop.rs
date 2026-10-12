//! The loop that renders the current mode to the panel, and the timers that change what it shows.

use super::*;

// ─── Display loop ─────────────────────────────────────────────────────────────

/// True if `host` — a `Host` header value, with or without a port — names
/// this machine's loopback interface.
pub(crate) fn is_loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        // "[::1]" or "[::1]:7070"
        match rest.split_once(']') {
            Some((addr, port)) if port.is_empty() || port.starts_with(':') => addr,
            _ => return false,
        }
    } else {
        host.rsplit_once(':').map_or(host, |(name, _port)| name)
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// True if `origin` — an `Origin` header value — is a page served from this
/// machine's loopback interface.
pub(crate) fn is_loopback_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|host| !host.contains('/') && is_loopback_host(host))
}

/// Why a request must be refused, judged by the headers a browser attaches,
/// or `None` if it may proceed.
///
/// Binding to loopback keeps other machines out, but not the user's own
/// browser: any web page can make it send requests to `127.0.0.1`.  Two
/// attacks are blocked here:
/// - cross-site requests (a page on another origin calling the API), by
///   `Origin` / `Sec-Fetch-Site`;
/// - DNS rebinding (a hostile domain re-pointed at 127.0.0.1 so its page
///   becomes "same-origin"), by requiring a loopback `Host`.
///
/// Non-browser clients (curl, scripts) send none of these headers apart from
/// `Host` and are unaffected.
pub(crate) fn cross_origin_violation(
    host: Option<&str>,
    origin: Option<&str>,
    sec_fetch_site: Option<&str>,
) -> Option<&'static str> {
    if host.is_some_and(|h| !is_loopback_host(h)) {
        return Some("Host header is not a loopback address");
    }
    if origin.is_some_and(|o| !is_loopback_origin(o)) {
        return Some("cross-origin requests are not allowed");
    }
    if sec_fetch_site == Some("cross-site") {
        return Some("cross-site requests are not allowed");
    }
    None
}

/// Middleware applying [`cross_origin_violation`] to every request.
pub(crate) async fn origin_guard(
    req: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, actix_web::Error> {
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
    if let Some(reason) =
        cross_origin_violation(header("host"), header("origin"), header("sec-fetch-site"))
    {
        log::warn!("refused {} {}: {reason}", req.method(), req.path());
        return Err(actix_web::error::ErrorForbidden(reason));
    }
    next.call(req).await
}

/// Compare two secrets without leaking, through timing, how many leading
/// bytes match.
pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// A host name or IP address that is safe to hand to `ping` as an argument
/// (in particular, it cannot be mistaken for an option).
pub(crate) fn is_valid_ping_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_:%".contains(&b))
}

/// A systemd unit name that is safe to hand to `systemctl` as an argument.
pub(crate) fn is_valid_unit_name(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 255
        && !unit.starts_with('-')
        && unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_:@\\".contains(&b))
}

/// Largest image the daemon will accept, uploaded or downloaded.
pub(crate) const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

/// Largest JSON or RSS document the daemon will download.
pub(crate) const MAX_REMOTE_TEXT_BYTES: usize = 2 * 1024 * 1024;

/// Read a response body, giving up once it exceeds `max` bytes rather than
/// buffering whatever the remote end chooses to send.
pub(crate) async fn read_body_capped(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    if resp.content_length().is_some_and(|len| len > max as u64) {
        anyhow::bail!("response is larger than {max} bytes");
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len() + chunk.len() > max {
            anyhow::bail!("response is larger than {max} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The web UI / API is unauthenticated, so it is only ever served on a
/// loopback address.  Returns the address to bind, or an error for anything
/// that would expose it to the network.
pub(crate) fn require_loopback(bind: &str) -> Result<std::net::IpAddr> {
    let ip: std::net::IpAddr = if bind.eq_ignore_ascii_case("localhost") {
        std::net::Ipv4Addr::LOCALHOST.into()
    } else {
        bind.parse()
            .with_context(|| format!("invalid bind address '{bind}'"))?
    };
    anyhow::ensure!(
        ip.is_loopback(),
        "refusing to bind the web UI to {ip}: only loopback addresses (127.0.0.1, ::1) are allowed"
    );
    Ok(ip)
}

/// Sleep for `dur`, returning early if the active display mode changes.
pub(crate) fn sleep_unless_mode_changes(state: &DaemonState, mode: &DisplayMode, dur: Duration) {
    let deadline = std::time::Instant::now() + dur;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() || *state.mode.lock().unwrap() != *mode {
            return;
        }
        thread::sleep(left.min(Duration::from_millis(50)));
    }
}

pub(crate) fn display_loop(panel: Arc<Mutex<Option<Panel>>>, state: DaemonState) {
    let mut consecutive_errors: u32 = 0;
    loop {
        // If panel is None and we're not headless, attempt to open it now.
        // This handles the auto-headless startup fallback case.
        if !state.headless && panel.lock().unwrap().is_none() {
            log::debug!("display_loop: panel is None in non-headless mode, attempting open");
            let open_result = if consecutive_errors >= 3 {
                Panel::open_with_reset(state.vid, state.pid)
            } else {
                Panel::open(state.vid, state.pid)
            };
            match open_result {
                Ok(mut p) => match p.init() {
                    Ok(()) => {
                        log::info!(
                            "display_loop: initial panel open succeeded (serial={})",
                            p.info.serial
                        );
                        push_history(
                            &state,
                            HistoryKind::System,
                            format!("USB panel connected (serial={})", p.info.serial),
                        );
                        *state.panel_info.lock().unwrap() = Some(p.info.clone());
                        *state.panel_connected.lock().unwrap() = true;
                        *panel.lock().unwrap() = Some(p);
                        consecutive_errors = 0;
                    }
                    Err(e) => {
                        log::warn!("display_loop: panel init failed: {e}");
                        consecutive_errors += 1;
                        thread::sleep(Duration::from_secs(5));
                    }
                },
                Err(e) => {
                    log::debug!("display_loop: panel not yet available: {e}");
                    consecutive_errors += 1;
                    let backoff = (5u64 << consecutive_errors.saturating_sub(1).min(3)).min(60);
                    thread::sleep(Duration::from_secs(backoff));
                    continue;
                }
            }
        }

        let mut mode = state.mode.lock().unwrap().clone();
        // Sleep between frames, but wake early when the mode is switched.
        let requested = mode.clone();
        let nap = |d: Duration| sleep_unless_mode_changes(&state, &requested, d);
        let quality = *state.quality.lock().unwrap();
        let rotate_deg = *state.rotate_deg.lock().unwrap();

        let result: anyhow::Result<()> = (|| {
            // ── Freeze: skip rendering, just keepalive ─────────────────────
            if *state.frozen.lock().unwrap() {
                let mut g = panel.lock().unwrap();
                if let Some(p) = g.as_mut() {
                    p.keepalive()?;
                }
                nap(Duration::from_secs(2));
                return Ok(());
            }

            // ── Flash message override ────────────────────────────────────────────
            {
                let maybe_flash = state.flash_message.lock().unwrap().clone();
                if let Some(ref fm) = maybe_flash {
                    if std::time::Instant::now() < fm.expires_at {
                        let img = render_flash_message(&fm.text, &fm.color);
                        let img = apply_rotation(img, rotate_deg);
                        let img = apply_brightness(img, *state.dim_factor.lock().unwrap());
                        let jpeg = encode_for_panel(&img, quality)?;
                        let jpeg = store_frame(&state, jpeg);
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.send_jpeg(&jpeg)?;
                        }
                        nap(Duration::from_secs(1));
                        return Ok(());
                    } else {
                        // Expired — clear it
                        *state.flash_message.lock().unwrap() = None;
                    }
                }
            }

            // ── Night mode override ─────────────────────────────────────────────────
            if *state.night_mode_enabled.lock().unwrap() {
                let start_h = *state.night_mode_start_hour.lock().unwrap() as u32;
                let end_h = *state.night_mode_end_hour.lock().unwrap() as u32;
                let current_h = chrono::Local::now().hour();
                let in_night = if start_h > end_h {
                    current_h >= start_h || current_h < end_h
                } else {
                    current_h >= start_h && current_h < end_h
                };
                if in_night {
                    let nm = state.night_mode_name.lock().unwrap().clone();
                    if let Some(nm_mode) = DisplayMode::from_str_name(&nm) {
                        mode = nm_mode;
                    }
                }
            }
            match mode {
                DisplayMode::Idle => {
                    let mut g = panel.lock().unwrap();
                    if let Some(p) = g.as_mut() {
                        p.keepalive()?;
                    }
                    nap(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomImage => {
                    let jpeg = state.pending_jpeg.lock().unwrap().take();
                    if let Some(j) = jpeg {
                        let j = store_frame(&state, j);
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.send_jpeg(&j)?;
                        }
                    } else {
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.keepalive()?;
                        }
                    }
                    nap(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::CustomText => {
                    // Auto-expiry check
                    {
                        let exp = *state.auto_clear_at.lock().unwrap();
                        if let Some(t) = exp {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |d| d.as_secs());
                            if now >= t {
                                *state.auto_clear_at.lock().unwrap() = None;
                                *state.pending_text.lock().unwrap() = None;
                                let prev = state.prev_mode.lock().unwrap().clone();
                                *state.mode.lock().unwrap() = prev;
                                thread::sleep(Duration::from_millis(100));
                                return Ok(());
                            }
                        }
                    }
                    let payload = state.pending_text.lock().unwrap().clone();
                    if let Some(p) = payload {
                        let img = render_custom_text(&p);
                        let img = apply_rotation(img, rotate_deg);
                        let img = apply_brightness(img, *state.dim_factor.lock().unwrap());
                        let jpeg = encode_for_panel(&img, quality)?;
                        let jpeg = store_frame(&state, jpeg);
                        let mut g = panel.lock().unwrap();
                        if let Some(panel) = g.as_mut() {
                            panel.send_jpeg(&jpeg)?;
                        }
                    } else {
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.keepalive()?;
                        }
                    }
                    nap(Duration::from_secs(2));
                    return Ok(());
                }
                DisplayMode::Slideshow => {
                    let paths = state.slideshow_paths.lock().unwrap().clone();
                    if paths.is_empty() {
                        let placeholder = render_custom_text(&CustomTextPayload {
                            title: Some("Slideshow".into()),
                            subtitle: Some("No images loaded".into()),
                            ..Default::default()
                        });
                        let placeholder = apply_rotation(placeholder, rotate_deg);
                        let jpeg = store_frame(&state, encode_for_panel(&placeholder, quality)?);
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.send_jpeg(&jpeg)?;
                        }
                        nap(Duration::from_secs(2));
                        return Ok(());
                    }
                    let idx = {
                        let mut g = state.slideshow_idx.lock().unwrap();
                        let i = *g;
                        *g = (i + 1) % paths.len();
                        i
                    };
                    let interval = *state.slideshow_interval.lock().unwrap();
                    let img = ttlcd::open_image(&paths[idx])?;
                    let img = apply_rotation(img, rotate_deg);
                    let img = apply_brightness(img, *state.dim_factor.lock().unwrap());
                    let jpeg = letterbox_for_panel(&img, quality)?;
                    let jpeg = store_frame(&state, jpeg);
                    {
                        let mut g = panel.lock().unwrap();
                        if let Some(p) = g.as_mut() {
                            p.send_jpeg(&jpeg)?;
                        }
                    }
                    nap(Duration::from_secs(interval));
                    return Ok(());
                }
                _ => {}
            }

            let (img, sleep_secs): (DynamicImage, u64) = match mode {
                DisplayMode::Clock => (render_clock(), 1),
                DisplayMode::BigClock => (render_big_clock(), 1),
                DisplayMode::ClockSysinfo => (render_clock_sysinfo(), 2),
                DisplayMode::Sysinfo => (render_sysinfo(), 2),
                DisplayMode::CpuDetail => (render_cpu_detail(), 2),
                DisplayMode::Network => (render_network(), 2),
                DisplayMode::DiskUsage => (render_disk_usage(), 8),
                DisplayMode::ProcessTop => (render_process_top(), 3),
                DisplayMode::Temperature => (render_temperature(), 5),
                DisplayMode::MemoryDetail => (render_memory_detail(), 4),
                DisplayMode::CpuGraph => (render_cpu_graph(), 1),
                DisplayMode::NetGraph => (render_net_graph(), 2),
                DisplayMode::ProcessMemory => (render_process_memory(), 3),
                DisplayMode::DiskIo => (render_disk_io(), 3),
                DisplayMode::Calendar => (render_calendar(), 30),
                DisplayMode::Stopwatch => {
                    let (elapsed_ms, laps, running) = {
                        let sw = state.stopwatch.lock().unwrap();
                        (sw.current_elapsed_ms(), sw.laps.clone(), sw.running)
                    };
                    (
                        render_stopwatch(elapsed_ms, &laps, running),
                        if running { 1 } else { 2 },
                    )
                }
                DisplayMode::Metrics => {
                    let title = state.metrics_title.lock().unwrap().clone();
                    let data = state.metrics_data.lock().unwrap().clone();
                    (render_metrics(&title, &data), 5)
                }
                DisplayMode::Weather => {
                    let wd = state.weather_data.lock().unwrap().clone();
                    (render_weather(&wd), 60)
                }
                DisplayMode::Notes => {
                    let title = state.notes_title.lock().unwrap().clone();
                    let items = state.notes_items.lock().unwrap().clone();
                    (render_notes(&title, &items), 10)
                }
                DisplayMode::QrCode => {
                    let payload = state.qr_payload.lock().unwrap().clone();
                    let title = state.qr_title.lock().unwrap().clone();
                    // QR content never changes, so refresh every 60 s (just keepalive)
                    (render_qr_code(&payload, &title), 60)
                }
                DisplayMode::RecentAlerts => {
                    let entries: Vec<(u64, bool, String)> = {
                        let h = state.history.lock().unwrap();
                        h.iter()
                            .rev()
                            .take(5)
                            .map(|e| {
                                (
                                    e.timestamp,
                                    matches!(e.kind, HistoryKind::Alert),
                                    e.summary.clone(),
                                )
                            })
                            .collect()
                    };
                    let refs: Vec<(u64, bool, &str)> = entries
                        .iter()
                        .map(|(ts, a, s)| (*ts, *a, s.as_str()))
                        .collect();
                    (render_recent_alerts(&refs), 5)
                }
                DisplayMode::Countdown => {
                    let cd_opt = state.countdown.lock().unwrap().clone();
                    if let Some(cd) = cd_opt {
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs());
                        let remaining = (cd.target_unix as i64) - (now_secs as i64);
                        // Auto-expiry: when countdown just hits zero, record history and
                        // return to the previous mode after showing "TIME'S UP" for 10 s.
                        if remaining <= 0 && !cd.expired_notified {
                            let label = cd.label.clone();
                            {
                                let mut cd_lock = state.countdown.lock().unwrap();
                                if let Some(ref mut c) = *cd_lock {
                                    c.expired_notified = true;
                                }
                            }
                            push_history(
                                &state,
                                HistoryKind::Alert,
                                format!(
                                    "⏰ Countdown expired: {}",
                                    if label.is_empty() { "Timer" } else { &label }
                                ),
                            );
                            log::info!("countdown expired: {label}");
                        }
                        // After 10 s in expired state, return to previous mode
                        if remaining < -10 {
                            let prev = state.prev_mode.lock().unwrap().clone();
                            *state.mode.lock().unwrap() = prev;
                            *state.countdown.lock().unwrap() = None;
                        }
                        (
                            render_countdown(remaining, cd.total_secs as i64, &cd.label),
                            1,
                        )
                    } else {
                        (
                            render_custom_text(&CustomTextPayload {
                                title: Some("No countdown set".into()),
                                ..Default::default()
                            }),
                            5,
                        )
                    }
                }
                DisplayMode::Pomodoro => {
                    // Check for auto-advance (phase expired while running)
                    let (check_running, check_remaining, check_phase) = {
                        let p = state.pomodoro.lock().unwrap();
                        (p.running, p.remaining_secs(), p.phase.as_str().to_string())
                    };
                    if check_running && check_remaining <= 0 {
                        let next_phase = {
                            let mut p = state.pomodoro.lock().unwrap();
                            p.advance_phase();
                            p.phase.as_str().to_string()
                        };
                        let msg = format!("Pomodoro: {check_phase} complete → {next_phase}");
                        push_history(&state, HistoryKind::Alert, msg);
                        // Audio notification (best-effort, fire-and-forget)
                        thread::spawn(move || {
                            // Try paplay (PulseAudio), then aplay (ALSA)
                            let played = std::process::Command::new("paplay")
                                .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status()
                                .is_ok_and(|s| s.success());
                            if !played {
                                let _ = std::process::Command::new("aplay")
                                    .arg("/usr/share/sounds/alsa/Front_Center.wav")
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .status();
                            }
                        });
                    }
                    let (phase_str, remaining, total, completed, running) = {
                        let p = state.pomodoro.lock().unwrap();
                        (
                            p.phase.as_str().to_string(),
                            p.remaining_secs(),
                            p.phase_total_secs(),
                            (p.completed_sessions % p.long_break_interval) as u8,
                            p.running,
                        )
                    };
                    let interval = if running { 1 } else { 2 };
                    (
                        render_pomodoro(&phase_str, remaining, total, completed, running),
                        interval,
                    )
                }
                DisplayMode::Ticker => {
                    let text = state.ticker_text.lock().unwrap().clone();
                    let color = state.ticker_color.lock().unwrap().clone();
                    let pos = *state.ticker_pos.lock().unwrap();
                    let img = render_ticker(&text, pos, &color);
                    // Advance ~80 px per 1-second tick (good ticker speed)
                    let text_w = text.len() as i32 * 26 + 480;
                    {
                        let mut p = state.ticker_pos.lock().unwrap();
                        *p = (*p + 80) % text_w.max(1);
                    }
                    (img, 1)
                }
                DisplayMode::Battery => (render_battery(), 10),
                DisplayMode::WorldClock => {
                    let cities = state.world_clock_cities.lock().unwrap().clone();
                    (render_world_clock(&cities), 1)
                }
                DisplayMode::HttpStatus => {
                    let results = state.http_results.lock().unwrap().clone();
                    (render_http_status(&results), 5)
                }
                DisplayMode::ShellCmd => {
                    let cmd = state.shell_cmd_text.lock().unwrap().clone();
                    let output = state.shell_cmd_output.lock().unwrap().clone();
                    let exit_ok = *state.shell_cmd_exit_ok.lock().unwrap();
                    let run_at = *state.shell_cmd_run_at.lock().unwrap();
                    (render_shell_cmd(&cmd, &output, exit_ok, run_at), 5)
                }
                DisplayMode::PingMonitor => {
                    let results = state.ping_results.lock().unwrap().clone();
                    (render_ping_monitor(&results), 5)
                }
                DisplayMode::Dashboard => (render_dashboard(), 2),
                DisplayMode::LogTail => {
                    let path = state.log_tail_path.lock().unwrap().clone();
                    let lines = state.log_tail_lines.lock().unwrap().clone();
                    let ok = *state.log_tail_ok.lock().unwrap();
                    (render_log_tail(&path, &lines, ok), 3)
                }
                DisplayMode::CpuFreq => (render_cpu_freq(), 2),
                DisplayMode::HostnameIp => (render_hostname_ip(), 10),
                DisplayMode::RssFeed => {
                    let title = state.rss_feed_title.lock().unwrap().clone();
                    let items = state.rss_feed_items.lock().unwrap().clone();
                    let ok = *state.rss_feed_ok.lock().unwrap();
                    (render_rss_feed(&title, &items, ok), 30)
                }
                DisplayMode::ServiceStatus => {
                    let units = state.service_statuses.lock().unwrap().clone();
                    (render_service_status(&units), 5)
                }
                DisplayMode::NetConnections => (render_net_connections(), 5),
                DisplayMode::UptimeFancy => (render_uptime(), 2),
                DisplayMode::OsInfo => (render_os_info(), 10),
                DisplayMode::DockerStatus => {
                    let containers = tokio::task::block_in_place(read_docker_containers);
                    (render_docker_status(&containers), 10)
                }
                DisplayMode::LoggedInUsers => {
                    let sessions = tokio::task::block_in_place(read_user_sessions);
                    (render_logged_in_users(&sessions), 10)
                }
                DisplayMode::LoadGraph => (render_load_graph(), 5),
                DisplayMode::MemGraph => (render_mem_graph(), 5),
                DisplayMode::Progress => {
                    let ps = state.progress_state.lock().unwrap().clone();
                    (render_progress(&ps), 2)
                }
                DisplayMode::TempGraph => (render_temp_graph(), 5),
                DisplayMode::DiskIoGraph => (render_disk_io_graph(), 5),
                DisplayMode::GraphsOverview => (render_graphs_overview(), 5),
                DisplayMode::AnalogClock => (render_analog_clock(), 1),
                DisplayMode::StatusOverview => (render_status_overview(), 3),
                DisplayMode::GameOfLife => (render_game_of_life(), 0),
                DisplayMode::LedClock => (render_led_clock(), 1),
                DisplayMode::BounceBall => (render_bounce_ball(), 0),
                DisplayMode::PingGraph => {
                    let results = state.ping_results.lock().unwrap().clone();
                    (render_ping_graph(&results), 5)
                }
                DisplayMode::NetSpeedGraph => (render_net_speed_graph(), 5),
                DisplayMode::CpuCoreBars => (render_cpu_core_bars(), 5),
                DisplayMode::SpeedometerGauge => (render_speedometer_gauge(), 2),
                DisplayMode::DiskPie => (render_disk_pie(), 10),
                DisplayMode::ThermalDashboard => (render_thermal_dashboard(), 5),
                DisplayMode::Plasma => (render_plasma(), 0),
                DisplayMode::Fireworks => (render_fireworks(), 0),
                DisplayMode::CpuHeatmap => (render_cpu_heatmap(), 2),
                DisplayMode::Starfield => (render_starfield(), 0),
                DisplayMode::Pulse => (render_pulse(), 0),
                DisplayMode::MinimalClock => (render_minimal_clock(), 1),
                DisplayMode::ParticleClock => (render_particle_clock(), 0),
                DisplayMode::Snake => (render_snake(), 0),
                DisplayMode::Cube3d => (render_cube_3d(), 0),
                DisplayMode::DvdBounce => (render_dvd_bounce(), 0),
                DisplayMode::BinaryClock => (render_binary_clock(), 1),
                DisplayMode::Radar => (render_radar(), 0),
                DisplayMode::Oscilloscope => (render_oscilloscope(), 0),
                DisplayMode::Lissajous => (render_lissajous(), 0),
                DisplayMode::Pong => (render_pong(), 0),
                DisplayMode::NeonClock => (render_neon_clock(), 1),
                DisplayMode::NetHeatmap => (render_net_heatmap(), 2),
                DisplayMode::WordClock => (render_word_clock(), 1),
                DisplayMode::Breakout => (render_breakout(), 0),
                DisplayMode::Spectrum => (render_spectrum(), 0),
                DisplayMode::DoomFire => (render_doom_fire(), 0),
                DisplayMode::Boids => (render_boids(), 0),
                DisplayMode::Tunnel => (render_tunnel(), 0),
                DisplayMode::Aurora => (render_aurora(), 0),
                DisplayMode::Metaballs => (render_metaballs(), 0),
                DisplayMode::Sand => (render_sand(), 0),
                DisplayMode::Gravity => (render_gravity(), 0),
                DisplayMode::Voronoi => (render_voronoi(), 0),
                DisplayMode::ReactionDiffusion => (render_reaction_diffusion(), 0),
                DisplayMode::Snowfall => (render_snowfall(), 0),
                DisplayMode::HexGrid => (render_hex_grid(), 1),
                DisplayMode::BubbleChart => (render_bubble_chart(), 1),
                DisplayMode::ConwayColors => (render_conway_colors(), 0),
                DisplayMode::JuliaSet => (render_julia_set(), 0),
                DisplayMode::Confetti => (render_confetti(), 0),
                DisplayMode::Cpu3dBars => (render_cpu_3d_bars(), 1),
                DisplayMode::SystemHealth => (render_system_health(), 1),
                DisplayMode::NeonGrid => (render_neon_grid(), 0),
                DisplayMode::Ripple => (render_ripple(), 0),
                DisplayMode::Hypno => (render_hypno(), 0),
                DisplayMode::Lorenz => (render_lorenz(), 0),
                DisplayMode::Mandelbrot => (render_mandelbrot(), 0),
                DisplayMode::Kaleidoscope => (render_kaleidoscope(), 0),
                DisplayMode::DnaHelix => (render_dna_helix(), 0),
                DisplayMode::RainGlass => (render_rain_glass(), 0),
                DisplayMode::PerlinTerrain => (render_perlin_terrain(), 0),
                DisplayMode::FireText => (render_fire_text(), 0),
                DisplayMode::DiscoFloor => (render_disco_floor(), 0),
                DisplayMode::Heartbeat => (render_heartbeat(), 0),
                DisplayMode::ClockFaces => (render_clock_faces(), 0),
                DisplayMode::WaveformBars => (render_waveform_bars(), 0),
                DisplayMode::CitySkyline => (render_city_skyline(), 0),
                DisplayMode::NeonSign => (render_neon_sign(), 0),
                DisplayMode::GradientClock => (render_gradient_clock(), 0),
                DisplayMode::Galaxy => (render_galaxy(), 0),
                DisplayMode::OceanWaves => (render_ocean_waves(), 0),
                DisplayMode::WarpSpeed => (render_warp_speed(), 0),
                DisplayMode::MorphingBlobs => (render_morphing_blobs(), 0),
                DisplayMode::ClockMatrix => (render_clock_matrix(), 0),
                DisplayMode::Sierpinski => (render_sierpinski(), 0),
                DisplayMode::LavaLamp => (render_lava_lamp(), 0),
                DisplayMode::PendulumWave => (render_pendulum_wave(), 0),
                DisplayMode::FiberOptic => (render_fiber_optic(), 0),
                DisplayMode::NetworkGlobe => (render_network_globe(), 0),
                DisplayMode::PlasmaBall => (render_plasma_ball(), 0),
                DisplayMode::MossGrowth => (render_moss_growth(), 0),
                DisplayMode::Torus => (render_torus(), 0),
                DisplayMode::Lemniscate => (render_lemniscate(), 0),
                DisplayMode::SystemArt => (render_system_art(), 0),
                DisplayMode::FourierEpicycles => (render_fourier_epicycles(), 0),
                DisplayMode::Rule30 => (render_rule_30(), 0),
                DisplayMode::NbodyGravity => (render_nbody_gravity(), 0),
                DisplayMode::DoublePendulum => (render_double_pendulum(), 0),
                DisplayMode::RoseCurve => (render_rose_curve(), 0),
                DisplayMode::Truchet => (render_truchet(), 0),
                DisplayMode::Mandala => (render_mandala(), 0),
                DisplayMode::FlowField => (render_flow_field(), 0),
                DisplayMode::GlitchArt => (render_glitch_art(), 0),
                DisplayMode::WaveInterference => (render_wave_interference(), 0),
                DisplayMode::CliffordAttractor => (render_clifford_attractor(), 0),
                DisplayMode::Icosahedron => (render_icosahedron(), 0),
                DisplayMode::NeuralNet => (render_neural_net(), 0),
                DisplayMode::RibbonDance => (render_ribbon_dance(), 0),
                DisplayMode::HexPlasma => (render_hex_plasma(), 0),
                DisplayMode::SandWaterfall => (render_sand_waterfall(), 0),
                DisplayMode::MagneticField => (render_magnetic_field(), 0),
                DisplayMode::SmokePlume => (render_smoke_plume(), 0),
                DisplayMode::ColorMosaic => (render_color_mosaic(), 0),
                DisplayMode::SolarSystem => (render_solar_system(), 0),
                DisplayMode::LightningStorm => (render_lightning_storm(), 0),
                DisplayMode::Spirograph => (render_spirograph(), 0),
                DisplayMode::FractalTree => (render_fractal_tree(), 0),
                DisplayMode::BubbleRise => (render_bubble_rise(), 0),
                DisplayMode::AsteroidField => (render_asteroid_field(), 0),
                DisplayMode::PetCat => (ttlcd::pet::render_pet_cat(), 0),
                DisplayMode::EkgStrip => (render_ekg_strip(), 0),
                DisplayMode::MatrixRain => (render_matrix_rain(), 0),
                DisplayMode::Idle
                | DisplayMode::CustomImage
                | DisplayMode::CustomText
                | DisplayMode::Slideshow => unreachable!("handled above"),
            };

            // Update CPU / load / mem history on every live frame
            if matches!(
                mode,
                DisplayMode::Clock
                    | DisplayMode::BigClock
                    | DisplayMode::ClockSysinfo
                    | DisplayMode::Sysinfo
                    | DisplayMode::CpuDetail
                    | DisplayMode::Network
                    | DisplayMode::DiskUsage
                    | DisplayMode::ProcessTop
                    | DisplayMode::Temperature
                    | DisplayMode::MemoryDetail
                    | DisplayMode::NetGraph
                    | DisplayMode::LoadGraph
                    | DisplayMode::MemGraph
                    | DisplayMode::TempGraph
                    | DisplayMode::DiskIoGraph
                    | DisplayMode::NetSpeedGraph
                    | DisplayMode::CpuCoreBars
            ) {
                use sysinfo::System;
                let mut s = System::new_all();
                s.refresh_cpu_usage();
                s.refresh_memory();
                push_cpu_history(s.global_cpu_usage());
                let load = System::load_average();
                push_load_history(load.one as f32);
                let total = s.total_memory();
                if total > 0 {
                    push_mem_history(s.used_memory() as f32 / total as f32 * 100.0);
                }
                // Temperature history (used by TempGraph mode)
                {
                    use sysinfo::Components;
                    let comps = Components::new_with_refreshed_list();
                    let hot = comps
                        .iter()
                        .filter_map(sysinfo::Component::temperature)
                        .filter(|t| t.is_finite() && *t > 0.0)
                        .fold(0.0_f32, f32::max);
                    if hot > 0.0 {
                        push_temp_history(hot);
                    }
                }
                // Disk I/O history (used by DiskIoGraph mode) - lightweight sample
                {
                    use sysinfo::{DiskRefreshKind, Disks};
                    let mut disks = Disks::new_with_refreshed_list();
                    disks.refresh_specifics(false, DiskRefreshKind::everything());
                    let mut r: u64 = 0;
                    let mut w: u64 = 0;
                    for d in &disks {
                        if d.total_space() > 0 {
                            let io = d.usage();
                            r += io.read_bytes;
                            w += io.written_bytes;
                        }
                    }
                    push_disk_io_history(r as f32, w as f32);
                }
            }
            let img = apply_rotation(img, rotate_deg);
            let img = apply_brightness(img, *state.dim_factor.lock().unwrap());
            let jpeg = store_frame(&state, encode_for_panel(&img, quality)?);
            {
                let mut g = panel.lock().unwrap();
                if let Some(p) = g.as_mut() {
                    p.send_jpeg(&jpeg)?;
                }
            }
            update_fps(&state);
            consecutive_errors = 0;
            if !state.headless {
                *state.panel_connected.lock().unwrap() = true;
            }
            if sleep_secs > 0 {
                nap(Duration::from_secs(sleep_secs));
            } else {
                // Continuous modes (sleep_secs==0): yield briefly so other threads run.
                thread::sleep(Duration::from_millis(16));
            }
            Ok(())
        })();

        if let Err(e) = result {
            consecutive_errors += 1;
            *state.panel_connected.lock().unwrap() = false;
            log::error!("display loop error #{consecutive_errors}: {e:#}");
            push_history(
                &state,
                HistoryKind::System,
                format!("USB error #{consecutive_errors}: {e}"),
            );

            // Exponential backoff: 5 → 10 → 20 → 40 → 60s (capped)
            let backoff_secs: u64 = (5u64 << (consecutive_errors.saturating_sub(1).min(3))).min(60);
            log::info!("reconnect backoff: {backoff_secs}s (error #{consecutive_errors})");
            thread::sleep(Duration::from_secs(backoff_secs));

            if !state.headless {
                // After 3+ consecutive errors use open_with_reset to clear stale
                // kernel driver state left by the previous abrupt disconnect.
                let use_reset = consecutive_errors >= 3;
                let open_result = if use_reset {
                    log::info!(
                        "reconnect: using open_with_reset after {consecutive_errors} consecutive errors"
                    );
                    Panel::open_with_reset(state.vid, state.pid)
                } else {
                    Panel::open(state.vid, state.pid)
                };
                match open_result {
                    Ok(mut p) => match p.init() {
                        Ok(()) => {
                            log::info!(
                                "reconnected (serial={}, after {} errors)",
                                p.info.serial,
                                consecutive_errors
                            );
                            push_history(
                                &state,
                                HistoryKind::System,
                                format!("USB reconnected (serial={})", p.info.serial),
                            );
                            *state.panel_info.lock().unwrap() = Some(p.info.clone());
                            *state.panel_connected.lock().unwrap() = true;
                            *panel.lock().unwrap() = Some(p);
                            consecutive_errors = 0;
                        }
                        Err(init_err) => log::warn!("reconnect init failed: {init_err}"),
                    },
                    Err(open_err) => log::warn!("reconnect open failed: {open_err}"),
                }
            }
        }
    }
}

// ─── Auto-rotation loop ───────────────────────────────────────────────────────

pub(crate) fn rotation_loop(state: DaemonState) {
    loop {
        let interval = *state.rotation_interval.lock().unwrap();
        if interval == 0 {
            // Rotation is disabled; check again after a short sleep.
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        thread::sleep(Duration::from_secs(interval));

        // Only advance if current mode is not transient (user-pushed content).
        let cur = state.mode.lock().unwrap().clone();
        if cur.is_transient() {
            continue;
        }

        let modes = state.rotation_modes.lock().unwrap().clone();
        if modes.is_empty() {
            continue;
        }

        // Find current mode's position in the rotation list; advance from there.
        let cur_pos = modes.iter().position(|m| m == &cur).unwrap_or(0);
        let next = modes[(cur_pos + 1) % modes.len()].clone();
        log::info!("auto-rotation: {} → {}", cur.as_str(), next.as_str());
        *state.prev_mode.lock().unwrap() = next.clone();
        *state.mode.lock().unwrap() = next;
    }
}

// ─── Threshold monitoring loop ───────────────────────────────────────────────

/// Background thread: poll CPU usage and temperatures every 15 s;
/// fire an alert notification if any threshold is exceeded.
pub(crate) fn threshold_monitor_loop(state: DaemonState) {
    use sysinfo::{Components, System};
    loop {
        thread::sleep(Duration::from_secs(15));

        let thresholds = state.alert_thresholds.lock().unwrap().clone();

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        // Rate-limit
        let last_alert = *state.last_threshold_alert_at.lock().unwrap();
        if now_secs.saturating_sub(last_alert) < thresholds.cooldown_secs {
            continue;
        }

        // ── CPU check ──────────────────────────────────────────────────────
        if thresholds.cpu_pct > 0.0 {
            let mut sys = System::new();
            sys.refresh_cpu_usage();
            thread::sleep(Duration::from_millis(200));
            sys.refresh_cpu_usage();
            let cpu = sys.global_cpu_usage();
            if cpu >= thresholds.cpu_pct {
                log::warn!(
                    "threshold alert: CPU {:.0}% ≥ {:.0}%",
                    cpu,
                    thresholds.cpu_pct
                );
                push_threshold_alert(
                    &state,
                    format!("CPU {cpu:.0}%"),
                    format!("Threshold: {:.0}%", thresholds.cpu_pct),
                    ttlcd::display::NamedColor::Red,
                    now_secs,
                );
                continue;
            }
        }

        // ── Temperature check ──────────────────────────────────────────────
        if thresholds.temp_c > 0.0 {
            let components = Components::new_with_refreshed_list();
            for c in &components {
                if let Some(temp) = c.temperature() {
                    if temp >= thresholds.temp_c {
                        let label = c.label().to_string();
                        log::warn!(
                            "threshold alert: {} = {:.0}°C ≥ {:.0}°C",
                            label,
                            temp,
                            thresholds.temp_c
                        );
                        push_threshold_alert(
                            &state,
                            format!("{temp:.0}°C — {label}"),
                            format!("Threshold: {:.0}°C", thresholds.temp_c),
                            ttlcd::display::NamedColor::Red,
                            now_secs,
                        );
                        break;
                    }
                }
            }
        }
    }
}

/// Push a threshold alert into the history ring buffer and switch to
/// `RecentAlerts` mode briefly (5 s) then return.
pub(crate) fn push_threshold_alert(
    state: &DaemonState,
    title: String,
    subtitle: String,
    color: ttlcd::display::NamedColor,
    now_secs: u64,
) {
    *state.last_threshold_alert_at.lock().unwrap() = now_secs;

    let payload = CustomTextPayload {
        title: Some(title.clone()),
        subtitle: Some(subtitle),
        color,
        show_time: true,
        ..Default::default()
    };
    let summary = format!("THRESHOLD — {title}");
    push_history(state, HistoryKind::Alert, summary);
    *state.pending_text.lock().unwrap() = Some(payload);
    let prev = state.mode.lock().unwrap().clone();
    if !prev.is_transient() {
        *state.prev_mode.lock().unwrap() = prev;
    }
    *state.mode.lock().unwrap() = DisplayMode::CustomText;
    *state.auto_clear_at.lock().unwrap() = Some(now_secs + 10);
}

// ─── Dim schedule loop ────────────────────────────────────────────────────────

/// Background thread: check the dim schedule every 60 s and apply/lift the dim.
pub(crate) fn dim_schedule_loop(state: DaemonState) {
    loop {
        thread::sleep(Duration::from_secs(60));

        let schedule_opt = state.dim_schedule.lock().unwrap().clone();
        let Some(sched) = schedule_opt else { continue };

        let now_hour = Local::now().hour() as u8;
        let in_window = if sched.start_hour <= sched.end_hour {
            now_hour >= sched.start_hour && now_hour < sched.end_hour
        } else {
            // Overnight window e.g. 22:00 – 06:00
            now_hour >= sched.start_hour || now_hour < sched.end_hour
        };

        let current = *state.dim_factor.lock().unwrap();
        let target = if in_window { sched.dim_level } else { 1.0_f32 };
        if (current - target).abs() > 0.01 {
            log::info!(
                "dim schedule: {} → brightness {:.0}%",
                if in_window { "dim" } else { "restore" },
                target * 100.0
            );
            *state.dim_factor.lock().unwrap() = target;
        }
    }
}

// ─── Mode schedule loop ───────────────────────────────────────────────────────

/// Path to ~/.config/ttlcd/mode_schedule.json
pub(crate) fn mode_schedule_path() -> Option<std::path::PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("mode_schedule.json"))
}

pub(crate) fn load_mode_schedule() -> Vec<ModeScheduleEntry> {
    (|| -> Option<Vec<ModeScheduleEntry>> {
        let path = mode_schedule_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_mode_schedule(entries: &[ModeScheduleEntry]) {
    let Some(path) = mode_schedule_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(&path, &text);
    }
}

// ─── On-demand mode preview rendering ────────────────────────────────────────

/// GET /api/render/{mode} — render any display mode to a JPEG on demand.
///
/// The panel is NOT updated. This is purely a preview/screenshot endpoint.
/// Useful for generating thumbnails or testing what a mode looks like.
///
/// Returns: JPEG image (Content-Type: image/jpeg)
/// Query params:
///   `quality=N` — JPEG quality (default: current daemon quality, 1-100)
///   `rotate=N`  — Rotation in degrees: 0, 90, 180, 270 (default: 0)
///
/// Example:
///   curl "http://localhost:7070/api/render/sysinfo" > sysinfo.jpg
///   curl "http://localhost:7070/api/render/matrix_rain?quality=90" > rain.jpg
pub(crate) async fn api_render_mode_preview(
    state: web::Data<DaemonState>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let mode_name = path.into_inner();
    let Some(mode) = DisplayMode::from_str_name(&mode_name) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "unknown mode",
            "available": DisplayMode::ALL_NAMES,
        }));
    };

    // Quality and rotation from query params
    let quality = query
        .get("quality")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(*state.quality.lock().unwrap())
        .clamp(1, 100);
    let rotate_deg = query
        .get("rotate")
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0);

    // Render the requested mode in a blocking task
    let state_clone = state.clone();
    let img_result = tokio::task::spawn_blocking(move || -> anyhow::Result<image::DynamicImage> {
        use ttlcd::display::*;
        let img: image::DynamicImage = match mode {
            DisplayMode::Clock => render_clock(),
            DisplayMode::BigClock => render_big_clock(),
            DisplayMode::ClockSysinfo => render_clock_sysinfo(),
            DisplayMode::Sysinfo => render_sysinfo(),
            DisplayMode::CpuDetail => render_cpu_detail(),
            DisplayMode::Network => render_network(),
            DisplayMode::DiskUsage => render_disk_usage(),
            DisplayMode::ProcessTop => render_process_top(),
            DisplayMode::Temperature => render_temperature(),
            DisplayMode::MemoryDetail => render_memory_detail(),
            DisplayMode::CpuGraph => render_cpu_graph(),
            DisplayMode::NetGraph => render_net_graph(),
            DisplayMode::ProcessMemory => render_process_memory(),
            DisplayMode::DiskIo => render_disk_io(),
            DisplayMode::Calendar => render_calendar(),
            DisplayMode::Stopwatch => {
                let (elapsed_ms, laps, running) = {
                    let sw = state_clone.stopwatch.lock().unwrap();
                    (sw.current_elapsed_ms(), sw.laps.clone(), sw.running)
                };
                render_stopwatch(elapsed_ms, &laps, running)
            }
            DisplayMode::Battery => render_battery(),
            DisplayMode::Dashboard => render_dashboard(),
            DisplayMode::CpuFreq => render_cpu_freq(),
            DisplayMode::HostnameIp => render_hostname_ip(),
            DisplayMode::NetConnections => render_net_connections(),
            DisplayMode::UptimeFancy => render_uptime(),
            DisplayMode::OsInfo => render_os_info(),
            DisplayMode::DockerStatus => {
                let containers = read_docker_containers();
                render_docker_status(&containers)
            }
            DisplayMode::LoggedInUsers => {
                let sessions = read_user_sessions();
                render_logged_in_users(&sessions)
            }
            DisplayMode::LoadGraph => render_load_graph(),
            DisplayMode::MemGraph => render_mem_graph(),
            DisplayMode::TempGraph => render_temp_graph(),
            DisplayMode::DiskIoGraph => render_disk_io_graph(),
            DisplayMode::GraphsOverview => render_graphs_overview(),
            DisplayMode::AnalogClock => render_analog_clock(),
            DisplayMode::PingGraph => {
                let results = state_clone.ping_results.lock().unwrap().clone();
                render_ping_graph(&results)
            }
            DisplayMode::NetSpeedGraph => render_net_speed_graph(),
            DisplayMode::CpuCoreBars => render_cpu_core_bars(),
            DisplayMode::SpeedometerGauge => render_speedometer_gauge(),
            DisplayMode::DiskPie => render_disk_pie(),
            DisplayMode::ThermalDashboard => render_thermal_dashboard(),
            DisplayMode::Plasma => render_plasma(),
            DisplayMode::Breakout => render_breakout(),
            DisplayMode::Spectrum => render_spectrum(),
            DisplayMode::DoomFire => render_doom_fire(),
            DisplayMode::NeonGrid => render_neon_grid(),
            DisplayMode::Ripple => render_ripple(),
            DisplayMode::Hypno => render_hypno(),
            DisplayMode::Lorenz => render_lorenz(),
            DisplayMode::Mandelbrot => render_mandelbrot(),
            DisplayMode::Kaleidoscope => render_kaleidoscope(),
            DisplayMode::DnaHelix => render_dna_helix(),
            DisplayMode::RainGlass => render_rain_glass(),
            DisplayMode::PerlinTerrain => render_perlin_terrain(),
            DisplayMode::FireText => render_fire_text(),
            DisplayMode::DiscoFloor => render_disco_floor(),
            DisplayMode::Heartbeat => render_heartbeat(),
            DisplayMode::ClockFaces => render_clock_faces(),
            DisplayMode::WaveformBars => render_waveform_bars(),
            DisplayMode::CitySkyline => render_city_skyline(),
            DisplayMode::NeonSign => render_neon_sign(),
            DisplayMode::GradientClock => render_gradient_clock(),
            DisplayMode::Galaxy => render_galaxy(),
            DisplayMode::OceanWaves => render_ocean_waves(),
            DisplayMode::WarpSpeed => render_warp_speed(),
            DisplayMode::MorphingBlobs => render_morphing_blobs(),
            DisplayMode::ClockMatrix => render_clock_matrix(),
            DisplayMode::Sierpinski => render_sierpinski(),
            DisplayMode::LavaLamp => render_lava_lamp(),
            DisplayMode::PendulumWave => render_pendulum_wave(),
            DisplayMode::FiberOptic => render_fiber_optic(),
            DisplayMode::NetworkGlobe => render_network_globe(),
            DisplayMode::PlasmaBall => render_plasma_ball(),
            DisplayMode::MossGrowth => render_moss_growth(),
            DisplayMode::Torus => render_torus(),
            DisplayMode::Lemniscate => render_lemniscate(),
            DisplayMode::SystemArt => render_system_art(),
            DisplayMode::FourierEpicycles => render_fourier_epicycles(),
            DisplayMode::Rule30 => render_rule_30(),
            DisplayMode::NbodyGravity => render_nbody_gravity(),
            DisplayMode::DoublePendulum => render_double_pendulum(),
            DisplayMode::RoseCurve => render_rose_curve(),
            DisplayMode::Truchet => render_truchet(),
            DisplayMode::Mandala => render_mandala(),
            DisplayMode::FlowField => render_flow_field(),
            DisplayMode::GlitchArt => render_glitch_art(),
            DisplayMode::WaveInterference => render_wave_interference(),
            DisplayMode::CliffordAttractor => render_clifford_attractor(),
            DisplayMode::Icosahedron => render_icosahedron(),
            DisplayMode::NeuralNet => render_neural_net(),
            DisplayMode::RibbonDance => render_ribbon_dance(),
            DisplayMode::HexPlasma => render_hex_plasma(),
            DisplayMode::SandWaterfall => render_sand_waterfall(),
            DisplayMode::MagneticField => render_magnetic_field(),
            DisplayMode::SmokePlume => render_smoke_plume(),
            DisplayMode::ColorMosaic => render_color_mosaic(),
            DisplayMode::SolarSystem => render_solar_system(),
            DisplayMode::LightningStorm => render_lightning_storm(),
            DisplayMode::Spirograph => render_spirograph(),
            DisplayMode::FractalTree => render_fractal_tree(),
            DisplayMode::BubbleRise => render_bubble_rise(),
            DisplayMode::AsteroidField => render_asteroid_field(),
            DisplayMode::PetCat => ttlcd::pet::render_pet_cat(),
            DisplayMode::EkgStrip => render_ekg_strip(),
            DisplayMode::ConwayColors => render_conway_colors(),
            DisplayMode::Cpu3dBars => render_cpu_3d_bars(),
            DisplayMode::SystemHealth => render_system_health(),
            DisplayMode::JuliaSet => render_julia_set(),
            DisplayMode::Confetti => render_confetti(),
            DisplayMode::Voronoi => render_voronoi(),
            DisplayMode::HexGrid => render_hex_grid(),
            DisplayMode::BubbleChart => render_bubble_chart(),
            DisplayMode::ReactionDiffusion => render_reaction_diffusion(),
            DisplayMode::Snowfall => render_snowfall(),
            DisplayMode::Metaballs => render_metaballs(),
            DisplayMode::Sand => render_sand(),
            DisplayMode::Gravity => render_gravity(),
            DisplayMode::Boids => render_boids(),
            DisplayMode::Tunnel => render_tunnel(),
            DisplayMode::Aurora => render_aurora(),
            DisplayMode::CpuHeatmap => render_cpu_heatmap(),
            DisplayMode::WordClock => render_word_clock(),
            DisplayMode::Pulse => render_pulse(),
            DisplayMode::NetHeatmap => render_net_heatmap(),
            DisplayMode::Pong => render_pong(),
            DisplayMode::NeonClock => render_neon_clock(),
            DisplayMode::Snake => render_snake(),
            DisplayMode::Cube3d => render_cube_3d(),
            DisplayMode::DvdBounce => render_dvd_bounce(),
            DisplayMode::BinaryClock => render_binary_clock(),
            DisplayMode::Lissajous => render_lissajous(),
            DisplayMode::Radar => render_radar(),
            DisplayMode::Oscilloscope => render_oscilloscope(),
            DisplayMode::MatrixRain => render_matrix_rain(),
            DisplayMode::Progress => {
                let ps = state_clone.progress_state.lock().unwrap().clone();
                render_progress(&ps)
            }
            DisplayMode::RssFeed => {
                let title = state_clone.rss_feed_title.lock().unwrap().clone();
                let items = state_clone.rss_feed_items.lock().unwrap().clone();
                let ok = *state_clone.rss_feed_ok.lock().unwrap();
                render_rss_feed(&title, &items, ok)
            }
            DisplayMode::ServiceStatus => {
                let units = state_clone.service_statuses.lock().unwrap().clone();
                render_service_status(&units)
            }
            // Modes that require special context — return a blank with label
            _ => {
                let mut blank = image::RgbImage::new(480, 480);
                for pix in blank.pixels_mut() {
                    *pix = image::Rgb([30, 30, 46]);
                }
                image::DynamicImage::ImageRgb8(blank)
            }
        };
        Ok(img)
    })
    .await;

    let img = match img_result {
        Ok(Ok(i)) => i,
        Ok(Err(e)) => {
            return HttpResponse::InternalServerError().body(format!("render error: {e}"))
        }
        Err(e) => return HttpResponse::InternalServerError().body(format!("task error: {e}")),
    };

    let img = apply_rotation(img, rotate_deg);
    match encode_for_panel(&img, quality) {
        Ok(jpeg) => HttpResponse::Ok()
            .content_type("image/jpeg")
            .insert_header(("X-Mode", mode_name.as_str()))
            .insert_header(("X-Quality", quality.to_string().as_str()))
            .body(jpeg),
        Err(e) => HttpResponse::InternalServerError().body(format!("encode error: {e}")),
    }
}

// ─── Auto-screensaver idle detection ─────────────────────────────────────────

/// Background thread: monitors user inactivity and activates/deactivates
/// the screensaver mode.
///
/// When `screensaver_timeout_secs` is > 0 and the time since `last_activity_secs`
/// exceeds the timeout, the daemon switches to `screensaver_mode_name`.
///
/// The screensaver is deactivated automatically when the next API call arrives
/// (handled in `auth_middleware`).
pub(crate) fn screensaver_loop(state: DaemonState) {
    loop {
        thread::sleep(std::time::Duration::from_secs(10));

        let timeout = *state.screensaver_timeout_secs.lock().unwrap();
        if timeout == 0 {
            continue; // screensaver disabled
        }

        let last = *state.last_activity_secs.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        let already_active = *state.screensaver_active.lock().unwrap();

        if !already_active && now.saturating_sub(last) >= timeout {
            // Activate screensaver
            let ss_name = state.screensaver_mode_name.lock().unwrap().clone();
            if let Some(ss_mode) = DisplayMode::from_str_name(&ss_name) {
                let cur = state.mode.lock().unwrap().clone();
                if !cur.is_transient() && cur.as_str() != ss_name.as_str() {
                    log::info!("screensaver: activating {ss_name} after {timeout}s of inactivity");
                    *state.prev_mode.lock().unwrap() = cur;
                    *state.mode.lock().unwrap() = ss_mode;
                    *state.screensaver_active.lock().unwrap() = true;
                }
            }
        }
    }
}
