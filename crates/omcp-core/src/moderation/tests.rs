//! §23.9 / TM1–TM4 tests. Every key is generated at test time.

use ed25519_dalek::SigningKey;

use super::*;

fn key() -> SigningKey {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    SigningKey::from_bytes(&b)
}

fn pk(k: &SigningKey) -> [u8; 32] {
    k.verifying_key().to_bytes()
}

const C: [u8; 32] = [7u8; 32];

fn signed(mut a: TeamAction, k: &SigningKey) -> TeamAction {
    a.sign(k);
    a
}

#[test]
fn valid_takedown_signature_removes_community_and_logs_it() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let a = signed(TeamAction::takedown(C, Reason::Hate, "", 1), &team);
    let e = st.apply(&p, &a, 1000).unwrap();
    assert!(st.is_removed(&C));
    assert_eq!(st.removal(&C).unwrap().reason, Reason::Hate);
    assert_eq!(e.signers, vec![peer_id_hash(&pk(&team))]);
    assert_eq!(st.log().len(), 1); // MR9
    assert!(!st.relay_allows(Some(&C)));
    assert!(st.relay_allows(Some(&[8u8; 32])));
    assert!(st.relay_allows(None)); // non-community traffic (private chats) untouched
}

#[test]
fn tampered_or_forged_signature_is_rejected() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let mut a = signed(TeamAction::takedown(C, Reason::Spam, "", 1), &team);
    a.target_id = [9u8; 32]; // body changed after signing
    assert_eq!(st.apply(&p, &a, 0), Err(Rejected::BadSignature));
    let mut b = signed(TeamAction::takedown(C, Reason::Spam, "", 1), &team);
    b.sigs[0].signature[0] ^= 1;
    assert_eq!(st.apply(&p, &b, 0), Err(Rejected::BadSignature));
    assert!(!st.is_removed(&C));
    assert!(st.log().is_empty());
}

#[test]
fn non_whitelisted_key_is_ignored() {
    let team = key();
    let stranger = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let a = signed(TeamAction::takedown(C, Reason::Illegal, "", 1), &stranger);
    assert_eq!(st.apply(&p, &a, 0), Err(Rejected::NotWhitelisted));
    let unsigned = TeamAction::takedown(C, Reason::Illegal, "", 1);
    assert_eq!(st.apply(&p, &unsigned, 0), Err(Rejected::BelowThreshold));
    assert!(!st.is_removed(&C));
}

#[test]
fn threshold_counts_distinct_whitelisted_keys() {
    let (a1, a2) = (key(), key());
    let p = TeamPolicy::from_public_keys(&[pk(&a1), pk(&a2)], 2);
    let mut st = ModerationState::default();
    let mut twice = TeamAction::takedown(C, Reason::Terror, "", 1);
    twice.sign(&a1);
    twice.sign(&a1);
    assert_eq!(st.apply(&p, &twice, 0), Err(Rejected::BelowThreshold));
    let mut both = TeamAction::takedown(C, Reason::Terror, "", 1);
    both.sign(&a1);
    both.sign(&a2);
    assert_eq!(st.apply(&p, &both, 0).unwrap().signers.len(), 2);
}

#[test]
fn app_without_team_ignores_everything() {
    let k = key();
    let p = TeamPolicy::from_config("# no team\n").unwrap();
    let a = signed(TeamAction::takedown(C, Reason::Hate, "", 1), &k);
    assert_eq!(
        ModerationState::default().apply(&p, &a, 0),
        Err(Rejected::NoTeam)
    );
}

#[test]
fn replayed_takedown_is_rejected() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let a = signed(TeamAction::takedown(C, Reason::Hate, "", 5), &team);
    st.apply(&p, &a, 0).unwrap();
    assert_eq!(st.apply(&p, &a, 1), Err(Rejected::Replay));
    let older = signed(TeamAction::takedown(C, Reason::Spam, "", 4), &team);
    assert_eq!(st.apply(&p, &older, 1), Err(Rejected::Replay));
    assert_eq!(st.log().len(), 1);
}

#[test]
fn restore_lifts_only_older_takedown_and_old_takedown_cannot_come_back() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let td = signed(TeamAction::takedown(C, Reason::Harassment, "", 1), &team);
    let rs = signed(TeamAction::restore(C, 2), &team);
    let stale_rs = signed(TeamAction::restore(C, 1), &team);

    // In order.
    let mut st = ModerationState::default();
    st.apply(&p, &td, 0).unwrap();
    assert_eq!(st.apply(&p, &stale_rs, 1), Err(Rejected::Replay));
    assert!(st.is_removed(&C));
    st.apply(&p, &rs, 2).unwrap();
    assert!(!st.is_removed(&C));
    // Replaying the old takedown after the restore does nothing.
    assert_eq!(st.apply(&p, &td, 3), Err(Rejected::Replay));
    assert!(!st.is_removed(&C));
    let log = st.log();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].kind, ActionKind::Restore); // newest first

    // Out of order (restore gossiped first): same end state.
    let mut st2 = ModerationState::default();
    st2.apply(&p, &rs, 0).unwrap();
    assert_eq!(st2.apply(&p, &td, 1), Err(Rejected::Replay));
    assert!(!st2.is_removed(&C));

    // A newer takedown after a restore removes it again.
    let td3 = signed(TeamAction::takedown(C, Reason::Hate, "", 3), &team);
    st.apply(&p, &td3, 4).unwrap();
    assert!(st.is_removed(&C));
}

#[test]
fn reason_other_needs_a_note_and_restore_has_no_reason() {
    let team = key();
    let mut a = TeamAction::takedown(C, Reason::Other, " ", 1);
    a.sign(&team);
    assert!(a.encode().is_err());
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    assert_eq!(
        ModerationState::default().apply(&p, &a, 0),
        Err(Rejected::Malformed)
    );
    let ok = signed(TeamAction::takedown(C, Reason::Other, "doxxing", 1), &team);
    assert!(ModerationState::default().apply(&p, &ok, 0).is_ok());
}

#[test]
fn wire_and_envelope_round_trip() {
    let team = key();
    let a = signed(
        TeamAction::takedown(C, Reason::Illegal, "court order", 42),
        &team,
    );
    let w = a.encode().unwrap();
    assert_eq!(TeamAction::decode(&w).unwrap(), a);
    assert!(TeamAction::decode(&w[..w.len() - 1]).is_err());
    let env = seal_team_action(&a, 1_000_000, 8).unwrap();
    assert!(is_team_mod_envelope(&env));
    assert_eq!(env.flags & flags::PRIORITY, flags::PRIORITY);
    let back = EnvelopeV1::decode(&env.encode().unwrap()).unwrap();
    assert_eq!(open_team_action(&back).unwrap(), a);
    let mut bad = back.clone();
    bad.ciphertext[1] ^= 1;
    assert!(open_team_action(&bad).is_err());
}

#[test]
fn config_parses_public_keys_and_threshold() {
    let k = key();
    let cfg = format!("# Hopla\nk = 1\nteam_key = {}\n", hex::encode(pk(&k)));
    let p = TeamPolicy::from_config(&cfg).unwrap();
    assert_eq!(p.k, 1);
    assert!(p.members.contains(&peer_id_hash(&pk(&k))));
    assert!(
        TeamPolicy::from_config(&format!("k = 2\nteam_key = {}", hex::encode(pk(&k)))).is_err()
    );
    assert!(TeamPolicy::from_config("team_key = zz").is_err());
    // The shipped config parses.
    TeamPolicy::from_config(include_str!("../../../../config/team_moderation.conf")).unwrap();
}

#[test]
fn state_survives_serialization() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    st.apply(
        &p,
        &signed(TeamAction::takedown(C, Reason::Hate, "", 1), &team),
        0,
    )
    .unwrap();
    let mut buf = Vec::new();
    ciborium::into_writer(&st, &mut buf).unwrap();
    let back: ModerationState = ciborium::from_reader(buf.as_slice()).unwrap();
    assert_eq!(back, st);
    assert_eq!(back.current_actions().len(), 1);
}

// ---- Live takedown (v2 kind 7 / ITEM_RESTORE 8) ----

const S: [u8; 16] = [9u8; 16];

#[test]
fn live_takedown_and_item_restore_use_the_team_key_with_own_epochs() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let td = signed(
        TeamAction::live_takedown(C, S, Reason::Terror, "", 1),
        &team,
    );
    let back = TeamAction::decode(&td.encode().unwrap()).unwrap();
    assert_eq!(back, td);
    st.apply(&p, &back, 1).unwrap();
    assert!(st.is_live_removed(&S));
    assert!(!st.live_allowed(&S, None));
    assert!(st
        .removed_live_tags()
        .contains(&crate::live::stream_tag(&S)));
    assert!(!st.is_removed(&C)); // only the item
    let c1 = signed(TeamAction::takedown(C, Reason::Spam, "", 1), &team);
    st.apply(&p, &c1, 2).unwrap();
    assert!(!st.live_allowed(&[1; 16], Some(&C)));
    assert!(st.live_allowed(&[1; 16], Some(&[3; 32])));
    assert_eq!(st.apply(&p, &back, 3), Err(Rejected::Replay));
    let rs = signed(TeamAction::item_restore(C, pad_id(&S), 2), &team);
    st.apply(&p, &rs, 4).unwrap();
    assert!(!st.is_live_removed(&S));
    assert_eq!(st.apply(&p, &back, 5), Err(Rejected::Replay));
    assert_eq!(st.log()[0].kind, ActionKind::ItemRestore);
}

#[test]
fn unsigned_foreign_or_reinterpreted_v2_actions_are_rejected() {
    let team = key();
    let p = TeamPolicy::from_public_keys(&[pk(&team)], 1);
    let mut st = ModerationState::default();
    let unsigned = TeamAction::live_takedown(C, S, Reason::Spam, "", 1);
    assert_eq!(st.apply(&p, &unsigned, 1), Err(Rejected::BelowThreshold));
    let foreign = signed(TeamAction::live_takedown(C, S, Reason::Spam, "", 1), &key());
    assert_eq!(st.apply(&p, &foreign, 1), Err(Rejected::NotWhitelisted));
    // A post takedown signature can't be re-used as a topic or group action.
    let mut c = signed(
        TeamAction::post_takedown(C, [4; 32], Reason::Spam, "", 1),
        &team,
    );
    c.kind = ActionKind::TopicTakedown;
    assert_eq!(st.apply(&p, &c, 1), Err(Rejected::BadSignature));
    // Item kinds need an item; group kinds must not have one.
    let mut bad = TeamAction::post_takedown(C, [0; 32], Reason::Spam, "", 1);
    bad.sign(&team);
    assert!(bad.encode().is_err());
    let mut bad = TeamAction::group_restore(C, 1);
    bad.item_id = [1; 32];
    assert!(bad.encode().is_err());
    assert!(!st.is_live_removed(&S));
}
