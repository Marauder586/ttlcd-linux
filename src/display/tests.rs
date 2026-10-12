//! Tests for the shared drawing helpers and the basic screens.

use super::*;

fn assert_panel_size(img: &DynamicImage) {
    assert_eq!(img.width(), 480, "frame must be 480px wide");
    assert_eq!(img.height(), 480, "frame must be 480px tall");
}

#[test]
fn test_clock_size() {
    assert_panel_size(&render_clock());
}

#[test]
fn test_clock_sysinfo_size() {
    assert_panel_size(&render_clock_sysinfo());
}

#[test]
fn test_sysinfo_size() {
    assert_panel_size(&render_sysinfo());
}

#[test]
fn test_cpu_detail_size() {
    assert_panel_size(&render_cpu_detail());
}

#[test]
fn test_network_size() {
    // render_network sleeps 1 s — skip in fast test runs via cfg
    // To run: cargo test -- --ignored
    assert_panel_size(&render_network());
}

#[test]
fn test_disk_usage_size() {
    assert_panel_size(&render_disk_usage());
}

#[test]
fn test_process_top_size() {
    // render_process_top sleeps ~200 ms for CPU delta
    assert_panel_size(&render_process_top());
}

#[test]
fn test_solid_size() {
    assert_panel_size(&render_solid(255, 0, 128));
}

#[test]
fn test_test_card_size() {
    assert_panel_size(&render_test_card());
}

#[test]
fn test_solid_colour_values() {
    let img = render_solid(10, 20, 30).into_rgb8();
    // Every pixel must match the requested colour
    for px in img.pixels() {
        assert_eq!(px.0, [10, 20, 30]);
    }
}

#[test]
fn test_blank_background() {
    let img = blank();
    // All pixels must match BG colour
    for px in img.pixels() {
        assert_eq!(*px, BG);
    }
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1_048_576), "1.0 MB");
    assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
}

#[test]
fn test_format_bytes_si() {
    assert_eq!(format_bytes_si(1_000_000_000), "1.0 GB");
    assert_eq!(format_bytes_si(1_000_000_000_000), "1.0 TB");
    assert_eq!(format_bytes_si(500_000_000), "500 MB"); // 500 000 000 bytes
}

#[test]
fn test_format_uptime() {
    assert_eq!(format_uptime(45), "0m 45s");
    assert_eq!(format_uptime(3661), "1h 1m");
    assert_eq!(format_uptime(90061), "1d 1h 1m");
}

#[test]
fn test_temperature_size() {
    // render_temperature gracefully handles no-sensor environments (VMs)
    // by rendering a placeholder message; either way it must be 480x480.
    assert_panel_size(&render_temperature());
}

#[test]
fn test_custom_text_size() {
    use crate::display::{CustomTextPayload, NamedColor};
    let payload = CustomTextPayload {
        title: Some("Hello World".into()),
        subtitle: Some("subtitle".into()),
        lines: vec!["line 1".into(), "line 2".into()],
        progress: Some(0.75),
        color: NamedColor::Green,
        show_time: true,
        ..Default::default()
    };
    assert_panel_size(&render_custom_text(&payload));
}

#[test]
fn test_memory_detail_size() {
    assert_panel_size(&render_memory_detail());
}

#[test]
fn test_countdown_size() {
    // Renders with time remaining
    assert_panel_size(&render_countdown(125, 300, "Build cooldown"));
}

#[test]
fn test_countdown_expired_size() {
    // Renders the "TIME'S UP" screen
    assert_panel_size(&render_countdown(0, 300, "Done"));
}

#[test]
fn test_cpu_graph_size() {
    // Push a few fake samples so the chart has data to draw
    for pct in [10.0_f32, 25.0, 50.0, 75.0, 90.0] {
        push_cpu_history(pct);
    }
    assert_panel_size(&render_cpu_graph());
}

#[test]
fn test_net_graph_size() {
    for (rx, tx) in [(1024u64, 512u64), (2048, 1024), (4096, 2048)] {
        push_net_history(rx, tx);
    }
    assert_panel_size(&render_net_graph());
}

#[test]
fn test_recent_alerts_size() {
    let entries: Vec<(u64, bool, &str)> = vec![
        (1747000000, true, "ERROR — Build FAILED"),
        (1747000060, false, "Deploying ttlcd-rs v0.5"),
        (1747000120, true, "SUCCESS — Deploy complete"),
    ];
    assert_panel_size(&render_recent_alerts(&entries));
}

#[test]
fn test_recent_alerts_empty_size() {
    assert_panel_size(&render_recent_alerts(&[]));
}

#[test]
fn test_process_memory_size() {
    assert_panel_size(&render_process_memory());
}

#[test]
fn test_cpu_history_accessor() {
    push_cpu_history(42.0);
    let h = get_cpu_history();
    assert!(!h.is_empty(), "history should have at least one sample");
    assert!(h.iter().all(|&v| (0.0..=100.0).contains(&v)));
}

#[test]
fn test_net_history_accessor() {
    push_net_history(1024, 512);
    let (rx, tx) = get_net_history();
    assert!(!rx.is_empty());
    assert!(!tx.is_empty());
    assert_eq!(rx.len(), tx.len());
}

#[test]
fn test_calendar_size() {
    assert_panel_size(&render_calendar());
}

#[test]
fn test_boot_splash_size() {
    assert_panel_size(&render_boot_splash("0.7.0"));
}

#[test]
fn test_big_text_banner_single_line() {
    use image::Rgb;
    assert_panel_size(&render_big_text_banner(
        &["99.8%"],
        "SLA",
        Rgb([137u8, 180, 250]),
        true,
    ));
}

#[test]
fn test_big_text_banner_three_lines() {
    use image::Rgb;
    assert_panel_size(&render_big_text_banner(
        &["99.8%", "Uptime", "Last 30 days"],
        "SLA",
        Rgb([166u8, 227, 161]),
        false,
    ));
}

#[test]
fn test_stopwatch_ready_size() {
    // fresh state: no elapsed time, not running, no laps
    assert_panel_size(&render_stopwatch(0, &[], false));
}

#[test]
fn test_stopwatch_running_size() {
    assert_panel_size(&render_stopwatch(75_123, &[], true));
}

#[test]
fn test_stopwatch_paused_with_laps() {
    let laps = vec![15_000u64, 32_500, 61_200, 95_000, 130_800];
    assert_panel_size(&render_stopwatch(130_800, &laps, false));
}

#[test]
fn test_stopwatch_over_one_hour() {
    assert_panel_size(&render_stopwatch(3_661_500, &[], true));
}

#[test]
fn test_format_sw_time_under_minute() {
    assert_eq!(format_sw_time(0), "00:00.00");
    assert_eq!(format_sw_time(1_000), "00:01.00");
    assert_eq!(format_sw_time(10), "00:00.01");
    assert_eq!(format_sw_time(59_999), "00:59.99");
}

#[test]
fn test_format_sw_time_minutes() {
    assert_eq!(format_sw_time(60_000), "01:00.00");
    assert_eq!(format_sw_time(90_500), "01:30.50");
}

#[test]
fn test_format_sw_time_hours() {
    assert_eq!(format_sw_time(3_600_000), "1:00:00.00");
    assert_eq!(format_sw_time(3_661_000), "1:01:01.00");
}

#[test]
fn test_metrics_empty_size() {
    assert_panel_size(&render_metrics("", &[]));
}

#[test]
fn test_metrics_with_data_size() {
    let metrics = vec![
        Metric::new("CPU", "72.4%"),
        Metric::new("RAM", "6.1 GB"),
        Metric::new("Uptime", "3d 4h"),
        Metric::new("Requests", "1234/s"),
        Metric::new("Errors", "0.12%"),
        Metric::new("Latency", "42ms"),
    ];
    assert_panel_size(&render_metrics("Dashboard", &metrics));
}

#[test]
fn test_metrics_eight_items() {
    let metrics: Vec<Metric> = (1..=8)
        .map(|i| Metric {
            label: format!("Key {i}"),
            value: format!("{i}"),
            color: "green".into(),
        })
        .collect();
    assert_panel_size(&render_metrics("Full Grid", &metrics));
}

#[test]
fn test_weather_empty_size() {
    assert_panel_size(&render_weather(&WeatherData::default()));
}

#[test]
fn test_weather_full_size() {
    let data = WeatherData {
        city: "London".to_string(),
        temp_c: Some(14.5),
        feels_like_c: Some(12.0),
        condition: "Partly cloudy".to_string(),
        humidity_pct: Some(72),
        wind_kmh: Some(18.0),
        wind_dir: "SW".to_string(),
        high_c: Some(16.0),
        low_c: Some(9.0),
        visibility_km: Some(10.0),
        updated_at: Some(1_700_000_000),
    };
    assert_panel_size(&render_weather(&data));
}

#[test]
fn test_weather_hot_size() {
    let data = WeatherData {
        city: "Dubai".to_string(),
        temp_c: Some(42.0),
        condition: "Sunny".to_string(),
        ..Default::default()
    };
    assert_panel_size(&render_weather(&data));
}

#[test]
fn test_weather_freezing_size() {
    let data = WeatherData {
        city: "Reykjavik".to_string(),
        temp_c: Some(-12.0),
        condition: "Snow".to_string(),
        wind_kmh: Some(35.0),
        ..Default::default()
    };
    assert_panel_size(&render_weather(&data));
}

#[test]
fn test_weather_thunder_icon_size() {
    let data = WeatherData {
        condition: "Thunderstorm".to_string(),
        temp_c: Some(20.0),
        ..Default::default()
    };
    assert_panel_size(&render_weather(&data));
}

#[test]
fn test_notes_empty_size() {
    assert_panel_size(&render_notes("", &[]));
}

#[test]
fn test_notes_with_items_size() {
    let items: Vec<String> = (1..=6).map(|i| format!("Note item number {i}")).collect();
    assert_panel_size(&render_notes("Sprint 42", &items));
}

#[test]
fn test_notes_overflow_size() {
    let items: Vec<String> = (1..=15)
        .map(|i| format!("Task {i}: something to do"))
        .collect();
    assert_panel_size(&render_notes("Many Notes", &items));
}

#[test]
fn test_big_clock_size() {
    assert_panel_size(&render_big_clock());
}

#[test]
fn test_qr_code_url_size() {
    assert_panel_size(&render_qr_code("https://example.com/test", "Example"));
}

#[test]
fn test_qr_code_empty_payload() {
    // Empty payload → error frame, should still be panel-sized
    assert_panel_size(&render_qr_code("", "Empty"));
}

#[test]
fn test_pomodoro_work_running_size() {
    assert_panel_size(&render_pomodoro("work", 1440, 1500, 0, true));
}

#[test]
fn test_pomodoro_short_break_paused_size() {
    assert_panel_size(&render_pomodoro("short_break", 120, 300, 1, false));
}

#[test]
fn test_pomodoro_long_break_expired_size() {
    // remaining_secs ≤ 0 should still render (shows 00:00)
    assert_panel_size(&render_pomodoro("long_break", 0, 900, 0, false));
}

#[test]
fn test_battery_size() {
    // render_battery reads from /sys; on CI it returns "No battery detected" — still 480×480
    assert_panel_size(&render_battery());
}

#[test]
fn test_world_clock_defaults_size() {
    assert_panel_size(&render_world_clock(&[]));
}

#[test]
fn test_world_clock_custom_size() {
    let cities = vec![
        ("Tokyo".into(), 540),
        ("UTC".into(), 0),
        ("NYC".into(), -300),
    ];
    assert_panel_size(&render_world_clock(&cities));
}

#[test]
fn test_ticker_short_text_size() {
    // Short text fits in frame and is centred
    assert_panel_size(&render_ticker("Hello!", 0, "blue"));
}

#[test]
fn test_ticker_long_text_scrolling() {
    // Long text scrolls; different positions should still produce panel-sized frames
    let text =
        "This is a very long scrolling ticker message that does not fit in one screen width!";
    assert_panel_size(&render_ticker(text, 0, "green"));
    assert_panel_size(&render_ticker(text, 400, "red"));
}

#[test]
fn test_http_status_empty_size() {
    assert_panel_size(&render_http_status(&[]));
}

#[test]
fn test_http_status_with_results_size() {
    let results = vec![
        HttpMonitorResult {
            label: "Google".into(),
            url: "https://google.com".into(),
            status_code: Some(200),
            latency_ms: Some(45),
            ok: true,
        },
        HttpMonitorResult {
            label: "Down".into(),
            url: "https://example-down.local".into(),
            status_code: None,
            latency_ms: None,
            ok: false,
        },
    ];
    assert_panel_size(&render_http_status(&results));
}

#[test]
fn test_shell_cmd_ok_size() {
    assert_panel_size(&render_shell_cmd(
        "uptime",
        "14:22:01 up 3 days, load average: 0.12",
        true,
        0,
    ));
}

#[test]
fn test_shell_cmd_err_size() {
    assert_panel_size(&render_shell_cmd(
        "bad_cmd",
        "bash: bad_cmd: not found",
        false,
        1715000000,
    ));
}

#[test]
fn test_shell_cmd_multiline_size() {
    let output = "line 1
line 2
line 3
line 4
line 5
line 6
line 7
line 8";
    assert_panel_size(&render_shell_cmd("echo lines", output, true, 1715000000));
}

#[test]
fn test_ping_monitor_empty_size() {
    assert_panel_size(&render_ping_monitor(&[]));
}

#[test]
fn test_ping_monitor_with_results_size() {
    let results = vec![
        PingResult {
            host: "8.8.8.8".into(),
            label: "Google DNS".into(),
            latency_ms: Some(12.3),
            loss_pct: 0.0,
        },
        PingResult {
            host: "1.1.1.1".into(),
            label: "Cloudflare".into(),
            latency_ms: Some(5.8),
            loss_pct: 0.0,
        },
        PingResult {
            host: "192.168.1.1".into(),
            label: "Router".into(),
            latency_ms: None,
            loss_pct: 100.0,
        },
    ];
    assert_panel_size(&render_ping_monitor(&results));
}

#[test]
fn test_dashboard_size() {
    assert_panel_size(&render_dashboard());
}

#[test]
fn test_log_tail_empty_size() {
    assert_panel_size(&render_log_tail("", &[], false));
}

#[test]
fn test_log_tail_ok_empty_lines() {
    assert_panel_size(&render_log_tail("/var/log/syslog", &[], true));
}

#[test]
fn test_log_tail_with_lines() {
    let lines: Vec<String> = (1..=20)
        .map(|i| format!("2026-05-13 12:00:{i:02} [INFO]  test message line {i}"))
        .collect();
    assert_panel_size(&render_log_tail("/var/log/syslog", &lines, true));
}

#[test]
fn test_log_tail_error_colors() {
    let lines = vec![
        "2026-05-13 ERROR failed to connect".to_string(),
        "2026-05-13 WARN  retrying in 5s".to_string(),
        "2026-05-13 DEBUG retry attempt 1".to_string(),
        "2026-05-13 INFO  connected ok".to_string(),
    ];
    assert_panel_size(&render_log_tail("/tmp/app.log", &lines, true));
}

#[test]
fn test_log_tail_long_lines() {
    let lines = vec!["a".repeat(100), "b".repeat(80)];
    assert_panel_size(&render_log_tail("/tmp/test.log", &lines, true));
}

#[test]
fn test_cpu_freq_size() {
    assert_panel_size(&render_cpu_freq());
}

#[test]
fn test_hostname_ip_size() {
    assert_panel_size(&render_hostname_ip());
}

#[test]
fn test_rss_feed_empty_size() {
    let img = render_rss_feed("TEST FEED", &[], true);
    assert_panel_size(&img);
}

#[test]
fn test_rss_feed_items() {
    let items = vec![
        ("Headline one".to_string(), "14 May 2026".to_string()),
        (
            "Another headline that is quite long indeed".to_string(),
            "author".to_string(),
        ),
    ];
    let img = render_rss_feed("My Feed", &items, true);
    assert_panel_size(&img);
}

#[test]
fn test_rss_feed_error_state() {
    let img = render_rss_feed("Offline Feed", &[], false);
    assert_panel_size(&img);
}

#[test]
fn test_service_status_empty() {
    let img = render_service_status(&[]);
    assert_panel_size(&img);
}

#[test]
fn test_service_status_units() {
    let units = vec![
        ServiceStatus {
            name: "nginx.service".to_string(),
            state: "active".to_string(),
            sub: "running".to_string(),
        },
        ServiceStatus {
            name: "sshd.service".to_string(),
            state: "inactive".to_string(),
            sub: "dead".to_string(),
        },
        ServiceStatus {
            name: "bad.service".to_string(),
            state: "failed".to_string(),
            sub: "failed".to_string(),
        },
    ];
    let img = render_service_status(&units);
    assert_panel_size(&img);
}

#[test]
fn test_net_connections_size() {
    // Just verify it renders without panic (may have 0 connections in test env)
    let img = render_net_connections();
    assert_panel_size(&img);
}

#[test]
fn test_uptime_size() {
    let img = render_uptime();
    assert_panel_size(&img);
}

#[test]
fn test_os_info_size() {
    let img = render_os_info();
    assert_panel_size(&img);
}
