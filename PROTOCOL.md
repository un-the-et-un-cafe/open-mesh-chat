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

**PeerID** = `multihash(identity_signing_public_key)` truncated to 32 bytes, encoded as `omcp1:<base32>` for display.

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

**Next editors:** Implementers SHOULD open PRs against this file for any wire-format change. Semantic versioning: breaking wire changes → 1.0 major after first production interop; until then 0.x may break with explicit changelog entries.

---

## 20. Application features — communities and livestreams (v0.2 target)

Inspired by familiar messenger patterns (channels, communities, live video), implemented without weakening E2EE for private chats or mesh-first rules. Communities are open and not end-to-end encrypted (decided 2026-10-29, §20.1). **Live in communities is deferred (future work) until communities get a shared key** (maintainer decision 2026-10-31); Live in 1:1 chats and private groups is unaffected. v0.1 MVP stays 1:1 + small MLS groups; this section defines the next product layer so Proggy can plan capacity and Pipely can stage releases.

### 20.1 Communities

A **Community** is a signed container of related, **open** conversations. Community content is **not end-to-end confidential**: anyone who joins, and any node that stores or relays it, can read it. It still travels in OMCP envelopes over the mesh and gateways (§10, §22), and every post is signed by its author, so it is authenticated but public. Private 1:1 chats and private groups stay end-to-end encrypted with MLS.

| Object | Confidentiality | Mesh notes |
|--------|------|------------|
| Community roster + roles | Public to members; signed by the community identity key | Roster sync via the community's signed state |
| Announcement channel | Open (signed, not E2E encrypted) | Large fan-out via gateway + chunked store-and-forward |
| Topic channels / forums | Open (signed, not E2E encrypted); each topic has its own tag | Prefer lazy join; invite links are signed capability tokens |
| Voice chat rooms (optional later) | *Future work:* deferred with community Live until communities get a shared key | BLE unsuitable for many-way audio; Internet or Wi‑Fi Direct required |

**User-facing note (normative).** Apps MUST tell users, on the community join screen and in the post composer, that community posts are public: "Posts in communities are public. Anyone in the community can read them, and they are not end-to-end encrypted." Private groups and 1:1 chats MUST be visibly marked as end-to-end encrypted, so the two are never confused.

**Signing key for community activity (v1.0, normative).** In v1.0 a member's community posts, replies, reactions, topic joins, roster entry and community profile (including `invite_pk`, §20.7) are signed with the member's **Ed25519 identity key**; there is no per-community pseudonym key. *Honest note:* everything a member does in a community is therefore linkable to their identity, and across every community they join. The "posts are public" notice above MUST add: "Your posts show your Hopla identity, and people can see the same identity in other communities you join." Per-community keys are future work (§23.2).

**Roles:** `owner`, `admin`, `moderator`, `member`, `guest`. Capability tokens are signed by the community identity key. Moderation actions are signed control frames in the community; there is no moderation API on relays.

**Discovery:** Invite link / QR first. Communities whose owner turns on **Listed** can also be found by browsing and local search, through signed announcements gossiped over the mesh (§20.8). There is no central index server; the earlier "optional public index" is replaced by §20.8. Discovery never carries message content.

### 20.2 Channels (broadcast)

- One-to-many with admin-controlled send rights.
- Recipients verify admin signatures on posts.
- Comments/reactions are separate signed threads keyed per channel epoch (open, like the channel, §20.1).
- Forwarding preserves original signature when the sender allows “signed forwards”.

### 20.3 Livestreams

Livestreams NEED a high-bandwidth path. On pure BLE mesh they are **not** supported beyond a low-bitrate status beacon.

| Mode | Transport | Security |
|------|-----------|----------|
| `mesh-beacon` | BLE | Title/status (open in communities, E2EE in private groups); no video |
| `lan-relay` | Wi‑Fi / local IP discovered via OMCP | SFrame (RFC 9605) or Insertable Streams keys distributed via MLS exporter |
| `internet-sfu` | Gateway or user-chosen SFU | E2EE with SFU seeing only encrypted media; keys never on SFU |

**Normative:** In private groups, livestream media keys are derived from the group's MLS epoch exporter + per-stream random. In both cases below SFUs and gateways MUST NOT receive media keys.

> **Future work (non-normative): community Live.** Live in communities is **deferred until communities get a shared key** (maintainer decision 2026-10-31). Until then, compliant apps MUST NOT offer starting or joining a Live in a community and MUST ignore community Live announcements. The earlier draft (a per-stream key given to every viewer, so community Live would be public and presented as such) is kept only as a design note. Live in 1:1 chats and private groups is unaffected, and team kind `7 LIVE_TAKEDOWN` (§23.10.6) stays registered for any Live it may apply to.

**Signalling:** WebRTC-style offers/answers carried as OMCP content-type `0x50 application/omcp-rtc-signal` (E2EE).

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
- Live tab (1:1 chats and private groups only; community Live is future work, §20.3): start/join stream when `lan-relay` or `internet-sfu` available; clear “video needs Wi‑Fi/Internet” when only BLE is up
- Admin tools: slow mode, ban, delete-for-all (tombstone signed by admin)

### 20.6 Acceptance (communities / live)

1. Create a community of 3 peers on mesh; announcement post delivers to all offline-then-online members within retention window.
2. Start a livestream with Internet SFU in a private group (community Live is future work, §20.3); a member on another network watches with E2EE (keys never logged by SFU test harness).
3. BLE-only peers see live *announcement* but not video, with an explicit UI reason.

### 20.7 Private invites from a community into a private group (v0.3, normative)

**Problem.** A member may want to invite someone they only know from a community (not a contact) into a private MLS group. If the invite, the KeyPackage fetch or the reply travelled as community traffic (open, relayed, kept in history), anyone, including the Hopla team, could link the invitee to the group. That would break the hard rule of §23.10.5. This section defines a private path instead. Terms:
- **Invite key**: an HPKE public key (suite `OMCP-2026-A`, §5.1: DHKEM(X25519) + ML-KEM-768 hybrid, HKDF-SHA384, ChaCha20-Poly1305) that a member publishes in their community profile so others can send them sealed invites.
- **Inviter / invitee**: the member sending the invite / the member receiving it.
- **Invite envelope**: an ordinary `EnvelopeV1` (§10.1) whose ciphertext is a sealed invite message.
- **Recognition hint**: a value inside the envelope that lets only the invitee tell that an envelope is for them.

1. **Invite key in the community profile.**
   - Each member's community profile (the signed record under their identity key, §20.1, §23.2) MAY carry field `invite_pk` (profile field key `0x10`): `{ key_id: [8]u8, x25519_pk: [32]u8, mlkem768_pk: [1184]u8, not_after: u32 (unix s) }`, signed with the profile; the field is 1,228 B (8 + 32 + 1,184 + 4). It is published only while the user has turned on invites from non-contacts in that community (item 6, default off); otherwise the field MUST be absent.
   - The key is **per community** (a different `invite_pk` in each community, so the invite key itself does not link two communities; the profile is still signed with the identity key, §20.1) and is not the identity key, a device key or an MLS KeyPackage. Rotation follows item 1a.
   - No KeyPackage is ever published in a profile or fetched as community traffic. The invitee's KeyPackage is sent privately in step 3(b).
1a. **Invite key rotation (MUST).**
   - (a) **When.** A client MUST generate a new `invite_pk` for a community (new `key_id`, `not_after` = now + 30 d): when the current key is 30 days old; immediately when the user turns the setting of item 6 **on**; and immediately when the key may be compromised (device restore from backup, suspected key leak, user request). It MUST NOT rotate on app start, app foreground or reinstall-free restart; a key whose `not_after` has not passed is reused across launches. When the user turns the setting **off**, no new key is made (see (d)).
   - (b) **Republish.** After every rotation the client MUST republish its signed community profile with the new field. Invite-driven republishes are rate-limited to **at most one per community per hour**; a rotation inside that hour is published at the end of it (only the newest key). Turning the setting off is exempt (the withdrawal is published at once).
   - (c) **Grace period.** The previous invite private key MUST still be accepted for recognition (item 5) and decryption for **7 days** after the rotation was published (matching the maximum `outer_expiry`, §22.8(2)), and MUST then be erased from the keystore. At most 2 keys per community are held at any time; a third rotation within 7 days erases the oldest at once. A key rotated for suspected compromise is erased at once, without grace.
   - (d) **Setting turned off.** The next profile (published immediately) MUST NOT contain `invite_pk`; the private key stays usable for decryption for the 7-day grace period of (c), then is erased. Offers recognised during the grace period are dropped unread, as in item 6.
2. **Sealing.** Every invite message (item 3) is sealed with HPKE base mode to the invitee's `invite_pk`, `info = "OMCP-invite-v1" || 0x00 || key_id`, `aad = ""`. Replies are sealed the same way to a one-time `reply_pk` the inviter puts in the offer, with `info = "OMCP-invite-reply-v1" || 0x00`. The sealed plaintext is then padded to the §22.3 buckets (256 B / 1 KiB / 4 KiB / 16 KiB, then multiples of 16 KiB) exactly as 1:1 MLS ciphertexts are, never compressed.
3. **Messages (inside the seal only).** The sealed plaintext starts with an invite payload type (u8):
   - (a) `0x01 InviteOffer { offer_id[16], reply_pk (HPKE public key, same suite), inviter_pk[32], invitation_text (≤ 280 B UTF-8, optional), group_display (name, avatar hash; optional, shown only to the invitee), expires u32, sig }`. `inviter_pk` is the inviter's identity key (§20.1). `sig` is Ed25519 by `inviter_pk` over `"OMCP-invite-offer-v1" || 0x00 ||` all preceding fields, so the invitee can see which community member is inviting them and can block or report them. The offer contains **no** group ID, `group_ref`, member list or MLS material.
   - (b) `0x02 InviteReply { offer_id[16], decision u8 (1 accept, 2 decline), key_package (present only if accept; a fresh single-use MLS KeyPackage), pad }`. Accept and decline MUST be padded to the same bucket (the accept size), so they look alike.
   - (c) `0x03 InviteWelcome { offer_id[16], welcome (MLS Welcome), ratchet_tree (if not in the Welcome), group_info_extensions needed to join }`, sealed to the invitee's `invite_pk`. Only after this does the invitee learn the group ID and members, inside MLS.
   Afterwards all group traffic uses the group's normal MLS tags (§22.7(5), (9)); the invite path is not used again for that group.
4. **Outer envelope (indistinguishable from a 1:1 envelope).** An invite envelope MUST be built exactly like a 1:1 chat envelope of the same padded size: same `version`, `suite_id`, `flags` (`RELAY_OK | STORE_OK`, never `PRIORITY`), adaptive `ttl_hops` (§22.1), default store expiry 72 h and `outer_expiry` TLV `0x58` set as for 1:1 envelopes, and the same `header_aad` TLV set (`0x54` mailbox tag, `0x56` one sub-tag, `0x58`). It MUST NOT carry a group ID, `group_ref`, member list, inviter identity key, community ID or ref, the `0x5B` community TLV, or any invite-specific TLV, flag or content-type marker. Relays handle it as an ordinary private envelope.
5. **Tags only the invitee can recognise.**
   - The sender picks a fresh ephemeral X25519 key `e` for every invite message, separate from the HPKE encapsulation, and puts its **Elligator2 representative** `rep(e_pub)` (32 B, indistinguishable from random) as the first 32 bytes of the ciphertext field, followed by the HPKE output.
   - `s = X25519(e, x25519_pk)`; `t = HKDF-SHA-384(salt = key_id, ikm = s, info = "OMCP-inv-tag-v1" || 0x00, L = 16)`; mailbox tag (`0x54`) = `t[0..8]`, sub-tag (`0x56`) = `t[8..16]`.
   - Tags are new for every message, so they rotate automatically and two invites to the same person share nothing visible. Nobody without the invite private key can compute or recognise them.
   - **Delivery.** Because no one can know the tag in advance, invite envelopes are delivered by the **scoped flood** of §22.7(9)(c), the path ordinary 1:1 envelopes take on cold starts. A node with invites turned on MUST, for each envelope that reaches it with a mailbox tag it does not want, compute `X25519(invite_sk, unrep(first 32 B))` for each of its current invite keys (at most 2 per community during the grace period of item 1a(c), at most 16 in total) and compare the derived tag. On a match it decrypts; otherwise it forwards normally. This costs one X25519 per key per unknown envelope; nodes MAY skip the check when the battery saver is on and catch up when it ends (envelopes stay stored until expiry).
   - Replies use the same construction to `reply_pk`; the inviter recognises them the same way.
6. **User controls, rate limits and abuse handling (none of which reveal the group).**
   - Setting **"Invites from people who aren't contacts"**, per community, default **off**. While it is off, no `invite_pk` is published in that profile and offers are dropped unread; turning it off later removes the field and erases the private key after the grace period (item 1a(d)). The app MUST offer to turn it on in context (when the user joins a community, and when they open group invites), with a one-line explanation, and never turns it on by itself. Invites from contacts always use the normal 1:1 chat and are not affected.
   - Inviter side (sending client, MUST): at most 10 offers per hour and 30 per day in total, and at most 1 open offer per invitee `invite_pk`; a declined or expired offer to the same `invite_pk` cannot be repeated for 7 days.
   - Invitee side (receiving client, MUST): at most 3 offers per inviter (`inviter_pk`) per 7 days and 20 offers per day in total are shown; extra offers are dropped silently, without a reply. Offers from a blocked inviter are dropped silently.
   - Offers are shown as requests ("<inviter name> from <community> invites you to a private group"); nothing is joined until the user taps Accept.
   - The invitee can **block** the inviter and **report** the offer. A report is an evidence file (§23.10.3) containing only the signed `InviteOffer` (and nothing about any group, which the invitee does not know). An offer report can lead to action on the inviter in that community (§23.10.11), never to a group takedown.
   - Relays cannot rate-limit invites (they cannot tell them apart); flood cost is bounded by the normal relay budgets (§22.4) and the sender caps above.
7. **Never in community history; nothing for moderators.** Invite messages MUST NOT be posted to, stored in, or referenced from community history, topics, rosters, reputation attestations or `REPUTATION_SUMMARY` (§23.8(2)). Clients MUST NOT log them to the team or to the moderation log. Moderators learn about an invite only if a reporter submits it (item 6).
8. **Honest limits.**
   - A **global or local radio observer** who sees most BLE/Wi-Fi traffic, with timing, may still guess from traffic patterns (for example an envelope flooded from the inviter's area followed by new MLS traffic near the invitee). Padding and ordinary flood behaviour make this harder, not impossible.
   - A **compromised inviter or invitee device** (or a malicious inviter) reveals everything: the inviter knows the group and the invitee by definition, and can disclose them.
   - The **inviter's identity is seen by the invitee** (in v1.0 the community key is the identity key, §20.1); an invitee who declines still learns that this person is in some private group.
   - Publishing `invite_pk` shows that a member accepts invites in that community (but not whether they received any).
   - **Mesh-only in v1.0.** Invites are delivered only over the mesh (scoped flood). The Internet gateway path has no rendezvous for unknown tags, so an invitee reachable only over the Internet does not receive the invite. The inviter's app MUST tell the inviter, when sending, that the invite may not reach someone who is only reachable over the Internet, and show "Not delivered yet" until a reply arrives or the offer expires.
   - Scoped flood is limited by `ttl_hops`; an invitee far away on the mesh may receive the offer only after moving, or not before it expires.

> **Future work (non-normative): gateway delivery.** Two options for a later version: (b) the invitee gives its gateway a short-lived invite-recognition key (an X25519 key used only for tags), which lets that gateway see that the invitee received *an* invite, but not from whom or for which group; (c) **preferred, because it leaks nothing**: gateways keep a bounded pool of envelopes with unknown tags that clients download and check locally, so the gateway learns nothing about who received an invite.

**Acceptance tests (invites):**
- MR33: An observer holding all relayed envelopes and all community history (including the team's view) cannot link the invitee to the group: in a simulation with invite and ordinary 1:1 traffic mixed, a classifier using size, TLVs, tags, flags, TTL, expiry and timing class does no better than chance at telling invite envelopes from 1:1 envelopes, and community history contains no trace of the invite.
- MR34: No invite envelope, offer, reply or welcome outer header contains a group ID, `group_ref`, member list, inviter identity key, community ID or ref, or TLV `0x5B` (field-by-field parse and fuzzing).
- MR35: Only the holder of the invite private key can recognise or open an invite; with every other key in the simulation (including all team keys) recognition and decryption fail.
- MR36: A decline is the same padded size as an accept, is sealed to the one-time `reply_pk`, and reveals nothing to relays or to the team; a decline and a silent ignore both leave community history unchanged.
- MR37: Sender and receiver rate limits and the per-community "Invites from people who aren't contacts" setting work as in item 6; the setting is off on a fresh install and in every newly joined community, and while it is off the profile has no `invite_pk` and offers are dropped unread.
- MR39: Invites are not sent over the Internet gateway path in v1.0, and the inviter's app shows the "may not reach someone only reachable over the Internet" notice when sending.
- MR64: Invite keys rotate only as in §20.7(1a): restarting the app 100 times within 30 days produces no new `key_id` and no profile republish; a key reaches 30 days, the setting is turned on, or a compromise is flagged → a new key and a republished profile; invite-driven republishes never exceed one per community per hour.
- MR65: An offer sealed to the previous invite key is recognised and opened for 7 days after the rotation was published and refused afterwards; after day 7 the old private key is absent from the keystore (forensic scan); a compromise rotation erases the old key at once.
- MR66: Turning the setting off publishes, at once, a profile without `invite_pk`; offers arriving in the next 7 days are dropped unread; after 7 days the private key is erased.
- MR38: An offer report contains only the signed `InviteOffer`, and the team can act on the inviter in that community but cannot derive anything about a group from it.

### 20.8 Community discovery (v0.3, normative unless marked)

**Goal.** Let users browse and search open communities without any server or central index, with little traffic, and without revealing what anyone searches for. Terms:
- **Listed community**: an open community (§20.1) whose owner has turned on **Listed**. Only listed communities are ever announced. Listed is **off by default** for every new community; the owner must opt in, and an unlisted community is joinable only by invite link, QR/code or private invite. *This default is deliberate, for privacy:* a community must never become findable by accident. Designs and apps MUST NOT default to Listed.
- **Unlisted community**: any other community. It is never announced and can be joined only through an invite link or QR (§20.1) or a private invite (§20.7).
- **Announcement**: a signed `CommunityAnnouncement` (item 2) that describes a listed community.
- **Discovery index**: the local, on-device store of valid announcements that browsing and search run against.

1. **No servers, no remote queries (MUST).**
   - (a) Announcements travel only in ordinary OMCP envelopes over the mesh and, between online phones, peer to peer over the gateway path (§9), exactly like messages. Apps MUST NOT upload announcements to, or fetch them from, any index or directory server, and the protocol defines none.
   - (b) **Search is local.** Search and browsing run only against the device's discovery index. No search term, tag, category or click ever leaves the device, and the protocol defines no remote search request; apps MUST NOT add one.
   - (c) **Uniform carrying.** Nodes carry all valid announcements they hold within budget (item 5), not only those matching the user's interests, and there are no per-topic or per-language discovery subscriptions; *choice:* this costs a little more traffic but keeps what a phone carries from revealing what its user likes.
2. **Announcement format.** Payload type `0x01` (u8) followed by:
```
CommunityAnnouncementV1 {
  domain:        "OMCP-community-announce-v1" || 0x00   // in signed body only
  version:       u8      // = 1
  community_id:  [32]u8  // needed to join; announced communities are public anyway
  community_ref: [32]u8  // BLAKE2b-256("OMCP-community-ref-v1" || 0x00 || community_id); MUST match
  community_pk:  [32]u8  // community identity key that signs the roster and capability tokens (§20.1)
  owner_pk:      [32]u8  // owner's role key in the roster (in v1.0 the owner's identity key, §20.1; see §23.2)
  seq:           u64     // big-endian; strictly increasing per community_ref
  issued_ms:     u64     // big-endian, owner's clock
  name_len u8, name            // 1–64 B UTF-8
  desc_len u16, description    // 0–280 B UTF-8 and at most 140 Unicode code points; longer MUST be rejected
  n_lang u8, langs             // 0–3 BCP 47 tags, each u8 length + ≤ 8 B ASCII
  n_cat u8, categories         // 1–2 category codes (u8, list below)
  n_tags u8, tags              // 0–5 tags, each u8 length + ≤ 24 B UTF-8, lowercase, NFC
  member_bucket: u8            // 1: 1–10, 2: 11–50, 3: 51–200, 4: 201–1,000, 5: 1,001–5,000
  join_policy:   u8            // 1 open, 2 approval by a community moderator
  has_icon u8, icon_hash [32]u8 if 1  // BLAKE2b-256 of the icon chunk (§7); ≤ 16 KB WebP, fetched only on view
  sig_community: [64]u8        // Ed25519 by community_pk over domain plus all preceding fields
  sig_owner:     [64]u8        // Ed25519 by owner_pk over the same bytes plus sig_community
}
```
   - Categories (*decided 2026-10-31*): `1` local and neighbourhood, `2` news and current affairs, `3` hobbies and games, `4` arts, music and culture, `5` sports and fitness, `6` science, tech and learning, `7` support and wellbeing, `8` other (includes faith, causes and anything else). Codes `9`–`255` are reserved; receivers MUST treat an unknown code as `8` other. *Choice:* a small fixed set is easier to choose from, translates well, fits small screens and is harder to game; finer detail goes in the up to 5 free tags.
   - **Description (MUST).** The description is at most **140 Unicode code points** (and within the byte limits above and the 1 KB payload); receivers MUST reject, and not store or relay, an announcement with a longer description.
   - **Size (MUST).** The encoded payload MUST be ≤ 1,024 B; larger announcements are invalid. *Choice:* a member **bucket**, not a count, because counts cannot be checked and buckets reveal less.
   - **Announcement id**: `BLAKE2b-256("OMCP-announce-id-v1" || 0x00 || community_ref || seq)`; a newer `seq` supersedes older ones for the same `community_ref`.
   - **Deferred past v1.0 (*decided 2026-11-10*, the maintainer can overrule).** (i) **Approval-only joins:** `join_policy` `2` has no request or approve protocol in v1.0. v1.0 apps MUST send `join_policy = 1` and MUST NOT offer an approval-only option. A received announcement with `join_policy` other than `1` stays valid (it is stored, relayed and shown), but the app shows no Join button for it, only "This community joins by invite. Ask a member for an invite.", and never sends a join from discovery. (ii) **Community icons:** there is no icon set, fetch or verify protocol in v1.0. v1.0 apps MUST send `has_icon = 0` and show the default icon (first letter of the name). A received announcement with `has_icon = 1` stays valid; the app ignores `icon_hash`, fetches nothing and shows the default icon. The 5 MB icon cache of item 5(a) stays reserved and unused. Both fields keep their wire layout so a later version can add the protocols without a new announcement version. *Choice:* both add new traffic and new abuse paths (join request spam, image moderation) and are not needed to ship v1.0.
   - **Delisting.** Payload type `0x02` `CommunityDelist { domain "OMCP-community-announce-v1" || 0x00, version u8, community_ref [32]u8, seq u64, issued_ms u64, sig_community [64]u8, sig_owner [64]u8 }` with a higher `seq` withdraws the community. Nodes keep a delist for 14 d, relay it like an announcement, and drop every older announcement for that ref. Turning Listed off MUST send a delist.
3. **Envelope.** Announcements and delists are public envelopes on one well-known discovery mailbox tag `BLAKE2b-256("OMCP-discovery-tag-v1" || 0x00)[0..8]` (TLV `0x54`), flags `RELAY_OK | STORE_OK` (never `PRIORITY`), outer expiry (`0x58`) at the announcement's expiry (item 4), and `header_aad` TLV `0x5D` `community_announce`, len 41: `version u8 (= 1) || community_ref [32]u8 || seq u64 (big-endian)`, so relays can drop taken-down, delisted and superseded announcements without parsing the body.
4. **Gossip, refresh and expiry.**
   - (a) **Digest exchange.** Neighbours reconcile announcement ids with the set reconciliation of §22.2, as part of the normal digest exchange (§22.7(11)), and pull only the announcements they lack. Nothing is pushed unasked except a node's own new announcement or delist, sent once to current neighbours. **Pacing (MUST, v1.0):** the discovery part of the digest (the discovery tag's ranges) is included for a given neighbour at most once per 6 h, unless the node's own discovery index changed since it last sent it to that neighbour; other tags in the same digest are unaffected. A per-neighbour timestamp is enough; no separate reconciliation session is needed.
   - (b) **Scoped spread.** `ttl_hops` for discovery envelopes MUST be ≤ 6 (*decided 2026-10-31*); a node does not forward an announcement whose remaining hop count is 0, but may still keep it locally. Online phones MAY reconcile with peers they already talk to over the gateway path under the same rules; they MUST NOT bulk-fetch from strangers.
   - (c) **Refresh.** While Listed is on, the owner's app re-announces with a new `seq` and `issued_ms` every **7 days**, and on any change of name, description, tags, categories, language, icon or bucket.
   - (d) **Expiry.** An announcement expires **14 days** after the signed `issued_ms` of the newest valid announcement held for that `community_ref`, i.e. after two missed 7-day refreshes, and is then dropped from the index and relays. Announcements with `issued_ms` more than 24 h in the future (measured as in §23.10.11(4)(f)) are invalid. Hearing the same announcement again (same `seq`) MUST NOT extend its life: only a new signed announcement from the owner does, so a copied or replayed announcement cannot keep a community listed after its owner stopped refreshing it. Delists likewise expire 14 days after their own `issued_ms`. *Clarified 2026-11-09:* this is what "14 days after last heard" meant.
5. **Budgets (MUST).**
   - (a) **Index storage:** at most **2 MB** and **5,000 announcements** per device by default (*decided 2026-10-31*) (icons cached separately, ≤ 5 MB, evicted first). This budget is separate from the 500 MB relay cap (§22.4) and from community history (30 d / 50 MB per community, §23.10.6); discovery data MUST NOT count toward, or be evicted from, either. When full, the node evicts expired items, then the farthest (highest hop count when received), then the oldest `issued_ms`.
   - (b) **Traffic:** a node forwards at most **200** discovery envelopes per hour in total, and discovery traffic always runs below chats, invites, moderation and community posts; it MUST NOT push the idle budget of §22.7(11) over its limit. With battery saver on, discovery exchange pauses.
   - (c) **Rate limits per key:** nodes accept at most **one new `seq` per `community_ref` per 24 h** (delists are exempt), and keep at most **3** listed communities per `owner_pk` and per `community_pk` (newest kept; *decided 2026-10-31*). Extra announcements are dropped and not relayed.
6. **Validation (MUST, at every node before storing or relaying).** An announcement is valid only if: the size and field limits of item 2 hold; `community_ref` matches `community_id`; both signatures verify; `seq` is higher than any held or delisted one for that ref; it has not expired; and none of the moderation conditions of item 8 apply. A joining client MUST additionally check that `community_pk` matches the community's signed roster and that `owner_pk` holds the `owner` role there; on mismatch it drops the announcement, refuses the join from discovery, and stops relaying announcements for that ref.
7. **Ranking (local only).** Results are ordered by text match on name, description and tags (and category/language filters the user sets), then by nearness (the local "Near you" signal of item 10, then lower hop count when received), then by recency of `issued_ms`; `member_bucket` is only a tie-breaker. There is no paid placement, no "verified" or official badge, and no ranking from team keys or engagement. *Stated plainly:* member buckets and recency are set by the owner and can be gamed; the rate limits of item 5(c) and the hiding rules of item 8 limit, but do not stop, this.
8. **Moderation hooks (MUST).**
   - (a) A `community_ref` under a team `TAKEDOWN` (kind 1) or with any community tombstone (§23.10.6), in either state `banned` or `restored`, MUST never be shown, stored in the index, or relayed in discovery. (A restored community is re-created under a new ID and must be announced afresh.)
   - (b) An announcement fails validation when its `owner_pk`'s member_id is under a team `MEMBER_REMOVE` (kind 9) in that community, or when `owner_pk` no longer holds the `owner` role in the latest roster the node knows.
   - (c) Communities in the **Reported** or worse state (§23.4) are hidden from browse and search as already required there; a user's local block or hide of a community also hides it.
   - (d) **Report path.** Each discovery card offers "Report community", which uses the existing community report flow (§23) with reason codes `1`–`7`, `9` and `10` (§23.10.4(3)); choosing `10` shows the supportive notice. A report to the app team contains only the signed announcement bytes (as for invite offers, MR38). Nothing is sent automatically.
9. **Owner opt-in (MUST).** Only the owner can turn Listed on. Before the first announcement, the app MUST show: "Listing makes this community findable by anyone nearby and by phones that pass it on. Its name, description, tags and approximate size become public. Posts in communities are public." (extends §20.1).
10. **Privacy (stated plainly).** An observer who carries discovery envelopes learns that a listed community exists, its announced fields, its `community_pk` and `owner_pk`, and roughly where on the mesh it was first seen. It learns nothing about who searches or browses, because search is local and every node carries the same set. It learns nothing about unlisted communities, which are never announced. Joining remains visible to the community, as today (§20.1).
11. **Limits (stated plainly).** Offline, discovery only shows what has reached your area of the mesh; a community far away may never appear, and a new announcement takes hours to days to spread. Member buckets are the owner's own estimate and are gameable. An announcement can be copied by a modified app, but it cannot be altered or renewed without the community and owner keys.

**Acceptance tests (discovery):**
10. **"Near you" label (SHOULD, local only).** Apps SHOULD show a coarse "Near you" label (no distance, no place name, no map) on a listing whose announcement this phone itself received over the local mesh radio (Bluetooth or Wi-Fi, §9; not via an internet-connected relay or the gateway path) within at most **2 hops**, in the last **7 days**. It is computed only from this phone's own receive data; it MUST NOT be sent, relayed, stored in announcements or used to change what other phones see. Announcements carry no location. Ranking nearness (item 7) uses this same signal. *Stated plainly:* someone physically nearby can still guess that a community is active in the area, as with any mesh traffic.
- MR53: A new community is unlisted by default (no build or design defaults to Listed) and no discovery envelope for it is ever sent; it is joinable by invite link, QR/code or private invite only. Turning Listed on shows the public notice of item 9 before the first announcement.
- MR54: An announcement over 1,024 B, with a mismatched `community_ref`, a bad `sig_community` or `sig_owner`, a non-increasing `seq`, `issued_ms` more than 24 h in the future, or a description over 140 Unicode code points (141 code points rejected, 140 accepted, even when under 1,024 B) is rejected and not relayed.
- MR55: In a simulation, no search term, filter or result click from a device appears in any envelope or link frame it sends (traffic diff with and without searching is identical).
- MR56: An announcement reaches all nodes within 6 hops in a 50-node mesh within 24 h; no discovery envelope is forwarded beyond `ttl_hops` 6; an online phone never contacts any host other than peers it already reconciles with.
- MR57: An announcement disappears from the index and relays 14 days after its latest `issued_ms` unless refreshed (one missed 7-day refresh keeps it; two missed remove it); a delist removes it from every compliant index within 10 min of gossip reaching the node.
- MR58: Index storage never exceeds 2 MB / 5,000 entries; filling it does not evict relay data or community history; discovery traffic stays ≤ 200 envelopes per hour per node and chat p95 latency is unchanged in a mixed-load simulation.
- MR59: A second new `seq` for the same `community_ref` within 24 h and a fourth listed community for the same `owner_pk` or `community_pk` are dropped and not relayed.
- MR60: A `community_ref` under a team TAKEDOWN or with a community tombstone (banned or restored) is never shown, stored or relayed in discovery; an announcement whose owner is under a team MEMBER_REMOVE, or whose `owner_pk` is not the owner in the latest known roster, fails validation.
- MR61: Ranking ignores any field not in item 7; no result shows a verified or official badge; a Reported community is hidden from browse and search.
- MR62: "Report community" from a discovery card offers reason codes 1–7, 9 and 10 and sends only the signed announcement bytes; joining from discovery fails if `community_pk` or `owner_pk` does not match the roster.
- MR63: A listing received directly over Bluetooth/Wi-Fi within 2 hops in the last 7 days shows "Near you" with no distance, place or map, and ranks above an equal match without it; one received only via an internet-connected relay or the gateway path, over 2 hops, or more than 7 days ago does not; a traffic diff shows no bit of the label in any envelope or announcement sent. An announcement with category code 9–255 is accepted and shown as "Other"; codes 1–8 map to the list in item 2.



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
- **Mailbox tags.** Every 1:1 chat and MLS group derives a rotating 8-byte tag per 6-hour time slot and per MLS epoch: `tag = BLAKE2b-64(exporter_secret, "OMCP-tag" || epoch)`, where `exporter_secret` is the current MLS epoch's exporter (§22.7(5), (13)) and `epoch` is the 6-hour slot number. Envelopes carry the tag (this replaces the sealed address for routing purposes), and only members can compute it, so outsiders can't link tags across epochs.
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
Every report is a signed `REPORT{target, reason, epoch}` signed with the reporter's **report key** for that community (below), so it can't be linked to their identity key. Its weight is
`w = age_factor × activity_factor × accuracy × independence`
- **age_factor**: 0 if the account is less than 7 days old, rising linearly to 1 at 90 days.
- **activity_factor**: whether the reporter has real activity (sent messages, contacts with verified keys). Accounts with no activity count for 0.1.
- **accuracy**: starts at 0.5. It goes up when the reporter's past reports were later upheld and down when they were dismissed on appeal.
- **independence**: reports from accounts that are tightly linked to each other (the same join wave, mutual contacts, the same gateway or BLE cluster) are grouped and counted with √n weight, so 100 coordinated accounts count about as much as 10.
- **Insiders vs outsiders**: reports from people who have actually participated (joined, viewed for ≥ 2 min, posted) count 1×. Reports from people who never participated count 0.25×.
- Each reporter can file at most 10 reports a day, and each new report in a day weighs less than the last (1/n).

**Community signing key in v1.0 (normative).** Community posts, profiles, roster entries, invites (§20.7) and community moderation actions (§23.10.11) are signed with the member's **identity key** in v1.0 (§20.1), so a member's community activity is linkable to their identity and across communities. Where this document said "per-community pseudonym" for those objects, it means the identity key in v1.0.

**Report key (v1.0, normative; maintainer decision 2026-10-31).** Reports (this section) and attestations (§23.3) are signed with a dedicated per-community **report key**, used for nothing else:
- **Derivation.** Ed25519 seed = `HKDF-SHA256(ikm = identity secret seed (32 B), salt = community_ref, info = "OMCP-report-key-v1", L = 32)`, with `community_ref` as in §23.10.11(2). The key is deterministic: it is the same on every device holding the identity and after a backup restore, and is not stored in backups.
- **Properties.** Stable within one community, so per-reporter rate limits, 1/n decay and duplicate-report checks work per community; different in every community; without the identity secret it cannot be linked to the identity key or to the same user's report key in another community.
- **Key separation (MUST).** The report key MUST NOT sign posts, profiles, roster entries, invites or moderation actions, and the identity key MUST NOT sign reports or attestations. Receivers MUST reject a report or attestation whose signing key appears as a member identity key in that community's roster, and a post, profile or invite signed by a key seen signing a report or attestation there.
- **Honest limits.** Anyone receiving reports (including the intake, §23.10.3) can link reports by the same person within one community. Anything the reporter writes in free text, and the age and activity factors above, may identify them. Report keys hide the reporter from other members' views of their posts, not from a determined observer of one community's reports.

> **Future work (non-normative): per-community keys.** A later version may give each member a separate Ed25519 key per community, certified privately, so activity in two communities cannot be linked. This needs a key-binding for moderation (removal must still stick across key changes) and a migration for existing `member_id`s.

### 23.3 Community and stream reputation `R` (0–1)
Computed per community, and per stream host, from **participant attestations**:
- **Endorsements**: members can endorse a community (♥ "Vouch") once per 30 days. Endorsements are weighted by the endorser's own trust score (EigenTrust over the contact graph, seeded by verified-key contacts), so fake accounts add almost nothing.
- **Retention**: the share of members still active after 30 days.
- **Engagement quality**: reactions and replies per active member, with reports from members subtracted.
- **Age and track record**: how long the community has existed, how many reports were dismissed (this raises R) and how many were upheld (this lowers R strongly).
- `R = 0.35·endorse + 0.25·retention + 0.15·engagement + 0.15·age + 0.10·track_record`, smoothed over 14 days so a sudden attack can't move it quickly.
- Attestations are signed by participants' report keys (§23.2) and aggregated as counts plus a Merkle root, so there's no public member list.

**Deferred past v1.0 (normative; 2026-11-09).** Reputation is parked by the maintainer until after v1.0, so this section and the reporter weight `w` of §23.2 have no wire format in v1.0: v1.0 apps MUST NOT send attestations, endorsements, counter-votes, gossiped `REPORT{}` frames or `REPUTATION_SUMMARY`, and receivers MUST ignore them. In v1.0 every report (of a post, a member, a community or a discovery card) is sent only as the evidence file of §23.10.3, with key 10 `community_ref` or key 1 `group_ref`, signed with the §23.2 report key; the report-key derivation, key separation and MR70–MR71 apply unchanged. The wire format for attestations will be specified together with Reputation.

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
- MR71: An evidence file for a private group is signed with the report key derived with salt = `group_ref`, and one for a community with the §23.2 report key; the same member's files for two groups, or for a group and a community, carry different, unlinkable `reporter_key`s (MR70 thresholds), the same key for repeat files in one group, and the same key after backup restore; a file signed with the identity key, or whose `reporter_key` matches a sender or roster identity key, is rejected at verification. A file with both key 1 (`group_ref`) and key 10 (`community_ref`), or with neither, is rejected.
- MR6: no attestation or report reveals a reporter's identity key: every report and attestation is signed with the report key of §23.2, and nothing in them contains or is derived publicly from the identity key.
- MR70: The report key is derived exactly as in §23.2 (known-answer test vectors); it is identical after a backup restore and on a second device of the same identity; for 10⁴ simulated users in 2 communities, a distinguisher given both report keys and identity public keys links a user's report keys across communities, or a report key to its identity key, with advantage < 0.005; a report or attestation signed by an identity key, and a post, profile or invite signed by a report key, are rejected.

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

13. **Group mailbox tags: key schedule and scope (v0.3, normative; decided by the maintainer 2026-10-31; replaces any reading of items 1, 3, 5 and 9 and of §22.1 that conflicts: the exporter context is empty, `epoch` in §22.1 means the 6-hour slot, and the community tag is as in (f)).**
   - (a) **Private groups and 1:1 chats only.** Tags and sub-tags of items 5 and 9 apply to MLS groups (private groups, and 1:1 chats as 2-member groups). Each is derived only from the group's MLS secrets: `tag_secret = MLS-Exporter("OMCP-tag", "", 32)` of the **current MLS epoch**; the chat tag is `BLAKE2b-64(tag_secret, "OMCP-tag" || slot)` and the sub-tag `BLAKE2b-64(tag_secret, "OMCP-sub" || leaf_index || slot)`, where `slot` (u32, big-endian) is the 6-hour time slot. The rendezvous label of item 3 uses the same `tag_secret`. Tags MUST NOT be derived from the group ID, `group_ref`, any member's identity or device key, a contact relationship or any 1:1 chat secret.
   - (b) **Rotation per MLS epoch.** Tags change on every commit (join, leave, remove, update, §23.10.12) as well as every 6-hour slot. After a commit, members MUST send only under the new epoch's tags; they MAY keep wanting the previous epoch's tags (in their digest) for envelopes still in flight, for at most 7 days (max `outer_expiry`, §22.8(2)) and never after they have erased that epoch's secrets. A removed or leaving member therefore cannot compute any tag of an epoch after its removal, and relays see no link between tags of different epochs (SY6).
   - (c) **No contact needed.** A member who joined by Welcome, including one invited from a community who is not a contact of anyone in the group (§20.7), derives all tags from the Welcome's epoch secrets and sends and receives group messages with no 1:1 chat, contact or KeyPackage exchange with other members. Sub-tags are addressed by leaf index, not by contact.
   - (d) **Removal notice.** The in-epoch removal notice of §23.10.12(2)(a) is sent under the old epoch's tags (the removed member's last epoch) before the commit is applied; the removed member keeps wanting those tags until it has received the notice or the commit, or 7 days have passed.
   - (e) **Takedown and erasure.** A group's tags and `tag_secret`s of all epochs it still holds are erased with the group (§23.10.2(3)(b)–(c)); a re-formed group (§23.10.2(4a)) has a new random group ID and new MLS secrets, so its tags share nothing with the old group's. Tombstones are matched on `group_ref`, never on tags.
   - (f) **Open communities are not covered.** Communities are open (§20.1) and have no MLS secrets, so their mailbox tag is not secret and not unlinkable: `community_tag = BLAKE2b-64("OMCP-community-tag-v1" || 0x00 || community_ref || topic_ref || slot)`, computable by anyone who knows the community. SY6 does not apply to it. Community envelopes are routed by this tag and carry TLV `0x5B` (§23.10.6); their storage counts against the community history budget (30 days or 50 MB per community, §23.10.6), not against the relay budget of §22.4 (default up to 500 MB).

Acceptance tests (group tags):
- MR67: After a member is removed or leaves, it cannot compute any tag or sub-tag of later epochs (given all its stored state), and the remaining members' tags change at the commit, not only at the next 6-hour slot.
- MR68: A member who joined a private group by a §20.7 invite and has no contact with any other member sends and receives group messages; no tag computation reads a contact, 1:1 chat or identity key, and no tag is a function of the group ID or `group_ref` alone.
- MR69: After a GROUP_TAKEDOWN no tag or `tag_secret` of the group remains on the device (forensic scan as in MR25), and a re-formed group's tags are unlinkable to the old group's (SY6 thresholds, item 8).

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
| `0x5A` | `team_mod` | 1 | §23.10.7 |
| `0x5B` | `community_post` | 96 | §23.10.6 |
| `0x5C` | `community_mod` | 33 | §23.10.11(3) |
| `0x5D` | `community_announce` | 41 | §20.8(3) |
| `0x55`, `0x57`, `0x59`, `0x5E`–`0x7F` | unassigned (extension process §16) | — | — |
| `0x80`–`0xFF` | experimental / private use | — | — |


### 21.9 Link-control frame types
| Type | Name | Size | Section |
|---|---|---|---|
| `0x48` | `HELLO` | 6 B | §22.7(12) |
| `0x52` | `DIGEST_REQ` | 6 B | §22.7(12) |
| `0x03` | `MESH_FRAME` (link PDU; inner frame `0x44` DATA, `0x45` DELIVERY_ACK, `0x64` DIGEST_DELTA) | var | §22.8(2) |
| `0x04` | `MLS_SETUP` (link PDU; KEY_PACKAGE, WELCOME, JOINED for 2-member MLS) | var | §22.8(1) |
| other | unassigned (extension process §16) | — | — |

### 23.9 App-team moderation (v0.2.3, draft)
An app can name a small team allowed to delete communities. In the Hopla reference app this is the Hopla team. Other OMCP apps can ship their own list or none, so the protocol stays open.
1. **Whitelist.** The app ships a list of team PeerID hashes (BLAKE2b-256 of the Ed25519 identity key, §22). The list is part of the signed app release and changes only with an app update, never over the mesh.
2. **Takedown.** `TAKEDOWN{community_id, reason_code, note, epoch, sigs[]}` is gossiped like a moderation list (§23.1(4)) at high priority, with an outer expiry of 30 d (re-sent while it stays in force).
3. **Signature threshold.** The app ships a threshold `k` with the whitelist: a takedown is valid with signatures from `k` different whitelisted keys. Hopla v0.2 ships `k = 1` with a single key held by the maintainer. An AI moderator can later be added as another whitelisted key; the app can then raise `k` (for example AI plus a human) without a protocol change.
4. **Effect on compliant clients.** The community disappears from lists, search and Nearby; its posts, media and invite links are deleted from the device; joins and new posts are refused; relays drop its envelopes. Members see "This community was removed by the Hopla team" with the reason. Members' private 1:1 and group chats are never touched by a community action. Private groups (never 1:1 chats) can only be affected by a `GROUP_TAKEDOWN` (§23.10.2), and no team action ever reads a private chat.
5. **Transparency.** Every takedown is shown in a public in-app log (community name, reason, date, which team keys signed), so the team's actions can be checked.
6. **Reversal.** `RESTORE{community_id, epoch, sigs[]}` with `k` team signatures undoes a takedown. The owner can appeal through the jury process (§23.5); a ⅔ "fine" result is shown next to the takedown in the log, and the team must review it.
7. **Reasons.** `reason_code`: 1 hate speech or incitement, 2 illegal content, 3 terrorism or violent extremism, 4 coordinated harassment, 5 spam or scam, 9 other (note required); extended in §23.10.4(3) (adds 6, 7, 8 for restores, and 10 self-harm or suicide).
8. **Key loss.** A compromised team key is removed in the next app release. With `k = 1` the key must be kept offline (hardware key or the device's secure enclave), and the transparency log makes misuse visible.

Acceptance tests:
- MR7: A takedown signed by `k` whitelisted keys removes the community on all compliant clients within 10 min of gossip.
- MR8: A takedown with fewer than `k` team signatures, or from a non-whitelisted key, is ignored.
- MR9: Every takedown and restore appears in the transparency log.
- MR10: A community takedown never deletes or reads private chats; no team action ever touches a 1:1 chat or reads any private chat (group takedowns: §23.10.2, MR14).

### 23.10 Moderation of private groups and outside reports (v0.3, draft; maintainer decisions 2026-10-30)
This section extends §23.9 to private end-to-end encrypted groups, adds reports from outside parties, a decision log, and finer takedowns for open communities. Terms used here:
- **Private group**: an MLS group (§22.7(5)) whose messages only members can decrypt. 1:1 chats are 2-member private groups.
- **Community**: an open, joinable space (§20.1) with posts and topics. (Community Live is future work until communities get a shared key, §20.3; references to Live in communities below apply only once it ships.)
- **Team key**: an Ed25519 key on the app's whitelist (§23.9(1)). **Moderator** means anyone holding a team key, human or AI.
- **Sender signature**: the signature MLS already puts on every message (RFC 9420 `FramedContentTBS`, signed with the sender's leaf signature key). A member can export it to prove who sent a message and that it was not changed.
- **Group reference** (`group_ref`): `BLAKE2b-256("OMCP-group-ref-v1" || 0x00 || group_id)`, a fingerprint that names a group without revealing anything about it.
- **Evidence file**: the file a member's app exports for a report (§23.10.3).

> **Hard rule (normative).** Moderators can never read a private group, join it, or see its members. They only ever get what a reporter chooses to submit. Nothing in this section gives a team key any MLS role, key, or credential. See §23.10.5.

#### 23.10.1 Verifying reported messages with MLS sender signatures
OMCP adds no extra seal. Report verification uses the signature MLS already puts on every private-group message (OMCP MLS profile, §22.7(5), §22.8(1)).

1. **What is signed.** For an application message from a member (MLS `wire_format` 2 = `mls_private_message`, the only form OMCP uses for application messages; 1 = `mls_public_message` is used for handshake messages only), the sender signs `FramedContentTBS = {version, wire_format, FramedContent{group_id, epoch, sender (leaf index), authenticated_data, content_type, application_data}, GroupContext}` with its leaf signature key. The leaf's credential binds that signature key to the sender's OMCP Ed25519 identity key.
2. **What receivers keep.** A receiving client MUST keep, with each stored message, the bytes needed to re-check the signature: the serialized `FramedContent`, the `GroupContext` of that epoch (stored once per epoch), the signature, and the sender's `LeafNode` for that epoch. They are deleted with the message (including by disappearing timers) and on takedown (§23.10.2).
3. **No plaintext group metadata (profile rule).** So that evidence holds only hashes and random values: the OMCP `group_id` MUST be 32 random bytes with no meaning, and `GroupContext` extensions MUST NOT carry a group name, description, avatar or any other human-readable metadata; those travel only inside encrypted application messages. `authenticated_data` MUST NOT hold plaintext metadata either. With this rule the GroupContext holds only the random group ID, epoch, tree hash, confirmed transcript hash and capability extensions.
4. **Verification by the team.** For each reported message in an evidence file, the verifier MUST:
   a. **message signature:** rebuild `FramedContentTBS` from the submitted `wire_format`, `FramedContent` and `GroupContext`, and verify the submitted signature with the leaf signature key (`signature_key`) in the submitted `LeafNode`;
   b. **leaf node signature:** verify the `LeafNode`'s own signature (RFC 9420 §7.2, `LeafNodeTBS`, with that same `signature_key`); for a `leaf_node_source` of `commit` or `update` this needs the group ID and leaf index, which are taken from the `FramedContent`;
   c. **credential matches sender:** verify that the `LeafNode`'s credential is a valid OMCP credential binding `signature_key` to an Ed25519 identity key, and that BLAKE2b-256 of that identity key equals the submitted `sender_id`;
   d. check that `group_id` and `epoch` agree between `FramedContent` and `GroupContext`, that `BLAKE2b-256(GroupContext)` equals the submitted `group_context_hash`, and that `BLAKE2b-256("OMCP-group-ref-v1" || 0x00 || group_id)` equals the file's `group_ref`; and that `wire_format` is an allowed value (§23.10.3(3));
   e. check that the submitted `body` equals the `application_data` in the signed `FramedContent`.
   Only if all checks (a)–(e) pass is the message marked **verified genuine**. Any failure marks it **unverified**; the team MAY still consider it but MUST record that in the log (§23.10.4).
5. **Scope (stated plainly).** A verified message proves who sent it and that it was not altered. It does **not** prove the group's membership, or anything about the group beyond what is signed (the random group ID, the epoch, and the tree and transcript hashes). It does not prove that anyone received the message, or when it was sent (MLS does not sign the sender's clock). Reported messages lose deniability: anyone holding the evidence can prove who sent them. This applies to every message, old or new, for which a member still has the stored signature data. Users MUST be told this in the app (Settings > Privacy and the group info screen): "Members can report a message to the Hopla team and prove you sent it."

#### 23.10.2 Private-group takedown and restore
0. **1:1 chats are never affected.** A 1:1 chat (a 2-member MLS group created as a 1:1 chat, §22.7(5)) is never a target. Apps MUST NOT apply a `GROUP_TAKEDOWN` or `GROUP_RESTORE` to a 1:1 chat, even if its `group_ref` matches, and do not offer evidence export there (§23.10.3(2)). This keeps §23.9(4), MR10 and REQUIREMENTS TM4 true: no team action ever touches a 1:1 chat, and no team action ever reads any private chat.
1. **Actions.** Two new team actions use the §23.9 signed-action format with new `kind` values: `3 GROUP_TAKEDOWN` and `4 GROUP_RESTORE`. They apply only to private groups that are not 1:1 chats. The 32-byte target is `group_ref` (§23.10 terms), so the action does not reveal the raw MLS group ID to relays. Fields: `GROUP_TAKEDOWN{group_ref, reason_code, evidence_hash, note, epoch, sigs[]}`, `GROUP_RESTORE{group_ref, epoch, sigs[]}` (§23.10.7 gives the byte layout).
2. **Spread.** Gossiped exactly like a community TAKEDOWN (§23.9(2)): well-known team-moderation tag, `PRIORITY`, outer expiry 30 d, re-sent while in force. Relays cannot tell which group is meant without knowing its `group_id`.
3. **Effect on compliant members' apps (full erasure).** A member's app computes `group_ref` for each of its groups. On a valid `GROUP_TAKEDOWN` with a higher moderation epoch than the last applied one, it MUST, within the same run of the app: (a) delete every message, reaction, draft and media file of the group, including cached thumbnails and search-index entries; (b) delete the group's MLS state: ratchet tree, epoch secrets, exporter secrets, pending proposals, and its KeyPackages and mailbox/sub-tags; (c) delete queued and relayed envelopes for the group's tags; (d) record a **tombstone** (erase record) `{group_ref, takedown_epoch, reason_code, erased_at_ms, state = banned}` (`erased_at_ms` is informational only and MUST NOT be used for any decision) and refuse, in either state, any MLS Welcome, commit or envelope whose group ID hashes to that `group_ref`; and (e) show a single notice "A group was removed by the Hopla team" with the reason, without the group's name or content. It MUST NOT send anything about the group to the team. Deletion means the data is unrecoverable by the app: rows deleted and the database vacuumed, files removed, and keys erased from the keystore.
4. **Backups.** The group's portion of every encrypted chat backup MUST be erased too. The next backup after a takedown MUST NOT contain the group, and MUST contain the tombstone list. On restore, the app MUST first apply all tombstones it knows (from the device, from the backup being restored, and from team actions received since), before anything is written to the chat store: it drops everything belonging to any tombstoned `group_ref`, whether the tombstone's state is `banned` or `restored`, with no time comparison. An app restoring a backup older than a takedown therefore never brings erased content back, whether or not the ban has since been lifted. A group re-formed after a restore has a new `group_ref` (§23.10.2(4a)), so it is not affected.
   **Post-restore hold (normative).** After restoring a backup, the app MUST NOT show any restored private group, and MUST NOT send or relay for it, until it has completed at least one full digest exchange (§22.7(11)) with at least one mesh neighbour or gateway and applied every team action received in it. Until then such groups are listed only as "Checking…".
   **Tombstone lifetime.** Tombstones, `banned` or `restored`, MUST be kept for at least 2 years after the takedown, and for as long as the takedown is in force if that is longer. A RESTORE never deletes a tombstone.
4a. **Restore.** Content is erased, so a valid `GROUP_RESTORE` with a higher epoch can never bring content back. It only lifts the ban: the tombstone is **kept** and its `state` set to `restored`, so old backups still cannot bring erased content back (§23.10.2(4)). The old `group_ref` is no longer flagged as removed in the UI, and apps MUST NOT block members from forming a replacement group. A re-formed group MUST be a new MLS group with a new, random 32-byte group ID (and therefore a new `group_ref`); the old group ID is never reused, and messages for it stay refused. It starts empty. Members are told "This group is no longer blocked; earlier messages were erased and cannot be restored".
5. **Limits (stated plainly).** A takedown cannot erase screenshots, exported files (including evidence files), copies made by a modified app, or devices that never receive the action; a modified app can ignore it entirely. Evidence a reporter already submitted stays in the team's private store under its retention period (§23.10.4(4)). A takedown only reaches members whose apps are online on the mesh or gateway. It is a measure for compliant apps, not a guarantee of erasure.

#### 23.10.3 Reports from outside parties and the evidence file (REQUIRED before v1.0)
1. **Who can report.** Any member of a private group, or an outside party (for example law enforcement) who has obtained an evidence file from a member's phone. A report is the evidence file, which names the group only by its `group_ref`. The team decides on the evidence alone, after verifying sender signatures (§23.10.1(4)).
2. **Export.** The app offers "Report to Hopla team" on a message (long-press), on a multi-select of messages, and in group info, in private groups only (never in 1:1 chats, §23.10.2(0)). The reporter picks exactly which messages to include; nothing else is added. The app MUST show what will be included before export.
3. **Evidence file format.** Deterministic CBOR (RFC 8949 §4.2.1 core deterministic encoding), file extension `.omcp-evidence`, media type `application/vnd.omcp.evidence+cbor`. Top level is a map with integer keys:
```
EvidenceFileV1 = {
  0: 1,                       ; version (uint)
  ? 1: bstr .size 32,         ; group_ref (§23.10 terms), private-group reports; never a group name or other metadata
  2: uint,                    ; export_ms (unix ms, reporter's clock)
  3: [+ ReportedMessage],     ; 1..500 messages, in the order the reporter chose
  4: uint,                    ; reason_code (§23.10.4(3) report categories: 1–7, 9, 10)
  ? 5: tstr,                  ; reporter note (≤ 2000 bytes UTF-8)
  ? 6: ReporterContact,       ; optional
  ? 7: [* MemberDisclosure],  ; optional, only members the reporter chooses to name
  8: bstr .size 32,           ; reporter_key: the reporter's report key (§23.2) for this group or community
  9: bstr .size 64,           ; reporter_sig over the file with key 9 removed
  ? 10: bstr .size 32         ; community_ref (§23.10.11(2)), community reports. Exactly one of key 1 or key 10 MUST be present
}
ReportedMessage = {
  0: bstr .size 32,           ; sender_id (BLAKE2b-256 of the sender's identity key)
  1: bstr,                    ; framed_content (TLS-encoded MLS FramedContent, exactly as signed)
  2: bstr .size 32,           ; group_context_hash: BLAKE2b-256 of key 3
  3: bstr,                    ; group_context (TLS-encoded; random ID, epoch and hashes only, §23.10.1(3))
  4: bstr,                    ; sender MLS signature
  5: bstr,                    ; sender LeafNode (TLS-encoded, credential + signature key)
  6: bstr,                    ; body: the application_data as the app encodes it (must equal the signed one)
  ? 7: [* MediaItem],         ; optional attached media the body references
  8: uint .size 2             ; wire_format (uint16, MLS WireFormat); default and only allowed value for
                              ; reported application messages: 2 = mls_private_message.
                              ; 1 = mls_public_message is reserved (not used by OMCP for application
                              ; messages; verifiers MUST mark it unverified); any other value: file invalid
}
MediaItem = { 0: tstr, 1: bstr, 2: bstr .size 32 } ; mime, bytes, BLAKE2b-256 of bytes as referenced in body
ReporterContact = { ? 0: tstr, ? 1: tstr, ? 2: tstr } ; name or organisation, email, phone; all optional
MemberDisclosure = { 0: bstr .size 32, ? 1: tstr }    ; sender_id, optional label
```
   `reporter_sig = Ed25519(reporter_key, "OMCP-evidence-v1" || 0x00 || CBOR(file without key 9))`. `reporter_key` MUST be the reporter's **report key** (§23.2), never the identity key: for a private group, Ed25519 seed = `HKDF-SHA256(ikm = identity secret seed, salt = group_ref, info = "OMCP-report-key-v1", L = 32)`; for a community, the §23.2 report key (salt = `community_ref`). It is stable for one group or community, so repeat reports by the same member link there, and unlinkable across groups, communities and to the identity key. **Verification (MUST):** the intake verifies `reporter_sig` with `reporter_key` and MUST reject the file if `reporter_key` equals, or its BLAKE2b-256 equals, any `sender_id` or LeafNode signature key in the file or any member identity key known for that community's roster (key separation, §23.2); the derivation itself cannot be checked without the identity secret. **Target (MUST):** exactly one of key 1 (`group_ref`, private group) or key 10 (`community_ref`, community) is present; the intake MUST reject a file with both or neither, and uses the present one as the salt context of `reporter_key`. *Honest limit:* the intake cannot check that `reporter_key` was derived correctly, because that would need the reporter's identity secret; it only checks the signature and that the key is not an identity key. The signature proves the file was not changed after export; it does not by itself prove who the reporter is. The file MUST NOT contain the group name, description, avatar or any other group metadata in plaintext; the group appears only as `group_ref`, the random group ID inside the signed bytes, and context hashes. Files over 50 MB are refused at export. `evidence_hash = BLAKE2b-256(whole file bytes)`.
4. **No member list.** The file MUST NOT contain the group's member list, roster, ratchet tree, group secrets, epoch secrets or any private key. The only members named are the senders of reported messages (key 0 and the LeafNode of each message) and those the reporter adds under key 7.
5. **Intake channel.** There is no server, so the maintainer personally runs one intake point: an email address `<INTAKE_EMAIL — to be set by the maintainer>` and/or a web form at `<INTAKE_URL — to be set by the maintainer>` that accepts `.omcp-evidence` files. Both MUST be published in the app (Settings > Help > Report to the Hopla team, and on the report screen) and on the project website. The web form MUST use HTTPS. Stored reports MUST be encrypted at rest by the intake operator (the maintainer) with a key held locally by the operator. This is operational and outside the protocol: OMCP defines no team encryption key, and the team's Ed25519 keys are used only for signing. Plaintext copies MUST NOT be kept beyond what is needed to review a report. 1:1 chats are excluded from reporting (§23.10.2(0)); the group's random ID inside evidence is acceptable because it is meaningless random bytes (§23.10.1). This is the only centralized point in OMCP moderation; it holds no user data beyond the reports people submit, and it has no access to the mesh, to groups, or to keys. Other OMCP apps run their own intake or none.
6. **Decision.** The team verifies every message (§23.10.1(4)), records the decision in the log (§23.10.4), and, if upheld, issues a `GROUP_TAKEDOWN` (or a community action, §23.10.6). The team MAY reply to the reporter's optional contact. The team never contacts the group.

#### 23.10.4 Takedown log (REQUIRED before v1.0)
1. **Every decision is logged**: each TAKEDOWN, RESTORE, GROUP_TAKEDOWN, GROUP_RESTORE, community item action (§23.10.6), team MEMBER_REMOVE and its ITEM_RESTORE (§23.10.11), and each report that was rejected. Removals by a community's own owner, admins or moderators are logged in that community's own log instead (§23.10.11(6)).
2. **Public log entry** (extends §23.9(5)): `{decision_id, action kind, target (community_id, item id, member_id for kind 9, or group_ref), reason_category, evidence_hash (32 B, or all-zero if the decision was not based on a report), verified (number of messages verified genuine / total), date, team keys that signed}`. It holds only the fingerprint of the evidence, never the evidence. For private groups the target is `group_ref`, never the group name or members.
3. **Reason categories** (fixed list; same codes as `reason_code` in §23.9(7), extended): `1` hate speech or incitement, `2` illegal content (other), `3` terrorism or violent extremism, `4` coordinated harassment, `5` spam or scam, `6` child sexual abuse material, `7` threats of violence, `8` restore after review or appeal (the only code allowed on restore actions, kinds 2, 4 and 8; restores MUST carry 8, never 0), `9` other (note required), `10` self-harm or suicide. Codes `11`–`255` are unassigned (§16).
   **Valid ranges (normative).** In an evidence file (`reason_code`, key 4) and in a report-based log entry, valid report categories are `1`–`7`, `9` and `10`; `8` is not a report category, and `0` or any unassigned code makes the file invalid. Takedown and removal actions (kinds 1, 3, 5, 6, 7, 9 and community member removals, §23.10.11) accept `1`–`7`, `9` and `10`; restore actions accept only `8` (§23.10.7, MR28).
   **Self-harm or suicide (code 10).** Reports with code 10 SHOULD be placed first in the intake queue and reviewed ahead of other categories except `6` and `7`. When a user picks this category, the app SHOULD show the reporter a short supportive notice pointing to local help resources (for example "If you or someone you know is in danger, contact local emergency services or a crisis line") with a regional help link chosen by the app (Hopla: REQUIREMENTS TM18). The protocol defines no phone numbers; apps MUST NOT hard-code them in protocol data, and the notice MUST be shown on the device only, never sent to the team or the group. Code 10 is a category, not a crisis service: the team cannot locate or contact the person.
4. **Private evidence store.** The evidence file itself is kept in the team's private, encrypted store, access-logged and limited to team members. Default retention: **90 days** from the decision, unless the law requires longer or an authority has asked to preserve it. Then it MUST be deleted; the public hash stays.
5. **Never republished.** Evidence MUST never be republished, attached to the log, gossiped over the mesh, or shared beyond what the law requires, because it may itself be illegal material. Child sexual abuse material is handled only as the law of the team's jurisdiction requires (for example reporting to the competent body) and never stored longer than needed for that.
6. **Checkable.** Anyone holding an evidence file (for example the reporter) can hash it and find the matching log entry.

#### 23.10.5 Moderators cannot see into private groups (hard rule)
1. A team key MUST NOT appear in any MLS credential, LeafNode, KeyPackage, Welcome or Add proposal. Clients MUST reject any Add or Welcome whose credential identity hash is on the team whitelist, and MUST reject any GROUP_* action that tries to carry MLS material.
2. `GROUP_TAKEDOWN` and `GROUP_RESTORE` carry only the fields in §23.10.7. They MUST NOT carry a member list, sender IDs, epochs or any group secret.
3. Evidence files follow §23.10.3(4).
4. Members' apps never send anything to the team on their own. Only the user can export and submit a report.
5. Invites into private groups travel only by the private path of §20.7: sealed to the invitee, indistinguishable from 1:1 traffic, never in community history. Moderators get nothing from them beyond what a reporter submits (§20.7(6)–(7), MR33–MR38).

#### 23.10.6 Community takedowns (open communities)
Communities are open and not end-to-end encrypted (§20.1, decided 2026-10-29), so team actions can target finer items. New `kind` values in the §23.9 format:
| kind | Name | Target (`target_id`, `item_id`) | Effect on compliant clients |
|---|---|---|---|
| 1 | `TAKEDOWN` (community) | community_id, zero | §23.9(4) |
| 2 | `RESTORE` (community) | community_id, zero | §23.9(6) |
| 5 | `POST_TAKEDOWN` | community_id, post_id | the post and its media and replies are deleted and replaced by "Removed by the Hopla team" with the reason; relays drop it |
| 6 | `TOPIC_TAKEDOWN` | community_id, topic_id | the topic is hidden and its posts deleted; new posts to it are refused |
| 7 | `LIVE_TAKEDOWN` | community_id, stream_id (kept registered; community Live is future work, §20.3) | the stream ends for all viewers and new viewers are refused; **Live restart block (normative):** compliant clients MUST refuse to start, announce or join any new Live by the same host in that community for 24 h after the takedown is applied (MR29); an `ITEM_RESTORE` of the stream does not shorten the block |
| 8 | `ITEM_RESTORE` | community_id, item_id of the earlier action | lifts the ban from kind 5, 6, 7 or 9 on that item; erased content is not brought back |
| 9 | `MEMBER_REMOVE` | community_id, member_id (BLAKE2b-256 of the member's identity key) | the member is removed from that community only (§23.10.11); no content is erased; lifted by kind 8 |
**Erasure (also applies to kind 1; not to kind 9).** Every takedown of a community, topic, post or Live stream is a full erasure with the same rules as §23.10.2(3)–(4): content, media, cached copies, the community's or topic's keys and state, and its portion of chat backups are deleted; a tombstone keyed by `(community_id, item_id)` is recorded, kept in backups, applied before any backup restore, and kept for at least 2 years. `RESTORE` (kind 2) and `ITEM_RESTORE` (kind 8) only lift the ban: the tombstone stays, marked `restored`, and its target ID is never reused: a lifted community, topic or Live is re-created under a new random ID, empty, and the old ID is no longer flagged in the UI. On backup restore, everything belonging to a tombstoned `(community_id, item_id)` is dropped whatever its state, with no time comparison. Any `erased_at_ms` in a community tombstone is informational only. (This replaces the "if content is still held" wording and clarifies §23.9(4) and (6).)

**Compatibility note (pre-v1.0).** App 0.1.23 used kinds `3`/`4` for Live takedown/restore. In this spec Live moved to kinds `7`/`8`, and `3`/`4` are group actions, so team-action state saved by 0.1.23 will not load. This is acceptable before v1.0: on upgrade, apps MUST discard all stored team-action state (but keep tombstones and erased-state flags, so nothing erased reappears) and re-sync team actions from the mesh, which re-sends them while they are in force (§23.9(2)). After v1.0, kind values MUST NOT be reassigned. Likewise, 0.1.23 sent `reason_code` 0 on a v1 `RESTORE` (kind 2); from this version restores MUST carry 8, and clients MUST reject 0.

`post_id`, `topic_id` and `stream_id` are the 32-byte IDs the community already uses (§20); if an ID is shorter it is left-padded with zeros. Community limits that clients enforce and that bound what a takedown must remove: **about 5,000 members per community** (joins beyond 5,000 are refused), **4 KB per post** (UTF-8 text plus inline metadata; media are separate chunks, §7), and history kept **30 days or up to 50 MB per community, whichever is reached first** (oldest posts evicted first).

**Post envelopes (normative).** Every community post envelope carries `header_aad` TLV `0x5B` `community_post`, len 96:
```
CommunityPostTLV {
  community_ref: [32]u8  // BLAKE2b-256("OMCP-community-ref-v1" || 0x00 || community_id)
  topic_ref:     [32]u8  // BLAKE2b-256("OMCP-topic-ref-v1" || 0x00 || topic_id); all-zero for the announcement channel
  post_id:       [32]u8  // the post's 32-byte ID
}
```
Relays and clients use it to drop envelopes covered by a post, topic or community takedown without parsing the body. Communities are open, so the refs hide nothing from members; they only keep raw IDs out of the routing header.

Community history (30 days or 50 MB per community) is budgeted separately and does not count toward the relay budget in §22.4.

#### 23.10.7 Wire format of team actions (v2)
Existing kinds 1 and 2 keep the v1 body from §23.9 (`"OMCP-team-mod-v1" || 0x00 || kind u8 || community_id[32] || epoch u64 || reason_code u8 || note_len u16 || note`). Kinds 3–9 use a v2 body:
```
TeamActionV2 {
  domain:        "OMCP-team-mod-v2" || 0x00   // in signed body only, not on the wire
  kind:          u8      // 3 GROUP_TAKEDOWN, 4 GROUP_RESTORE, 5 POST_TAKEDOWN, 6 TOPIC_TAKEDOWN, 7 LIVE_TAKEDOWN, 8 ITEM_RESTORE, 9 MEMBER_REMOVE
  target_id:     [32]u8  // group_ref for 3/4, community_id for 5–9
  item_id:       [32]u8  // zero for 3/4; post_id/topic_id/stream_id for 5–7; member_id for 9; for 8 the item_id of the action it lifts
  epoch:         u64     // big-endian; per (target_id, item_id) moderation epoch, highest wins
  reason_code:   u8      // §23.10.4(3); MUST be 8 (restore after review) for kinds 4 and 8
  evidence_hash: [32]u8  // §23.10.3(3); all-zero if none
  duration_s:    u32     // PRESENT ONLY for kind 9: big-endian, 0 = permanent (team key only), else 3600–31,536,000 s
  note_len:      u16     // big-endian, ≤ 512
  note:          [..]    // UTF-8
  n:             u8      // number of signatures
  sigs:          n × (public_key[32] || ed25519_signature[64])
}
```
Signatures are over the domain plus all fields up to `note` (including `duration_s` for kind 9). Validity follows §23.9(3) (k distinct whitelisted keys) and the epoch rule of §23.9 (strictly greater than the last applied epoch for the same target). Envelope: same as §23.9 (public, team-moderation tag, `RELAY_OK | STORE_OK | PRIORITY`, outer expiry 30 d), with `header_aad` TLV `0x5A` `team_mod` (len 1, value = body version, 1 or 2).

> **Open question for the maintainer (W-1).** The implementation on the team-moderation branch marks team-moderation envelopes with TLV `0x8F` (experimental range). This section registers `0x5A` for it; the code should move to `0x5A` before v1.0. Also, the code comments refer to `config/team_moderation.json` while the shipped file is `config/team_moderation.conf`.

#### 23.10.8 Keys before v1.0
The real moderator key only arrives in v1.0. Until then: **debug builds** ship a published development key (clearly labelled, never used for real moderation) with `k = 1`, so these features can be tested; **release builds** ship an empty whitelist and MUST NOT enforce any TAKEDOWN, RESTORE or GROUP_*/item action (they ignore them, as `config/team_moderation.conf` already does with no key). Release builds MUST still implement evidence export once it ships.

#### 23.10.9 Registry additions
- Team action `kind`: `3` GROUP_TAKEDOWN, `4` GROUP_RESTORE, `5` POST_TAKEDOWN, `6` TOPIC_TAKEDOWN, `7` LIVE_TAKEDOWN, `8` ITEM_RESTORE, `9` MEMBER_REMOVE (§23.10.11); `10`–`255` unassigned (§16).
- Reason category `10` self-harm or suicide (§23.10.4(3)); `11`–`255` unassigned.
- `header_aad` TLV `0x5C` `community_mod` (len 33) — added to §21.8 (§23.10.11(3)).
- Community moderation action codes (§23.10.11(2)): `1` MEMBER_REMOVE, `2` MEMBER_RESTORE, `3` ROLE_REVOKE; `0`, `4`–`255` unassigned.
- Private-group control payloads (§23.10.12, inside MLS or the 1:1 seal): `0x01` RemovalNotice; `0x02`–`0xFF` unassigned.
- `header_aad` TLV `0x5A` `team_mod` (len 1) — added to §21.8.
- `header_aad` TLV `0x5B` `community_post` (len 96) — added to §21.8.
- Community profile field `0x10` `invite_pk` (§20.7(1)); `0x11`–`0xFF` unassigned. (No other profile-field registry exists yet; this starts one.)
- Invite payload types inside the seal (§20.7(3)): `0x01` InviteOffer, `0x02` InviteReply, `0x03` InviteWelcome; `0x04`–`0xFF` unassigned.
- No new `header_aad` TLV for invites, on purpose (§20.7(4)).
- `header_aad` TLV `0x5D` `community_announce` (len 41) — added to §21.8 (§20.8(3)).
- MLS GroupContext extension `0xF0A1` `omcp_group_admins` (private-use range) and community moderation action code 4 `OWNER_HANDOVER` (§23.10.13).
- Discovery payload types (§20.8(2)): `0x01` CommunityAnnouncement, `0x02` CommunityDelist; `0x00`, `0x03`–`0xFF` unassigned.
- Community category codes (§20.8(2)): `1`–`8` as listed there; `0` unassigned; `9`–`255` reserved (receivers treat unknown codes as `8` other).
- Media type `application/vnd.omcp.evidence+cbor`, file extension `.omcp-evidence`.
- Domain strings: `"OMCP-group-ref-v1"`, `"OMCP-community-ref-v1"`, `"OMCP-topic-ref-v1"`, `"OMCP-invite-v1"`, `"OMCP-invite-reply-v1"`, `"OMCP-invite-offer-v1"`, `"OMCP-inv-tag-v1"` (§20.7), `"OMCP-team-mod-v2"`, `"OMCP-evidence-v1"`, `"OMCP-community-mod-v1"`, `"OMCP-member-id-v1"` (§23.10.11), `"OMCP-removal-notice-v1"` (§23.10.12), `"OMCP-community-announce-v1"`, `"OMCP-announce-id-v1"`, `"OMCP-discovery-tag-v1"` (§20.8), `"OMCP-report-key-v1"` (HKDF info, §23.2), `"OMCP-community-tag-v1"` (§22.7(13)(f)).

#### 23.10.10 Acceptance tests
- MR11: A reported message whose framed content, body, sender, group ID or epoch is changed by one bit fails verification; an unchanged one passes all checks of §23.10.1(4). Separate negative cases MUST each fail: (a) a wrong message signature, (b) a LeafNode with a broken own signature, (c) a credential whose identity hash differs from `sender_id`, and a `wire_format` other than 2.
- MR12: Verifying one reported message never requires, and the evidence file never contains, any other message, group secret, ratchet tree, or plaintext group name or metadata (only `group_ref`, the random group ID and context hashes).
- MR13: *Withdrawn* (was a franking test; franking was dropped).
- MR14: A GROUP_TAKEDOWN whose `group_ref` matches a 1:1 chat leaves that chat untouched. A valid GROUP_TAKEDOWN erases the group on all compliant members' apps within 10 min of gossip; a valid later GROUP_RESTORE lets members re-form or rejoin it, empty, keeps the tombstone (state `restored`), and never brings content back, including from a backup made before the takedown. The re-formed group has a new random group ID and `group_ref`; envelopes, Welcomes and commits for the old group ID are still refused.
- MR15: A GROUP_* action with fewer than `k` signatures, a non-whitelisted key, or an epoch not higher than the last applied is ignored.
- MR16 (hard rule): A moderator key cannot open group keys: given every team key, a test harness cannot decrypt any private-group message or derive any epoch secret.
- MR17 (hard rule): A moderator key cannot add itself as a member: Add proposals, KeyPackages and Welcomes carrying a whitelisted identity are rejected by every compliant client.
- MR18 (hard rule): GROUP_TAKEDOWN and GROUP_RESTORE encodings never contain a member list, sender ID or group secret (checked by fuzzing the encoder and by field-by-field parsing).
- MR19 (hard rule): An exported evidence file contains no member identity beyond the senders of the selected messages and the members the reporter explicitly added.
- MR20: The evidence file is byte-identical when exported twice from the same selection (deterministic CBOR), and a one-bit change breaks `reporter_sig`.
- MR21: Every decision appears in the public log with its `evidence_hash` and reason category; no log entry contains evidence content.
- MR22: POST_, TOPIC_ and LIVE_TAKEDOWN erase only the named item on compliant clients within 10 min; ITEM_RESTORE lifts the ban but brings back no erased content.
- MR23: Clients refuse a community's 5,001st member, posts over 4 KB, and evict history older than 30 days or beyond 50 MB.
- MR24: Release builds before v1.0 ignore all team actions; debug builds enforce actions signed by the dev key.
- MR25 (erasure): After a GROUP_TAKEDOWN or community/item takedown, a forensic scan of the app's storage (database including free pages, files, caches, search index, keystore) finds nothing from the target; a fresh backup contains nothing from it and contains its tombstone; restoring a backup made before the takedown, on a clean device, does not resurrect it (the group or item never appears, and its MLS state is not reloaded). This holds for both tombstone states (`banned` and `restored`) and for any device clock setting: the test MUST also pass with the device clock set years ahead or behind.
- MR26: The intake operator stores reports only encrypted at rest with a locally held key (operational, outside the protocol; no protocol team encryption key exists), and no plaintext copy is kept beyond review.
- MR27: The community join screen and post composer show the "posts are public" note (§20.1), and private groups and 1:1 chats are marked end-to-end encrypted.
- MR28: Every restore action (kinds 2, 4, 8, and community MEMBER_RESTORE) carries `reason_code` 8; a restore with any other code is rejected.
- MR29: After a LIVE_TAKEDOWN, compliant clients refuse a new Live by the same host in that community for 24 h, and allow it after.
- MR30: After restoring a backup, no private group is shown or relayed until one full digest exchange has completed and its team actions are applied; content of a tombstoned `group_ref` is dropped in both states, while a re-formed group under a new group ID is restored normally.
- MR31: Every community post envelope carries a valid `0x5B` TLV; relays drop envelopes whose post, topic or community is under takedown.
- MR32: Upgrading from 0.1.23 discards old team-action state, keeps tombstones, and re-syncs actions from the mesh; no erased content reappears.
- MR40: Reason code `10` (self-harm or suicide) is accepted in evidence files, team actions and community removals; `0`, `8` on a non-restore and any code ≥ `11` make an evidence file or action invalid; restore actions still accept only `8`.
- MR41: Choosing reason 10 when reporting shows the reporter the supportive notice with a regional help link on the device; nothing about the notice is sent, and the protocol data contains no phone number. Code-10 reports are sorted ahead of other categories (except 6 and 7) in the intake queue.
- MR42: A community MEMBER_REMOVE signed by the owner, an admin or a moderator of that community takes effect on all compliant clients within 10 min of gossip: the member cannot post, react, join topics or start/join Live there; others drop the member's new posts there; existing posts stay; re-join is refused.
- MR43: A removal by an actor not in the roster, of equal or higher rank, targeting the owner, for another community, with a mismatched `member_id`, a bad signature, or an epoch not higher than the last applied, is rejected.
- MR44: A removed member's 1:1 chats, private groups and other communities are unchanged (byte-for-byte state check before and after), and the device shows the plain notice naming the role, the reason and any end date.
- MR45: A temporary removal ends at `issued_ms + duration_s` as measured in §23.10.11(4)(f), including with the removed device's clock set ahead; a permanent team `kind 9` removal (`duration_s = 0`) ends only with a restore. A community MEMBER_REMOVE with `duration_s` 0, below 3,600 or above 31,536,000 is rejected. A MEMBER_RESTORE or ITEM_RESTORE with a higher epoch lifts it; a community restore cannot lift a team removal.
- MR46: Team `kind 9` with `k` whitelisted signatures removes a member in any community and appears in the public team log; ITEM_RESTORE with `item_id = member_id` and reason 8 lifts it.
- MR47: A moderator's or admin's 11th removal in a rolling hour or 31st in a rolling 24 h (by signed `issued_ms`, per `actor_pk`) is ignored by compliant clients (not applied, not relayed) but still counted and shown to the owner; the owner's 31st removal in 24 h and team `kind 9` actions are applied; the owner is alerted at a moderator's 6th removal in a day; the owner can revoke the moderator and reverse their removals; every moderator removal appears in the owner's Removals view and the community log.
- MR48: Every community removal, restore and role revoke appears in the community's own log, and every team removal and restore in the public team log; no entry contains post content or evidence.
- MR49: Community moderation envelopes carry a valid `0x5C` TLV whose `community_ref` matches the body.
- MR50: A member who leaves a private group sends a self-removal proposal and never commits it; a remaining member commits it within 24 h of first online receipt, and if none is online the next member to come online commits it after its first digest exchange; after the commit the leaver cannot decrypt any new message; the leave appears in the group's key-rotation log.
- MR51: A removed member receives a removal notice in the last epoch it held (or the 1:1 fallback), the notice names the remover's role and not their identity, and the device marks the group removed and erases its MLS state and keys (forensic scan as in MR25 for key material); history it already held stays readable only as the user chose.
- MR52: Leave, removal and removal notices never apply to 1:1 chats and never involve a team key.

#### 23.10.11 Removing a member from a community (v0.3, draft)
A single member can be removed from one open community without touching anything else they do.

**Decided (maintainer, 2026-10-31).** The choices in this section and §23.10.12 are approved as written and are no longer open: a removed member can still read but not post (item 4(a)); removal notices name the role only (item 4(a), §23.10.12(2)(b)); community removals are always timed, 1 hour to 365 days (item 2); moderator limits are 10 per hour and 30 per day, with an owner alert above 5 per day (item 5).

Terms:
- **member_id**: `BLAKE2b-256(Ed25519 identity public key)`, the OMCP identity hash (§23.9(1)). In v1.0 community activity is signed with the identity key (§20.1, §23.2), so a removal names the member's identity. (If per-community keys are added later, member_id would be the hash of that key.)
- **Community moderator**: a member holding the `owner`, `admin` or `moderator` role in that community's signed roster (§20.1). The `owner` is the community's founder.

1. **Who may remove.**
   - (a) In its own community only: the owner, any admin and any moderator, using a community moderation action (item 2) signed with their role key from the roster.
   - (b) **Rank rule (MUST):** an actor can remove or restore only members of a strictly lower rank (`owner` > `admin` > `moderator` > `member`/`guest`). Nobody can remove the owner. Clients MUST reject an action that breaks this rule.
   - (c) The app team, across any community, with team action `kind 9 MEMBER_REMOVE` (§23.10.6 table, §23.10.7 body), lifted by the existing `kind 8 ITEM_RESTORE` with `item_id = member_id`. *Choice:* re-using kind 8 instead of a new restore kind keeps one restore path for every per-item ban (5, 6, 7, 9), keeps the restore reason rule (8 only, MR28) unchanged, and needs no new code. Team removals are valid with `k` whitelisted signatures (§23.9(3)) and override community roles: a community moderator cannot lift a team removal.
   - (d) Never in private groups or 1:1 chats. Private-group membership is managed only by MLS (§23.10.12); 1:1 chats are never affected (§23.10.2(0)).
2. **Community moderation action (signed).** Carried as a community control frame (§20.1; open, not E2E encrypted):
```
CommunityModActionV1 {
  domain:        "OMCP-community-mod-v1" || 0x00   // in signed body only
  action:        u8      // 1 MEMBER_REMOVE, 2 MEMBER_RESTORE, 3 ROLE_REVOKE (owner only; demotes a moderator/admin to member), 4 OWNER_HANDOVER (§23.10.13(2))
  community_ref: [32]u8  // BLAKE2b-256("OMCP-community-ref-v1" || 0x00 || community_id)
  member_pk:     [32]u8  // target's Ed25519 identity public key (v1.0, §20.1)
  member_id:     [32]u8  // BLAKE2b-256(member_pk); MUST match
  reason_code:   u8      // §23.10.4(3): 1–7, 9, 10 for action 1; MUST be 8 for action 2; 0 allowed only for action 3
  duration_s:    u32     // big-endian; action 1: 3600–31,536,000 (1 h – 365 d), no permanent; MUST be 0 for actions 2 and 3
  issued_ms:     u64     // big-endian, actor's clock (informational and for duration, see item 4(f))
  epoch:         u64     // big-endian; per (community_ref, member_id) epoch, highest wins
  actor_pk:      [32]u8  // actor's role key, MUST be in the roster with a role allowed by item 1
  note_len:      u16     // ≤ 512
  note:          [..]    // UTF-8; required for reason 9
  sig:           [64]u8  // Ed25519 by actor_pk over domain plus all preceding fields
}
```
   **Duration (MUST).** A community MEMBER_REMOVE has no permanent option: `duration_s` MUST be between 3,600 (1 h) and 31,536,000 (365 d), and receivers MUST reject action 1 with any other value, including 0. Apps offer the presets 1 h, 24 h, 7 d, 30 d and 365 d; a longer ban is made by issuing a new removal (higher epoch) before the current one ends. Only the team form (`kind 9`, signed under §23.9(3)) may use `duration_s = 0` (permanent).
   The team form (`kind 9`) uses the v2 body of §23.10.7 with `target_id = community_id`, `item_id = member_id` and `duration_s` after `evidence_hash`.
3. **Propagation.** Both forms travel the same relay path as other moderation actions: community actions as community control frames on the community's mailbox tag, team actions on the team-moderation tag (§23.9(2), §23.10.7); `RELAY_OK | STORE_OK | PRIORITY`; outer expiry 30 d or the end of the duration if sooner, re-sent while in force. Community action envelopes carry `header_aad` TLV `0x5C` `community_mod`, len 33: `version u8 (= 1) || community_ref [32]u8`, so relays can prioritise them and drop superseded ones without parsing the body. Clients apply the action with the highest epoch per `(community_ref, member_id)`, across both forms; a valid team action always beats a community one of any epoch while it is in force.
4. **Effect on compliant clients.**
   - (a) **Removed member's device:** within the same run of the app, it MUST stop the member from posting, replying, reacting, creating or joining topics, voting, and starting, announcing or joining Live in that community; drop queued outgoing community envelopes for it; and show a plain notice: "You were removed from <community> by a community moderator" (or "by the Hopla team"), with the reason and, if temporary, the end date. It names the role, not the person. The community stays readable as before until history evicts or the member leaves; *decided 2026-10-31:* read access is not cut, because the community is open anyway and cutting it would give a false sense of control.
   - (b) **Other members' devices and relays:** MUST drop new posts, reactions, topic joins and Live announcements from that `member_id` in that community, dated after the action was applied locally; MUST refuse a re-join (roster entry) by that member.
   - (c) **Existing posts stay** unless separately taken down (kinds 5–7 or the community's own post removal).
   - (d) **Nothing else changes:** the member's 1:1 chats, private groups, other communities, contacts and identity are untouched. Apps MUST NOT apply a removal outside the named `community_ref`.
   - (e) **Rejoin:** blocked until a MEMBER_RESTORE / ITEM_RESTORE with a higher epoch, or until the duration ends. After that the member rejoins normally (a fresh join; nothing is restored).
   - (f) **Duration end:** a temporary removal ends at `issued_ms + duration_s`. Because device clocks can be wrong, clients MUST measure this against the mesh-checkpointed epoch time (§23.8(3) beacon) when available and otherwise the local clock; a removed member's own device MUST NOT treat it as ended earlier than any neighbour does (it re-checks with one digest exchange, §22.7(11), before posting again).
5. **Abuse guard against mass removal (MUST).**
   - (a) The limit applies to community **moderators and admins**: each `actor_pk` can issue at most **10 removals per rolling hour and 30 per rolling 24 h** per community, counted by the action's signed `issued_ms`. The community **owner (founder)** and the **team key** are exempt (decided by the maintainer, 2026-10-31); the team is bounded by its public log. Receivers MUST ignore a moderator/admin removal that would exceed the limit (not apply it, not relay it), but MUST still count it toward the actor's totals so the owner sees the abuse.
   - (b) Moderators and admins cannot remove or demote anyone of equal or higher rank (item 1(b)).
   - (c) The owner can revoke any moderator or admin role (`ROLE_REVOKE`) and reverse any of their removals (`MEMBER_RESTORE`). A revoked actor's removals stay in force until restored; *choice:* not undone automatically, so a revocation is not itself a mass-unban.
   - (d) Every removal by a moderator or admin is shown to the owner (and admins) in a "Removals" view with actor, target member, reason and date, and the owner's app MUST alert the owner when any moderator or admin exceeds 5 removals in 24 h (counting ignored over-limit attempts).
   - (e) A removed member can appeal to the owner by an ordinary 1:1 message, and to the jury (§23.5) for team removals.
6. **Transparency log.** Every removal and restore appears in a log: community-moderator actions in the **community's own log** (kept with the community's signed state; open to members, like the roster; `{action, member_id, reason_code, duration_s, issued_ms, actor role, actor_pk, epoch}`, kept at least as long as the action is in force and at least 1 year), and team actions (kind 9 and its ITEM_RESTORE) in the **public team log** (§23.10.4(2)), with `target = community_id`, item `member_id`. Neither log contains post content or evidence.

#### 23.10.12 Leaving and removal in private groups (MLS) (v0.3, draft)
This section is ordinary MLS membership change; it gives no team key any role (§23.10.5) and never applies to 1:1 chats (§23.10.2(0)), which have no leave other than deleting the chat locally.
1. **Voluntary leave.**
   - (a) A member who taps Leave sends a proposal removing its own leaf: an MLS `SelfRemove` proposal where the group's MLS implementation supports it, otherwise an RFC 9420 `Remove` proposal naming its own leaf index. RFC 9420 forbids a member committing its own removal, so it MUST NOT commit it.
   - (b) Its device then shows the group as **left** (read-only), and erases the group's MLS state, keys and tags as in §23.10.2(3)(b)–(c), keeping only the message history the user chooses to keep (default: keep, read-only; the user can also delete it).
   - (c) **Bounded commit (MUST):** remaining members' apps MUST commit the removal (epoch advance, so the leaver's keys stop working) within **24 h** of the first remaining member receiving the proposal while online. To avoid races, the online member with the lowest leaf index commits at once; others wait a random 10–60 min and commit only if no commit has arrived. If no member is online, the **next member to come online** commits it after its first digest exchange. A member MUST NOT send new application messages in the old epoch after it has seen a pending leave for more than 24 h; it commits first.
   - (d) **Key-rotation log:** each group keeps, inside its encrypted MLS state (never sent outside the group), a per-group log of epoch changes `{epoch, cause (join, leave, remove, update), affected member_id, committer member_id}`; every leave and removal MUST be recorded there and shown in group info ("<name> left"). This log is new in this version and is deleted with the group.
2. **Removal by a member (group admin).** A Remove commit is made as MLS already allows; the remover's app then sends a **removal notice** to the removed member:
   - (a) Because the removed member cannot decrypt the new epoch, the notice is sent as an MLS application message **in the last epoch the removed member held** (sent just before the commit is applied, in the same batch). If that is not possible (the remover is not on that epoch, or the removed member may already have missed it), it is sent as a 1:1 envelope from the remover (§20.7(4) form, ordinary 1:1).
   - (b) Content: `RemovalNotice { type 0x01, group_ref [32]u8, removed_at_epoch u64, remover_role u8 (1 group admin, 2 member), reason_text_len u16 (≤ 280), reason_text, sig }`, signed by the remover's leaf key with domain `"OMCP-removal-notice-v1" || 0x00`. *Decided 2026-10-31:* it names the remover's **role only**, never the remover's identity, so the notice cannot be used to target them; the removed member may still infer who it was from the group. The 1:1 fallback necessarily reveals the sender; apps SHOULD use it only when the in-epoch path failed, and say so to the remover.
   - (c) On a valid notice (and on detecting its own removal in a commit even without one), the removed member's device MUST mark the group **removed** (read-only), erase the group's MLS state, epoch and exporter secrets, KeyPackages and tags (§23.10.2(3)(b)–(c)), and show "You were removed from this group". It MUST NOT try to rejoin with old material; rejoining needs a new invite.
3. **What a removed or leaving member can still see (stated plainly).** A malicious removed member, or one with a modified app, keeps everything they had already decrypted: earlier messages, media and the signature data that can prove who sent them (§23.10.1). They can still report those messages (§23.10.3). They cannot read anything sent after the commit (forward secrecy by epoch advance), cannot see later joins or leaves, and their keys from old epochs open nothing new. Until the commit is made (item 1(c)), they could still decrypt messages sent in the old epoch, which is why the commit is bounded and new messages wait for it.

#### 23.10.13 Founder and owner handover (v0.3, REQUIRED before v1.0; maintainer may overrule, 2026-11-09)
Without this section a private-group founder or a community owner cannot leave. It is kept minimal for v1.0.

1. **Private groups: admin list in the MLS group context.**
   - (a) **Extension.** Group admins are listed in an MLS GroupContext extension `omcp_group_admins`, extension type **`0xF0A1`** (MLS private-use range), body `count u8 (1–16) || member_id [32]u8 × count`, member_id as in §23.10.11. The founder creates the group with the extension naming only itself, and lists `0xF0A1` in `required_capabilities`, so every member's client must understand it. A group created before this section, without the extension, is treated as having the member at leaf index 0 as its only admin.
   - (b) **Commit rules (replace the founder-only check).** Any member may commit leave (`SelfRemove` or self-`Remove`) and `Update` proposals. Only a current admin may commit `Add` or `Remove` of other members or a `GroupContextExtensions` proposal that changes `omcp_group_admins`. The MLS signature of the committer's leaf is the proof; no extra signature is needed. Receivers MUST reject a commit that breaks these rules.
   - (c) **Handover.** An admin may add or remove admins at any time. If the leaver is the **last admin**, the app MUST ask it to choose a new admin among the remaining members (admins first, then members in order of joining), then commit a `GroupContextExtensions` proposal naming the new admin, and only then send its leave proposal (§23.10.12(1)). The new admin needs no acceptance: becoming admin only allows it to add and remove people, and it can step down the same way.
   - (d) **Fallback (MUST, deterministic).** If, after any commit, no member named in `omcp_group_admins` is still in the group (for example the last admin's app was deleted, or its leave was committed without a handover), every client treats the remaining member with the **lowest leaf index** as admin, and that member's app commits a `GroupContextExtensions` proposal naming itself with its next commit. All clients compute the same answer from the same tree, so there is no race.
   - (e) **No one left.** If the leaver is the only member, leaving deletes the group on that device (§23.10.2(3)(b)–(c)); nothing is sent.
   - (f) The key-rotation log (§23.10.12(1)(d)) records admin changes with cause `admin` ("<name> is now an admin").
2. **Communities: owner handover.**
   - (a) **Who can receive it.** Any member in the current roster who is not under a community or team removal (§23.10.11), chosen by the owner; the app lists admins first. The receiver MUST accept in its own app; ownership is never given without consent.
   - (b) **Offer.** The owner's app sends the candidate an ordinary 1:1 E2E message (§20.7(4) form) containing `OwnerHandoverOffer { type 0x01, community_ref [32]u8, community_sk [32]u8 (the community identity secret key that signs the roster, §20.1), owner_epoch u64, expires_ms u64 (≤ 7 d) }`. An unanswered offer expires; the owner can withdraw it by sending a newer offer to someone else.
   - (c) **Accept and publish.** On accept, the new owner's app publishes a community moderation action (§23.10.11(2)) with new **`action = 4 OWNER_HANDOVER`**: `member_pk` = new owner, `actor_pk` = current owner, `reason_code = 0`, `duration_s = 0`, `epoch` = `owner_epoch` (counted per community, strictly higher than the last applied handover), `note` = the new owner's 64-byte Ed25519 acceptance signature over `"OMCP-owner-accept-v1" || 0x00 || community_ref || member_pk || owner_epoch` (`note_len = 64`). The old owner signs `sig` when making the offer and includes the signed action in it, so the new owner can publish it while the old owner is offline. It then publishes a roster, signed with the community key, naming itself `owner` and the old owner `admin` (or removing the old owner if it is leaving).
   - (d) **Validation (MUST).** Receivers accept an owner change in a roster only together with a valid action 4 whose `actor_pk` is the owner after the last applied handover (or the founder) and whose acceptance signature verifies with `member_pk`. A roster naming a different owner without such an action is rejected, so a former owner who kept a copy of the community key cannot take ownership back.
   - (e) **After the handover.** The old owner's app MUST erase its copy of `community_sk`. Removal limits, the Removals view and owner alerts (§23.10.11(5)) move to the new owner. If the community is Listed, the old listing fails validation (§20.8(8)(b)); the new owner's app re-announces with a new `seq` once the 24 h limit of §20.8(5)(c) allows, or leaves it delisted if the new owner prefers.
   - (f) **Leaving without a successor.** If no member can or will accept, the owner may still leave after a plain warning: "Nobody will be able to change roles in this community after you leave." The community becomes **ownerless**: posts, existing admins and moderators keep working, no one can grant roles or hand it over, and the owner's app sends a delist first if it was Listed. Ownerless communities are never announced.
   - (g) **Honest limit.** A former owner who kept the community key against the rules could still sign capability tokens or non-owner roster entries until a later version adds community key rotation; it cannot reclaim the `owner` role (item (d)). If the owner's phone is lost with no backup, the community stays ownerless as in (f); there is no recovery in v1.0.
3. **Acceptance tests.**
   - MR72: In a private group, a commit adding or removing another member by a non-admin is rejected; a leave or Update commit by any member is accepted; a group without `omcp_group_admins` treats leaf 0 as admin.
   - MR73: When the last admin leaves, the app requires choosing a new admin and commits the admin change before the leave proposal; if the admin list becomes empty anyway, all members pick the same lowest-leaf member as admin and it commits itself into the list; a one-member group is deleted on leave and sends nothing.
   - MR74: An owner handover takes effect only with a valid action 4 carrying the current owner's signature and the new owner's acceptance; a roster naming a new owner without it, a handover signed by a former owner, or one with a non-increasing `owner_epoch` is rejected; the old owner's app erases `community_sk`; the old listing fails validation until the new owner re-announces.
   - MR75: An owner who leaves without a successor sends a delist first if Listed; the community stays usable with its existing admins and moderators and is never announced again.
   - MR76: A v1.0 app sends `join_policy = 1` and `has_icon = 0`; it stores, relays and shows an announcement with `join_policy = 2` or `has_icon = 1`, shows no Join button for the former (invite text only), fetches no icon and shows the default icon.

### 23.8 Clarifications (v0.2.1)
1. **Hash matching scope and governance.**
   - Scope: only media posted to **public** communities, channels and livestreams (content anyone can join and see), and only when it's displayed or re-shared. Private 1:1 chats, private groups and private communities are **never scanned**, and no content leaves the device for scanning.
   - List: perceptual and cryptographic hashes from a recognised hash-sharing programme for known CSAM, which needs an agreement with the provider. The list is distributed as a signed, versioned set (Ed25519, signature checked by the client, a transparency log of list versions), and updates are fetched weekly over the gateway or gossip.
   - False positives: a match blurs the media locally and stops it from being re-shared. Nothing is reported automatically. The match goes to the community admins and the default moderator service, which is run by a human, for review. Only a confirmed match leads to the Reported or Restricted state, and only on that basis.
   - Legal basis: **open item, needs legal review before launch** (EU rules on voluntary CSAM detection, the DSA, GDPR, and app store requirements). The feature ships behind a flag until that's done.
2. **Where reputation lives.** Each client computes R and report scores **locally** from signed attestations (endorsements, reports, counter-votes, retention counts). Attestations are gossiped like envelopes, using the community's mailbox tag, and stored with the community's state. For public discovery, any node may publish a signed `REPUTATION_SUMMARY` (counts plus a Merkle root over the attestations), and clients can check it against a sample of attestations. No single server is trusted.
3. **Sybil-resistant jury.** Candidates are drawn from a pool of accounts with an EigenTrust score above the 50th percentile, an age of ≥ 90 days and verified keys, excluding anyone who has reported, endorsed or counter-voted on the community. Selection uses a VRF seeded with `H(appeal_id || epoch_beacon)`, where the beacon is the hash of the latest gossip-checkpointed epoch. At most 1 juror is taken per trust cluster (§23.2 independence). Anyone can verify the selection and the signed votes.
