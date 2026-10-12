//! Read-only endpoints describing the daemon: metrics, benchmark, version, badge, mode lists and the API docs page.

use super::*;

// ─── GET /api/benchmark — render all visual modes once and report timing ──────

/// GET /api/benchmark — measure render time for a curated set of visual modes.
/// Returns JSON: { "modes": [{"name": "...", "ms": 12.3}], "total_ms": 456.7 }
/// Only non-blocking (non-system) modes are benchmarked.
pub(crate) async fn api_benchmark(state: web::Data<DaemonState>) -> impl Responder {
    let quality = *state.quality.lock().unwrap();
    let rotate = *state.rotate_deg.lock().unwrap();

    let benchmark_modes = [
        "matrix_rain",
        "plasma",
        "fireworks",
        "starfield",
        "game_of_life",
        "doom_fire",
        "boids",
        "tunnel",
        "aurora",
        "metaballs",
        "sand",
        "voronoi",
        "reaction_diffusion",
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
        "city_skyline",
        "galaxy",
        "ocean_waves",
        "warp_speed",
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
        "sand_waterfall",
        "magnetic_field",
        "smoke_plume",
        "color_mosaic",
        "neural_net",
        "ribbon_dance",
        "hex_plasma",
        "wave_interference",
        "clifford_attractor",
        "icosahedron",
        "mandala",
        "flow_field",
        "glitch_art",
        "double_pendulum",
        "rose_curve",
        "truchet",
        "fourier_epicycles",
        "rule_30",
        "nbody_gravity",
    ];

    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut total_ms = 0.0f64;

    for &name in &benchmark_modes {
        if let Some(mode) = DisplayMode::from_str_name(name) {
            let start = std::time::Instant::now();
            let img = match mode {
                DisplayMode::MatrixRain => render_matrix_rain(),
                DisplayMode::Plasma => render_plasma(),
                DisplayMode::Fireworks => render_fireworks(),
                DisplayMode::Starfield => render_starfield(),
                DisplayMode::GameOfLife => render_game_of_life(),
                DisplayMode::DoomFire => render_doom_fire(),
                DisplayMode::Boids => render_boids(),
                DisplayMode::Tunnel => render_tunnel(),
                DisplayMode::Aurora => render_aurora(),
                DisplayMode::Metaballs => render_metaballs(),
                DisplayMode::Sand => render_sand(),
                DisplayMode::Voronoi => render_voronoi(),
                DisplayMode::ReactionDiffusion => render_reaction_diffusion(),
                DisplayMode::JuliaSet => render_julia_set(),
                DisplayMode::Confetti => render_confetti(),
                DisplayMode::NeonGrid => render_neon_grid(),
                DisplayMode::Ripple => render_ripple(),
                DisplayMode::Hypno => render_hypno(),
                DisplayMode::Lorenz => render_lorenz(),
                DisplayMode::Mandelbrot => render_mandelbrot(),
                DisplayMode::Kaleidoscope => render_kaleidoscope(),
                DisplayMode::DnaHelix => render_dna_helix(),
                DisplayMode::RainGlass => render_rain_glass(),
                DisplayMode::PerlinTerrain => render_perlin_terrain(),
                DisplayMode::CitySkyline => render_city_skyline(),
                DisplayMode::Galaxy => render_galaxy(),
                DisplayMode::OceanWaves => render_ocean_waves(),
                DisplayMode::WarpSpeed => render_warp_speed(),
                DisplayMode::Sierpinski => render_sierpinski(),
                DisplayMode::LavaLamp => render_lava_lamp(),
                DisplayMode::PendulumWave => render_pendulum_wave(),
                DisplayMode::FiberOptic => render_fiber_optic(),
                DisplayMode::NetworkGlobe => render_network_globe(),
                DisplayMode::PlasmaBall => render_plasma_ball(),
                DisplayMode::MossGrowth => render_moss_growth(),
                DisplayMode::AsteroidField => render_asteroid_field(),
                DisplayMode::PetCat => ttlcd::pet::render_pet_cat(),
                DisplayMode::EkgStrip => render_ekg_strip(),
                DisplayMode::SolarSystem => render_solar_system(),
                DisplayMode::LightningStorm => render_lightning_storm(),
                DisplayMode::Spirograph => render_spirograph(),
                DisplayMode::FractalTree => render_fractal_tree(),
                DisplayMode::BubbleRise => render_bubble_rise(),
                DisplayMode::SandWaterfall => render_sand_waterfall(),
                DisplayMode::MagneticField => render_magnetic_field(),
                DisplayMode::SmokePlume => render_smoke_plume(),
                DisplayMode::ColorMosaic => render_color_mosaic(),
                DisplayMode::NeuralNet => render_neural_net(),
                DisplayMode::RibbonDance => render_ribbon_dance(),
                DisplayMode::HexPlasma => render_hex_plasma(),
                DisplayMode::WaveInterference => render_wave_interference(),
                DisplayMode::CliffordAttractor => render_clifford_attractor(),
                DisplayMode::Icosahedron => render_icosahedron(),
                DisplayMode::Mandala => render_mandala(),
                DisplayMode::FlowField => render_flow_field(),
                DisplayMode::GlitchArt => render_glitch_art(),
                DisplayMode::DoublePendulum => render_double_pendulum(),
                DisplayMode::RoseCurve => render_rose_curve(),
                DisplayMode::Truchet => render_truchet(),
                DisplayMode::FourierEpicycles => render_fourier_epicycles(),
                DisplayMode::Rule30 => render_rule_30(),
                DisplayMode::NbodyGravity => render_nbody_gravity(),
                _ => continue,
            };
            // Also encode to JPEG to measure full pipeline cost
            let img = apply_rotation(img, rotate);
            let img = apply_brightness(img, *state.dim_factor.lock().unwrap());
            let _ = encode_for_panel(&img, quality);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            total_ms += elapsed;
            results.push(serde_json::json!({
                "name": name,
                "ms": (elapsed * 10.0).round() / 10.0,
            }));
        }
    }

    // Sort by descending render time for easy identification of slow modes
    results.sort_by(|a, b| {
        let ta = a["ms"].as_f64().unwrap_or(0.0);
        let tb = b["ms"].as_f64().unwrap_or(0.0);
        tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
    });

    HttpResponse::Ok().json(serde_json::json!({
        "count": results.len(),
        "total_ms": (total_ms * 10.0).round() / 10.0,
        "avg_ms": (total_ms / results.len().max(1) as f64 * 10.0).round() / 10.0,
        "modes": results,
    }))
}

// ─── Prometheus metrics ───────────────────────────────────────────────────────

/// GET /metrics — expose key system and daemon metrics in Prometheus text format.
pub(crate) async fn api_metrics(state: web::Data<DaemonState>) -> impl Responder {
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

    let mode = state.mode.lock().unwrap().as_str().to_string();
    let quality = *state.quality.lock().unwrap();
    let rotate = *state.rotate_deg.lock().unwrap();
    let history_len = state.history.lock().unwrap().len();

    let mut out = String::with_capacity(1024);
    let push = |s: &mut String, line: String| {
        s.push_str(&line);
        s.push('\n');
    };

    push(
        &mut out,
        "# HELP ttlcd_cpu_usage_percent System CPU usage percentage".into(),
    );
    push(&mut out, "# TYPE ttlcd_cpu_usage_percent gauge".into());
    push(
        &mut out,
        format!("ttlcd_cpu_usage_percent {:.2}", sys.global_cpu_usage()),
    );

    push(
        &mut out,
        "# HELP ttlcd_memory_used_bytes RAM used in bytes".into(),
    );
    push(&mut out, "# TYPE ttlcd_memory_used_bytes gauge".into());
    push(
        &mut out,
        format!("ttlcd_memory_used_bytes {}", sys.used_memory()),
    );

    push(
        &mut out,
        "# HELP ttlcd_memory_total_bytes Total RAM in bytes".into(),
    );
    push(&mut out, "# TYPE ttlcd_memory_total_bytes gauge".into());
    push(
        &mut out,
        format!("ttlcd_memory_total_bytes {}", sys.total_memory()),
    );

    push(&mut out, "# HELP ttlcd_load1 1-minute load average".into());
    push(&mut out, "# TYPE ttlcd_load1 gauge".into());
    push(&mut out, format!("ttlcd_load1 {:.4}", load.one));

    push(&mut out, "# HELP ttlcd_load5 5-minute load average".into());
    push(&mut out, "# TYPE ttlcd_load5 gauge".into());
    push(&mut out, format!("ttlcd_load5 {:.4}", load.five));

    push(
        &mut out,
        "# HELP ttlcd_load15 15-minute load average".into(),
    );
    push(&mut out, "# TYPE ttlcd_load15 gauge".into());
    push(&mut out, format!("ttlcd_load15 {:.4}", load.fifteen));

    if let Some(t) = cpu_temp {
        push(
            &mut out,
            "# HELP ttlcd_cpu_temperature_celsius CPU temperature".into(),
        );
        push(
            &mut out,
            "# TYPE ttlcd_cpu_temperature_celsius gauge".into(),
        );
        push(&mut out, format!("ttlcd_cpu_temperature_celsius {t:.1}"));
    }

    push(
        &mut out,
        "# HELP ttlcd_uptime_seconds System uptime in seconds".into(),
    );
    push(&mut out, "# TYPE ttlcd_uptime_seconds counter".into());
    push(
        &mut out,
        format!("ttlcd_uptime_seconds {}", System::uptime()),
    );

    push(
        &mut out,
        "# HELP ttlcd_frame_seq Monotonic frame counter".into(),
    );
    push(&mut out, "# TYPE ttlcd_frame_seq counter".into());
    push(
        &mut out,
        format!("ttlcd_frame_seq {}", *state.frame_seq.lock().unwrap()),
    );

    push(
        &mut out,
        "# HELP ttlcd_quality_percent Current JPEG quality setting".into(),
    );
    push(&mut out, "# TYPE ttlcd_quality_percent gauge".into());
    push(&mut out, format!("ttlcd_quality_percent {quality}"));

    push(
        &mut out,
        "# HELP ttlcd_rotate_deg Current output rotation (degrees)".into(),
    );
    push(&mut out, "# TYPE ttlcd_rotate_deg gauge".into());
    push(&mut out, format!("ttlcd_rotate_deg {rotate}"));

    push(
        &mut out,
        "# HELP ttlcd_display_mode Current display mode (label only)".into(),
    );
    push(&mut out, "# TYPE ttlcd_display_mode gauge".into());
    push(&mut out, format!("ttlcd_display_mode{{mode=\"{mode}\"}} 1"));

    push(
        &mut out,
        "# HELP ttlcd_history_total Total notification events pushed".into(),
    );
    push(&mut out, "# TYPE ttlcd_history_total counter".into());
    push(&mut out, format!("ttlcd_history_total {history_len}"));

    push(
        &mut out,
        "# HELP ttlcd_brightness Current display brightness 0.0–1.0".into(),
    );
    push(&mut out, "# TYPE ttlcd_brightness gauge".into());
    push(
        &mut out,
        format!("ttlcd_brightness {:.3}", *state.dim_factor.lock().unwrap()),
    );

    push(
        &mut out,
        "# HELP ttlcd_pending_scheduled Scheduled notifications not yet fired".into(),
    );
    push(&mut out, "# TYPE ttlcd_pending_scheduled gauge".into());
    push(
        &mut out,
        format!(
            "ttlcd_pending_scheduled {}",
            *state.pending_scheduled.lock().unwrap()
        ),
    );

    push(
        &mut out,
        "# HELP ttlcd_swap_used_bytes Swap space used in bytes".into(),
    );
    push(&mut out, "# TYPE ttlcd_swap_used_bytes gauge".into());
    push(
        &mut out,
        format!("ttlcd_swap_used_bytes {}", sys.used_swap()),
    );

    push(
        &mut out,
        "# HELP ttlcd_swap_total_bytes Total swap space in bytes".into(),
    );
    push(&mut out, "# TYPE ttlcd_swap_total_bytes gauge".into());
    push(
        &mut out,
        format!("ttlcd_swap_total_bytes {}", sys.total_swap()),
    );

    // Per-disk space metrics
    {
        use sysinfo::Disks;
        let disks = Disks::new_with_refreshed_list();
        push(
            &mut out,
            "# HELP ttlcd_disk_used_bytes Disk partition used bytes".into(),
        );
        push(&mut out, "# TYPE ttlcd_disk_used_bytes gauge".into());
        push(
            &mut out,
            "# HELP ttlcd_disk_total_bytes Disk partition total bytes".into(),
        );
        push(&mut out, "# TYPE ttlcd_disk_total_bytes gauge".into());
        for d in &disks {
            if d.total_space() == 0 {
                continue;
            }
            let mount = d.mount_point().to_string_lossy().replace('"', "");
            let used = d.total_space() - d.available_space();
            push(
                &mut out,
                format!("ttlcd_disk_used_bytes{{mount=\"{mount}\"}} {used}"),
            );
            push(
                &mut out,
                format!(
                    "ttlcd_disk_total_bytes{{mount=\"{mount}\"}} {}",
                    d.total_space()
                ),
            );
        }
    }

    // All temperature sensors
    {
        push(
            &mut out,
            "# HELP ttlcd_sensor_temperature_celsius Temperature sensor reading in Celsius".into(),
        );
        push(
            &mut out,
            "# TYPE ttlcd_sensor_temperature_celsius gauge".into(),
        );
        for c in &comps {
            if let Some(t) = c.temperature() {
                if t.is_nan() || t <= 0.0 {
                    continue;
                }
                let label = c.label().replace('"', "").replace(' ', "_");
                push(
                    &mut out,
                    format!("ttlcd_sensor_temperature_celsius{{sensor=\"{label}\"}} {t:.1}"),
                );
            }
        }
    }

    // Render FPS
    push(
        &mut out,
        "# HELP ttlcd_render_fps Exponential-moving-average render frames per second".into(),
    );
    push(&mut out, "# TYPE ttlcd_render_fps gauge".into());
    push(
        &mut out,
        format!("ttlcd_render_fps {:.3}", *state.render_fps.lock().unwrap()),
    );

    // Daemon uptime
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    push(
        &mut out,
        "# HELP ttlcd_daemon_uptime_seconds Seconds since daemon started".into(),
    );
    push(
        &mut out,
        "# TYPE ttlcd_daemon_uptime_seconds counter".into(),
    );
    push(
        &mut out,
        format!(
            "ttlcd_daemon_uptime_seconds {}",
            now_s.saturating_sub(state.daemon_start_secs)
        ),
    );

    // Frozen state
    push(
        &mut out,
        "# HELP ttlcd_display_frozen Whether the display is frozen (1 = yes, 0 = no)".into(),
    );
    push(&mut out, "# TYPE ttlcd_display_frozen gauge".into());
    push(
        &mut out,
        format!(
            "ttlcd_display_frozen {}",
            if *state.frozen.lock().unwrap() { 1 } else { 0 }
        ),
    );

    // Stopwatch
    {
        let sw = state.stopwatch.lock().unwrap();
        push(
            &mut out,
            "# HELP ttlcd_stopwatch_running Whether the stopwatch is running (1 = yes, 0 = no)"
                .into(),
        );
        push(&mut out, "# TYPE ttlcd_stopwatch_running gauge".into());
        push(
            &mut out,
            format!("ttlcd_stopwatch_running {}", if sw.running { 1 } else { 0 }),
        );
        push(
            &mut out,
            "# HELP ttlcd_stopwatch_elapsed_ms Stopwatch elapsed milliseconds".into(),
        );
        push(&mut out, "# TYPE ttlcd_stopwatch_elapsed_ms counter".into());
        push(
            &mut out,
            format!("ttlcd_stopwatch_elapsed_ms {}", sw.current_elapsed_ms()),
        );
        push(
            &mut out,
            "# HELP ttlcd_stopwatch_lap_count Number of recorded laps".into(),
        );
        push(&mut out, "# TYPE ttlcd_stopwatch_lap_count counter".into());
        push(
            &mut out,
            format!("ttlcd_stopwatch_lap_count {}", sw.laps.len()),
        );
    }

    // Custom metrics count
    push(
        &mut out,
        "# HELP ttlcd_metrics_count Number of custom metric items currently set".into(),
    );
    push(&mut out, "# TYPE ttlcd_metrics_count gauge".into());
    push(
        &mut out,
        format!(
            "ttlcd_metrics_count {}",
            state.metrics_data.lock().unwrap().len()
        ),
    );

    // Alert threshold
    {
        let t = state.alert_thresholds.lock().unwrap();
        push(
            &mut out,
            "# HELP ttlcd_threshold_cpu_pct CPU alert threshold percent (0 = disabled)".into(),
        );
        push(&mut out, "# TYPE ttlcd_threshold_cpu_pct gauge".into());
        push(&mut out, format!("ttlcd_threshold_cpu_pct {}", t.cpu_pct));
        push(
            &mut out,
            "# HELP ttlcd_threshold_temp_c Temperature alert threshold Celsius (0 = disabled)"
                .into(),
        );
        push(&mut out, "# TYPE ttlcd_threshold_temp_c gauge".into());
        push(&mut out, format!("ttlcd_threshold_temp_c {}", t.temp_c));
    }

    // HTTP monitor metrics
    {
        let results = state.http_results.lock().unwrap().clone();
        if !results.is_empty() {
            push(
                &mut out,
                "# HELP ttlcd_http_monitor_up HTTP monitor endpoint up (1=up, 0=down)".into(),
            );
            push(&mut out, "# TYPE ttlcd_http_monitor_up gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let url = r.url.replace('"', "'");
                push(
                    &mut out,
                    format!(
                        "ttlcd_http_monitor_up{{label=\"{}\",url=\"{}\"}} {}",
                        label,
                        url,
                        if r.ok { 1 } else { 0 }
                    ),
                );
            }
            push(&mut out, "# HELP ttlcd_http_monitor_latency_ms HTTP monitor last response latency milliseconds".into());
            push(
                &mut out,
                "# TYPE ttlcd_http_monitor_latency_ms gauge".into(),
            );
            for r in &results {
                if let Some(ms) = r.latency_ms {
                    let label = r.label.replace('"', "'");
                    let url = r.url.replace('"', "'");
                    push(
                        &mut out,
                        format!(
                            "ttlcd_http_monitor_latency_ms{{label=\"{label}\",url=\"{url}\"}} {ms}"
                        ),
                    );
                }
            }
            push(&mut out, "# HELP ttlcd_http_monitor_status_code HTTP monitor last HTTP status code (0=no response)".into());
            push(
                &mut out,
                "# TYPE ttlcd_http_monitor_status_code gauge".into(),
            );
            for r in &results {
                let label = r.label.replace('"', "'");
                let url = r.url.replace('"', "'");
                push(
                    &mut out,
                    format!(
                        "ttlcd_http_monitor_status_code{{label=\"{}\",url=\"{}\"}} {}",
                        label,
                        url,
                        r.status_code.unwrap_or(0)
                    ),
                );
            }
        }
    }

    // Ping monitor metrics
    {
        let results = state.ping_results.lock().unwrap().clone();
        if !results.is_empty() {
            push(&mut out, "# HELP ttlcd_ping_latency_ms Ping round-trip latency milliseconds (-1=unreachable)".into());
            push(&mut out, "# TYPE ttlcd_ping_latency_ms gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let host = r.host.replace('"', "'");
                let val = r
                    .latency_ms
                    .map_or_else(|| "-1".into(), |v| format!("{v:.3}"));
                push(
                    &mut out,
                    format!("ttlcd_ping_latency_ms{{label=\"{label}\",host=\"{host}\"}} {val}"),
                );
            }
            push(
                &mut out,
                "# HELP ttlcd_ping_loss_percent Ping packet loss percent".into(),
            );
            push(&mut out, "# TYPE ttlcd_ping_loss_percent gauge".into());
            for r in &results {
                let label = r.label.replace('"', "'");
                let host = r.host.replace('"', "'");
                push(
                    &mut out,
                    format!(
                        "ttlcd_ping_loss_percent{{label=\"{}\",host=\"{}\"}} {:.1}",
                        label, host, r.loss_pct
                    ),
                );
            }
        }
    }

    // Weather metrics
    {
        let wd = state.weather_data.lock().unwrap().clone();
        if let Some(temp) = wd.temp_c {
            push(
                &mut out,
                "# HELP ttlcd_weather_temp_c Last fetched weather temperature Celsius".into(),
            );
            push(&mut out, "# TYPE ttlcd_weather_temp_c gauge".into());
            let city = wd.city.replace('"', "'");
            push(
                &mut out,
                format!("ttlcd_weather_temp_c{{city=\"{city}\"}} {temp:.1}"),
            );
            if let Some(humidity) = wd.humidity_pct {
                push(
                    &mut out,
                    "# HELP ttlcd_weather_humidity_pct Last fetched weather humidity percent"
                        .into(),
                );
                push(&mut out, "# TYPE ttlcd_weather_humidity_pct gauge".into());
                push(
                    &mut out,
                    format!("ttlcd_weather_humidity_pct{{city=\"{city}\"}} {humidity}"),
                );
            }
            if let Some(wind) = wd.wind_kmh {
                push(
                    &mut out,
                    "# HELP ttlcd_weather_wind_kmh Last fetched weather wind speed km/h".into(),
                );
                push(&mut out, "# TYPE ttlcd_weather_wind_kmh gauge".into());
                push(
                    &mut out,
                    format!("ttlcd_weather_wind_kmh{{city=\"{city}\"}} {wind:.1}"),
                );
            }
        }
    }

    // Custom metrics source values (if populated)
    {
        let metrics = state.metrics_data.lock().unwrap().clone();
        if !metrics.is_empty() && state.metrics_source.lock().unwrap().is_some() {
            push(
                &mut out,
                "# HELP ttlcd_metrics_source_value Custom metric fetched from HTTP JSON source"
                    .into(),
            );
            push(&mut out, "# TYPE ttlcd_metrics_source_value untyped".into());
            for m in &metrics {
                let label_safe = m.label.replace('"', "'");
                // Try to parse value as a number for Prometheus
                let raw_val = m
                    .value
                    .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-');
                if let Ok(v) = raw_val.parse::<f64>() {
                    push(
                        &mut out,
                        format!("ttlcd_metrics_source_value{{label=\"{label_safe}\"}} {v}"),
                    );
                }
            }
        }
    }

    // RSS feed availability
    {
        let ok = *state.rss_feed_ok.lock().unwrap();
        let url = state.rss_feed_url.lock().unwrap().clone();
        if !url.is_empty() {
            let url_safe = url.replace('"', "'");
            push(
                &mut out,
                "# HELP ttlcd_rss_feed_up 1 if last RSS feed fetch succeeded".into(),
            );
            push(&mut out, "# TYPE ttlcd_rss_feed_up gauge".into());
            push(
                &mut out,
                format!(
                    "ttlcd_rss_feed_up{{url=\"{}\"}} {}",
                    url_safe,
                    if ok { 1 } else { 0 }
                ),
            );
            push(
                &mut out,
                "# HELP ttlcd_rss_feed_items_total Number of RSS items in cache".into(),
            );
            push(&mut out, "# TYPE ttlcd_rss_feed_items_total gauge".into());
            push(
                &mut out,
                format!(
                    "ttlcd_rss_feed_items_total{{url=\"{}\"}} {}",
                    url_safe,
                    state.rss_feed_items.lock().unwrap().len()
                ),
            );
        }
    }

    // Service statuses
    {
        let statuses = state.service_statuses.lock().unwrap().clone();
        if !statuses.is_empty() {
            push(
                &mut out,
                "# HELP ttlcd_service_active 1 if systemd unit is active".into(),
            );
            push(&mut out, "# TYPE ttlcd_service_active gauge".into());
            for svc in &statuses {
                let name_safe = svc.name.replace('"', "'");
                push(
                    &mut out,
                    format!(
                        "ttlcd_service_active{{name=\"{}\"}} {}",
                        name_safe,
                        if svc.state == "active" { 1 } else { 0 }
                    ),
                );
                if svc.state == "failed" {
                    push(
                        &mut out,
                        format!("ttlcd_service_failed{{name=\"{name_safe}\"}} 1"),
                    );
                }
            }
        }
    }

    // ── Docker container metrics ──────────────────────────────────────────────
    {
        let containers = tokio::task::block_in_place(read_docker_containers);
        if !containers.is_empty() {
            push(
                &mut out,
                "# HELP ttlcd_docker_running 1 if container is running, 0 if stopped".into(),
            );
            push(&mut out, "# TYPE ttlcd_docker_running gauge".into());
            for c in &containers {
                let name_safe = c.name.trim_start_matches('/').replace('"', "'");
                let image_safe = c.image.replace('"', "'");
                push(
                    &mut out,
                    format!(
                        "ttlcd_docker_running{{name=\"{}\",image=\"{}\"}} {}",
                        name_safe,
                        image_safe,
                        if c.running { 1 } else { 0 }
                    ),
                );
            }
            let running_count = containers.iter().filter(|c| c.running).count();
            push(
                &mut out,
                "# HELP ttlcd_docker_running_total Number of running containers".into(),
            );
            push(&mut out, "# TYPE ttlcd_docker_running_total gauge".into());
            push(
                &mut out,
                format!("ttlcd_docker_running_total {running_count}"),
            );
            push(
                &mut out,
                "# HELP ttlcd_docker_total_total Total number of containers".into(),
            );
            push(&mut out, "# TYPE ttlcd_docker_total_total gauge".into());
            push(
                &mut out,
                format!("ttlcd_docker_total_total {}", containers.len()),
            );
        }
    }

    // Screensaver metrics
    {
        let timeout = *state.screensaver_timeout_secs.lock().unwrap();
        let active = *state.screensaver_active.lock().unwrap();
        let last = *state.last_activity_secs.lock().unwrap();
        let now_s2 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        push(
            &mut out,
            "# HELP ttlcd_screensaver_active 1 if screensaver is currently active".into(),
        );
        push(&mut out, "# TYPE ttlcd_screensaver_active gauge".into());
        push(
            &mut out,
            format!("ttlcd_screensaver_active {}", if active { 1 } else { 0 }),
        );
        push(&mut out, "# HELP ttlcd_screensaver_timeout_secs Configured screensaver idle timeout (0=disabled)".into());
        push(
            &mut out,
            "# TYPE ttlcd_screensaver_timeout_secs gauge".into(),
        );
        push(
            &mut out,
            format!("ttlcd_screensaver_timeout_secs {timeout}"),
        );
        push(
            &mut out,
            "# HELP ttlcd_seconds_since_activity Seconds since last API activity".into(),
        );
        push(&mut out, "# TYPE ttlcd_seconds_since_activity gauge".into());
        push(
            &mut out,
            format!(
                "ttlcd_seconds_since_activity {}",
                now_s2.saturating_sub(last)
            ),
        );
    }

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(out)
}

// ─── Runtime config snapshot ─────────────────────────────────────────────────

/// GET /api/config — return the current daemon runtime configuration as JSON.
/// Useful for debugging, dashboards, and config-as-code verification.
pub(crate) async fn api_get_config(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct RuntimeConfig {
        version: &'static str,
        mode: String,
        quality: u8,
        rotate_deg: u16,
        headless: bool,
        rotation_modes: Vec<String>,
        rotation_interval: u64,
        preset_count: usize,
        history_count: usize,
        slideshow_images: usize,
        slideshow_interval: u64,
        frame_seq: u64,
    }
    let rotation_modes: Vec<String> = state
        .rotation_modes
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.as_str().to_string())
        .collect();
    HttpResponse::Ok().json(RuntimeConfig {
        version: env!("CARGO_PKG_VERSION"),
        mode: state.mode.lock().unwrap().as_str().to_string(),
        quality: *state.quality.lock().unwrap(),
        rotate_deg: *state.rotate_deg.lock().unwrap(),
        headless: state.headless,
        rotation_modes,
        rotation_interval: *state.rotation_interval.lock().unwrap(),
        preset_count: state.presets.lock().unwrap().len(),
        history_count: state.history.lock().unwrap().len(),
        slideshow_images: state.slideshow_paths.lock().unwrap().len(),
        slideshow_interval: *state.slideshow_interval.lock().unwrap(),
        frame_seq: *state.frame_seq.lock().unwrap(),
    })
}

// ─── Device info ──────────────────────────────────────────────────────────────

/// GET /api/info — return USB device information (serial, firmware).
/// Returns null/null in headless mode (no USB device attached).
pub(crate) async fn api_info(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(Serialize)]
    struct DeviceInfo {
        version: &'static str,
        vendor_id: String,
        product_id: String,
        headless: bool,
        os: String,
        arch: String,
        mode_count: usize,
        build_profile: &'static str,
    }
    HttpResponse::Ok().json(DeviceInfo {
        version: env!("CARGO_PKG_VERSION"),
        vendor_id: format!("0x{:04x}", state.vid),
        product_id: format!("0x{:04x}", state.pid),
        headless: state.headless,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        mode_count: DisplayMode::ALL_NAMES.len(),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    })
}

// ─── SVG status badge ────────────────────────────────────────────────────────

/// GET /api/status.svg — return a Shields.io-style SVG badge showing the
/// current mode.  Suitable for embedding in dashboards and GitHub README files.
///
/// Example: `<img src="http://localhost:7070/api/status.svg">`
pub(crate) async fn api_status_badge(state: web::Data<DaemonState>) -> impl Responder {
    let mode = state.mode.lock().unwrap().as_str().to_string();
    let quality = *state.quality.lock().unwrap();
    let headless = state.headless;
    let brightness = *state.dim_factor.lock().unwrap();

    // Pick badge colour based on mode category
    let badge_color = match mode.as_str() {
        "clock" | "clock_sysinfo" => "#89b4fa",              // blue
        "sysinfo" | "cpu_detail" | "cpu_graph" => "#cba6f7", // mauve
        "network" | "net_graph" => "#94e2d5",                // teal
        "disk_usage" | "disk_io" => "#f9e2af",               // yellow
        "process_top" | "process_memory" => "#a6e3a1",       // green
        "temperature" | "temp_graph" => "#fab387",           // peach
        "ping_graph" => "#a6e3a1",                           // green
        "game_of_life" => "#a6e3a1",                         // green
        "led_clock" => "#fab387",                            // peach
        "bounce_ball" => "#cba6f7",                          // mauve
        "speedometer_gauge" => "#89dceb",                    // sky
        "disk_pie" => "#a6e3a1",                             // green
        "thermal_dashboard" => "#fab387",                    // peach
        "plasma" => "#cba6f7",                               // mauve
        "fireworks" => "#f38ba8",                            // red
        "cpu_heatmap" => "#a6e3a1",                          // green
        "starfield" => "#89b4fa",                            // blue
        "pulse" => "#cba6f7",                                // mauve
        "minimal_clock" => "#cdd6f4",                        // text
        "particle_clock" => "#b4befe",                       // lavender
        "snake" => "#a6e3a1",                                // green
        "cube_3d" => "#89b4fa",                              // blue
        "dvd_bounce" => "#cba6f7",                           // mauve
        "binary_clock" => "#89dceb",                         // sky
        "radar" => "#a6e3a1",                                // green
        "oscilloscope" => "#a6e3a1",                         // green
        "lissajous" => "#cba6f7",                            // mauve
        "pong" => "#f9e2af",                                 // yellow
        "neon_clock" => "#cba6f7",                           // mauve
        "net_heatmap" => "#89b4fa",                          // blue
        "word_clock" => "#cdd6f4",                           // text
        "breakout" => "#f38ba8",                             // red
        "spectrum" => "#89dceb",                             // sky
        "doom_fire" => "#f38ba8",                            // red (fire)
        "boids" => "#a6e3a1",                                // green
        "tunnel" => "#cba6f7",                               // mauve
        "aurora" => "#94e2d5",                               // teal
        "metaballs" => "#cba6f7",                            // mauve
        "sand" => "#f9e2af",                                 // yellow
        "gravity" => "#89b4fa",                              // blue
        "voronoi" => "#a6e3a1",                              // green
        "reaction_diffusion" => "#74c7ec",                   // sapphire
        "snowfall" => "#cdd6f4",                             // text/white
        "hex_grid" => "#a6e3a1",                             // green
        "bubble_chart" => "#89b4fa",                         // blue
        "conway_colors" => "#a6e3a1",                        // green
        "julia_set" => "#cba6f7",                            // mauve
        "confetti" => "#f9e2af",                             // yellow
        "cpu_3d_bars" => "#fab387",                          // peach
        "system_health" => "#a6e3a1",                        // green
        "neon_grid" => "#74c7ec",                            // sapphire/cyan
        "ripple" => "#89dceb",                               // sky
        "hypno" => "#cba6f7",                                // mauve
        "lorenz" => "#cba6f7",                               // mauve
        "mandelbrot" => "#89b4fa",                           // blue
        "kaleidoscope" => "#f38ba8",                         // red (rainbow)
        "dna_helix" => "#94e2d5",                            // teal
        "rain_glass" => "#89dceb",                           // sky
        "perlin_terrain" => "#a6e3a1",                       // green
        "fire_text" => "#f38ba8",                            // red
        "disco_floor" => "#cba6f7",                          // mauve
        "heartbeat" => "#a6e3a1",                            // green
        "clock_faces" => "#89b4fa",                          // blue
        "waveform_bars" => "#cba6f7",                        // mauve
        "city_skyline" => "#89b4fa",                         // blue
        "neon_sign" => "#f38ba8",                            // red
        "gradient_clock" => "#cba6f7",                       // mauve
        "galaxy" => "#89b4fa",                               // blue
        "ocean_waves" => "#74c7ec",                          // sapphire
        "warp_speed" => "#cdd6f4",                           // text/white
        "morphing_blobs" => "#cba6f7",                       // mauve
        "clock_matrix" => "#a6e3a1",                         // green
        "sierpinski" => "#cba6f7",                           // mauve
        "lava_lamp" => "#f38ba8",                            // red/warm
        "pendulum_wave" => "#89b4fa",                        // blue
        "fiber_optic" => "#94e2d5",                          // teal
        "network_globe" => "#89b4fa",                        // blue
        "plasma_ball" => "#cba6f7",                          // mauve
        "moss_growth" => "#a6e3a1",                          // green
        "torus" => "#f38ba8",
        "lemniscate" => "#f5c2e7",
        "system_art" => "#89dceb",
        "fourier_epicycles" => "#cba6f7",
        "rule_30" => "#a6e3a1",
        "nbody_gravity" => "#89dceb",
        "double_pendulum" => "#89b4fa",
        "rose_curve" => "#f5c2e7",
        "truchet" => "#a6e3a1",
        "mandala" => "#f9e2af",
        "flow_field" => "#89b4fa",
        "glitch_art" => "#f38ba8",
        "wave_interference" => "#89b4fa",
        "clifford_attractor" => "#cba6f7",
        "icosahedron" => "#a6e3a1",
        "neural_net" => "#89dceb",
        "ribbon_dance" => "#f38ba8",
        "hex_plasma" => "#f9e2af",
        "sand_waterfall" => "#f9e2af",
        "magnetic_field" => "#89dceb",
        "smoke_plume" => "#a6adc8",
        "color_mosaic" => "#cba6f7",
        "solar_system" => "#89dceb",
        "lightning_storm" => "#cdd6f4",
        "spirograph" => "#cba6f7",
        "fractal_tree" => "#a6e3a1",
        "bubble_rise" => "#89dceb",
        "asteroid_field" => "#f9e2af",                    // yellow
        "pet_cat" => "#fab387",                           // peach
        "ekg_strip" => "#f38ba8",                         // red
        "status_overview" => "#89dceb",                   // sky
        "analog_clock" => "#cba6f7",                      // mauve
        "net_speed_graph" => "#a6e3a1",                   // green
        "cpu_core_bars" => "#cba6f7",                     // mauve
        "disk_io_graph" | "graphs_overview" => "#f9e2af", // yellow
        "countdown" => "#eba0ac",                         // maroon
        "recent_alerts" => "#f38ba8",                     // red
        "idle" => "#585b70",                              // surface2
        _ => "#89dceb",                                   // sky
    };

    let sub = if headless {
        "headless".to_string()
    } else if brightness < 0.99 {
        format!("{}% bright", (brightness * 100.0) as u8)
    } else {
        format!("q{quality}")
    };

    // Simple flat badge SVG (compatible with most browsers and README renderers)
    let mode_disp = mode.replace('_', " ");
    let left_w = 48i32;
    let right_w = (mode_disp.len() as i32 * 7).max(60);
    let total_w = left_w + right_w;
    let _sub_w = (sub.len() as i32 * 6).max(30);

    let lx = left_w / 2;
    let rx = left_w + right_w / 2;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="20"><defs><linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient></defs><rect rx="3" width="{total_w}" height="20" fill="#555"/><rect rx="3" x="{left_w}" width="{right_w}" height="20" fill="{badge_color}"/><rect x="{left_w}" width="4" height="20" fill="{badge_color}"/><rect rx="3" width="{total_w}" height="20" fill="url(#s)"/><g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="11"><text x="{lx}" y="15" fill="#010101" fill-opacity=".3">ttlcd</text><text x="{lx}" y="14">ttlcd</text><text x="{rx}" y="15" fill="#010101" fill-opacity=".3">{mode_disp}</text><text x="{rx}" y="14" fill="#1e1e2e">{mode_disp}</text></g><title>ttlcd — {mode} | {sub}</title></svg>"##
    );

    HttpResponse::Ok()
        .content_type("image/svg+xml")
        .insert_header(("Cache-Control", "no-cache, max-age=0"))
        .body(svg)
}

// ─── Version / uptime ───────────────────────────────────────────────────────

/// GET /api/version — daemon version info.
///
/// Returns: `{"version": "0.6.0", "name": "ttlcd"}`
pub(crate) async fn api_version() -> impl Responder {
    #[derive(serde::Serialize)]
    struct VersionInfo {
        name: &'static str,
        version: &'static str,
    }
    HttpResponse::Ok().json(VersionInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// GET /api/uptime — seconds and human-readable string since daemon start.
///
/// Returns: `{"uptime_secs": 3600, "uptime_human": "1h 0m 0s"}`
pub(crate) async fn api_uptime(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(serde::Serialize)]
    struct UptimeResp {
        uptime_secs: u64,
        uptime_human: String,
    }
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let secs = now_s.saturating_sub(state.daemon_start_secs);
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let human = if d > 0 {
        format!("{d}d {h}h {m}m {s}s")
    } else if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    };
    HttpResponse::Ok().json(UptimeResp {
        uptime_secs: secs,
        uptime_human: human,
    })
}

/// GET /api/panel/info — hardware info for the connected USB panel.
///
/// Returns null serial/firmware when running in headless mode.
pub(crate) async fn api_panel_info(state: web::Data<DaemonState>) -> impl Responder {
    #[derive(serde::Serialize)]
    struct PanelInfoResp {
        connected: bool,
        headless: bool,
        vid: String,
        pid: String,
        serial: Option<String>,
        firmware_byte: Option<u8>,
        status_byte: Option<u8>,
        width: u32,
        height: u32,
    }
    let info = state.panel_info.lock().unwrap().clone();
    HttpResponse::Ok().json(PanelInfoResp {
        connected: info.is_some(),
        headless: state.headless,
        vid: format!("{:04x}", state.vid),
        pid: format!("{:04x}", state.pid),
        serial: info.as_ref().map(|i| i.serial.clone()),
        firmware_byte: info.as_ref().map(|i| i.firmware_byte),
        status_byte: info.as_ref().map(|i| i.status_byte),
        width: PANEL_WIDTH,
        height: PANEL_HEIGHT,
    })
}

// ─── GET /api/modes ──────────────────────────────────────────────────────────

pub(crate) async fn api_modes_catalog(_state: web::Data<DaemonState>) -> impl Responder {
    // Return all modes with their descriptions and categories.
    // Data is derived from the same ModeInfo list used by /api/modes/list.
    use DisplayMode::*;
    let _ = Clock; // ensure DisplayMode is in scope

    fn cat(name: &str) -> &'static str {
        match name {
            "clock" | "big_clock" | "clock_sysinfo" | "led_clock" | "analog_clock"
            | "minimal_clock" | "particle_clock" | "binary_clock" | "neon_clock" | "word_clock" => {
                "clock"
            }
            "countdown" | "stopwatch" | "pomodoro" | "calendar" => "timer",
            "sysinfo" | "cpu_detail" | "temperature" | "process_top" | "memory_detail"
            | "cpu_graph" | "cpu_core_bars" | "cpu_heatmap" | "speedometer_gauge"
            | "thermal_dashboard" | "pulse" | "radar" | "oscilloscope" | "temp_graph"
            | "cpu_freq" | "load_graph" | "mem_graph" | "graphs_overview" | "status_overview"
            | "logged_in_users" | "os_info" | "hostname_ip" | "uptime_fancy" | "docker_status" => {
                "system"
            }
            "network" | "net_graph" | "net_speed_graph" | "net_connections" | "ping_monitor"
            | "ping_graph" | "net_heatmap" => "network",
            "disk_usage" | "disk_pie" | "disk_io" | "disk_io_graph" => "storage",
            "dashboard" | "process_memory" | "recent_alerts" | "metrics" => "overview",
            "weather" | "rss_feed" | "http_status" | "service_status" | "shell_cmd"
            | "log_tail" | "notes" | "battery" | "world_clock" => "info",
            "matrix_rain" | "game_of_life" | "bounce_ball" | "plasma" | "fireworks"
            | "starfield" | "snake" | "cube_3d" | "dvd_bounce" | "lissajous" | "pong"
            | "breakout" | "doom_fire" | "boids" | "tunnel" | "aurora" | "metaballs" | "sand"
            | "gravity" | "voronoi" | "reaction_diffusion" | "snowfall" => "fun",
            "hex_grid" | "bubble_chart" => "system",
            "conway_colors" | "julia_set" | "confetti" | "neon_grid" | "ripple" | "hypno"
            | "lorenz" | "mandelbrot" | "kaleidoscope" | "dna_helix" | "rain_glass"
            | "perlin_terrain" | "fire_text" | "disco_floor" => "fun",
            "heartbeat" | "waveform_bars" => "system",
            "clock_faces" | "gradient_clock" => "clock",
            "city_skyline" | "neon_sign" | "galaxy" | "ocean_waves" | "warp_speed"
            | "morphing_blobs" => "fun",
            "clock_matrix" => "clock",
            "sierpinski" | "lava_lamp" | "pendulum_wave" | "fiber_optic" | "network_globe"
            | "plasma_ball" | "moss_growth" | "asteroid_field" | "pet_cat" | "ekg_strip"
            | "solar_system" | "lightning_storm" | "bubble_rise" | "spirograph"
            | "fractal_tree" | "sand_waterfall" | "magnetic_field" | "smoke_plume"
            | "color_mosaic" | "neural_net" | "ribbon_dance" | "hex_plasma"
            | "wave_interference" | "clifford_attractor" | "icosahedron" | "mandala"
            | "flow_field" | "glitch_art" | "double_pendulum" | "rose_curve" | "truchet"
            | "fourier_epicycles" | "rule_30" | "nbody_gravity" | "torus" | "lemniscate" => "fun",
            "cpu_3d_bars" | "system_health" | "system_art" => "system",
            _ => "other",
        }
    }

    let modes: Vec<serde_json::Value> = DisplayMode::ALL_NAMES
        .iter()
        .map(|&name| {
            serde_json::json!({
                "name": name,
                "category": cat(name),
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "count": modes.len(),
        "modes": modes,
    }))
}

// ─── Modes info ──────────────────────────────────────────────────────────────

/// GET /api/modes/info — return all modes with name and description.
pub(crate) async fn api_modes_info() -> impl Responder {
    #[derive(serde::Serialize)]
    struct ModeInfo {
        name: &'static str,
        description: &'static str,
    }
    let modes: &[ModeInfo] = &[
        ModeInfo { name: "clock",          description: "Large digital clock with date and uptime" },
        ModeInfo { name: "big_clock",      description: "Full-screen giant HH:MM with large seconds and progress bar" },
        ModeInfo { name: "clock_sysinfo",  description: "Split: clock + quick stats" },
        ModeInfo { name: "sysinfo",        description: "CPU, RAM, temperature, load, uptime" },
        ModeInfo { name: "cpu_detail",     description: "Per-core CPU usage grid (up to 32 cores)" },
        ModeInfo { name: "network",        description: "Per-interface Rx/Tx rates and totals" },
        ModeInfo { name: "disk_usage",     description: "Disk partition usage bars" },
        ModeInfo { name: "process_top",    description: "Top processes by CPU usage" },
        ModeInfo { name: "temperature",    description: "Thermal sensor readings with colour-coded bars" },
        ModeInfo { name: "memory_detail",  description: "Detailed RAM/swap breakdown" },
        ModeInfo { name: "countdown",      description: "Countdown timer with colour transitions" },
        ModeInfo { name: "cpu_graph",      description: "CPU usage history bar chart (2 min)" },
        ModeInfo { name: "net_graph",      description: "Network I/O history chart (60 s)" },
        ModeInfo { name: "recent_alerts",  description: "Last 5 notifications as on-panel log" },
        ModeInfo { name: "process_memory", description: "Top processes ranked by RAM usage" },
        ModeInfo { name: "disk_io",        description: "Disk read/write throughput per device" },
        ModeInfo { name: "calendar",       description: "Full-month calendar with today highlighted" },
        ModeInfo { name: "stopwatch",      description: "Running stopwatch with lap times" },
        ModeInfo { name: "metrics",        description: "Custom key/value metrics dashboard (up to 8 items)" },
        ModeInfo { name: "weather",        description: "Weather snapshot — push via /api/weather or web UI" },
        ModeInfo { name: "notes",          description: "Bulletin board — push a list of text items via /api/notes" },
        ModeInfo { name: "qr_code",         description: "Display a scannable QR code — push payload via POST /api/qr" },
        ModeInfo { name: "pomodoro",        description: "Pomodoro focus timer: 25 min work / 5 min break cycles with visual progress ring" },
        ModeInfo { name: "battery",        description: "Battery level, charging status, voltage and estimated time remaining (Linux laptops)" },
        ModeInfo { name: "world_clock",    description: "World clock showing up to 4 cities with configurable UTC offsets and day/night coloring" },
        ModeInfo { name: "ticker",         description: "Horizontally scrolling text ticker" },
        ModeInfo { name: "http_status",    description: "HTTP health monitor dashboard — configure endpoints via /api/http_status" },
        ModeInfo { name: "shell_cmd",      description: "Shell command output viewer — set command via POST /api/shell_cmd" },
        ModeInfo { name: "ping_monitor",   description: "ICMP ping latency dashboard — configure hosts via /api/ping_monitor" },
        ModeInfo { name: "dashboard",      description: "Four-quadrant overview: clock, CPU/RAM, network, temperature" },
        ModeInfo { name: "log_tail",       description: "Live log tail viewer — configure file via POST /api/log_tail" },
        ModeInfo { name: "cpu_freq",       description: "Per-core CPU clock frequency grid (reads /proc/cpuinfo)" },
        ModeInfo { name: "pet_cat",        description: "Pet cat — pixel-art cat that sleeps, grooms, stretches and plays with a ball; coat and room are configurable" },
        ModeInfo { name: "ekg_strip",      description: "EKG rhythm strip — P wave, QRS complex and T wave on scrolling ECG paper; heart rate follows CPU load (70 bpm idle to 160 bpm)" },
        ModeInfo { name: "idle",           description: "Idle — no new frames; the last one stays on screen" },
    ];
    HttpResponse::Ok().json(modes)
}

// ─── API documentation ────────────────────────────────────────────────────────

/// GET /api/docs — brief HTML reference for all API endpoints.
///
/// Useful for quick lookup without reading source code.
pub(crate) async fn api_docs() -> HttpResponse {
    let version = env!("CARGO_PKG_VERSION");
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ttlcd API Reference v{version}</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ background: #1e1e2e; color: #cdd6f4; font-family: 'Segoe UI', system-ui, sans-serif; padding: 2rem; max-width: 900px; margin: 0 auto; }}
    h1 {{ color: #cba6f7; font-size: 1.8rem; margin-bottom: 0.25rem; }}
    h1 small {{ color: #6c7086; font-size: 0.75rem; font-weight: 400; margin-left: 0.5rem; }}
    h2 {{ color: #89b4fa; font-size: 1.1rem; margin: 1.5rem 0 0.6rem; border-bottom: 1px solid #313244; padding-bottom: 0.4rem; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.85rem; margin-bottom: 0.5rem; }}
    th {{ text-align: left; color: #a6adc8; padding: 0.3rem 0.6rem; background: #181825; font-weight: 600; }}
    td {{ padding: 0.3rem 0.6rem; border-bottom: 1px solid #313244; vertical-align: top; }}
    td:first-child {{ font-family: monospace; color: #94e2d5; white-space: nowrap; }}
    td:nth-child(2) {{ color: #89b4fa; white-space: nowrap; }}
    code {{ background: #313244; padding: 0.05rem 0.3rem; border-radius: 3px; font-family: monospace; font-size: 0.82rem; }}
    a {{ color: #89b4fa; }}
    .badge-get    {{ color: #a6e3a1; }}
    .badge-post   {{ color: #89b4fa; }}
    .badge-delete {{ color: #f38ba8; }}
    footer {{ margin-top: 2rem; color: #6c7086; font-size: 0.8rem; }}
  </style>
</head>
<body>
  <h1>ttlcd API Reference <small>v{version}</small></h1>
  <p style="color:#a6adc8;margin-top:0.5rem">HTTP JSON API. All endpoints respond to <code>localhost:7070</code> by default. <a href="/">← Web UI</a></p>

  <h2>Status &amp; Info</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/status</td><td class="badge-get">GET</td><td>Full daemon status (mode, version, frame_seq, rotation, etc.)</td></tr>
    <tr><td>/api/version</td><td class="badge-get">GET</td><td>Version string only</td></tr>
    <tr><td>/api/uptime</td><td class="badge-get">GET</td><td>Daemon uptime in seconds and human-readable</td></tr>
    <tr><td>/api/panel/info</td><td class="badge-get">GET</td><td>USB panel hardware info (serial, firmware, connection status)</td></tr>
    <tr><td>/api/panel/reconnect</td><td class="badge-post">POST</td><td>Trigger an immediate USB reconnect attempt (non-blocking)</td></tr>
    <tr><td>/api/modes</td><td class="badge-get">GET</td><td>List of all mode names</td></tr>
    <tr><td>/api/modes/info</td><td class="badge-get">GET</td><td>Mode names with descriptions</td></tr>
    <tr><td>/health</td><td class="badge-get">GET</td><td>Liveness probe (plain text)</td></tr>
    <tr><td>/api/status.svg</td><td class="badge-get">GET</td><td>Shields.io-style SVG badge for current mode</td></tr>
  </tbody></table>

  <h2>Display Control</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/mode</td><td class="badge-post">POST</td><td>Set mode <code>&#123;"mode":"clock"&#125;</code></td></tr>
    <tr><td>/api/mode/next</td><td class="badge-post">POST</td><td>Advance to next mode in ALL_NAMES</td></tr>
    <tr><td>/api/mode/prev</td><td class="badge-post">POST</td><td>Go to previous mode</td></tr>
    <tr><td>/api/quality</td><td class="badge-post">POST</td><td>Set JPEG quality <code>&#123;"quality":85&#125;</code></td></tr>
    <tr><td>/api/orient</td><td class="badge-post">POST</td><td>Set rotation <code>&#123;"degrees":90&#125;</code></td></tr>
    <tr><td>/api/brightness</td><td class="badge-get">GET</td><td>Get brightness factor</td></tr>
    <tr><td>/api/brightness</td><td class="badge-post">POST</td><td>Set brightness <code>&#123;"brightness":0.8&#125;</code></td></tr>
    <tr><td>/api/display/freeze</td><td class="badge-post">POST</td><td>Freeze display (last frame stays)</td></tr>
    <tr><td>/api/display/unfreeze</td><td class="badge-post">POST</td><td>Resume display updates</td></tr>
    <tr><td>/api/display/frozen</td><td class="badge-get">GET</td><td>Returns <code>&#123;"frozen":true|false&#125;</code></td></tr>
    <tr><td>/api/blank</td><td class="badge-post">POST</td><td>Send a solid black frame</td></tr>
  </tbody></table>

  <h2>Content Push</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/text</td><td class="badge-post">POST</td><td>Push custom text (title, subtitle, lines, progress, color, duration_secs)</td></tr>
    <tr><td>/api/clear</td><td class="badge-post">POST</td><td>Return to previous live mode</td></tr>
    <tr><td>/api/image</td><td class="badge-post">POST</td><td>Upload image (multipart/form-data)</td></tr>
    <tr><td>/api/image/path</td><td class="badge-post">POST</td><td>Display image by server path</td></tr>
    <tr><td>/api/banner</td><td class="badge-post">POST</td><td>Large centred text banner</td></tr>
    <tr><td>/api/alert</td><td class="badge-post">POST</td><td>Alert-style push (uses red color)</td></tr>
    <tr><td>/api/notify</td><td class="badge-post">POST</td><td>Webhook-compatible notification</td></tr>
    <tr><td>/api/test</td><td class="badge-post">POST</td><td>Show test card for 5s</td></tr>
  </tbody></table>

  <h2>Presets</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/presets</td><td class="badge-get">GET</td><td>List all presets</td></tr>
    <tr><td>/api/presets</td><td class="badge-post">POST</td><td>Save preset <code>&#123;"name":"…","payload":…&#125;</code></td></tr>
    <tr><td>/api/presets</td><td class="badge-delete">DELETE</td><td>Clear all presets</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-get">GET</td><td>Get single preset payload</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-post">POST</td><td>Apply preset to panel</td></tr>
    <tr><td>/api/presets/&#123;name&#125;</td><td class="badge-delete">DELETE</td><td>Delete preset</td></tr>
    <tr><td>/api/presets/export</td><td class="badge-get">GET</td><td>Export all presets as JSON</td></tr>
    <tr><td>/api/presets/import</td><td class="badge-post">POST</td><td>Import presets from JSON file</td></tr>
  </tbody></table>

  <h2>Timers</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/countdown</td><td class="badge-post">POST</td><td>Start countdown <code>&#123;"seconds":300,"label":"Break"&#125;</code></td></tr>
    <tr><td>/api/countdown</td><td class="badge-delete">DELETE</td><td>Cancel countdown</td></tr>
    <tr><td>/api/stopwatch</td><td class="badge-get">GET</td><td>Get stopwatch state</td></tr>
    <tr><td>/api/stopwatch/start</td><td class="badge-post">POST</td><td>Start/resume stopwatch</td></tr>
    <tr><td>/api/stopwatch/stop</td><td class="badge-post">POST</td><td>Pause stopwatch</td></tr>
    <tr><td>/api/stopwatch/lap</td><td class="badge-post">POST</td><td>Record lap</td></tr>
    <tr><td>/api/stopwatch/reset</td><td class="badge-post">POST</td><td>Reset stopwatch</td></tr>
    <tr><td>/api/schedule</td><td class="badge-post">POST</td><td>Schedule delayed notification</td></tr>
  </tbody></table>

  <h2>Slideshow</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/slideshow</td><td class="badge-post">POST</td><td>Start slideshow <code>&#123;"dir":"/path","interval":5&#125;</code></td></tr>
    <tr><td>/api/slideshow/next</td><td class="badge-post">POST</td><td>Next image</td></tr>
    <tr><td>/api/slideshow/prev</td><td class="badge-post">POST</td><td>Previous image</td></tr>
    <tr><td>/api/slideshow/shuffle</td><td class="badge-post">POST</td><td>Randomise image order</td></tr>
    <tr><td>/api/slideshow/images</td><td class="badge-get">GET</td><td>List images in current slideshow directory</td></tr>
  </tbody></table>

  <h2>Auto-Rotation</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/rotation</td><td class="badge-get">GET</td><td>Get rotation config</td></tr>
    <tr><td>/api/rotation</td><td class="badge-post">POST</td><td>Set <code>&#123;"modes":["clock","sysinfo"],"interval":30&#125;</code></td></tr>
  </tbody></table>

  <h2>Custom Metrics</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/metrics</td><td class="badge-get">GET</td><td>Get current metrics</td></tr>
    <tr><td>/api/metrics</td><td class="badge-post">POST</td><td>Push metrics <code>&#123;"title":"…","metrics":[&#123;"label":"CPU","value":"72%","color":"blue"&#125;]&#125;</code></td></tr>
    <tr><td>/api/metrics</td><td class="badge-delete">DELETE</td><td>Clear metrics</td></tr>
  </tbody></table>

  <h2>Weather &amp; Notes</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/weather</td><td class="badge-get">GET</td><td>Get cached weather data</td></tr>
    <tr><td>/api/weather</td><td class="badge-post">POST</td><td>Push weather snapshot (WeatherData JSON)</td></tr>
    <tr><td>/api/weather</td><td class="badge-delete">DELETE</td><td>Clear weather data</td></tr>
    <tr><td>/api/weather/fetch</td><td class="badge-post">POST</td><td>Trigger server-side wttr.in fetch <code>&#123;"location":"London","switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/weather/auto</td><td class="badge-put">PUT</td><td>Enable auto-fetch <code>&#123;"location":"London","interval_secs":1800&#125;</code></td></tr>
    <tr><td>/api/weather/auto</td><td class="badge-delete">DELETE</td><td>Disable weather auto-fetch</td></tr>
    <tr><td>/api/notes</td><td class="badge-get">GET</td><td>Get current notes</td></tr>
    <tr><td>/api/notes</td><td class="badge-post">POST</td><td>Push notes <code>&#123;"title":"…","items":["…"],"append":false&#125;</code></td></tr>
    <tr><td>/api/notes</td><td class="badge-delete">DELETE</td><td>Clear notes</td></tr>
    <tr><td>/api/qr</td><td class="badge-get">GET</td><td>Get current QR code payload and title</td></tr>
    <tr><td>/api/qr</td><td class="badge-post">POST</td><td>Set QR code <code>{{&quot;payload&quot;:&quot;https://…&quot;,&quot;title&quot;:&quot;…&quot;,&quot;show&quot;:true}}</code></td></tr>
    <tr><td>/api/qr</td><td class="badge-delete">DELETE</td><td>Clear QR code and return to previous mode</td></tr>
    <tr><td>/api/image/url</td><td class="badge-post">POST</td><td>Fetch image from URL and display it <code>{{&quot;url&quot;:&quot;https://…&quot;,&quot;letterbox&quot;:true}}</code></td></tr>
    <tr><td>/api/preview.png</td><td class="badge-get">GET</td><td>Current panel frame as lossless PNG</td></tr>
  </tbody></table>

  <h2>System Stats</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/sysinfo</td><td class="badge-get">GET</td><td>CPU, RAM, load, uptime</td></tr>
    <tr><td>/api/temperatures</td><td class="badge-get">GET</td><td>Thermal sensor readings</td></tr>
    <tr><td>/api/network</td><td class="badge-get">GET</td><td>Network interface rates</td></tr>
    <tr><td>/api/processes</td><td class="badge-get">GET</td><td>Top processes by CPU</td></tr>
    <tr><td>/api/disks</td><td class="badge-get">GET</td><td>Disk partition usage</td></tr>
    <tr><td>/api/cpu/history</td><td class="badge-get">GET</td><td>CPU usage ring-buffer (120 samples)</td></tr>
    <tr><td>/api/net/history</td><td class="badge-get">GET</td><td>Net Rx/Tx ring-buffer (60 samples)</td></tr>
    <tr><td>/api/sys/snapshot</td><td class="badge-get">GET</td><td>All system stats in one call</td></tr>
  </tbody></table>

  <h2>Alerts &amp; Scheduling</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/alerts/thresholds</td><td class="badge-get">GET</td><td>Get CPU/temp alert thresholds</td></tr>
    <tr><td>/api/alerts/thresholds</td><td class="badge-post">POST</td><td>Set thresholds <code>&#123;"cpu_pct":90,"temp_c":80,"cooldown_secs":300&#125;</code></td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-get">GET</td><td>Get night-dim schedule</td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-post">POST</td><td>Set <code>&#123;"start_hour":22,"end_hour":7,"dim_level":0.15&#125;</code></td></tr>
    <tr><td>/api/dim/schedule</td><td class="badge-delete">DELETE</td><td>Clear dim schedule</td></tr>
    <tr><td>/api/history</td><td class="badge-get">GET</td><td>Notification history <code>?limit=20&amp;format=json|csv</code></td></tr>
    <tr><td>/api/history</td><td class="badge-delete">DELETE</td><td>Clear history</td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-get">GET</td><td>List time-of-day mode schedule entries</td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-post">POST</td><td>Add schedule entry <code>{{&quot;hour&quot;:9,&quot;minute&quot;:0,&quot;mode&quot;:&quot;clock_sysinfo&quot;,&quot;label&quot;:&quot;…&quot;}}</code></td></tr>
    <tr><td>/api/schedule/mode</td><td class="badge-delete">DELETE</td><td>Clear all schedule entries</td></tr>
    <tr><td>/api/schedule/mode/{{index}}</td><td class="badge-delete">DELETE</td><td>Remove one schedule entry by index</td></tr>
    <tr><td>/api/fps/history</td><td class="badge-get">GET</td><td>Render FPS ring-buffer (last 60 samples)</td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-get">GET</td><td>Get webhook forward URL</td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-post">POST</td><td>Set webhook forward URL <code>{{&quot;url&quot;:&quot;https://…&quot;}}</code></td></tr>
    <tr><td>/api/webhook/config</td><td class="badge-delete">DELETE</td><td>Clear webhook forward URL</td></tr>
  </tbody></table>

  <h2>Profiles &amp; Config</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/profiles</td><td class="badge-get">GET</td><td>List saved display profiles</td></tr>
    <tr><td>/api/profiles</td><td class="badge-post">POST</td><td>Save current state as profile <code>&#123;"name":"night"&#125;</code></td></tr>
    <tr><td>/api/profiles/&#123;name&#125;/apply</td><td class="badge-post">POST</td><td>Apply saved profile</td></tr>
    <tr><td>/api/profiles/&#123;name&#125;</td><td class="badge-delete">DELETE</td><td>Delete profile</td></tr>
    <tr><td>/api/config</td><td class="badge-get">GET</td><td>Return effective daemon configuration</td></tr>
    <tr><td>/api/state/reload</td><td class="badge-post">POST</td><td>Reload persisted state from disk</td></tr>
    <tr><td>/api/info</td><td class="badge-get">GET</td><td>Alias for /api/status</td></tr>
  </tbody></table>

  <h2>Preview &amp; Screenshot</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/preview</td><td class="badge-get">GET</td><td>Current panel frame as JPEG</td></tr>
    <tr><td>/api/screenshot</td><td class="badge-get">GET</td><td>Alias for /api/preview (download filename)</td></tr>
    <tr><td>/api/screenshot.png</td><td class="badge-get">GET</td><td>Current panel frame as lossless PNG</td></tr>
    <tr><td>/api/stream</td><td class="badge-get">GET</td><td>MJPEG live stream</td></tr>
    <tr><td>/api/events</td><td class="badge-get">GET</td><td>SSE stream: <code>frame</code> and <code>mode</code> events</td></tr>
    <tr><td>/api/render</td><td class="badge-post">POST</td><td>Render text payload to JPEG (no panel update)</td></tr>
    <tr><td>/metrics</td><td class="badge-get">GET</td><td>Prometheus-format metrics</td></tr>
  </tbody></table>

  <h2>Pomodoro Timer</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/pomodoro</td><td class="badge-get">GET</td><td>Get Pomodoro state</td></tr>
    <tr><td>/api/pomodoro/start</td><td class="badge-post">POST</td><td>Start/resume timer</td></tr>
    <tr><td>/api/pomodoro/pause</td><td class="badge-post">POST</td><td>Pause timer</td></tr>
    <tr><td>/api/pomodoro/skip</td><td class="badge-post">POST</td><td>Skip current phase</td></tr>
    <tr><td>/api/pomodoro/stop</td><td class="badge-post">POST</td><td>Stop and reset timer</td></tr>
    <tr><td>/api/pomodoro/reset</td><td class="badge-post">POST</td><td>Reset to work phase</td></tr>
    <tr><td>/api/pomodoro/config</td><td class="badge-post">POST</td><td>Configure durations <code>&#123;"work_secs":1500,"short_break_secs":300&#125;</code></td></tr>
  </tbody></table>

  <h2>World Clock, Ticker &amp; Battery</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/world_clock</td><td class="badge-get">GET</td><td>Get city list</td></tr>
    <tr><td>/api/world_clock</td><td class="badge-post">POST</td><td>Set cities <code>[&#123;"city":"Berlin","utc_offset_mins":60&#125;]</code></td></tr>
    <tr><td>/api/world_clock</td><td class="badge-delete">DELETE</td><td>Reset to default cities</td></tr>
    <tr><td>/api/ticker</td><td class="badge-get">GET</td><td>Get ticker text and color</td></tr>
    <tr><td>/api/ticker</td><td class="badge-post">POST</td><td>Set ticker <code>&#123;"text":"…","color":"teal","switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/ticker</td><td class="badge-delete">DELETE</td><td>Clear ticker</td></tr>
    <tr><td>/api/battery</td><td class="badge-get">GET</td><td>Battery level and status</td></tr>
  </tbody></table>

  <h2>HTTP Monitor, Shell Command &amp; Ping</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/http_status</td><td class="badge-get">GET</td><td>Get monitors and latest results</td></tr>
    <tr><td>/api/http_status</td><td class="badge-post">POST</td><td>Add monitor <code>&#123;"label":"…","url":"https://…"&#125;</code></td></tr>
    <tr><td>/api/http_status/&#123;index&#125;</td><td class="badge-delete">DELETE</td><td>Remove monitor by index</td></tr>
    <tr><td>/api/http_status/interval</td><td class="badge-get">GET</td><td>Set check interval <code>&#123;"secs":30&#125;</code> (PUT)</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-get">GET</td><td>Get command, output, status</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-post">POST</td><td>Set command <code>&#123;"cmd":"…","interval_secs":10,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/shell_cmd/run</td><td class="badge-post">POST</td><td>Force immediate re-run</td></tr>
    <tr><td>/api/shell_cmd</td><td class="badge-delete">DELETE</td><td>Clear command</td></tr>
    <tr><td>/api/ping_monitor</td><td class="badge-get">GET</td><td>Get hosts and latest ping results</td></tr>
    <tr><td>/api/ping_monitor</td><td class="badge-post">POST</td><td>Add host <code>&#123;"label":"…","host":"8.8.8.8"&#125;</code></td></tr>
    <tr><td>/api/ping_monitor/&#123;index&#125;</td><td class="badge-delete">DELETE</td><td>Remove host by index</td></tr>
    <tr><td>/api/ping_monitor/interval</td><td class="badge-get">GET</td><td>Set ping interval <code>&#123;"secs":10&#125;</code> (PUT)</td></tr>
    <tr><td>/api/log_tail</td><td class="badge-get">GET</td><td>Get log tail config and cached lines</td></tr>
    <tr><td>/api/log_tail</td><td class="badge-post">POST</td><td>Set file <code>&#123;"path":"/var/log/syslog","n":16,"interval_secs":3,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/log_tail</td><td class="badge-delete">DELETE</td><td>Clear log tail config</td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-get">GET</td><td>Get HTTP JSON metrics source config</td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-post">POST</td><td>Set source <code>&#123;"url":"…","fields":[&#123;"label":"CPU","path":"cpu_pct","format":"&#123;&#125;%","color":"green"&#125;]&#125;</code></td></tr>
    <tr><td>/api/metrics/source</td><td class="badge-delete">DELETE</td><td>Clear metrics source</td></tr>
    <tr><td>/api/metrics/source/fetch</td><td class="badge-post">POST</td><td>Trigger immediate fetch from metrics source</td></tr>
    <tr><td>/api/rss</td><td class="badge-get">GET</td><td>Get RSS feed config, cached title and items</td></tr>
    <tr><td>/api/rss</td><td class="badge-post">POST</td><td>Set feed <code>&#123;"url":"https://…","interval_secs":300,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/rss</td><td class="badge-delete">DELETE</td><td>Clear RSS feed config</td></tr>
    <tr><td>/api/rss/fetch</td><td class="badge-post">POST</td><td>Trigger immediate fetch of the configured RSS feed</td></tr>
    <tr><td>/api/services</td><td class="badge-get">GET</td><td>Get service unit list and latest statuses</td></tr>
    <tr><td>/api/services</td><td class="badge-post">POST</td><td>Set units <code>&#123;"units":["nginx.service","sshd"],"interval_secs":10,"switch_mode":true&#125;</code></td></tr>
    <tr><td>/api/services</td><td class="badge-delete">DELETE</td><td>Clear service monitor</td></tr>
  </tbody></table>

  <h2>System Info (New)</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/sys/os</td><td class="badge-get">GET</td><td>OS name, kernel, CPU, memory, uptime, arch</td></tr>
    <tr><td>/api/sys/net</td><td class="badge-get">GET</td><td>Detailed network interfaces: addresses, counters, errors</td></tr>
    <tr><td>/api/sys/snapshot</td><td class="badge-get">GET</td><td>All system stats in one call</td></tr>
    <tr><td>/api/docker</td><td class="badge-get">GET</td><td>Docker containers list (running/stopped)</td></tr>
    <tr><td>/api/sessions</td><td class="badge-get">GET</td><td>Active user sessions from <code>who -u</code></td></tr>
  </tbody></table>

  <h2>Progress Display</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/progress</td><td class="badge-get">GET</td><td>Get current progress state</td></tr>
    <tr><td>/api/progress</td><td class="badge-post">POST</td><td>Update progress <code>&#123;"label":"…","value":47,"total":200,"color":"blue","start_timer":true&#125;</code> — auto-switches to Progress mode</td></tr>
    <tr><td>/api/progress</td><td class="badge-delete">DELETE</td><td>Clear progress and return to previous mode</td></tr>
  </tbody></table>

  <h2>Screensaver / Idle Timeout</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/screensaver</td><td class="badge-get">GET</td><td>Get screensaver config: timeout, mode, active status, seconds since activity</td></tr>
    <tr><td>/api/screensaver</td><td class="badge-post">POST</td><td>Configure screensaver <code>&#123;"timeout_secs":300,"mode":"matrix_rain"&#125;</code> — 0 to disable</td></tr>
    <tr><td>/api/screensaver</td><td class="badge-delete">DELETE</td><td>Disable screensaver</td></tr>
    <tr><td>/api/screensaver/wake</td><td class="badge-post">POST</td><td>Immediately deactivate screensaver and restore previous mode</td></tr>
  </tbody></table>

  <h2>Authentication</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/auth/status</td><td class="badge-get">GET</td><td>Check if API key auth is active</td></tr>
    <tr><td>/api/auth/key</td><td class="badge-post">POST</td><td>Enable Bearer token auth <code>&#123;"key":"secret"&#125;</code></td></tr>
    <tr><td>/api/auth/key</td><td class="badge-delete">DELETE</td><td>Disable auth (clears key)</td></tr>
  </tbody></table>

  <h2>Push Notifications (ntfy.sh)</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/ntfy/config</td><td class="badge-get">GET</td><td>Get ntfy.sh topic URL</td></tr>
    <tr><td>/api/ntfy/config</td><td class="badge-post">POST</td><td>Set topic <code>&#123;"url":"https://ntfy.sh/my-topic"&#125;</code></td></tr>
    <tr><td>/api/ntfy/config</td><td class="badge-delete">DELETE</td><td>Disable ntfy notifications</td></tr>
    <tr><td>/api/ntfy/test</td><td class="badge-post">POST</td><td>Send test notification to configured topic</td></tr>
  </tbody></table>

  <h2>Batch API</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/batch</td><td class="badge-post">POST</td><td>Execute up to 20 operations atomically <code>[&#123;"method":"POST","path":"/api/mode","body":&#123;"mode":"sysinfo"&#125;&#125;,...]</code> — returns per-op status array</td></tr>
  </tbody></table>

  <h2>On-Demand Render</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/render/&#123;mode&#125;</td><td class="badge-get">GET</td><td>Render any mode to JPEG without updating the panel. Query: <code>quality=85&amp;rotate=0</code></td></tr>
  </tbody></table>

  <h2>Settings</h2>
  <table><thead><tr><th>Endpoint</th><th>Method</th><th>Description</th></tr></thead><tbody>
    <tr><td>/api/settings/export</td><td class="badge-get">GET</td><td>Export all settings as JSON</td></tr>
    <tr><td>/api/settings/import</td><td class="badge-post">POST</td><td>Import settings from JSON file</td></tr>
  </tbody></table>

  <footer>ttlcd v{version} — <a href="/">Back to Web UI</a></footer>
</body>
</html>"##
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "no-cache"))
        .body(html)
}

// ─── FPS history ─────────────────────────────────────────────────────────────

/// GET /api/fps/history — return the render-FPS ring-buffer as a JSON array.
///
/// Returns up to 60 recent FPS samples (EMA-smoothed), oldest first.
/// Use this to draw a sparkline showing render performance over time.
pub(crate) async fn api_fps_history(state: web::Data<DaemonState>) -> impl Responder {
    let fh = state.fps_history.lock().unwrap();
    let samples: Vec<f32> = fh.iter().copied().collect();
    HttpResponse::Ok().json(serde_json::json!({
        "samples": samples,
        "current": state.render_fps.lock().unwrap().clone(),
        "count":   samples.len(),
    }))
}
