//! Keeps the computer's RGB lighting in step with the panel, through OpenRGB.

use super::*;

use ttlcd::lights::{differs, dominant_color, OpenRgb, Rgb, Smoother};

/// How often the lights are reconsidered.
const TICK: Duration = Duration::from_millis(250);
/// Seconds for the lights to close about two thirds of the gap to a new
/// frame colour.
const SMOOTHING_SECS: f64 = 1.5;
/// Smallest per-channel change worth sending to the devices.
const SEND_THRESHOLD: u8 = 3;
/// Wait between attempts to reach OpenRGB while it is unavailable.
const RETRY: Duration = Duration::from_secs(5);

/// Light-sync settings and what the worker last observed.
pub(crate) struct LightSync {
    pub(crate) enabled: bool,
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
    pub(crate) fn new(enabled: bool, addr: String) -> Self {
        Self {
            enabled,
            addr,
            devices: Vec::new(),
            color: None,
            error: None,
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "address": self.addr,
            "connected": !self.devices.is_empty(),
            "devices": self.devices,
            "color": self.color.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")),
            "error": self.error,
        })
    }
}

/// Background worker: while enabled, follow the colour of the latest frame.
/// Problems reaching OpenRGB are recorded for the UI and retried; they never
/// affect the panel.
pub(crate) fn light_sync_loop(state: DaemonState) {
    let mut client: Option<OpenRgb> = None;
    let mut smoother = Smoother::new(SMOOTHING_SECS);
    let mut target: Option<Rgb> = None;
    let mut sent: Option<Rgb> = None;
    let mut seen_seq = None;
    let mut next_attempt = Instant::now();

    loop {
        thread::sleep(TICK);
        let (enabled, addr) = {
            let sync = state.lights.lock().unwrap();
            (sync.enabled, sync.addr.clone())
        };

        if !enabled {
            // Dropping the connection leaves the lights on their last colour.
            if client.take().is_some() || sent.is_some() {
                let mut sync = state.lights.lock().unwrap();
                sync.devices.clear();
                sync.color = None;
                sync.error = None;
            }
            (smoother, target, sent, seen_seq) = (Smoother::new(SMOOTHING_SECS), None, None, None);
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
                }
                Err(e) => {
                    log::debug!("light sync: {e:#}");
                    state.lights.lock().unwrap().error = Some(format!("{e:#}"));
                    next_attempt = Instant::now() + RETRY;
                    continue;
                }
            }
        }

        // Re-read the colour only when the panel has a new frame.
        let seq = *state.frame_seq.lock().unwrap();
        if seen_seq != Some(seq) {
            seen_seq = Some(seq);
            let frame = state.last_frame.lock().unwrap().clone();
            if let Some(img) = frame.and_then(|jpeg| ttlcd::decode_image(&jpeg).ok()) {
                target = Some(dominant_color(&img.to_rgb8()));
            }
        }
        let Some(target) = target else { continue };

        let color = smoother.step(target, TICK.as_secs_f64());
        if sent.is_some_and(|s| !differs(s, color, SEND_THRESHOLD)) {
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
    enabled: bool,
}

/// POST /api/lights — turn light sync on or off: `{"enabled": true}`.
pub(crate) async fn api_set_lights(
    state: web::Data<DaemonState>,
    body: web::Json<LightsUpdate>,
) -> impl Responder {
    let json = {
        let mut sync = state.lights.lock().unwrap();
        sync.enabled = body.enabled;
        if !sync.enabled {
            sync.error = None;
        }
        sync.json()
    };
    persist_state(&state);
    HttpResponse::Ok().json(json)
}
