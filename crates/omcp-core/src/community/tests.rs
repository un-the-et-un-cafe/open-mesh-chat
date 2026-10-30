//! Open communities: roles, limits (MR23), team item takedowns (MR22) and
//! erasure through backups (MR25).

use ed25519_dalek::SigningKey;

use super::*;
use crate::moderation::{Reason, TeamAction, TeamPolicy};

fn key() -> SigningKey {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    SigningKey::from_bytes(&b)
}
fn pk(k: &SigningKey) -> [u8; 32] {
    k.verifying_key().to_bytes()
}

const NOW: u64 = 1_800_000_000_000;
const T: [u8; 32] = [0x70; 32];

fn setup() -> (
    CommunityStore,
    ModerationState,
    SigningKey,
    SigningKey,
    [u8; 32],
) {
    let mut s = CommunityStore::default();
    let m = ModerationState::default();
    let founder = key();
    let bob = key();
    let c = s.create(&founder, "Climbers", NOW).unwrap();
    let id = c.community_id;
    s.apply_control(&Control::new(&bob, id, ControlKind::Join, NOW), &m)
        .unwrap();
    s.apply_control(
        &Control::new(
            &founder,
            id,
            ControlKind::NewTopic {
                topic_id: T,
                title: "Routes".into(),
            },
            NOW,
        ),
        &m,
    )
    .unwrap();
    (s, m, founder, bob, id)
}

#[test]
fn posts_are_public_flag_and_notice() {
    assert!(posts_are_public());
    assert!(POSTS_ARE_PUBLIC_NOTICE.contains("not end-to-end encrypted"));
}

#[test]
fn signed_posts_threads_and_mesh_envelope_round_trip() {
    let (mut s, m, _f, bob, id) = setup();
    let p = Post::new(&bob, id, T, None, "hello", NOW).unwrap();
    let pid = s.add_post(&p, &m, NOW).unwrap();
    let r = Post::new(&bob, id, T, Some(pid), "reply", NOW + 1).unwrap();
    s.add_post(&r, &m, NOW).unwrap();
    assert_eq!(s.get(&id).unwrap().topic_posts(&T).len(), 2);
    let mut forged = p.clone();
    forged.body = "changed".into();
    assert!(s.add_post(&forged, &m, NOW).is_err());
    let stranger = key();
    assert!(s
        .add_post(
            &Post::new(&stranger, id, T, None, "x", NOW).unwrap(),
            &m,
            NOW
        )
        .is_err());
    let env = seal(&Wire::Post(p.clone()), &id, NOW, 3);
    assert_eq!(open(&env, &id).unwrap(), Wire::Post(p));
    assert!(open(&env, &[1; 32]).is_err());
}

#[test]
fn only_founder_appoints_and_removal_needs_reason_and_hides_for_everyone() {
    let (mut s, m, founder, bob, id) = setup();
    let carol = key();
    s.apply_control(&Control::new(&carol, id, ControlKind::Join, NOW), &m)
        .unwrap();
    // Bob can't appoint; founder can.
    let appoint = |by: &SigningKey, who| {
        Control::new(by, id, ControlKind::AppointModerator { member: who }, NOW)
    };
    assert!(s.apply_control(&appoint(&bob, pk(&carol)), &m).is_err());
    s.apply_control(&appoint(&founder, pk(&bob)), &m).unwrap();
    assert!(s.apply_control(&appoint(&bob, pk(&carol)), &m).is_err()); // moderators can't appoint
    let p = Post::new(&carol, id, T, None, "spam link", NOW).unwrap();
    let pid = s.add_post(&p, &m, NOW).unwrap();
    let rm = |reason: &str| {
        Control::new(
            &bob,
            id,
            ControlKind::RemovePost {
                post_id: pid,
                reason: reason.into(),
            },
            NOW,
        )
    };
    assert!(s.apply_control(&rm("  "), &m).is_err());
    assert!(s
        .apply_control(
            &Control::new(
                &carol,
                id,
                ControlKind::RemovePost {
                    post_id: pid,
                    reason: "x".into()
                },
                NOW
            ),
            &m
        )
        .is_err());
    s.apply_control(&rm("spam"), &m).unwrap();
    let sp = &s.get(&id).unwrap().posts[&pid];
    assert!(sp.post.is_none()); // erased for everyone, moderators included
    assert_eq!(sp.removed_reason.as_deref(), Some("spam"));
    let mut buf = Vec::new();
    ciborium::into_writer(&s, &mut buf).unwrap();
    assert!(!buf.windows(9).any(|w| w == b"spam link"));
}

#[test]
fn live_is_members_only() {
    let (s, _m, _f, bob, id) = setup();
    let c = s.get(&id).unwrap();
    assert!(c.can_join_live(&pk(&bob)));
    assert!(!c.can_join_live(&pk(&key())));
}

#[test]
fn mr23_member_cap_post_size_and_history_eviction() {
    let (mut s, m, _f, bob, id) = setup();
    {
        let c = s.communities.get_mut(&id).unwrap();
        for i in 0..(MAX_MEMBERS - c.members.len()) {
            let mut k = [0u8; 32];
            k[..8].copy_from_slice(&(i as u64).to_be_bytes());
            k[31] = 0xEE;
            c.members.insert(k);
        }
        assert_eq!(c.members.len(), MAX_MEMBERS);
    }
    assert!(s
        .apply_control(&Control::new(&key(), id, ControlKind::Join, NOW), &m)
        .is_err());
    assert!(Post::new(&bob, id, T, None, &"a".repeat(MAX_POST_BYTES + 1), NOW).is_err());
    let mut big = Post::new(&bob, id, T, None, "ok", NOW).unwrap();
    big.body = "a".repeat(MAX_POST_BYTES + 1);
    assert!(s.add_post(&big, &m, NOW).is_err());
    s.add_post(
        &Post::new(&bob, id, T, None, &"a".repeat(MAX_POST_BYTES), NOW).unwrap(),
        &m,
        NOW,
    )
    .unwrap();
    // 30 days.
    let old = Post::new(&bob, id, T, None, "old", NOW).unwrap();
    s.add_post(&old, &m, NOW).unwrap();
    s.communities
        .get_mut(&id)
        .unwrap()
        .evict(NOW + HISTORY_MAX_AGE_MS + 1);
    assert!(s.get(&id).unwrap().posts.is_empty());
    // 50 MB, oldest first.
    let c = s.communities.get_mut(&id).unwrap();
    let body = "b".repeat(MAX_POST_BYTES);
    let n = HISTORY_MAX_BYTES / (MAX_POST_BYTES as u64 + 200) + 10;
    for i in 0..n {
        let mut id2 = [0u8; 32];
        id2[..8].copy_from_slice(&i.to_be_bytes());
        c.posts.insert(
            id2,
            StoredPost {
                post: Some(Post {
                    body: body.clone(),
                    created_ms: NOW + i,
                    ..old.clone()
                }),
                topic_id: T,
                parent: None,
                created_ms: NOW + i,
                removed_reason: None,
            },
        );
    }
    c.evict(NOW + n);
    assert!(c.history_bytes() <= HISTORY_MAX_BYTES);
    let mut first = [0u8; 32];
    first[..8].copy_from_slice(&0u64.to_be_bytes());
    assert!(!c.posts.contains_key(&first)); // oldest dropped
    let mut last = [0u8; 32];
    last[..8].copy_from_slice(&(n - 1).to_be_bytes());
    assert!(c.posts.contains_key(&last));
}

fn team() -> (SigningKey, TeamPolicy) {
    let k = key();
    let p = TeamPolicy::from_public_keys(&[pk(&k)], 1);
    (k, p)
}
fn act(mut a: TeamAction, k: &SigningKey) -> TeamAction {
    a.sign(k);
    a
}
fn bytes(s: &CommunityStore) -> Vec<u8> {
    let mut b = Vec::new();
    ciborium::into_writer(s, &mut b).unwrap();
    b
}
fn has(b: &[u8], needle: &[u8]) -> bool {
    b.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn mr22_item_takedowns_erase_only_the_item_and_restore_brings_nothing_back() {
    let (mut s, mut m, founder, bob, id) = setup();
    let (tk, tp) = team();
    let t2 = [0x71; 32];
    s.apply_control(
        &Control::new(
            &founder,
            id,
            ControlKind::NewTopic {
                topic_id: t2,
                title: "Gear".into(),
            },
            NOW,
        ),
        &m,
    )
    .unwrap();
    let bad = Post::new(&bob, id, T, None, "BADPOST", NOW).unwrap();
    let bad_id = s.add_post(&bad, &m, NOW).unwrap();
    let reply = Post::new(&bob, id, T, Some(bad_id), "BADREPLY", NOW).unwrap();
    s.add_post(&reply, &m, NOW).unwrap();
    let good = Post::new(&bob, id, T, None, "GOODPOST", NOW).unwrap();
    s.add_post(&good, &m, NOW).unwrap();
    let gear = Post::new(&bob, id, t2, None, "GEARPOST", NOW).unwrap();
    s.add_post(&gear, &m, NOW).unwrap();

    m.apply(
        &tp,
        &act(
            TeamAction::post_takedown(id, bad_id, Reason::Hate, "", 1),
            &tk,
        ),
        NOW,
    )
    .unwrap();
    s.enforce(&m);
    let b = bytes(&s);
    assert!(!has(&b, b"BADPOST") && !has(&b, b"BADREPLY"));
    assert!(has(&b, b"GOODPOST") && has(&b, b"GEARPOST"));
    assert!(s.add_post(&bad, &m, NOW).is_err()); // re-gossip is refused

    m.apply(
        &tp,
        &act(TeamAction::topic_takedown(id, t2, Reason::Spam, "", 1), &tk),
        NOW,
    )
    .unwrap();
    s.enforce(&m);
    assert!(!has(&bytes(&s), b"GEARPOST"));
    assert!(s
        .add_post(&Post::new(&bob, id, t2, None, "new", NOW).unwrap(), &m, NOW)
        .is_err());

    m.apply(&tp, &act(TeamAction::item_restore(id, bad_id, 2), &tk), NOW)
        .unwrap();
    s.enforce(&m);
    assert!(!has(&bytes(&s), b"BADPOST")); // nothing comes back by itself
}

#[test]
fn mr25_takedown_erases_and_old_backup_restore_does_not_resurrect() {
    let (mut s, mut m, _f, bob, id) = setup();
    let (tk, tp) = team();
    s.add_post(
        &Post::new(&bob, id, T, None, "FORENSIC-MARKER", NOW).unwrap(),
        &m,
        NOW,
    )
    .unwrap();
    let old_backup = s.backup(&m); // made before the takedown
    let mut old_bytes = Vec::new();
    ciborium::into_writer(&old_backup, &mut old_bytes).unwrap();
    assert!(has(&old_bytes, b"FORENSIC-MARKER"));

    let td = act(TeamAction::takedown(id, Reason::Illegal, "", 1), &tk);
    m.apply(&tp, &td, NOW).unwrap();
    s.enforce(&m);
    // Device storage scan.
    let b = bytes(&s);
    assert!(!has(&b, b"FORENSIC-MARKER") && !has(&b, &id));
    // A fresh backup holds nothing from it, but holds its tombstone.
    let fresh = s.backup(&m);
    let mut fb = Vec::new();
    ciborium::into_writer(&fresh.store, &mut fb).unwrap();
    assert!(!has(&fb, b"FORENSIC-MARKER"));
    assert!(fresh.tombstones.iter().any(|t| t.target_id == id));

    // Restore the OLD backup on a device that knows the takedown.
    let restored = CommunityStore::restore_backup(old_backup.clone(), &mut m.clone());
    assert!(!has(&bytes(&restored), b"FORENSIC-MARKER"));
    // Restore the OLD backup on a clean device together with a fresh backup's
    // tombstones (tombstones are applied before anything is written).
    let mut clean = ModerationState::default();
    clean.merge_tombstones(&fresh.tombstones);
    let r2 = CommunityStore::restore_backup(old_backup.clone(), &mut clean);
    assert!(r2.get(&id).is_none());
    // Clean device, old backup only: the team action synced afterwards erases it.
    let mut clean = ModerationState::default();
    let mut r3 = CommunityStore::restore_backup(old_backup, &mut clean);
    clean.apply(&tp, &td, NOW).unwrap();
    r3.enforce(&clean);
    assert!(!has(&bytes(&r3), b"FORENSIC-MARKER"));
    // Restore only lifts the ban.
    m.apply(&tp, &act(TeamAction::restore(id, 2), &tk), NOW)
        .unwrap();
    s.enforce(&m);
    assert!(s.get(&id).is_none());
}
