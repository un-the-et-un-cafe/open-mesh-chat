//! Online video routing (no server): the host keeps the [`ViewerTree`] and
//! relays sealed 0x50 signals between viewers, who need not be contacts of
//! each other. Viewers address peers by the host-assigned id; `[0; 32]` is
//! the host. Media then flows peer-to-peer (WebRTC, STUN only, SFrame E2EE).

use super::signal::{RtcSignal, SignalKind};
use super::tree::{PeerId, ViewerTree};

/// The host's own id inside its tree and in signals.
pub const HOST: PeerId = [0; 32];
pub const PARKED: &str = "parked";

/// Where a signal goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// Our own app (host side).
    Local,
    /// A contact (viewer), sent over the E2EE chat.
    Contact(PeerId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routed {
    pub dest: Dest,
    /// Peer the receiving app talks to (offer/answer/ICE counterpart).
    pub peer: PeerId,
    pub kind: SignalKind,
    pub body: String,
}

/// Host-side router.
#[derive(Debug, Clone)]
pub struct VideoRouter {
    pub tree: ViewerTree,
}

impl Default for VideoRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoRouter {
    pub fn new() -> Self {
        Self {
            tree: ViewerTree::new(HOST),
        }
    }

    fn assign(parent: PeerId, child: PeerId) -> Routed {
        Routed {
            dest: if parent == HOST {
                Dest::Local
            } else {
                Dest::Contact(parent)
            },
            peer: child,
            kind: SignalKind::Assign,
            body: String::new(),
        }
    }

    fn parked(v: PeerId) -> Routed {
        Routed {
            dest: Dest::Contact(v),
            peer: HOST,
            kind: SignalKind::Bye,
            body: PARKED.into(),
        }
    }

    /// A signal from authenticated viewer `from` addressed to `to` (HOST = us).
    pub fn on_signal(
        &mut self,
        from: PeerId,
        to: PeerId,
        kind: SignalKind,
        body: String,
    ) -> Vec<Routed> {
        if from == HOST {
            return Vec::new();
        }
        match kind {
            SignalKind::Join => {
                if self.tree.depth(&from).is_some() {
                    return Vec::new(); // already placed
                }
                match self.tree.join(from, true) {
                    Some(p) => vec![Self::assign(p, from)],
                    None => vec![Self::parked(from)],
                }
            }
            SignalKind::Bye if to == HOST => self.leave(&from),
            SignalKind::Assign => Vec::new(), // only the host assigns
            _ => {
                // Offer / Answer / ICE / Bye between tree neighbours only.
                let linked =
                    self.tree.parent(&from) == Some(to) || self.tree.parent(&to) == Some(from);
                if !linked {
                    return Vec::new();
                }
                let dest = if to == HOST {
                    Dest::Local
                } else {
                    Dest::Contact(to)
                };
                vec![Routed {
                    dest,
                    peer: from,
                    kind,
                    body,
                }]
            }
        }
    }

    /// Viewer gone (left, removed, or ICE failed for good): re-parent its children.
    pub fn leave(&mut self, v: &PeerId) -> Vec<Routed> {
        self.tree
            .leave(v)
            .into_iter()
            .map(|r| match r.new_parent {
                Some(p) => Self::assign(p, r.viewer),
                None => Self::parked(r.viewer),
            })
            .collect()
    }

    /// Host's own signal to a direct child.
    pub fn from_host(&self, to: PeerId, kind: SignalKind, body: String) -> Option<Routed> {
        (self.tree.parent(&to) == Some(HOST)).then_some(Routed {
            dest: Dest::Contact(to),
            peer: HOST,
            kind,
            body,
        })
    }
}

/// Build the wire signal (sealed by the caller with the stream key).
pub fn wire(
    stream_id: [u8; 16],
    from: PeerId,
    to: PeerId,
    kind: SignalKind,
    body: String,
) -> RtcSignal {
    RtcSignal {
        stream_id,
        from,
        to,
        kind,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{StreamKey, HOST_FANOUT, VIEWER_FANOUT};

    fn v(i: u8) -> PeerId {
        let mut p = [0u8; 32];
        p[0] = i;
        p[31] = 1;
        p
    }

    #[test]
    fn fanout_limits_6_and_4() {
        let mut r = VideoRouter::new();
        let mut local = 0;
        for i in 1..=40u8 {
            let out = r.on_signal(v(i), HOST, SignalKind::Join, String::new());
            assert_eq!(out.len(), 1);
            if out[0].dest == Dest::Local {
                local += 1;
            }
        }
        assert_eq!(local, HOST_FANOUT);
        assert_eq!(r.tree.children(&HOST).len(), HOST_FANOUT);
        for i in 1..=40u8 {
            assert!(r.tree.children(&v(i)).len() <= VIEWER_FANOUT);
        }
        assert!(r.tree.check());
        // Duplicate join is ignored.
        assert!(r
            .on_signal(v(1), HOST, SignalKind::Join, String::new())
            .is_empty());
    }

    #[test]
    fn signals_only_between_tree_neighbours() {
        let mut r = VideoRouter::new();
        for i in 1..=7u8 {
            r.on_signal(v(i), HOST, SignalKind::Join, String::new());
        }
        let child = *r.tree.children(&v(1)).first().expect("v7 under a viewer");
        let parent = r.tree.parent(&child).unwrap();
        let o = r.on_signal(parent, child, SignalKind::Offer, "sdp".into());
        assert_eq!(
            o,
            vec![Routed {
                dest: Dest::Contact(child),
                peer: parent,
                kind: SignalKind::Offer,
                body: "sdp".into()
            }]
        );
        let a = r.on_signal(child, parent, SignalKind::Answer, "ans".into());
        assert_eq!(a[0].dest, Dest::Contact(parent));
        // Not linked: dropped.
        assert!(r
            .on_signal(v(2), v(3), SignalKind::Offer, "x".into())
            .is_empty());
        // Viewers cannot assign.
        assert!(r
            .on_signal(v(2), HOST, SignalKind::Assign, String::new())
            .is_empty());
        // Host's direct child answering the host goes Local.
        assert_eq!(
            r.on_signal(v(1), HOST, SignalKind::Answer, "a".into())[0].dest,
            Dest::Local
        );
        assert!(r.from_host(v(1), SignalKind::Offer, "o".into()).is_some());
        assert!(r.from_host(child, SignalKind::Offer, "o".into()).is_none());
    }

    #[test]
    fn leave_reparents_and_full_tree_parks() {
        let mut r = VideoRouter::with_small();
        for i in 1..=3u8 {
            r.on_signal(v(i), HOST, SignalKind::Join, String::new());
        }
        // fan-out 1/1: line host-v1-v2-v3; a 4th is parked? (line can grow) – fill check:
        let parent_of_v3 = r.tree.parent(&v(3)).unwrap();
        let out = r.on_signal(parent_of_v3, HOST, SignalKind::Bye, String::new());
        assert!(!out.is_empty());
        assert!(out
            .iter()
            .all(|o| o.kind == SignalKind::Assign || o.body == PARKED));
        assert!(r.tree.check());
    }

    #[test]
    fn parked_when_no_forwarders() {
        let mut r = VideoRouter {
            tree: ViewerTree::with_fanout(HOST, 1, 0),
        };
        assert_eq!(
            r.on_signal(v(1), HOST, SignalKind::Join, String::new())[0].kind,
            SignalKind::Assign
        );
        let o = r.on_signal(v(2), HOST, SignalKind::Join, String::new());
        assert_eq!(
            o,
            vec![Routed {
                dest: Dest::Contact(v(2)),
                peer: HOST,
                kind: SignalKind::Bye,
                body: PARKED.into()
            }]
        );
    }

    #[test]
    fn signal_seal_roundtrip_all_kinds() {
        let k = StreamKey::new([7; 16], 1, [2; 32]);
        for kind in [
            SignalKind::Offer,
            SignalKind::Answer,
            SignalKind::Ice,
            SignalKind::Bye,
            SignalKind::Join,
            SignalKind::Assign,
        ] {
            let s = wire(
                [7; 16],
                v(1),
                HOST,
                kind,
                "candidate:1 1 udp 2122260223 192.0.2.1 5000 typ host".into(),
            );
            let b = s.seal(&k).unwrap();
            assert_eq!(b[0], 0x50);
            assert_eq!(RtcSignal::open(&k, &b).unwrap(), s);
            assert!(RtcSignal::open(&StreamKey::new([7; 16], 1, [3; 32]), &b).is_err());
            assert!(RtcSignal::open(&StreamKey::new([7; 16], 2, [2; 32]), &b).is_err());
        }
        let big = wire(
            [7; 16],
            v(1),
            HOST,
            SignalKind::Offer,
            "x".repeat(17 * 1024),
        );
        assert!(big.seal(&k).is_err());
    }

    impl VideoRouter {
        fn with_small() -> Self {
            Self {
                tree: ViewerTree::with_fanout(HOST, 1, 1),
            }
        }
    }
}
