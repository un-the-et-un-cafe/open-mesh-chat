//! §23.10.11 community member removal: MR40, MR42–MR49.

use ed25519_dalek::SigningKey;

use super::member_mod::*;
use super::*;
use crate::moderation::{peer_id_hash, Reason, TeamAction, TeamPolicy};

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
const DAY: u32 = 86_400;

struct W {
    s: CommunityStore,
    m: ModerationState,
    owner: SigningKey,
    admin: SigningKey,
    moder: SigningKey,
    id: [u8; 32],
}

fn ctl(
    s: &mut CommunityStore,
    m: &ModerationState,
    k: &SigningKey,
    id: [u8; 32],
    kind: ControlKind,
) {
    s.apply_control(&Control::new(k, id, kind, NOW), m).unwrap();
}

fn setup() -> W {
    let mut s = CommunityStore::default();
    let m = ModerationState::default();
    let (owner, admin, moder) = (key(), key(), key());
    let id = s.create(&owner, "Climbers", NOW).unwrap().community_id;
    for k in [&admin, &moder] {
        ctl(&mut s, &m, k, id, ControlKind::Join);
    }
    ctl(
        &mut s,
        &m,
        &owner,
        id,
        ControlKind::AppointAdmin { member: pk(&admin) },
    );
    ctl(
        &mut s,
        &m,
        &owner,
        id,
        ControlKind::AppointModerator { member: pk(&moder) },
    );
    ctl(
        &mut s,
        &m,
        &owner,
        id,
        ControlKind::NewTopic {
            topic_id: T,
            title: "Routes".into(),
        },
    );
    W {
        s,
        m,
        owner,
        admin,
        moder,
        id,
    }
}

fn member(w: &mut W) -> SigningKey {
    let k = key();
    ctl(&mut w.s, &w.m, &k, w.id, ControlKind::Join);
    k
}

#[allow(clippy::too_many_arguments)]
fn act(
    by: &SigningKey,
    a: ModAction,
    id: &[u8; 32],
    who: [u8; 32],
    rc: u8,
    d: u32,
    at: u64,
    ep: u64,
) -> CommunityModActionV1 {
    CommunityModActionV1::new_signed(by, a, id, who, rc, d, at, ep, "")
}
fn remove(
    by: &SigningKey,
    id: &[u8; 32],
    who: &SigningKey,
    d: u32,
    at: u64,
    ep: u64,
) -> CommunityModActionV1 {
    act(by, ModAction::MemberRemove, id, pk(who), 5, d, at, ep)
}

fn post(k: &SigningKey, id: [u8; 32], at: u64) -> Post {
    Post::new(k, id, T, None, "hi", at).unwrap()
}

#[test]
fn mr40_reason_10_accepted_everywhere_and_bad_codes_rejected() {
    let w = setup();
    let x = key();
    for rc in [1u8, 2, 3, 4, 5, 6, 7, 9, 10] {
        let mut a = act(
            &w.owner,
            ModAction::MemberRemove,
            &w.id,
            pk(&x),
            rc,
            3600,
            NOW,
            1,
        );
        if rc == 9 {
            a = CommunityModActionV1::new_signed(
                &w.owner,
                ModAction::MemberRemove,
                &w.id,
                pk(&x),
                9,
                3600,
                NOW,
                1,
                "why",
            );
        }
        assert!(a.check_fields().is_ok(), "{rc}");
    }
    for rc in [0u8, 8, 11, 255] {
        assert!(act(
            &w.owner,
            ModAction::MemberRemove,
            &w.id,
            pk(&x),
            rc,
            3600,
            NOW,
            1
        )
        .check_fields()
        .is_err());
    }
    assert!(act(
        &w.owner,
        ModAction::MemberRestore,
        &w.id,
        pk(&x),
        8,
        0,
        NOW,
        2
    )
    .check_fields()
    .is_ok());
    assert!(act(
        &w.owner,
        ModAction::MemberRestore,
        &w.id,
        pk(&x),
        10,
        0,
        NOW,
        2
    )
    .check_fields()
    .is_err());
    // Team kind 9 with reason 10 and the restore rule.
    let t = TeamAction::member_remove(w.id, peer_id_hash(&pk(&x)), Reason::SelfHarm, "", 0, 1);
    assert!(t.check_fields().is_ok());
    assert!(TeamAction::decode(&t.encode().unwrap()).is_ok());
    assert!(crate::moderation::Reason::from_code(11).is_none());
    assert_eq!(
        crate::moderation::SELF_HARM_HELP_URL,
        "https://findahelpline.com"
    );
}

#[test]
fn mr42_removal_stops_posting_rejoin_and_topics_but_not_reading() {
    let mut w = setup();
    let bob = member(&mut w);
    let old = post(&bob, w.id, NOW);
    let old_id = w.s.add_post(&old, &w.m, NOW).unwrap();
    let a = remove(&w.moder, &w.id, &bob, DAY, NOW + 10, 1);
    // Gossip round trip with the 0x5C TLV (MR49).
    let env = seal_mod_action(&a, &w.id, NOW + 10, 4);
    let got = open_mod_action(&env, &w.id).unwrap();
    w.s.apply_member_mod(&got, &w.m, NOW + 10).unwrap();
    assert!(!w.s.can_post(&w.m, &w.id, &pk(&bob), NOW + 20));
    assert!(w
        .s
        .add_post(&post(&bob, w.id, NOW + 20), &w.m, NOW + 20)
        .is_err());
    assert!(w
        .s
        .apply_control(
            &Control::new(&bob, w.id, ControlKind::Leave, NOW + 30),
            &w.m
        )
        .is_ok());
    let rejoin = Control::new(&bob, w.id, ControlKind::Join, NOW + 40);
    assert!(w.s.apply_control(&rejoin, &w.m).is_err());
    let topic = Control::new(
        &bob,
        w.id,
        ControlKind::NewTopic {
            topic_id: [9; 32],
            title: "x".into(),
        },
        NOW + 40,
    );
    assert!(w.s.apply_control(&topic, &w.m).is_err());
    // Existing posts stay; the community is still readable.
    assert!(w.s.get(&w.id).unwrap().posts.contains_key(&old_id));
}

#[test]
fn mr43_bad_actor_rank_target_community_sig_and_replay_rejected() {
    let mut w = setup();
    let bob = member(&mut w);
    let carol = member(&mut w);
    let stranger = key();
    use ModReject::*;
    let (owner, admin, moder, id) = (w.owner.clone(), w.admin.clone(), w.moder.clone(), w.id);
    let r = |w: &mut W, a: &CommunityModActionV1| w.s.apply_member_mod(a, &w.m, NOW).unwrap_err();
    assert_eq!(
        r(&mut w, &remove(&stranger, &id, &bob, DAY, NOW, 1)),
        NotAllowed
    );
    assert_eq!(
        r(&mut w, &remove(&bob, &id, &carol, DAY, NOW, 1)),
        NotAllowed
    );
    assert_eq!(
        r(&mut w, &remove(&moder, &id, &admin, DAY, NOW, 1)),
        NotAllowed
    );
    assert_eq!(
        r(&mut w, &remove(&admin, &id, &owner, DAY, NOW, 1)),
        NotAllowed
    );
    let peer_mod = key();
    ctl(&mut w.s, &w.m.clone(), &peer_mod, id, ControlKind::Join);
    ctl(
        &mut w.s,
        &w.m.clone(),
        &owner,
        id,
        ControlKind::AppointModerator {
            member: pk(&peer_mod),
        },
    );
    assert_eq!(
        r(&mut w, &remove(&moder, &id, &peer_mod, DAY, NOW, 1)),
        NotAllowed
    ); // equal rank
    assert_eq!(
        r(&mut w, &remove(&owner, &[3; 32], &bob, DAY, NOW, 1)),
        WrongCommunity
    );
    let mut bad = remove(&moder, &id, &bob, DAY, NOW, 1);
    bad.member_id = [1; 32];
    assert_eq!(r(&mut w, &bad), Malformed);
    let mut bad = remove(&moder, &id, &bob, DAY, NOW, 1);
    bad.sig[0] ^= 1;
    assert_eq!(r(&mut w, &bad), BadSignature);
    w.s.apply_member_mod(&remove(&moder, &id, &bob, DAY, NOW, 5), &w.m, NOW)
        .unwrap();
    assert_eq!(r(&mut w, &remove(&moder, &id, &bob, DAY, NOW, 5)), Replay);
    assert_eq!(r(&mut w, &remove(&moder, &id, &bob, DAY, NOW, 4)), Replay);
}

#[test]
fn mr44_nothing_outside_the_community_changes_and_notice_names_the_role() {
    let mut w = setup();
    let bob = member(&mut w);
    let other = w.s.create(&w.owner, "Other", NOW).unwrap().community_id;
    ctl(&mut w.s, &w.m.clone(), &bob, other, ControlKind::Join);
    let before = w.s.communities.get(&other).cloned();
    w.s.apply_member_mod(&remove(&w.moder, &w.id, &bob, 7 * DAY, NOW, 1), &w.m, NOW)
        .unwrap();
    assert_eq!(w.s.communities.get(&other).cloned(), before);
    assert!(w.s.can_post(&w.m, &other, &pk(&bob), NOW + 1));
    let r = w.s.member_removal(&w.m, &w.id, &pk(&bob), NOW + 1).unwrap();
    let n = r.notice("Climbers");
    assert_eq!(n, "You were removed from Climbers by a community moderator");
    assert_eq!(r.end_ms, Some(NOW + 7 * 86_400_000));
    assert_eq!(member_mod::reason_label(r.reason_code), "Spam");
}

#[test]
fn mr45_durations_bounds_clock_ahead_and_restores() {
    let mut w = setup();
    let bob = member(&mut w);
    for d in [0u32, 3_599, 31_536_001] {
        assert_eq!(
            w.s.apply_member_mod(&remove(&w.owner, &w.id, &bob, d, NOW, 1), &w.m, NOW)
                .unwrap_err(),
            ModReject::Malformed
        );
    }
    for d in DURATION_PRESETS_S {
        assert!(remove(&w.owner, &w.id, &bob, d, NOW, 1)
            .check_fields()
            .is_ok());
    }
    w.s.apply_member_mod(&remove(&w.owner, &w.id, &bob, 3_600, NOW, 1), &w.m, NOW)
        .unwrap();
    let end = NOW + 3_600_000;
    // Removed device's clock set 2 h ahead; the mesh checkpoint says it's still on.
    let own = effective_now(NOW + 7_200_000, Some(NOW + 60_000), true);
    assert!(w.s.member_removal(&w.m, &w.id, &pk(&bob), own).is_some());
    assert!(w
        .s
        .member_removal(&w.m, &w.id, &pk(&bob), end - 1)
        .is_some());
    assert!(w.s.member_removal(&w.m, &w.id, &pk(&bob), end).is_none());
    // Restore with a higher epoch lifts it.
    w.s.apply_member_mod(&remove(&w.owner, &w.id, &bob, DAY, NOW, 2), &w.m, NOW)
        .unwrap();
    let rs = act(
        &w.owner,
        ModAction::MemberRestore,
        &w.id,
        pk(&bob),
        8,
        0,
        NOW,
        3,
    );
    w.s.apply_member_mod(&rs, &w.m, NOW).unwrap();
    assert!(w
        .s
        .member_removal(&w.m, &w.id, &pk(&bob), NOW + 1)
        .is_none());
    // Permanent team kind 9 is only lifted by ITEM_RESTORE; a community restore cannot.
    let tk = key();
    let tp = TeamPolicy::from_public_keys(&[pk(&tk)], 1);
    let mid = peer_id_hash(&pk(&bob));
    let mut t = TeamAction::member_remove(w.id, mid, Reason::Harassment, "", 0, 1);
    t.sign(&tk);
    w.m.apply(&tp, &t, NOW).unwrap();
    let rs = act(
        &w.owner,
        ModAction::MemberRestore,
        &w.id,
        pk(&bob),
        8,
        0,
        NOW,
        50,
    );
    w.s.apply_member_mod(&rs, &w.m, NOW).unwrap();
    let far = NOW + 400 * 86_400_000;
    let r = w.s.member_removal(&w.m, &w.id, &pk(&bob), far).unwrap();
    assert_eq!(r.by, RemovedBy::Team);
    assert_eq!(r.end_ms, None);
    assert_eq!(
        r.notice("Climbers"),
        "You were removed from Climbers by the Hopla team"
    );
}

#[test]
fn mr46_team_kind_9_in_any_community_logged_and_lifted_by_item_restore() {
    let mut w = setup();
    let bob = member(&mut w);
    let tk = key();
    let tp = TeamPolicy::from_public_keys(&[pk(&tk)], 1);
    let mid = peer_id_hash(&pk(&bob));
    let mut t = TeamAction::member_remove(w.id, mid, Reason::Threats, "", 7 * DAY, 1);
    t.sign(&tk);
    let env = crate::moderation::seal_team_action(&t, NOW, 4).unwrap();
    let got = crate::moderation::open_team_action(&env).unwrap();
    assert_eq!(got.duration_s, 7 * DAY);
    let e = w.m.apply(&tp, &got, NOW).unwrap();
    assert_eq!(
        (e.kind, e.item_id),
        (crate::moderation::ActionKind::MemberRemove, mid)
    );
    assert!(w
        .m
        .log()
        .iter()
        .any(|l| l.item_id == mid && l.target_id == w.id));
    assert!(!w.s.can_post(&w.m, &w.id, &pk(&bob), NOW + 1));
    // Unsigned (no k signatures) is rejected.
    let unsigned = TeamAction::member_remove(w.id, mid, Reason::Threats, "", 0, 2);
    assert!(w.m.apply(&tp, &unsigned, NOW).is_err());
    let mut rs = TeamAction::item_restore(w.id, mid, 3);
    rs.sign(&tk);
    w.m.apply(&tp, &rs, NOW).unwrap();
    assert!(w.s.can_post(&w.m, &w.id, &pk(&bob), NOW + 2));
    // Durations only on kind 9.
    let mut bad = TeamAction::post_takedown(w.id, [1; 32], Reason::Spam, "", 9);
    bad.duration_s = 5;
    assert!(bad.check_fields().is_err());
}

#[test]
fn mr47_rate_limits_owner_exempt_alert_and_reversal() {
    let mut w = setup();
    let victims: Vec<SigningKey> = (0..40).map(|_| member(&mut w)).collect();
    let id = w.id;
    // 10 in an hour OK, 11th ignored but counted.
    for (i, v) in victims[..10].iter().enumerate() {
        w.s.apply_member_mod(&remove(&w.moder, &id, v, DAY, NOW + i as u64, 1), &w.m, NOW)
            .unwrap();
    }
    assert!(w.s.owner_alerts_for(&id, NOW + 20).len() == 1);
    let eleventh = remove(&w.moder, &id, &victims[10], DAY, NOW + 11, 1);
    assert_eq!(
        w.s.apply_member_mod(&eleventh, &w.m, NOW).unwrap_err(),
        ModReject::OverLimit
    );
    assert!(w
        .s
        .member_removal(&w.m, &id, &pk(&victims[10]), NOW + 20)
        .is_none());
    assert_eq!(
        w.s.member_mods.owner_alerts(&id, NOW + 20)[0].removals_24h,
        11
    );
    // Daily: spread over hours, 31st in 24 h is ignored.
    let a = &w.admin.clone();
    for (i, v) in victims[..30].iter().enumerate() {
        let at = NOW + (i as u64 / 5) * 3_600_000;
        w.s.apply_member_mod(&remove(a, &id, v, DAY, at, 2), &w.m, NOW)
            .unwrap();
    }
    let d31 = remove(a, &id, &victims[30], DAY, NOW + 7 * 3_600_000, 2);
    assert_eq!(
        w.s.apply_member_mod(&d31, &w.m, NOW).unwrap_err(),
        ModReject::OverLimit
    );
    // Owner exempt: 31 in an hour are all applied.
    for (i, v) in victims[..31].iter().enumerate() {
        w.s.apply_member_mod(&remove(&w.owner, &id, v, DAY, NOW + i as u64, 3), &w.m, NOW)
            .unwrap();
    }
    // Every moderator/admin removal is in the Removals view.
    assert_eq!(w.s.member_mods.removals_view(&id).len(), 40);
    // Owner revokes the moderator; their removals stay; owner can reverse one.
    let rv = act(
        &w.owner,
        ModAction::RoleRevoke,
        &id,
        pk(&w.moder),
        0,
        0,
        NOW,
        1,
    );
    w.s.apply_member_mod(&rv, &w.m, NOW).unwrap();
    assert_eq!(
        w.s.get(&id).unwrap().role(&pk(&w.moder)),
        Some(Role::Member)
    );
    let rs = act(
        &w.owner,
        ModAction::MemberRestore,
        &id,
        pk(&victims[0]),
        8,
        0,
        NOW,
        4,
    );
    w.s.apply_member_mod(&rs, &w.m, NOW).unwrap();
    assert!(w
        .s
        .member_removal(&w.m, &id, &pk(&victims[0]), NOW + 1)
        .is_none());
    // Only the owner revokes roles.
    let rv2 = act(
        &w.admin,
        ModAction::RoleRevoke,
        &id,
        pk(&victims[5]),
        0,
        0,
        NOW,
        1,
    );
    assert_eq!(
        w.s.apply_member_mod(&rv2, &w.m, NOW).unwrap_err(),
        ModReject::NotAllowed
    );
}

#[test]
fn mr48_logs_have_no_content_or_evidence() {
    let mut w = setup();
    let bob = member(&mut w);
    w.s.add_post(&post(&bob, w.id, NOW), &w.m, NOW).unwrap();
    w.s.apply_member_mod(
        &remove(&w.moder, &w.id, &bob, DAY, NOW + 5, 1),
        &w.m,
        NOW + 5,
    )
    .unwrap();
    let rs = act(
        &w.owner,
        ModAction::MemberRestore,
        &w.id,
        pk(&bob),
        8,
        0,
        NOW,
        2,
    );
    w.s.apply_member_mod(&rs, &w.m, NOW).unwrap();
    let log = w.s.member_mods.log(&w.id);
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].actor_role, Role::Moderator);
    let mut b = Vec::new();
    ciborium::into_writer(&log, &mut b).unwrap();
    assert!(!b.windows(2).any(|x| x == b"hi"));
}

#[test]
fn mr49_envelopes_carry_matching_0x5c_tlv() {
    let w = setup();
    let x = key();
    let a = remove(&w.owner, &w.id, &x, DAY, NOW, 1);
    let mut env = seal_mod_action(&a, &w.id, NOW, 4);
    assert_eq!(mod_tlv(&env.header_aad), Some(community_ref(&w.id)));
    assert!(env.flags & crate::framing::flags::PRIORITY != 0);
    let pos = env
        .header_aad
        .iter()
        .position(|b| *b == AAD_TLV_COMMUNITY_MOD)
        .unwrap();
    env.header_aad[pos + 4] ^= 1;
    assert!(open_mod_action(&env, &w.id).is_err());
}
