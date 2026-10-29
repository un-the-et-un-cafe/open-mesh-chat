//! Mesh forwarding rules for live audio (any phone, member or not).
//!
//! - Forward each packet once (dedup by tag + seq), with `hop + 1`, until
//!   `max_hops`.
//! - Drop packets older than [`AUDIO_EXPIRY_MS`](super::AUDIO_EXPIRY_MS) or
//!   from the future; never store them (RL1–RL4 do not apply to live audio).
//! - Drop, never queue, when a link is congested; chat keeps priority
//!   ([`LinkScheduler`]).

use std::collections::{HashMap, VecDeque};

use super::audio::{next_hop, AudioHeader};
use super::AUDIO_EXPIRY_MS;

const CLOCK_SKEW_MS: u64 = 2_000;
/// Live packets queued per link before new ones are dropped.
pub const LIVE_QUEUE_CAP: usize = 4;

/// What to do with one received packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayDecision {
    /// New (not a duplicate) and fresh: hand to the local player if we hold the key.
    pub deliver: bool,
    /// Packet to send on to the other neighbours.
    pub forward: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct LiveRelay {
    seen: HashMap<([u8; 8], u32), u64>,
    pub dropped_expired: u64,
    pub dropped_duplicate: u64,
}

impl LiveRelay {
    pub fn on_packet(&mut self, pkt: &[u8], now_ms: u64) -> RelayDecision {
        let none = RelayDecision {
            deliver: false,
            forward: None,
        };
        let Ok(h) = AudioHeader::parse(pkt) else {
            return none;
        };
        if h.sent_ms.saturating_add(AUDIO_EXPIRY_MS) < now_ms
            || h.sent_ms > now_ms.saturating_add(CLOCK_SKEW_MS)
        {
            self.dropped_expired += 1;
            return none;
        }
        if self.seen.len() > 4096 {
            self.seen.retain(|_, exp| *exp >= now_ms);
        }
        if self
            .seen
            .insert((h.tag, h.seq), h.sent_ms + AUDIO_EXPIRY_MS)
            .is_some()
        {
            self.dropped_duplicate += 1;
            return none;
        }
        RelayDecision {
            deliver: true,
            forward: next_hop(pkt).ok().flatten(),
        }
    }
}

/// Per-link send queue: chat frames always first and never dropped; live
/// packets are dropped when [`LIVE_QUEUE_CAP`] are already waiting (LV2).
#[derive(Debug, Default)]
pub struct LinkScheduler {
    chat: VecDeque<Vec<u8>>,
    live: VecDeque<Vec<u8>>,
    pub live_dropped: u64,
}

impl LinkScheduler {
    pub fn push_chat(&mut self, f: Vec<u8>) {
        self.chat.push_back(f);
    }

    /// `false` if the packet was dropped (congested).
    pub fn push_live(&mut self, f: Vec<u8>) -> bool {
        if self.live.len() >= LIVE_QUEUE_CAP {
            self.live_dropped += 1;
            return false;
        }
        self.live.push_back(f);
        true
    }

    pub fn pop(&mut self) -> Option<Vec<u8>> {
        self.chat.pop_front().or_else(|| self.live.pop_front())
    }

    pub fn len(&self) -> usize {
        self.chat.len() + self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
