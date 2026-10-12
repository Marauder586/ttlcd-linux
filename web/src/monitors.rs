//! Monitor cards built at start-up: HTTP checks, shell command, ping, log
//! tail, RSS feed and systemd services. Each is placed after the previous
//! one, so `init` runs them in order.

use crate::api;
use crate::core::{self, or_toast, toast};
use crate::dom::{self, card, place_after, placeholder_item, El};
use crate::js::arr;
use crate::obj;
use crate::util::{esc, int_or, parse_int, prefix, safe_href, split_list};

pub fn init() {
    inject_http_status();
    inject_shell_cmd();
    inject_ping();
    inject_log_tail();
    inject_rss();
    inject_services();
}

/// Handle a click on a row's remove button: the button's `data-<attr>` holds
/// the row index, which is deleted at `path/<index>`. True if it was removed.
async fn remove_clicked(e: web_sys::Event, attr: &str, path: &str) -> bool {
    let btn = dom::target(&e).closest(&format!("[data-{attr}]"));
    match parse_int(&btn.data(attr)) {
        Some(idx) => or_toast(api::delete(&format!("{path}/{idx}")).await).is_some(),
        None => false,
    }
}

// ─── HTTP monitor ────────────────────────────────────────────────────────────

fn inject_http_status() {
    let section = card(
        "http-status-section",
        r#"<h2>🌐 HTTP Monitor</h2>
<p class="card-hint">Ping up to 10 URLs; status shown on panel with response codes and latency. Checks run every 30 s.</p>
<ul id="hs-list" class="sched-list" style="margin-bottom:0.75rem"></ul>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:1"><label>Label</label>
    <input type="text" id="hs-label" placeholder="My Site" style="width:100%"></div>
  <div class="form-group" style="flex:2"><label>URL</label>
    <input type="url" id="hs-url" placeholder="https://example.com" style="width:100%"></div>
  <button id="hs-add-btn" class="btn-primary">Add</button>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">
  <button id="hs-mode-btn" class="btn-primary" style="flex:1">Switch to HTTP Monitor Mode</button>
  <button id="hs-check-btn" class="btn-secondary" style="flex:1" title="Reload the latest results">Refresh</button>
</div>"#,
    );
    place_after(&El::id("world-clock-section"), &section);

    El::id("hs-add-btn").on_async("click", |_| async {
        let (label, url) = (El::id("hs-label").trimmed(), El::id("hs-url").trimmed());
        if label.is_empty() || url.is_empty() {
            return;
        }
        let body = obj! { "label": label.as_str(), "url": url };
        if or_toast(api::post_only("/api/http_status", body).await).is_some() {
            El::id("hs-label").set_val("");
            El::id("hs-url").set_val("");
            fetch_http_status().await;
            toast(&format!("Monitor added: {label}"), "ok", 2000);
        }
    });
    El::id("hs-mode-btn").on_async("click", |_| core::go_to_mode("http_status"));
    El::id("hs-check-btn").on_async("click", |_| fetch_http_status());
    El::id("hs-list").on_async("click", |e| async {
        if remove_clicked(e, "hs-remove", "/api/http_status").await {
            fetch_http_status().await;
        }
    });
}

pub async fn fetch_http_status() {
    let Ok(d) = api::get("/api/http_status").await else {
        return;
    };
    let monitors = d.get("monitors").arr();
    let list = El::id("hs-list");
    if monitors.is_empty() {
        list.set_html(&placeholder_item("No monitors configured yet."));
        return;
    }
    let results = d.get("results");
    let rows: String = monitors
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let res = results.at(idx as u32);
            let checked = res.truthy();
            let (mark, color) = match (checked, res.get("ok").truthy()) {
                (false, _) => ("…", "var(--overlay0)"),
                (true, true) => ("✓", "var(--green)"),
                (true, false) => ("✗", "var(--red)"),
            };
            let detail = if checked {
                let code = res.get("status_code").num().filter(|c| *c > 0.0).map_or("timeout".to_string(), |c| c.to_string());
                let latency = res.get("latency_ms").num().filter(|l| *l > 0.0).map_or(String::new(), |l| format!(" {l}ms"));
                format!(r#"<span style="margin-left:0.5rem;font-size:0.8rem;color:{color}">{code}{latency}</span>"#)
            } else {
                String::new()
            };
            format!(
                r#"<li class="sched-entry"><span class="sched-mode" style="color:{color}">{mark} {}</span><span class="sched-time" style="font-size:0.8rem;color:var(--overlay0)">{}</span>{detail}<button class="sched-remove" data-hs-remove="{idx}" title="Remove">✕</button></li>"#,
                esc(&m.get("label").string()),
                esc(&prefix(&m.get("url").string(), 40)),
            )
        })
        .collect();
    list.set_html(&rows);
}

// ─── Shell command ───────────────────────────────────────────────────────────

fn inject_shell_cmd() {
    let section = card(
        "shell-cmd-section",
        r#"<h2>💻 Shell Command</h2>
<p class="card-hint">Execute a shell command and display stdout on the panel. Refreshes on the configured interval.</p>
<div class="form-group"><label>Command</label>
  <input type="text" id="sc-cmd" placeholder="uptime && df -h | head -4" style="width:100%"></div>
<div class="form-row" style="gap:0.5rem;align-items:flex-end">
  <div class="form-group" style="flex:1"><label>Refresh (seconds; 0 = manual)</label>
    <input type="number" id="sc-interval" value="10" min="0" style="width:100%"></div>
  <button id="sc-run-btn" class="btn-primary">Set &amp; Run</button>
  <button id="sc-now-btn" class="btn-secondary" title="Force re-run now">↺ Now</button>
  <button id="sc-clear-btn" class="btn-danger">✕</button>
</div>
<div id="sc-output" style="font-family:monospace;font-size:0.8rem;background:var(--crust);border-radius:6px;padding:0.6rem;margin-top:0.5rem;white-space:pre-wrap;max-height:150px;overflow-y:auto;color:var(--text)"></div>"#,
    );
    place_after(&El::id("http-status-section"), &section);

    // The command takes a moment to run, so read its output a little later.
    let refresh_soon = || {
        dom::set_timeout(2000, || dom::spawn(fetch_shell_cmd()));
    };

    El::id("sc-run-btn").on_async("click", move |_| async move {
        let cmd = El::id("sc-cmd").trimmed();
        if cmd.is_empty() {
            return;
        }
        // Zero is meaningful here (run only on demand), so only a blank
        // field falls back to the default.
        let interval = parse_int(&El::id("sc-interval").val()).unwrap_or(10).max(0) as f64;
        let body = obj! { "cmd": cmd, "interval_secs": interval, "switch_mode": true };
        if or_toast(api::post_only("/api/shell_cmd", body).await).is_some() {
            toast("Command set, running…", "ok", 2000);
            refresh_soon();
        }
    });
    El::id("sc-now-btn").on_async("click", move |_| async move {
        if or_toast(api::post_empty("/api/shell_cmd/run").await).is_some() {
            toast("Re-running command…", "ok", 1500);
            refresh_soon();
        }
    });
    El::id("sc-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/shell_cmd").await).is_some() {
            El::id("sc-output").set_text("");
            El::id("sc-cmd").set_val("");
            core::fetch_status().await;
            toast("Shell command cleared", "ok", 1500);
        }
    });
}

pub async fn fetch_shell_cmd() {
    let Ok(d) = api::get("/api/shell_cmd").await else {
        return;
    };
    let out = El::id("sc-output");
    let cmd = d.get("cmd").string();
    let input = El::id("sc-cmd");
    // Don't overwrite a command the user is in the middle of typing.
    if !cmd.is_empty() && !input.same(&El(dom::doc().active_element())) {
        input.set_val(&cmd);
    }
    let output = d.get("output").string();
    if !output.is_empty() {
        out.color(if d.get("exit_ok").truthy() {
            "var(--text)"
        } else {
            "var(--red)"
        })
        .set_text(&output);
    } else {
        out.set_text(if cmd.is_empty() {
            "(no command set)"
        } else {
            "(no output yet)"
        });
    }
}

// ─── Ping monitor ────────────────────────────────────────────────────────────

fn inject_ping() {
    let section = card(
        "ping-section",
        r#"<h2>📶 Ping Monitor</h2>
<p class="card-hint">ICMP ping up to 12 hosts; displays RTT and packet loss on panel. Checks run every 10 s.</p>
<ul id="pm-list" class="sched-list" style="margin-bottom:0.75rem"></ul>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:1"><label>Label</label>
    <input type="text" id="pm-label" placeholder="Router" style="width:100%"></div>
  <div class="form-group" style="flex:2"><label>Host / IP</label>
    <input type="text" id="pm-host" placeholder="192.168.1.1" style="width:100%"></div>
  <button id="pm-add-btn" class="btn-primary">Add</button>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">
  <button id="pm-mode-btn" class="btn-primary" style="flex:1">Switch to Ping Monitor Mode</button>
</div>"#,
    );
    place_after(&El::id("shell-cmd-section"), &section);

    El::id("pm-add-btn").on_async("click", |_| async {
        let (label, host) = (El::id("pm-label").trimmed(), El::id("pm-host").trimmed());
        if label.is_empty() || host.is_empty() {
            return;
        }
        let body = obj! { "label": label.as_str(), "host": host };
        if or_toast(api::post_only("/api/ping_monitor", body).await).is_some() {
            El::id("pm-label").set_val("");
            El::id("pm-host").set_val("");
            fetch_ping().await;
            toast(&format!("Ping host added: {label}"), "ok", 2000);
        }
    });
    El::id("pm-mode-btn").on_async("click", |_| core::go_to_mode("ping_monitor"));
    El::id("pm-list").on_async("click", |e| async {
        if remove_clicked(e, "pm-remove", "/api/ping_monitor").await {
            fetch_ping().await;
        }
    });
}

pub async fn fetch_ping() {
    let Ok(d) = api::get("/api/ping_monitor").await else {
        return;
    };
    let hosts = d.get("hosts").arr();
    let list = El::id("pm-list");
    if hosts.is_empty() {
        list.set_html(&placeholder_item("No hosts configured yet."));
        return;
    }
    let results = d.get("results");
    let rows: String = hosts
        .iter()
        .enumerate()
        .map(|(idx, h)| {
            let res = results.at(idx as u32);
            let latency = res.get("latency_ms").num();
            let color = match latency {
                Some(ms) if ms < 50.0 => "var(--green)",
                Some(ms) if ms < 200.0 => "var(--yellow)",
                Some(_) => "var(--peach)",
                None => "var(--red)",
            };
            let detail = if res.truthy() {
                let rtt = latency.map_or("timeout".to_string(), |ms| format!("{ms:.1} ms"));
                let loss = res.get("loss_pct").num().filter(|l| *l > 0.0).map_or(String::new(), |l| format!(" {l:.0}% loss"));
                format!(r#"<span style="margin-left:0.5rem;font-size:0.8rem;color:{color}">{rtt}{loss}</span>"#)
            } else {
                String::new()
            };
            format!(
                r#"<li class="sched-entry"><span class="sched-mode" style="color:{color}">{}</span><span class="sched-time" style="font-size:0.8rem;color:var(--overlay0)">{}</span>{detail}<button class="sched-remove" data-pm-remove="{idx}" title="Remove">✕</button></li>"#,
                esc(&h.get("label").string()),
                esc(&h.get("host").string()),
            )
        })
        .collect();
    list.set_html(&rows);
}

// ─── Log tail ────────────────────────────────────────────────────────────────

fn inject_log_tail() {
    let section = card(
        "log-tail-section",
        r#"<h2>📄 Log Tail Viewer</h2>
<p class="card-hint">Display the last N lines of any log file live on the panel. Updates every few seconds.</p>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:3"><label>File path</label>
    <input type="text" id="lt-path-input" placeholder="/var/log/syslog" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>Lines</label>
    <input type="number" id="lt-n-input" value="16" min="1" max="50" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>Interval (s)</label>
    <input type="number" id="lt-interval-input" value="3" min="1" max="60" style="width:100%"></div>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">
  <button id="lt-set-btn"   class="btn-primary" style="flex:2">Set &amp; Switch to Log Tail</button>
  <button id="lt-apply-btn" class="btn-secondary" style="flex:1">Apply (no switch)</button>
  <button id="lt-clear-btn" class="btn-danger"    style="flex:1">Clear</button>
</div>
<div id="lt-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>
<pre id="lt-preview" style="margin-top:0.5rem;background:var(--crust);color:var(--subtext1);font-size:0.75rem;max-height:160px;overflow-y:auto;padding:0.5rem;border-radius:4px;white-space:pre-wrap;word-break:break-all"></pre>"#,
    );
    place_after(&El::id("ping-section"), &section);

    for (id, switch_mode) in [("lt-set-btn", true), ("lt-apply-btn", false)] {
        El::id(id).on_async("click", move |_| async move {
            let path = El::id("lt-path-input").trimmed();
            if path.is_empty() {
                return toast("Enter a file path first", "alert", 2000);
            }
            let body = obj! {
                "path": path.as_str(),
                "n": int_or(&El::id("lt-n-input").val(), 16) as f64,
                "interval_secs": int_or(&El::id("lt-interval-input").val(), 3) as f64,
                "switch_mode": switch_mode,
            };
            if or_toast(api::post_only("/api/log_tail", body).await).is_some() {
                let what = if switch_mode { "started" } else { "configured" };
                toast(&format!("Log tail {what}: {path}"), "ok", 2500);
                dom::set_timeout(500, || dom::spawn(fetch_log_tail()));
                if switch_mode {
                    core::fetch_status().await;
                }
            }
        });
    }
    El::id("lt-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/log_tail").await).is_some() {
            El::id("lt-status").set_text("Cleared.");
            El::id("lt-preview").set_text("");
            core::fetch_status().await;
            toast("Log tail cleared", "ok", 2000);
        }
    });
}

pub async fn fetch_log_tail() {
    let Ok(d) = api::get("/api/log_tail").await else {
        return;
    };
    let (status, preview) = (El::id("lt-status"), El::id("lt-preview"));
    let path = d.get("path").string();
    if path.is_empty() {
        status.set_text("No file configured.");
        preview.set_text("");
        return;
    }
    let ok = d.get("ok").truthy();
    let lines: Vec<String> = d
        .get("lines")
        .arr()
        .iter()
        .map(super::js::Js::show)
        .collect();
    let (text, color) = if ok {
        ("✓ readable", "var(--green)")
    } else {
        ("✗ unreadable", "var(--red)")
    };
    status.set_html(&format!(
        r#"<span style="color:{color}">{text}</span> &mdash; <code>{}</code> ({} lines, every {}s)"#,
        esc(&path),
        lines.len(),
        esc(&d.get("interval_secs").show()),
    ));

    let input = El::id("lt-path-input");
    if input.val().is_empty() {
        input.set_val(&path);
    }
    if lines.is_empty() {
        preview.set_text(if ok {
            "(no lines yet)"
        } else {
            "(file not readable)"
        });
    } else {
        preview.set_text(&lines.join("\n"));
        // Keep the newest line in view.
        preview.set("scrollTop", preview.get("scrollHeight"));
    }
}

// ─── RSS feed ────────────────────────────────────────────────────────────────

fn inject_rss() {
    let section = card(
        "rss-section",
        r#"<h2>📰 RSS / Atom Feed</h2>
<p class="card-hint">Display the latest headlines from any RSS or Atom feed URL on the panel.</p>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:4"><label>Feed URL</label>
    <input type="url" id="rss-url-input" placeholder="https://feeds.bbci.co.uk/news/rss.xml" style="width:100%"></div>
  <div class="form-group" style="flex:1"><label>Interval (s)</label>
    <input type="number" id="rss-interval-input" value="300" min="30" max="86400" style="width:100%"></div>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">
  <button id="rss-set-btn"   class="btn-primary"   style="flex:2">Set &amp; Switch to RSS Feed</button>
  <button id="rss-fetch-btn" class="btn-secondary" style="flex:1">⟳ Fetch Now</button>
  <button id="rss-clear-btn" class="btn-danger"    style="flex:1">Clear</button>
</div>
<div id="rss-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>
<ul id="rss-items" style="margin-top:0.5rem;list-style:none;padding:0;font-size:0.82rem;max-height:160px;overflow-y:auto"></ul>"#,
    );
    place_after(&El::id("log-tail-section"), &section);

    // Give the daemon a moment to store the feed before reading it back.
    let refresh_soon = || {
        dom::set_timeout(400, || dom::spawn(fetch_rss()));
    };

    El::id("rss-set-btn").on_async("click", move |_| async move {
        let url = El::id("rss-url-input").trimmed();
        if url.is_empty() {
            return toast("Enter a feed URL first", "alert", 2000);
        }
        let secs = int_or(&El::id("rss-interval-input").val(), 300) as f64;
        let body = obj! { "url": url, "interval_secs": secs, "switch_mode": true };
        if or_toast(api::post_only("/api/rss", body).await).is_some() {
            toast("RSS feed started", "ok", 2500);
            refresh_soon();
            core::fetch_status().await;
        }
    });
    El::id("rss-fetch-btn").on_async("click", move |_| async move {
        match api::post_empty("/api/rss/fetch").await {
            Ok(()) => {
                toast("RSS feed fetched", "ok", 2000);
                refresh_soon();
            }
            Err(e) => toast(&format!("Fetch error: {e}"), "alert", 3000),
        }
    });
    El::id("rss-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/rss").await).is_some() {
            El::id("rss-status").set_text("Cleared.");
            El::id("rss-items").clear();
            core::fetch_status().await;
            toast("RSS feed cleared", "ok", 2000);
        }
    });
}

pub async fn fetch_rss() {
    let Ok(d) = api::get("/api/rss").await else {
        return;
    };
    let (status, items_el) = (El::id("rss-status"), El::id("rss-items"));
    let url = d.get("url").string();
    if url.is_empty() {
        status.set_text("No feed configured.");
        items_el.clear();
        return;
    }
    let ok = d.get("ok").truthy();
    let items = d.get("items").arr();
    let title = d
        .get("title")
        .str()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| url.clone());
    let (text, color) = if ok {
        ("✓ live", "var(--green)")
    } else {
        ("✗ unavailable", "var(--red)")
    };
    status.set_html(&format!(
        r#"<span style="color:{color}">{text}</span> &mdash; <a href="{}" target="_blank" rel="noopener noreferrer" style="color:var(--blue)">{}</a> ({} items)"#,
        safe_href(&url),
        esc(&title),
        items.len(),
    ));

    let input = El::id("rss-url-input");
    if input.val().is_empty() {
        input.set_val(&url);
    }
    if items.is_empty() {
        let message = if ok {
            "(no items yet)"
        } else {
            "(feed unavailable)"
        };
        items_el.set_html(&format!(
            r#"<li style="color:var(--overlay0)">{message}</li>"#
        ));
        return;
    }
    // Each item is a [headline, date or author] pair.
    let rows: String = items
        .iter()
        .take(6)
        .map(|item| {
            let date = item.at(1).string();
            let date = if date.is_empty() {
                String::new()
            } else {
                format!(r#" <span style="color:var(--overlay0);font-size:0.75rem">{}</span>"#, esc(&date))
            };
            format!(
                r#"<li style="padding:0.25rem 0;border-bottom:1px solid var(--surface0)"><span style="color:var(--text)">{}</span>{date}</li>"#,
                esc(&item.at(0).string()),
            )
        })
        .collect();
    items_el.set_html(&rows);
}

// ─── Services ────────────────────────────────────────────────────────────────

fn inject_services() {
    let section = card(
        "service-section",
        r#"<h2>⚙ Service Status Monitor</h2>
<p class="card-hint">Watch systemd unit states on the panel. Enter unit names (one per line or comma-separated).</p>
<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
  <div class="form-group" style="flex:3"><label>Units <span class="hint-inline">(e.g. nginx.service, sshd)</span></label>
    <textarea id="svc-units-input" rows="3" placeholder="nginx.service&#10;sshd.service&#10;postgresql.service" style="width:100%;resize:vertical;background:var(--surface0);color:var(--text);border:1px solid var(--overlay0);border-radius:4px;padding:0.4rem"></textarea></div>
  <div class="form-group" style="flex:1"><label>Poll interval (s)</label>
    <input type="number" id="svc-interval-input" value="10" min="5" max="3600" style="width:100%"></div>
</div>
<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">
  <button id="svc-set-btn"   class="btn-primary"   style="flex:2">Set &amp; Switch to Services</button>
  <button id="svc-apply-btn" class="btn-secondary" style="flex:1">Apply (no switch)</button>
  <button id="svc-clear-btn" class="btn-danger"    style="flex:1">Clear</button>
</div>
<div id="svc-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>
<table id="svc-table" style="margin-top:0.5rem;width:100%;font-size:0.82rem;border-collapse:collapse"></table>"#,
    );
    place_after(&El::id("rss-section"), &section);

    for (id, switch_mode) in [("svc-set-btn", true), ("svc-apply-btn", false)] {
        El::id(id).on_async("click", move |_| async move {
            let units = split_list(&El::id("svc-units-input").val(), true);
            if units.is_empty() {
                return toast("Enter at least one unit name", "alert", 2000);
            }
            let body = obj! {
                "units": arr(units),
                "interval_secs": int_or(&El::id("svc-interval-input").val(), 10) as f64,
                "switch_mode": switch_mode,
            };
            if or_toast(api::post_only("/api/services", body).await).is_some() {
                toast(
                    if switch_mode {
                        "Services monitor started"
                    } else {
                        "Services configured"
                    },
                    "ok",
                    2500,
                );
                dom::set_timeout(400, || dom::spawn(fetch_services()));
                if switch_mode {
                    core::fetch_status().await;
                }
            }
        });
    }
    El::id("svc-clear-btn").on_async("click", |_| async {
        if or_toast(api::delete("/api/services").await).is_some() {
            El::id("svc-status").set_text("Cleared.");
            El::id("svc-table").clear();
            core::fetch_status().await;
            toast("Services cleared", "ok", 2000);
        }
    });
}

pub async fn fetch_services() {
    let Ok(d) = api::get("/api/services").await else {
        return;
    };
    let (status, table) = (El::id("svc-status"), El::id("svc-table"));
    let units: Vec<String> = d
        .get("units")
        .arr()
        .iter()
        .map(super::js::Js::show)
        .collect();
    if units.is_empty() {
        status.set_text("No units configured.");
        table.clear();
        return;
    }
    let statuses = d.get("statuses").arr();
    let count = |state: &str| {
        statuses
            .iter()
            .filter(|s| s.get("state").string() == state)
            .count()
    };
    status.set_html(&format!(
        r#"<span style="color:var(--green)">{} active</span>  <span style="color:var(--red)">{} failed</span>  of {} units (poll every {}s)"#,
        count("active"),
        count("failed"),
        units.len(),
        esc(&d.get("interval").show()),
    ));

    let input = El::id("svc-units-input");
    if input.val().is_empty() {
        input.set_val(&units.join("\n"));
    }

    const CELL: &str = "padding:0.2rem 0.3rem";
    let header: String = ["Unit", "State", "Sub"]
        .iter()
        .map(|h| format!(r#"<th style="text-align:left;{CELL};color:var(--overlay0)">{h}</th>"#))
        .collect();
    let rows: String = statuses
        .iter()
        .map(|s| {
            let state = s.get("state").string();
            let (icon, color) = match state.as_str() {
                "active" => ("●", "var(--green)"),
                "inactive" => ("○", "var(--overlay0)"),
                "failed" => ("✖", "var(--red)"),
                _ => ("?", "var(--yellow)"),
            };
            let name = s.get("name").string();
            format!(
                r#"<tr style="border-bottom:1px solid var(--surface0)"><td style="{CELL};color:var(--text)">{}</td><td style="{CELL};color:{color}">{icon} {}</td><td style="{CELL};color:var(--subtext1)">{}</td></tr>"#,
                esc(name.strip_suffix(".service").unwrap_or(&name)),
                esc(&state),
                esc(&s.get("sub").string()),
            )
        })
        .collect();
    table.set_html(&format!(
        r#"<tr style="border-bottom:1px solid var(--surface0)">{header}</tr>{rows}"#
    ));
}
