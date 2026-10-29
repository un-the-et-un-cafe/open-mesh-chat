# Live v0.2

Scope: REQUIREMENTS "Live", PROTOCOL §20.3 (0x50 RTC signalling), DESIGN §5.12 (live tab).

## What ships
| ID | Status | Where |
|---|---|---|
| Offline audio, Opus 12 kbit/s (6 kbit/s fallback), 20 ms frames, ≤3 hops, drop-not-queue, chat first | Done | `omcp-core/src/live/{audio,relay}.rs`, `omcp-ffi/src/mesh.rs` |
| Per-stream key, HKDF + XChaCha20-Poly1305, rotated when a listener is removed; key sent in the MLS-encrypted invite | Done | `live/mod.rs`, `live/host.rs` |
| Never stored by Relay; frames expire after 5 s | Done | `live/relay.rs` (frames bypass the relay store) |
| Jitter buffer (600 ms target), loss concealment, late join | Done | `live/jitter.rs` |
| Viewer tree: host fan-out 6, viewer fan-out 4, re-parenting, parked viewers | Done (core) | `live/tree.rs` |
| RTC signalling sealed as 0x50 | Done (core) | `live/signal.rs` |
| Video → audio fallback when video stalls ≥2 s | Done (core) | `live/mode.rs` |
| LV1 3 hops, <6 s, <5 % loss | Sim test: worst listener 3.6 % loss, ~4.1 s max delay; hop 4 gets nothing | `live/sim.rs` |
| LV2 chat p95 rises ≤20 % during a live | Sim test + ffi test (chat delivered during live) | `live/sim.rs`, `omcp-ffi/src/tests.rs` |
| LV3 switch to audio ≤5 s | Sim test (mode selector) | `live/sim.rs` |
| LV4 non-member relay can't decrypt | Sim + ffi 3-hop test | both |
| Android Live tab, mic asked on first Go live, MediaCodec Opus, AudioRecord/AudioTrack | Done | `apps/android/.../live/`, `ui/screens/LiveScreens.kt` |
| iOS Live tab (start/stop/listen/leave, mic asked on first Go live) | Done | `apps/ios/OpenMeshChat/Screens/LiveViews.swift` |
| iOS mesh audio: AVAudioEngine + AudioToolbox Opus (48 kHz mono, 20 ms, 12 kbit/s, raw packets like Android) | Done (CI build; not device-tested) | `apps/ios/OpenMeshChat/Live/LiveAudioIO.swift` |
| Online video: WebRTC viewer tree (host 6, viewer 4), STUN only, 0x50 signals relayed by the host | Done (Android + iOS) | `live/video.rs`, `omcp-ffi/src/live.rs`, `apps/*/…/Live*` |
| SFrame (RFC 9605, AES_128_GCM_SHA256_128) key schedule + frame format, RFC test vectors | Done (core) | `live/sframe.rs` |
| Video E2EE on the wire (WebRTC frame cryptor, key from SFrame base key) | Done | `WebRtcPeers.kt`, `WebRtcPeers.swift` |
| No direct path / tree full → audio over the mesh, with a message | Done | `LiveVideoController` (Android + HoplaKit) |

FLAG_SECURE and the privacy rules are unchanged; the Live screen lives inside the same secured activity.

## Live inside a community
- **Communities** (minimal v0.2 cut, `omcp-ffi/src/community.rs`): one MLS group per community (OpenMLS, ≤ 32 members in this build). The founder invites contacts. An invitee accepts and sends a KeyPackage, then gets the Welcome, and the other members get the Commit. Control messages are pairwise E2EE app messages (`application/omcp-community`, no chat row). Only the founder adds members.
- **Key (PROTOCOL §20.3, "MLS epoch exporter + per-stream random").**
  ```text
  exporter = MLS-Exporter("OMCP-live-v1", community_id ‖ stream_id, 32)
  secret   = HKDF-SHA-256(salt = nonce[32], ikm = exporter, info = "OMCP-community-live-v1" ‖ community_id ‖ stream_id)
  ```
  Epoch 0 of the stream key is this secret; the audio key and the SFrame base key are derived from it exactly as before. Every member derives it, and outsiders and other streams can't (`live::community` tests). The label is domain-separated from the mailbox-tag exporter.
- **Announcement.** `CommunityLive {community_id, stream_id, nonce, title, started_ms, video, max_hops, live, host_key}` is an MLS application message of the community group, so MLS authenticates the sender and only members can read it. `host_key` must hash to the sender's PeerID. Members forward it unchanged to their own contacts in the roster (dedup by hash), so members who aren't the host's contacts still hear about it. The end of the stream (`live = false`) travels the same way.
- **Listing and notification.** `live_community_streams(id)` lists the active lives, and `CommunityInfo.live_count` counts them. `live_take_started()` feeds a lock-screen notification that always uses the generic text ("Hopla · New message"): no title, host or community name (DESIGN §5.36 says "livestream starts in Private"; this build uses the generic text in every mode, per the owner brief).
- **Contact-picked lives stay.** REQUIREMENTS still says "MLS group of the community **or chat**". The epoch-0 key there is random and sent in each 1:1 MLS chat.
- **Remove viewer.** The host taps **Remove** next to a member. `live_remove_listener` rotates to a *random* secret at `key_epoch + 1`, which the removed member can't derive from the exporter. It travels as a host-signed `Rekey {community, stream, epoch, secret, excluded[], host_key, sig}` inside pairwise E2EE chats, forwarded member to member and never to the excluded PeerIDs. The list of excluded viewers is cumulative and signed, so nobody can strip it. The host also drops the viewer from the video tree. Leaving doesn't rotate: a leaver is still a member and could derive epoch 0 anyway, so rotating on leave buys nothing in a community.
- **Moderation.** See docs/MODERATION.md "Lives". A removed community can't start or join lives, and its announcements aren't applied or forwarded. A live takedown (`TAKEDOWN_LIVE`, same team key) stops the host, the viewers and the relays.

## Host frame authentication
Members hold the stream key, so AEAD alone can't stop a relaying member from injecting frames.
- **Mesh audio (done).** Packet version 2 appends `sig = Ed25519(host identity key, "OMCP-live-audio-sig-v1" ‖ header-without-hop ‖ nonce ‖ ciphertext)`. Listeners need the host key from the invite or announcement, and they check it against the sender's PeerID (`BLAKE2b-256(identity key)`) or the pinned identity key. Unsigned packets, packets signed by someone else and tampered packets are dropped. Relays bump the hop byte, which is outside the signature. Overhead: 64 B per 100 ms packet, that is **5.1 kbit/s on top of 12 kbit/s Opus** (≈ +29 % on the BLE link), plus one Ed25519 sign per packet on the host and one verify per packet per listener (≈ 50 µs on a phone, 10 per second). Batching (one signature over the hashes of ten packets, once per second) would cut this to ≈ 0.8 kbit/s but adds 1 s of latency before playout; we chose per-packet signing.
- **Video (not done).** Encoded video frames are only reachable inside the WebRTC frame cryptor, which exposes no hook for our own signature. Signing key-frame hashes needs the frame-transformer work below. Until then a relaying viewer, who is a member and holds the key, could inject video into its own subtree (not into the audio). Open risk.

## SFrame pass-through plan
Goals: (1) RFC 9605 SFrame bytes on the wire from our Rust codec (`live/sframe.rs`), (2) relaying viewers forward ciphertext without decoding and re-encoding, (3) host signatures on key frames.

| Step | Android (`io.github.webrtc-sdk:android` 150.7871.04) | iOS (`WebRTC.xcframework` 150.7871.04) | Fork? |
|---|---|---|---|
| A. Tap encoded frames | Custom `VideoDecoderFactory` that wraps the built-in decoder. `VideoDecoder.decode(EncodedImage)` sees every encoded frame after the frame cryptor has decrypted it. | Custom `RTCVideoDecoderFactory` with `RTCVideoDecoder.decode(RTCEncodedImage…)`. | No |
| B. Re-send encoded frames to children | Custom `VideoEncoderFactory` with a "pass-through encoder" that ignores the input `VideoFrame` and emits the queued `EncodedImage` (driven by a dummy capturer at the same frame rate). | Same with `RTCVideoEncoder`. | No, but fragile (see risks) |
| C. Verify host key-frame signatures | In the decoder wrapper: hash each key frame (BLAKE2b-256), check it against a host-signed batch of key-frame hashes sent as a 0x50 signal every second, and drop the subtree's video on a mismatch. | Same. | No |
| D. RFC 9605 bytes on the wire | Replace the frame cryptor with our own `FrameTransformerInterface` that calls Rust `sframe::protect/unprotect` over a C ABI. The Java and ObjC wrappers only expose the frame cryptor, and the SDK's `.so` doesn't export the C++ symbols. | Same (the ObjC API exposes no `RTCFrameTransformer`). | **Yes**: patch `sdk/android` and `sdk/objc` to expose `RtpSender/RtpReceiver.setFrameTransformer` |

Effort: A–C without a fork ≈ 6–8 dev-days per platform plus 3–4 days of device testing. D needs a fork: ≈ 2–3 weeks (libwebrtc checkout ≈ 30 GB, about 2 h per ABI build, CI caching, then keeping up with upstream releases).
Risks: (B) WebRTC's RTP packetizer needs codec-specific info (VP8 picture id, temporal layers) that the Java and ObjC `CodecSpecificInfo` don't carry. Pass-through may break keyframe requests (PLI/FIR) and bitrate adaptation, because children's PLIs can't be answered without asking the host. Simulcast layers can't be re-chosen per child. (D) A fork means we own security updates for libwebrtc.
**Decision: not implemented in this PR.** A–C are feasible without a fork but can't be tested here (no devices). They are the next step once a device test bench exists. D stays deferred.

## Codec
Android uses the platform Opus codec through `MediaCodec` (`c2.android.opus`, AOSP, Apache-2.0, libopus BSD-3-Clause). No codec is bundled. MediaCodec exposes no in-band FEC decode, so a lost frame plays as silence (PLC); the FEC bit is still set on encode for future decoders.

## Video architecture (online)
- **Tree.** The host keeps the `ViewerTree` (Rust, `live/video.rs`): host fan-out 6, viewer fan-out 4, new viewers attach to the shallowest free slot, a leaving viewer's children are re-parented, a viewer with no slot is *parked* (audio only).
- **Signalling (0x50, PROTOCOL §20.3).** A viewer with internet sends `Join` to the host. The host answers `Assign` to the chosen parent (or to itself), the parent creates the WebRTC offer for that child. Offer / answer / ICE are `RtcSignal`s sealed with the stream key (XChaCha20-Poly1305) and sent as `application/omcp-rtc-signal` E2EE chat payloads (no chat row). Viewers need not be contacts of each other: every signal goes viewer → host → viewer, and the host only relays between tree neighbours. Peers are named by host-assigned ids; 64 zeros = host. FFI: `live_video_join`, `live_send_signal`, `live_take_signals`, `live_video_drop`, `live_media_key`.
- **Media.** One `RTCPeerConnection` per tree edge, unified plan, video only (640×480 @ 24 fps front camera on the host). A relaying viewer adds the track it receives to its children's connections. Audio stays on the mesh (Opus), so viewers who drop out of video keep hearing the stream.
- **Clients.** `LiveVideoController` (Android `live/`, iOS `HoplaKit/LiveVideo.swift`, same logic and tests) turns signals into connections, enforces 6/4 locally, rotates the media key, and falls back. `WebRtcPeers` (Kotlin / Swift) is the WebRTC side.

## End-to-end encryption of video (SFrame)
- `live/sframe.rs` implements RFC 9605 for cipher suite `AES_128_GCM_SHA256_128` (0x0004): header encoding (KID/CTR), key/salt derivation (`SFrame 1.0 Secret key|salt` labels), nonce = salt ⊕ CTR, AAD = header ‖ metadata. Tests pass the RFC 9605 Appendix C header vectors and the C.3 encryption vector for 0x0004.
- The SFrame base key is `HKDF-SHA-256(salt = "OMCP-live-sframe-v1", ikm = stream secret, info = stream_id ‖ key_epoch)` (16 bytes), KID = key epoch. It rotates with the stream key (listener removed → new epoch → new media key). Domain-separated from the mesh audio key.
- **On the wire today:** neither the Android (`io.github.webrtc-sdk:android`) nor the iOS (`webrtc-sdk/Specs`) SDK exposes a raw frame-transformer API; both expose the frame cryptor (insertable streams, AES-GCM per encoded frame, key ring, shared-key mode). Both apps feed it the SFrame base key (key index = KID mod 16, salt `OMCP-live-frame-v1`, shared-key mode, PBKDF2 frame-key derivation on both sides), so every encoded frame is E2EE and only stream members can decrypt; STUN and any network observer see ciphertext. This is the frame cryptor's format, **not** the RFC 9605 byte layout. Switching to the Rust SFrame codec needs a custom `FrameTransformer` in the SDK (native code) and is tracked as follow-up.
- **Relaying viewers re-encrypt.** WebRTC's track forwarding decodes and re-encodes, so a relay decrypts (it is a member and holds the key anyway) and re-encrypts for its children. Pass-through forwarding of ciphertext needs the same custom transformer (and an SFU-style RTP forwarder); `sframe.rs` is ready for it (`relay_forwards_ciphertext_unchanged`).

## STUN / TURN and NAT limits
The spec allows public STUN and no TURN. `STUN_SERVERS = ["stun:stun.l.google.com:19302"]`; TURN is off and no server of ours exists, so no relay sees media. Consequences:
- Works: same Wi-Fi/LAN, most home routers (full-cone / port-restricted NAT), many mobile networks where one side is not symmetric.
- Fails: symmetric NAT or carrier-grade NAT on both ends, UDP-blocking corporate/public Wi-Fi. ICE then fails (or nothing connects within 20 s); the viewer leaves the tree (its children are re-parented), shows "Video couldn't connect directly (this network blocks it and Hopla uses no relay servers). You're still hearing the audio over the mesh." and keeps the mesh audio. A full tree shows "Video is full right now…".
- STUN reveals each phone's public IP:port to the STUN server and to tree neighbours (inherent to P2P).

## Size (Android)
WebRTC is no longer behind a flag: `io.github.webrtc-sdk:android:150.7871.04` (BSD-3-Clause) is always included (owner decision: budget ~35 MB). The sideload APK is arm64-v8a only (other WebRTC ABIs dropped, native libs stripped). Measured arm64 sideload APK (`.debug` package, libs stored uncompressed, R8 off): 33.7 MB (33,739,342 bytes) — WebRTC .so 12.3 MB, omcp_ffi 7.3 MB. iOS links the same release (`WebRTC.xcframework` 150.7871.04 from `webrtc-sdk/Specs`, ~12 MB arm64) through a local SwiftPM wrapper (`apps/ios/Packages/WebRTCBinary`, pinned checksum), because the upstream manifest needs a newer PackageDescription than the CI Xcode.

## Deferred
- Video frame signatures and pass-through relaying (see "SFrame pass-through plan").
- RFC 9605 byte layout on the wire and pass-through relaying (custom frame transformer, see above).
- Audio in the WebRTC tree (today audio is mesh-only, so an online viewer outside Bluetooth range of the mesh gets video without sound).
- Wi-Fi Direct / LAN relay, recording.

## Risks
- Community MLS epochs: members must be at the founder's epoch to derive the same live key. A member who missed a Commit derives a different key and hears nothing (no error shown yet). Commits travel pairwise from the founder, with no resync protocol yet.
- Communities are limited to 32 members (OpenMLS small-group cap in this build). Only the founder can add members, and nobody can remove members yet.
- Real-hardware latency: BLE connection intervals and multi-hop scheduling are simulated, not measured; LV1 needs an on-device run.
- MediaCodec Opus availability differs by vendor (Android 10+ has it in AOSP).
- NAT traversal without TURN will fail for some carrier-grade/symmetric NATs (graceful fallback to mesh audio).
- Video and iOS audio are not device-tested yet: iOS↔Android interop (frame cryptor parameters, Opus via AudioToolbox vs MediaCodec) is matched by construction only.
