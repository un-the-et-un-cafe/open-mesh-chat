//! Open Mesh Chat Protocol (`omcp-core`) — M1–M4 core library.
//!
//! Provides:
//! - Canonical [`framing::EnvelopeV1`] encode/decode (PROTOCOL §10.1, §21.1–§21.4)
//! - Version / suite negotiation hooks (PROTOCOL §5.2, §10.3)
//! - Noise_IK + ML-KEM-768 hybrid (`OMCP-Noise-Hybrid-v1`, PROTOCOL §21.5)
//! - BLE loopback transport: fragment AEAD, discovery, gossip seen-cache, and
//!   two-peer encrypted text chat over an in-memory GATT pipe (PROTOCOL §8, §10.2)
//! - MLS small groups via OpenMLS + OMCP-MLS-PQ hybrid mix helper (PROTOCOL §6.2)
//! - Chunked encrypted file sharing with BLAKE3 + Content Key wrap (PROTOCOL §7)
//!
//! - Gateway role (opaque forwarding, policy, outage/drain, control messages,
//!   pinning, store-and-forward) and an in-memory rendezvous reference
//!   (PROTOCOL §8.4, §9) — **experimental (M4)**
//!
//! - Scalable sync and data efficiency (PROTOCOL §22): rotating mailbox tags,
//!   cuckoo interest digests, spray-and-wait, range reconciliation, watermarks,
//!   lazy media, padded compression, receipts batching, Plumtree, relay budgets —
//!   **experimental (v0.2)**
//!
//! - Encrypted local store (SQLite, XChaCha20-Poly1305 per row, versioned
//!   migrations, crypto-shredded burn rows) — [`store`]
//!
//! Not in scope yet: real phone BLE bindings, real TLS/QUIC gateway transport, production hardware,
//! full OpenMLS PQ extension registration on the wire.

#![forbid(unsafe_code)]
#![allow(missing_docs)] // field-level docs filled as API stabilizes

pub mod backup;
pub mod ble;
pub mod burn;
pub mod chat_backup;
pub mod device;
pub mod error;
pub mod files;
pub mod framing;
pub mod gateway;
pub mod handshake;
pub mod live;
pub mod mls;
pub mod moderation;
pub mod negotiate;
pub mod news;
pub mod recovery;
pub mod rendezvous;
pub mod safety;
pub mod store;
pub mod suite;
pub mod sync;

pub use error::{OmcpError, Result};
pub use framing::EnvelopeV1;
pub use handshake::{
    HybridInitiator, HybridResponder, HybridSessionKeys, StaticIdentity, HYBRID_PROTOCOL_NAME,
};
pub use negotiate::{negotiate, Hello, Negotiated};
pub use suite::{SUITE_OMCP_2026_A, SUITE_OMCP_2026_A_NAME};

#[cfg(test)]
mod m2_integration {
    //! End-to-end M2: Noise hybrid + BLE framing + 1:1 text over loopback.

    use std::time::Instant;

    use crate::ble::fragment::{fragment_and_seal, random_frag_id, FragmentReassembler};
    use crate::ble::gossip::SeenCache;
    use crate::ble::link::LinkSession;
    use crate::ble::loopback::BleLoopback;
    use crate::ble::message::{open_text_envelope, seal_text_envelope, CONTENT_TYPE_TEXT_PLAIN};
    use crate::ble::{capabilities, DiscoveryAdvertisement};
    use crate::framing::EnvelopeV1;
    use crate::handshake::{HybridInitiator, HybridResponder, StaticIdentity};
    use crate::negotiate::{negotiate, Hello};

    /// Send an encoded envelope as BLE fragments over the loopback endpoint.
    fn send_envelope(link: &mut LinkSession, ep: &crate::ble::BleEndpoint, envelope_bytes: &[u8]) {
        let frag_id = random_frag_id().unwrap();
        let frames = fragment_and_seal(link, envelope_bytes, frag_id, 512).unwrap();
        for f in frames {
            ep.send(f).unwrap();
        }
    }

    /// Receive and reassemble one full envelope from the loopback endpoint.
    fn recv_envelope(
        link: &mut LinkSession,
        ep: &crate::ble::BleEndpoint,
        reasm: &mut FragmentReassembler,
    ) -> EnvelopeV1 {
        let now = Instant::now();
        loop {
            let frame = ep
                .recv()
                .unwrap()
                .expect("expected BLE frame for envelope reassembly");
            if let Some(bytes) = reasm.push_frame(link, &frame, now).unwrap() {
                return EnvelopeV1::decode(&bytes).unwrap();
            }
        }
    }

    #[test]
    fn two_peer_ble_loopback_text_chat() {
        // --- identities & discovery ads ---
        let id_a = StaticIdentity::generate().unwrap();
        let id_b = StaticIdentity::generate().unwrap();
        let peer_a = id_a.dh_public();
        let peer_b = id_b.dh_public();

        let ad_a = DiscoveryAdvertisement::new(1, &peer_a, capabilities::PQ);
        let ad_b = DiscoveryAdvertisement::new(1, &peer_b, capabilities::PQ);
        assert_eq!(
            DiscoveryAdvertisement::decode(&ad_a.encode())
                .unwrap()
                .peer_id_trunc,
            peer_a[..6]
        );
        assert_eq!(ad_b.magic, *b"OMCP");

        // --- loopback GATT pipe ---
        let (ep_a, ep_b) = BleLoopback::pair();

        // --- OMCP-Noise-Hybrid-v1 over the pipe (A initiator, B responder) ---
        // Handshake messages ride as single opaque frames (pre-link-AEAD).
        let mut init = HybridInitiator::new(id_a, &peer_b).unwrap();
        let mut resp = HybridResponder::new(id_b);

        let hello_a = Hello::local(["PQ", "BLE"]);
        let hello_b = Hello::local(["PQ", "BLE"]);
        let negotiated = negotiate(&hello_a, &hello_b).unwrap();
        assert_eq!(negotiated.suite_id, crate::SUITE_OMCP_2026_A);

        let msg1 = init.write_message_1(b"omcp-m2-hello-b").unwrap();
        ep_a.send(msg1).unwrap();
        let raw1 = ep_b.recv().unwrap().unwrap();
        let p1 = resp.read_message_1(&raw1).unwrap();
        assert_eq!(p1, b"omcp-m2-hello-b");

        let (msg2, keys_b) = resp.write_message_2(b"omcp-m2-hello-a").unwrap();
        ep_b.send(msg2).unwrap();
        let raw2 = ep_a.recv().unwrap().unwrap();
        let (p2, keys_a) = init.read_message_2(&raw2).unwrap();
        assert_eq!(p2, b"omcp-m2-hello-a");
        assert_eq!(keys_a.k_i2r, keys_b.k_i2r);

        let mut link_a = LinkSession::initiator(&keys_a);
        let mut link_b = LinkSession::responder(&keys_b);
        let mut reasm_a = FragmentReassembler::default();
        let mut reasm_b = FragmentReassembler::default();
        let mut seen_a = SeenCache::default();
        let mut seen_b = SeenCache::default();
        let mut app_send_a = 0u64;
        let mut app_recv_b = 0u64;
        let mut app_send_b = 0u64;
        let mut app_recv_a = 0u64;

        // --- A → B text ---
        let env_ab = seal_text_envelope(
            &link_a.send_key(),
            &peer_a,
            "hello from A over BLE loopback",
            1_729_000_000_001,
            &mut app_send_a,
            8,
        )
        .unwrap();
        assert_eq!(env_ab.header_aad, vec![CONTENT_TYPE_TEXT_PLAIN]);
        assert!(!seen_a.check_and_insert(env_ab.envelope_id, Instant::now()));
        let bytes_ab = env_ab.encode().unwrap();
        send_envelope(&mut link_a, &ep_a, &bytes_ab);

        let got_ab = recv_envelope(&mut link_b, &ep_b, &mut reasm_b);
        assert!(!seen_b.check_and_insert(got_ab.envelope_id, Instant::now()));
        assert!(seen_b.check_and_insert(got_ab.envelope_id, Instant::now())); // gossip dup
        let text_ab =
            open_text_envelope(&link_a.send_key(), &peer_a, &got_ab, &mut app_recv_b).unwrap();
        assert_eq!(text_ab, "hello from A over BLE loopback");

        // --- B → A text ---
        let env_ba = seal_text_envelope(
            &link_b.send_key(),
            &peer_b,
            "hello from B — M2 complete",
            1_729_000_000_002,
            &mut app_send_b,
            8,
        )
        .unwrap();
        let bytes_ba = env_ba.encode().unwrap();
        send_envelope(&mut link_b, &ep_b, &bytes_ba);

        let got_ba = recv_envelope(&mut link_a, &ep_a, &mut reasm_a);
        let text_ba =
            open_text_envelope(&link_b.send_key(), &peer_b, &got_ba, &mut app_recv_a).unwrap();
        assert_eq!(text_ba, "hello from B — M2 complete");
    }
}

#[cfg(test)]
mod m3_integration {
    //! M3: MLS group of 3 + text app message + file chunk round-trip (in-memory bus).

    use crate::files::{
        decrypt_and_reassemble, encrypt_file, wrapped_key_for, FileMeta, FileRecipientSecrets,
        DEFAULT_CHUNK_PLAINTEXT_LEN,
    };
    use crate::mls::OmcpMlsMember;

    /// In-memory fan-out bus for MLS ciphertext / file blobs (stands in for BLE/mesh).
    struct MemBus {
        /// Broadcast queue: each peer drains copies addressed to them by index.
        queues: Vec<Vec<Vec<u8>>>,
    }

    impl MemBus {
        fn new(n: usize) -> Self {
            Self {
                queues: (0..n).map(|_| Vec::new()).collect(),
            }
        }

        fn broadcast(&mut self, from: usize, msg: Vec<u8>) {
            for (i, q) in self.queues.iter_mut().enumerate() {
                if i != from {
                    q.push(msg.clone());
                }
            }
        }

        #[allow(dead_code)]
        fn send_to(&mut self, to: usize, msg: Vec<u8>) {
            self.queues[to].push(msg);
        }

        fn drain(&mut self, peer: usize) -> Vec<Vec<u8>> {
            std::mem::take(&mut self.queues[peer])
        }
    }

    #[test]
    fn mls_group_three_peers_text_and_file_chunks() {
        let mut alice = OmcpMlsMember::generate(b"alice").unwrap();
        let mut bob = OmcpMlsMember::generate(b"bob").unwrap();
        let mut charlie = OmcpMlsMember::generate(b"charlie").unwrap();

        // File wrap keys (separate from MLS sig keys; M3 out-of-band)
        let mut alice_id = [0u8; 32];
        alice_id[0] = b'A';
        let mut bob_id = [0u8; 32];
        bob_id[0] = b'B';
        let mut charlie_id = [0u8; 32];
        charlie_id[0] = b'C';
        let alice_file = FileRecipientSecrets::generate(alice_id).unwrap();
        let bob_file = FileRecipientSecrets::generate(bob_id).unwrap();
        let charlie_file = FileRecipientSecrets::generate(charlie_id).unwrap();

        alice.create_group().unwrap();
        let bob_kp = bob.take_key_package().unwrap();
        let (_c1, welcome_bob) = alice.add_members(&[bob_kp]).unwrap();
        bob.join_from_welcome(&welcome_bob).unwrap();

        let charlie_kp = charlie.take_key_package().unwrap();
        let (commit_charlie, welcome_charlie) = alice.add_members(&[charlie_kp]).unwrap();
        bob.process_protocol_message(&commit_charlie).unwrap();
        charlie.join_from_welcome(&welcome_charlie).unwrap();

        assert_eq!(alice.member_count().unwrap(), 3);
        assert_eq!(bob.member_count().unwrap(), 3);
        assert_eq!(charlie.member_count().unwrap(), 3);

        let mut bus = MemBus::new(3);

        // --- A sends MLS text to group ---
        let text_ct = alice
            .encrypt_app_message(b"hello from Alice in MLS group")
            .unwrap();
        bus.broadcast(0, text_ct);

        for msg in bus.drain(1) {
            let pt = bob.decrypt_app_message(&msg).unwrap();
            assert_eq!(pt, b"hello from Alice in MLS group");
        }
        for msg in bus.drain(2) {
            let pt = charlie.decrypt_app_message(&msg).unwrap();
            assert_eq!(pt, b"hello from Alice in MLS group");
        }

        // --- A sends a small file to B and C via chunk protocol ---
        let file_plain = b"OMCP M3 file payload for the group";
        let (meta, _ck, chunks) = encrypt_file(
            file_plain,
            "note.txt",
            "text/plain",
            &[bob_file.public_keys(), charlie_file.public_keys()],
            DEFAULT_CHUNK_PLAINTEXT_LEN,
        )
        .unwrap();

        // Deliver meta + chunks over the bus as opaque blobs (could ride MLS app
        // messages in a follow-up; M3 proves the chunk crypto + group context).
        let meta_bytes = meta.encode().unwrap();
        bus.broadcast(0, meta_bytes);
        for ch in &chunks {
            bus.broadcast(0, ch.encode());
        }

        for peer in [1usize, 2] {
            let msgs = bus.drain(peer);
            assert!(msgs.len() >= 2);
            let meta = FileMeta::decode(&msgs[0]).unwrap();
            let file_chunks: Vec<_> = msgs[1..]
                .iter()
                .map(|m| crate::files::FileChunk::decode(m).unwrap())
                .collect();
            let secrets = if peer == 1 { &bob_file } else { &charlie_file };
            let w = wrapped_key_for(&meta, &secrets.recipient_id).unwrap();
            let ck = secrets.open_content_key(w).unwrap();
            let out = decrypt_and_reassemble(&meta, &ck, &file_chunks).unwrap();
            assert_eq!(out, file_plain);
        }

        // Alice (non-recipient) must not open Bob's wrap
        let meta_again = {
            let (m, _, _) =
                encrypt_file(b"x", "x", "text/plain", &[bob_file.public_keys()], 64).unwrap();
            m
        };
        let w_bob = wrapped_key_for(&meta_again, &bob_file.recipient_id).unwrap();
        assert!(alice_file.open_content_key(w_bob).is_err());

        // PQ hybrid present for current epoch
        let others = [
            (bob.own_leaf_index().unwrap(), *bob.pq_kem.public_bytes()),
            (
                charlie.own_leaf_index().unwrap(),
                *charlie.pq_kem.public_bytes(),
            ),
        ];
        let (_ext, _ss, hybrid) = alice.build_pq_epoch_secrets(&others).unwrap();
        assert_ne!(hybrid, alice.hybrid_exporter(&[]).unwrap());
    }
}
