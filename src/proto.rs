//! Wire-protocol constants and packet builders for XinWeiYe-OEM USB LCD panels
//! (Thermaltake "TT LCD Display Module" SKUs `0x264a:0x233d`, `0x264a:0x2343`).
//!
//! Reverse-engineered protocol per <https://github.com/bekindpleaserewind/ttlcd>
//! (Python, sibling SKU `0x233d`). The two SKUs share identical USB descriptors
//! and packet sizes; opcodes assumed identical until proven otherwise on `0x2343`.

pub const DEFAULT_VID: u16 = 0x264a;
pub const DEFAULT_PID: u16 = 0x2343;

pub const IF_CONTROL: u8 = 0;
pub const IF_IMAGE: u8 = 1;

pub const EP_CTRL_OUT: u8 = 0x01;
pub const EP_CTRL_IN: u8 = 0x82;
pub const EP_IMG_OUT: u8 = 0x03;
pub const EP_IMG_IN: u8 = 0x84;

pub const CTRL_PKT_SIZE: usize = 440;
pub const IMG_PKT_SIZE: usize = 1024;
pub const IMG_HDR_SIZE: usize = 4;
pub const IMG_PAYLOAD_SIZE: usize = IMG_PKT_SIZE - IMG_HDR_SIZE; // 1020

/// Size of the per-frame ACK the panel returns on [`EP_IMG_IN`] once it has
/// received a complete image (`[0x08, 0x81, 0, …]` on `0x2343`).
pub const IMG_ACK_SIZE: usize = 16;

/// Init opcodes, sent in order on EP_CTRL_OUT.  Each is followed by a 440-byte
/// read on EP_CTRL_IN.  The first response embeds the device serial number
/// starting at byte 4.
pub const INIT_OPCODES: &[u8] = &[0x85, 0x87, 0x85, 0x87, 0x84, 0x81];

/// Keepalive opcode.  Note: on `0x2343` this alone does not reliably keep an
/// image on screen — the panel falls back to its built-in screen ~2 s after
/// the last frame, so frames must be re-sent (`Panel` does this in a background thread).
pub const KEEPALIVE_OPCODE: u8 = 0x82;

/// "Begin image stream" bytes, zero-padded to [`CTRL_PKT_SIZE`].  Sent once
/// after init, not per frame.
pub const FRAME_BEGIN: [u8; 5] = [0x12, 0x01, 0x00, 0x80, 0x64];

/// Leading byte of every image data packet.
pub const IMG_OPCODE: u8 = 0x08;

// ─── Packet builders ─────────────────────────────────────────────────────────

/// 440-byte control packet: `[opcode, 0x01, 0x00, 0x80]` + 436 zero pad.
pub fn build_ctrl(opcode: u8) -> [u8; CTRL_PKT_SIZE] {
    let mut buf = [0u8; CTRL_PKT_SIZE];
    buf[0] = opcode;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x80;
    buf
}

/// 440-byte "begin frame" packet sent before each image stream.
pub fn build_frame_begin() -> [u8; CTRL_PKT_SIZE] {
    let mut buf = [0u8; CTRL_PKT_SIZE];
    buf[..FRAME_BEGIN.len()].copy_from_slice(&FRAME_BEGIN);
    buf
}

/// Split a JPEG payload into 1024-byte image packets.
///
/// Header layout:
/// - First packet:       `[0x08, total_count, 0x00, 0x80]` + up to 1020 image bytes
/// - Subsequent packets: `[0x08, pkt_index,  0x00, 0x00]` + up to 1020 image bytes
///
/// `pkt_index` is 1-based and increments only on packets after the first.
/// The last packet is zero-padded to exactly 1024 bytes.
pub fn build_image_packets(jpeg: &[u8]) -> Vec<[u8; IMG_PKT_SIZE]> {
    let total = jpeg.len().div_ceil(IMG_PAYLOAD_SIZE).max(1);
    let total_byte: u8 = total
        .try_into()
        .expect("JPEG too large for u8 packet count (>255 packets)");
    let mut out = Vec::with_capacity(total);

    let mut offset = 0usize;
    let mut pkt_index: u8 = 1;
    for i in 0..total {
        let mut pkt = [0u8; IMG_PKT_SIZE];
        pkt[0] = IMG_OPCODE;
        if i == 0 {
            pkt[1] = total_byte;
            pkt[3] = 0x80;
        } else {
            pkt[1] = pkt_index;
            pkt[3] = 0x00;
            pkt_index = pkt_index.wrapping_add(1);
        }

        let take = (jpeg.len() - offset).min(IMG_PAYLOAD_SIZE);
        pkt[IMG_HDR_SIZE..IMG_HDR_SIZE + take].copy_from_slice(&jpeg[offset..offset + take]);
        offset += take;
        out.push(pkt);
    }

    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_packet_is_440_bytes_with_header() {
        let p = build_ctrl(0x85);
        assert_eq!(p.len(), 440);
        assert_eq!(&p[..4], &[0x85, 0x01, 0x00, 0x80]);
        assert!(p[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn frame_begin_matches_python() {
        let p = build_frame_begin();
        assert_eq!(p.len(), 440);
        assert_eq!(&p[..5], &[0x12, 0x01, 0x00, 0x80, 0x64]);
        assert!(p[5..].iter().all(|&b| b == 0));
    }

    #[test]
    fn single_short_image_yields_one_packet() {
        let jpeg = vec![0xff, 0xd8, 0xff, 0xd9];
        let pkts = build_image_packets(&jpeg);
        assert_eq!(pkts.len(), 1);
        assert_eq!(&pkts[0][..4], &[0x08, 1, 0x00, 0x80]);
        assert_eq!(&pkts[0][4..8], &[0xff, 0xd8, 0xff, 0xd9]);
        assert!(pkts[0][8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn exact_multiple_of_payload_size_packs_cleanly() {
        let jpeg = vec![0xab; IMG_PAYLOAD_SIZE * 3];
        let pkts = build_image_packets(&jpeg);
        assert_eq!(pkts.len(), 3);
        assert_eq!(pkts[0][1], 3);
        assert_eq!(pkts[0][3], 0x80);
        assert_eq!(pkts[1][1], 1);
        assert_eq!(pkts[1][3], 0x00);
        assert_eq!(pkts[2][1], 2);
        assert_eq!(pkts[2][3], 0x00);
    }

    #[test]
    fn last_packet_is_zero_padded() {
        let jpeg = vec![0xcd; IMG_PAYLOAD_SIZE + 5];
        let pkts = build_image_packets(&jpeg);
        assert_eq!(pkts.len(), 2);
        assert_eq!(&pkts[1][4..9], &[0xcd; 5]);
        assert!(pkts[1][9..].iter().all(|&b| b == 0));
    }

    #[test]
    fn packet_indices_correct_for_five_chunks() {
        let jpeg = vec![0u8; IMG_PAYLOAD_SIZE * 5];
        let pkts = build_image_packets(&jpeg);
        let indices: Vec<u8> = pkts.iter().map(|p| p[1]).collect();
        assert_eq!(indices, vec![5, 1, 2, 3, 4]);
        let flags: Vec<u8> = pkts.iter().map(|p| p[3]).collect();
        assert_eq!(flags, vec![0x80, 0x00, 0x00, 0x00, 0x00]);
    }
}
