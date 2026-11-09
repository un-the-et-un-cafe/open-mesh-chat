//! §23.3 note (2026-11-09): reputation / attestations are deferred past v1.0.
//! v1.0 MUST NOT send attestations, endorsements, gossiped `REPORT{}` frames
//! or `REPUTATION_SUMMARY`, and receivers MUST ignore them.

/// Wire frame kinds that v1.0 never emits and always ignores on receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeferredReputationKind {
    Attestation = 0xA1,
    Endorsement = 0xA2,
    GossipedReport = 0xA3,
    ReputationSummary = 0xA4,
}

impl DeferredReputationKind {
    pub fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0xA1 => Self::Attestation,
            0xA2 => Self::Endorsement,
            0xA3 => Self::GossipedReport,
            0xA4 => Self::ReputationSummary,
            _ => return None,
        })
    }
}

/// v1.0 send path: always refused.
pub fn refuse_send(_kind: DeferredReputationKind) -> Result<(), &'static str> {
    Err("v1.0 must not send reputation attestations")
}

/// v1.0 receive path: ignore (do not apply, do not relay as reputation).
pub fn ignore_receive(payload: &[u8]) -> bool {
    payload
        .first()
        .copied()
        .and_then(DeferredReputationKind::from_byte)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_never_sends_or_applies_reputation_frames() {
        for k in [
            DeferredReputationKind::Attestation,
            DeferredReputationKind::Endorsement,
            DeferredReputationKind::GossipedReport,
            DeferredReputationKind::ReputationSummary,
        ] {
            assert!(refuse_send(k).is_err());
            assert!(ignore_receive(&[k as u8, 1, 2, 3]));
        }
        assert!(!ignore_receive(&[0x01, 1, 2])); // ordinary evidence / other
        assert!(!ignore_receive(&[]));
    }
}
