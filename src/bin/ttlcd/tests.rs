//! Tests for the daemon helpers.

use super::*;

#[test]
fn web_ui_page_satisfies_its_own_security_policy() {
    let script_src = WEB_UI_CSP
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src"))
        .expect("policy has a script-src directive");
    assert!(!script_src.contains("unsafe-inline"));
    assert!(WEB_UI_CSP.contains("frame-ancestors 'none'"));

    // The policy blocks inline scripts and handlers, so the page must
    // not rely on any.
    let page = include_str!("../../web/index.html");
    for (at, _) in page.match_indices("<script") {
        let tag = &page[at..at + page[at..].find('>').unwrap()];
        assert!(tag.contains("src="), "inline script: {tag}");
    }
    assert!(!page.contains(" onclick=") && !page.contains(" onload="));
    assert!(!page.contains("javascript:"));
}

#[test]
fn loopback_hosts_are_recognised_with_and_without_port() {
    for host in [
        "127.0.0.1",
        "127.0.0.1:7070",
        "localhost",
        "LOCALHOST:7070",
        "[::1]",
        "[::1]:7070",
        "127.8.9.1:80",
    ] {
        assert!(is_loopback_host(host), "{host}");
    }
    for host in [
        "",
        "evil.com",
        "evil.com:7070",
        "192.168.1.10:7070",
        "0.0.0.0:7070",
        "localhost.evil.com",
        "127.0.0.1.evil.com:7070",
        "[::1]evil.com",
        "[::1",
        "[2001:db8::1]:7070",
    ] {
        assert!(!is_loopback_host(host), "{host}");
    }
}

#[test]
fn only_loopback_pages_count_as_same_origin() {
    for origin in [
        "http://127.0.0.1:7070",
        "http://localhost:7070",
        "http://[::1]:7070",
        "https://localhost",
    ] {
        assert!(is_loopback_origin(origin), "{origin}");
    }
    for origin in [
        "null",
        "",
        "http://evil.com",
        "https://evil.com:7070",
        "http://127.0.0.1.evil.com",
        "http://localhost:7070/path",
        "file://localhost",
        "localhost:7070",
    ] {
        assert!(!is_loopback_origin(origin), "{origin}");
    }
}

#[test]
fn cross_origin_guard_blocks_browser_attacks_but_not_local_clients() {
    let local = Some("127.0.0.1:7070");
    // curl / scripts: just a Host header.
    assert_eq!(cross_origin_violation(local, None, None), None);
    assert_eq!(cross_origin_violation(None, None, None), None);
    // The web UI itself.
    assert_eq!(
        cross_origin_violation(local, Some("http://127.0.0.1:7070"), Some("same-origin")),
        None
    );
    // Typing the address into the URL bar.
    assert_eq!(
        cross_origin_violation(Some("localhost:7070"), None, Some("none")),
        None
    );
    // A page on another site calling the API.
    assert!(cross_origin_violation(local, Some("https://evil.com"), Some("cross-site")).is_some());
    assert!(cross_origin_violation(local, Some("null"), None).is_some());
    // …including a GET that carries no Origin header (e.g. an <img> tag).
    assert!(cross_origin_violation(local, None, Some("cross-site")).is_some());
    // DNS rebinding: "same-origin" as far as the browser knows, wrong Host.
    assert!(cross_origin_violation(
        Some("evil.com:7070"),
        Some("http://evil.com:7070"),
        Some("same-origin")
    )
    .is_some());
    assert!(cross_origin_violation(Some("evil.com:7070"), None, None).is_some());
}

#[test]
fn bind_address_must_be_loopback() {
    assert!(require_loopback("127.0.0.1").is_ok());
    assert!(require_loopback("localhost").is_ok());
    assert!(require_loopback("::1").is_ok());
    for bad in ["0.0.0.0", "::", "192.168.1.5", "example.com", ""] {
        assert!(require_loopback(bad).is_err(), "{bad}");
    }
}

#[test]
fn secrets_compare_exactly() {
    assert!(constant_time_eq("s3cret", "s3cret"));
    assert!(constant_time_eq("", ""));
    assert!(!constant_time_eq("s3cret", "s3creT"));
    assert!(!constant_time_eq("s3cret", "s3cret "));
    assert!(!constant_time_eq("s3cret", ""));
}

#[test]
fn command_arguments_cannot_be_options_or_shell_syntax() {
    for host in ["example.com", "10.0.0.1", "fe80::1%eth0", "my_host-1"] {
        assert!(is_valid_ping_host(host), "{host}");
    }
    for host in [
        "",
        "-f",
        "--flood",
        "a b",
        "x;reboot",
        "$(id)",
        "a/b",
        &"a".repeat(254),
    ] {
        assert!(!is_valid_ping_host(host), "{host}");
    }
    for unit in [
        "nginx.service",
        "getty@tty1.service",
        "dev-sda.device",
        r"a\x2db.mount",
    ] {
        assert!(is_valid_unit_name(unit), "{unit}");
    }
    for unit in ["", "-H", "--host=evil", "a b.service", "x;id", "../etc"] {
        assert!(!is_valid_unit_name(unit), "{unit}");
    }
}

#[test]
fn every_listed_mode_parses_and_round_trips() {
    let mut seen = std::collections::HashSet::new();
    for &name in DisplayMode::ALL_NAMES {
        assert!(seen.insert(name), "{name} is listed twice");
        let mode =
            DisplayMode::from_str_name(name).unwrap_or_else(|| panic!("{name} does not parse"));
        assert_eq!(mode.as_str(), name);
    }
    assert!(DisplayMode::from_str_name("no_such_mode").is_none());
}

#[test]
fn pet_settings_ignore_values_they_do_not_understand() {
    let before = ttlcd::pet::config();
    PetSettings {
        coat: "purple".into(),
        environment: "moon".into(),
        background: "red".into(),
    }
    .apply();
    assert_eq!(ttlcd::pet::config(), before);
}

/// A scratch file that is removed when dropped.
struct TempFile(PathBuf);
impl TempFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("ttlcd-test-{}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        Self(path)
    }
    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn tail_returns_the_last_non_empty_lines_in_order() {
    let f = TempFile::new("tail", b"one\ntwo\n\nthree\nfour\n");
    assert_eq!(read_tail_lines(f.path(), 2).unwrap(), ["three", "four"]);
    assert_eq!(
        read_tail_lines(f.path(), 10).unwrap(),
        ["one", "two", "three", "four"]
    );
    assert!(read_tail_lines(f.path(), 0).unwrap().is_empty());
    // No trailing newline, and an empty file.
    let f = TempFile::new("tail-nonl", b"a\nb");
    assert_eq!(read_tail_lines(f.path(), 1).unwrap(), ["b"]);
    let f = TempFile::new("tail-empty", b"");
    assert!(read_tail_lines(f.path(), 5).unwrap().is_empty());
}

#[test]
fn tail_tolerates_invalid_utf8() {
    let f = TempFile::new("tail-bin", b"ok\n\xff\xfe broken\nlast\n");
    let lines = read_tail_lines(f.path(), 3).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!((lines[0].as_str(), lines[2].as_str()), ("ok", "last"));
}

#[test]
fn tail_reads_only_the_end_of_a_huge_file() {
    // 400 kB of 100-byte lines: more than MAX_TAIL_BYTES.
    let line = format!("{}\n", "x".repeat(99));
    let mut body = line.repeat(4000).into_bytes();
    body.extend_from_slice(b"the end\n");
    let f = TempFile::new("tail-big", &body);
    let all = read_tail_lines(f.path(), usize::MAX).unwrap();
    assert!(
        all.len() < 4000,
        "read the whole file ({} lines)",
        all.len()
    );
    assert_eq!(all.last().unwrap(), "the end");
    assert!(
        all.iter().all(|l| l == "the end" || l.len() == 99),
        "kept a partial line"
    );
    // A single enormous line cannot blow up memory either.
    let f = TempFile::new("tail-oneline", &vec![b'y'; 2 * MAX_TAIL_BYTES as usize]);
    let total: usize = read_tail_lines(f.path(), 5)
        .unwrap()
        .iter()
        .map(String::len)
        .sum();
    assert!(total <= MAX_TAIL_BYTES as usize);
}

#[test]
fn tail_refuses_things_that_are_not_regular_files() {
    assert!(read_tail_lines("/nonexistent/ttlcd.log", 5).is_err());
    assert!(read_tail_lines("/", 5).is_err());
    assert!(read_tail_lines("/dev/zero", 5).is_err());
}

#[test]
fn rss_and_atom_feeds_are_parsed() {
    let rss = r#"<rss><channel><title>My &amp; Feed</title>
        <item><title><![CDATA[First <b>post</b>]]></title><pubDate>Mon, 01 Jan 2026</pubDate></item>
        <item><title>Tom &amp;amp; Jerry &lt;3</title></item>
        </channel></rss>"#;
    let (title, items) = parse_rss_xml(rss);
    assert_eq!(title, "My & Feed");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].0, "First <b>post</b>");
    // Entities are decoded exactly once.
    assert_eq!(items[1].0, "Tom &amp; Jerry <3");

    let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Atom feed</title>
        <entry><title>Entry one</title><updated>2026-01-01T00:00:00Z</updated></entry></feed>"#;
    let (title, items) = parse_rss_xml(atom);
    assert_eq!(title, "Atom feed");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "Entry one");
}

#[test]
fn malformed_feeds_do_not_panic() {
    for junk in [
        "",
        "<rss>",
        "<item><title>unterminated",
        "<entry>",
        "not xml at all",
        "<item></item><item>",
    ] {
        let _ = parse_rss_xml(junk);
    }
    let (_, items) = parse_rss_xml("plain text");
    assert!(items.is_empty());
}
