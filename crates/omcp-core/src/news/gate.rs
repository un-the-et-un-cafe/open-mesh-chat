//! Fetch gate for News: the existing gateway rules apply to RSS fetches
//! (500 MB/day cap shared accounting, Wi-Fi-only by default, metered
//! networks warn but are not blocked when Wi-Fi-only is off).

use crate::gateway::policy::{GatewayPolicy, NetworkKind, DAY_MS};

/// Why a fetch round is not allowed now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchBlock {
    Offline,
    WifiOnly,
    DailyCapReached,
}

/// Outcome of [`FetchGate::decide`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchDecision {
    pub allowed: bool,
    pub metered_warning: bool,
    pub blocked: Option<FetchBlock>,
}

/// Per-day byte accounting for News fetches under a [`GatewayPolicy`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchGate {
    day: u64,
    used: u64,
}

impl FetchGate {
    fn roll(&mut self, now_ms: u64) {
        let d = now_ms / DAY_MS;
        if d != self.day {
            self.day = d;
            self.used = 0;
        }
    }

    /// Bytes counted today.
    pub fn used_today(&mut self, now_ms: u64) -> u64 {
        self.roll(now_ms);
        self.used
    }

    /// May this phone fetch News feeds now?
    pub fn decide(&mut self, policy: &GatewayPolicy, net: NetworkKind, now_ms: u64) -> FetchDecision {
        self.roll(now_ms);
        let d = policy.evaluate(net);
        let blocked = if !d.bridge {
            Some(if net == NetworkKind::Offline {
                FetchBlock::Offline
            } else {
                FetchBlock::WifiOnly
            })
        } else if self.used >= policy.daily_byte_cap {
            Some(FetchBlock::DailyCapReached)
        } else {
            None
        };
        FetchDecision {
            allowed: blocked.is_none(),
            metered_warning: d.metered_warning && blocked.is_none(),
            blocked,
        }
    }

    /// Count a fetched feed body against the daily cap.
    pub fn record(&mut self, bytes: u64, now_ms: u64) {
        self.roll(now_ms);
        self.used = self.used.saturating_add(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_fetch_gate_wifi_only_metered_warning_and_daily_cap() {
        let mut g = FetchGate::default();
        let p = GatewayPolicy::default();
        let t = 10 * DAY_MS + 5;
        assert_eq!(g.decide(&p, NetworkKind::Offline, t).blocked, Some(FetchBlock::Offline));
        let d = g.decide(&p, NetworkKind::Cellular { metered: true }, t);
        assert_eq!(d.blocked, Some(FetchBlock::WifiOnly));
        assert!(g.decide(&p, NetworkKind::Wifi { metered: false }, t).allowed);
        let p2 = GatewayPolicy { wifi_only: false, ..p.clone() };
        let d = g.decide(&p2, NetworkKind::Cellular { metered: true }, t);
        assert!(d.allowed && d.metered_warning, "metered warns, never blocks");
        g.record(500_000_000, t);
        let d = g.decide(&p, NetworkKind::Wifi { metered: false }, t);
        assert_eq!(d.blocked, Some(FetchBlock::DailyCapReached));
        assert!(g.decide(&p, NetworkKind::Wifi { metered: false }, t + DAY_MS).allowed, "cap resets daily");
    }
}
