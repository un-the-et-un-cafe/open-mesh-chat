# Hopla for iOS (M6 beta)

**iOS 17+** · SwiftUI · Rust core (`crates/omcp-ffi`) through UniFFI Swift bindings.
The app is called **Hopla**; the protocol and crates stay **OMCP**. Scope matches the Android
M5 beta (`apps/android`).

## Layout

| Path | What |
|------|------|
| `HoplaKit/` | SwiftPM package, **platform-independent**: models, `OmcpCore` protocol + `FakeOmcpCore`, `MeshRepository`, `OnboardingFlow`, `BackupSettings`, `NotificationPolicy`, `PushPayload`, `ScreenCapturePolicy`, `OmcpGatt`, file/gateway policy, `RetryLimiter`. Builds and tests on Linux. |
| `OpenMeshChat/` | SwiftUI app target (display name **Hopla**): screens, `RustOmcpCore` (UniFFI adapter), CoreBluetooth transport, CloudKit/Keychain/Drive backup stores, notifications, secure-capture layer. |
| `NotificationService/` | Notification Service Extension for content-free APNs pushes (stub). |
| `OpenMeshChatTests/` | `RustBridgeTests`: the real Rust core through the Swift bindings (two nodes, QR, Noise link, burn + OPENED, iCloud key / password backups). |
| `Generated/`, `build/` | git-ignored: `OmcpFFI.swift` bindings and `OmcpFFI.xcframework`. |
| `scripts/build-rust.sh` | macOS: build `aarch64-apple-ios` + `aarch64-apple-ios-sim` static libs, generate bindings, make the xcframework. |
| `scripts/verify-linux.sh` | Linux: HoplaKit tests, then HoplaKit + generated bindings + `RustOmcpCore.swift` + `RustBridgeTests` against the host `libomcp_ffi.so`, then `swiftc -parse` of every Swift file. |
| `scripts/strings.py` | Source of truth for EN/FR strings; `--check` verifies every key used exists in both. |

## Build (macOS)

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install xcodegen
cd apps/ios
./scripts/build-rust.sh
xcodegen generate
xcodebuild -project OpenMeshChat.xcodeproj -scheme OpenMeshChat \
  -destination 'platform=iOS Simulator,name=iPhone 15' CODE_SIGNING_ALLOWED=NO build test
(cd HoplaKit && swift test)
```

CI: the `ios` job in `.github/workflows/ci.yml` (macos-latest) runs exactly this.

## Features

- **Onboarding** (plain layouts; the v0.1.2 onboarding design is not approved): Welcome → Profile →
  Secure your account → Done (O1: 4 screens). Secure: **iCloud** (CloudKit private DB record
  `OMCPBackup`; default "No password" mode seals the blob with a 256-bit key from
  `generate_backup_key()` kept in iCloud Keychain, `kSecAttrSynchronizable` +
  `kSecAttrAccessibleAfterFirstUnlock`; optional backup password → Argon2id), **Google Drive**
  (appDataFolder, password only), **Write it down** (24 words + quiz on 3 random words),
  **Remind me later** (24 h banner, then a blocking backup screen). Sealing and opening are in
  Rust (`create_backup_with_key` / `open_backup_with_key` / password variants).
- **Restore on reinstall**: detects an iCloud/Drive backup ("Welcome back"), asks for the password
  if set (rate-limited: 3 tries, then 30 s doubling to 15 min), 24-word fallback always available.
  Settings › Backup and restore: status, last backup, back up again / change password, delete the
  cloud backup (+ Keychain key), view the phrase behind Face ID / passcode.
- **Contact add** (design v0.1.1 §5.19–5.22): scan QR (AVFoundation, non-OMCP codes rejected and
  never opened), My QR (CoreImage, ECC H, brightness raised), preview sheet with the 12×5 safety
  number, **Verify & chat** / **Chat without verifying**; mismatch has no chat action. Verified
  seal only on verified contacts; unverified shows nothing.
- **BLE** (`Mesh/BleMeshTransport.swift`): same GATT as Android (`OmcpGatt` = Android
  `OmcpGatt`, cross-checked by a unit test): service `4f4d4350-0001-4c01-9e00-000000000001`,
  Inbox `…0002…` (write), Outbox `…0003…` (notify), Control `…0004…` (read: 13-byte
  advertisement), lower PeerID dials.
- **Notifications**: default ON with sender + preview; global and per-chat Private (per-chat
  override both ways); private calls show "Hopla · Incoming call"; Hide on lock screen uses a
  category whose `hiddenPreviewsBodyPlaceholder` is "New message" and hides the title. Burn
  messages never show content. APNs payloads are content-free (`PushPayload`: `mb` mailbox id +
  fixed generic alert, `mutable-content`).
- **Burn after read** entirely through the Rust core (queue with `burn` header, `open_burn` once,
  `close_burn`, `tick_burn` every 250 ms, OPENED receipts). View-once media: "🔥 Photo opened";
  no copy/share/quote; excluded from notifications and search.
- **No screenshots**: the whole window is re-parented into a secure `UITextField` canvas layer
  (blank in screenshots, recordings, mirroring), a blurred app-switcher cover when not active, and
  a cover while `UIScreen.isCaptured`. No screenshot notice. Settings › Privacy has the honest
  "second camera" copy.
- **Files**: picker confirm with the 50 MB cap read from `max_file_bytes()` (too-large state before
  the file is read or encrypted). Rust encrypts 64 KiB chunks (`queue_file`); the CoreBluetooth
  transport drains them on write/notify (`more_pending`), pauses on disconnect and resumes on the
  next link; received files are BLAKE3-verified in Rust, saved in Application Support and opened
  with Quick Look. Bubble states follow Designy v0.1.1 `file-transfer`.
- **Gateway**: cap from `default_gateway_daily_cap_bytes()`, Wi-Fi-only/metered decision from
  `gateway_decision()`; meter stays at 0 until real metering.
- EN/FR strings, placeholder icon.

## Background BLE on iOS (notes)

- `UIBackgroundModes`: `bluetooth-central`, `bluetooth-peripheral`, `remote-notification`.
- Both managers use state-restoration identifiers (`omcp.central`, `omcp.peripheral`) so iOS can
  relaunch Hopla for BLE events.
- In the background iOS strips the local name, moves service UUIDs to the overflow area, coalesces
  scan results and ignores `AllowDuplicates`; scans must name the service UUID (they do).
  Two backgrounded iPhones discover each other poorly; foreground or an Android companion nearby
  works best (copy on You › Gateway). Nearby sessions will use a Live Activity (v0.3).
- iOS peripherals can't advertise manufacturer data, so iOS serves the 13-byte discovery
  advertisement on the **Control** characteristic and advertises the local name `OMCP`. Android
  scans for the service UUID, the local name *and* Apple's overflow-area manufacturer data (what a
  backgrounded iPhone sends), then connects, discovers services and reads Control
  (`ScanClassifier.kt`). iPhone → Android works (iOS reads Control).

## Stubs / not done

- Google Drive on iOS: implemented (`HoplaKit/GoogleDrive.swift`: PKCE OAuth + Drive v3
  `appDataFolder`, tested on Linux against fakes; app side `ASWebAuthenticationSession`, device
  Keychain refresh token). Needs an iOS OAuth client in `Config/Secrets.xcconfig` (git-ignored),
  see [`../GOOGLE_DRIVE_BACKUP.md`](../GOOGLE_DRIVE_BACKUP.md); unavailable without it. Not run
  against real Google.
- Notification Service Extension: validates the payload and shows "Hopla · New message"; mailbox
  fetch + decrypt waits for the M4 gateway mailbox API and an App Group keychain.
- APNs token is stored locally only (no gateway registration yet).
- Calls don't exist yet; `NotificationPolicy.callContent` is the policy the call stack will use.
- NFC, PeerID entry, Nearby: not in this beta. File transfer has no end-to-end ack yet (sender
  shows "Sent" once the last chunk is on the link).
- Message history is in memory (as on Android); contacts and settings persist.

## Mesh relay

Multi-hop store-and-forward relay (PROTOCOL §22) is described in [docs/MESH_RELAY.md](../../docs/MESH_RELAY.md).
