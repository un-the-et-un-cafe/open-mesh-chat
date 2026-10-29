//! Mesh audio packet (link frame `0x4C`).
//!
//! ```text
//! 0x4C ‖ version 1 ‖ tag[8] ‖ key_epoch u32 ‖ seq u32 ‖ hop u8 ‖ max_hops u8
//!      ‖ sent_ms u64 ‖ nonce[24] ‖ XChaCha20-Poly1305(key, plain, aad)
//! plain = n u8 ‖ n × (len u8 ‖ opus frame)      // n consecutive 20 ms frames
//! aad   = header without the hop byte            // relays bump `hop`
//! ```
//!
//! Frame `k` of packet `seq` has index `seq × n + k` for the jitter buffer.
//! Packing 5 frames (100 ms) per packet keeps BLE at 10 packets/s; there is
//! no retransmission, Opus in-band FEC covers single losses.

use super::{err, StreamKey};
use crate::error::Result;

pub const FRAME_LIVE_AUDIO: u8 = 0x4C;
const VERSION: u8 = 1;
const HEADER_LEN: usize = 1 + 1 + 8 + 4 + 4 + 1 + 1 + 8;
const HOP_AT: usize = 18;
/// Frames per packet (5 × 20 ms).
pub const FRAMES_PER_PACKET: usize = 5;
pub const MAX_FRAME_BYTES: usize = 160;

/// Cleartext header, readable by any relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub tag: [u8; 8],
    pub key_epoch: u32,
    pub seq: u32,
    pub hop: u8,
    pub max_hops: u8,
    pub sent_ms: u64,
}

impl AudioHeader {
    fn bytes(&self) -> Vec<u8> {
        let mut b = vec![FRAME_LIVE_AUDIO, VERSION];
        b.extend_from_slice(&self.tag);
        b.extend_from_slice(&self.key_epoch.to_be_bytes());
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.push(self.hop);
        b.push(self.max_hops);
        b.extend_from_slice(&self.sent_ms.to_be_bytes());
        b
    }

    fn aad(&self) -> Vec<u8> {
        let mut b = self.bytes();
        b[HOP_AT] = 0;
        b
    }

    pub fn parse(pkt: &[u8]) -> Result<Self> {
        if pkt.len() < HEADER_LEN || pkt[0] != FRAME_LIVE_AUDIO || pkt[1] != VERSION {
            return err("not a live audio packet");
        }
        let h = Self {
            tag: pkt[2..10].try_into().expect("8"),
            key_epoch: u32::from_be_bytes(pkt[10..14].try_into().expect("4")),
            seq: u32::from_be_bytes(pkt[14..18].try_into().expect("4")),
            hop: pkt[18],
            max_hops: pkt[19],
            sent_ms: u64::from_be_bytes(pkt[20..28].try_into().expect("8")),
        };
        if h.hop == 0 || h.max_hops == 0 || h.hop > h.max_hops {
            return err("live hop count");
        }
        Ok(h)
    }
}

pub fn is_live_audio(pkt: &[u8]) -> bool {
    pkt.first() == Some(&FRAME_LIVE_AUDIO)
}

/// Host side: seal `frames` (each one Opus frame) as packet `seq`.
pub fn seal_audio(
    key: &StreamKey,
    seq: u32,
    max_hops: u8,
    sent_ms: u64,
    frames: &[Vec<u8>],
) -> Result<Vec<u8>> {
    if frames.is_empty() || frames.len() > 16 {
        return err("live frame count");
    }
    let h = AudioHeader {
        tag: key.tag(),
        key_epoch: key.key_epoch,
        seq,
        hop: 1,
        max_hops,
        sent_ms,
    };
    let mut plain = vec![frames.len() as u8];
    for f in frames {
        if f.is_empty() || f.len() > MAX_FRAME_BYTES {
            return err("live frame size");
        }
        plain.push(f.len() as u8);
        plain.extend_from_slice(f);
    }
    let mut pkt = h.bytes();
    pkt.extend_from_slice(&key.seal(&plain, &h.aad())?);
    Ok(pkt)
}

/// Relay side: the same packet one hop further (`None` at the hop limit).
pub fn next_hop(pkt: &[u8]) -> Result<Option<Vec<u8>>> {
    let h = AudioHeader::parse(pkt)?;
    if h.hop >= h.max_hops {
        return Ok(None);
    }
    let mut p = pkt.to_vec();
    p[HOP_AT] = h.hop + 1;
    Ok(Some(p))
}

/// Listener side: decrypt. Fails for anyone without the stream key (LV4).
pub fn open_audio(key: &StreamKey, pkt: &[u8]) -> Result<(AudioHeader, Vec<Vec<u8>>)> {
    let h = AudioHeader::parse(pkt)?;
    if h.tag != key.tag() || h.key_epoch != key.key_epoch {
        return err("live packet for another stream or key epoch");
    }
    let plain = key.open(&pkt[HEADER_LEN..], &h.aad())?;
    let n = *plain.first().ok_or(crate::OmcpError::Sync("live plain"))? as usize;
    let mut frames = Vec::with_capacity(n);
    let mut i = 1;
    for _ in 0..n {
        let l = *plain.get(i).ok_or(crate::OmcpError::Sync("live plain"))? as usize;
        let f = plain
            .get(i + 1..i + 1 + l)
            .ok_or(crate::OmcpError::Sync("live plain"))?;
        frames.push(f.to_vec());
        i += 1 + l;
    }
    if i != plain.len() {
        return err("live plain trailing bytes");
    }
    Ok((h, frames))
}
