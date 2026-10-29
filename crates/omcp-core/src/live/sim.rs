//! Live tests: packets, keys, multi-hop mesh simulation (LV1, LV2, LV4),
//! viewer tree and re-parenting, mode switching (LV3), signalling.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

use super::audio::*;
use super::host::*;
use super::jitter::*;
use super::mode::*;
use super::relay::*;
use super::signal::*;
use super::tree::*;
use super::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}

fn opus_frame(i: u64) -> Vec<u8> {
    // 12 kbit/s × 20 ms = 30 bytes.
    let mut f = vec![0u8; 30];
    f[..8].copy_from_slice(&i.to_be_bytes());
    f
}

#[test]
fn audio_packet_round_trip_and_hop_limit() {
    let key = StreamKey::random([1; 16], 0).unwrap();
    let frames: Vec<_> = (0..5).map(opus_frame).collect();
    let p = seal_audio(&key, 7, 3, 1000, &frames).unwrap();
    assert!(is_live_audio(&p));
    let (h, got) = open_audio(&key, &p).unwrap();
    assert_eq!((h.seq, h.hop, h.max_hops), (7, 1, 3));
    assert_eq!(got, frames);
    let p2 = next_hop(&p).unwrap().unwrap();
    let p3 = next_hop(&p2).unwrap().unwrap();
    assert_eq!(AudioHeader::parse(&p3).unwrap().hop, 3);
    assert!(next_hop(&p3).unwrap().is_none(), "3 hops max");
    // Relays bump the hop byte; listeners still open it.
    assert_eq!(open_audio(&key, &p3).unwrap().1, frames);
    // 12 kbit/s: 5 × 30 B Opus + header + AEAD stays under one 247 B BLE PDU budget.
    assert!(p.len() < 247, "{}", p.len());
}

#[test]
fn lv4_non_member_relay_cannot_decrypt_or_tamper() {
    let key = StreamKey::random([2; 16], 0).unwrap();
    let other = StreamKey::random([2; 16], 0).unwrap(); // same stream id, wrong secret
    let p = seal_audio(&key, 0, 3, 0, &[opus_frame(0)]).unwrap();
    assert!(open_audio(&other, &p).is_err());
    let mut t = p.clone();
    let n = t.len();
    t[n - 1] ^= 1;
    assert!(open_audio(&key, &t).is_err());
    let mut seq = p.clone();
    seq[17] ^= 1; // header is authenticated (except hop)
    assert!(open_audio(&key, &seq).is_err());
    // The non-member relay still forwards it.
    let mut r = LiveRelay::default();
    assert!(r.on_packet(&p, 10).forward.is_some());
}

#[test]
fn key_rotation_on_listener_removal() {
    let mut h = HostSession::start([9; 32], "Town hall", false, 0).unwrap();
    h.add_listener([1; 32]);
    h.add_listener([2; 32]);
    let old = h.invite(true);
    let mut removed = ListenerSession::new(old.clone(), false);
    let mut kept = ListenerSession::new(old, false);
    let notify = h.remove_listener(&[1; 32]).unwrap();
    assert_eq!(notify, vec![[2; 32]]);
    assert_eq!(h.key().key_epoch, 1);
    kept.update(h.invite(true));
    let pkts = h.push_frames((0..5).map(opus_frame).collect(), 0).unwrap();
    assert_eq!(pkts.len(), 1);
    assert!(kept.on_packet(&pkts[0], 0));
    assert!(
        !removed.on_packet(&pkts[0], 0),
        "removed listener is locked out"
    );
    let inv = h.invite(true);
    assert_eq!(LiveInvite::decode(&inv.encode().unwrap()).unwrap(), inv);
}

#[test]
fn relay_dedups_expires_and_never_stores() {
    let key = StreamKey::random([3; 16], 0).unwrap();
    let p = seal_audio(&key, 1, 3, 10_000, &[opus_frame(0)]).unwrap();
    let mut r = LiveRelay::default();
    assert!(r.on_packet(&p, 10_100).deliver);
    assert!(!r.on_packet(&p, 10_200).deliver, "duplicate");
    let old = seal_audio(&key, 2, 3, 10_000, &[opus_frame(0)]).unwrap();
    assert!(!r.on_packet(&old, 15_001).deliver, "older than 5 s");
    let future = seal_audio(&key, 3, 3, 20_000, &[opus_frame(0)]).unwrap();
    assert!(!r.on_packet(&future, 10_000).deliver);
    assert!(!r.on_packet(b"\x4cjunk", 0).deliver);
}

#[test]
fn jitter_buffer_reorders_conceals_and_drops_late() {
    let mut j = JitterBuffer::new(100);
    j.push(10, vec![10], 0); // late joiner starts at 10
    j.push(12, vec![12], 5);
    j.push(13, vec![13], 5);
    assert!(j.pop_due(50).is_empty());
    let out = j.pop_due(160);
    assert_eq!(
        out,
        vec![
            Playout::Frame {
                index: 10,
                opus: vec![10]
            },
            Playout::Lost {
                index: 11,
                next: Some(vec![12])
            },
            Playout::Frame {
                index: 12,
                opus: vec![12]
            },
            Playout::Frame {
                index: 13,
                opus: vec![13]
            },
        ]
    );
    j.push(11, vec![11], 170);
    assert_eq!(j.late, 1);
    j.push(5, vec![5], 170);
    assert_eq!(j.late, 2, "frames before the join point are ignored");
    assert!((j.loss_ratio() - 0.25).abs() < 1e-9);
}

/// LV1: host audio over a lossy BLE mesh. Layers: host → L1 → L2 → L3 → L4,
/// every node linked to every node of the next layer (2 per layer), 8% loss
/// per link, 1.0–1.5 s per hop. Listeners at 3 hops play with < 6 s delay and
/// < 5% lost frames; the 4th hop gets nothing (hop limit). L2 relays are not
/// members and cannot decrypt (LV4) but still forward.
#[test]
fn lv1_three_hop_mesh_simulation() {
    let mut rng = Rng(42);
    let layers: Vec<Vec<usize>> = vec![vec![0], vec![1, 2], vec![3, 4], vec![5, 6], vec![7]];
    let mut nbr: HashMap<usize, Vec<usize>> = HashMap::new();
    for w in layers.windows(2) {
        for a in &w[0] {
            for b in &w[1] {
                nbr.entry(*a).or_default().push(*b);
                nbr.entry(*b).or_default().push(*a);
            }
        }
    }
    // Also a link inside layers (BLE neighbours hear each other).
    for l in &layers[1..] {
        if l.len() == 2 {
            nbr.entry(l[0]).or_default().push(l[1]);
            nbr.entry(l[1]).or_default().push(l[0]);
        }
    }
    let mut host = HostSession::start([0; 32], "sim", false, 0).unwrap();
    let inv = host.invite(true);
    let members: BTreeSet<usize> = [1, 2, 5, 6, 7].into();
    let mut relays: HashMap<usize, LiveRelay> = (1..8).map(|n| (n, LiveRelay::default())).collect();
    let mut listeners: HashMap<usize, ListenerSession> = members
        .iter()
        .map(|n| (*n, ListenerSession::new(inv.clone(), false)))
        .collect();
    type Ev = Reverse<(u64, usize, usize, Vec<u8>)>;
    let mut q: BinaryHeap<Ev> = BinaryHeap::new();
    let send = |q: &mut BinaryHeap<_>,
                rng: &mut Rng,
                t: u64,
                from: usize,
                pkt: &Vec<u8>,
                nbr: &HashMap<usize, Vec<usize>>| {
        for to in &nbr[&from] {
            if rng.f() < 0.08 {
                continue;
            }
            q.push(Reverse((t + rng.range(1000, 1500), *to, from, pkt.clone())));
        }
    };
    let secs = 30;
    let mut sent_at: HashMap<u32, u64> = HashMap::new();
    for i in 0..(secs * 1000 / 100) as u64 {
        let t = i * 100;
        let frames = (0..5).map(|k| opus_frame(i * 5 + k)).collect();
        for p in host.push_frames(frames, t).unwrap() {
            sent_at.insert(AudioHeader::parse(&p).unwrap().seq, t);
            send(&mut q, &mut rng, t, 0, &p, &nbr);
        }
    }
    let mut not_opened_by_nonmember = 0;
    let mut delays: HashMap<usize, Vec<u64>> = HashMap::new();
    while let Some(Reverse((t, node, _from, pkt))) = q.pop() {
        if node == 0 {
            continue;
        }
        let d = relays.get_mut(&node).unwrap().on_packet(&pkt, t);
        if !d.deliver {
            continue;
        }
        if let Some(l) = listeners.get_mut(&node) {
            assert!(l.on_packet(&pkt, t));
            let seq = AudioHeader::parse(&pkt).unwrap().seq;
            for k in 0..5u64 {
                let idx = seq as u64 * 5 + k;
                if let Some(po) = l.jitter.playout_ms(idx) {
                    if po >= t {
                        delays.entry(node).or_default().push(po - sent_at[&seq]);
                    }
                }
            }
            l.jitter.pop_due(t);
        } else {
            let mut stranger = ListenerSession::new(
                LiveInvite {
                    key: StreamKey::random(inv.key.stream_id, 0).unwrap(),
                    ..inv.clone()
                },
                false,
            );
            if !stranger.on_packet(&pkt, t) {
                not_opened_by_nonmember += 1;
            }
        }
        if let Some(f) = d.forward {
            send(&mut q, &mut rng, t, node, &f, &nbr);
        }
    }
    assert!(not_opened_by_nonmember > 0);
    let last = (secs * 1000 / 100) as u64 * 5 - 1;
    for l in listeners.values_mut() {
        if let Some(t) = l.jitter.playout_ms(last) {
            l.jitter.pop_due(t);
        }
    }
    for n in [5usize, 6] {
        let l = &listeners[&n];
        let loss = l.jitter.loss_ratio();
        let max_delay = delays[&n].iter().copied().max().unwrap();
        eprintln!(
            "LV1 node {n}: played {} lost {} loss {loss:.4} max delay {max_delay} ms",
            l.jitter.played, l.jitter.lost
        );
        assert!(
            l.jitter.played > 1000,
            "node {n} played {}",
            l.jitter.played
        );
        assert!(loss < 0.05, "node {n} loss {loss}");
        assert!(max_delay < 6_000, "node {n} delay {max_delay} ms");
    }
    assert_eq!(
        listeners[&7].jitter.played, 0,
        "beyond 3 hops nothing arrives"
    );
}

/// LV2: chat frames keep priority; live packets are dropped, not queued,
/// on a congested link. Chat p95 queueing delay with a live stream stays
/// within 20% of the baseline (plus one frame time for the frame on air).
#[test]
fn lv2_live_audio_does_not_raise_chat_p95() {
    fn run(with_live: bool) -> u64 {
        let mut rng = Rng(7);
        let mut s = LinkScheduler::default();
        let mut waits = Vec::new();
        let mut stamp: std::collections::VecDeque<u64> = Default::default();
        // 10 ms per frame on air; chat ~ 30 msg/s bursts; live 10 packets/s.
        let mut busy_until = 0u64;
        let mut in_flight_chat: Option<u64> = None;
        for t in 0..60_000u64 {
            if rng.f() < 0.03 {
                s.push_chat(vec![0]);
                stamp.push_back(t);
            }
            if with_live && t % 100 == 0 {
                s.push_live(vec![1]);
            }
            if t >= busy_until {
                if let Some(t0) = in_flight_chat.take() {
                    waits.push(t - t0);
                }
                if let Some(f) = s.pop() {
                    busy_until = t + 10;
                    if f == vec![0] {
                        in_flight_chat = stamp.pop_front();
                    }
                }
            }
        }
        waits.sort();
        waits[waits.len() * 95 / 100]
    }
    let base = run(false);
    let live = run(true);
    assert!(
        live as f64 <= base as f64 * 1.2 + 10.0,
        "base {base} live {live}"
    );
}

#[test]
fn scheduler_drops_live_when_congested() {
    let mut s = LinkScheduler::default();
    for _ in 0..10 {
        s.push_live(vec![1]);
    }
    assert_eq!(s.live_dropped, 6);
    s.push_chat(vec![0]);
    assert_eq!(s.pop(), Some(vec![0]), "chat first");
}

fn pid(i: u32) -> PeerId {
    let mut p = [0u8; 32];
    p[..4].copy_from_slice(&i.to_be_bytes());
    p
}

#[test]
fn tree_fanout_limits_and_depth() {
    let mut t = ViewerTree::new(pid(0));
    for i in 1..=6 {
        assert_eq!(t.join(pid(i), true), Some(pid(0)));
    }
    // 7th goes one level down, spread over the host's children.
    for i in 7..=30 {
        let p = t.join(pid(i), true).unwrap();
        assert_ne!(p, pid(0));
        assert_eq!(t.depth(&pid(i)), Some(2));
    }
    assert_eq!(t.children(&pid(0)).len(), HOST_FANOUT);
    assert!((1..=6).all(|i| t.children(&pid(i)).len() == VIEWER_FANOUT));
    assert_eq!(t.depth(&pid(31)), None);
    t.join(pid(31), true);
    assert_eq!(t.depth(&pid(31)), Some(3));
    assert!(t.check());
}

#[test]
fn tree_weak_viewers_are_leaves_and_parked_when_full() {
    let mut t = ViewerTree::with_fanout(pid(0), 1, 4);
    assert_eq!(t.join(pid(1), false), Some(pid(0)));
    assert_eq!(
        t.join(pid(2), true),
        None,
        "weak viewer can't forward; host full"
    );
    assert!(t.is_parked(&pid(2)));
    // The weak viewer leaves: the parked one takes the slot.
    let r = t.leave(&pid(1));
    assert_eq!(
        r,
        vec![Reparent {
            viewer: pid(2),
            new_parent: Some(pid(0))
        }]
    );
    assert!(t.check());
}

#[test]
fn tree_reparents_children_with_subtrees_when_parent_leaves() {
    let mut t = ViewerTree::with_fanout(pid(0), 2, 2);
    t.join(pid(1), true);
    t.join(pid(2), true);
    t.join(pid(3), true); // under 1
    t.join(pid(4), true); // under 2
    t.join(pid(5), true); // under 1
    t.join(pid(6), true); // under 2
    t.join(pid(7), true); // under 3
    assert_eq!(t.parent(&pid(7)), Some(pid(3)));
    let p3 = t.parent(&pid(3)).unwrap();
    let r = t.leave(&p3);
    // Orphans 3 and 5 move under the host's free slot / other subtrees, keeping 7 under 3.
    assert!(r
        .iter()
        .any(|x| x.viewer == pid(3) && x.new_parent.is_some()));
    assert_eq!(t.parent(&pid(7)), Some(pid(3)), "subtree intact");
    assert!(
        t.parent(&pid(3)) != Some(pid(7)),
        "never under its own subtree"
    );
    assert!(t.check());
    // The host's slot that p3 freed was taken by one orphan.
    assert_eq!(t.children(&pid(0)).len(), 2);
}

#[test]
fn tree_random_churn_keeps_invariants() {
    let mut rng = Rng(99);
    let mut t = ViewerTree::new(pid(0));
    let mut present: Vec<u32> = Vec::new();
    for step in 1..2000u32 {
        if present.len() < 5 || rng.f() < 0.6 {
            t.join(pid(step), rng.f() < 0.8);
            present.push(step);
        } else {
            let i = (rng.next() as usize) % present.len();
            let v = present.swap_remove(i);
            t.leave(&pid(v));
        }
        assert!(t.check(), "step {step}");
    }
    assert_eq!(t.viewers(), present.len());
    assert_eq!(t.host_left().len(), present.len());
    assert_eq!(t.viewers(), 0);
}

#[test]
fn lv3_switch_to_audio_within_5s_of_host_going_offline() {
    let mut m = ModeSelector::new(true, true);
    assert_eq!(m.mode(), LiveMode::Audio, "audio until video arrives");
    for t in (0..10_000).step_by(33) {
        m.on_video_frame(t);
    }
    assert_eq!(m.mode(), LiveMode::Video);
    // Host goes offline at 10 s: video frames stop.
    let mut switched = None;
    for t in (10_000..20_000).step_by(100) {
        if m.tick(t) == LiveMode::Audio {
            switched = Some(t);
            break;
        }
    }
    assert!(switched.unwrap() - 10_000 <= 5_000);
    // Our own internet dropping switches immediately.
    m.on_video_frame(30_000);
    assert_eq!(m.set_internet(false, 30_010), LiveMode::Audio);
}

#[test]
fn rtc_signal_is_e2ee() {
    let key = StreamKey::random([5; 16], 2).unwrap();
    let s = RtcSignal {
        stream_id: [5; 16],
        from: pid(1),
        to: pid(2),
        kind: SignalKind::Offer,
        body: "v=0\r\n".into(),
    };
    let b = s.seal(&key).unwrap();
    assert_eq!(b[0], CONTENT_TYPE_RTC_SIGNAL);
    assert_eq!(RtcSignal::open(&key, &b).unwrap(), s);
    assert!(RtcSignal::open(&StreamKey::random([5; 16], 2).unwrap(), &b).is_err());
    assert!(!String::from_utf8_lossy(&b).contains("v=0"));
}

#[test]
fn stun_is_public_only_and_no_turn() {
    assert!(STUN_SERVERS.iter().all(|s| s.starts_with("stun:")));
}
