# OMCP Smartphone Apps — Build Requirements

Derived from PROTOCOL.md §13. Implementers (Proggy) treat this as the product checklist; Pipely owns CI, signing prep, and GitHub release automation.

## Platforms

- Android 12+ (API 31+), Kotlin UI, Rust core via JNI
- iOS 17+, SwiftUI, Rust core via FFI

## Languages & accessibility
- UI languages: English, French and Hebrew (RTL, with full layout mirroring). Hebrew copy is a draft until reviewed by a native speaker.
- Light and dark themes are a matched pair, and both meet WCAG 2.2 AA (text 4.5:1, icons and UI 3:1). Disabled controls are exempt.
- L1: Every screen renders correctly in Hebrew RTL (navigation, bubbles, chevrons and swipes mirrored; media, numbers and the clock not mirrored).
- L2: An automated contrast check passes for all frames in light and dark.

## Must-have features (MVP)

1. Identity creation + 24-word recovery phrase, secured through the onboarding backup flow (see "Onboarding & recovery backup")
2. QR / NFC contact add with safety numbers
3. 1:1 E2EE chat over BLE mesh (Airplane Mode works)
4. Group chat via MLS (small groups first, ≤32 members in MVP)
5. File share ≤50 MB with chunked encrypted transfer
6. Gateway toggle with battery/privacy disclosure
7. Offline queue + path indicator (mesh / gateway / mixed)
8. Security centre: verify, reset session, block
9. Encrypted local backup / restore
10. Message notifications ON by default with sender + preview; per-chat and global "Private notifications" option (see "Notifications (v0.1.2)")

## Acceptance tests

See PROTOCOL.md §13.6. All five scenarios must pass before beta tag.

## Out of scope for MVP

- Desktop clients
- Tor transport
- Voice/video
- Public global directory

## Post-MVP (communities & livestreams)

See PROTOCOL.md §20.

- Communities with roles, announcement channel, topic channels
- Livestream: see "Live (v0.2, draft)" below
- Stories/ephemeral, polls, stickers, threads in v0.2
- Usernames and bots deferred to v0.3

## Onboarding & recovery backup (v0.1.1)

Goal: a new user is chatting in under 60 seconds, and nobody loses their identity because they skipped writing down 24 words.

### Flow
1. **Welcome**: one screen with the value proposition and a "Get started" button. No account, phone number or email needed.
2. **Profile**: display name (optional) and avatar (optional). The identity key and 24-word phrase are generated in the background.
3. **Secure your account**: the user picks one or more backup methods. A cloud backup (Google Drive or iCloud) saves the recovery phrase **and all chat text**, encrypted (see "Encrypted chat backup"). Writing the phrase down restores only the identity and contacts, not the chats. At least one is required before the user can leave onboarding, but there's a "Remind me later" option that allows 24 h of use with a persistent banner.
   - **Google Drive** (Android default; also available on iOS)
   - **iCloud** (iOS default)
   - **Write it down** (shows the 24 words, then a quiz on 3 random words)
4. **Done**: show your QR code so people can add you. This also offers a nearby-contact add over BLE.

### Cloud backup requirements
- **Never upload the plaintext phrase.** The phrase is encrypted on the device before upload:
  - KDF: Argon2id (m=64 MiB, t=3, p=1, 16-byte random salt) over a **backup password** the user picks (min. 8 characters, with a zxcvbn score ≥ 3), *or*, on iCloud only, a 256-bit key held in iCloud Keychain (synced, end-to-end encrypted by Apple). The user can choose "No password" for iCloud.
  - Cipher: XChaCha20-Poly1305, with AAD = `"OMCP-Backup-v1" || suite_id || created_at`.
  - Blob format (CBOR): `{v:1, kdf, salt, nonce, ct, created_at, device_hint}`.
- **Google Drive**: the blob goes in the `appDataFolder` scope (hidden from the user's Drive and other apps). OAuth scope is `drive.appdata` only. Sign in with the Google account already on the device (Credential Manager).
- **iCloud**: the blob goes in the CloudKit **private** database (an `OMCPBackup` record). The key, when there's no password, lives in iCloud Keychain with `kSecAttrSynchronizable` and `kSecAttrAccessibleAfterFirstUnlock`.
- **Restore**: on a fresh install, the app detects an existing backup ("Welcome back, restore from Google Drive or iCloud?") and asks for the backup password if one was set. The 24-word phrase restore is always available as a fallback.
- **Management** (Settings > Backup): see the backup status and last-backup date, change the password, add or remove a backup method, delete the cloud backup, and view the phrase again (biometric or device PIN required).
- **Privacy**: no cloud backup is enabled without an explicit opt-in. No analytics on phrase content. Gateway or mesh traffic is never used for backups, only the platform's own cloud APIs over the Internet. If you're offline, the backup is queued and retried.
- **Honest copy**: the app tells the user plainly that anyone with access to their cloud account *and* the backup password can restore their identity, and that if they forget the password they need the written phrase.

### Acceptance tests
- O1: A fresh install reaches the chat list in ≤ 4 screens and ≤ 60 s for a user who picks a cloud backup.
- O2: The Drive and iCloud blobs never contain the phrase or the key in plaintext (verified by inspecting the uploaded bytes in a test).
- O3: Reinstall, then restore from Google Drive (Android) and from iCloud (iOS). The identity and contacts are restored, along with your verifications of contacts. Contacts' verification of you resets (see RS2).
- O4: A wrong backup password fails cleanly, with rate-limited retries. The phrase fallback still works.
- O5: Deleting the cloud backup in Settings removes the remote record (verified via API).
- O6: Skipping the backup shows a banner, and after 24 h the user can't continue without completing a backup method.

### Encrypted chat backup (v0.1.4, text by default, media optional)
- When Google Drive or iCloud backup is enabled, Hopla also backs up **all message text**: 1:1 chats, groups, community chats you joined, plus reactions, edits, timestamps and contact names. It does **not** back up images, videos, voice notes or files.
- The backup is encrypted end to end on the device with a key derived from the 24-word phrase (HKDF, label `hopla/chat-backup/v1`, XChaCha20-Poly1305). Google and Apple only ever see opaque blobs, stored in Drive `appDataFolder` or the iCloud private database.
- The backup is incremental: an encrypted append-only log plus a weekly compacted snapshot, padded to buckets of 64 KiB. It uploads on Wi-Fi only by default and runs at most hourly.
- Burn-after-read and view-once messages are never backed up.
- **Include media (setting, off by default)**: Settings > Backup > "Also back up photos, videos and files". When it's on, media is added to the same encrypted backup, with per-type toggles (photos, videos, voice notes, files), Wi-Fi only by default, and an estimated size plus a warning about provider storage quota. Restored media then appears directly with no placeholder. Media that hasn't been backed up still uses the placeholder flow. Turning it off deletes the backed-up media blobs after confirmation.
- Media backup defaults: when it's switched on, photos, voice notes and files are pre-selected and videos are not. Media backs up in a separate daily pass, on Wi-Fi only by default (an optional "Back up media on mobile data" toggle is off by default), apart from the hourly text backup. If the media download fails partway through a restore, the remaining items fall back to placeholders that refill from nearby contacts.
- CB5: With media backup on, a restore returns every media item, its hash verified. With it off, no media bytes are uploaded.
- **Restore**: after the 24-word recovery, the text history is restored. Every media item comes back as a **missing-media placeholder**, keeping its type, size, sender, time and caption (designed by Designy). When a chat partner who still has the file is reachable over the mesh or a gateway, the app automatically requests it again using the stored media locator and hash (§7, 64 KiB chunks), verifies the hash, and swaps the placeholder for the file. A user can also tap the placeholder to request it.
- **Partner no longer has it**: the placeholder says so and offers a one-tap "Ask to resend" request, which the user confirms before it is sent.
- **Restore indicator (security)**: after a restore from the 24 words, the restored device creates a new device key, signed by the identity key, with a `restored_at` timestamp. Contacts see an in-chat system notice, "<name> restored Hopla on a new device · <date>", and a "Restored" badge on the contact for 7 days, like a safety-number change. On the restored phone, a banner in each chat says "You restored this account. Your contacts were notified." If the restore was not made by the account owner, the contact can tap the notice to verify the safety number or block. Media re-requests themselves carry no extra metadata.
- **Re-verification after restore**: a restore resets the contact's "Verified" status on every contact's device. The safety number now covers the identity key plus the current device key, so it changes after a restore and contacts must verify again (QR code in person). Until then the contact shows as "Restored, not verified".
- **Groups and communities**: a restored member shows a small "Restored, not verified" tag next to their name in group and community chats, with no full notice there. The full system notice appears only in one-to-one chats.
- **Implementation clarifications (v0.1.4a)**:
  - iCloud uses the private database, custom zone `HoplaBackupV1`. Record types are `ChatLogSegment` (record name `seg-<u64 seq, hex>`), `ChatSnapshot` (`snap-<YYYYWW>`), `MediaBlob` (`media-<hex BLAKE2b-256 of ciphertext>`) and `BackupManifest` (`manifest`, one record). Each has a field `blob: CKAsset` plus `seq` and `created`, and no plaintext fields.
  - RS2 scope: the restored device keeps its own verifications of others. Contacts' verification of the restored account is cleared (consistent with O3).
  - Device binding: in every session handshake, the device key signs the Noise handshake hash. A certificate that's missing or superseded fails the handshake, so the restore notice can't be suppressed by replaying an old certificate.
  - Key types: PeerID = BLAKE2b-256 of the Ed25519 identity public key. X25519 keys are per-device and certified by the identity key. Contacts are keyed by PeerID, not by X25519.
- **Ask to resend**: a one-tap request with an optional short note, which the user can add before confirming. It's sent as a single message, not two.
- **Keep sent media**: Hopla keeps local copies of media you sent, by default for 90 days or up to 2 GB, with the oldest deleted first. Both limits can be changed in Settings > Storage. Burn-after-read and view-once media is never kept. These copies are served when a restored contact re-requests a file.
- Details: there's a 5 s Undo window after an Ask to resend is sent (10 s with a screen reader on), and the note is limited to 140 characters. Storage presets are 30 days / 90 days / 1 year / Forever and 500 MB / 2 GB / 5 GB / No limit. If the sender no longer has the file, they see "You no longer have this file" and can reply in the chat.
- RS3: A restored contact re-requests a file you sent 30 days ago and receives it, hash verified. Burn-after-read media is never served.
- RS2: After a restore, the safety number differs from the one before, the verified badge is cleared for all contacts, and verifying with the old QR code fails.
- RS1: Every contact who comes online after the restore shows the notice within 5 s of syncing. The notice can't be suppressed by the restored device, and the old device key is marked superseded.
- **Data use**: missing media re-downloads over a gateway on Wi-Fi only by default, with a setting to allow mobile data. Over direct BLE mesh it is always allowed.
- Tests:
  - CB1: Restoring on a new device brings back 100% of message text and no media bytes.
  - CB2: The backup blob can't be decrypted without the 24 words, and the provider sees only padded sizes.
  - CB3: Placeholders show for every media item, and each one resolves within 10 s once a partner holding the file is in range.
  - CB4: Burn-after-read content is never present in the backup.

## Notifications (v0.1.2)
- **Default ON**: when a message arrives, the notification shows the sender name, avatar and a text preview. Calls, mentions and livestream starts notify too.
- **Private notifications**: a toggle that applies to all chats (Settings > Notifications) or to one chat (Chat info). When it's on, the notification shows only "Hopla: New message", with no sender, no preview and no avatar. A separate **Hide on lock screen** option uses the platform's lock-screen visibility (Android `VISIBILITY_PRIVATE`/`SECRET`; iOS `hiddenPreviewsBodyPlaceholder`).
- **Delivery**: mesh and local delivery raise notifications on-device. Internet wake-ups use FCM/APNs with **content-free** pushes (just the opaque mailbox id). The app decrypts in a Notification Service Extension (iOS) or the FCM handler (Android). Google and Apple never see the sender or text.
- Quiet hours, mute per chat (1 h / 8 h / 1 week / forever), and per-community notification levels (all, mentions, none).
- Acceptance: N1 a fresh install shows a sender and preview notification; N2 with Private on, no sender or content appears anywhere in the notification (including on the lock screen and in the notification history); N3 the FCM/APNs payload has no plaintext (verified in a test).

## Hopla Nearby: dating (v0.3, opt-in)
Find people nearby to meet, without a central server and without sharing exact locations.

### Principles
- **Off by default, 18+ only**: you enable it in a separate Nearby tab, after confirming your age and agreeing to the community rules. Your dating profile is **not linked** to your chat identity until both people choose to reveal it.
- **No exact location, ever**: "nearby" means in BLE mesh range (≈ hops ≤ 3) or, over a gateway, the same coarse geohash cell (precision 5, ≈ 5 km), which is rotated and salted daily. Distance is shown only in buckets ("< 1 km", "< 5 km", "in your area").

### Profile
- A separate **Nearby key** (Ed25519/X25519) for each user, rotated every 24 h, so a profile can't be tracked over time or linked to the chat key.
- Fields: first name or nickname, age (in a range), up to 6 photos, a short bio, interests (tags), what you're looking for (dating / friends / both), who you'd like to see (gender, age range). Photos are re-encoded on-device and all metadata (EXIF, GPS) is stripped.
- Visibility controls: pause, show only to people with shared interests, hide from contacts in your address book (hashed matching, done locally).

### Discovery & matching
- Profiles are broadcast as signed, small `NEARBY_CARD` gossip packets (TTL 3 hops, 1 h expiry). Photos are fetched on demand, peer-to-peer.
- The user swipes or taps **Like** or **Pass**. A like is sent as a **sealed like** (HPKE to the other person's Nearby key). Neither side learns anything unless it's **mutual**. After a mutual like, both get a match notification, and a fresh 1:1 E2EE chat (Noise hybrid, as usual) opens under the Nearby keys.
- **Reveal identity**: after matching, either person can offer to share their main Hopla contact (it needs a mutual tap). Until then, the chat identity stays hidden.

### Safety (required for launch)
- Block and report are one tap from the profile, the chat and the match list. A block immediately hides you from that person's Nearby key and future rotations (this uses a local blocklist of their profile fingerprints, which are derived per epoch).
- Report bundles (the reported profile card and the messages the reporter chooses) go to an optional, community-run moderation service that the client chooses. The protocol stays open, and the reference app ships a default moderator.
- Limits: 100 likes per day; a message rate limit before a match (none allowed); first-contact photos blurred by default; a "Safety tips" screen before the first meeting; and an optional **share my date** feature that sends the meeting time and place to a trusted contact.
- No minors: age confirmation, plus an age-estimation check on the device if the store requires it. Profiles under 18 are never shown.

### Acceptance tests
- D1: Nearby is off on a fresh install and doesn't send any packets until it's enabled.
- D2: No exact location or GPS data leaves the device (verified in a packet-capture test); distance is shown in buckets only.
- D3: A one-way like reveals nothing to either side; a mutual like creates a match on both devices within 5 s in BLE range.
- D4: A Nearby profile can't be linked to the chat identity before an explicit mutual reveal (keys differ, and Nearby keys rotate every 24 h).
- D6: Nearby is never visible without the persistent indicator; auto-pause triggers within 60 s of its condition; an unanswered check-in turns Nearby off within 15 min.
- D5: Blocking hides the user across key rotations; reporting sends a bundle only with the reporter's confirmation.

## Burn after read (v0.1.3)
Messages and images that delete themselves once they've been read.

### Behaviour
- **Per message**: the 🔥 toggle in the composer (long-press Send) marks the next message or image as *burn after read*. **Per chat**: a "Burn after read" option in Chat info, with the timers *on open*, 10 s, 1 min, 1 h, or off.
- **Text**: it shows blurred ("Tap to read"). The timer starts when it's opened, with a visible countdown ring, and when it runs out the message is deleted on both sides.
- **Images and videos**: they open full-screen once (*view once*). They can't be saved, forwarded or quoted, and once they're closed or the timer runs out they're gone. Only a placeholder is left: "🔥 Photo opened".
- **Unread expiry**: anything not opened within 7 days (the sender chooses 1 h, 24 h or 7 days) is deleted unopened.
- **Groups and communities**: messages burn for each member separately after that member opens them. In channels with > 256 members it's not available (use the stories TTL).

### Protocol (PROTOCOL.md §19.x, `burn` header)
- The inner, encrypted message header gets `burn: {mode: on_open|timer, secs, unread_ttl}`. It's encrypted, so relays and gateways can't see that a message is ephemeral.
- The outer envelope `expires_at` = sent + unread_ttl, so relays and store-and-forward nodes drop the opaque envelope once it expires.
- The **media key is single-use**. It's deleted from the receiver once the media is viewed, and the encrypted media blob is deleted from the relay and cache. The sender deletes its local copy of the media key once it gets an `OPENED` receipt, which is sent as an encrypted control message.

### Device-side protection
- Burnable content is kept only in an encrypted, in-memory store or a separate encrypted DB table. Deletion zeroizes the keys, so it's crypto-shredded and can't be recovered from disk.
- **Screenshots**: burn-after-read content is always protected from capture, even when the Block screenshots toggle is off (see "Maintainer decisions" item 5).
- Burnable content never appears in notification previews, backups, the search index or share sheets.
- **Honest copy**: the app tells users plainly that burn after read can't stop someone photographing the screen with another device.

### Acceptance tests
- B1: A burn-after-read text is gone from both devices ≤ 1 s after the timer runs out, and there's no trace in the DB file afterwards (forensic test).
- B2: A view-once image can be opened exactly once. A second tap shows the placeholder, and save, forward and quote are disabled.
- B3: Burn content can never be captured, per SS1 (always protected, regardless of the toggle).
- B4: An unopened burn message expires at unread_ttl on the receiver and is dropped by relays (envelope `expires_at`).
- B5: Burn content is excluded from cloud backup, notifications and search.

## Maintainer decisions (2026-10-24)
1. **Private-mode calls** show "Hopla · Incoming call" (no caller name). Every other notification shows "New message".
2. **Per-chat override**: a single chat may turn Private notifications **off**, even when it's on globally.
3. **Android "Hide on lock screen"** shows "Hopla · New message" (not blank).
4. **Nearby must be a pleasure to use**, including on iOS:
   - **Nearby session**: one tap on "Go Nearby" makes you visible until you turn it off, with no time cap. Auto-pause: discovery pauses quietly after 2 h without motion, from 01:00 to 07:00 local time, or when battery is below 20%, and resumes automatically. Check-in: every 4 h of actual visibility (paused time does not count) a notification asks "Still up for meeting people?"; with no answer within 15 min, Nearby turns off. While you are visible, a persistent indicator is always shown, including in area mode (gateway visibility outside Go Nearby). On iOS this runs as a **Live Activity** and Dynamic Island item (people nearby, new likes) and keeps BLE scanning active for the whole session. On Android it's a foreground service with an ongoing notification.
   - **Background fallback**: outside a session, discovery keeps working over the gateway using the coarse geohash cell. iOS uses BLE state restoration and background app refresh to pick up cards opportunistically, so matches and likes still arrive as notifications.
   - **Instant start**: cached nearby cards appear in under 1 s when the tab is opened. Photos load progressively (blurhash first), swipes are 60/120 fps with haptics, and the "Nobody nearby yet" state shows a live radar and suggests starting a session or widening the radius.
   - **Honest status**: a small status chip ("Live nearby", "Paused" or "Area mode", with no countdown) instead of warnings. There are no confusing OS limitation messages.
   - Acceptance: NX1 the first card shows in under 1 s from a cold tab open (with a cache); NX2 an iOS session keeps discovering for 60 min with the phone locked; NX3 a match in background (area mode) is notified within 5 min.
5. **Block screenshots toggle, on by default** (Settings > Privacy; replaces the "notify sender" behaviour). While it is on, the following applies app-wide. Burn-after-read and view-once content is always protected, even with the toggle off:
   - **Android**: `FLAG_SECURE` on every app window, so screenshots, screen recordings, casting and the recent-apps thumbnail all show black.
   - **iOS**: all sensitive views (chats, media, Nearby, profiles) render inside a secure-capture layer (the `UITextField.isSecureTextEntry` container technique), so they come out blank in screenshots and recordings. The app switcher shows a blurred privacy cover. If screen recording or mirroring is detected (`UIScreen.isCaptured`), the content is hidden behind a cover until it stops.
   - The app tells users plainly (in Settings > Privacy) that a second camera can still photograph the screen.
   - Acceptance: SS1 with the toggle on (default), screenshots and recordings of every Hopla screen are black or blank; with it off, burn-after-read and view-once content still comes out black or blank, on Android 10–15 and iOS 17–26; SS2 the app switcher never shows chat content while Block screenshots is on; with it off, the preview is normal, but open burn-after-read and view-once screens stay hidden.

## Hopla team moderation (v0.2, draft), maintainer decision 2026-10-24
- TM1: The Hopla team can delete communities that spread hate speech or illegal content, using whitelisted team keys (PROTOCOL §23.9).
- TM2: For now the maintainer moderates alone with one key; an AI moderator is planned later as an additional key. Each deletion shows its reason to members and is listed in a public in-app log ("Removed communities").
- TM3: The team can undo a deletion; owners can appeal via the jury (§23.5).
- TM4: Private chats are never read, and 1:1 chats are never affected by any team action; private groups can only be erased by a group takedown (TM6). Other OMCP apps decide their own team list.
- TM5 (maintainer decision 2026-10-30): A member can prove a reported private-group message is genuine using the MLS sender signature every message already carries (PROTOCOL §23.10.1); there is no extra seal. The app tells users plainly that a reported message can be proven to come from them.
- TM6: 1:1 chats are never affected. A signed group takedown fully erases a private group on members' devices (messages, media, keys, and its part of backups; an older backup restore does not bring it back). A restore only lifts the ban so members can form a new, empty group (with a new group ID; the old one is never reused) and the old group is no longer flagged; the erase record stays (marked restored, kept at least 2 years), and a backup restore always drops the erased group's content, without relying on the device clock. After a backup restore, groups stay hidden until the app has synced with the mesh once. Community takedowns erase the same way. The app explains that screenshots, exported files and modified apps are out of reach (§23.10.2, §23.10.6).
- TM7 (required before v1.0): Members can export selected messages as an evidence file and send it to the Hopla team's intake address or web form (§23.10.3). The maintainer runs the inbox and form (<INTAKE_EMAIL>, <INTAKE_URL>), shown in Settings > Help and on the report screen; the maintainer stores reports encrypted at rest with a locally held key (operational, outside the protocol). 1:1 chats offer no reporting. Evidence files name the group only by hashes, never by its name or other metadata.
- TM8 (required before v1.0): Every decision is listed in the public log with only an evidence fingerprint and a reason category; evidence stays private for 90 days by default and is never republished (§23.10.4).
- TM9 (hard rule): The Hopla team can never read, join or see the members of a private group (§23.10.5, tests MR16–MR19).
- TM10: In communities the team can remove a single post, a topic, the whole community, or a Live stream (after a Live takedown the host cannot start Live in that community for 24 h); communities hold about 5,000 members, 4 KB per post, and 30 days or 50 MB of history (§23.10.6).
- TM11: Until v1.0 debug builds use a development key and release builds ignore all team actions (§23.10.8).
- TM12: Community posts are public. The join screen and post composer say "Posts in communities are public. Anyone in the community can read them, and they are not end-to-end encrypted." Private groups and 1:1 chats are marked end-to-end encrypted (PROTOCOL §20.1).

## Live (v0.2, draft), maintainer decision 2026-10-24
Offline: audio only. Online: video. The app picks the mode automatically from connectivity.

### Offline audio live (no internet)
- Transport: BLE mesh, plus Wi-Fi Direct/Aware (Android), Apple peer-to-peer Wi-Fi (iOS) or a local hotspot when available, for range and capacity.
- Codec: Opus at 12 kbit/s mono with 20 ms frames (fallback 6 kbit/s; Codec2 3.2 kbit/s optional for weak links). Forward error correction on, no retransmission.
- Spread: listeners re-broadcast the stream to the next hop, up to 3 hops by default, about 1–2 s of delay per hop. Frames are dropped, never queued, when a link is congested; chat traffic keeps priority.
- Encryption: in chats and private groups, a per-stream group key from the MLS group, rotated when a listener is removed; relays can't decrypt. In communities (open, PROTOCOL §20.1) the stream key goes to every listener, so community live audio is public, and the app says so.
- Late joiners start from the next frame; there's no recording unless the host turns it on and listeners are told.
- Live audio frames are not stored by Relay (RL1–RL4); they expire after 5 s.

### Online video live (no servers)
- Transport: peer-to-peer WebRTC from the host's app directly to viewers' apps, with no media server. Signalling goes through the existing OMCP envelopes.
- Scale: the host sends to up to 6 direct viewers. Each viewer forwards to up to 4 more, forming a tree, so the audience grows without a server. Viewers on weak links receive a lower-quality layer (simulcast/SVC).
- Encryption: DTLS-SRTP plus SFrame with the stream's group key, so forwarding viewers can't alter the stream and non-members can't watch.
- Connectivity: direct connections with ICE. Public STUN only reveals a phone's public address, and no media passes through it. TURN relaying is off by default; a phone that can't connect directly joins via another viewer instead.
- Video up to 720p with adaptive bitrate; audio Opus 32 kbit/s.
- If the host or a viewer loses internet, they drop to audio-only over the mesh without leaving the stream.

### Permissions
- The microphone is requested the first time the user records a voice message or goes live. The camera is already declared for QR codes and is reused for video.

### Acceptance tests
- LV1: With internet off, a host's audio reaches a listener 3 hops away over BLE with under 6 s total delay and under 5% lost frames at 12 kbit/s.
- LV2: A live audio stream never raises chat p95 latency by more than 20%.
- LV3: When the host goes offline mid-stream, listeners switch from video to audio within 5 s.
- LV4: A relay phone that isn't a group member can't decrypt the stream.

## Relay (store and forward), maintainer decision 2026-10-24
- RL1: Relay is always on, with no toggle and no contact/stranger split. Every phone carries all encrypted envelopes (messages and news) and passes them on, as §22 already does.
- RL2: Automatic cap of 500 MB, lowered automatically when free storage is under 10%. When full, drop the oldest envelopes first. Priority order: own chats, then contacts' envelopes, then everything else, then news.
- RL3: Envelopes are deleted once delivery is acknowledged or at outer expiry (max 7 d).
- RL4: Settings show only "Storage used for relaying: X MB", for information.

## Hopla News: aggregator over the mesh (v0.4, draft, revised 2026-10-24)
Goal: everyone stays up to date on the newest news, whether they have internet or not.

### How it works
- **Sources.** v0.4 uses **Google News RSS** only (feeds by topic, language and place). An **X news RSS feed** is planned as a second source, built by the maintainer in a separate project; clients treat any source as an RSS feed URL so it can be added later without protocol changes. No other publishers or curators.
- **Fetch when online.** Any Hopla phone with internet fetches the latest headlines from these two sources. No dedicated gateway role is needed.
- **Relay like messages.** Each fetched story is packed into a small news item (headline, 2–3 sentence summary, source name, original link, time, language, topic, optional thumbnail ≤ 20 KB WebP; ≤ 2 KB without thumbnail). It is sent through the mesh in the same low-cost encrypted envelopes as messages, on a public news channel mailbox tag per topic, language and place, with Plumtree (§22.3).
- **No per-story verification.** Clients do not check publisher signatures or query a trust service, to avoid extra traffic. The relaying phone's normal envelope signature is kept, so the app can show "via <contact or short PeerID>".
- **Dedup.** Items are identified by a hash of source and original link, so the same story fetched by many phones is stored and relayed once.
- **Priority and expiry.** News runs below chats, with an outer expiry of 48 h (emergency alerts 7 d). Each phone keeps ≤ 20 MB of news (configurable) and drops the oldest first.
- **Works offline.** A phone offline for days catches up from the first neighbour it meets, via the same set reconciliation (§22) as messages.

### In the app
- A **News** tab in the bottom bar: Chats · News · Communities · Nearby · You. Live ("Coming in v0.2") moves to the top of Communities.
- Top stories, Local, and topics (World, Politics, Tech, Sport, Health, Emergencies).
- **Emergency alerts** are pinned at the top and notify by default; the user can turn this off.
- A story opens a reader view with the summary and an "Open on X / Read on Google News" link.
- Users pick topics and languages (EN, FR, HE), and can mute a topic.
- **Share to chat or community** sends the story as a news card and starts a discussion thread there; community moderation (§23) applies.
- **No citizen reports.** Users cannot publish into News. Discussion of topics happens in communities, to keep rumours and conspiracy theories out of the News feed.

### Personalisation
- Default: newest world headlines, out of the box.
- City: typed in, or taken from the phone's location if the user allows it (the same permission the dating feature uses). Only the city name is used, to subscribe to that city's channel; the location is never sent.
- Personalisation runs on the device; the phone subscribes only to matching channel tags.

### Privacy
- Reading is local; no tracking and no engagement ranking. Sorting is by recency, then by how many phones relayed the story.
- Fetching phones call Google News directly, so those services see that phone's request, as with any app.

### Acceptance tests
- NW1: A phone offline for 72 h gets the current top stories within 60 s of meeting one up-to-date neighbour.
- NW2: The same story fetched by many phones appears once and is relayed once per link.
- NW3: News traffic never delays chat delivery (chat p95 latency unchanged in a mixed-load simulation).
- NW4: News storage stays within the cap, and the idle budget (§22.7(11)) still holds.
- NW5: Picking a city shows that city's stories within 60 s of a neighbour having them, without downloading other cities' channels.
- NW6: A shared news card opens in chats and communities and its thread works like a normal post.

### Open decisions
- X news RSS feed: separate maintainer project, added as a source later.
