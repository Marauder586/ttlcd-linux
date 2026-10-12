//! High-level driver for the XinWeiYe-OEM USB LCD panel
//! (Thermaltake `0x264a:0x2343`, "3.95 square TFT LCD Display Module", 480×480).
//!
//! Talks to the panel's two HID interfaces as raw USB endpoints.  The kernel
//! `usbhid` driver is detached from both interfaces before claiming (and
//! re-attached on [`Drop`]).

pub mod display;
pub mod pet;
pub mod proto;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageReader};
use rusb::{DeviceHandle, GlobalContext};

use crate::proto::*;

/// Panel native resolution (480×480 for `0x2343`).
pub const PANEL_WIDTH: u32 = 480;
pub const PANEL_HEIGHT: u32 = 480;

/// Default JPEG quality used when encoding frames.
pub const DEFAULT_JPEG_QUALITY: u8 = 85;

/// USB transfer timeout for both control and image transfers.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_millis(3000);

/// Largest JPEG that fits in one frame (255 packets of 1020 payload bytes).
pub const MAX_JPEG_BYTES: usize = u8::MAX as usize * IMG_PAYLOAD_SIZE;

/// Size [`encode_for_panel`] aims to stay under (100 packets).
pub const TARGET_JPEG_BYTES: usize = 100 * IMG_PAYLOAD_SIZE;

/// Lowest JPEG quality [`encode_for_panel`] will fall back to.
pub const MIN_JPEG_QUALITY: u8 = 10;

/// Largest width or height accepted when decoding an image from a file,
/// upload or URL.
pub const MAX_DECODE_DIMENSION: u32 = 8192;

/// Most memory a single image decode may allocate.
pub const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

/// How long to wait for the per-frame ACK on the image IN endpoint.  It
/// normally arrives within ~10 ms of the last packet.  With very busy
/// (near-noise) frames about one in seventy is acknowledged late or not at
/// all — cause unknown; ordinary frames have not been seen to miss — so this
/// is kept short to make that a brief hitch rather than a stall.
pub const IMG_ACK_TIMEOUT: Duration = Duration::from_millis(100);

/// How often a static frame must be re-sent to stay on screen.  The panel
/// reverts to its built-in screen ~2 s after the last frame it received.
pub const FRAME_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

// ─── Device-info parsed from the init ACK ────────────────────────────────────

/// Information extracted from the device's initialization ACK packets.
#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    /// Serial number (ASCII bytes 4.. of the 0x85 ACK response).
    pub serial: String,
    /// Firmware byte from the 0x87 response (byte 4).
    pub firmware_byte: u8,
    /// Status byte from the 0x84 response (byte 4).
    pub status_byte: u8,
}

// ─── Panel ───────────────────────────────────────────────────────────────────

pub struct Panel {
    link: Arc<Link>,
    pub info: DeviceInfo,
    /// Background thread that re-sends the last frame so it stays on screen.
    refresher: Option<JoinHandle<()>>,
}

/// USB state shared between [`Panel`] and its refresher thread.
struct Link {
    handle: DeviceHandle<GlobalContext>,
    /// Serialises whole control packets / whole frames on the wire.  The
    /// value is whether the `0x12` stream-begin prelude and first frame are out.
    io: Mutex<bool>,
    /// Last JPEG sent and when it went out.
    last_frame: Mutex<Option<(Arc<Vec<u8>>, Instant)>>,
    stop: AtomicBool,
}

impl Panel {
    /// Open the first matching device, claim both interfaces, and return a
    /// `Panel` ready for [`Panel::init`].
    ///
    /// Robustness policy:
    /// - `set_active_configuration(1)` failure is logged as a warning and
    ///   execution continues — the device is almost always already in config 1.
    /// - Kernel-driver detach errors are logged but non-fatal.
    pub fn open(vid: u16, pid: u16) -> Result<Self> {
        Self::open_inner(vid, pid)
    }

    /// Like [`Panel::open`] but attempts a USB driver unbind/rebind via sysfs
    /// to clear stale kernel driver state before opening.
    ///
    /// This is safer than a full USB bus reset in VM environments (the bus reset
    /// can disconnect the device entirely in QEMU passthrough).  If the sysfs
    /// approach is unavailable (no root, no sysfs path found), it falls back to
    /// a normal open.
    ///
    /// Use this to recover from a previous abrupt termination that left the
    /// device's endpoint state inconsistent.
    pub fn open_with_reset(vid: u16, pid: u16) -> Result<Self> {
        log::info!("open_with_reset: attempting sysfs driver rebind");
        Self::sysfs_rebind(vid, pid);
        // Short pause for re-enumeration.
        std::thread::sleep(Duration::from_millis(800));
        Self::open_inner(vid, pid)
    }

    /// Attempt to rebind the USB device via /sys/bus/usb.
    /// Requires that the calling process has write access to the sysfs paths
    /// (typically requires CAP_SYS_ADMIN / root).  Silently ignores failures.
    fn sysfs_rebind(vid: u16, pid: u16) {
        use std::io::Write;

        // Find the sysfs port path for this VID:PID.
        let target_vid = format!("{vid:04x}");
        let target_pid = format!("{pid:04x}");

        let port_path: Option<String> = (|| {
            let dir = std::fs::read_dir("/sys/bus/usb/devices").ok()?;
            for entry in dir.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                // Only look at port entries (e.g. "1-6"), not interface entries.
                if name.contains(':') {
                    continue;
                }
                let vid_path = p.join("idVendor");
                let pid_path = p.join("idProduct");
                if std::fs::read_to_string(&vid_path).ok()?.trim() == target_vid
                    && std::fs::read_to_string(&pid_path).ok()?.trim() == target_pid
                {
                    return Some(name);
                }
            }
            None
        })();

        let Some(port) = port_path else {
            log::warn!("sysfs_rebind: could not find sysfs port for {vid:04x}:{pid:04x}");
            return;
        };

        log::debug!("sysfs_rebind: found port {port}");
        let unbind = "/sys/bus/usb/drivers/usb/unbind";
        let bind = "/sys/bus/usb/drivers/usb/bind";

        match std::fs::OpenOptions::new()
            .write(true)
            .open(unbind)
            .and_then(|mut f| f.write_all(port.as_bytes()))
        {
            Ok(()) => {
                log::debug!("sysfs_rebind: unbound {port}");
                std::thread::sleep(Duration::from_millis(300));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .open(bind)
                    .and_then(|mut f| f.write_all(port.as_bytes()))
                {
                    Ok(()) => log::debug!("sysfs_rebind: rebound {port}"),
                    Err(e) => log::warn!("sysfs_rebind bind failed: {e}"),
                }
            }
            Err(e) => log::warn!("sysfs_rebind unbind failed (likely not root): {e}"),
        }
    }

    fn open_inner(vid: u16, pid: u16) -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(vid, pid).ok_or_else(|| {
            anyhow!(
                "USB device {vid:04x}:{pid:04x} not found.\n\
                 Hint: ensure the device is connected and you have read/write \
                 permission on /dev/bus/usb (udev rule or run as root)."
            )
        })?;

        // Set configuration 1.  The device is almost always already in config 1,
        // and attempting to change an already-correct config can fail on some
        // platforms/states — so we only warn on failure.
        let cur_cfg = handle.active_configuration().unwrap_or(0);
        if cur_cfg != 1 {
            match handle.set_active_configuration(1) {
                Ok(()) => log::debug!("set configuration 1 (was {cur_cfg})"),
                Err(e) => log::warn!(
                    "set_active_configuration(1) failed ({e}); current config={cur_cfg}; continuing"
                ),
            }
        }

        // Detach usbhid kernel driver if bound, then claim both interfaces.
        for iface in [IF_CONTROL, IF_IMAGE] {
            match handle.kernel_driver_active(iface) {
                Ok(true) => {
                    if let Err(e) = handle.detach_kernel_driver(iface) {
                        log::warn!("detach_kernel_driver(if={iface}): {e}");
                    } else {
                        log::debug!("detached kernel driver from interface {iface}");
                    }
                }
                Ok(false) => {}
                Err(e) => log::warn!("kernel_driver_active(if={iface}): {e}"),
            }
            handle
                .claim_interface(iface)
                .with_context(|| format!("claim_interface(if={iface})"))?;
        }

        let link = Arc::new(Link {
            handle,
            io: Mutex::new(false),
            last_frame: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let refresher = {
            let link = Arc::clone(&link);
            std::thread::Builder::new()
                .name("ttlcd-refresh".into())
                .spawn(move || link.refresh_loop())
                .context("spawn refresher thread")?
        };

        Ok(Self {
            link,
            info: DeviceInfo::default(),
            refresher: Some(refresher),
        })
    }

    /// Run the 6-step init handshake on the control interface, reading the
    /// ACK after each opcode and parsing device info from the replies.
    ///
    /// Fails if any ACK does not arrive within [`TRANSFER_TIMEOUT`]; for
    /// firmware that never ACKs, use [`Panel::init_no_ack`] instead.
    pub fn init(&mut self) -> Result<()> {
        log::info!("init: running {}-step handshake", INIT_OPCODES.len());
        for (i, &opcode) in INIT_OPCODES.iter().enumerate() {
            self.write_ctrl(&build_ctrl(opcode))
                .with_context(|| format!("init write opcode 0x{opcode:02x}"))?;
            let ack = self
                .read_ctrl()
                .with_context(|| format!("init read ack for opcode 0x{opcode:02x}"))?;
            log::debug!(
                "init[{}]: 0x{:02x} → ack {} bytes, head={:02x?}",
                i,
                opcode,
                ack.len(),
                &ack[..ack.len().min(16)]
            );

            // Parse device info from the well-known response opcodes.
            if opcode == 0x85 && self.info.serial.is_empty() && ack.len() > 4 {
                let raw = &ack[4..];
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                self.info.serial = String::from_utf8_lossy(&raw[..end]).to_string();
                log::info!("device serial: {}", self.info.serial);
            }
            // Only log firmware/status on first occurrence.
            if opcode == 0x87 && self.info.firmware_byte == 0 && ack.len() > 4 {
                self.info.firmware_byte = ack[4];
                log::info!("firmware byte: 0x{:02x}", self.info.firmware_byte);
            }
            if opcode == 0x84 && ack.len() > 4 {
                self.info.status_byte = ack[4];
                log::info!("status byte: 0x{:02x}", self.info.status_byte);
            }
        }
        log::info!(
            "init OK — serial={} firmware=0x{:02x}",
            self.info.serial,
            self.info.firmware_byte
        );
        Ok(())
    }

    /// Variant of [`Panel::init`] that does NOT read ACKs, so
    /// [`Panel::info`] stays empty.  For firmware variants that don't reply.
    pub fn init_no_ack(&self) -> Result<()> {
        log::info!("init_no_ack: writing {} opcodes", INIT_OPCODES.len());
        for &opcode in INIT_OPCODES {
            self.write_ctrl(&build_ctrl(opcode))
                .with_context(|| format!("init write opcode 0x{opcode:02x}"))?;
        }
        Ok(())
    }

    /// Send a single JPEG frame on the image interface and wait for the
    /// panel's ACK.
    ///
    /// The first call after opening also sends the `0x12` stream-begin prelude
    /// on the control interface, and sends the frame twice: the panel discards
    /// the first image it receives after the handshake.
    ///
    /// The panel only shows a frame for ~2 s, so a background thread re-sends
    /// the most recent one every [`FRAME_REFRESH_INTERVAL`] until the next
    /// frame arrives or the `Panel` is dropped.
    pub fn send_jpeg(&self, jpeg: &[u8]) -> Result<()> {
        self.send_jpeg_with_delay(jpeg, 0)
    }

    /// Encode `img` to a panel-sized JPEG and send it via [`Panel::send_jpeg`].
    pub fn send_image(&self, img: &DynamicImage, quality: u8) -> Result<()> {
        let jpeg = encode_for_panel(img, quality)?;
        self.send_jpeg(&jpeg)
    }

    /// Like [`Panel::send_jpeg`] but inserts `packet_delay_ms` between image
    /// packets (useful when debugging bandwidth / protocol issues).
    pub fn send_jpeg_with_delay(&self, jpeg: &[u8], packet_delay_ms: u64) -> Result<()> {
        if jpeg.is_empty() {
            bail!("empty JPEG buffer");
        }
        if jpeg.len().div_ceil(IMG_PAYLOAD_SIZE) > u8::MAX as usize {
            bail!(
                "JPEG too large: {} bytes > 255-packet protocol limit",
                jpeg.len()
            );
        }
        self.link.send_frame(jpeg, packet_delay_ms)?;
        *self.link.last_frame.lock().unwrap() = Some((Arc::new(jpeg.to_vec()), Instant::now()));
        Ok(())
    }

    /// Block forever while the refresher thread keeps the last frame on
    /// screen.  For one-shot CLI commands.
    pub fn hold(&self) -> ! {
        loop {
            std::thread::park();
        }
    }

    /// Send the keepalive packet (opcode `0x82`) and consume its ACK.
    pub fn keepalive(&self) -> Result<()> {
        let _io = self.link.io.lock().unwrap();
        self.link
            .write_ctrl(&build_ctrl(KEEPALIVE_OPCODE))
            .context("keepalive")?;
        if let Err(e) = self.link.read_ctrl() {
            log::debug!("no keepalive ack: {e}");
        }
        Ok(())
    }

    /// Send a single arbitrary control opcode (useful for reverse engineering).
    pub fn send_ctrl_opcode(&self, opcode: u8) -> Result<()> {
        self.write_ctrl(&build_ctrl(opcode))
            .with_context(|| format!("ctrl opcode 0x{opcode:02x}"))
    }

    /// Read a single response packet from the control IN endpoint.
    pub fn read_ctrl_response(&self) -> Result<Vec<u8>> {
        self.read_ctrl()
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn write_ctrl(&self, buf: &[u8]) -> Result<()> {
        let _io = self.link.io.lock().unwrap();
        self.link.write_ctrl(buf)
    }

    fn read_ctrl(&self) -> Result<Vec<u8>> {
        self.link.read_ctrl()
    }
}

impl Link {
    /// Re-send the last frame whenever it is about to age out on the panel.
    fn refresh_loop(&self) {
        while !self.stop.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(50));
            let due = match &*self.last_frame.lock().unwrap() {
                Some((jpeg, sent)) if sent.elapsed() >= FRAME_REFRESH_INTERVAL => {
                    Some(Arc::clone(jpeg))
                }
                _ => None,
            };
            let Some(jpeg) = due else { continue };
            if let Err(e) = self.send_frame(&jpeg, 0) {
                log::debug!("frame refresh failed: {e:#}");
            }
            // Stamp even on failure so a dead device isn't hammered; a newer
            // frame from the caller simply overwrites this.
            if let Some((cur, sent)) = &mut *self.last_frame.lock().unwrap() {
                if Arc::ptr_eq(cur, &jpeg) {
                    *sent = Instant::now();
                }
            }
        }
    }

    fn send_frame(&self, jpeg: &[u8], packet_delay_ms: u64) -> Result<()> {
        let pkts = build_image_packets(jpeg);
        log::debug!("send_jpeg: {} bytes in {} packets", jpeg.len(), pkts.len());

        let mut stream_started = self.io.lock().unwrap();
        if !*stream_started {
            self.write_ctrl(&build_frame_begin())
                .context("frame begin prelude")?;
            self.write_frame(&pkts, packet_delay_ms)?;
            *stream_started = true;
        }
        self.write_frame(&pkts, packet_delay_ms)
    }

    fn write_ctrl(&self, buf: &[u8]) -> Result<()> {
        let n = self
            .handle
            .write_interrupt(EP_CTRL_OUT, buf, TRANSFER_TIMEOUT)?;
        if n != buf.len() {
            bail!("short ctrl write: {}/{}", n, buf.len());
        }
        Ok(())
    }

    fn read_ctrl(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; CTRL_PKT_SIZE];
        let n = self
            .handle
            .read_interrupt(EP_CTRL_IN, &mut buf, TRANSFER_TIMEOUT)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Write one frame's packets, then consume the panel's ACK.  A missing ACK
    /// is logged rather than treated as fatal: the next frame goes through.
    fn write_frame(&self, pkts: &[[u8; IMG_PKT_SIZE]], packet_delay_ms: u64) -> Result<()> {
        for (i, pkt) in pkts.iter().enumerate() {
            self.write_image(pkt)
                .with_context(|| format!("image packet {}/{}", i + 1, pkts.len()))?;
            if packet_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(packet_delay_ms));
            }
        }
        let mut ack = [0u8; IMG_ACK_SIZE];
        let t0 = Instant::now();
        match self
            .handle
            .read_interrupt(EP_IMG_IN, &mut ack, IMG_ACK_TIMEOUT)
        {
            Ok(n) => log::debug!(
                "frame ack {:02x?} after {:?}",
                &ack[..n.min(2)],
                t0.elapsed()
            ),
            Err(e) => log::debug!("no frame ack from panel after {:?}: {e}", t0.elapsed()),
        }
        Ok(())
    }

    fn write_image(&self, buf: &[u8]) -> Result<()> {
        let n = self
            .handle
            .write_interrupt(EP_IMG_OUT, buf, TRANSFER_TIMEOUT)?;
        if n != buf.len() {
            bail!("short image write: {}/{}", n, buf.len());
        }
        Ok(())
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.link.stop.store(true, Ordering::Release);
        if let Some(t) = self.refresher.take() {
            let _ = t.join();
        }
        for iface in [IF_CONTROL, IF_IMAGE] {
            let _ = self.link.handle.release_interface(iface);
            // Best-effort: re-attach the kernel driver so usbhid works again
            // after we exit.  Failure is silently ignored.
            let _ = self.link.handle.attach_kernel_driver(iface);
        }
    }
}

// ─── Image helpers ───────────────────────────────────────────────────────────

/// Resize `img` to the panel's native resolution (480×480) and JPEG-encode it.
///
/// `quality` is an upper bound: very busy images encode to huge JPEGs, which
/// cannot be sent at all past 255 packets and on `0x2343` are intermittently
/// acknowledged late.  The quality is stepped down until the frame is at most
/// [`TARGET_JPEG_BYTES`], and the level that worked is remembered process-wide
/// so an animation that stays busy doesn't pay for the retries every frame.
pub fn encode_for_panel(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    static QUALITY_HINT: AtomicU8 = AtomicU8::new(100);

    let start = quality.min(QUALITY_HINT.load(Ordering::Relaxed));
    let (jpeg, used) = encode_within(img, start, TARGET_JPEG_BYTES)?;
    // Creep back up once frames are comfortably small again.
    let comfortable = jpeg.len() < TARGET_JPEG_BYTES * 6 / 10;
    QUALITY_HINT.store(
        if comfortable {
            used.saturating_add(5).min(100)
        } else {
            used
        },
        Ordering::Relaxed,
    );

    if jpeg.len() > MAX_JPEG_BYTES {
        bail!(
            "frame is {} bytes even at q{used}; exceeds 255-packet limit",
            jpeg.len()
        );
    }
    Ok(jpeg)
}

/// Resize `img` to the panel and JPEG-encode it at `quality`, lowering the
/// quality by a quarter at a time (down to [`MIN_JPEG_QUALITY`]) until the
/// result is at most `target` bytes.  Returns the JPEG and the quality used;
/// the JPEG can still exceed `target` if even the minimum quality is too big.
fn encode_within(img: &DynamicImage, quality: u8, target: usize) -> Result<(Vec<u8>, u8)> {
    let resized = if img.width() == PANEL_WIDTH && img.height() == PANEL_HEIGHT {
        img.to_rgb8()
    } else {
        img.resize_exact(PANEL_WIDTH, PANEL_HEIGHT, FilterType::Lanczos3)
            .to_rgb8()
    };

    let mut q = quality.max(MIN_JPEG_QUALITY);
    loop {
        let mut out = Vec::with_capacity(64 * 1024);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q)
            .encode_image(&resized)
            .context("JPEG encode")?;
        if out.len() <= target || q == MIN_JPEG_QUALITY {
            return Ok((out, q));
        }
        log::debug!(
            "frame is {} bytes at q{q}; retrying at lower quality",
            out.len()
        );
        q = (q as u32 * 3 / 4).max(MIN_JPEG_QUALITY as u32) as u8;
    }
}

/// Fit `img` inside 480×480 with letterboxing (black bars) and JPEG-encode.
pub fn letterbox_for_panel(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let (w, h) = letterbox_size(img.width(), img.height());
    let resized = img.resize_exact(w, h, FilterType::Lanczos3).to_rgb8();

    let mut canvas = image::RgbImage::new(PANEL_WIDTH, PANEL_HEIGHT);
    image::imageops::replace(
        &mut canvas,
        &resized,
        ((PANEL_WIDTH - w) / 2) as i64,
        ((PANEL_HEIGHT - h) / 2) as i64,
    );
    encode_for_panel(&DynamicImage::ImageRgb8(canvas), quality)
}

/// Largest size with the aspect ratio of `width`×`height` that fits the
/// panel.  Each side is at least 1 px, however extreme the ratio.
fn letterbox_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width.max(1) as f64, height.max(1) as f64);
    let scale = (PANEL_WIDTH as f64 / w).min(PANEL_HEIGHT as f64 / h);
    let fit = |side: f64, max: u32| ((side * scale).round() as u32).clamp(1, max);
    (fit(w, PANEL_WIDTH), fit(h, PANEL_HEIGHT))
}

/// Decode an image from memory, refusing ones whose dimensions or memory use
/// are unreasonable — a few kilobytes of crafted file can otherwise ask the
/// decoder for gigabytes.  Use this for anything that arrives over the API.
pub fn decode_image(bytes: &[u8]) -> Result<DynamicImage> {
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("identify image format")?;
    decode_limited(reader)
}

/// Like [`decode_image`], for a file on disk.
pub fn open_image(path: impl AsRef<Path>) -> Result<DynamicImage> {
    let path = path.as_ref();
    let reader = ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .with_context(|| format!("open {}", path.display()))?;
    decode_limited(reader).with_context(|| format!("decode {}", path.display()))
}

fn decode_limited<R: std::io::BufRead + std::io::Seek>(
    mut reader: ImageReader<R>,
) -> Result<DynamicImage> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    reader.decode().context("decode image")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// Incompressible image: every pixel pseudo-random.
    fn noise(width: u32, height: u32) -> DynamicImage {
        let mut seed = 0x1234_5678u32;
        DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            Rgb([(seed >> 24) as u8, (seed >> 16) as u8, (seed >> 8) as u8])
        }))
    }

    fn solid(width: u32, height: u32, color: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb(color)))
    }

    fn png_bytes(img: &DynamicImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn simple_image_keeps_the_requested_quality() {
        let (jpeg, q) =
            encode_within(&solid(480, 480, [200, 30, 30]), 85, TARGET_JPEG_BYTES).unwrap();
        assert_eq!(q, 85);
        assert!(jpeg.len() <= TARGET_JPEG_BYTES);
        assert_eq!(&jpeg[..2], &[0xff, 0xd8], "not a JPEG");
    }

    #[test]
    fn busy_image_is_stepped_down_to_fit() {
        let img = noise(480, 480);
        let (full, _) = encode_within(&img, 85, usize::MAX).unwrap();
        assert!(
            full.len() > TARGET_JPEG_BYTES,
            "noise should not fit at q85"
        );

        let (jpeg, q) = encode_within(&img, 85, TARGET_JPEG_BYTES).unwrap();
        assert!(q < 85);
        assert!(jpeg.len() <= TARGET_JPEG_BYTES || q == MIN_JPEG_QUALITY);
    }

    #[test]
    fn quality_never_drops_below_the_minimum() {
        // An impossible target: the loop must still terminate, at the floor.
        let (_, q) = encode_within(&noise(480, 480), 85, 1).unwrap();
        assert_eq!(q, MIN_JPEG_QUALITY);
        let (_, q) = encode_within(&solid(480, 480, [0, 0, 0]), 0, usize::MAX).unwrap();
        assert_eq!(q, MIN_JPEG_QUALITY);
    }

    #[test]
    fn encoded_frames_are_panel_sized_and_sendable() {
        for img in [
            solid(64, 48, [10, 200, 90]),
            noise(480, 480),
            noise(1000, 300),
        ] {
            let jpeg = encode_for_panel(&img, 95).unwrap();
            assert!(jpeg.len() <= MAX_JPEG_BYTES);
            let back = decode_image(&jpeg).unwrap();
            assert_eq!((back.width(), back.height()), (PANEL_WIDTH, PANEL_HEIGHT));
            // Must not panic: the frame fits the one-byte packet count.
            assert!(build_image_packets(&jpeg).len() <= 255);
        }
    }

    #[test]
    fn letterbox_size_preserves_aspect_and_fits() {
        assert_eq!(letterbox_size(480, 480), (480, 480));
        assert_eq!(letterbox_size(960, 480), (480, 240));
        assert_eq!(letterbox_size(100, 400), (120, 480));
        assert_eq!(letterbox_size(1920, 1080), (480, 270));
        // Extreme ratios keep at least one pixel instead of collapsing to 0.
        assert_eq!(letterbox_size(10_000, 1), (480, 1));
        assert_eq!(letterbox_size(1, 10_000), (1, 480));
        assert_eq!(letterbox_size(0, 0), (480, 480));
    }

    #[test]
    fn letterbox_centres_the_image_on_black() {
        let jpeg = letterbox_for_panel(&solid(200, 100, [255, 255, 255]), 95).unwrap();
        let img = decode_image(&jpeg).unwrap().to_rgb8();
        assert_eq!((img.width(), img.height()), (480, 480));
        assert!(
            img.get_pixel(240, 240).0.iter().all(|&v| v > 240),
            "centre should be white"
        );
        assert!(
            img.get_pixel(240, 20).0.iter().all(|&v| v < 16),
            "top bar should be black"
        );
        assert!(
            img.get_pixel(240, 460).0.iter().all(|&v| v < 16),
            "bottom bar should be black"
        );
    }

    #[test]
    fn letterbox_handles_extreme_aspect_ratios() {
        assert!(letterbox_for_panel(&solid(2000, 1, [255, 0, 0]), 85).is_ok());
        assert!(letterbox_for_panel(&solid(1, 2000, [255, 0, 0]), 85).is_ok());
    }

    #[test]
    fn decode_accepts_normal_images_and_rejects_garbage() {
        let img = decode_image(&png_bytes(&solid(32, 16, [1, 2, 3]))).unwrap();
        assert_eq!((img.width(), img.height()), (32, 16));
        assert!(decode_image(b"definitely not an image").is_err());
        assert!(decode_image(&[]).is_err());
    }

    #[test]
    fn decode_rejects_oversized_dimensions() {
        let wide = png_bytes(&solid(MAX_DECODE_DIMENSION + 1, 1, [0, 0, 0]));
        assert!(decode_image(&wide).is_err());
        let ok = png_bytes(&solid(MAX_DECODE_DIMENSION, 1, [0, 0, 0]));
        assert!(decode_image(&ok).is_ok());
    }

    #[test]
    fn open_image_reads_files_and_reports_missing_ones() {
        let path = std::env::temp_dir().join(format!("ttlcd-test-{}.png", std::process::id()));
        std::fs::write(&path, png_bytes(&solid(8, 8, [9, 9, 9]))).unwrap();
        let img = open_image(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(img.unwrap().width(), 8);
        assert!(open_image("/nonexistent/ttlcd.png").is_err());
    }
}
