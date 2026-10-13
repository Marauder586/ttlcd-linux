//! Keeps the computer's RGB lighting in step with the panel, through OpenRGB.

use super::*;

use ttlcd::lights::{differs, dominant_color, OpenRgb, Rgb, SceneColor, Smoother};

/// How often the lights are reconsidered.
const TICK: Duration = Duration::from_millis(250);
/// Seconds for the lights to close about two thirds of the gap to the colour
/// they are aiming for.
const SMOOTHING_SECS: f64 = 2.0;
/// Seconds of frames averaged to judge a scene's colour. Long, so that an
/// event lasting ten seconds or so barely moves it.
const SCENE_SECS: f64 = 30.0;
/// Per-channel change in the scene's colour needed before the lights move.
const SCENE_DEAD_BAND: u8 = 24;
/// Ticks to wait after a mode switch before reading the frame colour, so
/// the frame read is one the new mode drew.
const MODE_SETTLE_TICKS: u8 = 2;
/// Smallest per-channel change worth sending to the devices.
const SEND_THRESHOLD: u8 = 3;
/// Wait between attempts to reach OpenRGB while it is unavailable.
const RETRY: Duration = Duration::from_secs(5);
/// How often an unchanged colour is sent again. A device can lose its
/// colour without any error (a write that arrives while it is still
/// switching mode, a reset on resume), and a steady colour would otherwise
/// never be written a second time.
const REFRESH: Duration = Duration::from_secs(60);
/// Just after connecting the colour is repeated this many times, this far
/// apart, because devices are told to change mode at that moment and may
/// drop a write that lands during the change.
const SETTLE_REPEATS: u8 = 3;
const SETTLE_INTERVAL: Duration = Duration::from_secs(1);

/// Decides when a colour that has not changed should be sent again.
pub(crate) struct Resend {
    next: Instant,
    settle_left: u8,
}

impl Resend {
    /// Start the schedule for a connection made at `now`.
    pub(crate) fn connected(now: Instant) -> Self {
        Self {
            next: now + SETTLE_INTERVAL,
            settle_left: SETTLE_REPEATS,
        }
    }

    /// Whether a repeat is due at `now`; if so, the next one is scheduled.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.settle_left = self.settle_left.saturating_sub(1);
        self.next = now
            + if self.settle_left > 0 {
                SETTLE_INTERVAL
            } else {
                REFRESH
            };
        true
    }
}

/// Light-sync settings and what the worker last observed.
pub(crate) struct LightSync {
    pub(crate) enabled: bool,
    /// A colour to hold instead of following the screen.
    pub(crate) fixed: Option<Rgb>,
    /// OpenRGB server address, fixed at start-up.
    pub(crate) addr: String,
    /// Names of the devices being driven; empty while disconnected.
    pub(crate) devices: Vec<String>,
    /// The colour last sent.
    pub(crate) color: Option<Rgb>,
    /// Why OpenRGB could not be used, if it could not.
    pub(crate) error: Option<String>,
}

impl LightSync {
    pub(crate) fn new(enabled: bool, fixed: Option<Rgb>, addr: String) -> Self {
        Self {
            enabled,
            fixed,
            addr,
            devices: Vec::new(),
            color: None,
            error: None,
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "fixed_color": self.fixed.map(ttlcd::pet::format_hex_color),
            "address": self.addr,
            "connected": !self.devices.is_empty(),
            "devices": self.devices,
            "color": self.color.map(ttlcd::pet::format_hex_color),
            "error": self.error,
        })
    }
}

/// Background worker: while enabled, show the fixed colour if one is set,
/// otherwise follow the colour of the scene on the panel, steadily: brief
/// events and flicker in the picture do not reach the lights.
/// Problems reaching OpenRGB are recorded for the UI and retried; they never
/// affect the panel.
pub(crate) fn light_sync_loop(state: DaemonState) {
    let mut client: Option<OpenRgb> = None;
    let mut smoother = Smoother::new(SMOOTHING_SECS);
    let mut scene = SceneColor::new(SCENE_SECS, SCENE_DEAD_BAND);
    let mut frame_color: Option<Rgb> = None;
    let mut sent: Option<Rgb> = None;
    let mut seen_seq = None;
    let mut last_mode = None;
    let mut settle = 0;
    let mut resend = Resend::connected(Instant::now());
    let mut next_attempt = Instant::now();

    loop {
        thread::sleep(TICK);
        let (enabled, fixed, addr) = {
            let sync = state.lights.lock().unwrap();
            (sync.enabled, sync.fixed, sync.addr.clone())
        };

        if !enabled {
            // Dropping the connection leaves the lights on their last colour.
            if client.take().is_some() || sent.is_some() {
                let mut sync = state.lights.lock().unwrap();
                sync.devices.clear();
                sync.color = None;
                sync.error = None;
            }
            (smoother, sent, last_mode) = (Smoother::new(SMOOTHING_SECS), None, None);
            next_attempt = Instant::now();
            continue;
        }

        if client.is_none() {
            if Instant::now() < next_attempt {
                continue;
            }
            match OpenRgb::connect(&addr) {
                Ok(connected) => {
                    let names: Vec<String> =
                        connected.devices().iter().map(|d| d.name.clone()).collect();
                    log::info!("light sync: driving {} OpenRGB device(s)", names.len());
                    let mut sync = state.lights.lock().unwrap();
                    sync.error = names
                        .is_empty()
                        .then(|| "OpenRGB reports no lighting devices".to_string());
                    sync.devices = names;
                    client = Some(connected);
                    sent = None;
                    resend = Resend::connected(Instant::now());
                }
                Err(e) => {
                    log::debug!("light sync: {e:#}");
                    state.lights.lock().unwrap().error = Some(format!("{e:#}"));
                    next_attempt = Instant::now() + RETRY;
                    continue;
                }
            }
        }

        let target = match fixed {
            Some(color) => color,
            // Without a fixed colour, aim for the colour of the scene.
            None => {
                // A new mode is a new scene: take its colour at once rather
                // than averaging it in slowly.
                let mode = state.mode.lock().unwrap().as_str();
                if last_mode != Some(mode) {
                    last_mode = Some(mode);
                    scene.reset();
                    (frame_color, seen_seq) = (None, None);
                    settle = MODE_SETTLE_TICKS;
                }
                if settle > 0 {
                    settle -= 1;
                    continue;
                }
                // Decode only when the panel has a new frame.
                let seq = *state.frame_seq.lock().unwrap();
                if seen_seq != Some(seq) {
                    seen_seq = Some(seq);
                    let frame = state.last_frame.lock().unwrap().clone();
                    if let Some(img) = frame.and_then(|jpeg| ttlcd::decode_image(&jpeg).ok()) {
                        frame_color = Some(dominant_color(&img.to_rgb8()));
                    }
                }
                let Some(frame_color) = frame_color else {
                    continue;
                };
                scene.observe(frame_color, TICK.as_secs_f64())
            }
        };

        let color = smoother.step(target, TICK.as_secs_f64());
        let unchanged = sent.is_some_and(|s| !differs(s, color, SEND_THRESHOLD));
        if unchanged && !resend.due(Instant::now()) {
            continue;
        }
        let Some(connected) = client.as_mut() else {
            continue;
        };
        match connected.set_all(color) {
            Ok(()) => {
                sent = Some(color);
                state.lights.lock().unwrap().color = Some(color);
            }
            Err(e) => {
                log::warn!("light sync: lost OpenRGB: {e:#}");
                let mut sync = state.lights.lock().unwrap();
                sync.devices.clear();
                sync.error = Some(format!("{e:#}"));
                client = None;
                next_attempt = Instant::now() + RETRY;
            }
        }
    }
}

/// GET /api/lights — whether light sync is on, and what it is driving.
pub(crate) async fn api_get_lights(state: web::Data<DaemonState>) -> impl Responder {
    HttpResponse::Ok().json(state.lights.lock().unwrap().json())
}

#[derive(Deserialize)]
pub(crate) struct LightsUpdate {
    enabled: Option<bool>,
    /// `#rrggbb` to hold that colour, or `auto` to follow the screen.
    color: Option<String>,
}

/// The word that selects following the screen in place of a colour.
const FOLLOW_SCREEN: &str = "auto";

/// POST /api/lights — change either setting or both:
/// `{"enabled": true, "color": "#ff8800"}`. `"color": "auto"` goes back to
/// following the screen. A bad colour is rejected and nothing is changed.
pub(crate) async fn api_set_lights(
    state: web::Data<DaemonState>,
    body: web::Json<LightsUpdate>,
) -> impl Responder {
    let fixed = match body.color.as_deref() {
        None => None,
        Some(FOLLOW_SCREEN) => Some(None),
        Some(text) => match ttlcd::pet::parse_hex_color(text) {
            Some(color) => Some(Some(color)),
            None => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "color must be #rrggbb or \"auto\"",
                }));
            }
        },
    };
    let json = {
        let mut sync = state.lights.lock().unwrap();
        if let Some(enabled) = body.enabled {
            sync.enabled = enabled;
        }
        if let Some(fixed) = fixed {
            sync.fixed = fixed;
        }
        if !sync.enabled {
            sync.error = None;
        }
        sync.json()
    };
    persist_state(&state);
    HttpResponse::Ok().json(json)
}
