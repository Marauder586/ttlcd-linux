//! Endpoints reporting system data and configuring the monitors.

use super::*;

// ─── System data JSON endpoints ───────────────────────────────────────────────

/// GET /api/processes?limit=N — return top N processes by CPU usage.
pub(crate) async fn api_processes_json(
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    use sysinfo::{ProcessesToUpdate, System};
    let limit: usize = query
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
        .min(50);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    #[derive(Serialize)]
    struct ProcessInfo {
        pid: u32,
        name: String,
        cpu_pct: f32,
        mem_kb: u64,
        status: String,
    }

    let mut procs: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().to_string(),
            cpu_pct: p.cpu_usage(),
            mem_kb: p.memory() / 1024,
            status: format!("{:?}", p.status()),
        })
        .collect();

    procs.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    procs.truncate(limit);

    HttpResponse::Ok().json(procs)
}

/// GET /api/disks — return all disk partition usage info.
pub(crate) async fn api_disks_json() -> impl Responder {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();

    #[derive(Serialize)]
    struct DiskInfo {
        name: String,
        mount: String,
        fs: String,
        total_gb: f64,
        used_gb: f64,
        free_gb: f64,
        used_pct: f64,
        removable: bool,
    }

    let info: Vec<DiskInfo> = disks
        .iter()
        .map(|d| {
            let total = d.total_space();
            let free = d.available_space();
            let used = total.saturating_sub(free);
            DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount: d.mount_point().to_string_lossy().to_string(),
                fs: d.file_system().to_string_lossy().to_string(),
                total_gb: total as f64 / 1_073_741_824.0,
                used_gb: used as f64 / 1_073_741_824.0,
                free_gb: free as f64 / 1_073_741_824.0,
                used_pct: if total > 0 {
                    used as f64 / total as f64 * 100.0
                } else {
                    0.0
                },
                removable: d.is_removable(),
            }
        })
        .collect();

    HttpResponse::Ok().json(info)
}

// ─── Network interface JSON ───────────────────────────────────────────────────

/// GET /api/network — return all network interfaces with Rx/Tx byte totals
/// and per-second rates (calculated over a ~1-second window).
pub(crate) async fn api_network_json() -> impl Responder {
    use sysinfo::Networks;

    #[derive(Serialize)]
    struct IfaceInfo {
        name: String,
        rx_bytes: u64,
        tx_bytes: u64,
        rx_per_sec: u64,
        tx_per_sec: u64,
        mac: String,
    }

    // Sample 1 — capture initial bytes
    let nets1 = Networks::new_with_refreshed_list();
    let t0 = std::time::Instant::now();
    let snap1: Vec<(String, u64, u64)> = nets1
        .iter()
        .map(|(name, n)| (name.clone(), n.total_received(), n.total_transmitted()))
        .collect();

    // Sleep ~1 s to compute per-second rates
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    let mut nets2 = Networks::new_with_refreshed_list();
    nets2.refresh(false);
    let elapsed = t0.elapsed().as_secs_f64().max(0.001);

    let ifaces: Vec<IfaceInfo> = nets2
        .iter()
        .map(|(name, n)| {
            let rx_bytes = n.total_received();
            let tx_bytes = n.total_transmitted();
            let (rx0, tx0) = snap1
                .iter()
                .find(|(nm, ..)| nm == name)
                .map_or((rx_bytes, tx_bytes), |(_, rx, tx)| (*rx, *tx));
            let rx_per_sec = ((rx_bytes.saturating_sub(rx0)) as f64 / elapsed) as u64;
            let tx_per_sec = ((tx_bytes.saturating_sub(tx0)) as f64 / elapsed) as u64;
            IfaceInfo {
                name: name.clone(),
                rx_bytes,
                tx_bytes,
                rx_per_sec,
                tx_per_sec,
                mac: n.mac_address().to_string(),
            }
        })
        .collect();

    HttpResponse::Ok().json(ifaces)
}

// ─── Temperature sensor JSON ──────────────────────────────────────────────────

/// GET /api/temperatures — return all thermal sensor readings.
pub(crate) async fn api_temperatures_json() -> impl Responder {
    use sysinfo::Components;

    #[derive(Serialize)]
    struct SensorInfo {
        label: String,
        current: f32,
        high: Option<f32>,
        critical: Option<f32>,
    }

    let mut comps = Components::new_with_refreshed_list();
    comps.refresh(false);

    let sensors: Vec<SensorInfo> = comps
        .iter()
        .filter_map(|c| {
            let temp = c.temperature()?;
            if temp.is_nan() || temp <= 0.0 {
                return None;
            }
            Some(SensorInfo {
                label: c.label().to_string(),
                current: temp,
                high: c.max(),
                critical: c.critical(),
            })
        })
        .collect();

    HttpResponse::Ok().json(sensors)
}

// ─── CPU & net history snapshots ─────────────────────────────────────────────

/// GET /api/cpu/history — return the last 120 CPU usage samples (% 0–100).
pub(crate) async fn api_cpu_history() -> impl Responder {
    #[derive(Serialize)]
    struct CpuHistoryResp {
        /// Samples, oldest first. Up to 120 values.
        samples: Vec<f32>,
        /// Number of samples in the buffer.
        count: usize,
        /// Maximum capacity of the ring-buffer.
        capacity: usize,
    }
    let samples = get_cpu_history();
    let count = samples.len();
    HttpResponse::Ok().json(CpuHistoryResp {
        samples,
        count,
        capacity: 120,
    })
}

/// GET /api/net/history — return the last 60 network Rx/Tx samples (bytes/sec).
pub(crate) async fn api_net_history() -> impl Responder {
    #[derive(Serialize)]
    struct NetHistoryResp {
        rx_bps: Vec<u64>,
        tx_bps: Vec<u64>,
        count: usize,
        capacity: usize,
    }
    let (rx, tx) = get_net_history();
    let count = rx.len();
    HttpResponse::Ok().json(NetHistoryResp {
        rx_bps: rx,
        tx_bps: tx,
        count,
        capacity: 60,
    })
}

// ─── OS Info API ──────────────────────────────────────────────────────────────

/// GET /api/sys/os — return OS, kernel, CPU, and architecture information.
pub(crate) async fn api_sys_os() -> impl Responder {
    // OS release info
    let (os_name, os_version, os_pretty) = std::fs::read_to_string("/etc/os-release")
        .map(|text| {
            let mut name = String::new();
            let mut version = String::new();
            let mut pretty = String::new();
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("NAME=") {
                    name = v.trim_matches('"').to_string();
                }
                if let Some(v) = line.strip_prefix("VERSION_ID=") {
                    version = v.trim_matches('"').to_string();
                }
                if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                    pretty = v.trim_matches('"').to_string();
                }
            }
            (name, version, pretty)
        })
        .unwrap_or_default();

    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    let arch = std::process::Command::new("uname")
        .arg("-m")
        .output()
        .map_or_else(
            |_| "x86_64".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );

    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_string();

    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("model name")
                .map(|r| r.trim_start_matches(['\t', ':', ' ']).to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    let mem_total_kb: u64 = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("MemTotal:")
                .and_then(|r| r.split_whitespace().next().and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0);

    let uptime_secs = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0) as u64;

    HttpResponse::Ok().json(serde_json::json!({
        "hostname":      hostname,
        "os_name":       os_name,
        "os_version":    os_version,
        "os_pretty":     os_pretty,
        "kernel":        kernel,
        "arch":          arch,
        "cpu_model":     cpu_model,
        "cpu_cores":     cpu_cores,
        "mem_total_mb":  mem_total_kb / 1024,
        "uptime_secs":   uptime_secs,
        "daemon_version": env!("CARGO_PKG_VERSION"),
    }))
}

// ─── Comprehensive system snapshot ───────────────────────────────────────────

/// GET /api/sys/snapshot — returns all system statistics in a single call.
/// Useful for dashboards that poll a single endpoint instead of many.
pub(crate) async fn api_sys_snapshot(state: web::Data<DaemonState>) -> impl Responder {
    use sysinfo::{Components, Disks, Networks, System};

    let mut sys = System::new_all();
    // Two-pass CPU sampling for accuracy
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_all();

    // CPU
    let cpu_pct = sys.global_cpu_usage();
    let cpu_cores: Vec<f32> = sys.cpus().iter().map(sysinfo::Cpu::cpu_usage).collect();
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_default();

    // Memory
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let swap_total = sys.total_swap();
    let swap_used = sys.used_swap();

    // Load average
    let load = System::load_average();

    // Uptime
    let uptime = System::uptime();

    // Hostname
    let hostname = System::host_name().unwrap_or_default();

    // Kernel / OS
    let os_name = System::name().unwrap_or_default();
    let kernel_ver = System::kernel_version().unwrap_or_default();

    // Processes (top 5 by CPU)
    let mut procs: Vec<serde_json::Value> = sys
        .processes()
        .values()
        .map(|p| {
            serde_json::json!({
                "pid":     p.pid().as_u32(),
                "name":    p.name().to_string_lossy(),
                "cpu_pct": p.cpu_usage(),
                "mem_bytes": p.memory(),
                "status":  format!("{:?}", p.status()),
            })
        })
        .collect();
    procs.sort_by(|a, b| {
        b["cpu_pct"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["cpu_pct"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    procs.truncate(5);

    // Disks
    let disks = Disks::new_with_refreshed_list();
    let disk_list: Vec<serde_json::Value> = disks
        .iter()
        .map(|d| {
            serde_json::json!({
                "mount":       d.mount_point().to_string_lossy(),
                "name":        d.name().to_string_lossy(),
                "total_bytes": d.total_space(),
                "used_bytes":  d.total_space() - d.available_space(),
                "free_bytes":  d.available_space(),
                "fs":          String::from_utf8_lossy(d.file_system().as_encoded_bytes()),
            })
        })
        .collect();

    // Network (1-sample deltas are zero without two passes; return totals)
    let nets = Networks::new_with_refreshed_list();
    let net_list: Vec<serde_json::Value> = nets
        .iter()
        .map(|(iface, data)| {
            serde_json::json!({
                "interface":   iface,
                "total_rx":    data.total_received(),
                "total_tx":    data.total_transmitted(),
                "mac":         data.mac_address().to_string(),
            })
        })
        .collect();

    // Temperatures
    let components = Components::new_with_refreshed_list();
    let temp_list: Vec<serde_json::Value> = components
        .iter()
        .filter_map(|c| {
            c.temperature().map(|t| {
                serde_json::json!({
                    "label": c.label(),
                    "temp_c": t,
                    "critical_c": c.critical(),
                })
            })
        })
        .collect();

    // Daemon state
    let mode = state.mode.lock().unwrap().as_str().to_string();
    let frozen = *state.frozen.lock().unwrap();
    let brightness = *state.dim_factor.lock().unwrap();
    let fps = *state.render_fps.lock().unwrap();
    let frame_seq = *state.frame_seq.lock().unwrap();
    let uptime_secs = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        now.saturating_sub(state.daemon_start_secs)
    };

    HttpResponse::Ok().json(serde_json::json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        "hostname":    hostname,
        "os":          os_name,
        "kernel":      kernel_ver,
        "cpu": {
            "name":    cpu_name,
            "pct":     cpu_pct,
            "cores":   cpu_cores,
            "load_1":  load.one,
            "load_5":  load.five,
            "load_15": load.fifteen,
        },
        "memory": {
            "total":      mem_total,
            "used":       mem_used,
            "free":       mem_total.saturating_sub(mem_used),
            "swap_total": swap_total,
            "swap_used":  swap_used,
        },
        "uptime_secs": uptime,
        "processes":   procs,
        "disks":       disk_list,
        "network":     net_list,
        "temperatures": temp_list,
        "daemon": {
            "mode":          mode,
            "frozen":        frozen,
            "brightness":    brightness,
            "fps":           fps,
            "frame_seq":     frame_seq,
            "uptime_secs":   uptime_secs,
            "version":       env!("CARGO_PKG_VERSION"),
        },
    }))
}

// ─── Battery info API ─────────────────────────────────────────────────────────

/// GET /api/battery — read live battery status from the system.
///
/// On desktops or systems without a battery, returns `{"present": false}`.
/// On laptops, returns capacity, status, voltage and estimated time remaining.
pub(crate) async fn api_battery_json() -> impl Responder {
    use ttlcd::display::read_battery_json;
    HttpResponse::Ok().json(read_battery_json())
}

// ─── HTTP Status Monitor API handlers ────────────────────────────────────────

/// GET /api/http_status — return monitors list and latest check results.
pub(crate) async fn api_get_http_status(state: web::Data<DaemonState>) -> impl Responder {
    let monitors: Vec<serde_json::Value> = state
        .http_monitors
        .lock()
        .unwrap()
        .iter()
        .map(|m| serde_json::json!({ "label": m.label, "url": m.url }))
        .collect();
    let results = state.http_results.lock().unwrap().clone();
    let interval = *state.http_check_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "monitors": monitors,
        "results":  results,
        "check_interval_secs": interval,
    }))
}

/// POST /api/http_status — add a monitor: {"label":"...", "url":"..."}
pub(crate) async fn api_add_http_monitor(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let url = body
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if label.is_empty() || url.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "label and url required" }));
    }
    let entry = HttpMonitorEntry {
        label: label.clone(),
        url: url.clone(),
    };
    let mut monitors = state.http_monitors.lock().unwrap();
    if monitors.len() >= 10 {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "maximum 10 monitors" }));
    }
    monitors.push(entry);
    save_http_monitors(&monitors.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "label": label, "url": url }))
}

/// DELETE /api/http_status/{index} — remove monitor at given index.
pub(crate) async fn api_delete_http_monitor(
    state: web::Data<DaemonState>,
    path: web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut monitors = state.http_monitors.lock().unwrap();
    if idx >= monitors.len() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "index out of range" }));
    }
    monitors.remove(idx);
    // Also drop the corresponding result if present
    let mut results = state.http_results.lock().unwrap();
    if idx < results.len() {
        results.remove(idx);
    }
    save_http_monitors(&monitors.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// PUT /api/http_status/interval — set check interval: {"secs": 30}
pub(crate) async fn api_set_http_interval(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let secs = body
        .get("secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(30)
        .max(5);
    *state.http_check_interval_secs.lock().unwrap() = secs;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "secs": secs }))
}

// ─── Shell Command API handlers ───────────────────────────────────────────────

/// GET /api/shell_cmd — return current command, output, and status.
pub(crate) async fn api_get_shell_cmd(state: web::Data<DaemonState>) -> impl Responder {
    let cmd = state.shell_cmd_text.lock().unwrap().clone();
    let output = state.shell_cmd_output.lock().unwrap().clone();
    let exit_ok = *state.shell_cmd_exit_ok.lock().unwrap();
    let run_at = *state.shell_cmd_run_at.lock().unwrap();
    let interval = *state.shell_cmd_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "cmd":          cmd,
        "output":       output,
        "exit_ok":      exit_ok,
        "run_at_unix":  run_at,
        "interval_secs": interval,
    }))
}

/// POST /api/shell_cmd — set command: {"cmd":"...", "interval_secs":10, "switch_mode":true}
pub(crate) async fn api_set_shell_cmd(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let cmd = body
        .get("cmd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if cmd.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "cmd required" }));
    }
    let interval = body
        .get("interval_secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(10);
    let switch = body
        .get("switch_mode")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    *state.shell_cmd_text.lock().unwrap() = cmd.clone();
    *state.shell_cmd_interval_secs.lock().unwrap() = interval;
    // Clear stale output
    *state.shell_cmd_output.lock().unwrap() = "(running…)".to_string();
    *state.shell_cmd_exit_ok.lock().unwrap() = true;

    if switch {
        let prev = state.mode.lock().unwrap().clone();
        if !matches!(prev, DisplayMode::ShellCmd) {
            *state.prev_mode.lock().unwrap() = prev;
        }
        *state.mode.lock().unwrap() = DisplayMode::ShellCmd;
        persist_state(&state);
    }

    // Trigger an immediate run in the background
    let state2 = state.as_ref().clone();
    let cmd2 = cmd.clone();
    tokio::spawn(async move { run_shell_cmd_once(&state2, &cmd2).await });

    save_new_settings(&state);
    HttpResponse::Ok()
        .json(serde_json::json!({ "ok": true, "cmd": cmd, "interval_secs": interval }))
}

/// POST /api/shell_cmd/run — force an immediate re-run of the current command.
pub(crate) async fn api_run_shell_cmd_now(state: web::Data<DaemonState>) -> impl Responder {
    let cmd = state.shell_cmd_text.lock().unwrap().clone();
    if cmd.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "no command configured" }));
    }
    *state.shell_cmd_output.lock().unwrap() = "(running…)".to_string();
    let state2 = state.as_ref().clone();
    tokio::spawn(async move { run_shell_cmd_once(&state2, &cmd).await });
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// DELETE /api/shell_cmd — clear command and return to previous mode.
pub(crate) async fn api_clear_shell_cmd(state: web::Data<DaemonState>) -> impl Responder {
    *state.shell_cmd_text.lock().unwrap() = String::new();
    *state.shell_cmd_output.lock().unwrap() = String::new();
    *state.shell_cmd_run_at.lock().unwrap() = 0;
    let prev = state.prev_mode.lock().unwrap().clone();
    *state.mode.lock().unwrap() = prev;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

// ─── Ping Monitor API handlers ────────────────────────────────────────────────

/// GET /api/ping_monitor — return hosts list and latest ping results.
pub(crate) async fn api_get_ping(state: web::Data<DaemonState>) -> impl Responder {
    let hosts: Vec<serde_json::Value> = state
        .ping_hosts
        .lock()
        .unwrap()
        .iter()
        .map(|h| serde_json::json!({ "label": h.label, "host": h.host }))
        .collect();
    let results = state.ping_results.lock().unwrap().clone();
    let interval = *state.ping_interval_secs.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "hosts":    hosts,
        "results":  results,
        "check_interval_secs": interval,
    }))
}

/// POST /api/ping_monitor — add a host: {"label":"...", "host":"8.8.8.8"}
pub(crate) async fn api_add_ping_host(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let host = body
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if label.is_empty() || host.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "label and host required" }));
    }
    if !is_valid_ping_host(&host) {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "host must be a host name or IP address" }));
    }
    let entry = PingHostEntry {
        label: label.clone(),
        host: host.clone(),
    };
    let mut hosts = state.ping_hosts.lock().unwrap();
    if hosts.len() >= 12 {
        return HttpResponse::BadRequest().json(serde_json::json!({ "error": "maximum 12 hosts" }));
    }
    hosts.push(entry);
    save_ping_hosts(&hosts.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "label": label, "host": host }))
}

/// DELETE /api/ping_monitor/{index} — remove host at given index.
pub(crate) async fn api_delete_ping_host(
    state: web::Data<DaemonState>,
    path: web::Path<usize>,
) -> impl Responder {
    let idx = path.into_inner();
    let mut hosts = state.ping_hosts.lock().unwrap();
    if idx >= hosts.len() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "index out of range" }));
    }
    hosts.remove(idx);
    let mut results = state.ping_results.lock().unwrap();
    if idx < results.len() {
        results.remove(idx);
    }
    save_ping_hosts(&hosts.clone());
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

/// PUT /api/ping_monitor/interval — set check interval: {"secs": 10}
pub(crate) async fn api_set_ping_interval(
    state: web::Data<DaemonState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let secs = body
        .get("secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(10)
        .max(5);
    *state.ping_interval_secs.lock().unwrap() = secs;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "secs": secs }))
}
// ─── Log tail API ─────────────────────────────────────────────────────────────

/// GET /api/log_tail — return current config and cached lines.
pub(crate) async fn api_get_log_tail(state: web::Data<DaemonState>) -> impl Responder {
    let path = state.log_tail_path.lock().unwrap().clone();
    let n = *state.log_tail_n.lock().unwrap();
    let lines = state.log_tail_lines.lock().unwrap().clone();
    let ok = *state.log_tail_ok.lock().unwrap();
    let secs = *state.log_tail_interval.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "path":     path,
        "n":        n,
        "ok":       ok,
        "interval_secs": secs,
        "lines":    lines,
    }))
}

#[derive(Deserialize)]
pub(crate) struct LogTailRequest {
    path: String,
    #[serde(default = "default_log_tail_n")]
    n: usize,
    #[serde(default = "default_log_tail_interval")]
    interval_secs: u64,
    #[serde(default)]
    switch_mode: bool,
}
pub(crate) fn default_log_tail_n() -> usize {
    16
}
pub(crate) fn default_log_tail_interval() -> u64 {
    3
}

/// POST /api/log_tail — set file path (and optionally number of lines / interval).
/// Body: `{"path":"/var/log/syslog","n":16,"interval_secs":3,"switch_mode":true}`
pub(crate) async fn api_set_log_tail(
    state: web::Data<DaemonState>,
    body: web::Json<LogTailRequest>,
) -> impl Responder {
    let req = body.into_inner();
    if req.path.is_empty() {
        return HttpResponse::BadRequest().body("path must not be empty");
    }
    let n = req.n.clamp(1, 50);
    let secs = req.interval_secs.max(1);

    *state.log_tail_path.lock().unwrap() = req.path.clone();
    *state.log_tail_n.lock().unwrap() = n;
    *state.log_tail_interval.lock().unwrap() = secs;

    // Attempt an immediate read so the display shows content right away
    match read_tail_lines(&req.path, n) {
        Ok(lines) => {
            *state.log_tail_lines.lock().unwrap() = lines;
            *state.log_tail_ok.lock().unwrap() = true;
        }
        Err(e) => {
            log::warn!("log_tail initial read '{}': {e}", req.path);
            *state.log_tail_ok.lock().unwrap() = false;
        }
    }

    if req.switch_mode {
        let cur = state.mode.lock().unwrap().clone();
        if !cur.is_transient() {
            *state.prev_mode.lock().unwrap() = cur;
        }
        *state.mode.lock().unwrap() = DisplayMode::LogTail;
    }

    save_new_settings(&state);
    log::info!(
        "log_tail configured: path='{}' n={} interval={}s",
        req.path,
        n,
        secs
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "path": req.path,
        "n": n,
        "interval_secs": secs,
    }))
}

/// DELETE /api/log_tail — clear log tail configuration and stop monitoring.
pub(crate) async fn api_clear_log_tail(state: web::Data<DaemonState>) -> impl Responder {
    *state.log_tail_path.lock().unwrap() = String::new();
    *state.log_tail_lines.lock().unwrap() = Vec::new();
    *state.log_tail_ok.lock().unwrap() = false;
    if matches!(*state.mode.lock().unwrap(), DisplayMode::LogTail) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("log tail cleared")
}

// ─── RSS feed API handlers ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub(crate) struct RssFeedStatus {
    url: String,
    title: String,
    ok: bool,
    items: Vec<(String, String)>,
    interval: u64,
}

pub(crate) async fn api_get_rss(state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(RssFeedStatus {
        url: state.rss_feed_url.lock().unwrap().clone(),
        title: state.rss_feed_title.lock().unwrap().clone(),
        ok: *state.rss_feed_ok.lock().unwrap(),
        items: state.rss_feed_items.lock().unwrap().clone(),
        interval: *state.rss_feed_interval.lock().unwrap(),
    })
}

#[derive(serde::Deserialize)]
pub(crate) struct RssFeedRequest {
    url: String,
    #[serde(default)]
    interval_secs: u64,
    #[serde(default)]
    switch_mode: bool,
}

pub(crate) async fn api_set_rss(
    state: web::Data<DaemonState>,
    body: web::Json<RssFeedRequest>,
) -> impl Responder {
    if body.url.is_empty() {
        return HttpResponse::BadRequest().body("url is required");
    }
    *state.rss_feed_url.lock().unwrap() = body.url.clone();
    *state.rss_feed_interval.lock().unwrap() = body.interval_secs.clamp(30, 86400);
    *state.rss_feed_ok.lock().unwrap() = false;
    if body.switch_mode {
        *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = DisplayMode::RssFeed;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("rss feed configured")
}

pub(crate) async fn api_clear_rss(state: web::Data<DaemonState>) -> impl Responder {
    *state.rss_feed_url.lock().unwrap() = String::new();
    *state.rss_feed_title.lock().unwrap() = String::new();
    *state.rss_feed_items.lock().unwrap() = Vec::new();
    *state.rss_feed_ok.lock().unwrap() = false;
    if matches!(*state.mode.lock().unwrap(), DisplayMode::RssFeed) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("rss feed cleared")
}

pub(crate) async fn api_fetch_rss(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.rss_feed_url.lock().unwrap().clone();
    if url.is_empty() {
        return HttpResponse::BadRequest().body(r#"no url configured; use POST /api/rss first"#);
    }
    match fetch_rss_feed(&state, &url).await {
        Ok(()) => HttpResponse::Ok().body("rss feed fetched"),
        Err(e) => HttpResponse::InternalServerError().body(format!("fetch error: {e:#}")),
    }
}

// ─── Service status API handlers ──────────────────────────────────────────────

#[derive(serde::Serialize)]
pub(crate) struct ServiceStatusResponse {
    units: Vec<String>,
    statuses: Vec<ServiceStatusItem>,
    interval: u64,
}

#[derive(serde::Serialize)]
pub(crate) struct ServiceStatusItem {
    name: String,
    state: String,
    sub: String,
}

pub(crate) async fn api_get_services(state: web::Data<DaemonState>) -> impl Responder {
    let statuses: Vec<ServiceStatusItem> = state
        .service_statuses
        .lock()
        .unwrap()
        .iter()
        .map(|s| ServiceStatusItem {
            name: s.name.clone(),
            state: s.state.clone(),
            sub: s.sub.clone(),
        })
        .collect();
    HttpResponse::Ok().json(ServiceStatusResponse {
        units: state.service_units.lock().unwrap().clone(),
        statuses,
        interval: *state.service_poll_interval.lock().unwrap(),
    })
}

#[derive(serde::Deserialize)]
pub(crate) struct ServicesRequest {
    units: Vec<String>,
    #[serde(default)]
    interval_secs: u64,
    #[serde(default)]
    switch_mode: bool,
}

pub(crate) async fn api_set_services(
    state: web::Data<DaemonState>,
    body: web::Json<ServicesRequest>,
) -> impl Responder {
    if body.units.is_empty() {
        return HttpResponse::BadRequest().body("units array is required");
    }
    if let Some(bad) = body.units.iter().find(|u| !is_valid_unit_name(u)) {
        return HttpResponse::BadRequest().body(format!("invalid unit name '{bad}'"));
    }
    *state.service_units.lock().unwrap() = body.units.clone();
    *state.service_poll_interval.lock().unwrap() = body.interval_secs.clamp(5, 3600);

    // Do an immediate poll
    let units = body.units.clone();
    let statuses = tokio::task::spawn_blocking(move || query_service_statuses(&units))
        .await
        .unwrap_or_default();
    *state.service_statuses.lock().unwrap() = statuses;

    if body.switch_mode {
        *state.prev_mode.lock().unwrap() = state.mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = DisplayMode::ServiceStatus;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("services configured")
}

pub(crate) async fn api_clear_services(state: web::Data<DaemonState>) -> impl Responder {
    *state.service_units.lock().unwrap() = Vec::new();
    *state.service_statuses.lock().unwrap() = Vec::new();
    if matches!(*state.mode.lock().unwrap(), DisplayMode::ServiceStatus) {
        let prev = state.prev_mode.lock().unwrap().clone();
        *state.mode.lock().unwrap() = prev;
    }
    save_new_settings(&state);
    HttpResponse::Ok().body("services cleared")
}

// ─── Docker containers API ────────────────────────────────────────────────────

/// GET /api/docker — return list of Docker containers (all, running first).
pub(crate) async fn api_get_docker() -> impl Responder {
    let containers = tokio::task::spawn_blocking(read_docker_containers)
        .await
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = containers
        .iter()
        .map(|c| {
            serde_json::json!({
                "id":      c.id,
                "name":    c.name,
                "image":   c.image,
                "status":  c.status,
                "ports":   c.ports,
                "running": c.running,
            })
        })
        .collect();
    HttpResponse::Ok().json(serde_json::json!({
        "containers": list,
        "running": list.iter().filter(|c| c["running"].as_bool().unwrap_or(false)).count(),
        "total":   list.len(),
    }))
}

// ─── Logged-in user sessions API ─────────────────────────────────────────────

/// GET /api/sessions — return active user sessions from `who -u`.
pub(crate) async fn api_get_sessions() -> impl Responder {
    let sessions = tokio::task::spawn_blocking(read_user_sessions)
        .await
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "username": s.username,
                "tty":      s.tty,
                "login_at": s.login_at,
                "idle":     s.idle,
                "pid":      s.pid,
                "from":     s.from,
            })
        })
        .collect();
    HttpResponse::Ok().json(serde_json::json!({
        "sessions": list,
        "count": list.len(),
    }))
}

// ─── API key authentication management ───────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SetAuthKeyPayload {
    key: String,
}

/// POST /api/auth/key — set an API key. When set, all subsequent API requests
/// must include `Authorization: Bearer <key>` header.
///
/// Body: `{ "key": "your-secret-key" }`
pub(crate) async fn api_set_auth_key(
    state: web::Data<DaemonState>,
    payload: web::Json<SetAuthKeyPayload>,
) -> impl Responder {
    let key = payload.key.trim().to_string();
    if key.is_empty() {
        return HttpResponse::BadRequest().body("key must not be empty");
    }
    *state.api_key.lock().unwrap() = Some(key.clone());
    push_history(
        &state,
        HistoryKind::System,
        format!("API key configured (length {})", key.len()),
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "API key set. Include 'Authorization: Bearer <key>' in all requests.",
        "key_length": key.len(),
    }))
}

/// DELETE /api/auth/key — remove the API key (disable authentication).
pub(crate) async fn api_clear_auth_key(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.api_key.lock().unwrap().take().is_some();
    if was_set {
        push_history(
            &state,
            HistoryKind::System,
            "API key cleared — authentication disabled".to_string(),
        );
        HttpResponse::Ok().body("API key cleared")
    } else {
        HttpResponse::Ok().body("no API key was set")
    }
}

/// GET /api/auth/status — returns whether an API key is currently configured.
pub(crate) async fn api_get_auth_status(state: web::Data<DaemonState>) -> impl Responder {
    let has_key = state.api_key.lock().unwrap().is_some();
    HttpResponse::Ok().json(serde_json::json!({
        "auth_required": has_key,
        "method": if has_key { "Bearer token" } else { "none" },
    }))
}

// ─── Detailed network interfaces API ─────────────────────────────────────────

/// GET /api/sys/net — returns detailed info about all network interfaces:
/// name, IPv4/IPv6 addresses, Rx/Tx bytes, packet counts, errors.
///
/// Data sources: /proc/net/dev (counters), ip addr (addresses).
pub(crate) async fn api_sys_net() -> impl Responder {
    let counters = parse_proc_net_dev();
    let addresses = parse_if_addresses();

    let ifaces: Vec<serde_json::Value> = counters
        .into_iter()
        .map(|(name, stats)| {
            let addrs = addresses.get(&name).cloned().unwrap_or_default();
            serde_json::json!({
                "name":       name,
                "addresses":  addrs,
                "rx_bytes":   stats.rx_bytes,
                "tx_bytes":   stats.tx_bytes,
                "rx_packets": stats.rx_packets,
                "tx_packets": stats.tx_packets,
                "rx_errors":  stats.rx_errors,
                "tx_errors":  stats.tx_errors,
                "rx_dropped": stats.rx_dropped,
                "tx_dropped": stats.tx_dropped,
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "interfaces": ifaces,
        "count": ifaces.len(),
    }))
}

pub(crate) struct IfStats {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_packets: u64,
    tx_packets: u64,
    rx_errors: u64,
    tx_errors: u64,
    rx_dropped: u64,
    tx_dropped: u64,
}

pub(crate) fn parse_proc_net_dev() -> Vec<(String, IfStats)> {
    let Ok(text) = std::fs::read_to_string("/proc/net/dev") else {
        return vec![];
    };
    // Skip first two header lines
    text.lines()
        .skip(2)
        .filter_map(|line| {
            let (name_raw, rest) = line.split_once(':')?;
            let name = name_raw.trim().to_string();
            let nums: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if nums.len() < 16 {
                return None;
            }
            Some((
                name,
                IfStats {
                    rx_bytes: nums[0],
                    rx_packets: nums[1],
                    rx_errors: nums[2],
                    rx_dropped: nums[3],
                    tx_bytes: nums[8],
                    tx_packets: nums[9],
                    tx_errors: nums[10],
                    tx_dropped: nums[11],
                },
            ))
        })
        .collect()
}

pub(crate) fn parse_if_addresses() -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    // IPv4 addresses
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-4", "addr"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Format: <idx>: <name> inet <addr/prefix> ...
            if parts.len() >= 4 && parts[2] == "inet" {
                let name = parts[1].trim_end_matches(':').to_string();
                let addr = parts[3].to_string();
                map.entry(name).or_default().push(addr);
            }
        }
    }
    // IPv6 addresses
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-6", "addr"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "inet6" {
                let name = parts[1].trim_end_matches(':').to_string();
                let addr = format!("ipv6:{}", parts[3]);
                map.entry(name).or_default().push(addr);
            }
        }
    }
    map
}

// ─── ntfy.sh push notification config ────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct SetNtfyPayload {
    url: String,
}

/// GET /api/ntfy/config — check current ntfy configuration.
pub(crate) async fn api_get_ntfy_config(state: web::Data<DaemonState>) -> impl Responder {
    let url = state.ntfy_url.lock().unwrap().clone();
    HttpResponse::Ok().json(serde_json::json!({
        "configured": url.is_some(),
        "url": url.unwrap_or_default(),
    }))
}

/// POST /api/ntfy/config — set the ntfy.sh topic URL.
///
/// Body: `{ "url": "https://ntfy.sh/my-topic" }`
///
/// When set, every Alert-kind event (threshold breach, service failure, etc.)
/// is POSTed to this URL as a plain-text ntfy notification.
pub(crate) async fn api_set_ntfy_config(
    state: web::Data<DaemonState>,
    payload: web::Json<SetNtfyPayload>,
) -> impl Responder {
    let url = payload.url.trim().to_string();
    if url.is_empty() {
        return HttpResponse::BadRequest().body("url must not be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return HttpResponse::BadRequest().body("url must start with http:// or https://");
    }
    *state.ntfy_url.lock().unwrap() = Some(url.clone());
    push_history(
        &state,
        HistoryKind::System,
        format!("ntfy notifications configured: {url}"),
    );
    save_new_settings(&state);
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "url": url,
        "message": "Alert notifications will be sent to this URL",
    }))
}

/// DELETE /api/ntfy/config — disable ntfy notifications.
pub(crate) async fn api_clear_ntfy_config(state: web::Data<DaemonState>) -> impl Responder {
    let was_set = state.ntfy_url.lock().unwrap().take().is_some();
    if was_set {
        push_history(
            &state,
            HistoryKind::System,
            "ntfy notifications disabled".to_string(),
        );
        save_new_settings(&state);
        HttpResponse::Ok().body("ntfy config cleared")
    } else {
        HttpResponse::Ok().body("ntfy was not configured")
    }
}

/// POST /api/ntfy/test — send a test notification to the configured ntfy URL.
pub(crate) async fn api_ntfy_test(state: web::Data<DaemonState>) -> impl Responder {
    let ntfy_opt = state.ntfy_url.lock().unwrap().clone();
    let Some(ntfy_url) = ntfy_opt else {
        return HttpResponse::BadRequest()
            .body("ntfy not configured — POST /api/ntfy/config first");
    };
    let result = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client
            .post(&ntfy_url)
            .header("Title", "ttlcd test notification")
            .header("Priority", "default")
            .header("Tags", "white_check_mark")
            .body("This is a test notification from ttlcd.")
            .send()
            .await?;
        Ok::<_, reqwest::Error>(resp.status())
    }
    .await;

    match result {
        Ok(status) => {
            push_history(
                &state,
                HistoryKind::System,
                format!("ntfy test notification sent ({status})"),
            );
            HttpResponse::Ok().json(serde_json::json!({
                "ok": status.is_success(),
                "http_status": status.as_u16(),
                "message": format!("Test notification sent to {} — HTTP {}", ntfy_url, status),
            }))
        }
        Err(e) => HttpResponse::BadGateway().json(serde_json::json!({
            "ok": false,
            "error": format!("{}", e),
        })),
    }
}
