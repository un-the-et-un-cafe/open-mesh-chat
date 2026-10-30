//! v0.1.25 settled points: MR25/MR28/MR29/MR30/MR32. Keys are made at test time.

use ed25519_dalek::SigningKey;

use super::*;

fn key() -> SigningKey {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    SigningKey::from_bytes(&b)
}
fn setup() -> (SigningKey, TeamPolicy) {
    let k = key();
    let p = TeamPolicy::from_public_keys(&[k.verifying_key().to_bytes()], 1);
    (k, p)
}
fn signed(mut a: TeamAction, k: &SigningKey) -> TeamAction {
    a.sign(k);
    a
}
const C: [u8; 32] = [0x11; 32];
const HOST: [u8; 32] = [0x22; 32];
const NOW: u64 = 1_800_000_000_000;

#[test]
fn mr28_restores_carry_reason_8_and_reason_0_is_rejected() {
    let (k, p) = setup();
    for mut r in [
        TeamAction::restore(C, 2),
        TeamAction::item_restore(C, [5; 32], 2),
        TeamAction::group_restore(C, 2),
    ] {
        assert_eq!(r.reason, Some(Reason::RestoreAfterReview));
        assert!(r.check_fields().is_ok());
        r.reason = None;
        assert!(r.check_fields().is_err());
        let mut st = ModerationState::default();
        assert!(st.apply(&p, &signed(r, &k), NOW).is_err());
    }
    // Reason 8 is not a takedown reason.
    let mut t = TeamAction::takedown(C, Reason::Spam, "", 1);
    t.reason = Some(Reason::RestoreAfterReview);
    assert!(t.check_fields().is_err());
}

#[test]
fn mr29_live_restart_block_lasts_24h_and_restore_does_not_shorten_it() {
    let (k, p) = setup();
    let mut st = ModerationState::default();
    st.block_live_host(C, HOST, NOW);
    assert!(st.live_host_blocked(&C, &HOST, NOW + 1));
    assert!(!st.live_host_blocked(&C, &[0x23; 32], NOW + 1));
    assert!(!st.live_host_blocked(&[0x12; 32], &HOST, NOW + 1));
    st.apply(
        &p,
        &signed(TeamAction::item_restore(C, [9; 32], 2), &k),
        NOW + 10,
    )
    .unwrap();
    assert!(st.live_host_blocked(&C, &HOST, NOW + LIVE_RESTART_BLOCK_MS - 1));
    assert!(st.live_host_blocked(&C, &HOST, NOW - 5)); // clock moved back
    assert!(!st.live_host_blocked(&C, &HOST, NOW + LIVE_RESTART_BLOCK_MS));
}

fn taken_down_and_restored() -> ModerationState {
    let (k, p) = setup();
    let mut st = ModerationState::default();
    let r = [0x33; 32];
    st.apply(
        &p,
        &signed(TeamAction::takedown(C, Reason::Spam, "", 1), &k),
        NOW,
    )
    .unwrap();
    st.apply(
        &p,
        &signed(
            TeamAction::group_takedown(r, Reason::Csam, [1; 32], "", 1),
            &k,
        ),
        NOW,
    )
    .unwrap();
    st.apply(&p, &signed(TeamAction::restore(C, 2), &k), NOW)
        .unwrap();
    st
}

#[test]
fn mr25_mr30_tombstones_survive_restore_and_backup_merge_with_any_clock() {
    let st = taken_down_and_restored();
    let r = [0x33; 32];
    assert!(!st.is_removed(&C) && st.is_community_tombstoned(&C));
    assert!(st.is_group_removed(&r) && st.is_group_tombstoned(&r));
    let ts = st.tombstones();
    assert_eq!(ts.iter().filter(|t| t.restored).count(), 1);
    // A backup from before the takedown, merged with clocks years either way.
    for skew in [0u64, 5 * 365 * 86_400_000, NOW - 1] {
        let mut old = ModerationState::default();
        let mut shifted = ts.clone();
        for t in &mut shifted {
            t.applied_ms = t.applied_ms.wrapping_add(skew);
        }
        old.merge_tombstones(&shifted);
        assert!(old.is_community_tombstoned(&C));
        assert!(old.is_group_tombstoned(&r));
        assert!(!old.is_removed(&C) && old.is_group_removed(&r));
        // Merging the other way never loses one either.
        let mut cur = taken_down_and_restored();
        cur.merge_tombstones(&old.tombstones());
        assert!(cur.is_community_tombstoned(&C) && cur.is_group_tombstoned(&r));
    }
    // A re-formed group with a new random ID is unaffected.
    assert!(!st.is_group_tombstoned(&[0x34; 32]));
}

#[test]
fn mr32_legacy_state_is_discarded_but_removed_communities_stay_tombstoned() {
    #[derive(serde::Serialize)]
    struct Old {
        epoch: u64,
        removed: bool,
        reason_code: u8,
    }
    #[derive(serde::Serialize)]
    struct Legacy {
        communities: BTreeMap<[u8; 32], Old>,
    }
    let mut m = BTreeMap::new();
    m.insert(
        C,
        Old {
            epoch: 4,
            removed: true,
            reason_code: 1,
        },
    );
    m.insert(
        [0x12; 32],
        Old {
            epoch: 2,
            removed: false,
            reason_code: 0,
        },
    );
    let mut b = Vec::new();
    ciborium::into_writer(&Legacy { communities: m }, &mut b).unwrap();
    let st = ModerationState::from_legacy_v1(&b).unwrap();
    assert!(st.is_community_tombstoned(&C));
    assert!(!st.is_community_tombstoned(&[0x12; 32]));
    assert_eq!(st.tombstones().len(), 1);
    assert!(ModerationState::from_legacy_v1(b"junk").is_none());
}
