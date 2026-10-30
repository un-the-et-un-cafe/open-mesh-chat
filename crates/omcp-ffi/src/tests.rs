use super::*;

fn node(name: &str) -> Arc<OmcpNode> {
    OmcpNode::from_phrase(generate_recovery_phrase().unwrap(), name.into()).unwrap()
}

/// Deliver every PDU in `out` to `to`, ping-ponging until quiet.
fn pump(from: &MeshLink, to: &MeshLink, out: LinkOutput) -> Vec<ReceivedText> {
    let mut got = Vec::new();
    // FIFO: a BLE link delivers each direction in order (link AEAD counters).
    let mut queue = std::collections::VecDeque::from([(false, out.outgoing)]);
    while let Some((dir_back, pdus)) = queue.pop_front() {
        for p in pdus {
            let (dst, back) = if dir_back { (from, true) } else { (to, false) };
            let o = dst.on_pdu(p).unwrap();
            got.extend(o.received);
            if !o.outgoing.is_empty() {
                queue.push_back((!back, o.outgoing));
            }
        }
    }
    got
}

fn paired() -> (Arc<OmcpNode>, Arc<OmcpNode>, Arc<MeshLink>, Arc<MeshLink>) {
    let a = node("Alice");
    let b = node("Bob");
    let ca = b.add_contact_from_qr(a.qr_payload()).unwrap();
    let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert_eq!(ca.safety_number, cb.safety_number);
    let la = a.open_initiator_link(cb.contact_id).unwrap();
    let lb = b.open_responder_link().unwrap();
    (a, b, la, lb)
}

#[test]
fn phrase_roundtrip_restores_identity() {
    let p = generate_recovery_phrase().unwrap();
    assert_eq!(p.split(' ').count(), 24);
    assert!(validate_recovery_phrase(p.clone()));
    assert!(!validate_recovery_phrase("hello world".into()));
    let n1 = OmcpNode::from_phrase(p.clone(), "x".into()).unwrap();
    let n2 = OmcpNode::from_phrase(p, "x".into()).unwrap();
    assert_eq!(n1.identity(), n2.identity());
}

#[test]
fn qr_contact_add_and_errors() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert_eq!(c.display_name, "Bob");
    assert_eq!(c.safety_number.len(), 12);
    assert!(!c.verified);
    a.set_verified(c.contact_id.clone(), true).unwrap();
    assert!(a.contacts()[0].verified);
    assert!(a.add_contact_from_qr("https://evil".into()).is_err());
    assert!(a.add_contact_from_qr(a.qr_payload()).is_err());
    let prefix = parse_advertisement(b.advertisement()).unwrap();
    assert_eq!(a.contact_for_prefix(prefix), Some(c.contact_id));
}

#[test]
fn end_to_end_text_both_directions() {
    let (a, b, la, lb) = paired();
    let out = la.start().unwrap();
    assert!(out.outgoing.len() > 1, "handshake must be chunked");
    assert!(out.outgoing.iter().all(|p| p.len() <= 512));
    pump(&la, &lb, out);
    assert!(la.is_established() && lb.is_established());
    assert_eq!(a.path_status().path, PathKind::Mesh);
    assert_eq!(b.path_status().nearby_peers, 1);

    let long = "é".repeat(1500); // multi-fragment
    let o = la.send_text(long.clone()).unwrap();
    assert!(o.outgoing.len() > 1);
    assert!(o.outgoing.iter().all(|p| p.len() <= 512));
    let got = pump(&la, &lb, o);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, long);
    assert_eq!(got[0].contact_id, a.identity().peer_id_hex);

    let got = pump(&lb, &la, lb.send_text("hi back".into()).unwrap());
    assert_eq!(got[0].text, "hi back");
    la.shutdown();
    assert_eq!(a.path_status().nearby_peers, 0);
}

#[test]
fn offline_queue_flushes_on_connect() {
    let (a, _b, la, lb) = paired();
    let bid = a.contacts()[0].contact_id.clone();
    a.queue_text(bid.clone(), "one".into()).unwrap();
    la.send_text("two".into()).unwrap(); // not up yet → queued
    assert_eq!(a.queued_for(bid), 2);
    assert_eq!(a.path_status().path, PathKind::Queued);
    let got = pump(&la, &lb, la.start().unwrap());
    let texts: Vec<_> = got.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(a.path_status().queued_count, 0);
}

#[test]
fn responder_rejects_unknown_initiator() {
    let a = node("Alice");
    let b = node("Bob");
    let cb = a.add_contact_from_qr(b.qr_payload()).unwrap(); // b never adds a
    let la = a.open_initiator_link(cb.contact_id).unwrap();
    let lb = b.open_responder_link().unwrap();
    let mut err = None;
    for p in la.start().unwrap().outgoing {
        if let Err(e) = lb.on_pdu(p) {
            err = Some(e);
        }
    }
    assert!(matches!(err, Some(FfiError::UnknownContact)));
    assert!(!lb.is_established());
}

#[test]
fn link_pdu_before_handshake_is_error() {
    let b = node("Bob");
    let lb = b.open_responder_link().unwrap();
    assert!(lb.on_pdu(vec![PDU_LINK, 0, 1]).is_err());
    assert!(lb.on_pdu(vec![]).is_err());
    assert!(lb.on_pdu(vec![9]).is_err());
}

#[test]
fn gateway_policy_defaults() {
    assert_eq!(default_gateway_daily_cap_bytes(), 500_000_000);
    let wifi = gateway_decision(true, Network::Wifi { metered: false });
    assert!(wifi.bridge && !wifi.metered_warning);
    assert!(!gateway_decision(true, Network::Cellular { metered: true }).bridge);
    let cell = gateway_decision(false, Network::Cellular { metered: true });
    assert!(cell.bridge && cell.metered_warning);
    assert!(!gateway_decision(false, Network::Offline).bridge);
}

#[test]
fn file_size_limit_exposed() {
    assert_eq!(max_file_bytes(), 50_000_000);
    assert!(check_file_size(50_000_000).is_ok());
    assert!(matches!(
        check_file_size(50_000_001),
        Err(FfiError::FileTooLarge {
            size: 50_000_001,
            max: 50_000_000
        })
    ));
}

#[test]
fn recovery_word_helpers() {
    assert!(is_recovery_word("fossil".into()));
    assert_eq!(suggest_recovery_words("foss".into()), vec!["fossil"]);
}

#[test]
fn preview_detects_known_and_mismatch() {
    let a = node("Alice");
    let b = node("Bob");
    let p = a.preview_contact_from_qr(b.qr_payload()).unwrap();
    assert!(!p.already_known && p.mismatch_with.is_none());
    assert!(a.contacts().is_empty(), "preview must not save");
    a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert!(
        a.preview_contact_from_qr(b.qr_payload())
            .unwrap()
            .already_known
    );
    let impostor = node("Bob");
    let p = a.preview_contact_from_qr(impostor.qr_payload()).unwrap();
    let old = p.mismatch_with.unwrap();
    assert_ne!(old.safety_number, p.contact.safety_number);
}

#[test]
fn backup_roundtrip_restores_identity_contacts_and_verification() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    a.set_verified(c.contact_id.clone(), true).unwrap();
    let pw = "correct horse battery staple 42".to_string();
    assert!(check_backup_password(pw.clone()).ok);
    assert!(!check_backup_password("password".into()).ok);
    let blob = a.create_backup(pw.clone(), "Pixel 8".into(), 42).unwrap();
    let phrase = a.recovery_phrase().unwrap();
    assert!(!blob.windows(phrase.len()).any(|w| w == phrase.as_bytes()));
    let info = backup_info(blob.clone()).unwrap();
    assert!(info.needs_password && info.created_at_ms == 42);
    assert!(matches!(
        open_backup_with_password(blob.clone(), "nope nope nope 1".into()),
        Err(FfiError::WrongBackupPassword)
    ));
    let r = open_backup_with_password(blob, pw).unwrap();
    assert_eq!(r.phrase, phrase);
    assert_eq!(r.display_name.as_deref(), Some("Alice"));
    let a2 = OmcpNode::from_phrase(r.phrase, "Alice".into()).unwrap();
    assert_eq!(a2.identity().peer_id_hex, a.identity().peer_id_hex);
    for k in r.contacts {
        a2.restore_contact(k.contact_id, k.display_name, k.verified)
            .unwrap();
    }
    let key = |v: Vec<ContactInfo>| {
        v.into_iter()
            .map(|c| (c.contact_id, c.display_name, c.verified))
            .collect::<Vec<_>>()
    };
    assert_eq!(key(a2.contacts()), key(a.contacts()));
}

// ---- burn after read end to end (PROTOCOL §19.1) ----

fn pump_all(
    la: &MeshLink,
    lb: &MeshLink,
    out: LinkOutput,
) -> (Vec<ReceivedText>, Vec<OpenedEvent>) {
    // Like `pump`, but also collects OPENED events from both sides.
    let mut got = Vec::new();
    let mut opened = Vec::new();
    let mut queue = std::collections::VecDeque::from([(false, out.outgoing)]);
    opened.extend(out.opened);
    while let Some((back, pdus)) = queue.pop_front() {
        for p in pdus {
            let dst = if back { la } else { lb };
            let o = dst.on_pdu(p).unwrap();
            got.extend(o.received);
            opened.extend(o.opened);
            if !o.outgoing.is_empty() {
                queue.push_back((!back, o.outgoing));
            }
        }
    }
    (got, opened)
}

fn spec(mode: BurnMode, secs: u32, ttl: u32) -> BurnSpec {
    BurnSpec {
        mode,
        secs,
        unread_ttl: ttl,
    }
}

#[test]
fn burn_timer_and_opened_shred_both_sides() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_message(
            bob_id,
            "secret".into(),
            Some(spec(BurnMode::Timer, 10, 604_800)),
        )
        .unwrap();
    assert!(a.burn_is_live(id.clone()));
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    let m = got.single_or_panic();
    assert_eq!(m.text, "", "burn content not in the received list");
    assert_eq!(m.burn, Some(spec(BurnMode::Timer, 10, 604_800)));
    assert_eq!(m.envelope_id_hex, id);

    let c = b.open_burn(id.clone(), 1_000).unwrap().unwrap();
    assert_eq!(c.text, "secret");
    assert_eq!(c.burn_at_ms, Some(11_000));
    assert!(
        b.open_burn(id.clone(), 1_001).unwrap().is_none(),
        "opens once"
    );
    let (_, opened) = pump_all(&lb, &la, lb.flush().unwrap());
    assert_eq!(opened.len(), 1);
    assert!(!opened[0].shredded);
    assert_eq!(opened[0].burn_at_ms, Some(11_000));
    assert!(b.tick_burn(10_999).is_empty());
    assert_eq!(b.tick_burn(11_000), vec![id.clone()]);
    assert_eq!(a.tick_burn(11_000), vec![id.clone()]);
    assert!(!a.burn_is_live(id.clone()) && !b.burn_is_live(id));
}

#[test]
fn burn_view_once_media_shreds_sender_on_opened() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_media(
            bob_id,
            "image/jpeg".into(),
            vec![0xFF, 0xD8, 1],
            Some(spec(BurnMode::OnOpen, 0, 86_400)),
        )
        .unwrap();
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    assert_eq!(got.single_or_panic().mime.as_deref(), Some("image/jpeg"));
    let c = b.open_burn(id.clone(), 5).unwrap().unwrap();
    assert_eq!(c.bytes, vec![0xFF, 0xD8, 1]);
    assert!(
        !b.burn_is_live(id.clone()),
        "view once: receiver copy shredded on view"
    );
    assert!(b.open_burn(id.clone(), 6).unwrap().is_none());
    let (_, opened) = pump_all(&lb, &la, lb.flush().unwrap());
    assert!(opened.single_or_panic().shredded);
    assert!(!a.burn_is_live(id));
}

#[test]
fn burn_unread_ttl_expires_both_sides_and_bad_specs_rejected() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let s = spec(BurnMode::OnOpen, 0, 3_600);
    let id = a
        .queue_message(bob_id.clone(), "never".into(), Some(s))
        .unwrap();
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    let sent = got.single_or_panic().created_ms;
    let exp = burn_expires_at_ms(sent, s).unwrap();
    assert_eq!(exp, sent + 3_600_000);
    assert!(b.tick_burn(exp - 1).is_empty());
    assert_eq!(b.tick_burn(exp), vec![id.clone()]);
    assert!(b.open_burn(id.clone(), exp).unwrap().is_none());
    assert_eq!(a.tick_burn(exp + 1_000), vec![id]); // sender copy expires too
    assert!(!burn_spec_valid(spec(BurnMode::Timer, 11, 3_600)));
    assert!(a
        .queue_message(bob_id, "x".into(), Some(spec(BurnMode::OnOpen, 0, 5)))
        .is_err());
}

#[test]
fn opened_from_wrong_peer_is_ignored_and_not_counted_as_flushed() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_message(bob_id, "t".into(), Some(spec(BurnMode::Timer, 60, 604_800)))
        .unwrap();
    let out = la.flush().unwrap();
    assert_eq!(out.flushed, 1);
    let _ = pump_all(&la, &lb, out);
    b.open_burn(id, 0).unwrap().unwrap();
    assert_eq!(
        b.queued_for(a.identity().peer_id_hex),
        0,
        "OPENED is control, not user-queued"
    );
    assert_eq!(lb.flush().unwrap().flushed, 0);
}

trait Single<T> {
    fn single_or_panic(self) -> T;
}
impl<T: std::fmt::Debug> Single<T> for Vec<T> {
    fn single_or_panic(mut self) -> T {
        assert_eq!(self.len(), 1, "{self:?}");
        self.pop().unwrap()
    }
}

#[test]
fn icloud_key_backup_roundtrip_and_no_plaintext() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    a.set_verified(c.contact_id.clone(), true).unwrap();
    let key = generate_backup_key().unwrap();
    assert_eq!(key.len(), 32);
    let blob = a
        .create_backup_with_key(key.clone(), "iPhone".into(), 7)
        .unwrap();
    let info = backup_info(blob.clone()).unwrap();
    assert!(!info.needs_password);
    // O2: neither the phrase words nor the key appear in the blob.
    let phrase = a.recovery_phrase().unwrap();
    for w in phrase.split(' ').filter(|w| w.len() >= 5) {
        assert!(!blob.windows(w.len()).any(|x| x == w.as_bytes()));
    }
    assert!(!blob.windows(32).any(|x| x == key.as_slice()));
    let r = open_backup_with_key(blob.clone(), key).unwrap();
    assert_eq!(r.phrase, phrase);
    assert!(r.contacts[0].verified);
    let wrong = generate_backup_key().unwrap();
    assert!(open_backup_with_key(blob.clone(), wrong).is_err());
    assert!(matches!(
        open_backup_with_key(blob, vec![0; 31]),
        Err(FfiError::Invalid { .. })
    ));
}

/// Like `pump`, but keeps calling `flush()` while the sender reports more file chunks.
fn pump_files(from: &MeshLink, to: &MeshLink) -> (Vec<ReceivedFile>, Vec<FileFailure>, u32) {
    let (mut files, mut fails, mut flushes) = (Vec::new(), Vec::new(), 0);
    loop {
        let out = from.flush().unwrap();
        flushes += 1;
        let more = out.more_pending;
        for p in out.outgoing {
            let o = to.on_pdu(p).unwrap();
            files.extend(o.files);
            fails.extend(o.file_failures);
        }
        if !more {
            return (files, fails, flushes);
        }
    }
}

#[test]
fn file_transfer_end_to_end_in_64k_chunks() {
    let (a, _b, la, lb) = paired();
    let start = la.start().unwrap();
    pump(&la, &lb, start);
    let bob = a.contacts()[0].contact_id.clone();
    let data: Vec<u8> = (0..(3 * FILE_CHUNK_BYTES + 123))
        .map(|i| (i * 7) as u8)
        .collect();
    let offer = a
        .queue_file(
            bob.clone(),
            "flyer.jpg".into(),
            "image/jpeg".into(),
            data.clone(),
        )
        .unwrap();
    assert_eq!(offer.num_chunks, 4);
    assert_eq!(a.files_pending_for(bob.clone()), 1);
    let (files, fails, flushes) = pump_files(&la, &lb);
    assert!(fails.is_empty());
    assert!(flushes >= 2, "chunks must be drained over several flushes");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "flyer.jpg");
    assert_eq!(files[0].mime, "image/jpeg");
    assert_eq!(files[0].bytes, data);
    assert_eq!(a.files_pending_for(bob), 0);
}

#[test]
fn file_too_large_rejected_before_encryption_and_cancel() {
    let (a, _b, _la, _lb) = paired();
    let bob = a.contacts()[0].contact_id.clone();
    let big = vec![0u8; (max_file_bytes() + 1) as usize];
    assert!(matches!(
        a.queue_file(bob.clone(), "x.bin".into(), "".into(), big),
        Err(FfiError::FileTooLarge { .. })
    ));
    let o = a
        .queue_file(bob.clone(), "e.txt".into(), "text/plain".into(), vec![])
        .unwrap();
    assert_eq!(o.num_chunks, 1);
    assert!(a.cancel_file(o.transfer_id_hex.clone()));
    assert!(!a.cancel_file(o.transfer_id_hex));
    assert_eq!(a.files_pending_for(bob), 0);
}

#[test]
fn interrupted_file_transfer_completes_on_reconnect() {
    let (a, b, la, lb) = paired();
    let start = la.start().unwrap();
    pump(&la, &lb, start);
    let bob = a.contacts()[0].contact_id.clone();
    let data: Vec<u8> = (0..(5 * FILE_CHUNK_BYTES))
        .map(|i| (i % 251) as u8)
        .collect();
    a.queue_file(
        bob.clone(),
        "v.mp4".into(),
        "video/mp4".into(),
        data.clone(),
    )
    .unwrap();
    // First link: one flush delivered, then the link drops.
    let out = la.flush().unwrap();
    assert!(out.more_pending);
    for p in out.outgoing {
        assert!(lb.on_pdu(p).unwrap().files.is_empty());
    }
    la.shutdown();
    lb.shutdown();
    // Reconnect: offer re-sent, chunks replayed, receiver keeps partial state.
    let la2 = a.open_initiator_link(bob).unwrap();
    let lb2 = b.open_responder_link().unwrap();
    let start = la2.start().unwrap();
    pump(&la2, &lb2, start);
    let (files, fails, _) = pump_files(&la2, &lb2);
    assert!(fails.is_empty());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, data);
}

// ---- encrypted local store: survives a restart ----

const SKEY: [u8; 32] = [0x5c; 32];

fn open_node(phrase: &str, name: &str, dir: &std::path::Path) -> Arc<OmcpNode> {
    OmcpNode::open(
        phrase.into(),
        name.into(),
        dir.join("omcp.db").to_string_lossy().into_owned(),
        SKEY.to_vec(),
    )
    .unwrap()
}

fn disk_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut all = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        all.extend(std::fs::read(e.unwrap().path()).unwrap());
    }
    all
}

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn contacts_chats_messages_and_queue_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let pa = generate_recovery_phrase().unwrap();
    let b = node("Bob Quintessa");
    let bob_id;
    {
        let a = open_node(&pa, "Alice", dir.path());
        b.add_contact_from_qr(a.qr_payload()).unwrap();
        let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
        bob_id = cb.contact_id.clone();
        a.set_verified(bob_id.clone(), true).unwrap();
        // One message exchanged now, one left in the offline queue.
        let la = a.open_initiator_link(bob_id.clone()).unwrap();
        let lb = b.open_responder_link().unwrap();
        pump(&la, &lb, la.start().unwrap());
        let got = pump(
            &lb,
            &la,
            lb.send_text("hello Alice, from Bob".into()).unwrap(),
        );
        assert_eq!(got[0].text, "hello Alice, from Bob");
        la.shutdown();
        a.queue_text(bob_id.clone(), "queued while offline".into())
            .unwrap();
        assert_eq!(a.queued_for(bob_id.clone()), 1);
    }
    let raw = disk_bytes(dir.path());
    assert!(!has(&raw, b"queued while offline"));
    assert!(!has(&raw, b"hello Alice"));
    assert!(!has(&raw, b"Bob Quintessa")); // 3 bytes would match random ciphertext now and then

    // "Restart": a fresh node over the same file and key.
    let a = open_node(&pa, "Alice", dir.path());
    let c = a.contacts();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].display_name, "Bob Quintessa");
    assert!(c[0].verified);
    assert_eq!(a.chats().unwrap()[0].contact_id, bob_id);
    let ms = a.stored_messages(bob_id.clone()).unwrap();
    let texts: Vec<_> = ms
        .iter()
        .map(|m| (m.text.as_str(), m.mine, m.status))
        .collect();
    assert_eq!(
        texts,
        [
            ("hello Alice, from Bob", false, MessageStatus::Received),
            ("queued while offline", true, MessageStatus::Queued),
        ]
    );
    assert_eq!(a.queued_for(bob_id.clone()), 1);

    // The restored queue is delivered on the next link and marked sent.
    let la = a.open_initiator_link(bob_id.clone()).unwrap();
    let lb = b.open_responder_link().unwrap();
    let got = pump(&la, &lb, la.start().unwrap());
    assert_eq!(got[0].text, "queued while offline");
    assert_eq!(a.queued_for(bob_id.clone()), 0);
    assert_eq!(
        a.stored_messages(bob_id.clone()).unwrap()[1].status,
        MessageStatus::Sent
    );
    drop((la, lb, a));
    let a = open_node(&pa, "Alice", dir.path());
    assert_eq!(a.queued_for(bob_id), 0);
}

#[test]
fn wrong_storage_key_and_other_identity_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let pa = generate_recovery_phrase().unwrap();
    drop(open_node(&pa, "A", dir.path()));
    let path = dir.path().join("omcp.db").to_string_lossy().into_owned();
    assert!(matches!(
        OmcpNode::open(pa.clone(), "A".into(), path.clone(), vec![1; 32]),
        Err(FfiError::Storage { .. })
    ));
    assert!(matches!(
        OmcpNode::open(pa, "A".into(), path.clone(), vec![1; 31]),
        Err(FfiError::Invalid { .. })
    ));
    let other = generate_recovery_phrase().unwrap();
    assert!(matches!(
        OmcpNode::open(other, "B".into(), path, SKEY.to_vec()),
        Err(FfiError::Storage { .. })
    ));
    assert_eq!(storage_schema_version(), 3);
    assert_eq!(generate_storage_key().unwrap().len(), 32);
}

/// PROTOCOL §19.1 / REQUIREMENTS B1: an unopened burn message survives a
/// restart (and opens exactly once); once burned, neither it nor its key can be
/// recovered from disk, and it doesn't come back after another restart.
#[test]
fn burn_survives_restart_until_burned_then_is_unrecoverable() {
    const SECRET: &str = "burn me: the spare key is under the blue flowerpot";
    let dir = tempfile::tempdir().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let a = node("Alice");
    let (alice_id, msg_id);
    {
        let b = open_node(&pb, "Bob", dir.path());
        alice_id = b.add_contact_from_qr(a.qr_payload()).unwrap().contact_id;
        let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
        msg_id = a
            .queue_message(
                cb.contact_id.clone(),
                SECRET.into(),
                Some(spec(BurnMode::Timer, 10, core_burn::UNREAD_TTL_7D)),
            )
            .unwrap();
        let la = a.open_initiator_link(cb.contact_id).unwrap();
        let lb = b.open_responder_link().unwrap();
        let (got, _) = pump_all(&la, &lb, la.start().unwrap());
        assert_eq!(got[0].text, "");
        assert!(got[0].burn.is_some());
        lb.shutdown();
    }
    assert!(!has(&disk_bytes(dir.path()), SECRET.as_bytes()));

    let t0 = now_ms();
    {
        let b = open_node(&pb, "Bob", dir.path());
        assert!(b.burn_is_live(msg_id.clone()));
        let m = &b.stored_messages(alice_id.clone()).unwrap()[0];
        assert_eq!(
            (m.text.as_str(), m.burn_state),
            ("", StoredBurnState::Unopened)
        );
        let c = b.open_burn(msg_id.clone(), t0).unwrap().unwrap();
        assert_eq!(c.text, SECRET);
        assert_eq!(c.burn_at_ms, Some(t0 + 10_000));
    }
    // Restart while open: content is never handed out a second time.
    {
        let b = open_node(&pb, "Bob", dir.path());
        assert_eq!(b.open_burn(msg_id.clone(), t0 + 1).unwrap(), None);
        assert_eq!(
            b.stored_messages(alice_id.clone()).unwrap()[0].burn_state,
            StoredBurnState::Open
        );
        // Control messages (the OPENED receipt) aren't user-visible queue items.
        assert_eq!(b.path_status().queued_count, 0);
        assert_eq!(b.tick_burn(t0 + 10_000), vec![msg_id.clone()]);
        assert!(!b.burn_is_live(msg_id.clone()));
        assert!(b.stored_messages(alice_id.clone()).unwrap().is_empty());
    }
    let b = open_node(&pb, "Bob", dir.path());
    assert!(!b.burn_is_live(msg_id.clone()));
    assert_eq!(b.open_burn(msg_id.clone(), t0 + 20_000).unwrap(), None);
    assert!(b.stored_messages(alice_id).unwrap().is_empty());
    drop(b);
    assert!(!has(&disk_bytes(dir.path()), SECRET.as_bytes()));
}

#[test]
fn identity_backup_no_longer_carries_history() {
    // v0.1.4: history moved to the chat backup (see `chatbackup` tests); the
    // Argon2id identity backup is identity + contacts + name only.
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    pump(
        &la,
        &lb,
        la.send_text("not in the identity backup".into()).unwrap(),
    );
    let key = generate_backup_key().unwrap();
    let blob = a
        .create_backup_with_key(key.clone(), "d".into(), 1)
        .unwrap();
    let r = open_backup_with_key(blob, key).unwrap();
    assert!(r.history.is_none());
    // Old blobs that still carry `history` keep restoring through `restore_history`.
    let legacy = a.inner.lock().db().export_history().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a2 = open_node(&r.phrase, "Alice", dir.path());
    for k in &r.contacts {
        a2.restore_contact(k.contact_id.clone(), k.display_name.clone(), k.verified)
            .unwrap();
    }
    a2.restore_history(legacy.to_vec()).unwrap();
    let bob = a.contacts()[0].contact_id.clone();
    assert_eq!(
        a2.stored_messages(bob).unwrap()[0].text,
        "not in the identity backup"
    );
    drop(b);
}

#[test]
fn outgoing_file_and_received_file_reference_survive_restart() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let (pa, pb) = (
        generate_recovery_phrase().unwrap(),
        generate_recovery_phrase().unwrap(),
    );
    let data: Vec<u8> = (0..(2 * FILE_CHUNK_BYTES + 7)).map(|i| i as u8).collect();
    let (bob, alice, tid);
    {
        let a = open_node(&pa, "Alice", dir_a.path());
        let b = open_node(&pb, "Bob", dir_b.path());
        alice = b.add_contact_from_qr(a.qr_payload()).unwrap().contact_id;
        bob = a.add_contact_from_qr(b.qr_payload()).unwrap().contact_id;
        tid = a
            .queue_file(
                bob.clone(),
                "notes.txt".into(),
                "text/plain".into(),
                data.clone(),
            )
            .unwrap()
            .transfer_id_hex;
    }
    // Restart before anything was sent: the transfer is still queued.
    let a = open_node(&pa, "Alice", dir_a.path());
    let b = open_node(&pb, "Bob", dir_b.path());
    assert_eq!(a.files_pending_for(bob.clone()), 1);
    let m = &a.stored_messages(bob.clone()).unwrap()[0];
    assert_eq!(m.file.as_ref().unwrap().state, StoredFileState::Queued);
    let la = a.open_initiator_link(bob.clone()).unwrap();
    let lb = b.open_responder_link().unwrap();
    pump(&la, &lb, la.start().unwrap());
    let (files, fails, _) = pump_files(&la, &lb);
    assert!(fails.is_empty());
    assert_eq!(files[0].bytes, data);
    assert_eq!(a.files_pending_for(bob.clone()), 0);
    b.set_received_file_path(tid.clone(), Some("/app/files/notes.txt".into()))
        .unwrap();
    drop((la, lb, a, b));

    let a = open_node(&pa, "Alice", dir_a.path());
    let b = open_node(&pb, "Bob", dir_b.path());
    assert_eq!(a.files_pending_for(bob.clone()), 0);
    let sent = a.stored_messages(bob).unwrap()[0].file.clone().unwrap();
    assert_eq!(sent.state, StoredFileState::Sent);
    let got = b.stored_messages(alice).unwrap()[0].file.clone().unwrap();
    assert_eq!(got.state, StoredFileState::Received);
    assert_eq!(got.local_path.as_deref(), Some("/app/files/notes.txt"));
    assert_eq!(
        (got.name.as_str(), got.size),
        ("notes.txt", data.len() as u64)
    );
}

// ---- chat backup, media re-fetch, restore indicator (CB1–CB5, RS1–RS2) ----

/// Pump in both directions until quiet, collecting every output.
fn pump_out(la: &MeshLink, lb: &MeshLink, out: LinkOutput) -> Vec<LinkOutput> {
    let mut outs = Vec::new();
    // FIFO: a BLE link delivers each direction in order (link AEAD counters).
    let mut queue = std::collections::VecDeque::from([(false, out.outgoing.clone())]);
    outs.push(out);
    while let Some((back, pdus)) = queue.pop_front() {
        for p in pdus {
            let dst = if back { la } else { lb };
            let o = dst.on_pdu(p).unwrap();
            if !o.outgoing.is_empty() {
                queue.push_back((!back, o.outgoing.clone()));
            }
            outs.push(o);
        }
    }
    outs
}

/// Drain pending file chunks from `from` to `to` (and replies back).
fn drain(from: &MeshLink, to: &MeshLink) -> Vec<LinkOutput> {
    let mut outs = Vec::new();
    loop {
        let o = from.flush().unwrap();
        let more = o.more_pending;
        outs.extend(pump_out(from, to, o));
        if !more {
            return outs;
        }
    }
}

fn link(a: &Arc<OmcpNode>, b: &Arc<OmcpNode>) -> (Arc<MeshLink>, Arc<MeshLink>, Vec<LinkOutput>) {
    let bid = b.identity().peer_id_hex;
    let la = a.open_initiator_link(bid).unwrap();
    let lb = b.open_responder_link().unwrap();
    let outs = pump_out(&la, &lb, la.start().unwrap());
    assert!(la.is_established() && lb.is_established());
    (la, lb, outs)
}

/// Back up all chat blobs of `n` (snapshot + logs so far).
fn backup_all(n: &OmcpNode, now: u64, blobs: &mut Vec<Vec<u8>>) {
    if let Some(u) = n.chat_backup_next(now, false).unwrap() {
        assert_eq!(
            u.blob.len() % omcp_core::chat_backup::BUCKET,
            0,
            "CB2 padded"
        );
        n.chat_backup_commit(u.seq).unwrap();
        blobs.push(u.blob);
    }
}

/// Restore `a` onto a new device from its 24 words and the chat backup.
fn restore_device(a: &OmcpNode, blobs: &[Vec<u8>], dir: &std::path::Path) -> Arc<OmcpNode> {
    // Identity backup (contacts with their X25519 keys) → new install.
    let key = generate_backup_key().unwrap();
    let r = open_backup_with_key(
        a.create_backup_with_key(key.clone(), "t".into(), 1)
            .unwrap(),
        key,
    )
    .unwrap();
    let a2 = open_node(&r.phrase, "Alice", dir);
    for c in r.contacts {
        assert!(c.dh_hex.is_some(), "v0.1.4a backups carry the X25519 key");
        a2.restore_contact_with_key(c.contact_id, c.display_name, c.verified, c.dh_hex)
            .unwrap();
    }
    a2.mark_restored(now_ms()).unwrap();
    a2.chat_backup_restore(blobs.to_vec()).unwrap();
    a2
}

const PIC: &[u8] = b"pretend these are the bytes of a holiday photo";

#[test]
fn cb1_cb4_restore_brings_back_all_text_no_media_bytes_no_burn() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let bob = b.identity().peer_id_hex;
    let mut sent = Vec::new();
    for i in 0..30 {
        let t = format!("text {i} – ünïcödé");
        pump(&la, &lb, la.send_text(t.clone()).unwrap());
        sent.push(t);
    }
    let got = pump(&lb, &la, lb.send_text("reply from Bob".into()).unwrap());
    assert_eq!(got.len(), 1);
    a.queue_message(
        bob.clone(),
        "burn never in backup".into(),
        Some(spec(BurnMode::OnOpen, 0, core_burn::UNREAD_TTL_1H)),
    )
    .unwrap();
    a.queue_file(
        bob.clone(),
        "pic.jpg".into(),
        "image/jpeg".into(),
        PIC.to_vec(),
    )
    .unwrap();
    pump(&la, &lb, la.flush().unwrap());
    drain(&la, &lb);
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    pump(
        &la,
        &lb,
        la.send_text("after the first upload".into()).unwrap(),
    );
    sent.push("after the first upload".into());
    backup_all(&a, 2, &mut blobs);
    assert_eq!(blobs.len(), 2, "snapshot + incremental log");
    for blob in &blobs {
        assert!(!has(blob, PIC), "no media bytes");
        assert!(!has(blob, b"burn never in backup"), "CB4");
        assert!(!has(blob, b"text 1"), "encrypted");
    }
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let msgs = a2.stored_messages(bob.clone()).unwrap();
    let texts: Vec<&str> = msgs.iter().map(|m| m.text.as_str()).collect();
    for t in &sent {
        assert!(texts.contains(&t.as_str()), "missing {t}");
    }
    assert!(texts.contains(&"reply from Bob"));
    assert!(msgs.iter().all(|m| m.burn.is_none()));
    let pic = msgs.iter().find(|m| m.file.is_some()).unwrap();
    let f = pic.file.as_ref().unwrap();
    assert_eq!(f.state, StoredFileState::Missing);
    assert_eq!((f.mime.as_str(), f.size), ("image/jpeg", PIC.len() as u64));
    assert!(f.local_path.is_none());
    // Text count is exact: everything that wasn't burn content.
    let before = a.stored_messages(bob).unwrap();
    assert_eq!(
        msgs.len(),
        before.iter().filter(|m| m.burn.is_none()).count()
    );
}

#[test]
fn cb2_wrong_words_cannot_decrypt() {
    let (a, _b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    pump(&la, &lb, la.send_text("private".into()).unwrap());
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let other = node("Mallory");
    assert!(matches!(
        other.chat_backup_restore(blobs.clone()),
        Err(FfiError::WrongBackupPassword)
    ));
    assert!(
        a.chat_backup_next(2, false).unwrap().is_none(),
        "nothing new"
    );
    assert!(a.chat_backup_due(1 + 3_600_000, true, false).unwrap());
    assert!(
        !a.chat_backup_due(1 + 3_600_000, false, false).unwrap(),
        "Wi-Fi only"
    );
}

#[test]
fn cb3_placeholder_refills_from_partner_and_unavailable_path() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let bob = b.identity().peer_id_hex;
    let alice = a.identity().peer_id_hex;
    // Bob sends Alice a photo; Alice saves it.
    lb.flush().unwrap();
    let offer = b
        .queue_file(
            alice.clone(),
            "pic.jpg".into(),
            "image/jpeg".into(),
            PIC.to_vec(),
        )
        .unwrap();
    let outs = drain(&lb, &la);
    let rf: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    assert_eq!(rf.len(), 1);
    a.set_received_file_path(offer.transfer_id_hex.clone(), Some("/a/pic.jpg".into()))
        .unwrap();
    b.set_received_file_path(offer.transfer_id_hex.clone(), Some("/b/pic.jpg".into()))
        .ok();
    // A second file Bob will no longer have.
    let gone = b
        .queue_file(
            alice.clone(),
            "doc.pdf".into(),
            "application/pdf".into(),
            b"pdf".to_vec(),
        )
        .unwrap();
    drain(&lb, &la);
    // Bob's kept copies: the photo answers from the app below; the pdf is gone.
    {
        let st = b.inner.lock();
        st.db()
            .delete_kept_media(&parse_msg_id(&offer.transfer_id_hex).unwrap())
            .unwrap();
        st.db()
            .delete_kept_media(&parse_msg_id(&gone.transfer_id_hex).unwrap())
            .unwrap();
    }
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    la.shutdown();
    lb.shutdown();

    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let st = |id: &str| {
        a2.stored_messages(bob.clone())
            .unwrap()
            .into_iter()
            .find(|m| m.id_hex == id)
            .unwrap()
            .file
            .unwrap()
            .state
    };
    assert_eq!(st(&offer.transfer_id_hex), StoredFileState::Missing);
    // Bob comes into range: requests go out on link-up, Bob's app answers.
    let t0 = Instant::now();
    let (l2, lb2, outs) = link(&a2, &b);
    assert!(outs.iter().any(|o| o
        .media_updates
        .iter()
        .any(|u| u.state == StoredFileState::Requested)));
    let reqs: Vec<_> = outs.iter().flat_map(|o| o.media_requests.clone()).collect();
    assert_eq!(reqs.len(), 2);
    for r in &reqs {
        let bytes = (r.msg_id_hex == offer.transfer_id_hex).then(|| PIC.to_vec());
        b.answer_media_request(r.contact_id.clone(), r.msg_id_hex.clone(), bytes)
            .unwrap();
    }
    let outs = drain(&lb2, &l2);
    let files: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, PIC);
    assert!(t0.elapsed().as_secs() < 10, "CB3 within 10 s");
    a2.set_received_file_path(files[0].transfer_id_hex.clone(), Some("/a2/pic.jpg".into()))
        .unwrap();
    assert_eq!(st(&offer.transfer_id_hex), StoredFileState::Received);
    let unavailable: Vec<_> = outs.iter().flat_map(|o| o.media_updates.clone()).collect();
    assert!(unavailable
        .iter()
        .any(|u| u.msg_id_hex == gone.transfer_id_hex && u.state == StoredFileState::Unavailable));
    assert_eq!(st(&gone.transfer_id_hex), StoredFileState::Unavailable);
    // "Ask to resend" reaches Bob as a prompt.
    // (e) "Ask to resend" is ONE message carrying the optional note.
    a2.ask_to_resend(
        gone.transfer_id_hex.clone(),
        Some("  lost it, pls resend ".into()),
    )
    .unwrap();
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let asks: Vec<_> = outs.iter().flat_map(|o| o.resend_asks.clone()).collect();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].note.as_deref(), Some("lost it, pls resend"));
    assert!(
        outs.iter().all(|o| o.received.is_empty()),
        "no separate text message"
    );
    // Without a note, and a too-long note is refused.
    a2.ask_to_resend(gone.transfer_id_hex.clone(), None)
        .unwrap();
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let asks: Vec<_> = outs.iter().flat_map(|o| o.resend_asks.clone()).collect();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].note, None);
    assert!(a2
        .ask_to_resend(gone.transfer_id_hex.clone(), Some("x".repeat(281)))
        .is_err());
}

#[test]
fn e_resend_ask_wire_carries_note_and_old_form_still_decodes() {
    use omcp_core::burn::{decode_app, encode_app, MediaControl, CONTROL_MEDIA_RESEND_ASK};
    let c = MediaControl {
        note: Some("merci".into()),
        ..MediaControl::new(CONTROL_MEDIA_RESEND_ASK, [1; 16], [2; 32])
    };
    let wire = encode_app(None, &AppContent::MediaControl(c.clone())).unwrap();
    assert_eq!(decode_app(&wire).unwrap().1, AppContent::MediaControl(c));
    let bare = MediaControl::new(CONTROL_MEDIA_RESEND_ASK, [1; 16], [2; 32]);
    assert_eq!(bare.encode().len(), MediaControl::WIRE_LEN);
    assert_eq!(MediaControl::decode(&bare.encode()).unwrap(), bare);
}

#[test]
fn rs3_restored_contact_refetches_file_sent_30_days_ago_burn_never_served() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let bob = b.identity().peer_id_hex;
    // Bob sends Alice a photo, and a view-once photo.
    lb.flush().unwrap();
    let offer = b
        .queue_file(
            alice.clone(),
            "pic.jpg".into(),
            "image/jpeg".into(),
            PIC.to_vec(),
        )
        .unwrap();
    let once = b
        .queue_media(
            alice.clone(),
            "image/jpeg".into(),
            b"view-once".to_vec(),
            Some(spec(BurnMode::OnOpen, 0, 86_400)),
        )
        .unwrap();
    drain(&lb, &la);
    let u = b.kept_media_usage().unwrap();
    assert_eq!(
        (u.count, u.bytes),
        (1, PIC.len() as u64),
        "burn media never kept"
    );
    assert_eq!(
        b.kept_media_settings().unwrap(),
        KeptMediaSettings {
            max_days: 90,
            max_bytes: 2_000_000_000
        }
    );
    // 30 days later the copy is still kept.
    let day = 86_400_000u64;
    assert_eq!(b.evict_kept_media(now_ms() + 30 * day).unwrap(), 0);
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    la.shutdown();
    lb.shutdown();

    // Alice restores; the photo is a placeholder. Bob's app answers nothing itself.
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let (l2, lb2, outs) = link(&a2, &b);
    assert!(
        outs.iter().all(|o| o.media_requests.is_empty()),
        "served from the kept copy, no app round trip"
    );
    let mut files: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    files.extend(drain(&lb2, &l2).iter().flat_map(|o| o.files.clone()));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].transfer_id_hex, offer.transfer_id_hex);
    assert_eq!(files[0].bytes, PIC);
    assert_eq!(
        blake3::hash(&files[0].bytes),
        blake3::hash(PIC),
        "hash verified"
    );

    // A (forged) request for the burn item is never served.
    {
        let mut st = a2.inner.lock();
        let peer = parse_id(&bob).unwrap();
        st.queue_control(
            &peer,
            AppContent::MediaControl(omcp_core::burn::MediaControl::new(
                omcp_core::burn::CONTROL_MEDIA_REQUEST,
                parse_msg_id(&once).unwrap(),
                *blake3::hash(b"view-once").as_bytes(),
            )),
        )
        .unwrap();
    }
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let outs2 = drain(&lb2, &l2);
    assert!(outs
        .iter()
        .chain(outs2.iter())
        .all(|o| o.files.is_empty() && o.media_requests.is_empty()));

    // Limits are configurable; shrinking them evicts oldest first.
    assert_eq!(
        b.set_kept_media_settings(
            KeptMediaSettings {
                max_days: 90,
                max_bytes: 10
            },
            now_ms()
        )
        .unwrap(),
        1
    );
    assert_eq!(b.kept_media_usage().unwrap().count, 0);
}

#[test]
fn cb3_refetch_with_wrong_bytes_is_rejected() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let offer = b
        .queue_file(alice, "pic.jpg".into(), "image/jpeg".into(), PIC.to_vec())
        .unwrap();
    drain(&lb, &la);
    // No kept copy: the misbehaving partner below answers by hand.
    b.inner
        .lock()
        .db()
        .delete_kept_media(&parse_msg_id(&offer.transfer_id_hex).unwrap())
        .unwrap();
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let (l2, lb2, _) = link(&a2, &b);
    // A misbehaving partner sends other bytes under the same locator.
    let mut st = b.inner.lock();
    let peer = st.contacts.keys().next().copied().unwrap();
    let f = files::build_outgoing(
        parse_msg_id(&offer.transfer_id_hex).unwrap(),
        "pic.jpg",
        "image/jpeg",
        b"evil",
    )
    .unwrap();
    st.files.out.entry(peer).or_default().push_back(f);
    drop(st);
    let outs = drain(&lb2, &l2);
    assert!(outs.iter().all(|o| o.files.is_empty()));
    assert!(outs
        .iter()
        .any(|o| o.file_failures.iter().any(|e| e.reason == "integrity")));
}

#[test]
fn cb5_media_backup_on_restores_hash_verified_off_uploads_nothing() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let offer = b
        .queue_file(alice, "pic.jpg".into(), "image/jpeg".into(), PIC.to_vec())
        .unwrap();
    drain(&lb, &la);
    a.set_received_file_path(offer.transfer_id_hex.clone(), Some("/a/pic.jpg".into()))
        .unwrap();
    // Off by default: nothing to upload, sealing refused.
    let s = a.media_backup_settings().unwrap();
    assert!(!s.enabled && s.photos && s.voice && s.files && !s.videos && !s.mobile_data);
    assert!(a.media_backup_todo().unwrap().is_empty());
    assert!(!a.media_backup_due(100_000_000, true).unwrap());
    let item_hash = hex::encode(blake3::hash(PIC).as_bytes());
    assert!(a
        .seal_media_backup(item_hash.clone(), PIC.to_vec())
        .is_err());
    // On.
    a.set_media_backup_settings(MediaBackupSettings { enabled: true, ..s })
        .unwrap();
    assert!(a.media_backup_due(100_000_000, true).unwrap());
    let todo = a.media_backup_todo().unwrap();
    assert_eq!(todo.len(), 1);
    assert_eq!(todo[0].hash_hex, item_hash);
    let mb = a
        .seal_media_backup(todo[0].hash_hex.clone(), PIC.to_vec())
        .unwrap();
    assert!(!has(&mb.blob, PIC));
    a.media_backup_commit(todo[0].hash_hex.clone(), 5).unwrap();
    assert!(a.media_backup_todo().unwrap().is_empty());
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let ph = a2.media_placeholders().unwrap();
    assert_eq!(ph.len(), 1);
    assert_eq!(ph[0].blob_name, mb.name);
    let bytes = a2
        .restore_media_from_backup(ph[0].msg_id_hex.clone(), mb.blob.clone())
        .unwrap();
    assert_eq!(bytes, PIC, "hash verified");
    assert!(a2
        .restore_media_from_backup(ph[0].msg_id_hex.clone(), blobs[0].clone())
        .is_err());
    // Turning it off hands back the blobs to delete.
    let del = a
        .set_media_backup_settings(MediaBackupSettings {
            enabled: false,
            ..s
        })
        .unwrap();
    assert_eq!(del, vec![mb.name]);
}

#[test]
fn rs1_contact_sees_unsuppressible_notice_and_old_key_superseded() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let old_dev = a.device_info().unwrap().device_key_hex;
    assert!(b.contacts()[0].restored_at_ms.is_none());
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    let info = a2.device_info().unwrap();
    assert_ne!(info.device_key_hex, old_dev);
    assert!(
        info.restored_at_ms.is_some(),
        "banner on the restored phone"
    );
    let t0 = Instant::now();
    let (l2, lb2, outs) = link(&a2, &b);
    let notices: Vec<_> = outs
        .iter()
        .flat_map(|o| o.restore_notices.clone())
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(t0.elapsed().as_secs() < 5, "RS1 within 5 s");
    assert_eq!(notices[0].contact_id, alice);
    assert_eq!(notices[0].restored_at_ms, info.restored_at_ms);
    let c = b
        .contacts()
        .into_iter()
        .find(|c| c.contact_id == alice)
        .unwrap();
    assert!(c.restored_badge && c.restored_unverified && !c.verified);
    let sys: Vec<_> = b
        .stored_messages(alice.clone())
        .unwrap()
        .into_iter()
        .filter(|m| m.status == MessageStatus::System)
        .collect();
    assert_eq!(sys.len(), 1);
    assert!(sys[0].text.starts_with("restored:"));
    // Old device key superseded: the old install can no longer connect.
    {
        let st = b.inner.lock();
        let d = &st.contacts.values().next().unwrap().device;
        assert!(d.is_superseded(&hex::decode(&old_dev).unwrap().try_into().unwrap()));
    }
    let lo = a.open_initiator_link(b.identity().peer_id_hex).unwrap();
    let lbo = b.open_responder_link().unwrap();
    let mut failed = false;
    for p in lo.start().unwrap().outgoing {
        failed |= lbo.on_pdu(p).is_err();
    }
    assert!(failed && !lbo.is_established());
    // Reconnecting with the same new device doesn't repeat the notice.
    l2.shutdown();
    lb2.shutdown();
    let (_l3, _lb3, outs) = link(&a2, &b);
    assert!(outs.iter().all(|o| o.restore_notices.is_empty()));
}

#[test]
fn rs2_safety_number_changes_verified_cleared_old_qr_fails() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let old_qr = a.qr_payload();
    b.verify_contact_from_qr(old_qr.clone()).unwrap();
    a.set_verified(b.identity().peer_id_hex, true).unwrap();
    let before = b.contacts()[0].safety_number.clone();
    assert_eq!(before, a.contacts()[0].safety_number, "both sides agree");
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    // O3: our own verification of Bob carries over ("Still verified on your side").
    assert!(a2.contacts().iter().all(|c| c.verified));
    link(&a2, &b);
    let c = b
        .contacts()
        .into_iter()
        .find(|c| c.contact_id == alice)
        .unwrap();
    assert!(!c.verified, "contact side cleared");
    assert_ne!(c.safety_number, before);
    assert_eq!(c.safety_number, a2.contacts()[0].safety_number);
    assert!(matches!(
        b.verify_contact_from_qr(old_qr),
        Err(FfiError::VerificationFailed)
    ));
    let v = b.verify_contact_from_qr(a2.qr_payload()).unwrap();
    assert!(v.verified && !v.restored_unverified);
}

// ---- v0.1.4a: device binding (c) and PeerID / migration (d) ----

/// Send a hand-built message 1 (initiator static `id`, payload from `payload(h)`)
/// to a fresh responder link of `b`. Returns whether `b` refused it.
fn raw_msg1_refused(
    id: StaticIdentity,
    b: &OmcpNode,
    b_dh: [u8; 32],
    payload: impl FnOnce(&[u8]) -> omcp_core::error::Result<Vec<u8>>,
) -> bool {
    let mut init = HybridInitiator::new(id, &b_dh).unwrap();
    let m1 = init.write_message_1_with(payload).unwrap();
    let mut pdus = Vec::new();
    chunk_handshake(&m1, &mut pdus).unwrap();
    let lb = b.open_responder_link().unwrap();
    let mut refused = false;
    for p in pdus {
        refused |= lb.on_pdu(p).is_err();
    }
    refused && !lb.is_established()
}

#[test]
fn c_missing_certificate_or_binding_fails_the_handshake() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let b_dh = b.inner.dh_public();
    // No certificate at all (a pre-v0.1.4a or modified client).
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |_| { Ok(b"omcp-ffi-v1".to_vec()) }
    ));
    // Certificate without the device-key signature over the handshake hash.
    let cert = a.inner.device().cert().unwrap().encode();
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |_| {
            let mut p = b"omcp-ffi-v1\x01".to_vec();
            p.extend_from_slice(&cert);
            Ok(p)
        }
    ));
    // The genuine payload still works.
    assert!(!raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |h| { a.inner.handshake_payload(h) }
    ));
}

#[test]
fn c_replayed_old_certificate_is_rejected_and_notice_still_shows() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    // An attacker captured a payload of the old device from an earlier session.
    let old_payload = a
        .inner
        .handshake_payload(b"an earlier session hash")
        .unwrap();
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    let b_dh = b.inner.dh_public();
    // A modified restored client replays it (with its own Noise key): rejected,
    // the binding signature is over another handshake hash and the cert's
    // X25519 key isn't this session's.
    let replay = old_payload.clone();
    assert!(raw_msg1_refused(
        a2.inner.identity().unwrap(),
        &b,
        b_dh,
        move |_| Ok(replay)
    ));
    // Even with the old Noise key it can't sign the fresh hash with the old device key.
    let replay = old_payload;
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        move |_| Ok(replay)
    ));
    // Nothing was applied: the genuine restored device still raises the notice.
    let (_l, _lb, outs) = link(&a2, &b);
    assert_eq!(
        outs.iter().map(|o| o.restore_notices.len()).sum::<usize>(),
        1
    );
    // And the old device key, now superseded, can't connect even signing a fresh hash.
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b.inner.dh_public(),
        |h| { a.inner.handshake_payload(h) }
    ));
}

#[test]
fn d_peer_id_is_blake2b_of_identity_key_and_contacts_key_by_it() {
    use blake2::digest::consts::U32;
    use blake2::{Blake2b, Digest};
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let ik = omcp_core::device::identity_signing_key(&a.inner.seed)
        .unwrap()
        .verifying_key()
        .to_bytes();
    let mut h = Blake2b::<U32>::new();
    h.update(ik);
    let want = hex::encode::<[u8; 32]>(h.finalize().into());
    assert_eq!(a.identity().peer_id_hex, want);
    assert_eq!(b.contacts()[0].contact_id, want);
    // X25519 is per device and not the id.
    assert_ne!(hex::encode(a.inner.dh_public()), want);
    let qr = a.qr_payload();
    assert!(qr.starts_with(&format!("omcp:3:{want}:{}:", hex::encode(ik))));
    // A QR whose identity key doesn't hash to the PeerID is refused.
    let bad = qr.replacen(&want, &"00".repeat(32), 1);
    assert!(b.preview_contact_from_qr(bad).is_err());
    // The advertisement carries the PeerID prefix and maps back to the contact.
    let adv = a.advertisement();
    let prefix = parse_advertisement(adv).unwrap();
    assert_eq!(b.contact_for_prefix(prefix), Some(want));
}

/// A store as written before v0.1.4a: owner and contact ids are X25519 keys.
fn legacy_store(dir: &std::path::Path, own_phrase: &str, peer_phrase: &str, peer_name: &str) {
    let own = recovery::phrase_to_seed(own_phrase).unwrap();
    let peer = recovery::phrase_to_seed(peer_phrase).unwrap();
    let db = Store::open(&dir.join("omcp.db"), &SKEY).unwrap();
    db.bind_owner(&omcp_core::device::legacy_dh_public(&own).unwrap())
        .unwrap();
    let pid = omcp_core::device::legacy_dh_public(&peer).unwrap();
    db.put_contact(&st::ContactRec {
        peer: pid,
        name: peer_name.into(),
        verified: true,
        device: None,
        dh: None,
    })
    .unwrap();
    db.put_message(&st::MessageRec {
        id: [7; 16],
        peer: pid,
        mine: true,
        time_ms: 10,
        status: st::STATUS_SENT,
        text: format!("hi {peer_name}, from before the upgrade"),
        mime: None,
        burn: None,
        burn_state: st::BURN_NONE,
        burn_at_ms: None,
        file: None,
    })
    .unwrap();
}

#[test]
fn d_legacy_store_migrates_to_peer_ids_on_first_handshake() {
    let pa = generate_recovery_phrase().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    legacy_store(da.path(), &pa, &pb, "Bob");
    legacy_store(db.path(), &pb, &pa, "Alice");
    let a = open_node(&pa, "Alice", da.path());
    let b = open_node(&pb, "Bob", db.path());
    // Migrated installs keep their X25519 key, so legacy contacts can still dial.
    let legacy_b = hex::encode(
        omcp_core::device::legacy_dh_public(&recovery::phrase_to_seed(&pb).unwrap()).unwrap(),
    );
    assert_eq!(hex::encode(b.inner.dh_public()), legacy_b);
    assert_eq!(a.contacts()[0].contact_id, legacy_b);
    let sn_before = a.contacts()[0].safety_number.clone();
    assert_eq!(sn_before, b.contacts()[0].safety_number);
    // Legacy contacts still find a migrated install by its advertisement.
    assert_eq!(
        a.contact_for_prefix(parse_advertisement(b.advertisement()).unwrap()),
        Some(legacy_b.clone())
    );
    let la = a.open_initiator_link(legacy_b).unwrap();
    let lb = b.open_responder_link().unwrap();
    pump_out(&la, &lb, la.start().unwrap());
    assert!(la.is_established() && lb.is_established());
    let (pid_a, pid_b) = (a.identity().peer_id_hex, b.identity().peer_id_hex);
    assert_eq!(a.contacts()[0].contact_id, pid_b);
    assert_eq!(b.contacts()[0].contact_id, pid_a);
    assert!(
        a.contacts()[0].verified && b.contacts()[0].verified,
        "verification kept"
    );
    assert!(
        b.contacts()[0].restored_at_ms.is_none(),
        "an upgrade is not a restore"
    );
    assert_eq!(a.contacts()[0].safety_number, b.contacts()[0].safety_number);
    let ms = a.stored_messages(pid_b.clone()).unwrap();
    assert!(ms[0].text.contains("before the upgrade"));
    // Persisted: survives a restart under the new ids.
    drop((la, lb, a));
    let a = open_node(&pa, "Alice", da.path());
    assert_eq!(a.contacts()[0].contact_id, pid_b);
    assert_eq!(a.stored_messages(pid_b).unwrap().len(), 1);
}

#[test]
fn d_old_identity_backup_restores_legacy_contacts() {
    let r = restored(backup::BackupContents {
        entropy: Zeroizing::new([5; 32]),
        contacts: vec![backup::BackupContact {
            peer_id: [9; 32],
            name: "Old".into(),
            verified: true,
            dh: None,
        }],
        display_name: None,
        history: None,
    })
    .unwrap();
    assert_eq!(r.contacts[0].dh_hex, None);
    let n = node("Me");
    let c = &r.contacts[0];
    n.restore_contact_with_key(
        c.contact_id.clone(),
        c.display_name.clone(),
        c.verified,
        None,
    )
    .unwrap();
    // A legacy contact: dialled by its id (the X25519 key) until re-keyed.
    assert_eq!(n.inner.lock().contacts[&[9; 32]].dh, [9; 32]);
}

// ---- multi-hop mesh relay (PROTOCOL §22 on BLE gossip) ----

/// Flush every link in both directions until no PDUs move.
fn settle(links: &[(Arc<MeshLink>, Arc<MeshLink>)]) -> Vec<LinkOutput> {
    let mut outs = Vec::new();
    for _ in 0..64 {
        let mut moved = false;
        for (x, y) in links {
            for (from, to) in [(x, y), (y, x)] {
                let o = from.flush().unwrap();
                moved |= !o.outgoing.is_empty();
                outs.extend(pump_out(from, to, o));
            }
        }
        if !moved {
            return outs;
        }
    }
    panic!("mesh did not settle");
}

fn texts(outs: &[LinkOutput]) -> Vec<ReceivedText> {
    outs.iter().flat_map(|o| o.received.clone()).collect()
}

fn befriend(x: &Arc<OmcpNode>, y: &Arc<OmcpNode>) {
    x.add_contact_from_qr(y.qr_payload()).unwrap();
    y.add_contact_from_qr(x.qr_payload()).unwrap();
}

fn up(a: &Arc<OmcpNode>, b: &Arc<OmcpNode>) -> (Arc<MeshLink>, Arc<MeshLink>, Vec<LinkOutput>) {
    let (la, lb, mut outs) = link(a, b);
    outs.extend(settle(&[(la.clone(), lb.clone())]));
    (la, lb, outs)
}

fn down(l: &(Arc<MeshLink>, Arc<MeshLink>, Vec<LinkOutput>)) {
    l.0.shutdown();
    l.1.shutdown();
}

/// Mutual QR add plus one direct link: the first handshake creates the 1:1
/// MLS group (§22.8(1)). Afterwards the two may never be in range again.
fn befriend_mls(x: &Arc<OmcpNode>, y: &Arc<OmcpNode>) {
    befriend(x, y);
    let l = up(x, y);
    down(&l);
    assert!(x.mesh_ready(y.identity().peer_id_hex).unwrap());
    assert!(y.mesh_ready(x.identity().peer_id_hex).unwrap());
}

fn status_of(n: &OmcpNode, contact: &str, id: &str) -> MessageStatus {
    n.stored_messages(contact.to_string())
        .unwrap()
        .into_iter()
        .find(|m| m.id_hex == id)
        .unwrap()
        .status
}

#[test]
fn mesh_chain_a_b_c_store_and_forward_end_to_end() {
    // A and C added each other (QR + first handshake: MLS group), then are
    // never in range again; B is a friend of both.
    let dir = tempfile::tempdir().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let a = node("Alice");
    let mut b = open_node(&pb, "Bob", dir.path());
    let c = node("Carol");
    befriend(&a, &b);
    befriend(&b, &c);
    befriend_mls(&a, &c);
    let aid = a.identity().peer_id_hex;
    let cid = c.identity().peer_id_hex;
    // RL1/RL2: always on, automatic 500 MB cap.
    assert_eq!(b.mesh_stats().storage_cap_bytes, mesh_relay_cap_bytes());

    // A meets B and sends to C: B carries an opaque envelope.
    let secret = "meet at the north gate 7pm";
    let mid = a.queue_message(cid.clone(), secret.into(), None).unwrap();
    assert_eq!(a.mesh_stats().own_pending, 1);
    assert_eq!(status_of(&a, &cid, &mid), MessageStatus::Queued);
    let ab = up(&a, &b);
    assert!(texts(&ab.2).is_empty(), "B must not receive A→C");
    assert_eq!(b.mesh_stats().carried, 1);
    // Handed to a relay: Sent.
    assert_eq!(status_of(&a, &cid, &mid), MessageStatus::Sent);
    let ups = a.mesh_take_status_updates();
    assert!(ups
        .iter()
        .any(|u| u.id_hex == mid && u.status == MessageStatus::Sent));
    assert_eq!(c.queued_for(aid.clone()), 0);
    down(&ab);

    // B restarts: the carry store survives, sealed at rest, without plaintext.
    drop(ab);
    drop(b);
    let disk = disk_bytes(dir.path());
    assert!(
        !has(&disk, secret.as_bytes()),
        "relay store holds no plaintext"
    );
    b = open_node(&pb, "Bob", dir.path());
    assert_eq!(b.mesh_stats().carried, 1);
    assert!(b.stored_messages(cid.clone()).unwrap().is_empty());

    // Later B meets C: delivered once, attributed to A.
    let bc = up(&b, &c);
    let got = texts(&bc.2);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, secret);
    assert_eq!(got[0].contact_id, aid);
    let stored = c.stored_messages(aid.clone()).unwrap();
    assert_eq!(stored.len(), 1);
    // C's DELIVERY_ACK reached B: B dropped its copy and now carries the ack.
    assert_eq!(b.mesh_stats().carried, 1, "only the ack is carried");
    down(&bc);
    // Meeting again does not deliver twice.
    let bc2 = up(&b, &c);
    assert!(texts(&bc2.2).is_empty());
    down(&bc2);

    // C replies the same way (C→B, then B→A).
    c.queue_text(aid.clone(), "ok".into()).unwrap();
    let cb = up(&c, &b);
    assert!(texts(&cb.2).is_empty());
    down(&cb);
    let ba = up(&b, &a);
    let got = texts(&ba.2);
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].text.as_str(), got[0].contact_id.as_str()),
        ("ok", cid.as_str())
    );
    // The end-to-end ack arrived with it: Delivered, own copy withdrawn.
    assert_eq!(status_of(&a, &cid, &mid), MessageStatus::Delivered);
    assert!(a
        .mesh_take_status_updates()
        .iter()
        .any(|u| u.id_hex == mid && u.status == MessageStatus::Delivered));
    assert_eq!(a.mesh_stats().own_pending, 1, "only A's ack for \"ok\"");
    down(&ba);
}

#[test]
fn mesh_waits_for_mls_group_before_relaying() {
    // Contacts by QR only: no 1:1 MLS group yet, so nothing goes on the mesh.
    let (a, b, c) = (node("A"), node("B"), node("C"));
    befriend(&a, &b);
    befriend(&a, &c);
    let cid = c.identity().peer_id_hex;
    let aid = a.identity().peer_id_hex;
    assert!(!a.mesh_ready(cid.clone()).unwrap());
    let mid = a.queue_message(cid.clone(), "later".into(), None).unwrap();
    assert_eq!(a.mesh_stats().own_pending, 0);
    let ab = up(&a, &b);
    down(&ab);
    assert_eq!(b.mesh_stats().carried, 0);
    assert_eq!(a.queued_for(cid.clone()), 1, "still in the local queue");
    // First direct link: group created (existing contacts migrate here) and
    // the queue is delivered directly.
    let ac = up(&a, &c);
    assert_eq!(texts(&ac.2).len(), 1);
    assert!(a.mesh_ready(cid.clone()).unwrap() && c.mesh_ready(aid).unwrap());
    assert_eq!(status_of(&a, &cid, &mid), MessageStatus::Sent);
    down(&ac);
}

#[test]
fn mesh_device_key_rotation_mid_transit_still_delivers() {
    let (a, b, c) = (node("A"), node("B"), node("C"));
    befriend(&a, &b);
    befriend(&b, &c);
    befriend_mls(&a, &c);
    let aid = a.identity().peer_id_hex;
    let cid = c.identity().peer_id_hex;
    a.queue_text(cid.clone(), "across a rotation".into())
        .unwrap();
    let ab = up(&a, &b);
    down(&ab);
    assert_eq!(b.mesh_stats().carried, 1);
    // C rotates its device key while the envelope is with B.
    let old = c.device_info().unwrap().fingerprint;
    c.rotate_device_key().unwrap();
    assert_ne!(c.device_info().unwrap().fingerprint, old);
    // B still knows C's previous key and dials it: C answers with it.
    let bc = up(&b, &c);
    let got = texts(&bc.2);
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].text.as_str(), got[0].contact_id.as_str()),
        ("across a rotation", aid.as_str())
    );
    down(&bc);
    // A (also holding the old key) reaches C directly, and C's new key works
    // when C dials.
    let ac = up(&a, &c);
    down(&ac);
    let ca = up(&c, &a);
    down(&ca);
    c.queue_text(aid.clone(), "after".into()).unwrap();
    let cb = up(&c, &b);
    down(&cb);
    let ba = up(&b, &a);
    assert!(texts(&ba.2).iter().any(|t| t.text == "after"));
    down(&ba);
}

#[test]
fn mesh_live_three_hop_line() {
    // A — B — D — C, all links up at once; A and C are not neighbours.
    let (a, b, d, c) = (node("A"), node("B"), node("D"), node("C"));
    befriend(&a, &b);
    befriend(&b, &d);
    befriend(&d, &c);
    befriend_mls(&a, &c);
    let cid = c.identity().peer_id_hex;
    let aid = a.identity().peer_id_hex;
    let ab = up(&a, &b);
    let bd = up(&b, &d);
    let dc = up(&d, &c);
    let links = [
        (ab.0.clone(), ab.1.clone()),
        (bd.0.clone(), bd.1.clone()),
        (dc.0.clone(), dc.1.clone()),
    ];
    a.queue_text(cid.clone(), "three hops".into()).unwrap();
    assert!(!a.mesh_pending_links().is_empty());
    let outs = settle(&links);
    let got = texts(&outs);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        (got[0].text.as_str(), got[0].contact_id.as_str()),
        ("three hops", aid.as_str())
    );
    // RL1: every relay passes the next one on too (no switch to stop it).
    a.queue_text(cid.clone(), "again".into()).unwrap();
    let outs = settle(&links);
    assert_eq!(texts(&outs).len(), 1);
    let _ = mesh_max_links();
    // Beacon is a 6-byte HELLO.
    let beacon = a.mesh_beacon();
    assert_eq!(beacon.len(), 6);
    assert_eq!(beacon[0], 0x48);
    assert!(a.mesh_tick(now_ms()).is_ok());
}

#[test]
fn mesh_burn_after_read_crosses_relay_and_opened_returns() {
    let (a, b, c) = (node("A"), node("B"), node("C"));
    befriend(&a, &b);
    befriend(&b, &c);
    befriend_mls(&a, &c);
    let aid = a.identity().peer_id_hex;
    let cid = c.identity().peer_id_hex;
    let id = a
        .queue_message(
            cid.clone(),
            "self-destruct".into(),
            Some(spec(BurnMode::OnOpen, 0, 3600)),
        )
        .unwrap();
    let ab = up(&a, &b);
    down(&ab);
    let bc = up(&b, &c);
    let got = texts(&bc.2);
    assert_eq!(got.len(), 1);
    assert!(got[0].burn.is_some());
    assert!(got[0].text.is_empty(), "burn content is not in the event");
    down(&bc);
    let content = c.open_burn(id.clone(), now_ms()).unwrap().unwrap();
    assert_eq!(content.text, "self-destruct");
    c.close_burn(id.clone()).unwrap();
    // OPENED travels back C → B → A.
    let cb = up(&c, &b);
    down(&cb);
    let ba = up(&b, &a);
    let opened: Vec<OpenedEvent> = ba.2.iter().flat_map(|o| o.opened.clone()).collect();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].contact_id, cid);
    assert_eq!(opened[0].msg_id_hex, id);
    assert!(!a.burn_is_live(id.clone()));
    down(&ba);
    let _ = aid;
}

#[test]
fn mesh_direct_delivery_and_mesh_copy_do_not_duplicate() {
    let (a, b, c) = (node("A"), node("B"), node("C"));
    befriend(&a, &b);
    befriend(&b, &c);
    befriend_mls(&a, &c);
    let cid = c.identity().peer_id_hex;
    a.queue_text(cid.clone(), "once".into()).unwrap();
    let ab = up(&a, &b); // B carries a copy
    down(&ab);
    let ac = up(&a, &c); // direct delivery
    assert_eq!(texts(&ac.2).len(), 1);
    assert_eq!(a.mesh_stats().own_pending, 0, "own mesh copy withdrawn");
    down(&ac);
    let bc = up(&b, &c); // the relayed copy is dropped as a duplicate
    assert!(texts(&bc.2).is_empty());
}

// ---- Relay RL1–RL4 (apps/REQUIREMENTS.md "Relay (store and forward)") ----

/// A, B, C where only B is ever between A and C; A has queued one message
/// for C and handed it to B. Returns (a, b, c, cid, B's db dir).
fn carried_by_b() -> (
    Arc<OmcpNode>,
    Arc<OmcpNode>,
    Arc<OmcpNode>,
    String,
    tempfile::TempDir,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let a = node("A");
    let b = open_node(&pb, "B", dir.path());
    let c = node("C");
    befriend(&a, &b);
    befriend(&b, &c);
    befriend_mls(&a, &c);
    let cid = c.identity().peer_id_hex;
    a.queue_text(cid.clone(), "carried".into()).unwrap();
    let ab = up(&a, &b);
    down(&ab);
    assert_eq!(b.mesh_stats().carried, 1);
    (a, b, c, cid, dir, pb)
}

#[test]
fn rl1_relay_always_on_old_off_setting_ignored() {
    let (_a, b, c, _cid, dir, pb) = carried_by_b();
    // A row left by the removed switch ("relay off", 10 MB) has no effect.
    b.inner
        .lock()
        .db
        .put_meta(b"mesh-settings", &(false, 10_000_000u64))
        .unwrap();
    drop(b);
    let b = open_node(&pb, "B", dir.path());
    assert_eq!(b.mesh_stats().carried, 1, "still carrying after restart");
    assert_eq!(b.mesh_stats().storage_cap_bytes, mesh_relay_cap_bytes());
    let bc = up(&b, &c);
    assert_eq!(texts(&bc.2).len(), 1);
    down(&bc);
}

#[test]
fn rl2_automatic_cap_500_mb_lowered_on_low_storage() {
    assert_eq!(mesh_relay_cap_bytes(), 500_000_000);
    let (_a, b, c, _cid, dir, pb) = carried_by_b();
    let gb = 1_000_000_000u64;
    b.mesh_set_free_storage(64 * gb, 128 * gb).unwrap();
    assert_eq!(b.mesh_stats().storage_cap_bytes, 500_000_000);
    b.mesh_set_free_storage(5 * gb, 100 * gb).unwrap();
    assert_eq!(b.mesh_stats().storage_cap_bytes, 250_000_000);
    assert_eq!(b.mesh_stats().carried, 1);
    // Disk full: the carried envelope is dropped, also on disk.
    b.mesh_set_free_storage(0, 100 * gb).unwrap();
    assert_eq!(b.mesh_stats().carried, 0);
    assert_eq!(b.mesh_stats().storage_used_bytes, 0);
    drop(b);
    let b = open_node(&pb, "B", dir.path());
    assert_eq!(b.mesh_stats().carried, 0);
    let bc = up(&b, &c);
    assert!(texts(&bc.2).is_empty());
    down(&bc);
}

#[test]
fn rl2_carry_class_persisted_and_legacy_rows_mapped() {
    let (_a, b, _c, _cid, _dir, _pb) = carried_by_b();
    let rows = b.inner.lock().db.carry_items().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].class,
        Some(omcp_core::sync::budget::RelayClass::Contact.to_u8())
    );
    let mut legacy = rows[0].clone();
    legacy.class = None;
    assert_eq!(
        mesh::carry_class(&legacy),
        omcp_core::sync::budget::RelayClass::Contact
    );
    legacy.own = true;
    assert_eq!(
        mesh::carry_class(&legacy),
        omcp_core::sync::budget::RelayClass::OwnChat
    );
}

#[test]
fn rl3_carried_copy_deleted_on_delivery_ack() {
    let (_a, b, c, _cid, _dir, _pb) = carried_by_b();
    let before = b.mesh_stats();
    let bc = up(&b, &c);
    assert_eq!(texts(&bc.2).len(), 1);
    down(&bc);
    // C's ack reached B: the message is gone, only the (small) ack is carried.
    let after = b.mesh_stats();
    assert_eq!(after.carried, 1);
    assert!(b
        .inner
        .lock()
        .db
        .carry_items()
        .unwrap()
        .iter()
        .all(|r| r.ack_of.is_some()));
    assert!(after.carried_bytes <= before.carried_bytes);
}

#[test]
fn rl4_storage_used_for_relaying_reported() {
    let fresh = node("Z");
    assert_eq!(fresh.mesh_stats().storage_used_bytes, 0);
    let (a, b, _c, _cid, _dir, _pb) = carried_by_b();
    let s = b.mesh_stats();
    assert!(s.storage_used_bytes > 0);
    // B relays only for others: everything it stores is relayed traffic.
    assert_eq!(s.storage_used_bytes, s.carried_bytes);
    // A's own pending copy counts towards A's relay storage as well.
    let sa = a.mesh_stats();
    assert_eq!(sa.own_pending, 1);
    assert!(sa.storage_used_bytes > sa.carried_bytes);
}

// ---- Link PDU registry (PROTOCOL §21.9) ----

#[test]
fn mls_setup_is_link_pdu_0x04_per_registry() {
    // §21.9: 0x03 MESH_FRAME, 0x04 MLS_SETUP; 0x01/0x02 are the handshake
    // and link-data PDUs. No two kinds share a value.
    assert_eq!(mesh::PDU_MLS_SETUP, 0x04);
    assert_eq!(mesh::PDU_MESH, 0x03);
    let kinds = [PDU_HANDSHAKE, PDU_LINK, mesh::PDU_MESH, mesh::PDU_MLS_SETUP];
    let uniq: std::collections::BTreeSet<u8> = kinds.iter().copied().collect();
    assert_eq!(uniq.len(), kinds.len());
    let setup = [
        mesh::SETUP_KEY_PACKAGE,
        mesh::SETUP_WELCOME,
        mesh::SETUP_JOINED,
    ];
    assert_eq!(setup, [0x01, 0x02, 0x03]);
    // On the wire: the first direct link carries MLS setup as 0x04 PDUs and
    // nothing outside the registry.
    let (a, b) = (node("A"), node("B"));
    befriend(&a, &b);
    let l = up(&a, &b);
    let first: Vec<u8> =
        l.2.iter()
            .flat_map(|o| o.outgoing.iter().map(|p| p[0]))
            .collect();
    assert!(first.contains(&0x04), "{first:?}");
    assert!(first.iter().all(|k| uniq.contains(k)), "{first:?}");
    assert!(a.mesh_ready(b.identity().peer_id_hex).unwrap());
    down(&l);
    // Unknown PDU kinds are refused.
    let lb = b.open_responder_link().unwrap();
    assert!(lb.on_pdu(vec![0x05, 0, 0]).is_err());
}

// ---- Hopla News (REQUIREMENTS § Hopla News) ----

const NEWS_FIXTURE: &str =
    include_str!("../../omcp-core/src/news/fixtures/google_news_world_en.xml");

/// RFC 822 date for unix ms (GMT).
fn rfc822(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil from days (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "Mon, {d:02} {} {y} {:02}:{:02}:{:02} GMT",
        MON[(m - 1) as usize],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The core fixture with its dates moved to "now": item k is k × 10 min old,
/// except the fixture's week-old item, which stays expired.
fn news_feed_now() -> String {
    let now = now_ms();
    let mut out = String::new();
    let mut rest = NEWS_FIXTURE;
    let mut k = 0u64;
    while let Some(i) = rest.find("<pubDate>") {
        let j = rest[i..].find("</pubDate>").unwrap() + i;
        out.push_str(&rest[..i + 9]);
        let old = &rest[i + 9..j];
        let ms = if old.contains("17 Oct") {
            now - 8 * 24 * 3_600_000
        } else {
            now - (k + 1) * 600_000
        };
        out.push_str(&rfc822(ms));
        k += 1;
        rest = &rest[j..];
    }
    out.push_str(rest);
    out
}

fn world_en_url() -> String {
    "https://news.google.com/rss/headlines/section/topic/WORLD?hl=en-US&gl=US&ceid=US:en".into()
}

#[test]
fn news_prefs_default_world_headlines_and_due_feeds() {
    let n = node("N");
    let p = n.news_prefs();
    assert_eq!(p.topics, vec!["world", "emergencies"]);
    assert_eq!(p.langs, vec!["en"]);
    assert!(p.alerts_notify, "emergency alerts notify by default");
    let now = now_ms();
    let due = n.news_due_feeds(now);
    assert_eq!(due.len(), 2);
    assert!(due.contains(&world_en_url()));
    assert!(due
        .iter()
        .all(|u| u.starts_with("https://news.google.com/rss")));
    // Asked for: not due again before the retry delay; after a successful
    // ingest, not before 30 min.
    assert!(n.news_due_feeds(now + 60_000).is_empty());
    assert_eq!(n.news_due_feeds(now + news::FETCH_RETRY_MS).len(), 2);
    n.news_ingest_rss(world_en_url(), news_feed_now(), now)
        .unwrap();
    let later = now + 2 * news::FETCH_RETRY_MS;
    assert!(!n.news_due_feeds(later).contains(&world_en_url()));
    assert!(n
        .news_due_feeds(now + news_fetch_interval_ms())
        .contains(&world_en_url()));
    // City + French: local feed per language; bad input refused.
    let p2 = n
        .news_set_prefs(NewsPrefs {
            city: Some("  Riverside ".into()),
            langs: vec!["fr".into(), "en".into()],
            ..p.clone()
        })
        .unwrap();
    assert_eq!(p2.city.as_deref(), Some("Riverside"));
    let due = n.news_due_feeds(now + 10 * news_fetch_interval_ms());
    assert!(due.contains(
        &"https://news.google.com/rss/search?q=riverside&hl=fr&gl=FR&ceid=FR:fr".to_string()
    ));
    assert_eq!(due.len(), 6);
    for bad in [
        NewsPrefs {
            topics: vec!["reports".into()],
            ..p.clone()
        },
        NewsPrefs {
            topics: vec!["local".into()],
            ..p.clone()
        },
        NewsPrefs {
            langs: vec!["de".into()],
            ..p.clone()
        },
        NewsPrefs {
            city: Some("a/b".into()),
            ..p.clone()
        },
    ] {
        assert!(n.news_set_prefs(bad).is_err());
    }
}

#[test]
fn nw2_ingest_dedups_by_source_and_link_and_relays_to_neighbours_once() {
    let dir = tempfile::tempdir().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let a = node("Ana");
    let b = open_node(&pb, "Ben", dir.path());
    befriend(&a, &b);
    let now = now_ms();
    assert_eq!(
        a.news_ingest_rss(world_en_url(), news_feed_now(), now)
            .unwrap(),
        3
    );
    // Same feed again (or another phone's copy): nothing new.
    assert_eq!(
        a.news_ingest_rss(world_en_url(), news_feed_now(), now)
            .unwrap(),
        0
    );
    let mine = a.news_feed("top".into()).unwrap();
    assert_eq!(mine.len(), 3);
    assert!(mine.iter().all(|s| s.fetched_here && s.via.is_none()));
    assert_eq!(mine[0].source_name, "Google News");
    assert_eq!(mine[0].title, "Riverside council opens new flood barrier");
    // B (offline until now) meets A once and catches up.
    let l = up(&a, &b);
    assert!(
        texts(&l.2).is_empty(),
        "news never shows up as chat messages"
    );
    let got = b.news_feed("top".into()).unwrap();
    assert_eq!(got.len(), 3, "each story once");
    assert!(got
        .iter()
        .all(|s| s.via.as_deref() == Some("Ana") && !s.fetched_here));
    assert_eq!(b.news_feed("world".into()).unwrap().len(), 3);
    assert!(b.news_feed("sport".into()).unwrap().is_empty());
    // B already holds A's stories: its own fetch of the same feed adds none.
    assert_eq!(
        b.news_ingest_rss(world_en_url(), news_feed_now(), now)
            .unwrap(),
        0
    );
    down(&l);
    // Stories, their class and the "via" survive a restart.
    let st = b.news_status();
    assert_eq!(st.stories, 3);
    assert!(st.storage_bytes > 0 && st.storage_cap_bytes == 20_000_000);
    drop(b);
    let b = open_node(&pb, "Ben", dir.path());
    let again = b.news_feed("top".into()).unwrap();
    assert_eq!(again, got);
}

#[test]
fn nw3_news_rows_are_stored_in_the_news_class() {
    let dir = tempfile::tempdir().unwrap();
    let pa = generate_recovery_phrase().unwrap();
    let a = open_node(&pa, "Ana", dir.path());
    a.news_ingest_rss(world_en_url(), news_feed_now(), now_ms())
        .unwrap();
    let rows = a.inner.lock().db.carry_items().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|r| !r.own && r.class == Some(omcp_core::sync::budget::RelayClass::News.to_u8())));
    assert_eq!(a.mesh_stats().own_pending, 0, "news is not an own chat");
}

#[test]
fn news_publish_ban_ingest_accepts_only_subscribed_google_news_feeds() {
    let n = node("N");
    let now = now_ms();
    let feed = news_feed_now();
    for url in [
        "https://rumours.example/rss".to_string(),
        "https://news.google.com/rss/headlines/section/topic/SPORTS?hl=en-US&gl=US&ceid=US:en"
            .into(),
        format!("{}&x=1", world_en_url()),
    ] {
        assert!(n.news_ingest_rss(url, feed.clone(), now).is_err());
    }
    assert!(n
        .news_ingest_rss(world_en_url(), "<html>not rss</html>".into(), now)
        .is_err());
    assert_eq!(n.news_status().stories, 0);
    // There is no other way to create news: the only producers are the
    // ingest above and relayed envelopes, which the engine checks
    // (core: news_publish_ban_relays_drop_news_not_from_the_rss_ingest_path).
}

#[test]
fn news_emergency_alerts_notify_by_default_and_can_be_turned_off() {
    let n = node("N");
    let now = now_ms();
    let url = "https://news.google.com/rss/search?q=%22emergency%20alert%22%20OR%20evacuation&hl=en-US&gl=US&ceid=US:en";
    assert!(n.news_due_feeds(now).contains(&url.to_string()));
    assert_eq!(
        n.news_ingest_rss(url.into(), news_feed_now(), now).unwrap(),
        3
    );
    let alerts = n.news_take_alerts();
    assert_eq!(alerts.len(), 3);
    assert!(alerts.iter().all(|s| s.emergency));
    assert!(n.news_take_alerts().is_empty());
    // Pinned first in Top.
    n.news_ingest_rss(world_en_url(), news_feed_now(), now)
        .unwrap();
    let top = n.news_feed("top".into()).unwrap();
    assert!(top.len() >= 3 && top[..3].iter().all(|s| s.emergency));
    let p = n.news_prefs();
    n.news_set_prefs(NewsPrefs {
        alerts_notify: false,
        ..p
    })
    .unwrap();
    assert!(n.news_take_alerts().is_empty());
}

#[test]
fn nw1_offline_phone_catches_up_from_one_neighbour_within_60s() {
    let a = node("Ana");
    let b = node("Ben");
    befriend(&a, &b);
    let now = now_ms();
    a.news_ingest_rss(world_en_url(), news_feed_now(), now)
        .unwrap();
    // B was offline for 72 h (never fetched); one encounter catches it up.
    let t0 = std::time::Instant::now();
    let l = up(&a, &b);
    let got = b.news_feed("top".into()).unwrap();
    assert_eq!(got.len(), 3);
    assert!(t0.elapsed().as_secs() < 60);
    down(&l);
}

#[test]
fn nw5_city_channel_subscribes_only_that_city() {
    let n = node("N");
    let p = n.news_prefs();
    n.news_set_prefs(NewsPrefs {
        city: Some("Lyon".into()),
        ..p
    })
    .unwrap();
    let due = n.news_due_feeds(now_ms());
    let local: Vec<_> = due
        .iter()
        .filter(|u| u.contains("Lyon") || u.contains("lyon"))
        .collect();
    assert!(!local.is_empty(), "the city's feed is subscribed: {due:?}");
    assert!(
        !due.iter().any(|u| u.contains("Paris")),
        "no other city channels"
    );
    // Location is never sent: only the city name appears in the feed URL.
    assert!(due
        .iter()
        .all(|u| !u.contains("lat") && !u.contains("geo=")));
}

#[test]
fn nw6_news_card_round_trips_through_a_chat_message() {
    let n = node("N");
    let now = now_ms();
    n.news_ingest_rss(world_en_url(), news_feed_now(), now)
        .unwrap();
    let s = n.news_feed("top".into()).unwrap()[0].clone();
    let text = news::news_card_text(s.clone());
    let (_a, _b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let got = pump(&la, &lb, la.send_text(text).unwrap());
    let card = news::parse_news_card(got[0].text.clone()).expect("card");
    assert_eq!(card.id_hex, s.id_hex);
    assert_eq!(card.link, s.link);
    assert_eq!(card.title, s.title);
    assert!(news::parse_news_card("hello".into()).is_none());
    assert!(news::parse_news_card(format!(
        "{}{}\ngn\nX\nhttp://x\nT",
        news::NEWS_CARD_PREFIX,
        s.id_hex
    ))
    .is_none());
}

#[test]
fn news_fetch_plan_applies_gateway_rules() {
    let n = node("N");
    let now = now_ms();
    let p = n.news_fetch_plan(Network::Cellular { metered: true }, true, now);
    assert!(p.feeds.is_empty() && p.blocked.as_deref() == Some("wifi_only"));
    let p = n.news_fetch_plan(Network::Offline, true, now);
    assert_eq!(p.blocked.as_deref(), Some("offline"));
    let p = n.news_fetch_plan(Network::Cellular { metered: true }, false, now);
    assert!(!p.feeds.is_empty() && p.metered_warning && p.blocked.is_none());
    assert_eq!(p.daily_cap_bytes, 500_000_000);
    n.news_ingest_rss(world_en_url(), news_feed_now(), now)
        .unwrap();
    assert!(
        n.news_fetch_plan(Network::Wifi { metered: false }, true, now)
            .used_today_bytes
            > 0
    );
}

/// Full round trip (CB1–CB4, RS1/RS2): identity + two chats on disk, back up all
/// text with the key derived from the 24 words, wipe the install, restore from
/// the 24 words alone, and compare every message field by field.
#[test]
fn roundtrip_backup_wipe_restore_from_24_words_is_identical() {
    let phrase = generate_recovery_phrase().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a = open_node(&phrase, "Alice", dir.path());
    let b = node("Bob");
    let c = node("Carol");
    let mut before = Vec::new();
    let mut blobs = Vec::new();
    let mut ident_blob = None;
    let key = generate_backup_key().unwrap();
    for (peer, n) in [(&b, 12usize), (&c, 7usize)] {
        peer.add_contact_from_qr(a.qr_payload()).unwrap();
        let cp = a.add_contact_from_qr(peer.qr_payload()).unwrap();
        peer.verify_contact_from_qr(a.qr_payload()).unwrap();
        a.set_verified(cp.contact_id.clone(), true).unwrap();
        let la = a.open_initiator_link(cp.contact_id.clone()).unwrap();
        let lp = peer.open_responder_link().unwrap();
        pump(&la, &lp, la.start().unwrap());
        for i in 0..n {
            pump(
                &la,
                &lp,
                la.send_text(format!("{i} → {} · émoji 🎉", cp.display_name))
                    .unwrap(),
            );
            pump(&lp, &la, lp.send_text(format!("re {i}")).unwrap());
        }
        la.shutdown();
        lp.shutdown();
        before.push((
            cp.contact_id.clone(),
            a.stored_messages(cp.contact_id).unwrap(),
        ));
    }
    backup_all(&a, 1, &mut blobs);
    ident_blob.replace(
        a.create_backup_with_key(key.clone(), "t".into(), 1)
            .unwrap(),
    );
    // Key really comes from the words: same phrase → same key, other phrase → none.
    assert!(matches!(
        node("Mallory").chat_backup_restore(blobs.clone()),
        Err(FfiError::WrongBackupPassword)
    ));
    // Wipe: drop the node and every file of the install.
    drop(a);
    drop(dir);
    let dir2 = tempfile::tempdir().unwrap();
    let r = open_backup_with_key(ident_blob.unwrap(), key).unwrap();
    assert_eq!(r.phrase, phrase);
    let a2 = open_node(&phrase, "Alice", dir2.path());
    assert!(a2.device_info().unwrap().restored_at_ms.is_none());
    for ct in r.contacts {
        a2.restore_contact_with_key(ct.contact_id, ct.display_name, ct.verified, ct.dh_hex)
            .unwrap();
    }
    a2.mark_restored(now_ms()).unwrap();
    let sum = a2.chat_backup_restore(blobs).unwrap();
    let _ = sum;
    for (cid, msgs) in &before {
        let after = a2.stored_messages(cid.clone()).unwrap();
        let key = |m: &StoredMessage| (m.id_hex.clone(), m.text.clone(), m.mine, m.time_ms);
        assert_eq!(
            after.iter().map(key).collect::<Vec<_>>(),
            msgs.iter().map(key).collect::<Vec<_>>(),
            "chat {cid} identical after restore"
        );
        assert!(!after.is_empty());
    }
    // Restored indicator (RS1) is set on the new install.
    assert!(a2.device_info().unwrap().restored_at_ms.is_some());
    // RS2: contacts' verified badge for the restored account resets on next link.
    let alice = a2.identity().peer_id_hex;
    for peer in [&b, &c] {
        assert!(peer
            .contacts()
            .iter()
            .any(|x| x.contact_id == alice && x.verified));
        link(&a2, peer);
        let x = peer
            .contacts()
            .into_iter()
            .find(|x| x.contact_id == alice)
            .unwrap();
        assert!(!x.verified, "verified badge reset");
        assert!(x.restored_at_ms.is_some(), "restored notice");
    }
}

/// News prefs and the gateway fetch byte counter survive an app restart, so the
/// 500 MB/day cap holds (design §5.115; gateway rules).
#[test]
fn news_prefs_and_fetch_byte_counter_survive_restart() {
    let phrase = generate_recovery_phrase().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let now = now_ms();
    let used;
    {
        let n = open_node(&phrase, "N", dir.path());
        let mut p = n.news_prefs();
        p.topics = vec!["tech".into(), "emergencies".into()];
        p.langs = vec!["en".into(), "fr".into()];
        p.city = Some("Riverside".into());
        p.muted = vec!["sport".into()];
        p.alerts_notify = false;
        n.news_set_prefs(p).unwrap();
        let url = n.news_due_feeds(now).into_iter().next().unwrap();
        let _ = n.news_ingest_rss(url, news_feed_now(), now);
        used = n
            .news_fetch_plan(Network::Wifi { metered: false }, true, now)
            .used_today_bytes;
        assert!(used > 0);
    }
    let n = open_node(&phrase, "N", dir.path());
    let p = n.news_prefs();
    assert_eq!(p.city.as_deref(), Some("Riverside"));
    assert_eq!(p.muted, vec!["sport".to_string()]);
    assert!(!p.alerts_notify);
    assert!(p.langs.contains(&"fr".to_string()));
    assert_eq!(
        n.news_fetch_plan(Network::Wifi { metered: false }, true, now)
            .used_today_bytes,
        used,
        "byte counter persisted"
    );
    // Next day the counter resets.
    assert_eq!(
        n.news_fetch_plan(Network::Wifi { metered: false }, true, now + 86_400_000)
            .used_today_bytes,
        0
    );
}

#[test]
fn news_fetch_role_is_one_flag() {
    use crate::news::news_may_fetch;
    assert!(news_may_fetch(true));
    // Product decision: any phone with internet fetches and relays news into the mesh.
    assert!(news_may_fetch(false));
}

// ---- Team moderation (PROTOCOL §23.9, TM1–TM4). Keys are generated at test time. ----

fn team_key() -> ed25519_dalek::SigningKey {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    ed25519_dalek::SigningKey::from_bytes(&b)
}

fn with_team(n: &Arc<OmcpNode>, k: &ed25519_dalek::SigningKey) {
    n.inner
        .lock()
        .set_team_policy(omcp_core::moderation::TeamPolicy::from_public_keys(
            &[k.verifying_key().to_bytes()],
            1,
        ));
}

fn team_hex(a: omcp_core::moderation::TeamAction, k: &ed25519_dalek::SigningKey) -> String {
    let mut a = a;
    a.sign(k);
    hex::encode(a.encode().unwrap())
}

#[test]
fn tm_takedown_gossips_to_neighbour_and_restore_undoes_it() {
    use omcp_core::moderation::{Reason, TeamAction};
    let team = team_key();
    let (a, b) = (node("Ana"), node("Ben"));
    befriend(&a, &b);
    with_team(&a, &team);
    with_team(&b, &team);
    let cid = [0x42u8; 32];
    let cid_hex = hex::encode(cid);
    let chats_before = b.chats().unwrap();
    // Published while offline: carried and handed over at the next encounter.
    let td = team_hex(TeamAction::takedown(cid, Reason::Hate, "", 1), &team);
    assert_eq!(
        a.moderation_publish(td.clone()).unwrap(),
        TeamActionResult::Applied
    );
    let l = up(&a, &b);
    assert_eq!(b.moderation_removed_reason(cid_hex.clone()), Some(1));
    let log = b.moderation_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].kind, "takedown");
    assert_eq!(log[0].signed_by.len(), 1);
    // TM4 / MR10: private chats untouched.
    assert_eq!(b.chats().unwrap(), chats_before);
    down(&l);
    // Replaying the same takedown is a no-op.
    assert_eq!(b.moderation_publish(td).unwrap(), TeamActionResult::Replay);
    let rs = team_hex(TeamAction::restore(cid, 2), &team);
    assert_eq!(a.moderation_publish(rs).unwrap(), TeamActionResult::Applied);
    let l = up(&a, &b);
    assert_eq!(b.moderation_removed_reason(cid_hex), None);
    assert_eq!(b.moderation_log().len(), 2);
    down(&l);
}

#[test]
fn tm_non_whitelisted_or_forged_action_is_ignored_and_not_gossiped() {
    use omcp_core::moderation::{Reason, TeamAction};
    let team = team_key();
    let stranger = team_key();
    let (a, b) = (node("Ana"), node("Ben"));
    befriend(&a, &b);
    with_team(&a, &team);
    with_team(&b, &team);
    let cid = [0x43u8; 32];
    let bad = team_hex(TeamAction::takedown(cid, Reason::Spam, "", 1), &stranger);
    assert_eq!(
        a.moderation_publish(bad).unwrap(),
        TeamActionResult::NotWhitelisted
    );
    let mut forged = TeamAction::takedown(cid, Reason::Spam, "", 1);
    forged.sign(&team);
    forged.epoch = 9;
    let forged = hex::encode(forged.encode().unwrap());
    assert_eq!(
        a.moderation_publish(forged).unwrap(),
        TeamActionResult::BadSignature
    );
    let l = up(&a, &b);
    assert_eq!(b.moderation_removed_reason(hex::encode(cid)), None);
    assert!(a.moderation_log().is_empty() && b.moderation_log().is_empty());
    down(&l);
}

#[test]
fn tm_moderation_state_survives_restart_and_shipped_policy_parses() {
    use omcp_core::moderation::{Reason, TeamAction};
    let _ = crate::moderation::shipped_policy();
    assert!(omcp_core::moderation::TeamPolicy::from_config(include_str!(
        "../../../config/team_moderation.conf"
    ))
    .is_ok());
    let team = team_key();
    let phrase = generate_recovery_phrase().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cid = [0x44u8; 32];
    {
        let n = open_node(&phrase, "N", dir.path());
        with_team(&n, &team);
        let td = team_hex(TeamAction::takedown(cid, Reason::Illegal, "", 1), &team);
        assert_eq!(n.moderation_publish(td).unwrap(), TeamActionResult::Applied);
    }
    let n = open_node(&phrase, "N", dir.path());
    assert_eq!(n.moderation_removed_reason(hex::encode(cid)), Some(2));
    assert_eq!(n.moderation_log().len(), 1);
}

#[test]
fn news_ingests_real_google_news_fr_response() {
    let xml = include_str!("../../omcp-core/src/news/fixtures/google_news_world_fr_live.xml");
    let n = node("N");
    let mut p = n.news_prefs();
    p.langs = vec!["fr".into()];
    n.news_set_prefs(p).unwrap();
    let url = "https://news.google.com/rss/headlines/section/topic/WORLD?hl=fr&gl=FR&ceid=FR:fr";
    let newest = omcp_core::news::rss::parse_rss(xml)
        .unwrap()
        .iter()
        .filter_map(|i| i.published_ms)
        .max()
        .unwrap();
    let added = n
        .news_ingest_rss(url.into(), xml.into(), newest + 60_000)
        .unwrap();
    assert!(added > 0, "real FR feed yields stories");
}
