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
- **On the wire today:** neither the Android (`io.github.webrtc-sdk:android`) nor the iOS (`webrtc-sdk/Specs`) SDK exposes a raw frame-transformer API; both expose the frame cryptor (insertable streams, AES-GCM per encoded frame, key ring, shared-key mode). Both apps feed it the SFrame base key (key index = KID mod 16, salt `OMCP-live-frame-v1`, HKDF), so every encoded frame is E2EE and only stream members can decrypt; STUN and any network observer see ciphertext. This is the frame cryptor's format, **not** the RFC 9605 byte layout. Switching to the Rust SFrame codec needs a custom `FrameTransformer` in the SDK (native code) and is tracked as follow-up.
- **Relaying viewers re-encrypt.** WebRTC's track forwarding decodes and re-encodes, so a relay decrypts (it is a member and holds the key anyway) and re-encrypts for its children. Pass-through forwarding of ciphertext needs the same custom transformer (and an SFU-style RTP forwarder); `sframe.rs` is ready for it (`relay_forwards_ciphertext_unchanged`).

## STUN / TURN and NAT limits
The spec allows public STUN and no TURN. `STUN_SERVERS = ["stun:stun.l.google.com:19302"]`; TURN is off and no server of ours exists, so no relay sees media. Consequences:
- Works: same Wi-Fi/LAN, most home routers (full-cone / port-restricted NAT), many mobile networks where one side is not symmetric.
- Fails: symmetric NAT or carrier-grade NAT on both ends, UDP-blocking corporate/public Wi-Fi. ICE then fails (or nothing connects within 20 s); the viewer leaves the tree (its children are re-parented), shows "Video couldn't connect directly (this network blocks it and Hopla uses no relay servers). You're still hearing the audio over the mesh." and keeps the mesh audio. A full tree shows "Video is full right now…".
- STUN reveals each phone's public IP:port to the STUN server and to tree neighbours (inherent to P2P).

## Size (Android)
WebRTC is no longer behind a flag: `io.github.webrtc-sdk:android:150.7871.04` (BSD-3-Clause) is always included (owner decision: budget ~35 MB). The sideload APK is arm64-v8a only (other WebRTC ABIs dropped, native libs stripped). Measured arm64 sideload APK: see the PR / release note for the exact figure (≈ 33–34 MB with libs stored uncompressed). iOS links `WebRTC.xcframework` (~12 MB arm64) via SwiftPM (`webrtc-sdk/Specs` 150.7871.04).

## Deferred
- RFC 9605 byte layout on the wire and pass-through relaying (custom frame transformer, see above).
- Audio in the WebRTC tree (today audio is mesh-only, so an online viewer outside Bluetooth range of the mesh gets video without sound).
- Wi-Fi Direct / LAN relay, recording.

## Risks
- Real-hardware latency: BLE connection intervals and multi-hop scheduling are simulated, not measured; LV1 needs an on-device run.
- MediaCodec Opus availability differs by vendor (Android 10+ has it in AOSP).
- NAT traversal without TURN will fail for some carrier-grade/symmetric NATs (graceful fallback to mesh audio).
- Video and iOS audio are not device-tested yet: iOS↔Android interop (frame cryptor parameters, Opus via AudioToolbox vs MediaCodec) is matched by construction only.
