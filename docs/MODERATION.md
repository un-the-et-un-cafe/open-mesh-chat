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
```

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
mailbox tag (`BLAKE2b-64("OMCP-team-mod-tag-v1\0")`) that every compliant client wants, with TLV `0x8F`
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
| App without team ignores actions | core `app_without_team_ignores_everything`; Android `TeamModerationTest` |

## Gaps

- Communities are a v0.2 feature and are not built yet, so nothing is hidden on screen today. The API
  (`moderation_removed_reason`, `relay_allows`) and the log are ready for the communities code to call.
- The maintainer public key is not in the config yet (shipped default: no team).
- The jury appeal (§23.5) and the ⅔ "fine" result next to a takedown are not implemented.
- MR7's 10-minute bound is not measured. Actions spread at the next encounter like other high-priority
  envelopes.
