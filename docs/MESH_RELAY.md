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

## Carry store
- Store schema v3 has a `relay` table. It holds sealed envelopes only, so relays
  never see plaintext. Budgets follow §22.4: 50 MB storage by default (the user
  can raise the cap to 500 MB), 20 MB/day forwarding, 72 h default expiry and a
  7 d maximum.
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
- Settings › Mesh relay: on/off and carry cap (EN/FR).

## Release builds
`[profile.release]` uses `lto = "fat"` and `codegen-units = 1`, which keeps the
arm64 core library small enough for the APK to stay under 25 MB.

## Open items and spec gaps
1. **MLS setup transport**: the spec says the KeyPackage is exchanged "with the
   contact QR or first handshake". This implementation uses link PDU `0x04`
   right after the first handshake. `0x04`, and its inner types `0x01`–`0x03`,
   are not in the §21.9 registry. Contacts who added each other by QR but have
   never been in range cannot use the mesh. The spec could allow a Welcome to
   travel over the mesh.
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
6. **Relay default and cap presets** and **non-contact relays** (items (3) and
   (4) of the v0.2.2 review answers) are still pending. The current values
   (on, 50 MB) are placeholders, and relaying only happens over contact links.
7. **No settings mockup** for Mesh relay.
8. **MLS epoch races**: 1:1 groups have no commits after setup. A future
   re-key commit would need the receiver to keep previous-epoch exporters for
   tags (4 past epochs are kept for content).
9. **Platform rule**: core calls for a given link must be made one at a time.
