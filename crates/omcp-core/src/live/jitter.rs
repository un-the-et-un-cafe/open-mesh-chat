//! Listener jitter buffer for 20 ms Opus frames.
//!
//! Playout starts `target_ms` after the first frame arrives, then one frame
//! every 20 ms. A missing frame yields [`Playout::Lost`] (the decoder runs
//! Opus FEC from the next frame if it has it, else packet-loss concealment).
//! Frames arriving after their slot are dropped; nothing is retransmitted.
//! Late joiners start from the first frame they receive.

use std::collections::BTreeMap;

use super::FRAME_MS;

pub const DEFAULT_TARGET_MS: u64 = 600;
const MAX_BUFFERED: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playout {
    Frame {
        index: u64,
        opus: Vec<u8>,
    },
    /// Missing frame; `next` is the following frame if already here (for FEC).
    Lost {
        index: u64,
        next: Option<Vec<u8>>,
    },
}

#[derive(Debug)]
pub struct JitterBuffer {
    target_ms: u64,
    frames: BTreeMap<u64, Vec<u8>>,
    next: Option<u64>,
    clock0: Option<(u64, u64)>, // (first index, playout start ms)
    max_index: u64,
    pub played: u64,
    pub lost: u64,
    pub late: u64,
}

/// After the newest frame, conceal at most this many slots (1 s), then go
/// quiet until frames arrive again.
const MAX_CONCEAL: u64 = 50;

impl JitterBuffer {
    pub fn new(target_ms: u64) -> Self {
        Self {
            target_ms,
            frames: BTreeMap::new(),
            next: None,
            clock0: None,
            max_index: 0,
            played: 0,
            lost: 0,
            late: 0,
        }
    }

    pub fn push(&mut self, index: u64, opus: Vec<u8>, now_ms: u64) {
        if self.clock0.is_none() {
            self.clock0 = Some((index, now_ms + self.target_ms));
            self.next = Some(index);
        }
        if index < self.next.unwrap_or(0) {
            self.late += 1;
            return;
        }
        self.max_index = self.max_index.max(index);
        if self.frames.len() < MAX_BUFFERED {
            self.frames.insert(index, opus);
        }
    }

    /// All frames whose playout time has come.
    pub fn pop_due(&mut self, now_ms: u64) -> Vec<Playout> {
        let mut out = Vec::new();
        let Some((i0, t0)) = self.clock0 else {
            return out;
        };
        while let Some(n) = self.next {
            if n > self.max_index + MAX_CONCEAL {
                break;
            }
            let due = t0 + (n - i0) * FRAME_MS;
            if due > now_ms {
                break;
            }
            match self.frames.remove(&n) {
                Some(opus) => {
                    self.played += 1;
                    out.push(Playout::Frame { index: n, opus });
                }
                None => {
                    self.lost += 1;
                    out.push(Playout::Lost {
                        index: n,
                        next: self.frames.get(&(n + 1)).cloned(),
                    });
                }
            }
            self.next = Some(n + 1);
        }
        out
    }

    /// Lost share of frames played so far.
    pub fn loss_ratio(&self) -> f64 {
        let t = self.played + self.lost;
        if t == 0 {
            0.0
        } else {
            self.lost as f64 / t as f64
        }
    }

    /// Playout time of `index` (for delay measurement).
    pub fn playout_ms(&self, index: u64) -> Option<u64> {
        let (i0, t0) = self.clock0?;
        (index >= i0).then(|| t0 + (index - i0) * FRAME_MS)
    }
}
