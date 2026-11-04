//! Report key (§23.2): MR6, MR70, MR71; TM34.

use ed25519_dalek::{Signer, SigningKey};

use super::evidence::EvidenceFile;
use super::report_key::*;
use super::tests_groups::msg;
use crate::community::community_ref;

fn id_key(i: u64) -> SigningKey {
    let mut s = [0u8; 32];
    s[..8].copy_from_slice(&i.to_be_bytes());
    s[8] = 0x5a;
    SigningKey::from_bytes(&s)
}

#[test]
fn mr70_known_answer_vector_and_determinism() {
    let seed: [u8; 32] = core::array::from_fn(|i| i as u8);
    let k = derive_seed(&seed, &[0xaa; 32]);
    // HKDF-SHA256(ikm=00..1f, salt=aa×32, info="OMCP-report-key-v1"), checked
    // with an independent implementation.
    assert_eq!(
        hex::encode(k.to_bytes()),
        "78bb43285f05723837a7b99080fc503a303a31559d0d541999654ed7cdc08074"
    );
    let id = SigningKey::from_bytes(&seed);
    let scope = ReportScope::Community([0xaa; 32]);
    assert_eq!(report_key(&id, &scope).to_bytes(), k.to_bytes());
    // Same on a second device / after restore (same identity seed).
    let restored = SigningKey::from_bytes(&id.to_bytes());
    assert_eq!(report_key(&restored, &scope).to_bytes(), k.to_bytes());
    // Different per community, never the identity key.
    let other = report_key(&id, &ReportScope::Community([0xbb; 32]));
    assert_ne!(other.to_bytes(), k.to_bytes());
    assert_ne!(k.verifying_key(), id.verifying_key());
}

#[test]
fn mr70_no_linking_distinguisher_advantage() {
    // 10^4 users in 2 communities: bitwise agreement between a user's two
    // report keys, and between a report key and the identity key, is ~1/2.
    let (c1, c2) = (
        ReportScope::Community([1; 32]),
        ReportScope::Community([2; 32]),
    );
    let (mut same, mut ident, mut total) = (0u64, 0u64, 0u64);
    for i in 0..10_000u64 {
        let id = id_key(i);
        let a = report_key(&id, &c1).verifying_key().to_bytes();
        let b = report_key(&id, &c2).verifying_key().to_bytes();
        let p = id.verifying_key().to_bytes();
        for j in 0..32 {
            same += (!(a[j] ^ b[j])).count_ones() as u64;
            ident += (!(a[j] ^ p[j])).count_ones() as u64;
            total += 8;
        }
    }
    for x in [same, ident] {
        let adv = (x as f64 / total as f64 - 0.5).abs();
        assert!(adv < 0.005, "advantage {adv}");
    }
}

#[test]
fn mr6_community_report_signed_by_report_key_and_key_separation() {
    let id = id_key(7);
    let cref = community_ref(&[9; 32]);
    let r = CommunityReport::new(&id, cref, b"post".to_vec(), 10, "n").unwrap();
    assert_ne!(r.reporter_key, id.verifying_key().to_bytes());
    let pk = id.verifying_key().to_bytes();
    assert!(!r.encode().windows(32).any(|w| w == pk));
    let back = CommunityReport::decode(&r.encode()).unwrap();
    assert!(back.verify([&[0u8; 32]]));
    // Signed with an identity key that is in the roster: rejected.
    let mut forged = back.clone();
    forged.reporter_key = pk;
    forged.sig = id.sign(b"x").to_bytes();
    assert!(!forged.verify([&pk]));
    let mut t = back;
    t.reason_code = 3;
    assert!(!t.verify([&[0u8; 32]]));
    assert!(CommunityReport::new(&id, cref, vec![], 8, "").is_err());
}

#[test]
fn mr71_evidence_files_use_scoped_report_keys_and_one_ref() {
    let s = id_key(100);
    let m = || vec![msg(&s, [0x11; 32], b"hi")];
    let me = id_key(1);
    let f = |b: &[u8]| EvidenceFile::decode(b).unwrap();
    let g1 = f(&EvidenceFile::export(&[1; 32], m(), 1, None, None, vec![], &me, 5).unwrap());
    let g1b = f(&EvidenceFile::export(&[1; 32], m(), 2, None, None, vec![], &me, 6).unwrap());
    let g2 = f(&EvidenceFile::export(&[2; 32], m(), 1, None, None, vec![], &me, 5).unwrap());
    let cref = community_ref(&[1; 32]);
    let c = f(&EvidenceFile::export_scoped(
        ReportScope::Community(cref),
        m(),
        1,
        None,
        None,
        vec![],
        &me,
        5,
    )
    .unwrap());
    assert_eq!(g1.reporter_key, g1b.reporter_key);
    assert_ne!(g1.reporter_key, g2.reporter_key);
    assert_ne!(g1.reporter_key, c.reporter_key);
    assert_ne!(g1.reporter_key, me.verifying_key().to_bytes());
    assert_eq!(c.community_ref, Some(cref));
    assert!(c.group_ref.is_none());
    assert!(g1.reporter_sig_ok() && c.reporter_sig_ok());
    // Same key after restore (re-derived, not stored).
    let again = f(&EvidenceFile::export(
        &[1; 32],
        m(),
        1,
        None,
        None,
        vec![],
        &SigningKey::from_bytes(&me.to_bytes()),
        5,
    )
    .unwrap());
    assert_eq!(again.reporter_key, g1.reporter_key);
    // Both or neither ref: rejected.
    let mut both = g1.clone();
    both.community_ref = Some(cref);
    assert!(EvidenceFile::decode(&both.encode_unchecked()).is_err());
    let mut none = g1.clone();
    none.group_ref = None;
    assert!(EvidenceFile::decode(&none.encode_unchecked()).is_err());
    // reporter_key equal to a message sender's identity: rejected.
    let mut ident = g1;
    ident.reporter_key = [0x11; 32];
    assert!(EvidenceFile::decode(&ident.encode_unchecked()).is_err());
}
