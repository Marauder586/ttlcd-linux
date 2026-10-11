//! High-level driver for the XinWeiYe-OEM USB LCD panel
//! (Thermaltake `0x264a:0x2343`, "3.95 square TFT LCD Display Module", 480×480).
//!
//! Talks to the panel's two HID interfaces as raw USB endpoints.  The kernel
//! `usbhid` driver is detached from both interfaces before claiming (and
//! re-attached on [`Drop`]).

pub mod display;
pub mod pet;
pub mod proto;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageFormat};
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

/// How long to wait for the per-frame ACK on the image IN endpoint.  It
/// normally arrives within ~10 ms of the last packet.  Roughly one frame in a
/// few hundred is acknowledged late or not at all (cause unknown), so this is
/// kept short to make that a barely visible hitch rather than a stall.
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
    /// Serialises whole control packets / whole frames on the wire.
    io: Mutex<()>,
    /// Set once the `0x12` stream-begin prelude and the first frame are out.
    stream_started: AtomicBool,
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
        let target_vid = format!("{:04x}", vid);
        let target_pid = format!("{:04x}", pid);

        let port_path: Option<String> = (|| {
            let dir = std::fs::read_dir("/sys/bus/usb/devices").ok()?;
            for entry in dir.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                // Only look at port entries (e.g. "1-6"), not interface entries.
                if name.contains(':') { continue; }
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
            log::warn!("sysfs_rebind: could not find sysfs port for {:04x}:{:04x}", vid, pid);
            return;
        };

        log::debug!("sysfs_rebind: found port {}", port);
        let unbind = "/sys/bus/usb/drivers/usb/unbind";
        let bind   = "/sys/bus/usb/drivers/usb/bind";

        match std::fs::OpenOptions::new().write(true).open(unbind)
            .and_then(|mut f| f.write_all(port.as_bytes()))
        {
            Ok(()) => {
                log::debug!("sysfs_rebind: unbound {}", port);
                std::thread::sleep(Duration::from_millis(300));
                match std::fs::OpenOptions::new().write(true).open(bind)
                    .and_then(|mut f| f.write_all(port.as_bytes()))
                {
                    Ok(()) => log::debug!("sysfs_rebind: rebound {}", port),
                    Err(e) => log::warn!("sysfs_rebind bind failed: {}", e),
                }
            }
            Err(e) => log::warn!("sysfs_rebind unbind failed (likely not root): {}", e),
        }
    }

    fn open_inner(vid: u16, pid: u16) -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(vid, pid)
            .ok_or_else(|| anyhow!(
                "USB device {:04x}:{:04x} not found.\n\
                 Hint: ensure the device is connected and you have read/write \
                 permission on /dev/bus/usb (udev rule or run as root).",
                vid, pid
            ))?;

        // Set configuration 1.  The device is almost always already in config 1,
        // and attempting to change an already-correct config can fail on some
        // platforms/states — so we only warn on failure.
        let cur_cfg = handle.active_configuration().unwrap_or(0);
        if cur_cfg != 1 {
            match handle.set_active_configuration(1) {
                Ok(())  => log::debug!("set configuration 1 (was {})", cur_cfg),
                Err(e)  => log::warn!(
                    "set_active_configuration(1) failed ({e}); current config={}; continuing",
                    cur_cfg
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
                .with_context(|| format!("claim_interface(if={})", iface))?;
        }

        let link = Arc::new(Link {
            handle,
            io: Mutex::new(()),
            stream_started: AtomicBool::new(false),
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

    /// Run the 6-step init handshake on the control interface and parse
    /// device info from the ACK packets.
    ///
    /// If the device times out on the first ACK read, automatically retries
    /// with [`Panel::init_no_ack`] (some firmware variants don't ACK).
    pub fn init(&mut self) -> Result<()> {
        log::info!("init: running {}-step handshake", INIT_OPCODES.len());
        for (i, &opcode) in INIT_OPCODES.iter().enumerate() {
            self.write_ctrl(&build_ctrl(opcode))
                .with_context(|| format!("init write opcode 0x{:02x}", opcode))?;
            let ack = self
                .read_ctrl()
                .with_context(|| format!("init read ack for opcode 0x{:02x}", opcode))?;
            log::debug!(
                "init[{}]: 0x{:02x} → ack {} bytes, head={:02x?}",
                i, opcode, ack.len(), &ack[..ack.len().min(16)]
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

    /// Variant of [`Panel::init`] that does NOT read ACKs.
    /// Matches the C++ reference's behaviour on sibling SKU `0x2428:0x350d`.
    pub fn init_no_ack(&self) -> Result<()> {
        log::info!("init_no_ack: writing {} opcodes", INIT_OPCODES.len());
        for &opcode in INIT_OPCODES {
            self.write_ctrl(&build_ctrl(opcode))
                .with_context(|| format!("init write opcode 0x{:02x}", opcode))?;
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
            .with_context(|| format!("ctrl opcode 0x{:02x}", opcode))
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
                Some((jpeg, sent)) if sent.elapsed() >= FRAME_REFRESH_INTERVAL => Some(Arc::clone(jpeg)),
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

        let _io = self.io.lock().unwrap();
        if !self.stream_started.load(Ordering::Acquire) {
            self.write_ctrl(&build_frame_begin())
                .context("frame begin prelude")?;
            self.write_frame(&pkts, packet_delay_ms)?;
            self.stream_started.store(true, Ordering::Release);
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
        match self.handle.read_interrupt(EP_IMG_IN, &mut ack, IMG_ACK_TIMEOUT) {
            Ok(n) => log::debug!("frame ack {:02x?} after {:?}", &ack[..n.min(2)], t0.elapsed()),
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
pub fn encode_for_panel(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let resized = if img.width() == PANEL_WIDTH && img.height() == PANEL_HEIGHT {
        img.to_rgb8()
    } else {
        img.resize_exact(PANEL_WIDTH, PANEL_HEIGHT, FilterType::Lanczos3)
            .to_rgb8()
    };

    let _ = ImageFormat::Jpeg; // suppress unused-import warning
    let resized = image::DynamicImage::ImageRgb8(resized);

    // Very busy images encode to huge JPEGs: past 255 packets they cannot be
    // sent at all, and on `0x2343` frames above ~120 kB are intermittently
    // not acknowledged (a visible stall).  Step the quality down until the
    // frame fits the target, and remember it so an animation that stays busy
    // doesn't pay for the retries on every frame.
    static QUALITY_HINT: AtomicU8 = AtomicU8::new(100);
    let mut q = quality.min(QUALITY_HINT.load(Ordering::Relaxed));
    loop {
        let mut out = Vec::with_capacity(64 * 1024);
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::io::Cursor::new(&mut out),
            q,
        );
        resized.write_with_encoder(encoder).context("JPEG encode")?;
        if out.len() <= TARGET_JPEG_BYTES || q <= 10 {
            // Creep back up once frames are comfortably small again.
            let next = if out.len() < TARGET_JPEG_BYTES * 6 / 10 { q.saturating_add(5).min(100) } else { q };
            QUALITY_HINT.store(next, Ordering::Relaxed);
            if out.len() > MAX_JPEG_BYTES {
                bail!("frame is {} bytes even at q{}; exceeds 255-packet limit", out.len(), q);
            }
            return Ok(out);
        }
        log::debug!("frame is {} bytes at q{}; retrying at lower quality", out.len(), q);
        q = (q as u32 * 3 / 4).max(10) as u8;
    }
}

/// Fit `img` inside 480×480 with letterboxing (black bars) and JPEG-encode.
pub fn letterbox_for_panel(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut canvas = image::RgbImage::new(PANEL_WIDTH, PANEL_HEIGHT);

    let src_w = img.width() as f32;
    let src_h = img.height() as f32;
    let scale = (PANEL_WIDTH as f32 / src_w).min(PANEL_HEIGHT as f32 / src_h);
    let new_w = (src_w * scale) as u32;
    let new_h = (src_h * scale) as u32;

    let resized = img
        .resize_exact(new_w, new_h, FilterType::Lanczos3)
        .to_rgb8();

    let x_off = (PANEL_WIDTH - new_w) / 2;
    let y_off = (PANEL_HEIGHT - new_h) / 2;

    for (x, y, px) in resized.enumerate_pixels() {
        canvas.put_pixel(x + x_off, y + y_off, *px);
    }

    encode_for_panel(&image::DynamicImage::ImageRgb8(canvas), quality)
}
