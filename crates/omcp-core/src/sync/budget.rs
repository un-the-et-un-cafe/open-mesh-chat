//! Relay budgets, eviction, expiry, metered-network rules (PROTOCOL §22.4)
//! and gateway rendezvous offload (§22.5).
//!
//! Storage follows apps/REQUIREMENTS.md "Relay (store and forward)", RL1–RL4
//! (maintainer decision 2026-10-24), which supersedes the §22.4 storage
//! presets: one automatic cap of [`RELAY_CAP_BYTES`] (500 MB), lowered by
//! [`auto_storage_cap`] when free storage is under 10 %. When full, the
//! lowest [`RelayClass`] goes first (news, then everything else, then
//! contacts' envelopes, then own chats), oldest first within a class.
//!
//! These budgets are **per-device mesh relay** budgets. They are separate
//! from, and in addition to, the gateway's Internet byte cap
//! ([`crate::gateway::policy::DEFAULT_DAILY_BYTE_CAP`], 500 MB/day).

use std::collections::HashMap;

use super::tag::rendezvous_label;
use crate::error::{OmcpError, Result};
use crate::gateway::policy::{NetworkKind, DAY_MS};

pub const MB: u64 = 1_000_000;
pub const DEFAULT_EXPIRY_MS: u64 = 72 * 3_600_000;
pub const MAX_EXPIRY_MS: u64 = 7 * DAY_MS;
/// RL2: automatic relay storage cap (500 MB).
pub const RELAY_CAP_BYTES: u64 = 500 * MB;
/// RL2: the cap is lowered when free storage is under this percentage.
pub const LOW_STORAGE_PERCENT: u64 = 10;
/// §22.4 daily forwarding budget on battery / cellular (kept; RL1–RL4 do
/// not change it).
pub const DAILY_FORWARD_BYTES: u64 = 20 * MB;

/// RL2 priority of a stored envelope. Ordered from first-dropped to
/// last-dropped: `News < Other < Contact < OwnChat`. Forwarding offers the
/// reverse order (own chats first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelayClass {
    /// Public news items (§NW, news channel mailbox tags).
    News = 0,
    /// Everything else (e.g. handed over by a non-contact).
    Other = 1,
    /// Envelopes handed to us by a contact.
    Contact = 2,
    /// Our own chats (our outbound envelopes and acks).
    OwnChat = 3,
}

impl RelayClass {
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::News,
            1 => Self::Other,
            2 => Self::Contact,
            3 => Self::OwnChat,
            _ => return None,
        })
    }
}

/// RL2: relay storage cap for the device's storage situation. 500 MB while
/// free storage is at least 10 % of the total; below that the cap shrinks in
/// proportion (`500 MB × free / (10 % of total)`), reaching 0 when the disk
/// is full. `total_bytes == 0` (unknown) keeps the full cap.
pub fn auto_storage_cap(free_bytes: u64, total_bytes: u64) -> u64 {
    let threshold = u128::from(total_bytes) * u128::from(LOW_STORAGE_PERCENT) / 100;
    if total_bytes == 0 || u128::from(free_bytes) >= threshold {
        return RELAY_CAP_BYTES;
    }
    (u128::from(RELAY_CAP_BYTES) * u128::from(free_bytes) / threshold) as u64
}

/// Storage / forwarding limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayBudget {
    pub storage_bytes: u64,
    /// `None` = unlimited.
    pub daily_forward_bytes: Option<u64>,
}

impl RelayBudget {
    /// RL2 budget: the automatic 500 MB storage cap (no user setting), with
    /// the §22.4 daily forwarding budget.
    pub fn automatic() -> Self {
        Self {
            storage_bytes: RELAY_CAP_BYTES,
            daily_forward_bytes: Some(DAILY_FORWARD_BYTES),
        }
    }

    /// Legacy §22.4 presets (simulations only; the engine uses
    /// [`Self::automatic`]). Defaults: 50 MB / 20 MB-per-day on battery or cellular; 500 MB /
    /// unlimited on Wi-Fi while charging.
    pub fn for_conditions(net: NetworkKind, charging: bool) -> Self {
        let generous = charging && matches!(net, NetworkKind::Wifi { metered: false });
        if generous {
            Self {
                storage_bytes: 500 * MB,
                daily_forward_bytes: None,
            }
        } else {
            Self {
                storage_bytes: 50 * MB,
                daily_forward_bytes: Some(20 * MB),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEnvelope {
    pub id: [u8; 16],
    pub size: u64,
    /// RL2 priority class.
    pub class: RelayClass,
    /// Age key: the envelope's `created_ms`, clamped to the time it was
    /// stored (a future timestamp cannot make an envelope look newer).
    pub created_ms: u64,
    pub expires_at: u64,
    /// Hops already travelled.
    pub hop_count: u8,
}

impl StoredEnvelope {
    /// Eviction key (RL2): smallest is evicted first — lowest class first
    /// (news, other, contacts, own chats), then oldest, then most hops
    /// travelled, then id (deterministic).
    pub fn evict_key(&self) -> (RelayClass, u64, std::cmp::Reverse<u8>, [u8; 16]) {
        (
            self.class,
            self.created_ms,
            std::cmp::Reverse(self.hop_count),
            self.id,
        )
    }
}

/// Clamp an expiry: default 72 h, max 7 d. Burn envelopes keep their own
/// (earlier) `expires_at`.
pub fn clamp_expiry(now_ms: u64, requested: Option<u64>, burn: bool) -> u64 {
    let max = now_ms + MAX_EXPIRY_MS;
    match requested {
        // Burn envelopes always carry their own expiry; others may too.
        Some(t) => t.min(max),
        None if burn => now_ms,
        None => now_ms + DEFAULT_EXPIRY_MS,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Stored {
        evicted: Vec<[u8; 16]>,
    },
    /// Would have been the first victim itself, or larger than the budget.
    Rejected,
}

/// Store-and-forward buffer that never exceeds its budget.
#[derive(Debug)]
pub struct RelayStore {
    budget: RelayBudget,
    items: HashMap<[u8; 16], StoredEnvelope>,
    used: u64,
    day: u64,
    forwarded_today: u64,
}

impl RelayStore {
    pub fn new(budget: RelayBudget) -> Self {
        Self {
            budget,
            items: HashMap::new(),
            used: 0,
            day: 0,
            forwarded_today: 0,
        }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    /// Bytes used by envelopes of one class.
    pub fn used_by(&self, class: RelayClass) -> u64 {
        self.items
            .values()
            .filter(|e| e.class == class)
            .map(|e| e.size)
            .sum()
    }

    pub fn forwarded_today(&self) -> u64 {
        self.forwarded_today
    }

    pub fn budget(&self) -> RelayBudget {
        self.budget
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Change budget (e.g. unplugged); evicts down to the new cap.
    pub fn set_budget(&mut self, b: RelayBudget) -> Vec<[u8; 16]> {
        self.budget = b;
        let mut ev = Vec::new();
        while self.used > b.storage_bytes {
            ev.push(self.evict_one().expect("used > 0 implies items"));
        }
        ev
    }

    fn evict_one(&mut self) -> Option<[u8; 16]> {
        let id = self.items.values().min_by_key(|e| e.evict_key())?.id;
        let e = self.items.remove(&id)?;
        self.used -= e.size;
        Some(id)
    }

    pub fn purge_expired(&mut self, now_ms: u64) -> usize {
        let before = self.items.len();
        self.items.retain(|_, e| e.expires_at > now_ms);
        self.used = self.items.values().map(|e| e.size).sum();
        before - self.items.len()
    }

    pub fn insert(&mut self, env: StoredEnvelope, now_ms: u64) -> InsertOutcome {
        self.purge_expired(now_ms);
        if env.size > self.budget.storage_bytes
            || env.expires_at <= now_ms
            || self.items.contains_key(&env.id)
        {
            return InsertOutcome::Rejected;
        }
        // Would the newcomer be evicted before everything needed to make room?
        if self.used + env.size > self.budget.storage_bytes {
            let mut sorted: Vec<&StoredEnvelope> = self.items.values().collect();
            sorted.sort_by_key(|e| e.evict_key());
            let mut free = self.budget.storage_bytes - self.used;
            for e in sorted {
                if free >= env.size {
                    break;
                }
                if env.evict_key() <= e.evict_key() {
                    return InsertOutcome::Rejected;
                }
                free += e.size;
            }
        }
        let mut evicted = Vec::new();
        while self.used + env.size > self.budget.storage_bytes {
            evicted.push(self.evict_one().expect("room computed above"));
        }
        self.used += env.size;
        self.items.insert(env.id, env);
        InsertOutcome::Stored { evicted }
    }

    pub fn take(&mut self, id: &[u8; 16]) -> Option<StoredEnvelope> {
        let e = self.items.remove(id)?;
        self.used -= e.size;
        Some(e)
    }

    /// Account forwarding `bytes`; refuses when the daily cap would be exceeded.
    pub fn try_forward(&mut self, bytes: u64, now_ms: u64) -> Result<()> {
        let day = now_ms / DAY_MS;
        if day != self.day {
            self.day = day;
            self.forwarded_today = 0;
        }
        if let Some(cap) = self.budget.daily_forward_bytes {
            if self.forwarded_today + bytes > cap {
                return Err(OmcpError::Sync("daily forward budget exhausted"));
            }
        }
        self.forwarded_today += bytes;
        Ok(())
    }
}

/// How much a device may sync over the Internet on the current link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncScope {
    /// Unmetered: everything (subject to budgets).
    Full,
    /// Cellular with mobile data allowed: only pending outbound and the
    /// device's own chats.
    OwnOnly,
    None,
}

pub fn internet_sync_scope(net: NetworkKind, allow_mobile_data: bool) -> SyncScope {
    match net {
        NetworkKind::Offline => SyncScope::None,
        NetworkKind::Wifi { metered: false } => SyncScope::Full,
        // Cellular (metered or not) and metered Wi-Fi count as mobile data.
        _ if allow_mobile_data => SyncScope::OwnOnly,
        _ => SyncScope::None,
    }
}

/// May this envelope be synced over the Internet under `scope`?
pub fn may_sync(scope: SyncScope, pending_outbound: bool, is_member: bool) -> bool {
    match scope {
        SyncScope::Full => true,
        SyncScope::OwnOnly => pending_outbound || is_member,
        SyncScope::None => false,
    }
}

/// Where to send an envelope (§22.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// One upload to the gateway rendezvous mailbox for this label.
    Rendezvous { label: [u8; 32] },
    /// BLE gossip fallback.
    BleGossip,
}

/// Prefer the gateway rendezvous mailbox when any gateway is reachable;
/// the label is `HKDF-SHA-384(tag_secret, "OMCP-rdv" || epoch)` (§22.7(3)).
pub fn choose_route(gateway_reachable: bool, tag_secret: &[u8], tag_epoch: u64) -> Result<Route> {
    Ok(if gateway_reachable {
        Route::Rendezvous {
            label: rendezvous_label(tag_secret, tag_epoch)?,
        }
    } else {
        Route::BleGossip
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `t` is both the age key and the expiry.
    fn env(i: u8, size: u64, prio: bool, t: u64, hops: u8) -> StoredEnvelope {
        StoredEnvelope {
            id: [i; 16],
            size,
            class: if prio {
                RelayClass::OwnChat
            } else {
                RelayClass::Contact
            },
            created_ms: t,
            expires_at: t,
            hop_count: hops,
        }
    }

    fn cls(i: u8, size: u64, class: RelayClass, created: u64) -> StoredEnvelope {
        StoredEnvelope {
            id: [i; 16],
            size,
            class,
            created_ms: created,
            expires_at: u64::MAX,
            hop_count: 0,
        }
    }

    #[test]
    fn rl2_automatic_cap_is_500_mb() {
        let b = RelayBudget::automatic();
        assert_eq!(b.storage_bytes, 500 * MB);
        assert_eq!(b.storage_bytes, RELAY_CAP_BYTES);
        assert_eq!(b.daily_forward_bytes, Some(DAILY_FORWARD_BYTES));
    }

    #[test]
    fn rl2_cap_lowered_when_free_storage_under_10_percent() {
        let gb = 1_000 * MB;
        // ≥ 10 % free: full cap.
        assert_eq!(auto_storage_cap(64 * gb, 128 * gb), RELAY_CAP_BYTES);
        assert_eq!(auto_storage_cap(128 * gb / 10, 128 * gb), RELAY_CAP_BYTES);
        // Unknown total: full cap.
        assert_eq!(auto_storage_cap(0, 0), RELAY_CAP_BYTES);
        // Under 10 %: lowered in proportion, 0 when full.
        assert_eq!(auto_storage_cap(5 * gb, 100 * gb), RELAY_CAP_BYTES / 2);
        assert_eq!(auto_storage_cap(gb, 100 * gb), RELAY_CAP_BYTES / 10);
        assert!(auto_storage_cap(10 * gb - 1, 100 * gb) < RELAY_CAP_BYTES);
        assert_eq!(auto_storage_cap(0, 100 * gb), 0);
        // Monotone in free space.
        let mut last = 0;
        for f in 0..=20 {
            let c = auto_storage_cap(f * gb / 2, 100 * gb);
            assert!(c >= last);
            last = c;
        }
    }

    #[test]
    fn rl2_full_store_drops_by_priority_then_oldest() {
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 400,
            daily_forward_bytes: None,
        });
        // Own chats (newest of all, still kept), contact, other, news.
        for e in [
            cls(1, 100, RelayClass::OwnChat, 50),
            cls(2, 100, RelayClass::Contact, 10),
            cls(3, 100, RelayClass::Other, 20),
            cls(4, 100, RelayClass::News, 5),
        ] {
            assert!(matches!(s.insert(e, 0), InsertOutcome::Stored { .. }));
        }
        // News goes first, then everything else, then contacts.
        let order: Vec<[u8; 16]> = [5u8, 6, 7]
            .iter()
            .flat_map(
                |i| match s.insert(cls(*i, 100, RelayClass::OwnChat, 100 + *i as u64), 0) {
                    InsertOutcome::Stored { evicted } => evicted,
                    InsertOutcome::Rejected => panic!("own chats outrank the rest"),
                },
            )
            .collect();
        assert_eq!(order, vec![[4; 16], [3; 16], [2; 16]]);
        // Only own chats left: the oldest own envelope goes first.
        assert_eq!(
            s.insert(cls(8, 100, RelayClass::OwnChat, 200), 0),
            InsertOutcome::Stored {
                evicted: vec![[1; 16]]
            }
        );
        // A lower class never pushes out a higher one.
        assert_eq!(
            s.insert(cls(9, 100, RelayClass::Contact, 300), 0),
            InsertOutcome::Rejected
        );
        assert_eq!(s.used_by(RelayClass::OwnChat), 400);
        // Within one class: oldest first.
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 200,
            daily_forward_bytes: None,
        });
        s.insert(cls(1, 100, RelayClass::Contact, 30), 0);
        s.insert(cls(2, 100, RelayClass::Contact, 10), 0);
        assert_eq!(
            s.insert(cls(3, 100, RelayClass::Contact, 40), 0),
            InsertOutcome::Stored {
                evicted: vec![[2; 16]]
            }
        );
        // A full store refuses a newcomer that would be the first victim.
        assert_eq!(
            s.insert(cls(4, 50, RelayClass::News, 99), 0),
            InsertOutcome::Rejected
        );
        // Lowering the cap (RL2 low storage) evicts in the same order.
        assert_eq!(
            s.set_budget(RelayBudget {
                storage_bytes: 100,
                daily_forward_bytes: None
            }),
            vec![[1; 16]]
        );
    }

    #[test]
    fn defaults() {
        let b = RelayBudget::for_conditions(NetworkKind::Cellular { metered: true }, true);
        assert_eq!(b.storage_bytes, 50 * MB);
        assert_eq!(b.daily_forward_bytes, Some(20 * MB));
        let b = RelayBudget::for_conditions(NetworkKind::Wifi { metered: false }, true);
        assert_eq!(b.storage_bytes, 500 * MB);
        assert_eq!(b.daily_forward_bytes, None);
        let b = RelayBudget::for_conditions(NetworkKind::Wifi { metered: false }, false);
        assert_eq!(b.storage_bytes, 50 * MB);
    }

    #[test]
    fn eviction_order() {
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 300,
            daily_forward_bytes: Some(1000),
        });
        assert!(matches!(
            s.insert(env(1, 100, true, 50, 0), 0),
            InsertOutcome::Stored { .. }
        ));
        assert!(matches!(
            s.insert(env(2, 100, false, 90, 1), 0),
            InsertOutcome::Stored { .. }
        ));
        assert!(matches!(
            s.insert(env(3, 100, false, 90, 5), 0),
            InsertOutcome::Stored { .. }
        ));
        // Same class and age: more hops goes first.
        assert_eq!(
            s.insert(env(4, 100, false, 95, 0), 0),
            InsertOutcome::Stored {
                evicted: vec![[3; 16]]
            }
        );
        // A newcomer that would itself be first victim is rejected.
        assert_eq!(
            s.insert(env(5, 100, false, 10, 0), 0),
            InsertOutcome::Rejected
        );
        assert_eq!(s.used(), 300);
        assert_eq!(
            s.insert(env(6, 1000, true, 99, 0), 0),
            InsertOutcome::Rejected
        );
        assert_eq!(s.purge_expired(60), 1);
        assert_eq!(s.used(), 200);
        assert_eq!(
            s.set_budget(RelayBudget {
                storage_bytes: 100,
                daily_forward_bytes: None
            }),
            vec![[2; 16]]
        );
    }

    #[test]
    fn forward_cap_and_expiry() {
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 1,
            daily_forward_bytes: Some(100),
        });
        s.try_forward(60, 0).unwrap();
        assert!(s.try_forward(60, 1).is_err());
        s.try_forward(60, DAY_MS).unwrap();
        assert_eq!(clamp_expiry(0, None, false), DEFAULT_EXPIRY_MS);
        assert_eq!(clamp_expiry(0, Some(u64::MAX), false), MAX_EXPIRY_MS);
        assert_eq!(clamp_expiry(0, Some(5), true), 5);
    }

    #[test]
    fn metered_rules_and_offload() {
        use NetworkKind::*;
        assert_eq!(
            internet_sync_scope(Wifi { metered: false }, false),
            SyncScope::Full
        );
        assert_eq!(
            internet_sync_scope(Cellular { metered: true }, false),
            SyncScope::None
        );
        assert_eq!(
            internet_sync_scope(Cellular { metered: false }, true),
            SyncScope::OwnOnly
        );
        assert_eq!(
            internet_sync_scope(Wifi { metered: true }, true),
            SyncScope::OwnOnly
        );
        assert_eq!(internet_sync_scope(Offline, true), SyncScope::None);
        assert!(may_sync(SyncScope::OwnOnly, true, false));
        assert!(!may_sync(SyncScope::OwnOnly, false, false));
        assert!(matches!(
            choose_route(true, &[1; 32], 3).unwrap(),
            Route::Rendezvous { .. }
        ));
        assert_eq!(choose_route(false, &[1; 32], 3).unwrap(), Route::BleGossip);
    }
}
