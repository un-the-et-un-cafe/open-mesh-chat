# Hopla — OMCP Android beta (M5)

**Experimental · not production-ready · untested on real hardware.**
Consumer name: **Hopla**. Protocol / crates stay **OMCP**. Package id `chat.omcp.app`.

minSdk 31 (Android 12+) · Kotlin · Jetpack Compose · Material 3 · Rust core via UniFFI.

## Architecture

```
Compose UI ── ViewModels ── MeshRepository ── OmcpCore (interface)
                                   │              ├─ RustOmcpCore  → chat.omcp.ffi (UniFFI) → libomcp_ffi.so → omcp-core
                                   │              └─ FakeOmcpCore  (previews / JVM tests, no crypto)
                                   └─ BleMeshTransport (GATT server+client) in MeshService (foreground, connectedDevice)
```

- **`crates/omcp-ffi`** — safe UniFFI API over `omcp-core`: identity from 24-word phrase, QR contact
  add/preview with safety numbers, offline queue, per-connection `MeshLink` (hybrid Noise IK + ML-KEM
  handshake, M2 link-AEAD fragments), path status, gateway policy, 50 MB file cap, encrypted backup.
  `omcp-core` keeps `#![forbid(unsafe_code)]`; the only unsafe is UniFFI's generated scaffolding.
- **BLE** (PROTOCOL §8.1): every phone advertises the service and hosts `Inbox` (write) / `Outbox`
  (notify) / `Control` (read → 13-byte discovery ad). The lower PeerID dials (central = Noise
  initiator). The §8.1 placeholder UUID is not valid hex, so the beta uses
  `4f4d4350-000{1..4}-4c01-9e00-000000000001` (`OmcpGatt`). Discovery ad rides in the scan response
  as manufacturer data (id 0xFFFF, testing). PDU framing (`0x01` handshake chunks, `0x02` link
  fragments, ≤ 512 B) is documented in `crates/omcp-ffi/src/lib.rs`.
- Mutual QR add is required: a responder refuses initiators that aren't contacts.

## Onboarding & backup (apps/REQUIREMENTS.md “Onboarding & recovery backup (v0.1.1)”)

Welcome → Profile → **Secure your account** (Google Drive and/or Write it down + 3-word quiz, or
“Remind me later” = 24 h with banner, then blocked) → Done (own QR + “Find people nearby”).
Restore-on-reinstall prompt when a cloud backup is found; 24-word phrase restore always available.
Settings › Backup: status/last date, change password (re-upload), delete cloud backup, view phrase
behind BiometricPrompt / device credential (FLAG_SECURE while words are shown).

Backup blob = Rust core (`omcp_core::backup`): Argon2id (64 MiB, t=3, p=1) + XChaCha20-Poly1305,
AAD `OMCP-Backup-v1‖suite‖created_at`, CBOR `{v,kdf,salt,nonce,ct,created_at,device_hint}`; plaintext
contains entropy + contacts + verification state. Password: ≥ 8 chars, zxcvbn ≥ 3.

**Google Drive backup** (`backup/DriveAppData.kt`, `backup/GoogleDriveAuth.kt`): Google Identity
Services `AuthorizationClient` (scope `drive.appdata` only, consent shown by `MainActivity`) +
Drive v3 REST on `appDataFolder` (create / overwrite / download / delete one file). It is live
when the build has `GOOGLE_OAUTH_CLIENT_ID` (Gradle property or env var, never committed); see
[`../GOOGLE_DRIVE_BACKUP.md`](../GOOGLE_DRIVE_BACKUP.md) for creating the OAuth client. Without it,
Drive shows as unavailable and debug builds use an in-memory “simulated Drive”. TODO: WorkManager
retry when offline.

Onboarding screens are plain layouts (`ui/components/Plain.kt`) pending design v0.1.2. Contact-add,
QR and file-confirm follow DESIGN_SPEC §5.19–§5.24.

### Acceptance tests (Android)

| Test | Status |
|------|--------|
| O1 ≤ 4 screens to chats with cloud backup | JVM test (`OnboardingViewModelTest`); timing ≤ 60 s not measured on device |
| O2 blob has no plaintext phrase/key | Rust tests (`backup::tests`, `omcp-ffi`) + Kotlin bridge test |
| O3 reinstall → restore from Drive | Logic tested with in-memory store; Drive REST tested against a fake API (`DriveAppDataClientTest`); **not run against real Google** (needs an OAuth client + device) |
| O4 wrong password fails, rate-limited, phrase fallback | Rust + JVM tests (`RetryLimiter`) |
| O5 delete removes remote record | JVM tests (in-memory store + fake Drive API); real Google check needs an OAuth client |
| O6 skip → banner, blocked after 24 h | JVM test |
| iCloud parts of O2/O3, platform-key mode UI | Deferred to iOS (M6); core supports `platform-key` KDF |

## What works vs. stubbed

| Area | Status |
|------|--------|
| Identity, phrase, encrypted backup, restore | Real (Rust) |
| QR show (ZXing) / scan (zxing-android-embedded), preview + safety number, verify / chat without verifying, mismatch block | Real |
| 1:1 text over BLE GATT, offline queue flush on connect | Real code path; verified Kotlin↔Rust in JVM test with simulated PDUs, **not on phones** |
| Notifications | Previews off by default (“New Hopla message”) |
| Gateway settings | Policy from core (500 MB/day, Wi-Fi only, metered warns); no real gateway transport |
| File send / receive | Picker + 50 MB check before reading; Rust encrypts 64 KiB chunks (`queue_file`), transport streams them (`more_pending`), pause/auto-resume on reconnect, BLAKE3-verified receive saved to app storage, v0.1.1 bubble states. Verified Kotlin↔Rust in JVM; **not on phones** |
| Android ↔ iPhone discovery | Scan filters: OMCP service UUID, OMCP manufacturer data, OMCP local name, Apple overflow-area data; iPhones are probed via GATT service discovery + Control read (`ScanClassifier`). Not tried on hardware |
| Groups (MLS), NFC, session reset / block, Room persistence of messages | TODO |
| Communities / Live | “Coming in v0.2” empty states |

## Build

Needs JDK 17, Android SDK 35, NDK 27, Rust + `cargo-ndk`, targets `aarch64-linux-android x86_64-linux-android`.

```bash
cd apps/android
export ANDROID_HOME=~/android-sdk ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
./scripts/build-rust.sh           # .so + Kotlin bindings into app/build/rust (git-ignored)
./gradlew assembleDebug testDebugUnitTest
```

`RustBridgeTest` loads the host `target/debug/libomcp_ffi.so` through JNA (skipped if absent).
App icon is a **placeholder** (redesign pending).

## Notifications (v0.1.2)
- Default ON: sender + preview (N1). Global **Private notifications** (You tab) and per-chat toggle (Security centre / chat info) show only "Hopla: New message" with no sender, preview or avatar (N2). **Hide on lock screen** uses `VISIBILITY_SECRET`; otherwise `VISIBILITY_PRIVATE` with a generic public version.
- Push: `notify/PushReceiver.kt` defines a content-free `PushPayload` (only the opaque mailbox id; extra keys are rejected) (N3). `StubPushReceiver` stands in for FCM since there's no Firebase project yet, so no `FirebaseMessagingService` is registered.
- Tests: `NotificationsTest` (N1–N3, policy level). N2 history/lock screen is covered by the policy plus the public version; it hasn't been checked on a device.
- Not done yet: real FCM integration and gateway token registration; avatars in notifications; quiet hours, per-chat mute and per-community levels; call, mention and livestream notifications.
- Deferred to iOS (M6): APNs content-free push, the Notification Service Extension that decrypts on-device, `hiddenPreviewsBodyPlaceholder`, and iOS N1–N3.

## Burn after read (v0.1.3)
- **Per-message**: long-press Send to toggle 🔥 for the next message. **Per-chat**: Security centre › Burn after read, with Off / On open / 10 s / 1 min / 1 h, plus an unread TTL of 1 h / 24 h / 7 d.
- **Text**: shows blurred as "Tap to read" and opens in a full-screen viewer with a countdown ring. When the timer ends it's deleted. The sender deletes its copy once the `OPENED` receipt arrives and its timer ends.
- **View-once image**: opens once. A second tap shows "🔥 Photo opened". Save, forward, quote and share are disabled (`ChatMessage.canShare`).
- **FLAG_SECURE**: the viewer is a `Dialog` with `SecureFlagPolicy.SecureOn`, on top of the app-wide block (B3, updated: burn content can never be captured, per SS1).
- **Storage**: all burn rules live in Rust (`omcp-core::burn` through `omcp-ffi`): validation, timers, unread expiry, single use, `OPENED` and crypto-shredding. Kotlin only holds UI state, and `ChatMessage.text` stays empty for burn messages.
- **Wire**: every send uses `InnerV1` (outer content type `0x02`), with the encrypted `burn` header and inner content type. Opening a message queues an encrypted `OPENED` receipt. When the sender gets it, its copy is shredded in Rust (view once or media), or its timer starts.
- **Exclusions (B5)**: notifications always use the generic text. Backups exclude burn content: `allowBackup=false`, and the cloud backup holds only the phrase and contacts. There's no search index yet, so nothing is indexed. The honest-copy line is shown in the viewer and in chat info.
- **Tests**:
  - `RustBridgeTest`: two Rust nodes behind two repositories. Covers timer burn plus `OPENED` deleting on both sides, view-once image (single use, placeholder, sender shredded on `OPENED`), and unread-TTL expiry on both sides.
  - `BurnAfterReadTest`: UI-only checks and B5.
  - Rust `omcp-core::burn` and `omcp-ffi` burn tests.
- **Stubs**:
  - Media transfer from the app stays queued: the picker path doesn't send bytes. `queueMedia` (≤ 16 KiB) exists in the core and is used by tests only.
  - View-once video playback is a placeholder (no Media3).
  - Relays: the outer `expires_at` applies to gateway `OpaqueEnvelope.expiry_ms`, but the gateway transport isn't wired on Android yet. Over BLE, `EnvelopeV1` has no expiry field, so unread expiry is enforced by both endpoints.
  - The 256-member cap isn't enforced in the UI, because there are no groups in the Android beta.
- **Deferred to iOS (M6)**: iOS B1–B5. The 📸 screenshot notice has been dropped: screenshots are now blocked app-wide (see below).

## Product decisions (2026-10-24)
- **Private calls**: `NotificationPolicy.callContent` shows "Hopla · Incoming call" with no caller name. This is policy and tests only, since there are no calls yet.
- **Per-chat override**: the Security centre switch sets a per-chat override in both directions. Off shows the sender and preview even when global Private notifications is on.
- **Hide on lock screen**: uses `VISIBILITY_PRIVATE` with a public version "Hopla · New message" (not blank). Otherwise `VISIBILITY_PUBLIC`.
- **No screenshots anywhere**: `ScreenSecurity.Callbacks`, registered in `OmcpApplication`, sets FLAG_SECURE on every activity at pre-create, including the third-party QR scanner. On Android 13+ it also calls `setRecentsScreenshotEnabled(false)`. Compose dialogs and sheets inherit FLAG_SECURE, and nothing clears it. `ScreenSecurityTest` covers SS1, SS2 and B3 at the source and hook level. Black screenshots, recordings and casting on Android 10–15 still need a device check.
- **Deferred to iOS (M6)**: secure-capture layer, app-switcher blur cover, `UIScreen.isCaptured` cover, and iOS SS1 and SS2.

## Roadmap
- **v0.3 Hopla Nearby** (not started): dating, plus a Nearby session (Android foreground service with an ongoing notification, 30/60/120 min) and area mode over the gateway geohash. Acceptance NX1–NX3: first card < 1 s from cache, an iOS session lasting 60 min while locked, and an area-mode match notified within 5 min.

## Mesh relay

Multi-hop store-and-forward relay (PROTOCOL §22) is described in [docs/MESH_RELAY.md](../../docs/MESH_RELAY.md).
The relay is always on with an automatic 500 MB cap (REQUIREMENTS RL1–RL4): there is no relay switch or cap
setting, and Settings › Mesh relay shows only "Storage used for relaying: X MB". The app reports free storage
to the core on every mesh tick so the cap is lowered under 10 % free.
