//! Match the computer's RGB lighting to what the panel is showing.
//!
//! Two independent pieces: picking one representative colour from a frame
//! ([`dominant_color`], smoothed over time by [`Smoother`]), and sending a
//! colour to every device of a running [OpenRGB] server ([`OpenRgb`]).
//!
//! [OpenRGB]: https://openrgb.org

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use image::RgbImage;

pub type Rgb = [u8; 3];

// ─── Colour of a frame ───────────────────────────────────────────────────────

/// Sample every n-th pixel in each direction; plenty for an average.
const SAMPLE_STEP: usize = 4;
/// Below this mean weight per sample the frame counts as colourless.
const COLOURLESS_WEIGHT: f64 = 0.0005;
/// Dimmest white shown for a colourless frame, so the lights never go out.
const MIN_WHITE: f64 = 0.15;

/// One colour that represents a frame, suited to driving LEDs.
///
/// Pixels are weighted by how saturated and bright they are, so a small
/// vivid accent outweighs a large dark background, and the result is scaled
/// to full brightness: LEDs showing a faithful dark average would look off.
/// A frame with no colour to speak of gives white, dimmed with the frame.
pub fn dominant_color(img: &RgbImage) -> Rgb {
    let mut sum = [0.0f64; 3];
    let (mut weight_sum, mut value_sum, mut samples) = (0.0f64, 0.0f64, 0u32);
    for y in (0..img.height()).step_by(SAMPLE_STEP) {
        for x in (0..img.width()).step_by(SAMPLE_STEP) {
            let p = img.get_pixel(x, y).0;
            let max = f64::from(p[0].max(p[1]).max(p[2])) / 255.0;
            let min = f64::from(p[0].min(p[1]).min(p[2])) / 255.0;
            let chroma = max - min;
            let weight = chroma * chroma * max;
            for (total, channel) in sum.iter_mut().zip(p) {
                *total += weight * f64::from(channel);
            }
            weight_sum += weight;
            value_sum += max;
            samples += 1;
        }
    }
    if samples == 0 {
        return [0; 3];
    }
    let samples = f64::from(samples);
    if weight_sum / samples < COLOURLESS_WEIGHT {
        let white = ((value_sum / samples).max(MIN_WHITE) * 255.0).round() as u8;
        return [white; 3];
    }
    let mean = sum.map(|c| c / weight_sum);
    let brightest = mean[0].max(mean[1]).max(mean[2]).max(1.0);
    mean.map(|c| (c / brightest * 255.0).round() as u8)
}

/// Eases the light colour towards the frame colour so the lights drift
/// instead of flickering with every frame.
pub struct Smoother {
    current: [f64; 3],
    /// Seconds for the remaining difference to shrink to about a third.
    time_constant: f64,
    started: bool,
}

impl Smoother {
    pub fn new(time_constant: f64) -> Self {
        Self {
            current: [0.0; 3],
            time_constant,
            started: false,
        }
    }

    /// Move towards `target` for `dt` seconds and return the new colour.
    /// The first call jumps straight to the target.
    pub fn step(&mut self, target: Rgb, dt: f64) -> Rgb {
        let target = target.map(f64::from);
        if self.started {
            let blend = 1.0 - (-dt / self.time_constant).exp();
            for (c, t) in self.current.iter_mut().zip(target) {
                *c += (t - *c) * blend;
            }
        } else {
            self.current = target;
            self.started = true;
        }
        self.current.map(|c| c.round() as u8)
    }
}

/// Decides what colour the lights should aim for while following the screen.
///
/// Frame colours wobble (a flickering candle) and are briefly pulled away by
/// passing events (a ball crossing the scene); neither should reach the
/// lights. So the frame colour is averaged over a long window, and the
/// result only replaces the current aim once it has moved by more than a
/// dead band.
pub struct SceneColor {
    average: Smoother,
    time_constant: f64,
    dead_band: u8,
    aim: Option<Rgb>,
}

impl SceneColor {
    /// `time_constant` is the averaging window in seconds; `dead_band` is
    /// the per-channel change the average must exceed to move the aim.
    pub fn new(time_constant: f64, dead_band: u8) -> Self {
        Self {
            average: Smoother::new(time_constant),
            time_constant,
            dead_band,
            aim: None,
        }
    }

    /// Forget the scene, so the next frame's colour is adopted at once.
    /// For when the display switches to something unrelated.
    pub fn reset(&mut self) {
        *self = Self::new(self.time_constant, self.dead_band);
    }

    /// Take the colour of the frame shown for the last `dt` seconds and
    /// return the colour to aim for.
    pub fn observe(&mut self, frame: Rgb, dt: f64) -> Rgb {
        let average = self.average.step(frame, dt);
        match self.aim {
            Some(aim) if !differs(aim, average, self.dead_band) => aim,
            _ => {
                self.aim = Some(average);
                average
            }
        }
    }
}

/// Whether two colours differ enough to be worth sending again.
pub fn differs(a: Rgb, b: Rgb, threshold: u8) -> bool {
    a.iter().zip(b).any(|(x, y)| x.abs_diff(y) >= threshold)
}

// ─── OpenRGB client ──────────────────────────────────────────────────────────

/// OpenRGB's default SDK server address.
pub const DEFAULT_OPENRGB_ADDR: &str = "127.0.0.1:6742";

const MAGIC: &[u8; 4] = b"ORGB";
const HEADER_LEN: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Upper bound on a reply; a device description is a few kilobytes.
const MAX_PACKET: usize = 4 * 1024 * 1024;
/// OpenRGB sends one colour per LED as a 16-bit count.
const MAX_LEDS: usize = u16::MAX as usize;

// Packet ids from OpenRGB's NetworkProtocol.h. This client never negotiates
// a protocol version, so the server speaks version 0 to it.
const REQUEST_CONTROLLER_COUNT: u32 = 0;
const REQUEST_CONTROLLER_DATA: u32 = 1;
const SET_CLIENT_NAME: u32 = 50;
const RESIZE_ZONE: u32 = 1000;
const UPDATE_LEDS: u32 = 1050;
const SET_CUSTOM_MODE: u32 = 1100;
const UPDATE_MODE: u32 = 1101;

/// The mode in which a device shows exactly the colours it is sent.
const DIRECT_MODE: &str = "Direct";

/// A lighting device as reported by OpenRGB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub index: u32,
    pub name: String,
    pub leds: usize,
}

/// A connection to an OpenRGB server.
pub struct OpenRgb {
    stream: TcpStream,
    devices: Vec<Device>,
}

impl OpenRgb {
    /// Connect, list the devices and switch each to its directly controlled
    /// mode, which replaces whatever effect it was running.
    pub fn connect(addr: &str) -> Result<Self> {
        let addr: SocketAddr = addr
            .to_socket_addrs()
            .with_context(|| format!("invalid OpenRGB address '{addr}'"))?
            .next()
            .with_context(|| format!("OpenRGB address '{addr}' did not resolve"))?;
        let stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)
            .with_context(|| format!("cannot reach OpenRGB at {addr}"))?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        stream.set_nodelay(true)?;

        let mut client = Self {
            stream,
            devices: Vec::new(),
        };
        client.send(0, SET_CLIENT_NAME, b"ttlcd\0")?;
        client.send(0, REQUEST_CONTROLLER_COUNT, &[])?;
        let count = read_u32(&mut client.receive(REQUEST_CONTROLLER_COUNT)?.as_slice())?;
        for index in 0..count {
            client.send(index, REQUEST_CONTROLLER_DATA, &[])?;
            let data = client.receive(REQUEST_CONTROLLER_DATA)?;
            let controller = parse_controller(&data)
                .with_context(|| format!("cannot read OpenRGB device {index}"))?;
            if controller.leds > 0 {
                // Re-apply the size of every resizable zone. After a
                // restart OpenRGB remembers these sizes but has not told
                // the hardware, and an MSI motherboard then lights none of
                // its addressable headers until a resize is made. Going by
                // way of a neighbouring size guarantees the resize is a
                // real change and not skipped.
                for (zone, sizes) in controller.zones.iter().enumerate() {
                    for size in sizes.reapply() {
                        client.send(index, RESIZE_ZONE, &resize_zone_payload(zone as u32, size))?;
                    }
                }
                // Apply the Direct mode in full where the device has one,
                // so the hardware is told to change mode; "set custom
                // mode" is the fallback for devices that name it otherwise.
                match &controller.direct {
                    Some((mode, description)) => {
                        client.send(
                            index,
                            UPDATE_MODE,
                            &update_mode_payload(*mode, description),
                        )?;
                    }
                    None => client.send(index, SET_CUSTOM_MODE, &[])?,
                }
                let Controller { name, leds, .. } = controller;
                client.devices.push(Device { index, name, leds });
            }
        }
        Ok(client)
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Set every LED of every device to one colour.
    pub fn set_all(&mut self, color: Rgb) -> Result<()> {
        for i in 0..self.devices.len() {
            let (index, leds) = (self.devices[i].index, self.devices[i].leds);
            self.send(index, UPDATE_LEDS, &update_leds_payload(leds, color))?;
        }
        Ok(())
    }

    fn send(&mut self, device: u32, id: u32, payload: &[u8]) -> Result<()> {
        self.stream.write_all(&packet(device, id, payload))?;
        Ok(())
    }

    /// Read packets until one with the expected id arrives. The server may
    /// interleave notifications, such as "device list changed".
    fn receive(&mut self, expected: u32) -> Result<Vec<u8>> {
        loop {
            let mut header = [0u8; HEADER_LEN];
            self.stream.read_exact(&mut header)?;
            if &header[..4] != MAGIC {
                bail!("not an OpenRGB server (bad packet header)");
            }
            let id = u32::from_le_bytes(header[8..12].try_into().unwrap());
            let size = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
            if size > MAX_PACKET {
                bail!("OpenRGB sent an implausibly large packet ({size} bytes)");
            }
            let mut payload = vec![0u8; size];
            self.stream.read_exact(&mut payload)?;
            if id == expected {
                return Ok(payload);
            }
        }
    }
}

/// A complete packet: header followed by payload.
fn packet(device: u32, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&device.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Payload setting `leds` LEDs to one colour: total size, colour count, then
/// four bytes (red, green, blue, unused) per LED.
fn update_leds_payload(leds: usize, color: Rgb) -> Vec<u8> {
    let leds = leds.min(MAX_LEDS);
    let size = 4 + 2 + leds * 4;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&(leds as u16).to_le_bytes());
    for _ in 0..leds {
        out.extend_from_slice(&[color[0], color[1], color[2], 0]);
    }
    out
}

fn take<'a>(data: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if data.len() < n {
        bail!("device description is truncated");
    }
    let (head, rest) = data.split_at(n);
    *data = rest;
    Ok(head)
}

fn read_u16(data: &mut &[u8]) -> Result<u16> {
    Ok(u16::from_le_bytes(take(data, 2)?.try_into().unwrap()))
}

fn read_u32(data: &mut &[u8]) -> Result<u32> {
    Ok(u32::from_le_bytes(take(data, 4)?.try_into().unwrap()))
}

/// A length-prefixed, NUL-terminated string.
fn read_string(data: &mut &[u8]) -> Result<String> {
    let len = read_u16(data)? as usize;
    let bytes = take(data, len)?;
    let text = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    Ok(String::from_utf8_lossy(text).into_owned())
}

/// Payload applying a mode: total size, the mode's index, then the mode's
/// description exactly as the server sent it.
fn update_mode_payload(mode: u32, description: &[u8]) -> Vec<u8> {
    let size = 4 + 4 + description.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(description);
    out
}

/// Payload setting a zone's LED count: zone index, then the new size.
fn resize_zone_payload(zone: u32, size: u32) -> Vec<u8> {
    [zone.to_le_bytes(), size.to_le_bytes()].concat()
}

/// A zone's LED count and the range it may be resized within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZoneSize {
    min: u32,
    max: u32,
    count: u32,
}

impl ZoneSize {
    /// The sizes to set, in order, to re-apply the current size as a real
    /// change: a neighbouring size, then the current one. Empty for a zone
    /// that cannot be resized or has no LEDs.
    fn reapply(self) -> Vec<u32> {
        if self.min == self.max || self.count == 0 || self.count < self.min || self.count > self.max
        {
            return Vec::new();
        }
        let neighbour = if self.count > self.min {
            self.count - 1
        } else {
            self.count + 1
        };
        vec![neighbour, self.count]
    }
}

/// What this client needs to know about a device.
#[derive(Debug, PartialEq, Eq)]
struct Controller {
    name: String,
    leds: usize,
    zones: Vec<ZoneSize>,
    /// Index and raw description of the device's Direct mode, if it has one.
    direct: Option<(u32, Vec<u8>)>,
}

/// Read a device from a protocol-version-0 controller description. The
/// layout is: type, five strings, the modes, the zones, the LEDs, then the
/// current colours. Most of it is skipped, and the description must be
/// consumed exactly, so a layout this parser does not understand is an
/// error rather than a guess.
fn parse_controller(data: &[u8]) -> Result<Controller> {
    let mut d = data;
    let declared = read_u32(&mut d)? as usize;
    if declared != data.len() {
        bail!("device description length does not match its packet");
    }
    read_u32(&mut d)?; // device type
    let name = read_string(&mut d)?;
    for _ in 0..4 {
        read_string(&mut d)?; // description, version, serial, location
    }

    let modes = read_u16(&mut d)?;
    read_u32(&mut d)?; // active mode
    let mut direct = None;
    for index in 0..u32::from(modes) {
        let start = d;
        let mode_name = read_string(&mut d)?;
        // value, flags, speed min/max, colours min/max, speed, direction,
        // colour mode
        take(&mut d, 9 * 4)?;
        let colors = read_u16(&mut d)? as usize;
        take(&mut d, colors * 4)?;
        if mode_name == DIRECT_MODE && direct.is_none() {
            direct = Some((index, start[..start.len() - d.len()].to_vec()));
        }
    }

    let zone_count = read_u16(&mut d)?;
    let mut zones = Vec::new();
    for _ in 0..zone_count {
        read_string(&mut d)?;
        read_u32(&mut d)?; // zone type
        zones.push(ZoneSize {
            min: read_u32(&mut d)?,
            max: read_u32(&mut d)?,
            count: read_u32(&mut d)?,
        });
        let matrix_len = read_u16(&mut d)? as usize;
        take(&mut d, matrix_len)?;
    }

    let leds = read_u16(&mut d)? as usize;
    for _ in 0..leds {
        read_string(&mut d)?;
        read_u32(&mut d)?; // value
    }

    let colors = read_u16(&mut d)? as usize;
    take(&mut d, colors * 4)?;
    if !d.is_empty() {
        bail!(
            "device description has {} unexpected trailing bytes",
            d.len()
        );
    }
    Ok(Controller {
        name,
        leds,
        zones,
        direct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb as Pixel;

    fn solid(color: Rgb) -> RgbImage {
        RgbImage::from_pixel(64, 64, Pixel(color))
    }

    #[test]
    fn a_vivid_frame_gives_its_hue_at_full_brightness() {
        assert_eq!(dominant_color(&solid([200, 0, 0])), [255, 0, 0]);
        assert_eq!(dominant_color(&solid([0, 40, 80])), [0, 128, 255]);
    }

    #[test]
    fn a_small_accent_outweighs_a_dark_background() {
        // Dark, slightly blue background with a green block covering 4 %.
        let mut img = solid([30, 30, 46]);
        for y in 0..13 {
            for x in 0..13 {
                img.put_pixel(x, y, Pixel([40, 220, 60]));
            }
        }
        let [r, g, b] = dominant_color(&img);
        assert!(g == 255 && r < 90 && b < 110, "got {r},{g},{b}");
    }

    #[test]
    fn a_colourless_frame_gives_white_that_follows_its_brightness() {
        assert_eq!(dominant_color(&solid([255, 255, 255])), [255; 3]);
        assert_eq!(dominant_color(&solid([128, 128, 128])), [128; 3]);
        // Black still leaves the lights faintly on.
        assert_eq!(dominant_color(&solid([0, 0, 0])), [38; 3]);
        assert_eq!(dominant_color(&RgbImage::new(0, 0)), [0; 3]);
    }

    #[test]
    fn smoother_jumps_first_then_eases() {
        let mut s = Smoother::new(1.0);
        assert_eq!(s.step([100, 0, 0], 0.25), [100, 0, 0]);
        // After one time constant about 63 % of the way there.
        assert_eq!(s.step([200, 0, 0], 1.0), [163, 0, 0]);
        // Never overshoots, and converges.
        for _ in 0..40 {
            s.step([200, 0, 0], 0.25);
        }
        assert_eq!(s.step([200, 0, 0], 0.25), [200, 0, 0]);
    }

    /// Feed `frame` to `scene` four times a second for `secs` seconds and
    /// return every distinct aim it produced.
    fn watch(scene: &mut SceneColor, secs: u32, frame: impl Fn(u32) -> Rgb) -> Vec<Rgb> {
        let mut aims = Vec::new();
        for tick in 0..secs * 4 {
            let aim = scene.observe(frame(tick), 0.25);
            if aims.last() != Some(&aim) {
                aims.push(aim);
            }
        }
        aims
    }

    #[test]
    fn scene_colour_ignores_wobble_and_passing_events() {
        let mut scene = SceneColor::new(30.0, 24);
        let base = [196, 209, 255];
        // A minute of candle-like wobble: the aim never moves.
        let wobble = watch(&mut scene, 60, |t| {
            if t % 2 == 0 {
                base
            } else {
                [210, 217, 255]
            }
        });
        assert_eq!(wobble.len(), 1, "{wobble:?}");
        // Ten seconds pulled well towards pink, then back for a minute.
        let event = watch(&mut scene, 10, |_| [255, 219, 254]);
        let after = watch(&mut scene, 60, |_| base);
        assert_eq!((event, after), (vec![wobble[0]], vec![wobble[0]]));
    }

    #[test]
    fn scene_colour_follows_a_lasting_change_and_resets_at_once() {
        let mut scene = SceneColor::new(30.0, 24);
        watch(&mut scene, 10, |_| [0, 0, 255]);
        // A change that stays is followed, gradually, and gets there.
        let aims = watch(&mut scene, 300, |_| [255, 0, 0]);
        assert!(aims.len() > 3, "{aims:?}");
        let [r, _, b] = *aims.last().unwrap();
        assert!(r > 230 && b < 25, "{aims:?}");
        // After a reset the next frame's colour is adopted outright.
        scene.reset();
        assert_eq!(scene.observe([0, 255, 0], 0.25), [0, 255, 0]);
    }

    #[test]
    fn differs_uses_a_per_channel_threshold() {
        assert!(!differs([10, 10, 10], [11, 12, 10], 3));
        assert!(differs([10, 10, 10], [10, 13, 10], 3));
        assert!(differs([0, 0, 255], [0, 0, 0], 3));
    }

    #[test]
    fn packets_have_the_openrgb_header() {
        let p = packet(2, UPDATE_LEDS, &[9, 8]);
        assert_eq!(&p[..4], b"ORGB");
        assert_eq!(p[4..8], 2u32.to_le_bytes());
        assert_eq!(p[8..12], 1050u32.to_le_bytes());
        assert_eq!(p[12..16], 2u32.to_le_bytes());
        assert_eq!(&p[16..], [9, 8]);
    }

    #[test]
    fn led_update_lists_one_colour_per_led() {
        let p = update_leds_payload(2, [1, 2, 3]);
        assert_eq!(p[..4], 14u32.to_le_bytes());
        assert_eq!(p[4..6], 2u16.to_le_bytes());
        assert_eq!(&p[6..], [1, 2, 3, 0, 1, 2, 3, 0]);
        // The count field is 16 bits, so the LED count is capped to fit.
        assert_eq!(update_leds_payload(70_000, [0; 3]).len(), 6 + 65_535 * 4);
    }

    /// A version-0 device description with one mode, one zone and `leds` LEDs.
    fn controller_blob(name: &str, leds: u16) -> Vec<u8> {
        fn string(out: &mut Vec<u8>, s: &str) {
            out.extend_from_slice(&(s.len() as u16 + 1).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        let mut body = Vec::new();
        body.extend_from_slice(&1u32.to_le_bytes()); // type
        for s in [name, "description", "version", "serial", "location"] {
            string(&mut body, s);
        }
        body.extend_from_slice(&1u16.to_le_bytes()); // modes
        body.extend_from_slice(&0u32.to_le_bytes()); // active mode
        string(&mut body, "Direct");
        body.extend_from_slice(&[0; 36]);
        body.extend_from_slice(&1u16.to_le_bytes()); // one mode colour
        body.extend_from_slice(&[0; 4]);
        body.extend_from_slice(&1u16.to_le_bytes()); // zones
        string(&mut body, "Zone");
        body.extend_from_slice(&1u32.to_le_bytes()); // zone type
        body.extend_from_slice(&0u32.to_le_bytes()); // fewest LEDs
        body.extend_from_slice(&200u32.to_le_bytes()); // most LEDs
        body.extend_from_slice(&u32::from(leds).to_le_bytes());
        body.extend_from_slice(&3u16.to_le_bytes()); // matrix bytes
        body.extend_from_slice(&[0; 3]);
        body.extend_from_slice(&leds.to_le_bytes());
        for i in 0..leds {
            string(&mut body, &format!("LED {i}"));
            body.extend_from_slice(&[0; 4]);
        }
        body.extend_from_slice(&leds.to_le_bytes()); // colours
        body.extend_from_slice(&vec![0; leds as usize * 4]);

        let mut blob = (body.len() as u32 + 4).to_le_bytes().to_vec();
        blob.extend_from_slice(&body);
        blob
    }

    #[test]
    fn controller_description_yields_name_and_led_count() {
        let blob = controller_blob("Corsair Vengeance RGB DDR5", 10);
        let controller = parse_controller(&blob).unwrap();
        assert_eq!(controller.name, "Corsair Vengeance RGB DDR5");
        assert_eq!(controller.leds, 10);
        assert_eq!(parse_controller(&controller_blob("x", 0)).unwrap().leds, 0);

        // The blob's only mode is "Direct": its index and its exact bytes
        // (name, nine fields, one colour) are kept for applying it.
        let (index, description) = controller.direct.unwrap();
        assert_eq!(index, 0);
        assert_eq!(description.len(), (2 + 7) + 36 + (2 + 4));
        assert_eq!(&description[2..8], b"Direct");
        assert!(blob.windows(description.len()).any(|w| w == description));

        // Applying it sends size, index, then those bytes unchanged.
        let payload = update_mode_payload(index, &description);
        assert_eq!(payload[..4], (payload.len() as u32).to_le_bytes());
        assert_eq!(payload[4..8], 0u32.to_le_bytes());
        assert_eq!(payload[8..], description[..]);

        // A device without a Direct mode has none to apply.
        let other = blob.windows(6).position(|w| w == b"Direct").unwrap();
        let mut renamed = blob.clone();
        renamed[other..other + 6].copy_from_slice(b"Static");
        assert_eq!(parse_controller(&renamed).unwrap().direct, None);
    }

    #[test]
    fn resizable_zones_are_reapplied_by_way_of_a_neighbouring_size() {
        let zones = parse_controller(&controller_blob("Fans", 60))
            .unwrap()
            .zones;
        assert_eq!(
            zones,
            [ZoneSize {
                min: 0,
                max: 200,
                count: 60
            }]
        );
        assert_eq!(zones[0].reapply(), [59, 60]);
        // At the bottom of its range the neighbour is the size above.
        assert_eq!(
            ZoneSize {
                min: 12,
                max: 200,
                count: 12
            }
            .reapply(),
            [13, 12]
        );
        // Fixed-size, empty and out-of-range zones are left alone.
        assert!(ZoneSize {
            min: 10,
            max: 10,
            count: 10
        }
        .reapply()
        .is_empty());
        assert!(ZoneSize {
            min: 0,
            max: 200,
            count: 0
        }
        .reapply()
        .is_empty());
        assert!(ZoneSize {
            min: 0,
            max: 50,
            count: 60
        }
        .reapply()
        .is_empty());

        assert_eq!(resize_zone_payload(3, 60), [3, 0, 0, 0, 60, 0, 0, 0]);
    }

    #[test]
    fn malformed_controller_descriptions_are_rejected() {
        let blob = controller_blob("Fans", 4);
        // Truncated anywhere, or with trailing bytes, it must not parse.
        for cut in [0, 3, 10, blob.len() / 2, blob.len() - 1] {
            assert!(parse_controller(&blob[..cut]).is_err(), "cut at {cut}");
        }
        let mut longer = blob.clone();
        longer.push(0);
        assert!(parse_controller(&longer).is_err());
        // A length field that lies about the size is caught too.
        let mut lying = blob;
        lying[0] = lying[0].wrapping_add(1);
        assert!(parse_controller(&lying).is_err());
    }

    #[test]
    #[ignore = "needs a running OpenRGB server on the default port"]
    fn live_server_lists_devices() {
        let client = OpenRgb::connect(DEFAULT_OPENRGB_ADDR).unwrap();
        println!("{:#?}", client.devices());
        assert!(!client.devices().is_empty());
    }

    #[test]
    fn connecting_to_nothing_fails_with_the_address() {
        // Port 1 on loopback: nothing listens there.
        let err = OpenRgb::connect("127.0.0.1:1").err().unwrap().to_string();
        assert!(err.contains("127.0.0.1:1"), "{err}");
        assert!(OpenRgb::connect("not an address").is_err());
    }
}
