# Hopla team moderation

Implements PROTOCOL §23.9 (App-team moderation) and REQUIREMENTS TM1–TM4 in `omcp-core`
(`crates/omcp-core/src/moderation/`), the FFI (`crates/omcp-ffi/src/moderation.rs`) and both apps
("You → Removed communities").

## Trust model

- **Whitelist + threshold** ship with the app in [`config/team_moderation.conf`](../config/team_moderation.conf):
  `k = <n>` and one `team_key = <Ed25519 public key, hex>` per team member. Each key is matched by its
  PeerID hash, BLAKE2b-256 of the public key (§22). The list changes only with an app release, never
  over the mesh (§23.9(1)).
- **Hopla v0.2 ships `k = 1` with one maintainer key** (TM2). An AI moderator can be added later as a
  second `team_key` and `k` raised to 2, with no protocol change (§23.9(3)).
- **Public keys only. Never commit a private key.** Tests generate their keys at test time.
- **No key in the config = no team.** Every TAKEDOWN/RESTORE is ignored (`NoTeam`). This is the
  shipped default until the maintainer adds the public key, and it is what other OMCP apps get unless
  they ship their own list (TM4).

## Messages

```
TAKEDOWN{community_id[32], reason_code, note, epoch, sigs[]}
RESTORE {community_id[32], epoch, sigs[]}
TAKEDOWN_LIVE{stream_id[16] ‖ 0[16], reason_code, note, epoch, sigs[]}   // kind 3
RESTORE_LIVE {stream_id[16] ‖ 0[16], epoch, sigs[]}                      // kind 4
```

Live takedowns use the **same** team keys, threshold, verification (`verify_strict`), envelope, tag,
propagation and transparency log as community actions; only the kind byte and the target differ. The
kind is inside the signed body, so a community action can't be replayed as a live one. Live targets
keep their own epochs (a separate map), so they never collide with community epochs.

Signed body: `"OMCP-team-mod-v1" ‖ 0x00 ‖ kind (1 takedown, 2 restore) ‖ community_id ‖ epoch u64 BE ‖
reason u8 (0 for restore) ‖ note_len u16 BE ‖ note`. Signatures are Ed25519 (`verify_strict`), one
`(public_key, signature)` pair per signer, at most 16. Reason codes (§23.9(7)): 1 hate speech or
incitement, 2 illegal content, 3 terrorism or violent extremism, 4 coordinated harassment, 5 spam or
scam, 9 other (a note is required). Notes are at most 500 bytes.

An action is **valid** only with signatures from `k` *distinct* whitelisted keys (MR8). Signatures
from other keys are ignored; a bad signature from a whitelisted key never counts.

## Ordering and replay

Each community has one moderation epoch. An action applies only if its epoch is **strictly greater**
than the last one applied for that community:

- replaying a takedown or restore does nothing (`Replay`);
- a RESTORE lifts only a TAKEDOWN with a lower epoch;
- an old TAKEDOWN cannot come back after a newer RESTORE;
- the highest epoch wins regardless of gossip order.

The key holder must use a new, higher epoch for each action on a community (for example a Unix
timestamp in seconds).

## Propagation

Actions travel as public envelopes (`flags = RELAY_OK | STORE_OK | PRIORITY`) on one well-known
mailbox tag (`BLAKE2b-64("OMCP-team-mod-tag-v1\0")`) that every compliant client wants, with TLV `0x5A` (`team_mod`, value = body version 1 or 2; §21.8, §23.10.7)
and a 30-day outer expiry (§23.9(2)). Relays keep them for at most 7 days (the relay store cap), so the
publishing phone keeps them in its carry store and re-offers them at every encounter while they are in
force. Every receiving node verifies, applies, logs and relays them. Invalid actions are dropped and
not gossiped.

## Effect (TM1, TM4)

`moderation_removed_reason(community_id)` returns the reason code for a removed community. Compliant
clients must hide it from lists, search and Nearby, delete its posts, media and invite links, refuse
joins and posts, and show "This community was removed by the Hopla team" with the reason. Relays drop
its envelopes (`ModerationState::relay_allows`). The module never reads, changes or deletes 1:1 or
group chats (MR10).

### Lives

A live is blocked when its stream is taken down (`TAKEDOWN_LIVE`) **or** its community is
(`ModerationState::live_allowed`). On every accepted action the FFI runs `enforce_live_moderation`:

- **Host:** the session is dropped, `live_push_audio` fails, and `live_take_ended` reports
  `taken_down`, so the app stops the mic and camera and closes every WebRTC connection. A new live in a
  removed community is refused.
- **Viewer:** listening stops, the stream key is dropped (`live_media_key` = none), joining is refused,
  and the app closes its peer connections. Without a key, `LiveVideoController.poll` closes every peer
  (Android and iOS), so a relaying viewer also stops forwarding video to its children.
- **Relays (any phone):** mesh audio packets whose public tag (`BLAKE2b-64(stream_id)`) belongs to a
  taken-down stream are neither played nor forwarded (`LiveStats.dropped_moderation`). Community
  announcements of a removed community are neither applied nor forwarded.
- **Spread:** the same as community actions: a public high-priority envelope over the mesh, re-offered
  at every encounter, or pasted on an online phone.

The input to `team_mod takedown-live` is the 32-hex stream id. There is no "Report this live" screen
yet that shows it to users (design gap); today the team gets it from a reporter's debug build.

## Transparency (TM2, MR9)

Every accepted takedown and restore is appended to a log persisted in the encrypted store and shown
in **You → Removed communities**: reason, note, date, community and the short ids of the team keys
that signed.

## Key holder workflow

The tool `crates/omcp-core/examples/team_mod.rs` runs offline on the key holder's machine:

```sh
cargo run -p omcp-core --example team_mod -- keygen ~/hopla-team.key   # prints the PUBLIC key
# put that public key in config/team_moderation.conf as `team_key = …`, then release the app
cargo run -p omcp-core --example team_mod -- takedown ~/hopla-team.key <community_id_hex> 1 <epoch> "note"
cargo run -p omcp-core --example team_mod -- restore  ~/hopla-team.key <community_id_hex> <epoch>
cargo run -p omcp-core --example team_mod -- takedown-live ~/hopla-team.key <stream_id_hex> 3 <epoch> "note"
cargo run -p omcp-core --example team_mod -- restore-live  ~/hopla-team.key <stream_id_hex> <epoch>
```

`keygen` writes the secret key to a new 0600 file outside the repo. Keep it offline (§23.9(8)). Paste
the printed signed action (hex) into **You → Removed communities → Publish** on any phone. That field
only appears in builds that ship a team. If a key is compromised, remove it in the next app release.

## Tests

| What | Where |
|------|-------|
| Signature verification, tampered bodies, forged signatures | core `moderation::tests::tampered_or_forged_signature_is_rejected` |
| Non-whitelisted key rejected, unsigned below threshold | core `non_whitelisted_key_is_ignored`; FFI `tm_non_whitelisted_or_forged_action_is_ignored_and_not_gossiped` |
| k distinct keys | core `threshold_counts_distinct_whitelisted_keys` |
| Replay protection | core `replayed_takedown_is_rejected` |
| RESTORE ordering (in order and out of order) | core `restore_lifts_only_older_takedown_and_old_takedown_cannot_come_back` |
| Mesh propagation and restore, private chats untouched | FFI `tm_takedown_gossips_to_neighbour_and_restore_undoes_it` |
| Persistence across restart, shipped config parses | FFI `tm_moderation_state_survives_restart_and_shipped_policy_parses` |
| Live takedown/restore with the team key, own epochs, community takedown blocks its lives | core `live_takedown_and_restore_use_the_team_key_with_own_epochs` |
| Unsigned, foreign or community-signature-reused live takedowns rejected | core `unsigned_foreign_or_reinterpreted_live_takedowns_are_rejected`; FFI `unsigned_foreign_or_replayed_live_takedowns_are_ignored` |
| A valid live takedown stops host, viewer and relay (over the mesh) | FFI `live_takedown_stops_host_viewer_and_relay` |
| Community takedown stops its lives and blocks new ones | FFI `community_takedown_blocks_its_lives_and_announcements` |
| App without team ignores actions | core `app_without_team_ignores_everything`; Android `TeamModerationTest` |

## Gaps

- Communities exist now (minimal v0.2 cut, `omcp-ffi/src/community.rs`): removed ones show the removal
  banner, can't start or join lives, and their announcements are not forwarded. Community posts and
  search don't exist yet, so there is nothing else to hide.
- A phone running another app (no team) keeps hosting after a live takedown. Compliant relays then
  drop its audio, but a direct WebRTC link to a non-compliant viewer is out of our reach.
- The maintainer public key is not in the config yet (shipped default: no team).
- The jury appeal (§23.5) and the ⅔ "fine" result next to a takedown are not implemented.
- MR7's 10-minute bound is not measured. Actions spread at the next encounter like other high-priority
  envelopes.


## v0.1.24: private groups, item takedowns, evidence (PROTOCOL §23.10)

- **Communities** are open (signed, not end-to-end encrypted): `omcp-core::community`.
  About 5,000 members, 4 KB posts, history 30 days or 50 MB per community (oldest
  dropped first, budgeted separately from the relay cap). Only the founder appoints
  moderators; a moderator removal needs a reason and hides the post from everyone.
- **Private groups** are the end-to-end encrypted MLS groups (up to 32 members,
  founder invites), with 32-byte random group IDs and no readable metadata in the
  GroupContext or authenticated data.
- **Team actions v2** (TLV `0x5A`): 3 GROUP_TAKEDOWN, 4 GROUP_RESTORE, 5 POST_TAKEDOWN,
  6 TOPIC_TAKEDOWN, 7 LIVE_TAKEDOWN, 8 ITEM_RESTORE. Every takedown erases the item on
  the device and leaves a tombstone that is applied before any backup restore.
  Restores only lift the ban. 1:1 chats are never affected.
- **Hard rule**: team keys can never read, join or list a private group; Adds and
  Welcomes with a team identity are refused; takedowns and reports carry no member list.
- **Evidence** (`.omcp-evidence`, deterministic CBOR, ≤ 500 messages, reporter-signed,
  anonymous one-off key): `omcp-core::moderation::evidence`. Verified with the MLS
  sender signatures (no franking). Not offered for 1:1 chats.
- **Known limits**: a takedown cannot erase screenshots, exported copies or devices
  that never receive it, and a modified app can ignore it.
- `team_mod v2 …` (example tool) signs v2 actions offline.

## v0.1.25: real MLS signatures in evidence, tombstones, community budget

- **Evidence uses the sender's real MLS signature.** A patched openmls
  (`vendor/README.md`) exposes the signed bytes of every received
  private-group application message; the app keeps them (up to 500 per group,
  erased with the group) and `report_kept_messages` hands them to
  `report_export_evidence`. Key 8 `wire_format`: 2 verifiable, 1 shown as
  unverified, anything else makes the file invalid. Verification (§23.10.1(4)
  a–e) checks the leaf's own signature, identity = sender, then the
  `FramedContentTBS` signature with the leaf's key.
- **Restores carry reason 8**; reason 0 or missing is rejected (MR28).
- **Tombstones are never deleted**; a restore marks them `restored`. Backups
  merge tombstones without comparing clocks. Tombstoned groups (either state)
  refuse Welcomes, commits and messages; a re-formed group needs a new random
  ID (MR25/MR30). After a backup restore groups show "Checking…" until one full
  digest exchange (MR30).
- **Live restart block**: 24 h per (community, host) after a Live takedown; a
  restore doesn't shorten it (MR29).
- **TLV 0x5B `community_post`** (community_ref, topic_ref, post_id) on posts;
  relays drop posts under a takedown (MR31).
- **Legacy (0.1.23) moderation state** is discarded on upgrade; removed
  communities stay tombstoned (MR32).
- **Community traffic budget**: relay class `Community`, own 50 MB store, not
  counted toward the 500 MB / daily relay budget (§22.4).
- **Dev moderation key**: `OMCP_DEV_MOD_KEY` (public key) is compiled in only
  for debug builds (`OMCP_RUST_PROFILE=debug`) and only with
  `debug_assertions`. Release builds are always keyless.
- **Not yet**: inviting community members who aren't contacts into a private
  group — deferred: public delivery would show the team who is invited, which
  breaks the hard rule; needs a sealed-delivery spec.
