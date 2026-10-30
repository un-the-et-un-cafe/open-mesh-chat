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
| iOS Live tab (start/stop/listen/leave, mic asked on first Go live) | Done; iOS audio I/O deferred | `apps/ios/OpenMeshChat/Screens/LiveViews.swift` |

FLAG_SECURE and the privacy rules are unchanged; the Live screen lives inside the same secured activity.

## Codec
Android uses the platform Opus codec through `MediaCodec` (`c2.android.opus`, AOSP, Apache-2.0, libopus BSD-3-Clause). No codec is bundled. MediaCodec exposes no in-band FEC decode, so a lost frame plays as silence (PLC); the FEC bit is still set on encode for future decoders.

## STUN / TURN
The spec allows public STUN and no TURN. `STUN_SERVERS = ["stun:stun.l.google.com:19302"]`. TURN is off: no relay server sees media. Consequence: peers behind symmetric NAT on both sides cannot connect for video and fall back to audio over the mesh.

## WebRTC video: behind a flag
`io.github.webrtc-sdk:android:150.7871.04` (BSD-3-Clause) adds ~12.3 MB of arm64 native code (stored uncompressed) — the APK would go from ~20 MB to ~32 MB, over the 25 MB budget. So it is behind `-POMCP_LIVE_VIDEO=true` (default false), which adds the dependency and `BuildConfig.LIVE_VIDEO`. The default build ships audio only.

## Deferred
- WebRTC video peer connections and SFrame (core tree, signalling and mode switching are ready).
- iOS audio capture/playback (AVAudioEngine + Opus AudioConverter) — UI and core ship; needs device testing.
- Wi-Fi Direct / LAN relay, recording.

## Risks
- Real-hardware latency: BLE connection intervals and multi-hop scheduling are simulated, not measured; LV1 needs an on-device run.
- MediaCodec Opus availability differs by vendor (Android 10+ has it in AOSP).
- NAT traversal without TURN will fail for some carrier-grade/symmetric NATs.
