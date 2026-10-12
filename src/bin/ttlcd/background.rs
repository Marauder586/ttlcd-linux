//! Background workers that poll outside sources: HTTP checks, shell command, ping, RSS and services.

use super::*;

// ─── HTTP health-check loop ───────────────────────────────────────────────────

/// Async background loop: periodically checks all configured HTTP monitors.
pub(crate) async fn http_check_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.http_check_interval_secs.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let monitors: Vec<HttpMonitorEntry> = state.http_monitors.lock().unwrap().clone();
        if monitors.is_empty() {
            continue;
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("ttlcd-rs/0.1 health-checker")
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let mut results: Vec<HttpMonitorResult> = Vec::with_capacity(monitors.len());
        for m in &monitors {
            let start = std::time::Instant::now();
            let resp = client.get(&m.url).send().await;
            let latency_ms = start.elapsed().as_millis() as u64;
            let (status_code, ok) = match resp {
                Ok(r) => {
                    let code = r.status().as_u16();
                    (Some(code), code < 400)
                }
                Err(_) => (None, false),
            };
            results.push(HttpMonitorResult {
                label: m.label.clone(),
                url: m.url.clone(),
                status_code,
                latency_ms: if ok || status_code.is_some() {
                    Some(latency_ms)
                } else {
                    None
                },
                ok,
            });
        }
        *state.http_results.lock().unwrap() = results;
    }
}

// ─── Shell command loop ───────────────────────────────────────────────────────

/// Async background loop: periodically runs the configured shell command.
pub(crate) async fn shell_cmd_loop(state: DaemonState) {
    let mut last_run_at: u64 = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let cmd_text = state.shell_cmd_text.lock().unwrap().clone();
        if cmd_text.is_empty() {
            continue;
        }

        let interval_secs = *state.shell_cmd_interval_secs.lock().unwrap();
        if interval_secs == 0 {
            continue;
        } // manual-only mode

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        if now_secs < last_run_at + interval_secs {
            continue;
        }
        last_run_at = now_secs;

        run_shell_cmd_once(&state, &cmd_text).await;
    }
}

/// Execute `cmd_text` via `/bin/sh -c` and store output + exit status.
pub(crate) async fn run_shell_cmd_once(state: &DaemonState, cmd_text: &str) {
    use tokio::process::Command;
    let result = Command::new("/bin/sh")
        .args(["-c", cmd_text])
        .output()
        .await;

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                format!(
                    "{}
--- stderr ---
{}",
                    stdout.trim_end(),
                    stderr.trim_end()
                )
            };
            *state.shell_cmd_output.lock().unwrap() = combined;
            *state.shell_cmd_exit_ok.lock().unwrap() = out.status.success();
            *state.shell_cmd_run_at.lock().unwrap() = now_secs;
        }
        Err(e) => {
            *state.shell_cmd_output.lock().unwrap() = format!("exec error: {e}");
            *state.shell_cmd_exit_ok.lock().unwrap() = false;
            *state.shell_cmd_run_at.lock().unwrap() = now_secs;
        }
    }
}

// ─── Ping monitor loop ───────────────────────────────────────────────────────

/// Async background loop: periodically pings all configured hosts using the
/// system `ping` command, and stores results in `state.ping_results`.
pub(crate) async fn ping_monitor_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.ping_interval_secs.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let hosts: Vec<PingHostEntry> = state.ping_hosts.lock().unwrap().clone();
        if hosts.is_empty() {
            continue;
        }

        let mut results: Vec<PingResult> = Vec::with_capacity(hosts.len());
        for h in &hosts {
            let result = run_ping_once(&h.host).await;
            results.push(PingResult {
                host: h.host.clone(),
                label: h.label.clone(),
                latency_ms: result.0,
                loss_pct: result.1,
            });
        }
        push_ping_history(&results);
        *state.ping_results.lock().unwrap() = results;
    }
}

/// Execute `ping -c 3 -W 1 <host>` and parse the summary.
/// Returns `(latency_ms, loss_pct)`.
pub(crate) async fn run_ping_once(host: &str) -> (Option<f64>, f32) {
    use tokio::process::Command;
    if !is_valid_ping_host(host) {
        return (None, 100.0);
    }
    // -c 3 = send 3 packets; -W 1 = 1s timeout per packet
    let out = Command::new("ping")
        .args(["-c", "3", "-W", "1", "--", host])
        .output()
        .await;

    match out {
        Err(_) => (None, 100.0),
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            // Parse "rtt min/avg/max/mdev = X/Y/Z/W ms"
            let avg_ms = stdout
                .lines()
                .find(|l| l.contains("rtt min/avg"))
                .and_then(|l| l.split('/').nth(4))
                .and_then(|s| s.trim().split(' ').next())
                .and_then(|s| s.parse::<f64>().ok());
            // Parse "N packets transmitted, M received, X% packet loss"
            let loss_pct = stdout
                .lines()
                .find(|l| l.contains("packet loss"))
                .and_then(|l| {
                    l.split(',')
                        .find(|s| s.contains("packet loss"))
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.trim_end_matches('%').parse::<f32>().ok())
                })
                .unwrap_or(100.0);
            (avg_ms, loss_pct)
        }
    }
}

/// Background loop: periodically fetch the configured HTTP JSON metrics source and update metrics_data.
pub(crate) async fn metrics_source_loop(state: DaemonState) {
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    loop {
        let source = state.metrics_source.lock().unwrap().clone();
        let Some(src) = source else {
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        };
        let interval = src.interval_secs.max(5);

        if let Err(e) = fetch_metrics_source(&state, &src).await {
            log::warn!("metrics_source fetch '{}': {e}", src.url);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

/// Fetch the metrics source URL, extract fields, and update metrics_data + metrics_title.
pub(crate) async fn fetch_metrics_source(
    state: &DaemonState,
    src: &MetricsSource,
) -> anyhow::Result<()> {
    use reqwest::Client;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .user_agent(concat!("ttlcd/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(&src.url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let body: serde_json::Value =
        serde_json::from_slice(&read_body_capped(resp, MAX_REMOTE_TEXT_BYTES).await?)?;

    let mut metrics: Vec<Metric> = Vec::with_capacity(src.fields.len());
    for field in &src.fields {
        let val = json_path_get(&body, &field.path);
        let val_str = match val {
            Some(v) => {
                let raw = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                // Apply simple format: replace "{}" with the value
                if field.format == "{}" || field.format.is_empty() {
                    raw
                } else {
                    field.format.replacen("{}", &raw, 1)
                }
            }
            None => "N/A".to_string(),
        };
        metrics.push(Metric {
            label: field.label.clone(),
            value: val_str,
            color: field.color.clone(),
        });
    }

    if !src.title.is_empty() {
        *state.metrics_title.lock().unwrap() = src.title.clone();
    }
    *state.metrics_data.lock().unwrap() = metrics;
    log::debug!("metrics_source updated from '{}'", src.url);
    Ok(())
}

/// Walk a JSON value using a dot-separated path string.
/// Array indices are supported: "data.0.value" navigates `data[0].value`.
pub(crate) fn json_path_get<'a>(
    root: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut cur = root;
    for segment in path.split('.') {
        cur = if let Ok(idx) = segment.parse::<usize>() {
            cur.get(idx)?
        } else {
            cur.get(segment)?
        };
    }
    Some(cur)
}

/// Background loop: periodically fetch weather from wttr.in.
/// Only runs when `weather_location` is non-empty and `weather_auto_secs > 0`.
pub(crate) async fn weather_auto_loop(state: DaemonState) {
    // Stagger start a bit so it doesn't hammer the network at startup
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    loop {
        let location = state.weather_location.lock().unwrap().clone();
        let interval = *state.weather_auto_secs.lock().unwrap();

        if location.is_empty() || interval == 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            continue;
        }

        if let Err(e) = fetch_wttr(&state, &location).await {
            log::warn!("weather auto-fetch for '{location}': {e}");
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval.max(60))).await;
    }
}

/// Fetch current conditions from wttr.in JSON API and update weather_data.
pub(crate) async fn fetch_wttr(state: &DaemonState, location: &str) -> anyhow::Result<()> {
    use reqwest::Client;
    let url = format!("https://wttr.in/{}?format=j1", urlencoding_encode(location));
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("ttlcd/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("wttr.in HTTP {}", resp.status());
    }
    let body: serde_json::Value =
        serde_json::from_slice(&read_body_capped(resp, MAX_REMOTE_TEXT_BYTES).await?)?;

    let cc = body["current_condition"]
        .get(0)
        .cloned()
        .unwrap_or_default();
    let w0 = body["weather"].get(0).cloned().unwrap_or_default();

    let temp_c: Option<f32> = cc["temp_C"].as_str().and_then(|s| s.parse().ok());
    let feels_c: Option<f32> = cc["FeelsLikeC"].as_str().and_then(|s| s.parse().ok());
    let humidity: Option<u8> = cc["humidity"].as_str().and_then(|s| s.parse().ok());
    let wind_kmh: Option<f32> = cc["windspeedKmph"].as_str().and_then(|s| s.parse().ok());
    let wind_dir: String = cc["winddir16Point"].as_str().unwrap_or("").to_string();
    let condition: String = cc["weatherDesc"]
        .get(0)
        .and_then(|d| d["value"].as_str())
        .unwrap_or("Unknown")
        .to_string();
    let visibility: Option<f32> = cc["visibility"].as_str().and_then(|s| s.parse().ok());
    let max_c: Option<f32> = w0["maxtempC"].as_str().and_then(|s| s.parse().ok());
    let min_c: Option<f32> = w0["mintempC"].as_str().and_then(|s| s.parse().ok());

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let wd = WeatherData {
        city: location.to_string(),
        temp_c,
        feels_like_c: feels_c,
        condition,
        humidity_pct: humidity,
        wind_kmh,
        wind_dir,
        high_c: max_c,
        low_c: min_c,
        visibility_km: visibility,
        updated_at: Some(now_secs),
    };
    *state.weather_data.lock().unwrap() = wd;
    log::info!("weather updated for '{location}'");
    Ok(())
}

/// Percent-encode a location string for use in a URL path component.
/// Only encodes characters that are not safe in a URL path.
pub(crate) fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '+' => out.push(c),
            ' ' => out.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    out.push('%');
                    out.push(
                        char::from_digit((byte >> 4) as u32, 16)
                            .unwrap_or('0')
                            .to_ascii_uppercase(),
                    );
                    out.push(
                        char::from_digit((byte & 0xf) as u32, 16)
                            .unwrap_or('0')
                            .to_ascii_uppercase(),
                    );
                }
            }
        }
    }
    out
}

/// Background loop: periodically re-reads the tail of the configured log file.
pub(crate) async fn log_tail_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.log_tail_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(1))).await;

        let path = state.log_tail_path.lock().unwrap().clone();
        if path.is_empty() {
            continue;
        }

        let n = *state.log_tail_n.lock().unwrap();
        match read_tail_lines(&path, n) {
            Ok(lines) => {
                *state.log_tail_lines.lock().unwrap() = lines;
                *state.log_tail_ok.lock().unwrap() = true;
            }
            Err(e) => {
                log::warn!("log_tail: cannot read '{path}': {e}");
                *state.log_tail_ok.lock().unwrap() = false;
            }
        }
    }
}

/// Most of a file's tail that [`read_tail_lines`] will ever read.
pub(crate) const MAX_TAIL_BYTES: u64 = 256 * 1024;

/// Read the last `n` non-empty lines of a regular file.
///
/// Only the final [`MAX_TAIL_BYTES`] are read, so a huge file (or one with no
/// newlines) cannot exhaust memory; invalid UTF-8 is replaced rather than
/// rejected.  Anything that is not a regular file — a FIFO or device would
/// block or never end — is refused.
pub(crate) fn read_tail_lines(path: &str, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};

    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut file = std::fs::File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(MAX_TAIL_BYTES).read_to_end(&mut buf)?;

    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    // Starting mid-file almost certainly lands mid-line: drop the fragment.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(n);
    Ok(lines[skip..]
        .iter()
        .map(std::string::ToString::to_string)
        .collect())
}

// ─── RSS feed background loop ─────────────────────────────────────────────────

/// Parse a simple RSS 2.0 or Atom feed from XML text.
/// Returns (feed_title, Vec<(item_title, item_date_or_author)>)
pub(crate) fn parse_rss_xml(xml: &str) -> (String, Vec<(String, String)>) {
    // Very lightweight XML-free parser using string scanning
    fn extract_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = text.find(&open)? + open.len();
        let end = text[start..].find(&close)? + start;
        Some(&text[start..end])
    }
    fn extract_cdata(s: &str) -> &str {
        if let Some(inner) = s.strip_prefix("<![CDATA[") {
            inner.strip_suffix("]]>").unwrap_or(inner)
        } else {
            s
        }
    }
    fn decode_entities(s: &str) -> String {
        // `&amp;` last, so `&amp;lt;` decodes to the text "&lt;", not "<".
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&#x27;", "'")
            .replace("&amp;", "&")
    }

    let is_atom = xml.contains("xmlns=\"http://www.w3.org/2005/Atom\"");
    let mut feed_title = String::new();
    let mut items: Vec<(String, String)> = Vec::new();

    if is_atom {
        // Atom: <feed><title>...</title> and <entry><title/><updated/>...
        if let Some(raw) = extract_tag(xml, "title") {
            feed_title = decode_entities(extract_cdata(raw));
        }
        // Parse <entry> blocks
        let mut rest = xml;
        while let Some(entry_start) = rest.find("<entry>") {
            let after = &rest[entry_start + 7..];
            let entry_end = after.find("</entry>").unwrap_or(after.len());
            let block = &after[..entry_end];

            let title = extract_tag(block, "title")
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            let meta = extract_tag(block, "updated")
                .or_else(|| extract_tag(block, "published"))
                .or_else(|| extract_tag(block, "author"))
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            // Shorten ISO date to date only
            let meta = meta.split('T').next().unwrap_or(&meta).to_string();
            if !title.is_empty() {
                items.push((title, meta));
            }
            rest = &after[entry_end..];
            if rest.len() < 8 {
                break;
            }
        }
    } else {
        // RSS 2.0: <channel><title>...</title> and <item>...</item>
        // Find channel title (first <title> that is not inside an item)
        if let Some(ch_start) = xml.find("<channel>") {
            let ch = &xml[ch_start..];
            if let Some(raw) = extract_tag(ch, "title") {
                feed_title = decode_entities(extract_cdata(raw));
            }
        }
        let mut rest = xml;
        while let Some(item_start) = rest.find("<item>") {
            let after = &rest[item_start + 6..];
            let item_end = after.find("</item>").unwrap_or(after.len());
            let block = &after[..item_end];

            let title = extract_tag(block, "title")
                .map(|s| decode_entities(extract_cdata(s)))
                .unwrap_or_default();
            let meta = extract_tag(block, "pubDate")
                .or_else(|| extract_tag(block, "dc:date"))
                .or_else(|| extract_tag(block, "author"))
                .map(|s| {
                    let s = decode_entities(extract_cdata(s));
                    // Shorten RFC-2822 date: "Wed, 14 May 2026 12:00:00 +0000" -> "14 May 2026 12:00"
                    // Strip leading weekday
                    let s2 = s
                        .split_once(',')
                        .map_or(s.as_str(), |(_, rest)| rest.trim())
                        .to_string();
                    // Take date + time but drop seconds and timezone: "14 May 2026 12:00"
                    let parts: Vec<&str> = s2.split_whitespace().collect();
                    if parts.len() >= 5 {
                        // parts: ["14","May","2026","12:00:00","+0000"]
                        let time_short = parts[3]
                            .splitn(3, ':')
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(":");
                        format!("{} {} {} {}", parts[0], parts[1], parts[2], time_short)
                    } else {
                        s2.chars().take(20).collect::<String>().trim().to_string()
                    }
                })
                .unwrap_or_default();
            if !title.is_empty() {
                items.push((title, meta));
            }
            rest = &after[item_end..];
            if rest.len() < 6 {
                break;
            }
        }
    }

    (feed_title, items)
}

/// Fetch the RSS/Atom feed URL and update the cache.
pub(crate) async fn fetch_rss_feed(state: &DaemonState, url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("ttlcd/", env!("CARGO_PKG_VERSION"), " rss-reader"))
        .build()?;
    let body = read_body_capped(client.get(url).send().await?, MAX_REMOTE_TEXT_BYTES).await?;
    let text = String::from_utf8_lossy(&body);
    let (feed_title, items) = parse_rss_xml(&text);
    *state.rss_feed_title.lock().unwrap() = if feed_title.is_empty() {
        "RSS FEED".to_string()
    } else {
        feed_title
    };
    *state.rss_feed_items.lock().unwrap() = items;
    *state.rss_feed_ok.lock().unwrap() = true;
    Ok(())
}

/// Background loop: re-fetches the RSS feed at the configured interval.
pub(crate) async fn rss_feed_loop(state: DaemonState) {
    loop {
        let interval_secs = *state.rss_feed_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(30))).await;

        let url = state.rss_feed_url.lock().unwrap().clone();
        if url.is_empty() {
            continue;
        }

        if let Err(e) = fetch_rss_feed(&state, &url).await {
            log::warn!("rss_feed: fetch error for '{url}': {e:#}");
            *state.rss_feed_ok.lock().unwrap() = false;
        }
    }
}

// ─── Service status background loop ──────────────────────────────────────────

/// Query systemd service states via `systemctl show`.
pub(crate) fn query_service_statuses(units: &[String]) -> Vec<ttlcd::display::ServiceStatus> {
    units
        .iter()
        .map(|name| {
            let out = std::process::Command::new("systemctl")
                .args([
                    "show",
                    "--property=ActiveState,SubState",
                    "--no-pager",
                    "--",
                    name,
                ])
                .output();
            let (state, sub) = if let Ok(o) = out {
                let text = String::from_utf8_lossy(&o.stdout);
                let mut st = String::from("unknown");
                let mut sub = String::from("unknown");
                for line in text.lines() {
                    if let Some(v) = line.strip_prefix("ActiveState=") {
                        st = v.to_string();
                    }
                    if let Some(v) = line.strip_prefix("SubState=") {
                        sub = v.to_string();
                    }
                }
                (st, sub)
            } else {
                ("unknown".to_string(), "unknown".to_string())
            };
            ttlcd::display::ServiceStatus {
                name: name.clone(),
                state,
                sub,
            }
        })
        .collect()
}

/// Background loop: periodically polls systemd unit states.
pub(crate) async fn service_status_loop(state: DaemonState) {
    // prev_states: service name → last known ActiveState string
    let mut prev_states: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    loop {
        let interval_secs = *state.service_poll_interval.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs.max(5))).await;

        let units: Vec<String> = state.service_units.lock().unwrap().clone();
        if units.is_empty() {
            prev_states.clear();
            continue;
        }

        let statuses = tokio::task::spawn_blocking(move || query_service_statuses(&units))
            .await
            .unwrap_or_default();

        // ── State-change alerting ──────────────────────────────────────────
        for svc in &statuses {
            let prev = prev_states
                .get(&svc.name)
                .map_or("unknown", std::string::String::as_str);
            let cur = svc.state.as_str();
            if prev != cur {
                // Always log transitions
                log::info!("service state change: {} {} → {}", svc.name, prev, cur);

                // Alert on failure or unexpected stops
                let is_bad = cur == "failed" || (cur == "inactive" && prev == "active");
                let is_recov = cur == "active" && (prev == "failed" || prev == "inactive");
                if is_bad || is_recov {
                    let emoji = if is_bad { "✗" } else { "✓" };
                    let verdict = if is_bad { "FAILED" } else { "RECOVERED" };
                    let summary = format!(
                        "{emoji} Service {}: {} ({} → {})",
                        verdict, svc.name, prev, cur
                    );

                    // Show on display briefly
                    let color = if is_bad {
                        ttlcd::display::NamedColor::Red
                    } else {
                        ttlcd::display::NamedColor::Green
                    };
                    let now_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    push_threshold_alert(
                        &state,
                        format!("{emoji} {}", svc.name.chars().take(20).collect::<String>()),
                        format!("{verdict} ({prev} → {cur})"),
                        color,
                        now_secs,
                    );

                    // Fire outbound webhook if configured
                    let fwd_url_opt = state.webhook_forward_url.lock().unwrap().clone();
                    if let Some(fwd_url) = fwd_url_opt {
                        let payload = serde_json::json!({
                            "event":   "service_state_change",
                            "service": svc.name,
                            "state":   cur,
                            "prev":    prev,
                            "verdict": verdict,
                            "summary": summary,
                        });
                        tokio::spawn(async move {
                            match reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(10))
                                .build()
                            {
                                Ok(client) => {
                                    match client.post(&fwd_url).json(&payload).send().await {
                                        Ok(r) => log::info!(
                                            "service alert webhook → {} status {}",
                                            fwd_url,
                                            r.status()
                                        ),
                                        Err(e) => log::warn!(
                                            "service alert webhook → {fwd_url} failed: {e}"
                                        ),
                                    }
                                }
                                Err(e) => {
                                    log::warn!("service alert: could not build client: {e}");
                                }
                            }
                        });
                    }
                }
            }
        }

        // Update prev_states map for next iteration
        for svc in &statuses {
            prev_states.insert(svc.name.clone(), svc.state.clone());
        }

        *state.service_statuses.lock().unwrap() = statuses;
    }
}

/// Background loop: every 30 s, check if a mode schedule entry matches the current time.
/// Uses "most recent past slot" logic — finds the latest entry whose (hour, minute) <= now.
pub(crate) fn mode_schedule_loop(state: DaemonState) {
    use chrono::Timelike;
    // Track which slot we last activated to avoid repeated switches
    let mut last_activated: Option<(u8, u8)> = None;

    loop {
        thread::sleep(Duration::from_secs(30));

        let entries = state.mode_schedule.lock().unwrap().clone();
        if entries.is_empty() {
            last_activated = None;
            continue;
        }

        let now = Local::now();
        let now_h = now.hour() as u8;
        let now_m = now.minute() as u8;

        // Find the "active" entry: the latest one whose (hour, minute) <= (now_h, now_m).
        // Wrap-around: if no past entry exists, use the last entry of the previous day.
        let active = entries
            .iter()
            .filter(|e| (e.hour, e.minute) <= (now_h, now_m))
            .max_by_key(|e| (e.hour, e.minute));
        let active = active.or_else(|| entries.iter().max_by_key(|e| (e.hour, e.minute)));

        if let Some(entry) = active {
            let slot = (entry.hour, entry.minute);
            if last_activated != Some(slot) {
                if let Ok(mode) = entry.mode.parse::<DisplayMode>() {
                    let cur = state.mode.lock().unwrap().clone();
                    if cur != mode {
                        log::info!(
                            "mode schedule: switching to {} (slot {:02}:{:02})",
                            entry.mode,
                            entry.hour,
                            entry.minute
                        );
                        if !cur.is_transient() {
                            *state.prev_mode.lock().unwrap() = cur;
                        }
                        *state.mode.lock().unwrap() = mode;
                        push_history(
                            &state,
                            HistoryKind::System,
                            format!(
                                "Scheduled mode switch → {} at {:02}:{:02}",
                                entry.mode, entry.hour, entry.minute
                            ),
                        );
                    }
                    last_activated = Some(slot);
                }
            }
        }
    }
}
