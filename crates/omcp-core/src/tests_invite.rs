//! MR33–MR38 core tests for §20.7 private invites.
use super::*;
use crate::mls::OmcpMlsMember;
use crate::sync::compress::BUCKETS;

const NOW: u64 = 1_800_000_000;
const COMM: [u8; 32] = [7; 32];

fn keyring_on() -> (InviteKeyring, InviteKey) {
    let mut kr = InviteKeyring::default();
    kr.set_enabled(COMM, true, NOW);
    let k = kr.candidate_keys()[0].1.clone();
    (kr, k)
}

fn sk(b: u8) -> SigningKey {
    SigningKey::from_bytes(&[b; 32])
}

fn offer_to() -> (InviteOffer, InviteKey) {
    let reply = InviteKey::generate(NOW, OFFER_TTL_S + OVERLAP_S);
    let o = InviteOffer::new_signed(&sk(3), reply.public(), "join us", "Secret Club", None, NOW)
        .unwrap();
    (o, reply)
}

fn env_for(pk: &InvitePk, info: &[u8], msg: &[u8], bucket: usize) -> EnvelopeV1 {
    build_envelope(pk, info, msg, bucket, rand_bytes(), NOW * 1000, 7, 1).unwrap()
}

/// Real 1:1 envelope built exactly like the FFI `seal_for` path.
fn real_1to1(a: &mut OmcpMlsMember, bucket: usize) -> EnvelopeV1 {
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &rand_bytes());
    put_subtags_in_aad(&mut header_aad, &[rand_bytes()]).unwrap();
    put_expires_in_aad(&mut header_aad, NOW * 1000 + DEFAULT_EXPIRY_MS);
    let mut env = EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: rand_bytes(),
        flags: flags::RELAY_OK | flags::STORE_OK,
        ttl_hops: 7,
        created_ms: NOW * 1000,
        header_aad,
        ciphertext: Vec::new(),
    };
    let aad = mesh_aad(&env);
    env.ciphertext = a
        .encrypt_app_message_with_aad(&vec![0x42; bucket - 16], &aad)
        .unwrap();
    env
}

fn mls_pair() -> OmcpMlsMember {
    let mut a = OmcpMlsMember::generate(b"a".to_vec()).unwrap();
    let mut b = OmcpMlsMember::generate(b"b".to_vec()).unwrap();
    a.create_group().unwrap();
    let kp = a
        .parse_key_package(&b.key_package_bytes().unwrap())
        .unwrap();
    let (_c, w) = a.add_members(&[kp]).unwrap();
    b.join_from_welcome(&w).unwrap();
    a
}

/// Everything a relay, observer or the team sees, minus random-looking bytes.
fn features(e: &EnvelopeV1) -> Vec<u64> {
    let mut f = vec![
        e.version as u64,
        e.suite_id as u64,
        e.flags as u64,
        e.ttl_hops as u64,
        e.header_aad.len() as u64,
        e.ciphertext.len() as u64,
        e.encode().unwrap().len() as u64,
    ];
    let mut i = 0;
    while i + 1 < e.header_aad.len() {
        f.push(e.header_aad[i] as u64);
        f.push(e.header_aad[i + 1] as u64);
        if e.header_aad[i] == 0x58 {
            let secs = u32::from_be_bytes(e.header_aad[i + 2..i + 6].try_into().unwrap());
            f.push(secs as u64 * 1000 - e.created_ms);
        }
        i += 2 + e.header_aad[i + 1] as usize;
    }
    // MLS framing: version, wire format, group_id length, epoch, content type,
    // authenticated_data, sender-data length, ciphertext length prefix.
    let c = &e.ciphertext;
    f.extend(c[..5].iter().map(|b| *b as u64));
    f.push(u64::from_be_bytes(c[37..45].try_into().unwrap()));
    f.push(c[45] as u64);
    let (n, w) = get_varint(&c[46..]).unwrap();
    f.push((c[46 + w..46 + w + n] == mesh_aad(e)[..]) as u64);
    let j = 46 + w + n;
    f.push(c[j] as u64);
    let k = j + 1 + c[j] as usize;
    f.push(get_varint(&c[k..]).unwrap().0 as u64);
    f
}

#[test]
fn elligator_roundtrip_and_dh_match() {
    for _ in 0..32 {
        let (e, rep) = elligator::ephemeral();
        let u = elligator::map(&rep);
        let bob = StaticSecret::from(rand_bytes::<32>());
        let bob_pub = PublicKey::from(&bob);
        let s1 = StaticSecret::from(e).diffie_hellman(&bob_pub).to_bytes();
        let s2 = bob.diffie_hellman(&PublicKey::from(u)).to_bytes();
        assert_eq!(s1, s2);
    }
}

#[test]
fn representatives_look_uniform() {
    // Every bit position of 4,096 representatives is set 50% of the time
    // within 6 sigma (σ = 32; false-alarm rate < 1e-6 over 256 bits),
    // including the two top bits (a plain u coordinate never sets bit 255).
    let reps: Vec<[u8; 32]> = (0..4096).map(|_| elligator::ephemeral().1).collect();
    for bit in 0..256 {
        let n = reps
            .iter()
            .filter(|r| r[bit / 8] >> (bit % 8) & 1 == 1)
            .count();
        assert!((1856..=2240).contains(&n), "bit {bit}: {n}");
    }
}

#[test]
fn invite_pk_is_1228_bytes_and_roundtrips() {
    let (_, k) = keyring_on();
    let pk = k.public();
    assert_eq!(pk.encode().len(), 1228);
    assert_eq!(InvitePk::decode(&pk.encode()).unwrap(), pk);
    assert_eq!(InviteKey::from_bytes(&k.to_bytes()).unwrap().public(), pk);
}

#[test]
fn mls_shape_matches_real_1to1() {
    let mut a = mls_pair();
    // Invites need ≥ 1,120 B of HPKE encapsulation, so they use 4 KiB and up.
    for b in [BUCKETS[2], BUCKETS[3], 2 * BUCKETS[3]] {
        let real = real_1to1(&mut a, b);
        let (_, k) = keyring_on();
        let inv = env_for(&k.public(), &invite_info(&k.key_id), b"x", b);
        assert_eq!(features(&real), features(&inv), "bucket {b}");
    }
}

/// MR33: a classifier using size, TLVs, tags, flags, TTL, expiry and the
/// ciphertext framing sees identical feature vectors for invite and 1:1
/// envelopes in every bucket, so it can do no better than chance.
#[test]
fn mr33_invite_envelopes_indistinguishable_from_1to1() {
    let mut a = mls_pair();
    let (_, k) = keyring_on();
    let (offer, reply) = offer_to();
    let mut seen_1to1 = std::collections::HashMap::<Vec<u64>, usize>::new();
    let mut seen_inv = std::collections::HashMap::<Vec<u64>, usize>::new();
    for b in [1024usize, 4096, 16 * 1024] {
        for _ in 0..3 {
            *seen_1to1
                .entry(features(&real_1to1(&mut a, b)))
                .or_default() += 1;
        }
    }
    let msgs = [
        env_for(&k.public(), &invite_info(&k.key_id), &offer.encode(), 0),
        env_for(
            &reply.public(),
            INFO_REPLY,
            &InviteReply {
                offer_id: offer.offer_id,
                decision: DECISION_DECLINE,
                key_package: vec![],
                welcome_pk: None,
            }
            .encode(),
            InviteReply::bucket(1500),
        ),
        env_for(&k.public(), &invite_info(&k.key_id), &[3u8; 9000], 0),
    ];
    for e in &msgs {
        *seen_inv.entry(features(e)).or_default() += 1;
    }
    // Every invite feature vector also occurs for ordinary 1:1 traffic.
    for f in seen_inv.keys() {
        assert!(seen_1to1.contains_key(f), "invite-only features {f:?}");
    }
    // Tags differ per message and never repeat between two invites.
    let t: BTreeSet<_> = msgs.iter().map(|e| e.header_aad[2..10].to_vec()).collect();
    assert_eq!(t.len(), msgs.len());
}

/// MR34: no group ID, group_ref, member list, inviter identity or pseudonym,
/// community ID/ref or TLV 0x5B in any invite envelope.
#[test]
fn mr34_outer_header_carries_nothing_identifying() {
    let (_, k) = keyring_on();
    let (offer, _) = offer_to();
    let e = env_for(&k.public(), &invite_info(&k.key_id), &offer.encode(), 0);
    let bytes = e.encode().unwrap();
    let mut i = 0;
    while i < e.header_aad.len() {
        assert!(matches!(e.header_aad[i], 0x54 | 0x56 | 0x58));
        i += 2 + e.header_aad[i + 1] as usize;
    }
    for needle in [
        &offer.inviter_pseudonym_pk[..],
        &COMM[..],
        b"Secret Club",
        &offer.offer_id[..],
        &k.key_id[..],
    ] {
        assert!(!bytes.windows(needle.len()).any(|w| w == needle));
    }
    assert_eq!(e.flags & flags::PRIORITY, 0);
}

#[test]
fn mr34_decoders_survive_fuzz() {
    let (offer, _) = offer_to();
    let enc = offer.encode();
    for i in 0..enc.len() {
        let mut b = enc.clone();
        b[i] ^= 0x5a;
        let _ = InviteOffer::decode(&b);
        let _ = InviteOffer::decode(&enc[..i]);
        let _ = InviteReply::decode(&b);
        let _ = InviteWelcome::decode(&b);
    }
    let mut env = env_for(&offer.reply_pk, INFO_REPLY, b"hi", 0);
    for n in [0, 5, 46, 60, 200] {
        env.ciphertext.truncate(n);
        assert!(try_open_envelope(&env, &[], |_| vec![]).is_none());
    }
}

/// MR35: only the invite private key holder recognises and opens.
#[test]
fn mr35_only_the_holder_recognises_and_opens() {
    let (kr, k) = keyring_on();
    let (offer, _) = offer_to();
    let e = env_for(&k.public(), &invite_info(&k.key_id), &offer.encode(), 0);
    let others: Vec<InviteKey> = (0..15)
        .map(|_| InviteKey::generate(NOW, ROTATE_S))
        .collect();
    let refs: Vec<&InviteKey> = others.iter().collect();
    assert!(try_open_envelope(&e, &refs, |k| invite_info(&k.key_id)).is_none());
    let mut with = refs.clone();
    let mine = kr.candidate_keys()[0].1;
    with.push(mine);
    let (i, pt) = try_open_envelope(&e, &with, |k| invite_info(&k.key_id)).unwrap();
    assert_eq!(i, 15);
    assert_eq!(InviteOffer::decode(&pt).unwrap(), offer);
    // A 17th key is never tried.
    let mut many: Vec<&InviteKey> = refs.clone();
    many.push(refs[0]);
    many.push(mine);
    assert!(try_open_envelope(&e, &many, |k| invite_info(&k.key_id)).is_none());
    // Wrong info (a reply key's context) fails even with the right key.
    assert!(try_open_envelope(&e, &[mine], |_| INFO_REPLY.to_vec()).is_none());
}

/// MR36: accept and decline are the same padded size, sealed to reply_pk.
#[test]
fn mr36_decline_same_size_as_accept() {
    let (offer, reply_key) = offer_to();
    let (_, k) = keyring_on();
    let kp = vec![9u8; 1400];
    let accept = InviteReply {
        offer_id: offer.offer_id,
        decision: DECISION_ACCEPT,
        key_package: kp.clone(),
        welcome_pk: Some(k.public()),
    };
    let decline = InviteReply {
        offer_id: offer.offer_id,
        decision: DECISION_DECLINE,
        key_package: vec![],
        welcome_pk: None,
    };
    let bucket = InviteReply::bucket(accept.encode().len());
    let ea = env_for(&offer.reply_pk, INFO_REPLY, &accept.encode(), bucket);
    let ed = env_for(&offer.reply_pk, INFO_REPLY, &decline.encode(), bucket);
    assert_eq!(ea.encode().unwrap().len(), ed.encode().unwrap().len());
    assert_eq!(features(&ea), features(&ed));
    let (_, pt) = try_open_envelope(&ed, &[&reply_key], |_| INFO_REPLY.to_vec()).unwrap();
    assert_eq!(InviteReply::decode(&pt).unwrap(), decline);
    assert!(try_open_envelope(&ed, &[&k], |_| INFO_REPLY.to_vec()).is_none());
}

/// MR37: setting default off, no invite_pk while off, rotation and overlap.
#[test]
fn mr37_setting_rotation_and_overlap() {
    let mut kr = InviteKeyring::default();
    assert!(!kr.is_enabled(&COMM));
    assert!(kr.published(&COMM).is_none());
    assert!(kr.candidate_keys().is_empty());
    kr.set_enabled(COMM, true, NOW);
    let first = kr.published(&COMM).unwrap();
    kr.rotate(NOW + ROTATE_S);
    let second = kr.published(&COMM).unwrap();
    assert_ne!(first.key_id, second.key_id);
    assert_eq!(kr.key_count(), 2, "old key kept for the overlap");
    kr.rotate(NOW + ROTATE_S + OVERLAP_S);
    assert_eq!(kr.key_count(), 1, "old key erased after 7 days");
    kr.set_enabled(COMM, false, NOW + ROTATE_S + OVERLAP_S);
    assert!(kr.published(&COMM).is_none());
    assert_eq!(kr.key_count(), 1, "private key kept for the overlap");
    let kr2 = InviteKeyring::from_bytes(&kr.to_bytes()).unwrap();
    assert_eq!(kr2.key_count(), 1);
    kr.rotate(NOW + ROTATE_S + 2 * OVERLAP_S);
    assert_eq!(kr.key_count(), 0);
    // A second community gets an unrelated key.
    kr.set_enabled([8; 32], true, NOW);
    kr.set_enabled(COMM, true, NOW);
    assert_ne!(
        kr.published(&COMM).unwrap().x25519_pk,
        kr.published(&[8; 32]).unwrap().x25519_pk
    );
}

#[test]
fn mr37_sender_limits() {
    let mut s = SenderLimits::default();
    for i in 0..10u8 {
        s.check(&[i; 8], NOW).unwrap();
        s.record_sent([i; 8], NOW + OFFER_TTL_S, NOW);
    }
    assert!(s.check(&[99; 8], NOW + 60).is_err(), "10 per hour");
    assert!(
        s.check(&[0; 8], NOW + 3600).is_err(),
        "one open offer per pk"
    );
    s.check(&[99; 8], NOW + 3600).unwrap();
    for h in 1..3u64 {
        for i in 0..10u8 {
            s.record_sent(
                [100 + i + 10 * h as u8; 8],
                NOW + OFFER_TTL_S,
                NOW + h * 3600,
            );
        }
    }
    assert!(s.check(&[250; 8], NOW + 3 * 3600).is_err(), "30 per day");
    s.record_closed([0; 8], true, NOW + 4 * 3600);
    assert!(
        s.check(&[0; 8], NOW + DAY_S + 5 * 3600).is_err(),
        "7-day block"
    );
    assert!(s.check(&[0; 8], NOW + 8 * DAY_S).is_ok());
    s.tick(NOW + OFFER_TTL_S);
    assert!(
        s.check(&[1; 8], NOW + OFFER_TTL_S + 3600).is_err(),
        "expired = blocked"
    );
}

#[test]
fn mr37_receiver_limits() {
    let mut r = ReceiverLimits::default();
    for _ in 0..3 {
        assert!(r.admit(&[1; 32], NOW));
    }
    assert!(
        !r.admit(&[1; 32], NOW + DAY_S),
        "3 per pseudonym per 7 days"
    );
    assert!(r.admit(&[1; 32], NOW + 7 * DAY_S));
    let mut r = ReceiverLimits::default();
    for i in 0..20u8 {
        assert!(r.admit(&[i / 3 + 10; 32], NOW));
    }
    assert!(!r.admit(&[200; 32], NOW + 10), "20 per day");
    r.block([201; 32]);
    assert!(!r.admit(&[201; 32], NOW + 2 * DAY_S), "blocked dropped");
}

/// MR38: a report is exactly the signed offer: verifiable, nothing else.
#[test]
fn mr38_report_is_only_the_signed_offer() {
    let (offer, _) = offer_to();
    let rep = offer_report(&offer);
    assert_eq!(rep, offer.encode());
    let back = InviteOffer::decode(&rep).unwrap();
    assert_eq!(back.inviter_pseudonym_pk, sk(3).verifying_key().to_bytes());
    let mut forged = rep.clone();
    let n = forged.len();
    forged[n - 70] ^= 1;
    assert!(InviteOffer::decode(&forged).is_err());
}

#[test]
fn welcome_roundtrip_through_invite_key() {
    let (_, k) = keyring_on();
    let w = InviteWelcome {
        offer_id: [5; 16],
        welcome: vec![1; 3000],
    };
    let e = env_for(&k.public(), &invite_info(&k.key_id), &w.encode(), 0);
    let (_, pt) = try_open_envelope(&e, &[&k], |k| invite_info(&k.key_id)).unwrap();
    assert_eq!(InviteWelcome::decode(&pt).unwrap(), w);
}
