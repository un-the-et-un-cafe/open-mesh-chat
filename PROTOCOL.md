# Open Mesh Chat Protocol (OMCP) — Specification v0.1

**Status:** Draft for implementation  
**Date:** 2026-10-23  
**Audience:** Protocol implementers, mobile clients, gateway operators  
**Licence:** Proposed CC0 / Apache-2.0 dual for code and protocol text (final choice with project owners)

This document defines an open chat and data-sharing protocol that runs primarily over a Bluetooth Low Energy (BLE) mesh of peer devices. Every peer is also a potential Internet gateway for other peers. The network must remain usable during Internet outages and must be extremely secure by default. The design is versioned and extensible so later work can add transports, crypto suites, and application features without breaking existing peers.

---

## 1. Goals and non-goals

### 1.1 Goals

1. **Local-first mesh:** Peers exchange messages and shared objects over BLE mesh without requiring the public Internet.
2. **Optional Internet bridging:** Any peer with Internet access MAY act as a gateway, ferrying traffic for nearby BLE peers that lack Internet.
3. **Outage resilience:** Chat, presence, and previously synced data remain available when the Internet is down.
4. **Extreme security:** End-to-end encryption (E2EE) for all user content; forward secrecy; post-compromise security; authenticated peers; no cleartext content on gateways or relays.
5. **Open and evolvable:** Stable wire formats, explicit version negotiation, extension points, and a public change process.
6. **Cross-platform clients:** First-class Android and iOS apps implementing the same protocol.

### 1.2 Non-goals (v0.1)

- Replacing the global Internet or providing general-purpose IP routing.
- Guaranteeing delivery across arbitrary geographic distance without gateways or store-and-forward peers.
- Perfect anonymity against a global passive adversary (see threat model).
- Cryptocurrency, tokens, or payment rails inside the protocol.

### 1.3 Rules that do not change

- User message content is never readable by relays, gateways, or application servers.
- A peer MUST NOT be required to connect to any vendor cloud to chat on the local mesh.
- Cryptographic agility is mandatory; hard-coded single algorithms without negotiation are forbidden after v0.1 bootstrap.
- Protocol changes that break wire compatibility require a major version bump.

---

## 2. Architecture overview

```
┌─────────────┐   BLE mesh    ┌─────────────┐   BLE mesh    ┌─────────────┐
│  Peer A     │◄─────────────►│  Peer B     │◄─────────────►│  Peer C     │
│ (phone)     │               │ (gateway)   │               │ (phone)     │
└─────────────┘               └──────┬──────┘               └─────────────┘
                                     │ Internet (optional)
                                     ▼
                              ┌─────────────┐
                              │  Rendezvous │
                              │  / store-   │
                              │  and-forward│
                              │  (optional) │
                              └─────────────┘
```

- **Peer / Node:** Any device running OMCP. All peers are equal in the mesh; gateway capability is a role, not a privilege over content.
- **Mesh link:** BLE connections and advertisements used for discovery, routing hints, and frame transport.
- **Gateway role:** A peer that has Internet and volunteers to forward *encrypted* OMCP envelopes to other OMCP endpoints (other gateways, rendezvous services, or remote peers).
- **Rendezvous service (optional):** A minimal Internet service that stores opaque, encrypted envelopes addressed to peer public keys for later pickup. It NEVER sees plaintext.

---

## 3. Identities and keys

### 3.1 Peer identity

Each peer generates a long-term **Identity Key Pair**:

| Field | v0.1 algorithm | Notes |
|-------|----------------|-------|
| Identity signing | Ed25519 | Long-term; used to sign Identity Certificates and device attestations |
| Identity PQ hybrid (optional, recommended) | Ed25519 + ML-DSA-65 (Dilithium3) | Dual signatures during transition |
| Identity encryption / KEM long-term | X25519 + ML-KEM-768 hybrid | Used for sealed addressing and initial contact |

**PeerID** = `BLAKE2b-256(identity_signing_public_key)` (32 bytes; v0.1.4a, replaces the earlier "multihash" wording), encoded as `omcp1:<base32>` for display. Contacts are keyed by PeerID. The X25519 Noise static key is **per device** and certified by the identity key (§3.4); it is not the PeerID.

Peers MAY rotate Identity Keys. Rotation requires signing a **Successor Statement** with the old key and publishing it to contacts over authenticated channels.

### 3.2 Device keys

Each installation holds:

1. **Identity Key** (above)
2. **Device Ephemeral Key** — rotated frequently; used in Noise_XX / Noise_IK handshakes
3. **Group sender keys / MLS epoch secrets** — for multi-party chats (see §6)
4. **Local storage key** — device-bound (Android Keystore / iOS Secure Enclave), wraps the above

### 3.3 Contact and trust

- First contact uses an **out-of-band verification** (QR code with PeerID + short authentication string, or NFC tap).
- Safety number: `SHA3-256(sorted(PeerID_A || PeerID_B))` displayed as 60 decimal digits in 12 groups of 5.
- Trust is local: each peer maintains its own contact book with verification state (`unverified`, `verified`, `revoked`).
- From v0.1.4 the safety number also covers each side's current device key (§3.4); see the reference `safety_number_v2`.

### 3.4 Device certificates and the restore indicator (v0.1.4)

Normative for clients that implement apps/REQUIREMENTS.md "Restore indicator (security)" (RS1) and "Re-verification after restore" (RS2). Reference: `omcp-core::device`.

- **Identity signing key.** Ed25519, derived from the 24-word master seed: `HKDF-SHA-384(salt = "OMCP-Identity-Seed-v1", ikm = seed, info = "ed25519")`. It survives a restore.
- **Device key.** Each installation generates a random Ed25519 device key. A restore from the 24 words (or from a cloud backup) MUST generate a new one; it is never restored.
- **Certificate v2** (209 bytes, big-endian, v0.1.4a): `ver u8 = 2 | peer_id [32] | identity_sign_pub [32] | device_pub [32] | dh_pub [32] | created_ms u64 | restored_at_ms u64 (0 = not a restore) | sig [64]`, where `sig = Ed25519(identity_sign, "OMCP-DeviceCert-v2" || all preceding fields)`. `peer_id` MUST equal `BLAKE2b-256(identity_sign_pub)`; `dh_pub` is the device's X25519 Noise static key. (v1, 177 bytes without `dh_pub`, is no longer sent.)
- **Transport and device binding.** Every link's Noise handshake payload (initiator msg 1, responder msg 2) is `"omcp-ffi-v1" || 0x01 || cert || 0x02 || bind`, with `bind = Ed25519(device_key, "OMCP-DeviceBind-v1" || h)` where `h` is the Noise handshake hash at the moment the payload is encrypted. The receiver MUST check the certificate, `cert.dh_pub == remote static key`, and `bind` over its own `h`, and MUST fail the handshake if the certificate or binding is missing or invalid, or the device key is superseded. A replayed old certificate therefore can't be used to suppress the restore notice. Unknown extension tags MUST be ignored.
- **Dialing.** The initiator dials the contact's certified `dh_pub`. Advertisements carry the PeerID prefix (legacy installs: their X25519 prefix). A restored device has a new `dh_pub`, so its contacts can reach it only after it dials them or they scan its new QR.
- **Migration from v0.1.4.** Earlier builds keyed contacts by the seed-derived X25519 key. Such installs keep that X25519 as their device `dh_pub`; legacy contacts (key = X25519) are re-keyed to the PeerID, in the store and in memory, on the first verified handshake. Identity backups now carry each contact's `dh`; older backups without it restore legacy-keyed contacts that migrate the same way.
- **Receiver rules** (the restored device can't suppress them):
  1. The first `identity_sign_pub` seen for a contact over the authenticated channel is pinned; a different one MUST be rejected.
  2. A certificate whose `device_pub` differs from the pinned one, one with `restored_at_ms ≠ 0` for a new device, or a missing certificate after one was pinned, MUST raise the in-chat system notice ("<name> restored Hopla on a new device · <date>"), show a "Restored" badge for 7 days from `restored_at`, clear the contact's "Verified" state ("Restored, not verified"), and mark the previous device key **superseded**.
  3. A superseded device key MUST NOT be accepted again (the link is refused).
- **Safety number v2.** `BLAKE2b-512("OMCP-Safety-v2" || lo(peer || device) || hi(peer || device))`, iterated and grouped like v1, ordered by the 64-byte concatenation. When either device key is unknown (older peer) clients fall back to v1.
- **QR v3** (v0.1.4a). `omcp:3:<peer_id hex>:<identity_sign_pub hex>:<device_pub hex>:<dh_pub hex>:<display name>`; `peer_id` MUST equal `BLAKE2b-256(identity_sign_pub)`. Verifying by QR MUST fail when the code's device key isn't the contact's current one (e.g. a QR shown before a restore). `omcp:2:`/`omcp:1:` codes are still accepted as legacy contacts.

---

## 4. Threat model

### 4.1 Assets

- Message and file content
- Membership of private groups
- Contact graphs (as far as practical)
- Gateway/relay metadata minimization

### 4.2 Adversaries

| Adversary | Assumed capability | Protocol response |
|-----------|--------------------|-------------------|
| Local radio observer | Captures BLE frames | Encrypted frames; frequent key rotation; padding |
| Malicious peer / gateway | Relays or drops traffic | E2EE; authenticated routing; no cleartext on gateway |
| Compromised device | Reads local storage | Secure Enclave / Keystore; optional passphrase; remote wipe via tombstone (best-effort) |
| Nation-state with harvest-now-decrypt-later | Stores ciphertext for decades | Hybrid PQ KEMs and signatures from v0.1 |
| Network partition / Internet outage | No WAN | Local mesh continues; store-and-forward when links return |

### 4.3 Explicit non-protections (v0.1)

- Traffic analysis of who talks to whom on a densely monitored mesh
- Physical seizure of an unlocked device
- Compelled disclosure of passphrases

---

## 5. Cryptographic suite (v0.1)

All algorithms are named in the handshake and MAY be upgraded via negotiation.

### 5.1 Suite `OMCP-2026-A` (mandatory)

| Purpose | Algorithm |
|---------|-----------|
| Handshake | Noise_IK_25519_ChaChaPoly_BLAKE2b with ML-KEM-768 hybrid (Noise Hybrid Draft style): shared secret = `HKDF(X25519_ss \|\| ML-KEM_ss)` |
| AEAD | ChaCha20-Poly1305 |
| Hash / KDF | BLAKE3 (keyed mode) and HKDF-SHA-384 for Noise compatibility paths |
| Signatures | Ed25519; dual-sign with ML-DSA-65 when both peers advertise PQ |
| Group messaging | Messaging Layer Security (MLS) RFC 9420, ciphersuite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` **plus** PQ hybrid extension (see §6.2) |
| Sealed sender / addressing | HPKE (RFC 9180) with DHKEM(X25519) + ML-KEM-768 hybrid, HKDF-SHA384, ChaCha20-Poly1305 |
| Commitment / transcript | BLAKE3 |

### 5.2 Crypto agility

- Frames carry `suite_id` (uint16). Unknown suites → peer responds with `UNSUPPORTED_SUITE` and its supported list.
- Deprecation: suites enter `deprecated` then `forbidden` via signed **Suite Bulletin** documents distributed in-mesh and via gateways.

### 5.3 Randomness and nonces

- CSPRNG from platform (SecureRandom / SecRandomCopyBytes).
- 96-bit nonces for ChaCha20-Poly1305; never reused with the same key. Prefer counter||random construction per session.

---

## 6. Messaging model

### 6.1 One-to-one

1. Peers complete Noise hybrid handshake → transport keys.
2. Application messages are AEAD-encrypted frames with associated data: `suite_id`, `sender_peer_id`, `msg_id`, `timestamp_ms`, `content_type`.
3. Message IDs are 128-bit random; duplicates are ignored.
4. Delivery acknowledgements are separate encrypted control frames (not cleartext ACKs on the mesh).

### 6.2 Groups (MLS)

- Each group is an MLS group. Epoch secrets protect application messages.
- **PQ hybrid extension (OMCP-MLS-PQ):** For each epoch, members additionally encapsulate to every other member's ML-KEM public key and mix the shared secrets into the MLS exporter via HKDF. Spec of the extension binary format lives in `extensions/mls-pq.md` (to be developed; wire tag `0xOM01`).
- External joins require an invite authenticated by a current member (Welcome + Verification QR optional).

### 6.3 Content types (application layer)

| Code | Type | Notes |
|------|------|-------|
| 0x01 | `text/plain; charset=utf-8` | Default chat |
| 0x02 | `text/markdown` | Optional |
| 0x10 | `application/omcp-reaction` | Unicode + target msg_id |
| 0x20 | `application/omcp-file-meta` | Name, size, MIME, content hash |
| 0x21 | Chunked encrypted blob | See §7 |
| 0x30 | `application/omcp-presence` | Optional; encrypted to contacts only |
| 0x40 | Control (ACK, receipt, typing) | E2EE |

### 6.4 Sync and causality

- Each peer maintains a **per-conversation DAG** of message IDs with parent pointers (1–N parents) for offline concurrent edits/sends.
- Clients display a linearization by `( Lamport-like logical clock, PeerID )` with UI markers for branches when needed.
- Sync protocol: `HAVE` / `WANT` / `GIVE` over encrypted channels for missing DAG nodes and blobs.

---

## 7. Data sharing (files and objects)

1. Sender computes `content_hash = BLAKE3(plaintext)`.
2. Encrypts file with a random **Content Key** (256-bit); wraps Content Key to each recipient via HPKE hybrid.
3. Splits ciphertext into chunks (default 64 KiB; see §22.7(7)) with per-chunk AEAD; advertises `file-meta` then serves chunks on request.
4. Mesh peers MAY cache encrypted chunks (LRU). Gateways MAY cache for Internet recipients. Caches never hold Content Keys unless they are intended recipients.
5. Integrity: recipient verifies `BLAKE3(plaintext)` after decrypt.

### 7.1 Media re-fetch by locator and hash (v0.1.4)

After a chat-backup restore (apps/REQUIREMENTS.md "Encrypted chat backup", CB3), media come back as placeholders that keep the **locator** (the 16-byte `transfer_id` of the original offer, also the message id) and the `content_hash`. Three encrypted `InnerV1` control messages (outer content type `0x02` like every inner message, never with a `burn` header), each `type u8 | locator [16] | content_hash [32]`:

| Type | Name | Meaning |
|------|------|---------|
| `0x31` | `MEDIA_REQUEST` | Sent automatically when a link to the partner comes up, or on tap. |
| `0x32` | `MEDIA_UNAVAILABLE` | Reply: "I no longer have it" (also sent for unknown or burn items). |
| `0x33` | `MEDIA_RESEND_ASK` | User-confirmed "Ask to resend"; the partner sees a prompt. v0.1.4b: MAY be followed by an optional UTF-8 note (≤ 280 bytes) — `type | locator | content_hash | note`. It is the **only** message sent; clients MUST NOT send a separate text. |

The holder answers a request by re-sending the file as a normal §7 transfer under the **same** `transfer_id`. The requester MUST verify `BLAKE3(plaintext) == content_hash` from its placeholder before replacing it, and treats a mismatch as an integrity failure. Requests carry nothing beyond what the original offer already told the partner (plus the user's optional note on `0x33`). Over a gateway, re-downloads are Wi-Fi only by default.

**Kept sent media (v0.1.4b).** The sender keeps a local copy of every non-burn file it sent, sealed in its encrypted store like every other row, for 90 days or up to 2 GB by default (oldest evicted first; both limits user-configurable). Burn-after-read and view-once media MUST NOT be kept or served. A `MEDIA_REQUEST` for a kept item is answered from the copy automatically, after checking its hash; otherwise the app is asked, and failing that `MEDIA_UNAVAILABLE` is sent.

---

## 8. BLE mesh transport

### 8.1 Roles and GATT

- Service UUID (v0.1): `a0mc0001-0mcp-4c01-9e00-000000000001` (placeholder; finalize before ship).
- Characteristics: `Inbox` (write), `Outbox` (notify), `Control` (read/write).
- MTU: negotiate maximum; default fragment size 512 bytes application payload before link AEAD.

### 8.2 Discovery

- BLE advertisements include: protocol magic `OMCP`, version, truncated PeerID (first 6 bytes), capabilities flags (`GATEWAY`, `STORE_FORWARD`, `MLS`, `PQ`).
- Full PeerID and signed capability statement exchanged after connection.

### 8.3 Routing on the mesh

- **v0.1 routing:** epidemic / gossip with TTL and seen-cache (message_id bloom + LRU). Suitable for small/medium meshes (tens of peers).
- **v0.2 target:** distance-vector or source-routed paths with authenticated path metrics (to be specified).
- Loop prevention: `seen` set per `envelope_id` with TTL expiry (default 10 minutes; configurable).

### 8.4 Store-and-forward

- Peers MAY retain encrypted envelopes addressed to known contacts for up to `retention_hours` (default 72; user-configurable; max 168).
- On contact reappearance, pending envelopes are offered via `HAVE`.

---

## 9. Internet gateway role

### 9.1 Behaviour

1. Gateway peer maintains TLS 1.3 (or QUIC) sessions to configured rendezvous endpoints and/or remote OMCP peers.
2. It forwards **opaque envelopes** only: `{ destination_peer_id or sealed_address, suite_id, ciphertext, expiry }`.
3. It MUST NOT attempt decryption of user content. Implementation MUST use separate processes/sandboxes where OS allows.
4. Rate limits and battery policies are local; protocol defines `GATEWAY_BUSY` and `GATEWAY_OFFERING` control messages.

### 9.2 Addressing over Internet

- Preferred: **sealed sender** to PeerID via HPKE so the gateway learns only a blinded destination label.
- Optional account-free rendezvous: `POST /v1/enqueue` and `POST /v1/fetch` with authentication by short-lived signed tokens from the Peer Identity Key.

### 9.3 Outage behaviour

- When Internet is lost, gateway simply stops bridging; local mesh continues.
- When Internet returns, gateway drains its outbound queue and fetches pending envelopes for local peers that opted in.

### 9.4 Trust in gateways

- Using a gateway does not grant it trust over content.
- Users MAY pin allowed gateway PeerIDs for their traffic.
- Default: any mesh neighbour advertising `GATEWAY` may be used for *transport*; clients SHOULD prefer gateways whose PeerID is a verified contact when available.

---

## 10. Wire format

### 10.1 Envelope (canonical)

```
EnvelopeV1 {
  magic:           b"OMCP"
  version:         u16   // 1
  suite_id:        u16
  envelope_id:     [16]u8
  flags:           u16   // RELAY_OK, STORE_OK, PRIORITY, ...
  ttl_hops:        u8
  created_ms:      u64   // sender local clock; for ordering hints only
  header_aad_len:  u16
  header_aad:      [..]  // cleartext associated data (no secrets)
  ciphertext_len:  u32
  ciphertext:      [..]  // AEAD ciphertext + tag
}
```

`header_aad` MAY include destination hint (truncated or sealed), content_type class, and size — never plaintext bodies.

### 10.2 Framing on BLE

- Link layer: length-prefixed fragments with `frag_id`, `frag_index`, `frag_count`, per-link session AEAD (derived from Noise).
- Reassembly timeout: 30 seconds; incomplete messages dropped.

### 10.3 Version negotiation

- First control frame after handshake: `Hello { versions[], suites[], features[] }`.
- Intersection chosen by initiator using preference order published in this spec for each major version.

---

## 11. Security requirements (normative)

1. **E2EE default:** All user content types in §6.3 MUST be E2EE. Cleartext chat modes are forbidden.
2. **Forward secrecy:** Handshake and MLS epoch rotation MUST provide FS. Session keys wiped after use.
3. **Post-compromise security:** MLS + continuous key refresh; one-to-one sessions rekey at least every 7 days or 100 messages.
4. **PQ readiness:** Suite `OMCP-2026-A` hybrid KEMs REQUIRED for Internet-facing paths; REQUIRED for mesh after both peers advertise `PQ`.
5. **Auth:** No unsigned identity claims. Capability statements are signed.
6. **Replay:** `envelope_id` + timestamp window (±10 minutes skewed tolerance with logical clocks) + AEAD.
7. **Padding:** Optional traffic padding profiles (`off`, `low`, `medium`) to mitigate size side channels on gateways.
8. **Screenshots / notifications:** Clients SHOULD show sender and preview by default and MUST offer a "Private notifications" mode that shows only a generic "New message" (no sender, no content). Payloads are decrypted on-device only; gateways and push relays never see plaintext.
9. **Backups:** Encrypted backups only; recovery key is 24-word BIP-39-like phrase or platform secure recovery; cloud backup is optional and E2EE (client-side encrypted before upload; see apps/REQUIREMENTS.md "Onboarding & recovery backup").

### 11.1 Encrypted chat backup (v0.1.4)

Reference: `omcp-core::chat_backup`. Key: `HKDF-SHA-256(salt = none, ikm = 24-word master seed, info = "hopla/chat-backup/v1")`. Blob: `"HCB1" | kind u8 (0 snapshot, 1 log, 2 media) | seq u64 BE | nonce [24] | XChaCha20-Poly1305(ct)` with AAD `"hopla/chat-backup/v1" | kind | seq` (media: `| content_hash`). The plaintext is `len u32 BE | CBOR | zero padding` so the blob length is a multiple of 64 KiB. Logs carry changed and deleted messages since the previous upload; a snapshot (at most weekly) carries everything, after which lower `seq` blobs MAY be deleted. Burn-after-read and view-once content MUST NOT be included. Media blobs are only written when the user enables media backup; their names are a keyed hash of the content hash.

**iCloud records (v0.1.4a).** CloudKit private database, custom zone `HoplaBackupV1`. Record types: `ChatLogSegment` (name `seg-<seq, lowercase hex>`), `ChatSnapshot` (`snap-<YYYYWW>`, ISO-8601 week of upload, UTC; a newer snapshot in the same week replaces it), `MediaBlob` (`media-<hex BLAKE2b-256 of the blob>`) and one `BackupManifest` (`manifest`). Every record has exactly `blob` (CKAsset), `seq` (Int64) and `created` (ms, Int64); no plaintext fields. The manifest's blob is the index from the core blob names (sequence numbers and keyed hashes, already opaque) to record names. Google Drive keeps the core names in appDataFolder.

---

## 12. Privacy

- Contact discovery is local/QR/NFC first. No compulsory central directory.
- Optional user-run directory: stores only PeerID + signed reachability blobs.
- Metadata minimization on gateways: sealed addressing, short retention, no logs of payload hashes beyond operational minima (implementations SHOULD document logging).

---

## 13. Smartphone applications

### 13.1 Scope

Native (or high-quality cross-platform) apps for **Android 12+** and **iOS 17+** implementing OMCP v0.1 as both mesh peers and optional gateways.

### 13.2 Shared product requirements

| Area | Requirement |
|------|-------------|
| Onboarding | Generate identity; show recovery phrase once; QR for contact add |
| Chats | 1:1 and groups; offline queue; clear “mesh / gateway / mixed” path indicators |
| Files | Share images, PDFs, up to configurable size (default 50 MB); progress UI |
| Gateway toggle | User can enable “Help nearby peers reach the Internet” with battery warning |
| Security centre | Safety numbers, verified badges, session reset, blocked peers |
| Accessibility | Dynamic type, screen readers, sufficient contrast |
| Localisation | English and French at minimum for first release (project may add more) |
| Open source | Client source published; reproducible builds targeted |

### 13.3 Android-specific

- Foreground service for mesh participation with persistent notification explaining BLE use.
- Use Android BLE APIs; request `BLUETOOTH_SCAN` / `CONNECT` / `ADVERTISE` and location only as required by OS version.
- Prefer Kotlin; protocol core in shared Rust (see §14) via JNI.
- WorkManager for deferred gateway fetch when Internet returns.
- Keystore + StrongBox when available for identity keys.

### 13.4 iOS-specific

- Background modes: Bluetooth LE central/peripheral as allowed; document App Store justification.
- MultipeerConnectivity MAY be researched as an *additional* local transport in v0.2; v0.1 BLE remains mandatory for cross-Android interop.
- SwiftUI client; protocol core in shared Rust via FFI.
- Secure Enclave for key wrapping; Keychain accessibility `afterFirstUnlock` carefully reviewed for mesh background needs.
- Notify users clearly when background mesh is limited by iOS.

### 13.5 UX copy principles

- Plain language for security states (“Verified contact”, “Waiting for mesh”, “Sent via neighbour’s Internet”).
- Never claim “unhackable” or “military grade”; state concrete properties (E2EE, PQ hybrid, open source).

### 13.6 Testing acceptance (apps)

1. Two phones, Airplane Mode + BLE on: exchange text and a 2 MB file within a 10 m indoor range.
2. Three phones, only middle one online: outer peers exchange messages via gateway within 60 seconds.
3. Kill Internet on gateway mid-chat: mesh path continues for local peers.
4. Safety number verification prevents MITM in a staged adversarial BLE peer test.
5. Uninstall/reinstall with recovery phrase restores identity and contact verification state (from encrypted backup).

---

## 14. Reference implementation plan

### 14.1 Repository layout (suggested)

```
omcp/
  PROTOCOL.md          # this document
  extensions/          # MLS-PQ and future extensions
  crates/
    omcp-core/         # Rust: crypto, framing, MLS, sync
    omcp-ble/          # BLE transport adapters
  apps/
    android/
    ios/
  rendezvous/          # optional minimal server
  docs/
  CHANGELOG.md
```

### 14.2 Language choice

- **Protocol core in Rust** (auditable, shared on Android/iOS).
- **Apps:** Kotlin + Swift UI layers.

### 14.3 Milestones

| Milestone | Deliverable |
|-----------|-------------|
| M0 | This spec merged; CI skeleton via Pipely |
| M1 | `omcp-core` Noise hybrid handshake + framing unit tests |
| M2 | BLE transport loopback + two-device text chat |
| M3 | MLS groups + file chunks |
| M4 | Gateway role + optional rendezvous |
| M5 | Android beta |
| M6 | iOS beta |
| M7 | External crypto review + threat-model update |

---

## 15. Compliance, export, and ethics

- Implementations MUST follow applicable cryptography export rules in their distribution regions.
- No hidden law-enforcement plaintext access. Any future lawful intercept feature would require a protocol major version and user-visible distrust warnings — not planned.
- Mesh networking in some jurisdictions may face radio regulation constraints; apps MUST use BLE only within legal power/duty-cycle limits.

---

## 16. Extension process

1. Draft extension in `extensions/NNNN-short-name.md` with motivation, wire tags, security impact.
2. Interop test vectors required before merge.
3. Feature bit advertised in `Hello`.
4. Two independent implementations (or one impl + comprehensive test vectors) before marking extension `stable`.

---

## 17. Open questions (track in issues)

1. Final BLE Service UUID registration / company ID.
2. Exact Noise hybrid construction reference (IETF draft pinning).
3. Whether iOS background BLE limits require a companion “always-on” gateway device profile.
4. Default retention and legal constraints for store-and-forward in EU (GDPR minimization).
5. Optional Tor/onion transport for rendezvous as v0.3 privacy upgrade.

---

## 18. Glossary

| Term | Meaning |
|------|---------|
| Peer | Device running OMCP |
| Gateway | Peer offering Internet bridging of opaque envelopes |
| Envelope | Versioned encrypted unit of transport |
| Suite | Named crypto algorithm set |
| MLS | Messaging Layer Security (group E2EE) |
| Sealed address | HPKE-encrypted destination so relays learn little |

---

## 19. Document control

| Version | Date | Notes |
|---------|------|-------|
| 0.1 | 2026-10-23 | Initial specification for Proggy / Pipely handoff |
| 0.1.3 | 2026-10-24 | §19.1 burn after read (`burn` inner header, `OPENED` receipt) |
| 0.1.4 | 2026-10-24 | §3.4 device certificates, restore indicator, safety number v2, QR v2; §7.1 media re-fetch controls `0x31`–`0x33`; §11.1 chat backup format |
| 0.1.4b | 2026-10-24 | §7.1 one-message Ask to resend with optional note; kept sent media served to re-requests (RS3) |
| 0.1.4a | 2026-10-24 | §3.1 PeerID = BLAKE2b-256(identity key), per-device X25519; §3.4 certificate v2, handshake device binding, QR v3, migration; §11.1 iCloud records |

**Next editors:** Implementers SHOULD open PRs against this file for any wire-format change. Semantic versioning: breaking wire changes → 1.0 major after first production interop; until then 0.x may break with explicit changelog entries.

### 19.1 Burn after read: inner `burn` header (v0.1.3)

This section is normative for clients that implement burn after read (apps/REQUIREMENTS.md "Burn after read (v0.1.3)"). Reference: `omcp-core::burn`.

**Inner header (encrypted).** The AEAD plaintext of an application message starts with an inner header tag:

```
InnerV1 {
  tag:   u8          // 0x00 = no inner header, 0x01 = burn
  burn:  BurnHeader  // present iff tag == 0x01
  inner_ct: u8       // 0x01 text (UTF-8), 0x02 media (mime_len u8 | mime | bytes), 0x30 OPENED control
  body:  [..]
}
BurnHeader {         // 9 bytes, big-endian
  mode:       u8     // 0 = on_open (view once), 1 = timer
  secs:       u32    // on_open: 0; timer: one of 10, 60, 3600
  unread_ttl: u32    // seconds; one of 3600 (1 h), 86400 (24 h), 604800 (7 d)
}
```

The outer `header_aad` content type for every `InnerV1` message is `0x02` (inner v1), whether it's text, media, burn or control, so relays can't tell them apart. (`0x01` plain text from M2 is still accepted on receive.) `OPENED` MUST NOT carry a burn header, and receivers MUST ignore an `OPENED` that doesn't come from the message's recipient. The `burn` header is inside the ciphertext, so relays, gateways and store-and-forward nodes MUST NOT be able to tell that a message is ephemeral. Receivers MUST reject headers with values outside the sets above.

**Alignment with the envelope expiry.** The sender MUST set the outer expiry (`expiry` / `expires_at` of the opaque envelope, §9 item 2, `OpaqueEnvelope.expiry_ms` in the reference) to `sent_ms + unread_ttl × 1000`. Relays and store-and-forward nodes (§8.4) MUST drop the opaque envelope at or after `expires_at` and MUST NOT store an envelope that has already expired. The receiver MUST delete an unopened burn message at the same instant. `expires_at` is the only outer signal; every message type SHOULD carry one, so its presence doesn't reveal burn.

**Lifecycle.**
1. *on_open*: content is shown once; it is deleted when the viewer closes. *timer*: deletion happens `secs` after opening (or on close, whichever comes first).
2. **Single-use media key.** For burn media, the Content Key (§7) MUST be used for exactly one decryption. The receiver deletes it once the media is viewed, and the encrypted blob is deleted from the relay and the local cache.
3. **`OPENED` receipt.** On first open, the receiver sends an encrypted control message `OPENED { type = 0x30 u8, msg_id [16]u8, opened_ms u64 BE }`. When the sender receives it, it MUST delete its local copy of the media key, and it SHOULD delete its own copy of the content when the timer runs out.
4. **Crypto-shredding.** Burnable content at rest MUST be encrypted under a per-message key that is held only in memory (or in a separate encrypted table), and deletion MUST zeroize that key.
5. **Groups.** Each member burns independently after they open the message. Burn MUST NOT be offered in groups or channels with more than 256 members.

Burnable content MUST be excluded from notification previews, backups, search indexes and share sheets.

---

## 20. Application features — communities and livestreams (v0.2 target)

Inspired by familiar messenger patterns (channels, communities, live video), implemented without weakening E2EE or mesh-first rules. v0.1 MVP stays 1:1 + small MLS groups; this section defines the next product layer so Proggy can plan capacity and Pipely can stage releases.

### 20.1 Communities

A **Community** is a signed container of related conversations:

| Object | E2EE | Mesh notes |
|--------|------|------------|
| Community roster + roles | Encrypted to members; public communities MAY publish a cleartext name/description only | Roster sync via MLS-like epoch or per-community group |
| Announcement channel | E2EE to members; optional sealed public preview for public communities | Large fan-out via gateway + chunked store-and-forward |
| Topic channels / forums | Each topic is its own MLS group (or subgroup) | Prefer lazy join; invite links are signed capability tokens |
| Voice chat rooms (optional later) | Separate media SFrame/MLS keys | BLE unsuitable for many-way audio; Internet or Wi‑Fi Direct required |

**Roles:** `owner`, `admin`, `moderator`, `member`, `guest`. Capability tokens are signed by the community identity key. Moderation actions are control frames inside the encrypted community; there is no plaintext moderation API on relays.

**Discovery:** Invite link / QR first. Optional public index stores only `{ community_id, name, description, member_count_bucket, join_policy }` — never message content.

### 20.2 Channels (broadcast)

- One-to-many with admin-controlled send rights.
- Recipients verify admin signatures on posts.
- Comments/reactions are separate E2EE threads keyed per channel epoch.
- Forwarding preserves original signature when the sender allows “signed forwards”.

### 20.3 Livestreams

Livestreams NEED a high-bandwidth path. On pure BLE mesh they are **not** supported beyond a low-bitrate status beacon.

| Mode | Transport | Security |
|------|-----------|----------|
| `mesh-beacon` | BLE | Encrypted title/status to community; no video |
| `lan-relay` | Wi‑Fi / local IP discovered via OMCP | SFrame (RFC 9605) or Insertable Streams keys distributed via MLS exporter |
| `internet-sfu` | Gateway or user-chosen SFU | E2EE with SFU seeing only encrypted media; keys never on SFU |

**Normative:** Livestream media keys are derived from the community/channel MLS epoch exporter + per-stream random. SFUs and gateways MUST NOT receive media keys. Viewers who are not community members cannot decrypt.

**Signalling:** WebRTC-style offers/answers carried as OMCP content-type `0x50 application/omcp-rtc-signal` (E2EE). Body: `0x50 ‖ key_epoch (u32 BE) ‖ XChaCha20-Poly1305(stream key, CBOR {stream_id, from, to, kind, body})` with kinds `Offer, Answer, Ice, Bye, Join, Assign`. Viewers send every signal to the host, which keeps the viewer tree and relays only between tree neighbours (`Join` → `Assign` to the parent, `Bye "parked"` when no slot). Video frames are E2EE with a key derived from the stream secret (SFrame base key, RFC 9605 key schedule; docs/LIVE.md).

**Recording:** Local-only or E2EE cloud blob with Content Key wrapped to chosen recipients; default off.

### 20.4 Other Telegram-like surfaces (phased)

| Feature | Phase | Notes |
|---------|-------|-------|
| Usernames / @handles | v0.3 | Optional, user-run name servers; no global mandatory registry |
| Bots / mini-apps | v0.3 | Capability-scoped tokens; no privileged plaintext access |
| Stories / ephemeral posts | v0.2 | TTL enforced client-side; relays drop after expiry |
| Polls, stickers, threaded replies | v0.2 | Content types under `0x60–0x6F` |
| Secret chats (device-pair only) | v0.1 already covers via 1:1 Noise | |
| Payments / gifts | Out of scope | |

### 20.5 App UI additions (Android / iOS)

Update `apps/REQUIREMENTS.md` milestones:

- Community home: channels list, members, roles
- Live tab: start/join stream when `lan-relay` or `internet-sfu` available; clear “video needs Wi‑Fi/Internet” when only BLE is up
- Admin tools: slow mode, ban, delete-for-all (tombstone signed by admin)

### 20.6 Acceptance (communities / live)

1. Create a community of 3 peers on mesh; announcement post delivers to all offline-then-online members within retention window.
2. Start a livestream with Internet SFU; a member on another network watches with E2EE (keys never logged by SFU test harness).
3. BLE-only peers see live *announcement* but not video, with an explicit UI reason.


---

## 21. Normative wire pins (v0.1.1) — answers to M1 open questions

These pins supersede ambiguous wording in §§5, 10, and 17 for implementers. Update this section only via PR; bump the document patch version when changing a pin.

### 21.1 Suite ID

| suite_id | Name | Status |
|----------|------|--------|
| `0x0001` | `OMCP-2026-A` | Mandatory for v0.1 |

Unknown `suite_id` → `UNSUPPORTED_SUITE` with the peer’s supported list.

### 21.2 Envelope endianness

All multi-byte integer fields in `EnvelopeV1` (§10.1) are **big-endian (network byte order)**.

### 21.3 Envelope flag bits (`flags` u16)

Bit 0 = LSB.

| Bit | Name | Meaning when set |
|-----|------|------------------|
| 0 | `RELAY_OK` | Intermediate peers MAY gossip/relay this envelope |
| 1 | `STORE_OK` | Store-and-forward peers MAY retain until expiry / retention policy |
| 2 | `PRIORITY` | Prefer earlier scheduling on constrained relays (no content privilege) |
| 3–15 | Reserved | MUST be 0 on send; ignore on receive |

### 21.4 Maximum `ciphertext_len`

- Transport envelope ciphertext (AEAD ciphertext + tag): **1 048 576 bytes (1 MiB)** maximum.
- Envelopes larger than this MUST be rejected before decrypt.
- Application files larger than this use the chunked blob path (§7); each chunk is its own envelope under this limit.

### 21.5 Noise hybrid construction — `OMCP-Noise-Hybrid-v1`

**Pin (normative for suite `0x0001`):** pattern **Noise_IK** with classical DH **X25519**, AEAD **ChaCha20-Poly1305**, hash **BLAKE2b**, plus an ML-KEM-768 (FIPS 203) ephemeral encapsulation mixed into the Noise chaining key.

**Shared-secret mixing (answers §5.1 / Q6):**  
`X25519_ss` in the informal §5.1 text means the **Noise chaining-key state after the classical DH MixKey steps**, not a raw DH output concatenated outside Noise. After the classical Noise_IK DH contributions have been mixed into `ck` per the Noise spec, both parties compute:

```
ck' = HKDF-SHA-384(
        salt = ck,
        ikm  = mlkem_shared_secret,   // 32-byte ML-KEM-768 ss
        info = "OMCP-Noise-Hybrid-v1",
        L    = 32
      )
ck ← ck'
```

Then continue Noise splitting / transport key derivation from `ck` as usual.

**ML-KEM transcript binding:** The initiator’s ML-KEM public key (and the responder’s ciphertext) are included in the Noise handshake messages as cleartext handshake payloads (or dedicated handshake fields) and are covered by subsequent `MixHash` so they cannot be stripped without failing authentication.

**Wire compatibility:** `OMCP-Noise-Hybrid-v1` is **OMCP-defined**. It is **not** claimed wire-compatible with `snow` HFS names such as `Noise_XXhfs_25519+MLKEM768_*` or with libp2p `/noise-mlkem768-hfs/*`. A future suite (`0x0002+`) MAY adopt a finalized Noise HFS / community KAT set; until then interop tests use OMCP KATs only.

**References (informative):** Noise Protocol Framework; FIPS 203 ML-KEM; TLS hybrid practice in RFC 9954 / draft-ietf-tls-ecdhe-mlkem (concatenated secrets in TLS — different combiner; do not copy TLS concat into Noise without this section’s HKDF mix).

### 21.6 Document control addendum

| Version | Date | Notes |
|---------|------|-------|
| 0.1.1 | 2026-10-23 | Wire pins for M1 PR #1 |


### 21.7 Handshake field order (OMCP-Noise-Hybrid-v1)

Within each Noise handshake message that carries ML-KEM material:

1. Emit **ML-KEM public fields first** (initiator ephemeral encapsulation key `ek` in message 1; responder ciphertext `ct` in message 2), in cleartext as required by the hybrid construction.
2. Then emit the **Noise sealed/encrypted payload** (identity proofs, certificates, Hello, etc.).

Rationale: MixHash MUST bind `ek`/`ct` before AEAD seal so an adversary cannot strip or reorder PQ material without failing authentication. Implementations MUST NOT place `ek`/`ct` after the sealed payload.

This matches commit `9e90d33` on PR #1.

## 22. Scalable sync and data efficiency (v0.2, normative unless marked)

**Goal:** a device's sync traffic grows with **its own conversations**, not with the size of the network. Targets: ≤ 5 MB/day of background mesh traffic for a typical user (50 contacts, 5 groups) in a 10 000-node mesh; ≤ 2 % of bandwidth spent on sync metadata; and no device stores or forwards more than its configured budget.

### 22.1 Stop flooding: interest-based routing
- **Mailbox tags.** Every 1:1 chat and MLS group derives a rotating 8-byte tag per epoch (6 h): `tag = BLAKE2b-64(exporter_secret, "OMCP-tag" || epoch)`. Envelopes carry the tag (this replaces the sealed address for routing purposes), and only members can compute it, so outsiders can't link tags across epochs.
- **Interest digests.** Each node advertises a compact **cuckoo filter** of the tags it wants (its own plus, optionally, *carry* tags for nearby friends) to its 1-hop neighbours. The filter is padded with decoy entries to hide the real count.
- **Forwarding rule.** A node forwards an envelope only to neighbours whose digest matches the tag, or, if nothing matches, to at most **k = 2** neighbours chosen by spray-and-wait (binary spray with L = 8 copies total). This replaces epidemic flooding from §8.3 once the node sees more than 32 peers. Small meshes (≤ 32) may keep flooding.
- **Adaptive TTL**: `ttl_hops = clamp(ceil(log2(estimated_mesh_size)) + 2, 3, 12)`, where the mesh size is estimated with HyperLogLog over the peer IDs seen.

### 22.2 Sync without resending: set reconciliation
- When two peers meet, they reconcile **only the tags both care about** using **range-based set reconciliation** (Negentropy-style) over `(timestamp, envelope_id)`. The cost is O(d · log n), where d is the number of envelopes that differ. If they already agree, the exchange is about 100 bytes.
- **Fingerprints**: 16-byte XOR+SHA-256 hashes of each range. This uses up to 3 round trips on BLE and 1 over the Internet.
- **Watermarks**: each chat keeps a per-member, signed `high-water mark` (Lamport clock). A returning device asks "everything after X" instead of re-listing history.

### 22.3 Carry less
- **Media is lazy and content-addressed.** Messages carry only a 32-byte hash, a key and a blurhash (≤ 40 B). Blobs are chunked (64 KiB, BLAKE3 tree) and fetched on demand from the nearest holder, with chunks resumed and de-duplicated. Relays never fetch media they weren't asked for.
- **Compression** (MAY): the inner payload is compressed with zstd using the static, public OMCP dictionary *before* encryption. To limit length leaks (CRIME/BREACH-style), the following are **MUST** requirements:
  - **Padding is mandatory** for every compressed or uncompressed ciphertext, using buckets of 256 B / 1 KiB / 4 KiB / 16 KiB, and above that the next multiple of 16 KiB.
  - **Never compress secrets together with attacker-influenced content** in one compression context. Key material, tokens, recovery data and control messages are never compressed. Quoted or forwarded third-party text is compressed in a separate frame from the user's own text, or not at all.
  - Dictionaries are static and public; adaptive or shared dictionaries trained on user data MUST NOT be used.
  - Senders MUST disable compression for burn-after-read and Nearby messages.
- **Delta updates**: edits, reactions and read receipts are small ops (≤ 64 B) referencing the message ID. Receipts and typing indicators are **batched** (one per 5 s per chat) and never relayed more than 2 hops.
- **Group fan-out**: one MLS ciphertext per message for the whole group, never one per member. In communities > 256 members, posts propagate along a **tree** (Plumtree: eager push to 3 peers, lazy hashes to the rest).

### 22.4 Budgets and fairness (client, SHOULD)
- A per-device **relay budget**: default 50 MB of storage and 20 MB a day of forwarding on battery and cellular, and 500 MB and unlimited on Wi-Fi with charging. When it's full, envelopes are evicted by `(priority flag, expires_at, hop count)`.
- **Store-and-forward expiry**: default 72 h, with a maximum of 7 days. Burn envelopes use their own `expires_at`.
- **Metered awareness**: gateways sync over the Internet only on unmetered links unless the user allows mobile data. On cellular, only pending outbound messages and the envelopes this device is a member of are synced.
- **Rate limiting**: each neighbour gets a token bucket (default 64 envelopes/s) so a single noisy peer can't flood the mesh.

### 22.5 Internet offload
- When any gateway is reachable, peers prefer the **gateway rendezvous mailbox** (indexed by tag) over multi-hop BLE relaying: one upload and one download, instead of N hops. BLE gossip stays the fallback when the Internet is down.

### 22.6 Acceptance tests
- SY1: a simulation (ns-3 or similar) with 10 000 nodes and 1 % of nodes active: the median per-node sync traffic is ≤ 5 MB a day, and it grows by less than 1.5× from 1 000 to 10 000 nodes.
- SY2: two peers that are already in sync exchange ≤ 256 bytes on reconnect.
- SY3: a peer offline for 7 days catches up on only its own chats; traffic is ≤ 1.2× the size of the missed payloads.
- SY4: delivery rate ≥ 98 % within 10 min for a 6-hop path at 30 % churn (no worse than flooding).
- SY5: no relay ever exceeds its configured budget; eviction order follows §22.4.
- SY6: tags from different epochs can't be linked by a non-member (a statistical test over 10⁶ tags).

## 23. Moderation and reputation (v0.2)

**Goal:** bad communities and streams can be marked by the people who see them, but **good communities with strong support from their own participants are hard to take down**, because coordinated report brigades from outsiders don't work. Everything is computed by clients from signed, gossiped data; there's no central authority.

### 23.1 Layers (all apply together)
1. **Community admins**: remove posts, mute or ban members, approve joins, slow mode (§20).
2. **Community reports**: reports are weighted. When the weighted score passes a threshold, the community or stream is marked **Reported** (§23.4).
3. **Reputation**: the threshold grows with a community's reputation among its own participants (§23.3).
4. **Moderation lists you choose** (labelers): signed lists of flagged IDs that any party can publish. Hopla ships a default list, and users can add or remove lists.
5. **Hash matching on the device** for known illegal material (CSAM hash lists), **limited to public community, channel and livestream media** as set out in §23.8. A match skips reputation. Private 1:1 and group chats are never scanned.

### 23.2 Reporter weight `w` (0–1)
Every report is a signed `REPORT{target, reason, epoch}` sent under the reporter's **per-community pseudonym key** (so it can't be linked to their main identity). Its weight is
`w = age_factor × activity_factor × accuracy × independence`
- **age_factor**: 0 if the account is less than 7 days old, rising linearly to 1 at 90 days.
- **activity_factor**: whether the reporter has real activity (sent messages, contacts with verified keys). Accounts with no activity count for 0.1.
- **accuracy**: starts at 0.5. It goes up when the reporter's past reports were later upheld and down when they were dismissed on appeal.
- **independence**: reports from accounts that are tightly linked to each other (the same join wave, mutual contacts, the same gateway or BLE cluster) are grouped and counted with √n weight, so 100 coordinated accounts count about as much as 10.
- **Insiders vs outsiders**: reports from people who have actually participated (joined, viewed for ≥ 2 min, posted) count 1×. Reports from people who never participated count 0.25×.
- Each reporter can file at most 10 reports a day, and each new report in a day weighs less than the last (1/n).

### 23.3 Community and stream reputation `R` (0–1)
Computed per community, and per stream host, from **participant attestations**:
- **Endorsements**: members can endorse a community (♥ "Vouch") once per 30 days. Endorsements are weighted by the endorser's own trust score (EigenTrust over the contact graph, seeded by verified-key contacts), so fake accounts add almost nothing.
- **Retention**: the share of members still active after 30 days.
- **Engagement quality**: reactions and replies per active member, with reports from members subtracted.
- **Age and track record**: how long the community has existed, how many reports were dismissed (this raises R) and how many were upheld (this lowers R strongly).
- `R = 0.35·endorse + 0.25·retention + 0.15·engagement + 0.15·age + 0.10·track_record`, smoothed over 14 days so a sudden attack can't move it quickly.
- Attestations are signed by participant pseudonyms and aggregated as counts plus a Merkle root, so there's no public member list.

### 23.4 Thresholds and effects
- **Report threshold**: `T = max(25, 0.05·active_members) × (1 + 9·R²)`. A new community (R ≈ 0) needs 25 weighted reports, while a well-loved one (R = 0.9) needs about 8× as many.
- **Counter-signal**: participants can say "this community is fine" in response to reports. Their votes are weighted the same way and **subtracted** from the report score (capped at 50% of it).
- **Stages**:
  - `score ≥ 0.5·T`: **Under review**. Admins are alerted, and nothing changes for users.
  - `score ≥ T`: **Reported**. It's hidden from search, recommendations and Nearby. Images, videos and livestream media are **blocked**. Text stays readable behind a warning. Existing members keep access, but new members see a warning before joining.
  - `score ≥ 3·T` **and** upheld by at least one labeler you follow: **Restricted**. Only text is shown, and new joins are blocked.
- **Livestreams** react faster: weighted reports from viewers during the stream are compared against `0.3·T` **for the host's R**. If that's exceeded, the stream is blurred for new viewers right away, and current viewers get a "Continue watching?" prompt.
- **Decay**: reports lose half their weight every 14 days, so a community recovers if nothing new is reported.

### 23.5 Appeals
- The owner can appeal once per 30 days. A **jury of 15** members is chosen at random from verified, independent users who have neither reported nor endorsed the community, and they vote within 72 h. A ⅔ "fine" result clears the Reported state, lowers the reporters' accuracy and raises R's track record. A ⅔ "bad" result does the opposite.
- Jury votes are signed and published as aggregates, so the outcome can be verified.

### 23.6 Abuse resistance (summary)
| Attack | Defence |
|---|---|
| Report brigade from fake accounts | age/activity weights, √n clustering, outsider 0.25× |
| Fake endorsements to protect a bad community | endorsement weighted by EigenTrust; upheld reports cut R strongly; CSAM hash matches ignore R |
| Gaming the jury | random independent jurors with verified keys; ⅔ majority needed |
| Slow attack over time | 14-day smoothing of R; reports lose weight over time |

### 23.7 Acceptance tests
- MR1: In simulation, 500 fresh Sybil accounts reporting a community with R = 0.8 don't reach `T`.
- MR2: 40 real, independent participants reporting a new community (R = 0.1) reach Reported within 1 h.
- MR3: when a community is Reported, its media is blocked on every compliant client within 10 min (gossip propagation).
- MR4: a CSAM hash match on public community media blocks it no matter what R is; private chats are never scanned.
- MR5: a successful appeal clears Reported and raises the community's track record.
- MR6: no attestation or report reveals a reporter's main identity.

### 22.7 Decisions from implementation review (v0.2.1)
1. **Carry tags (normative).** A node that has delivered or forwarded an envelope for tag *t* within the last hour MAY advertise *t* as a **carry tag** with a hop-distance `d` (distance-vector: it advertises `d+1` of the best neighbour). Carry tags are kept for at most 1 h and limited to 64 entries per node, and they share the decoy-padded digest. Forwarding: (a) to any neighbour that wants *t*; else (b) to the neighbour with the lowest carry distance `d` for *t*; else (c) spray-and-wait (k = 2, L = 8). This fixes delivery on static multi-hop paths (SY4).
2. **Metadata budget (rephrased).** When the node is active, sync metadata is ≤ 2 % of the payload bytes delivered. When it's idle, total overhead is ≤ 1 MB/day per node. Hello interval: 1 s on neighbourhood change, doubling up to 60 s when stable. Digests are sent only as **deltas on change**, plus a full refresh every 5 min.
3. **Tags vs blinded rendezvous labels.** Keep both. The mesh uses the 8-byte tag; gateway rendezvous uses a 32-byte label `HKDF-SHA-384(tag_secret, "OMCP-rdv" || epoch, L=32)`.
4. **Tag on the wire.** The tag is carried in `header_aad` as TLV type `0x54` (len 8). TLV registry: see §21.8.
5. **1:1 chats are 2-member MLS groups.** Every chat uses MLS (with the PQ extension). The tag `exporter_secret` is the MLS exporter, `MLS-Exporter("OMCP-tag", epoch, 32)`. Noise stays per hop (link layer).
6. **Opaque media locators.** Media is indexed by `locator = BLAKE3-keyed(media_key, "OMCP-loc" || content_hash)`, never by the plaintext content hash, to prevent confirmation-of-file attacks. Relays only ever see locators.
7. **Chunk size: 64 KiB** for all blobs (this replaces the 16 KiB default in §7). BLE framing (§10.2) fragments as needed.
8. **"Unlinkable" (SY6).** Tags from different epochs are unlinkable if, for any non-member distinguisher working on ≥ 10⁶ tag pairs, the advantage is < 0.005 and the test statistic |z| < 5. These are the pass thresholds for SY6.

9. **Per-recipient sub-tags (replaces the forwarding order in item 1).** Each member *m* of a chat or group has a sub-tag per epoch, `subtag_m = BLAKE2b-64(exporter_secret, "OMCP-sub" || leaf_index_m || epoch)`. Nodes advertise only their own sub-tags in their want-digest, and carry state is kept per sub-tag. An envelope for a 1:1 chat or a small group (≤ 8 members) carries the recipients' sub-tags (TLV `0x56`, 8 B each). Groups over 8 members use the chat tag and Plumtree (§22.3). Forwarding: (a) to neighbours that want a sub-tag; else (b) to the lowest carry distance; else (c) **scoped flood** within `ttl_hops` (it replaces spray-and-wait for cold starts and missing carry state). SY4 requires 100 % delivery, cold and warm, with fewer transmissions than pure flooding.
10. **Metadata metric (replaces 2).** *Per-message metadata* is the bytes needed to deliver one envelope beyond its ciphertext (header, routing TLVs, reconciliation exchanges caused by it). The averaged requirement is **≤ 256 B per delivered envelope, or ≤ 10 % of the ciphertext, whichever is larger**. Periodic hellos and digests count only towards the idle budget.
11. **Idle budget and refresh (replaces the 5-min refresh in 2).** Digests are sent as deltas on change. A full digest is sent **only to a new neighbour, on request** (`DIGEST_REQ`), and as a safety refresh every 30 min. Idle overhead is ≤ 1 MB/day per node with up to 32 neighbours, and above that the hello interval backs off further (up to 120 s).

12. **Compact beacon frames (normative).** Hello beacons and digest requests are 6-byte link-control frames: `type (u8) | flags (u8) | digest_id (u32, big-endian)`. `HELLO` = `0x48`; `DIGEST_REQ` = `0x52`, where `digest_id` is the digest being requested (0 = full). `flags` bit0 = `HAS_DELTA` (a delta digest follows), and bits 1–7 are reserved (0). `digest_id` = BLAKE2b with a 4-byte output (digest length parameter `nn = 4`, unkeyed, no salt or personalization) over the canonical encoding of the current want-digest, read as a big-endian u32. This is *not* a truncation of BLAKE2b-512. If the result is 0 (reserved for "full" in `DIGEST_REQ`), use 1. With these frames the idle budget is met with 32 neighbours (≈ 0.83 MB/day in simulation).

### 22.8 Mesh relay over BLE (v0.2.2)
1. **1:1 end-to-end key.** There is no pairwise static-DH stand-in. 1:1 chats are 2-member MLS groups (§22.7(5)), so implementations MUST create the MLS group on first contact (KeyPackage exchanged with the contact QR or first handshake) and use the MLS exporter for tags and the MLS application keys for content. A static-DH key with no forward secrecy is non-conformant. If the MLS group isn't established yet, messages wait in the local queue and are not mesh-relayed.
2. **Registry.** TLV `0x58` = `outer_expiry` (u32, unix seconds; relays drop expired envelopes, max now + 7 d). `0x80` stays experimental. Link PDU `0x03` = `MESH_FRAME` (carries one mesh frame). Mesh frame types: `0x44` = `DATA` (envelope), `0x45` = `DELIVERY_ACK`, `0x64` = `DIGEST_DELTA`.
3. **Delivery ack.** The recipient sends a `DELIVERY_ACK` (frame `0x45`) of 16 B: `msg_hash[0..16]`, i.e. the first 16 bytes of BLAKE2b-256 over the ciphertext. It is end-to-end MACed inside an MLS application message and addressed to the sender's sub-tag, and relays forward it like DATA. Relays that see an ack for an envelope they hold delete it. The sender's UI goes from Queued to Sent (handed to a relay) to Delivered (ack received).
4. **Media over mesh.** Media chunks (64 KiB, §7) can be relayed over the mesh for 1:1 and small groups (≤ 8) only. Each chunk is ≤ 4 hops and counts toward the relay's storage cap. Larger groups and communities fetch media directly or over a gateway. Until this is implemented, direct-only is an allowed interim.
5. **Device-key rotation.** Rotation does not invalidate in-flight envelopes. The recipient keeps the previous device key for a 7-day overlap (matching the max `outer_expiry`) and accepts envelopes encrypted to either key. Envelopes are bound to the MLS epoch, not to the device key, and the device key only signs the link handshake (§23 RS binding).

### 21.8 `header_aad` TLV registry
| Type | Name | Length | Section |
|---|---|---|---|
| `0x01`–`0x3F` | reserved for core | — | §21 |
| `0x54` | `mailbox_tag` | 8 | §22.1 |
| `0x56` | `recipient_subtags` | 8·n | §22.7(9) |
| `0x58` | `outer_expiry` | 4 | §22.8(2) |
| `0x55`, `0x57`, `0x59`–`0x7F` | unassigned (extension process §16) | — | — |
| `0x80`–`0xFF` | experimental / private use | — | — |


### 21.9 Link-control frame types
| Type | Name | Size | Section |
|---|---|---|---|
| `0x48` | `HELLO` | 6 B | §22.7(12) |
| `0x52` | `DIGEST_REQ` | 6 B | §22.7(12) |
| `0x03` | `MESH_FRAME` (link PDU; inner frame `0x44` DATA, `0x45` DELIVERY_ACK, `0x64` DIGEST_DELTA) | var | §22.8(2) |
| `0x04` | `MLS_SETUP` (link PDU; KEY_PACKAGE, WELCOME, JOINED for 2-member MLS) | var | §22.8(1) |
| other | unassigned (extension process §16) | — | — |

### 23.8 Clarifications (v0.2.1)
1. **Hash matching scope and governance.**
   - Scope: only media posted to **public** communities, channels and livestreams (content anyone can join and see), and only when it's displayed or re-shared. Private 1:1 chats, private groups and private communities are **never scanned**, and no content leaves the device for scanning.
   - List: perceptual and cryptographic hashes from a recognised hash-sharing programme for known CSAM, which needs an agreement with the provider. The list is distributed as a signed, versioned set (Ed25519, signature checked by the client, a transparency log of list versions), and updates are fetched weekly over the gateway or gossip.
   - False positives: a match blurs the media locally and stops it from being re-shared. Nothing is reported automatically. The match goes to the community admins and the default moderator service, which is run by a human, for review. Only a confirmed match leads to the Reported or Restricted state, and only on that basis.
   - Legal basis: **open item, needs legal review before launch** (EU rules on voluntary CSAM detection, the DSA, GDPR, and app store requirements). The feature ships behind a flag until that's done.
2. **Where reputation lives.** Each client computes R and report scores **locally** from signed attestations (endorsements, reports, counter-votes, retention counts). Attestations are gossiped like envelopes, using the community's mailbox tag, and stored with the community's state. For public discovery, any node may publish a signed `REPUTATION_SUMMARY` (counts plus a Merkle root over the attestations), and clients can check it against a sample of attestations. No single server is trusted.
3. **Sybil-resistant jury.** Candidates are drawn from a pool of accounts with an EigenTrust score above the 50th percentile, an age of ≥ 90 days and verified keys, excluding anyone who has reported, endorsed or counter-voted on the community. Selection uses a VRF seeded with `H(appeal_id || epoch_beacon)`, where the beacon is the hash of the latest gossip-checkpointed epoch. At most 1 juror is taken per trust cluster (§23.2 independence). Anyone can verify the selection and the signed votes.
