//! SY5: relays never exceed their budget; eviction follows the RL2 order
//! (class, then oldest).

use omcp_core::sync::budget::{
    InsertOutcome, RelayBudget, RelayClass, RelayStore, StoredEnvelope, MB,
};
use omcp_core::sync::SplitMix64;

#[derive(Debug, Clone)]
pub struct Sy5Report {
    pub relays: usize,
    pub inserts: u64,
    pub evictions: u64,
    pub rejections: u64,
    pub max_fill_ratio: f64,
    pub order_violations: u64,
    pub forward_cap_violations: u64,
}

/// Random envelopes (0.3–64 KB, random RL2 class, 1 h–7 d expiry, 0–11 hops)
/// pushed through `relays` relays on the constrained 50 MB / 20 MB-day
/// budget, over simulated time. Every eviction is checked to be the minimum
/// `(class, created_ms, Reverse(hop_count), id)` among stored envelopes.
pub fn run(relays: usize, inserts_per_relay: u64, seed: u64) -> Sy5Report {
    let mut rng = SplitMix64(seed);
    let budget = RelayBudget {
        storage_bytes: 50 * MB,
        daily_forward_bytes: Some(20 * MB),
    };
    let mut r = Sy5Report {
        relays,
        inserts: 0,
        evictions: 0,
        rejections: 0,
        max_fill_ratio: 0.0,
        order_violations: 0,
        forward_cap_violations: 0,
    };
    for _ in 0..relays {
        let mut s = RelayStore::new(budget);
        let mut shadow: Vec<StoredEnvelope> = Vec::new();
        let mut now = 0u64;
        for k in 0..inserts_per_relay {
            now += rng.below(2_000);
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&k.to_be_bytes());
            let e = StoredEnvelope {
                id,
                size: 300 + rng.below(64_000),
                class: RelayClass::from_u8(rng.below(4) as u8).expect("0..4"),
                created_ms: now.saturating_sub(rng.below(3_600_000)),
                expires_at: now + 3_600_000 + rng.below(6 * 86_400_000),
                hop_count: rng.below(12) as u8,
            };
            shadow.retain(|x| x.expires_at > now);
            r.inserts += 1;
            match s.insert(e.clone(), now) {
                InsertOutcome::Stored { evicted } => {
                    for v in evicted {
                        let min = shadow
                            .iter()
                            .map(|x| x.evict_key())
                            .min()
                            .expect("nonempty");
                        let pos = shadow.iter().position(|x| x.id == v).expect("known");
                        if shadow[pos].evict_key() != min {
                            r.order_violations += 1;
                        }
                        shadow.swap_remove(pos);
                        r.evictions += 1;
                    }
                    shadow.push(e);
                }
                InsertOutcome::Rejected => r.rejections += 1,
            }
            let fill = s.used() as f64 / budget.storage_bytes as f64;
            r.max_fill_ratio = r.max_fill_ratio.max(fill);
            if s.try_forward(rng.below(64_000), now).is_ok()
                && s.forwarded_today() > budget.daily_forward_bytes.expect("cap")
            {
                r.forward_cap_violations += 1;
            }
        }
    }
    r
}
