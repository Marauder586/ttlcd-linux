//! Pure formatting and parsing helpers. Nothing here touches the browser, so
//! the tests run on the host.

/// `1d 2h 3m`, `2h 3m 4s` or `3m 4s`.
pub fn fmt_uptime(secs: f64) -> String {
    let secs = secs.max(0.0) as u64;
    let (d, h, m, s) = (
        secs / 86400,
        secs % 86400 / 3600,
        secs % 3600 / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    }
}

/// Byte count with a B/K/M/G suffix.
pub fn fmt_bytes(b: f64) -> String {
    if b < 1024.0 {
        format!("{b}B")
    } else if b < 1_048_576.0 {
        format!("{:.1}K", b / 1024.0)
    } else if b < 1_073_741_824.0 {
        format!("{:.1}M", b / 1_048_576.0)
    } else {
        format!("{:.2}G", b / 1_073_741_824.0)
    }
}

/// Stopwatch time: `mm:ss.cc`, or `h:mm:ss.cc` past an hour.
pub fn fmt_sw_time(ms: f64) -> String {
    let ms = ms.max(0.0) as u64;
    let (h, m, s, cs) = (
        ms / 3_600_000,
        ms % 3_600_000 / 60_000,
        ms % 60_000 / 1000,
        ms % 1000 / 10,
    );
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{cs:02}")
    } else {
        format!("{m:02}:{s:02}.{cs:02}")
    }
}

/// Escape text for use in HTML, including inside quoted attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Only http(s) URLs may become links; anything else (javascript:, data:) is dropped.
pub fn safe_href(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        esc(url)
    } else {
        "#".into()
    }
}

/// The longest prefix of `s` (after leading whitespace) that looks like a number.
fn numeric_prefix(s: &str, allow_fraction: bool) -> &str {
    let s = s.trim_start();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    for (i, c) in s.char_indices() {
        let ok = match c {
            '+' | '-' => i == 0,
            '0'..='9' => {
                seen_digit = true;
                true
            }
            '.' if allow_fraction && !seen_dot => {
                seen_dot = true;
                true
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end = i + c.len_utf8();
    }
    if seen_digit {
        &s[..end]
    } else {
        ""
    }
}

/// Like JavaScript's `parseInt(s, 10)`: reads a leading integer and ignores
/// whatever follows. `None` where JavaScript gives `NaN`.
pub fn parse_int(s: &str) -> Option<i64> {
    numeric_prefix(s, false).parse().ok()
}

/// Like JavaScript's `parseFloat(s)` for plain decimal numbers.
pub fn parse_float(s: &str) -> Option<f64> {
    numeric_prefix(s, true).trim_end_matches('.').parse().ok()
}

/// `parseInt(s, 10) || default`: the default also replaces zero.
pub fn int_or(s: &str, default: i64) -> i64 {
    parse_int(s).filter(|v| *v != 0).unwrap_or(default)
}

/// `value`, or `default` if it is empty.
pub fn non_empty_or(value: String, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

/// A mode id as shown to the user: `cpu_detail` becomes `cpu detail`.
pub fn mode_label(mode: &str) -> String {
    mode.replace('_', " ")
}

/// The first `n` characters of `s`.
pub fn prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `"s"` unless `n` is exactly one.
pub fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `UTC+5:30`, `UTC-8`, `UTC+0`.
pub fn utc_offset_label(mins: i64) -> String {
    let (h, m) = (mins.abs() / 60, mins.abs() % 60);
    let sign = if mins >= 0 { '+' } else { '-' };
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

/// Split on commas and/or newlines, trimming and dropping empty entries.
pub fn split_list(raw: &str, newlines: bool) -> Vec<String> {
    raw.split(|c| c == ',' || (newlines && c == '\n'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Colour for a 0–100 % gauge that turns yellow, then red, as it fills.
pub fn load_color(pct: f64, calm: &'static str) -> &'static str {
    if pct > 85.0 {
        "var(--red)"
    } else if pct > 60.0 {
        "var(--yellow)"
    } else {
        calm
    }
}

/// Insert `item` at the front of `list`, dropping any earlier copy and
/// keeping at most `max` entries.
pub fn push_recent(list: &mut Vec<String>, item: &str, max: usize) {
    list.retain(|m| m != item);
    list.insert(0, item.to_string());
    list.truncate(max);
}

/// Whether a mode button matches a search query (already lower-cased).
pub fn mode_matches(query: &str, mode: &str, label: &str, title: &str) -> bool {
    query.is_empty()
        || [mode, label, title]
            .iter()
            .any(|s| s.to_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_picks_largest_unit() {
        assert_eq!(fmt_uptime(59.0), "0m 59s");
        assert_eq!(fmt_uptime(3725.0), "1h 2m 5s");
        assert_eq!(fmt_uptime(90061.0), "1d 1h 1m");
        assert_eq!(fmt_uptime(-5.0), "0m 0s");
    }

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(fmt_bytes(0.0), "0B");
        assert_eq!(fmt_bytes(1023.0), "1023B");
        assert_eq!(fmt_bytes(1536.0), "1.5K");
        assert_eq!(fmt_bytes(5.0 * 1_048_576.0), "5.0M");
        assert_eq!(fmt_bytes(3.0 * 1_073_741_824.0), "3.00G");
    }

    #[test]
    fn stopwatch_time_shows_hours_only_when_needed() {
        assert_eq!(fmt_sw_time(0.0), "00:00.00");
        assert_eq!(fmt_sw_time(61_230.0), "01:01.23");
        assert_eq!(fmt_sw_time(3_661_000.0), "1:01:01.00");
        assert_eq!(fmt_sw_time(-1.0), "00:00.00");
    }

    #[test]
    fn esc_neutralises_markup_and_attribute_breakout() {
        assert_eq!(
            esc(r#"<a href="x" on='y'>&"#),
            "&lt;a href=&quot;x&quot; on=&#39;y&#39;&gt;&amp;"
        );
        assert_eq!(esc("plain ✓"), "plain ✓");
    }

    #[test]
    fn safe_href_allows_only_http() {
        assert_eq!(
            safe_href("https://a.example/?q=\"1\""),
            "https://a.example/?q=&quot;1&quot;"
        );
        assert_eq!(safe_href("HTTP://a.example"), "HTTP://a.example");
        assert_eq!(safe_href("javascript:alert(1)"), "#");
        assert_eq!(safe_href("data:text/html,x"), "#");
        assert_eq!(safe_href(""), "#");
    }

    #[test]
    fn parse_int_matches_javascript() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("  -7px"), Some(-7));
        assert_eq!(parse_int("3.9"), Some(3));
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("abc"), None);
        assert_eq!(parse_int("-"), None);
    }

    #[test]
    fn parse_float_matches_javascript() {
        assert_eq!(parse_float("21.5"), Some(21.5));
        assert_eq!(parse_float("-3"), Some(-3.0));
        assert_eq!(parse_float("7."), Some(7.0));
        assert_eq!(parse_float("1.2.3"), Some(1.2));
        assert_eq!(parse_float("n/a"), None);
    }

    #[test]
    fn int_or_treats_zero_and_garbage_as_missing() {
        assert_eq!(int_or("12", 5), 12);
        assert_eq!(int_or("0", 5), 5);
        assert_eq!(int_or("", 5), 5);
    }

    #[test]
    fn utc_offsets() {
        assert_eq!(utc_offset_label(0), "UTC+0");
        assert_eq!(utc_offset_label(330), "UTC+5:30");
        assert_eq!(utc_offset_label(-480), "UTC-8");
        assert_eq!(utc_offset_label(-210), "UTC-3:30");
    }

    #[test]
    fn split_list_handles_commas_and_newlines() {
        assert_eq!(split_list("a, b,,c ", false), ["a", "b", "c"]);
        assert_eq!(split_list("a\nb, c\n\n", true), ["a", "b", "c"]);
        assert_eq!(split_list("a\nb", false), ["a\nb"]);
        assert!(split_list(" , ", true).is_empty());
    }

    #[test]
    fn recent_list_dedupes_and_caps() {
        let mut list = vec!["a".to_string(), "b".into(), "c".into()];
        push_recent(&mut list, "b", 3);
        assert_eq!(list, ["b", "a", "c"]);
        push_recent(&mut list, "d", 3);
        assert_eq!(list, ["d", "b", "a"]);
    }

    #[test]
    fn mode_search_checks_id_label_and_title() {
        assert!(mode_matches("", "clock", "Clock", ""));
        assert!(mode_matches("cpu", "cpu_detail", "Detail", ""));
        assert!(mode_matches(
            "giant",
            "big_clock",
            "Big Clock",
            "Full-screen Giant clock"
        ));
        assert!(!mode_matches(
            "cat",
            "clock",
            "Clock",
            "Large digital clock"
        ));
    }

    #[test]
    fn small_helpers() {
        assert_eq!(non_empty_or(String::new(), "x"), "x");
        assert_eq!(non_empty_or("y".into(), "x"), "y");
        assert_eq!(mode_label("cpu_core_bars"), "cpu core bars");
        assert_eq!(prefix("héllo", 2), "hé");
        assert_eq!(plural(1), "");
        assert_eq!(plural(0), "s");
        assert_eq!(load_color(90.0, "var(--blue)"), "var(--red)");
        assert_eq!(load_color(70.0, "var(--blue)"), "var(--yellow)");
        assert_eq!(load_color(10.0, "var(--blue)"), "var(--blue)");
    }
}
