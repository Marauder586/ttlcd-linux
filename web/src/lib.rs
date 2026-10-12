//! The ttlcd web UI.
//!
//! This crate compiles to WebAssembly and drives the page in
//! `src/web/index.html`: it wires up the controls written there, builds the
//! remaining cards, and keeps everything in step with the daemon's HTTP API.
//!
//! Build it with `scripts/build-web.sh`; the daemon embeds the result.

mod api;
mod core;
mod dom;
mod js;
mod layout;
mod misc;
mod modes;
mod monitors;
mod push;
mod sections;
mod system;
pub mod util;

use wasm_bindgen::prelude::*;

/// Runs once the module has loaded, which is after the document is parsed.
#[wasm_bindgen(start)]
pub fn start() {
    core::init_theme();
    core::init_return_button();
    core::init_conn_dot();

    // Order matters from here: several cards are positioned relative to
    // cards created by an earlier step.
    push::init();
    system::init();
    sections::init();
    monitors::init();
    modes::init();
    misc::init();
    // Last, once every card exists.
    layout::init();

    core::init_sse();
    start_polling();
    core::log("ttlcd web UI connected");
}

/// Reload everything a settings import can change.
pub(crate) async fn refresh_settings() {
    core::fetch_status().await;
    push::fetch_presets().await;
    sections::fetch_profiles().await;
    sections::fetch_thresholds().await;
    sections::fetch_dim_schedule().await;
    sections::fetch_cached_notes().await;
    sections::fetch_metrics().await;
}

/// Load every card once, then keep the live ones fresh. Intervals are in
/// milliseconds.
fn start_polling() {
    // Loaded once at start-up; these are not polled.
    dom::spawn(system::fetch_panel_info());
    dom::spawn(push::fetch_presets());
    dom::spawn(sections::fetch_thresholds());
    dom::spawn(sections::fetch_dim_schedule());
    dom::spawn(sections::fetch_metrics());
    dom::spawn(sections::fetch_profiles());
    dom::spawn(sections::fetch_cached_weather());
    dom::spawn(sections::fetch_cached_notes());
    dom::spawn(sections::fetch_webhook_config());
    dom::spawn(sections::fetch_qr_state());
    dom::spawn(sections::fetch_mode_schedule());
    dom::spawn(sections::fetch_world_clock());

    dom::poll(3_000, core::fetch_status);
    dom::poll(5_000, system::fetch_snapshot);
    dom::poll(5_000, system::fetch_fps_history);
    dom::poll(10_000, system::update_net_sparkline);
    dom::poll(30_000, push::fetch_history);
    dom::poll(2_000, sections::fetch_stopwatch);
    dom::poll(3_000, sections::fetch_pomodoro);
    dom::poll(35_000, monitors::fetch_http_status);
    dom::poll(8_000, monitors::fetch_shell_cmd);
    dom::poll(15_000, monitors::fetch_ping);
    dom::poll(8_000, monitors::fetch_log_tail);
    dom::poll(30_000, monitors::fetch_rss);
    dom::poll(15_000, monitors::fetch_services);
}
