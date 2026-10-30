# Mesh relay (PROTOCOL §22 over BLE links): implementation notes

Status: experimental. Follows PROTOCOL v0.2.2 §22.8. This page describes what the
reference implementation does and lists where it goes beyond the spec, or where
the spec is still open.

## Transport (§21.9, §22.8(2))
- Mesh traffic rides existing contact links (Noise hybrid session) as link PDU
  `0x03` `MESH_FRAME`, one link-AEAD fragment stream per frame, on the same
  `LinkSession` as `0x02` with its own reassembler.
- Mesh frames inside `0x03`:

  | Type | Name | Body |
  |---|---|---|
  | `0x48` | `HELLO` | 6 B (§22.7(12)) |
  | `0x52` | `DIGEST_REQ` | 6 B (§22.7(12)) |
  | `0x44` | `DATA` | `EnvelopeV1` |
  | `0x45` | `DELIVERY_ACK` | `msg_hash[16] ‖ EnvelopeV1` |
  | `0x64` | `DIGEST_DELTA` | `mode ‖ body`: `0x00` delta, `0x01` full cuckoo filter |

- On link-up: full digest (`DIGEST_DELTA` mode `0x01`) + `HELLO`, then forwarding
  in the order *wanted by peer* > *lowest carry count* > *scoped flood* (flooding
  only while ≤ 32 peers are visible). Dedup by `envelope_id`, persisted in
  `mesh_seen`.

## Relay rules RL1–RL4 (apps/REQUIREMENTS.md, maintainer decision 2026-10-24)
- **RL1, always on.** There is no relay switch anywhere (core, FFI, Android, iOS).
  Every envelope from any neighbour is carried and passed on; whether the
  neighbour is a contact only affects RL2 priority. A `mesh-settings` row
  left by older builds (relay off, custom cap) is ignored.
  Tests: `rl1_relay_always_on_carries_from_contacts_and_strangers` (core),
  `rl1_relay_always_on_old_off_setting_ignored` (FFI).
- **RL2, automatic cap.** One carry store, own outbound envelopes included,
  capped at 500 MB (`RELAY_CAP_BYTES`). The apps report free and total
  storage on every mesh tick (`mesh_set_free_storage`; Android `StatFs` on
  `filesDir`, iOS `volumeAvailableCapacityForImportantUsage`). Under 10 % free,
  the cap becomes `500 MB × free / (10 % of total)`, reaching 0 when the disk is
  full. When full, the store drops by class, then oldest first (by
  `created_ms`, clamped to the time it was stored): news, then everything
  else, then contacts' envelopes, then own chats. Store-and-forward offers
  to a neighbour use the reverse order (own chats first, oldest first within
  a class). The class is persisted per carry row; rows from older builds map
  to own chats (own rows) or contacts (all earlier links were contact links).
  Tests: `rl2_*` in `omcp-core::sync::{budget,relay}` and `omcp-ffi`.
- **RL3, deletion.** On a `DELIVERY_ACK` (§22.8(3)) or at `outer_expiry`, at most
  7 d after it was stored. Tests: `rl3_*` (core, FFI).
- **RL4, Settings.** Settings › Mesh relay shows only "Storage used for relaying:
  X MB" / "Stockage utilisé pour le relais : X Mo" (`MeshStats.storage_used_bytes`,
  the whole carry store). Tests: `rl4_*` (FFI, Android `MeshRelayTest`, iOS
  `MeshRelayTests`).
- Gateway rules are unchanged: Wi-Fi only by default, a warning (not a block)
  on metered cellular, and the separate 500 MB/day gateway Internet cap.

## Carry store
- Store schema v3 has a `relay` table. It holds sealed envelopes only, so relays
  never see plaintext. Storage follows RL2 above (automatic 500 MB cap). The
  §22.4 forwarding budget (20 MB/day), 72 h default expiry and 7 d maximum are
  unchanged.
- `outer_expiry` is `header_aad` TLV `0x58` (u32 BE unix seconds). Relays drop
  expired envelopes, and a later expiry is clamped to now + 7 d. Burn envelopes
  expire at sent + unread TTL.
- Frames queued for a link that drops before they are sent are offered again on
  the next link.

## End-to-end keys: 2-member MLS (§22.7(5), §22.8(1))
- Every 1:1 chat is a 2-member MLS group (OpenMLS, `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`).
  There is no static-DH key.
- **Group setup** happens on the first direct link between the two contacts, over
  link PDU `0x04` (see gaps). The higher peer id sends `KEY_PACKAGE (0x01)`. The
  lower id creates the group, adds the joiner and answers `WELCOME (0x02)`. The
  joiner confirms with `JOINED (0x03)`. Existing contacts migrate the same way on
  their next direct link. If the joiner has lost its state, it sends a KeyPackage
  again and the creator starts a fresh group.
- **No relay before MLS.** Until both sides have the group, messages stay in the
  local queue and go direct only. Once the group is up, anything still queued is
  handed to the mesh.
- Tags: `tag_secret = MLS-Exporter("OMCP-tag", tag_epoch, 32)`. The chat tag
  (`0x54`) and the recipient sub-tag (`0x56`, the recipient's leaf) are derived
  from it. Receivers want their own sub-tag for the current and the previous
  tag epoch.
- Content: an MLS application message (PrivateMessage). Its MLS AAD binds
  `version ‖ suite ‖ envelope_id ‖ created_ms ‖ flags ‖ header_aad`, and the
  sender must be the other leaf. Settings for store-and-forward:
  out-of-order tolerance 1000, maximum forward distance 10 000, 4 past
  epochs, padding 64 B.
- MLS state (the OpenMLS key store) is persisted per contact in a sealed store row.

## Delivery ack and status (§22.8(3))
- For every mesh message, the recipient sends `DELIVERY_ACK`. `msg_hash` is the
  first 16 B of BLAKE2b-256 over the ciphertext and travels in the clear. The
  attached envelope carries `0x45 ‖ msg_hash` as an MLS application message to
  the sender's sub-tag, and is routed like `DATA`.
- Relays that see an ack delete the matching envelope they carry for others and
  refuse later copies. They never delete the sender's own copy. The sender
  trusts only the MLS-authenticated ack, then withdraws its copy.
- Status: Queued → Sent (the frame was actually sealed onto a link to a relay, or
  sent directly) → Delivered (verified ack). Both apps call
  `mesh_take_status_updates()` after each link step and tick, and show
  "Delivered · Mesh" / "Distribué · Mesh" (Android) and "Delivered" /
  "Distribué" (iOS).

## Device-key rotation (§22.8(5))
- `rotate_device_key()` mints a new device key and certificate and keeps the
  previous one for 7 days (`DEVICE_KEY_OVERLAP_MS`, stored sealed). During the
  overlap, a responder whose first handshake message fails with the current key
  retries with the previous one and signs with the previous certificate.
- Envelopes are bound to the MLS epoch, not to the device key, so carried
  envelopes still deliver after a rotation (test:
  `mesh_device_key_rotation_mid_transit_still_delivers`).

## Media (§22.8(4))
Media and file chunks, and media control messages, stay **direct-only**. This
is the allowed interim; relaying for 1:1 chats and groups of ≤ 8 (≤ 4 hops,
counted against the cap) is not implemented.

## Apps
- Android and iOS: the beacon goes in the scan response (Android) and in-link
  only (iOS). Up to 4 links (2 on low battery). Duty cycle: charging, 5 s tick;
  battery ≥ 20 %, 15 s; low battery or power saver, a 30 s scan every 5 min with
  a 60 s tick.
- Settings › Mesh relay: read-only "Storage used for relaying: X MB" (EN/FR), RL4.
  The You tab row reads "Always on · storage used for relaying".

## Release builds
`[profile.release]` uses `lto = "fat"` and `codegen-units = 1`, which keeps the
arm64 core library small enough for the APK to stay under 25 MB.

## Open items and spec gaps
1. **MLS setup transport**: the spec says the KeyPackage is exchanged "with the
   contact QR or first handshake". This implementation uses link PDU `0x04`
   `MLS_SETUP` (registered in §21.9) right after the first handshake. Its inner
   frame types (`0x01` KEY_PACKAGE, `0x02` WELCOME, `0x03` JOINED) are named in
   the registry but their values are not; these values are this
   implementation's. Link PDUs `0x01` (handshake) and `0x02` (link data) are
   not in §21.9 either. Contacts who added each other by QR but have never been
   in range cannot use the mesh. The spec could allow a Welcome to travel over
   the mesh. Test: `mls_setup_is_link_pdu_0x04_per_registry`.
2. **Full digest**: §22.8(2) assigns no full-digest frame. A full filter is sent
   as `DIGEST_DELTA` with mode byte `0x01`.
3. **`DELIVERY_ACK` layout**: §22.8(3) describes a 16-byte ack that is also
   end-to-end MACed and addressed to the sender's sub-tag. Here the frame is
   `0x45 ‖ msg_hash[16] ‖ EnvelopeV1`: the clear hash is for relays, and the
   envelope (MLS application message `0x45 ‖ msg_hash`) carries the routing
   and the MAC. The exact wire layout should be pinned in the spec.
4. **Ack authenticity at relays**: the clear `msg_hash` is not authenticated, so
   a malicious contact-relay can make other relays drop copies (never the
   sender's own copy). The spec could define a relay-verifiable ack.
5. **Device-key rotation UX**: contacts treat a rotated key like a new device
   (the existing restore notice and badge). The spec does not say whether a
   planned rotation should show that notice. The overlap also only covers
   handshakes where we are the responder.
6. **Non-contact links**: RL1 removes the contact/stranger split, and the engine
   carries for any neighbour, but the apps still only open links to contacts,
   so in practice every carried envelope is a "contact" one. Opening links to
   strangers needs its own handshake rules (not specified).
7. **No settings mockup** for Mesh relay (DESIGN_SPEC Q36 is a placeholder):
   plain one-row layouts, and the You-tab subtitle is placeholder copy.
8. **MLS epoch races**: 1:1 groups have no commits after setup. A future
   re-key commit would need the receiver to keep previous-epoch exporters for
   tags (4 past epochs are kept for content).
9. **Platform rule**: core calls for a given link must be made one at a time.
10. **RL2 open points** (conservative choices, see "Relay rules RL1–RL4"):
    how far the cap is lowered under 10 % free (proportional here); "oldest"
    measured by `created_ms`; own outbound copies counted in the cap (the
    local direct queue keeps the message either way); "contacts' envelopes"
    = envelopes handed over by a contact (relays cannot see the real sender);
    news = envelopes on registered news channel tags (`set_news_tags`; news
    tags are not wired up yet). PROTOCOL §22.4 (50 MB default, eviction by
    `(priority, expires_at, hops)`) and SY5 still describe the old budget.
