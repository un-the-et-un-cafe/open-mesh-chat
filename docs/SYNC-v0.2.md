# Scalable sync (PROTOCOL §22) — implementation notes, M1 conflicts, stubs

Status: **experimental (v0.2)**. Code: `crates/omcp-core/src/sync/`,
simulator: `crates/omcp-sim/` (`cargo run -p omcp-sim --release -- <seed>`).

## Compression side-channel (CRIME/BREACH) — read this first

Compressing before encrypting makes ciphertext length depend on content. If
attacker-chosen text shares a compression context with a secret, observed
lengths leak the secret. §22.3 makes compression a MAY and the following
MUSTs are enforced in `sync::compress` (each has a unit test):

| MUST | Enforcement |
|---|---|
| Pad every ciphertext to 256 B / 1 KiB / 4 KiB / 16 KiB, then multiples of 16 KiB | `seal_padded` always emits a bucket length; `open_padded` / `decode_padded` / `check_bucket_len` reject anything else and non-zero padding |
| Never compress secrets, keys, tokens, recovery data, control messages | `FrameClass::{Secret, Control, Binary}` are never compressed; the decoder rejects a compressed frame of those classes |
| Quoted/forwarded text in a separate frame or not compressed | `FrameClass::Quoted`; each frame gets a fresh zstd context; quoted compression is off unless `compress_quoted` |
| Static, public dictionaries only | `OMCP_DICT_V1` is a `const`; no API accepts a dictionary; decoder rejects unknown dictionary ids |
| No compression for burn or Nearby messages | `SealOptions { burn, nearby }` disable it |

Residual risk: padding only hides length *within* a bucket. A message that is
near a bucket edge can still cross buckets depending on compressibility.
That is accepted by §22.3 and is why attacker-influenced content is isolated.

## M1 conflicts — resolved by §21.8 / §22.7 (v0.2.1) and implemented

| §22.7 item | Implementation |
|---|---|
| (1) Carry tags, distance vector, ≤ 1 h, ≤ 64, want > lowest carry d > spray | `routing::{CarryTable, neighbour_distance, plan_forward}`, `tag::carry_tag(tag, d)` entries in the same padded digest |
| (2) Hello 1 s → 60 s, delta digests + 5-min full refresh | `hello::{HelloTimer, DigestAdvertiser}`, `cuckoo::{DigestDelta, insert_keep_padding, remove_keep_padding}` |
| (3) Rendezvous label `HKDF-SHA-384(tag_secret, "OMCP-rdv" ‖ epoch, 32)` | `tag::rendezvous_label`, used by `budget::choose_route` |
| (4) TLV `0x54` per §21.8 registry | `tag::{tlv_class, tag_from_aad}` (rejects types outside the registry, wrong length, duplicates) |
| (5) 1:1 = 2-member MLS; secret = `MLS-Exporter("OMCP-tag", epoch, 32)` | `OmcpMlsMember::tag_exporter`, `impl TagSecretSource`, `tag::chat_tag`; tested with a real 2-member OpenMLS group |
| (6) Locator `BLAKE3-keyed(media_key, "OMCP-loc" ‖ content_hash)` | `media::locator` |
| (7) 64 KiB chunks everywhere | `files::DEFAULT_CHUNK_PLAINTEXT_LEN = 64 KiB`; media uses it |
| (8) SY6 thresholds normative | SY6 test: 10⁶ pairs, advantage < 0.005, \|z\| < 5 |

Unchanged from before: gateway stays opaque-only; the 500 MB/day gateway cap
(Internet bytes) and the §22.4 relay budgets (mesh storage/forwarding) both
apply; padding fits the 1 MiB cap.

## §22.7(9)–(11) (accepted proposals) — implemented

| Item | Implementation |
|---|---|
| (9) Sub-tags `BLAKE2b-64(exporter, "OMCP-sub" ‖ leaf_u32 ‖ epoch_u64)`, TLV `0x56` (8·n), chats ≤ 8 members; larger groups chat tag + Plumtree | `tag::{subtag, chat_subtag, put_subtags_in_aad, parse_routing_aad, uses_subtags}`; MLS-backed via `TagSecretSource` |
| (9) Forwarding want > lowest carry > scoped flood within TTL, no spray | `routing::plan_forward(tags, ttl_hops, …)`; spray-and-wait removed |
| (10) Per-message metadata ≤ max(256 B, 10 % ct); beacons only in idle budget | SY1 `per_message_metadata`, SY4 `metadata_per_delivered` (incl. carry deltas) |
| (11) Full digest only to new neighbours / `DIGEST_REQ` / 30-min refresh; hello ≤ 60 s (≤ 32 nbrs) or 120 s | `hello::{DigestAdvertiser, needs_digest_req, hello_max_ms}`; frames: `hello::{LinkControl, digest_id32}` |

**§22.7(12) / §21.9 (normative):** `HELLO` (0x48) and `DIGEST_REQ` (0x52)
are 6-byte link-control frames `type | flags | digest_id u32-be`; flags bit0
`HAS_DELTA`, bits 1–7 reserved and rejected if set; `DIGEST_REQ` id 0 = full.
`digest_id` = unkeyed BLAKE2b with 32-bit output (nn = 4) over the canonical
want-digest encoding, big-endian u32; 0 maps to 1.
Idle at 32 neighbours ≈ 0.83 MB/day.

## Stubs / not done

- No real transport wiring: `plan_forward`, `TokenBucket`, `RelayStore`,
  `ReceiptBatcher`, `Plumtree` are state machines not yet hooked into
  `ble::gossip` / `gateway::Gateway`.
- Rendezvous server does not yet authorise tag-mailbox labels (tokens are
  tied to `blinded_label(signer_pk, epoch)`).
- `tag_exporter` agrees only when members share the MLS epoch (TODO in
  `mls/group.rs`: also try the previous MLS epoch during commit races).
- Carry tags / scoped flood are not yet wired into `ble::gossip`.
- Reconciliation runs over one combined set of shared-tag items; per-tag
  scoping/handshake message and BLE 3-round-trip cap are not enforced.
- Media: no "nearest holder" discovery; no bao-style verified streaming.
- Delta digests are not yet framed on BLE (no message type assigned).
- HyperLogLog sketches are not exchanged on the wire yet.
- `OMCP_DICT_V1` is a placeholder word list, not a trained dictionary.
- SY1 models spray/delivery costs (documented in `sy1.rs`) rather than
  executing every hop; SY3 does not execute a full mesh, only the
  watermark catch-up exchange.

## Mesh relay

Multi-hop store-and-forward relay (PROTOCOL §22) is described in [docs/MESH_RELAY.md](MESH_RELAY.md).
