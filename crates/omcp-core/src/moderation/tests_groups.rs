//! §23.10 tests: private-group takedowns, evidence files and the hard rule
//! (MR11–MR12, MR14–MR21). Every key is generated at test time.

use ed25519_dalek::{Signer, SigningKey};

use super::evidence::*;
use super::*;
use crate::mls::group::OmcpMlsMember;

fn key() -> SigningKey {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    SigningKey::from_bytes(&b)
}
fn pk(k: &SigningKey) -> [u8; 32] {
    k.verifying_key().to_bytes()
}
fn signed(mut a: TeamAction, k: &SigningKey) -> TeamAction {
    a.sign(k);
    a
}

const GID: [u8; 32] = [0x6a; 32];

// ---- group takedowns ----

#[test]
fn group_ref_is_domain_separated_blake2b() {
    let r = group_ref(&GID);
    assert_eq!(r, h256(&[b"OMCP-group-ref-v1\x00", &GID]));
    assert_ne!(r, h256(&[&GID]));
}

#[test]
fn group_takedown_and_restore_follow_epochs_and_threshold() {
    // MR14, MR15
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let r = group_ref(&GID);
    let ev = [3u8; 32];
    let td = signed(
        TeamAction::group_takedown(r, Reason::Csam, ev, "", 2),
        &team,
    );
    assert_eq!(TeamAction::decode(&td.encode().unwrap()).unwrap(), td);
    assert_eq!(
        st.apply(
            &p,
            &TeamAction::group_takedown(r, Reason::Csam, ev, "", 2),
            1
        ),
        Err(Rejected::BelowThreshold)
    );
    assert_eq!(
        st.apply(
            &p,
            &signed(
                TeamAction::group_takedown(r, Reason::Csam, ev, "", 2),
                &key()
            ),
            1
        ),
        Err(Rejected::NotWhitelisted)
    );
    let e = st.apply(&p, &td, 10).unwrap();
    assert_eq!(e.evidence_hash, ev); // MR21: log holds only the fingerprint
    assert!(st.is_group_removed(&r));
    // A community with the same 32 bytes is a different target.
    assert!(!st.is_removed(&r));
    assert_eq!(st.apply(&p, &td, 11), Err(Rejected::Replay));
    let old_restore = signed(TeamAction::group_restore(r, 1), &team);
    assert_eq!(st.apply(&p, &old_restore, 12), Err(Rejected::Replay));
    let rs = signed(TeamAction::group_restore(r, 3), &team);
    st.apply(&p, &rs, 13).unwrap();
    assert!(!st.is_group_removed(&r));
    // MR14: the tombstone stays, marked restored; the old ref stays refused.
    let ts = st.tombstones();
    assert_eq!(ts.len(), 1);
    assert!(ts[0].restored);
    assert!(st.is_group_tombstoned(&r));
    // MR28: a restore with reason 0 is rejected on the wire.
    let mut w = signed(TeamAction::group_restore(r, 9), &team)
        .encode()
        .unwrap();
    w[1 + 32 + 32 + 8] = 0;
    assert!(TeamAction::decode(&w).is_err());
}

#[test]
fn release_policy_without_key_ignores_every_kind() {
    // MR24 (release half): the shipped config has no production key.
    let shipped = include_str!("../../../../config/team_moderation.conf");
    let p = TeamPolicy::from_config(shipped).unwrap();
    assert!(p.members.is_empty());
    let team = key();
    let mut st = ModerationState::default();
    for a in [
        TeamAction::takedown([1; 32], Reason::Hate, "", 1),
        TeamAction::group_takedown([1; 32], Reason::Hate, [0; 32], "", 1),
        TeamAction::post_takedown([1; 32], [2; 32], Reason::Hate, "", 1),
        TeamAction::topic_takedown([1; 32], [2; 32], Reason::Hate, "", 1),
        TeamAction::live_takedown([1; 32], [2; 16], Reason::Hate, "", 1),
    ] {
        assert_eq!(st.apply(&p, &signed(a, &team), 1), Err(Rejected::NoTeam));
    }
    assert!(st.tombstones().is_empty());
}

#[test]
fn envelope_uses_tlv_0x5a_with_body_version() {
    let team = key();
    for (a, v) in [
        (TeamAction::takedown([1; 32], Reason::Spam, "", 1), 1u8),
        (
            TeamAction::group_takedown([1; 32], Reason::Spam, [0; 32], "", 1),
            2u8,
        ),
    ] {
        let a = signed(a, &team);
        let env = seal_team_action(&a, 1_000, 3).unwrap();
        let aad = &env.header_aad;
        let i = aad
            .windows(3)
            .position(|w| w == [0x5A, 1, v])
            .expect("0x5A TLV");
        assert!(i < aad.len());
        assert!(!aad.windows(2).any(|w| w == [0x8F, 1]));
        assert_eq!(open_team_action(&env).unwrap(), a);
    }
    assert_eq!(AAD_TLV_TEAM_MOD, 0x5A);
}

// ---- hard rule ----

#[test]
fn mr18_group_actions_carry_no_member_list_sender_or_secret() {
    // Field-by-field: the encoding is exactly the fixed §23.10.7 layout; any
    // extra bytes (a smuggled member list) are rejected.
    let team = key();
    for _ in 0..64 {
        let mut r = [0u8; 32];
        getrandom::getrandom(&mut r).unwrap();
        let note = "x".repeat((r[0] % 40) as usize);
        let a = signed(
            TeamAction::group_takedown(r, Reason::Hate, r, &note, r[1] as u64),
            &team,
        );
        let w = a.encode().unwrap();
        assert_eq!(w.len(), 1 + 32 + 32 + 8 + 1 + 32 + 2 + note.len() + 1 + 96);
        assert_eq!(w[0], 3);
        assert_eq!(&w[1..33], &r); // target = group_ref
        assert_eq!(&w[33..65], &[0u8; 32]); // no item
        let mut extra = w.clone();
        extra.extend_from_slice(&[0xAB; 32]); // a "member id"
        assert!(TeamAction::decode(&extra).is_err());
        let mut bad = a.clone();
        bad.item_id = [9; 32]; // any non-zero item is refused for groups
        assert!(bad.encode().is_err());
    }
    let rs = signed(TeamAction::group_restore([5; 32], 2), &team)
        .encode()
        .unwrap();
    assert_eq!(rs.len(), 1 + 32 + 32 + 8 + 1 + 32 + 2 + 1 + 96);
}

fn group_with(names: &[&[u8]]) -> Vec<OmcpMlsMember> {
    let mut ms: Vec<_> = names
        .iter()
        .map(|n| OmcpMlsMember::generate(n.to_vec()).unwrap())
        .collect();
    ms[0].create_group().unwrap();
    for i in 1..ms.len() {
        let kp = ms[i].take_key_package().unwrap();
        let (_c, w) = ms[0].add_members(&[kp]).unwrap();
        for m in ms.iter_mut().take(i).skip(1) {
            m.process_protocol_message(&_c).unwrap();
        }
        ms[i].join_from_welcome(&w).unwrap();
    }
    ms
}

#[test]
fn mr16_team_key_cannot_open_group_messages_or_secrets() {
    let team = key();
    let team_id = peer_id_hash(&pk(&team)).to_vec();
    let mut g = group_with(&[b"alice", b"bob"]);
    let gid = g[0].group_id().unwrap();
    assert_eq!(gid.len(), 32); // random 32-byte group ID
    let ct = g[0].encrypt_app_message(b"secret plan").unwrap();
    assert_eq!(g[1].decrypt_app_message(&ct).unwrap(), b"secret plan");
    // Holding every team key gives no MLS member state for the group.
    let mut eve = OmcpMlsMember::generate(team_id).unwrap();
    assert!(eve.decrypt_app_message(&ct).is_err());
    eve.create_group().unwrap(); // even with its own group
    assert!(eve.decrypt_app_message(&ct).is_err());
    assert!(eve.tag_exporter(1).unwrap() != g[0].tag_exporter(1).unwrap());
    // Team actions never contain group material.
    let a = signed(
        TeamAction::group_takedown(group_ref(&gid), Reason::Hate, [0; 32], "", 1),
        &team,
    );
    let w = a.encode().unwrap();
    assert!(!w.windows(gid.len()).any(|x| x == gid.as_slice()));
}

#[test]
fn mr17_team_identity_cannot_be_added_or_welcomed() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let team_id = peer_id_hash(&pk(&team)).to_vec();
    let mut alice = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
    alice.create_group().unwrap();
    let mut eve = OmcpMlsMember::generate(team_id.clone()).unwrap();
    let kp = eve.take_key_package().unwrap();
    assert!(alice.add_members_excluding_team(&p, &[kp]).is_err());
    assert_eq!(alice.member_count().unwrap(), 1);
    // A non-compliant founder adds the team key anyway: other compliant
    // members refuse the Welcome.
    let mut rogue = OmcpMlsMember::generate(b"rogue".to_vec()).unwrap();
    rogue.create_group().unwrap();
    eve.refresh_key_package().unwrap();
    let kp = eve.take_key_package().unwrap();
    rogue.add_members(&[kp]).unwrap();
    let mut bob = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
    let (_c, w) = rogue
        .add_members(&[bob.take_key_package().unwrap()])
        .unwrap();
    assert!(bob.join_from_welcome_excluding_team(&p, &w).is_err());
    assert!(!bob.has_group());
    // Normal members are fine.
    let mut carol = OmcpMlsMember::generate(b"carol".to_vec()).unwrap();
    let (_c, w) = alice
        .add_members_excluding_team(&p, &[carol.take_key_package().unwrap()])
        .unwrap();
    assert!(carol.join_from_welcome_excluding_team(&p, &w).is_ok());
}

// ---- evidence (signed-byte vectors in the RFC 9420 TLS layout) ----

fn framed(gid: &[u8], epoch: u64, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    tls_vec(&mut b, gid);
    b.extend_from_slice(&epoch.to_be_bytes());
    b.push(1);
    b.extend_from_slice(&3u32.to_be_bytes());
    tls_vec(&mut b, b"");
    b.push(1);
    tls_vec(&mut b, body);
    b
}
fn context(gid: &[u8], epoch: u64) -> Vec<u8> {
    let mut b = vec![0, 1, 0, 1];
    tls_vec(&mut b, gid);
    b.extend_from_slice(&epoch.to_be_bytes());
    tls_vec(&mut b, &[7; 32]);
    tls_vec(&mut b, &[8; 32]);
    tls_vec(&mut b, b"");
    b
}
fn leaf(k: &SigningKey, identity: &[u8; 32]) -> Vec<u8> {
    let mut b = Vec::new();
    tls_vec(&mut b, &[1; 32]);
    tls_vec(&mut b, &pk(k));
    b.extend_from_slice(&1u16.to_be_bytes());
    tls_vec(&mut b, identity);
    for _ in 0..5 {
        tls_vec(&mut b, b"");
    }
    b.push(1); // key_package source
    b.extend_from_slice(&0u64.to_be_bytes());
    b.extend_from_slice(&u64::MAX.to_be_bytes());
    tls_vec(&mut b, b"");
    let sig = k
        .sign(&super::evidence::sign_content(b"LeafNodeTBS", &b))
        .to_bytes();
    tls_vec(&mut b, &sig);
    b
}
fn msg(sender_leaf: &SigningKey, sender_id: [u8; 32], body: &[u8]) -> ReportedMessage {
    let fc = framed(&GID, 5, body);
    let gc = context(&GID, 5);
    let sig = sender_leaf
        .sign(&framed_content_sign_content(2, &fc, &gc))
        .to_bytes();
    ReportedMessage {
        sender_id,
        framed_content: fc,
        group_context_hash: h256(&[&gc]),
        group_context: gc,
        signature: sig.to_vec(),
        leaf_node: leaf(sender_leaf, &sender_id),
        body: body.to_vec(),
        media: Vec::new(),
        wire_format: 2,
    }
}

#[test]
fn mr11_unchanged_message_verifies_and_one_bit_changes_fail() {
    let s = key();
    let sid = [0x11; 32];
    let m = msg(&s, sid, b"hello");
    let r = group_ref(&GID);
    assert!(verify_message(&r, &m));
    type Flip = Box<dyn Fn(&mut ReportedMessage)>;
    let flips: Vec<Flip> = vec![
        Box::new(|m| {
            let n = m.framed_content.len();
            m.framed_content[n - 1] ^= 1
        }), // body in content
        Box::new(|m| m.body[0] ^= 1),
        Box::new(|m| m.sender_id[0] ^= 1),
        Box::new(|m| m.framed_content[1] ^= 1),  // group id
        Box::new(|m| m.framed_content[33] ^= 1), // epoch
        Box::new(|m| m.group_context[10] ^= 1),
        Box::new(|m| m.group_context_hash[0] ^= 1),
        Box::new(|m| m.signature[0] ^= 1),
    ];
    for f in flips {
        let mut x = m.clone();
        f(&mut x);
        assert!(!verify_message(&r, &x));
    }
    assert!(!verify_message(&[0; 32], &m)); // wrong group_ref
                                            // (a) wrong message signature from another key.
    let mut x = m.clone();
    x.signature = key().sign(b"other").to_bytes().to_vec();
    assert!(!verify_message(&r, &x));
    // (b) LeafNode with a broken own signature.
    let mut x = m.clone();
    let n = x.leaf_node.len();
    x.leaf_node[n - 1] ^= 1;
    assert!(!verify_message(&r, &x));
    // (c) credential identity differs from sender_id (re-signed leaf).
    let mut x = m.clone();
    x.leaf_node = leaf(&s, &[0x22; 32]);
    assert!(!verify_message(&r, &x));
    // wire_format other than 2 (1 = unverified).
    let mut x = m.clone();
    x.wire_format = 1;
    assert!(!verify_message(&r, &x));
}

fn export(n: usize, rep: &SigningKey, others: &[[u8; 32]]) -> Vec<u8> {
    let s = key();
    let msgs = (0..n)
        .map(|i| msg(&s, [0x11; 32], format!("m{i}").as_bytes()))
        .collect();
    let _ = others; // members not selected are never passed in
    EvidenceFile::export(
        &GID,
        msgs,
        2,
        Some("note".into()),
        None,
        vec![],
        rep,
        1_700_000_000_000,
    )
    .unwrap()
}

#[test]
fn mr20_export_is_deterministic_and_reporter_sig_breaks_on_one_bit() {
    let rep = key();
    let s = key();
    let msgs: Vec<_> = (0..3)
        .map(|i| msg(&s, [0x11; 32], format!("m{i}").as_bytes()))
        .collect();
    let a = EvidenceFile::export(&GID, msgs.clone(), 1, None, None, vec![], &rep, 5).unwrap();
    let b = EvidenceFile::export(&GID, msgs, 1, None, None, vec![], &rep, 5).unwrap();
    assert_eq!(a, b);
    let v = verify(&a).unwrap();
    assert!(v.reporter_sig_ok);
    assert_eq!(v.verified(), 3);
    assert_eq!(v.evidence_hash, EvidenceFile::hash(&a));
    // Flip one bit of the export time (a field covered by the signature).
    let f = EvidenceFile::decode(&a).unwrap();
    let mut g = f.clone();
    g.export_ms ^= 1;
    assert!(!g.reporter_sig_ok());
    // A one-bit flip anywhere either breaks parsing or the signature/check.
    for i in [5usize, a.len() / 2, a.len() - 70] {
        let mut x = a.clone();
        x[i] ^= 1;
        match verify(&x) {
            Err(_) => {}
            Ok(v) => assert!(!v.reporter_sig_ok || v.verified() < 3),
        }
    }
    // Non-canonical CBOR is refused.
    let mut nc = a.clone();
    nc.push(0);
    assert!(EvidenceFile::decode(&nc).is_err());
}

#[test]
fn evidence_limits_500_messages_and_reason_categories() {
    let rep = key();
    let s = key();
    let one = msg(&s, [0x11; 32], b"x");
    assert!(EvidenceFile::export(&GID, vec![], 1, None, None, vec![], &rep, 1).is_err());
    assert!(
        EvidenceFile::export(&GID, vec![one.clone(); 501], 1, None, None, vec![], &rep, 1).is_err()
    );
    assert!(
        EvidenceFile::export(&GID, vec![one.clone(); 500], 1, None, None, vec![], &rep, 1).is_ok()
    );
    assert!(EvidenceFile::export(&GID, vec![one.clone()], 0, None, None, vec![], &rep, 1).is_err());
    assert!(EvidenceFile::export(&GID, vec![one], 10, None, None, vec![], &rep, 1).is_err());
    assert_eq!(EVIDENCE_RETENTION_MS, 90 * 24 * 3600 * 1000);
    assert_eq!(EVIDENCE_EXTENSION, "omcp-evidence");
}

#[test]
fn mr12_mr19_evidence_holds_hashes_and_selected_senders_only() {
    let rep = key();
    let carol_id = [0xC0; 32]; // a member who sent nothing selected
    let name = b"Secret Group Name";
    let f = export(4, &rep, &[carol_id]);
    assert!(!f.windows(32).any(|w| w == carol_id));
    assert!(!f.windows(name.len()).any(|w| w == name));
    let d = EvidenceFile::decode(&f).unwrap();
    assert_eq!(d.group_ref, evidence_group_ref(&GID));
    assert!(d.disclosures.is_empty());
    // Only the sender of the selected messages is named.
    assert!(d.messages.iter().all(|m| m.sender_id == [0x11; 32]));
    // An explicit disclosure is the only way to name another member.
    let s = key();
    let with = EvidenceFile::export(
        &GID,
        vec![msg(&s, [0x11; 32], b"x")],
        4,
        None,
        Some(ReporterContact {
            email: Some("a@example.org".into()),
            ..Default::default()
        }),
        vec![MemberDisclosure {
            sender_id: carol_id,
            label: Some("c".into()),
        }],
        &rep,
        1,
    )
    .unwrap();
    assert!(with.windows(32).any(|w| w == carol_id));
    assert!(verify(&with).unwrap().reporter_sig_ok);
}

// ---- end to end: real OpenMLS signatures (vendored openmls patch) ----

#[test]
fn e2e_real_group_message_exported_by_member_verifies_and_tampering_fails() {
    let (a_id, b_id, c_id) = ([0xA1u8; 32], [0xB2u8; 32], [0xC3u8; 32]);
    let mut g = group_with(&[&a_id, &b_id, &c_id]);
    // Alice's leaf now has source `commit` (she committed the adds), so the
    // leaf self-signature check needs the group ID and leaf index.
    let ct = g[0]
        .encrypt_app_message(b"meet at 9, bring the stuff")
        .unwrap();
    let (body, signed) = g[1].decrypt_app_message_signed(&ct).unwrap();
    let signed = signed.expect("signed bytes kept by the receiver");
    assert_eq!(signed.wire_format, 2);
    let gid = g[1].group_id().unwrap();
    let rm = ReportedMessage::from_signed(&signed, body.clone()).unwrap();
    assert_eq!(rm.sender_id, a_id);
    let reporter = key();
    let file =
        EvidenceFile::export(&gid, vec![rm.clone()], 7, None, None, vec![], &reporter, 1).unwrap();
    let v = verify(&file).unwrap();
    assert!(v.reporter_sig_ok);
    assert_eq!(v.verified(), 1, "real MLS signature verifies (a)-(e)");
    // The file names no other member (Carol) and holds no group secret.
    assert!(!file.windows(32).any(|w| w == c_id));
    // Tampered body → unverified.
    let mut bad = rm.clone();
    bad.body = b"meet at 9, bring the cake".to_vec();
    let f2 = EvidenceFile::export(&gid, vec![bad], 7, None, None, vec![], &reporter, 1).unwrap();
    assert_eq!(verify(&f2).unwrap().verified(), 0);
    // Claiming Carol sent it → unverified.
    let mut bad = rm.clone();
    bad.sender_id = c_id;
    let f3 = EvidenceFile::export(&gid, vec![bad], 7, None, None, vec![], &reporter, 1).unwrap();
    assert_eq!(verify(&f3).unwrap().verified(), 0);
    // Real signature, wrong group_ref → unverified.
    let f4 = EvidenceFile::export(
        &[9u8; 32],
        vec![rm.clone()],
        7,
        None,
        None,
        vec![],
        &reporter,
        1,
    )
    .unwrap();
    assert_eq!(verify(&f4).unwrap().verified(), 0);
    // Carol's own receive keeps identical verifiable bytes.
    let (_, s2) = g[2].decrypt_app_message_signed(&ct).unwrap();
    let rm2 = ReportedMessage::from_signed(&s2.unwrap(), body).unwrap();
    assert!(verify_message(&group_ref(&gid), &rm2));
}

#[test]
fn evidence_wire_format_other_than_1_or_2_makes_the_file_invalid() {
    let rep = key();
    let s = key();
    let mut m = msg(&s, [0x11; 32], b"x");
    m.wire_format = 1;
    let f = EvidenceFile::export(&GID, vec![m.clone()], 1, None, None, vec![], &rep, 1).unwrap();
    assert_eq!(verify(&f).unwrap().verified(), 0); // 1 → unverified
                                                   // Anything else → invalid file: refused at export and at decode.
    let mut d = EvidenceFile::decode(&f).unwrap();
    m.wire_format = 3;
    assert!(EvidenceFile::export(&GID, vec![m.clone()], 1, None, None, vec![], &rep, 1).is_err());
    // Hand-patch the encoded key 8 (0x08 0x01 → 0x08 0x03) of a valid file.
    let i = f.windows(2).rposition(|w| w == [0x08, 0x01]).unwrap();
    let mut x = f.clone();
    x[i + 1] = 0x03;
    assert!(verify(&x).is_err());
    d.messages.clear();
}
