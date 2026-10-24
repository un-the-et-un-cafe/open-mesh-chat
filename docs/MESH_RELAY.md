# Mesh relay (PROTOCOL §22 over BLE links) — beta implementation notes

Status: experimental. This describes what the reference implementation does
today and lists where it departs from, or goes beyond, PROTOCOL.md.

## Transport
- Mesh frames ride existing contact links (Noise hybrid session) as link PDU
  `0x03`, using the same `LinkSession` and a separate reassembler.
  (`0x01` = handshake chunks, `0x02` = direct link fragments.)
- Frame types inside PDU 0x03: `0x48` HELLO, `0x52` DIGEST_REQ (§22, existing),
  `0x44` full digest (cuckoo filter), `0x64` digest delta, `0x45` envelope.
- On link-up: HELLO -> digest exchange -> forwarding in the order
  *wanted by peer* > *lowest carry count* > *scoped flood* (flood only when
  <=32 peers are visible, §22 routing). Dedup by `digest_id` (BLAKE2b nn=4,
  BE u32, 0 -> 1) persisted in `mesh_seen`.

## Carry store
- Store migration v3 adds a `relay` table. Items hold sealed envelopes only;
  relays never see plaintext. Budgets per §22.4: 50 MB storage default
  (user cap up to 500 MB), 20 MB/day forwarding, 72 h default expiry, 7 d max.
- Outer expiry travels in experimental TLV `0x80`. Burn envelopes expire at
  sent + unread TTL.

## End-to-end keys (interim)
§22.7(5) expects an MLS exporter. 1:1 chats have no MLS group yet, so the beta uses:
- `pair = HKDF-SHA384(salt "OMCP-mesh-pair-v1", X25519(device dh, contact dh), lo_id||hi_id)`
- mailbox tag secret = `HKDF(pair, "OMCP-tag"||epoch)` (current + previous epoch are wanted)
- `msg_key = HKDF(pair, "OMCP-mesh-msg-v1"||sender_leaf||envelope_id)`, nonce 0,
  AAD = version, suite, id, created, flags, header_aad. Payload: one Secret frame, never compressed.

## Apps
- Android/iOS: beacon in scan response (Android) / in-link only (iOS),
  max 4 links (2 on low battery), duty cycle: charging 5 s tick; battery >=20 %
  15 s; low/power-saver 30 s scan every 5 min, 60 s tick.
- Settings › Relay: on/off (default ON — placeholder) and carry cap.

## Known spec / design gaps
1. No MLS for 1:1: pairwise static DH has no forward secrecy and no PQ on mesh hops.
2. TLV 0x80 and mesh frame types 0x44/0x64/0x45 and PDU 0x03 are not in the §21.8/§21.9 registries.
3. Relay default ON and the cap presets are placeholders; there's no design mockup for relay settings.
4. Relaying only happens over contact links; strangers cannot relay.
5. No delivery ack: sender status stays "queued" after a mesh-only delivery.
6. Files/media and control messages stay direct-only.
7. Device-key rotation invalidates in-flight envelopes.
8. Platforms must serialize core calls per link.
