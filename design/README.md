# OMCP App Design Pack v0.1.21 — Hopla

**Date:** 2026-10-26 (Europe/Paris) · v0.1.20 2026-10-24 · v0.1.19 2026-10-24 · v0.1.18 2026-10-24 · v0.1.17 2026-10-24 · v0.1.16 2026-10-24 · v0.1.15 2026-10-24 · v0.1.14 2026-10-24 · v0.1.13 2026-10-24 · v0.1.12 2026-10-24 · v0.1.11 2026-10-24 · v0.1.10 2026-10-24 · v0.1.9 2026-10-24 · v0.1.8 2026-10-24 · v0.1.7 2026-10-24 · v0.1.6 2026-10-24 · v0.1.5 2026-10-24 · v0.1.4 2026-10-24 · v0.1.3 2026-10-24 · v0.1.2 2026-10-24 · v0.1.1 2026-10-24 · v0.1.0 2026-10-23  
**App name:** Hopla (consumer app) · **Protocol:** OMCP. Logo: approved `brand/hopla-v2/icon.svg` (inlined in welcome, done, restore mockups).  
**Audience:** Proggy (Android/iOS implementers), Pipely (release), the maintainer / Writey (product)  
**Source of truth for product rules:** `../PROTOCOL.md` §13, §20 and `../apps/REQUIREMENTS.md`  
**This pack:** visual language, IA, screen specs, and static HTML mockups — not wire protocol.

## Where to start

1. Read **[DESIGN_SPEC.md](./DESIGN_SPEC.md)** — principles, tokens, IA, navigation, screen-by-screen specs, platform notes, EN/FR copy (+ **HE draft** column, pending native-speaker review), component inventory.
2. Open **[mockups/](./mockups/)** in a browser — phone frames at 390×844. Shared tokens in `mockups/tokens.css`.
3. Accessibility (v0.1.18): both themes meet WCAG AA — **[a11y/light-contrast.md](./a11y/light-contrast.md)** and **[a11y/dark-contrast.md](./a11y/dark-contrast.md)** (every pair, before/after), **a11y/light-before-after.png**, **a11y/light-vs-dark.png** (matched pair + token-step table); older: a11y/dark-before-after.png, a11y/light-spot-check.png (v0.1.17).
4. Locales: EN, FR and **Hebrew (he, RTL) — DRAFT, pending native-speaker review**. Hebrew frames are the `*--he.png` / `*--he-dark.png` screenshots; RTL rules in DESIGN_SPEC §5.106.
5. **Hopla News (v0.4, v0.1.20)** — DESIGN_SPEC §5R: `mockups/news-*.html` (18 screens × EN/FR/HE, light/dark = 108 PNGs). Bar: Chats · News · Communities · Nearby · You; Live at the top of Communities. Source: Google News RSS only (neutral text label; second source can be added later) + “via ‹contact›”. Contrast: `a11y/news-contrast.md`. **Relay settings screen: placeholder — will follow once the relay questions are answered.**
5a. **WCAG 2.2 (v0.1.21)** — DESIGN_SPEC §5S: target size, focus not obscured, drag alternatives, iOS glass-bar scrim.
5b. Use **PNG screenshots** (`mockups/*.png`) in tickets and PR descriptions.
6. Mirror copy for Proggy/Pipely: `/home/box/shared/omcp-design/` (identical tree).

## MVP (v0.1) vs post-MVP (v0.2)

| Area | v0.1 MVP | v0.2 |
|------|----------|------|
| Identity + 24-word recovery | ✓ | — |
| QR / NFC contacts + safety numbers | ✓ | — |
| 1:1 + small MLS groups (≤32) | ✓ | — |
| Files ≤50 MB + progress | ✓ | — |
| Gateway toggle + battery/privacy disclosure | ✓ | — |
| Offline queue + path indicators (mesh / gateway / mixed) | ✓ | — |
| Security centre | ✓ | — |
| Encrypted backup / restore (Google Drive, iCloud: phrase + all chats; written phrase: identity + contacts) | ✓ | — |
| Notification previews **off** by default | ✓ | — |
| Communities (roles, announcement + topics) | sketched | ✓ |
| Livestreams (mesh-beacon / lan-relay / internet-sfu) | sketched (empty states) | ✓ |
| Stories, polls, stickers, threads | — | ✓ |
| Usernames, bots | — | v0.3 |

Mockups marked **Communities** and **Live** are forward-looking so capacity can be planned; ship gating stays on MVP acceptance tests (PROTOCOL §13.6).

## How Proggy should consume this

1. **Map components** in DESIGN_SPEC § Component inventory → Kotlin (Compose or View) / SwiftUI views. Prefer shared naming (`PathChip`, `SafetyNumberBlock`, `GatewayDisclosure`).
2. **Tokens** → Android `Color`/`Type`/`Dimens` XML or Compose `OmcpTheme`; iOS `Asset` colors + `Font` styles. Light **and** dark required; WCAG AA (both audited: `a11y/light-contrast.md`, `a11y/dark-contrast.md`; use `--brand-ink` for anything on a solid accent fill, never literal white).
3. **Copy** → string catalogs `en` + `fr` from § Copy principles; never invent “military grade” / “unhackable” language.
4. **Path indicator** is a first-class UI element (chat list row, thread banner, settings legend) — not a debug overlay.
5. **Do not** treat HTML mockups as production UI; they are layout + state references. Native widgets and platform HIG win on spacing micro-adjustments.
6. **Platform constraints** (§ Platform notes): Android persistent BLE foreground notification; iOS background BLE limits with clear user messaging.
7. Open product questions at the end of DESIGN_SPEC — resolve with the maintainer/Writey before locking v0.1 UI freeze.

## Screen index

PNG = `mockups/<name>.png` (390×844 @2x); extra states = `mockups/<name>--<state>.png`.

| Screen | Mockup | Spec | Extra state frames | Since |
|--------|--------|------|--------------------|-------|
| Welcome (1/4) | `onboarding-welcome` | §5.14 | — | 0.1.2 (updated) |
| Profile (2/4) | `onboarding-profile` | §5.25 | `--filled` | 0.1.2 |
| Secure your account (3/4) | `onboarding-secure` | §5.26 | `--ios`, `--password-android`, `--password-ios`, `--success`, `--failure` | 0.1.2 |
| Write it down (3/4 sub-flow) | `onboarding-secure-phrase` | §5.27 | `--checked` | 0.1.2 |
| Check your words (3/4 sub-flow) | `onboarding-secure-quiz` | §5.27 | `--mismatch` | 0.1.2 |
| Done (4/4) | `onboarding-done` | §5.28 | — | 0.1.2 |
| Remind me later banner / 24 h block | `onboarding-secure-reminder` | §5.29 | `--expired` | 0.1.2 |
| Settings › Backup | `settings-backup` | §5.30 | `--queued`, `--view-phrase-auth`, `--delete-confirm` | 0.1.2 |
| Restore on reinstall | `restore-reinstall` | §5.31 | `--password`, `--wrong-password`, `--error` | 0.1.2 |
| Restore with 24 words (fallback) | `onboarding-restore` | §5.17 | (error shown in default) | 0.1.1 |
| ~~Recovery phrase~~ **DEPRECATED** → replaced by `onboarding-secure-phrase` | `onboarding-recovery` | §5.15 | `--checked` | 0.1.1 |
| ~~Confirm phrase~~ **DEPRECATED** → replaced by `onboarding-secure-quiz` | `onboarding-recovery-confirm` | §5.16 | `--mismatch` | 0.1.1 |
| ~~Permissions~~ **DEPRECATED** → replaced by in-context Bluetooth on `onboarding-done` | `onboarding-permissions` | §5.18 | `--bluetooth-denied` | 0.1.1 |
| Chats list (VerifiedBadge) | `chats-list` | §5.2, §5.32 | `--dark`, `--ios` | 0.1.3 |
| Chat thread (VerifiedBadge, ghost seal, hint) | `chat-thread` | §5.3, §5.32–5.34 | `--dark`, `--ios`, `--ios-dark`, `--unverified`, `--unverified-dark`, `--unverified-ios`, `--verified`, `--safety-details` | 0.1.3 |
| Add contact chooser | `contact-add-chooser` | §5.19 | — | 0.1.1 |
| Scan QR | `qr-scan` | §5.20 | `--not-omcp` | 0.1.1 |
| My QR | `qr-my-code` | §5.21 | — | 0.1.1 |
| Contact preview + safety number | `contact-preview-safety` | §5.22 | `--mismatch` | 0.1.1 |
| File send confirm | `file-picker-confirm` | §5.23 | `--too-large` | 0.1.1 |
| File transfer states | `file-transfer` | §5.24 | — | 0.1.1 |
| Security centre | `security-centre` | §5.9 | — | 0.1.0 |
| Gateway settings | `gateway-settings` | §5.7 | — | 0.1.0 |
| Communities (v0.2) | `communities-home` | §5.10 | — | 0.1.0 |
| Live (v0.2) | `live-tab` | §5.12 | — | 0.1.0 |

## Files

```
design/
  README.md              ← you are here
  DESIGN_SPEC.md
  CHANGELOG.md
  brand/                 (logo — not mirrored; hopla-v2 approved, unchanged)
  mockups/
    tokens.css             (v0.1.1 components appended at end)
    notif-lockscreen.html (+ .png, --ios, --private, --private-ios, --hidden, --hidden-ios, --fr, --private-fr-ios, --burn, --shade-private, --shade-private-dark)  v0.1.4
    settings-notifications.html (+ .png, --dark, --private, --ios, --ios-dark, --fr)  v0.1.4
    chat-info-notifications.html (+ .png, --private, --opt-out, --opt-out-dark, --opt-out-ios, --opt-out-fr, --dark, --ios, --mute, --fr)  v0.1.4, opt-out v0.1.5
    settings-privacy.html (+ .png, --dark, --ios, --confirm-off, --confirm-off-dark, --confirm-off-ios, --off, --off-dark, --off-ios, --fr, --confirm-off-fr, --off-fr)  v0.1.5, toggle v0.1.6, copy v0.1.7
    mod-report.html (+ .png, --sent, --limit, --dark, --ios, --fr)  v0.2 / pack v0.1.6
    mod-community.html (+ .png, --vouch, --vouched, --reputation, --dark, --ios, --fr)  v0.2 / pack v0.1.6
    mod-states.html (+ .png, --reported, --reported-why, --restricted, --reported-dark, --restricted-dark, --ios, --fr)  v0.2 / pack v0.1.6
    mod-join-warning.html (+ .png, --restricted, --dark, --ios, --fr)  v0.2 / pack v0.1.6
    mod-stream.html (+ .png, --continue, --reduced-motion, --ios, --fr)  v0.2 / pack v0.1.6
    mod-appeal.html (+ .png, --pending, --cleared, --dark, --ios, --fr)  v0.2 / pack v0.1.6
    mod-jury.html (+ .png, --voted, --dark, --ios, --fr)  v0.2 / pack v0.1.6
    app-switcher-cover.html (+ .png, --android, --ios-off, --android-off, --ios-recording, --ios-recording-fr)  v0.1.5, off states v0.1.7
    burn-composer.html (+ .png, --armed, --dark, --ios, --ios-dark, --fr)  v0.1.4
    burn-bubbles.html (+ .png, --sender, --open, --placeholders, --dark, --ios, --ios-dark, --fr)  v0.1.4 (screenshot frames removed v0.1.5)
    burn-viewer.html (+ .png, --ios, --ios-recording, --fr)  v0.1.4
    burn-chat-info.html (+ .png, --dark, --ios, --fr)  v0.1.4
    nearby-tab.html (+ .png, --dark, --ios, --fr)  v0.3 (designed in pack v0.1.4)
    nearby-onboarding.html (+ .png, --under-18, --rules, --privacy, --dark, --ios, --fr)  v0.3
    nearby-profile.html (+ .png, --dark, --ios, --fr)  v0.3
    nearby-discover.html (+ .png, --swiping, --reduced-motion, --dark, --ios, --empty, --limit, --fr)  v0.3
    nearby-match.html (+ .png, --dark, --ios, --reduced-motion, --fr)  v0.3 (refined v0.1.5)
    nearby-session.html (+ .png, --live, --paused, --dark, --ios, --fr, --ios-live-activity[-paused-motion|-paused-night|-paused-battery][-private], --android-ongoing[-paused-…][-private], --ios-island, --ios-island-private, --ios-island-paused, --ios-island-expanded, --ios-island-expanded-paused, --android-ongoing-dark, --android-ongoing-paused-motion-dark, --fr-live-activity, --fr-live-activity-paused-night, --fr-android-ongoing-paused-battery)  v0.3 / rebuilt v0.1.8
    nearby-checkin.html (+ .png, --android, --in-app, --in-app-dark, --in-app-ios, --turned-off, --turned-off-dark, --turned-off-notif, --fr, --in-app-fr, --turned-off-fr)  v0.3 / pack v0.1.8
    settings-nearby.html (+ .png, --dark, --ios, --fr)  v0.3 / pack v0.1.8, copy v0.1.9
    nearby-area-indicator.html (+ .png, --ios-live-activity[-paused-night][-private], --android-ongoing[-paused-night][-private], --ios-island, --ios-island-private, --ios-island-expanded, --ios-island-paused, --android-ongoing-dark, --android-ongoing-paused-motion-dark, --fr-live-activity, --fr-android-ongoing-private, --fr-live-activity-paused-night)  v0.3 / pack v0.1.9
    nearby-area.html (+ .png, --sheet, --dark, --ios, --fr)  v0.3 / pack v0.1.5
    nearby-radar.html (+ .png, --dark, --reduced-motion, --live, --ios, --fr)  v0.3 / pack v0.1.5
    nearby-skeleton.html (+ .png, --blurhash, --resolved, --dark, --blurhash-dark)  v0.3 / pack v0.1.5
    nearby-chat.html (+ .png, --reveal-confirm, --reveal-pending, --reveal-incoming, --revealed, --photo-blurred, --dark, --ios, --ios-dark, --fr)  v0.3
    nearby-safety.html (+ .png, --block, --report, --report-confirm, --done, --tips, --share-date, --dark, --tips-dark, --ios, --fr)  v0.3
    onboarding-welcome.html (+ .png)                 v0.1.2 updated
    onboarding-profile.html (+ .png, --filled)       v0.1.2
    onboarding-secure.html (+ .png, --ios, --password-android, --password-ios, --success, --failure)  v0.1.2
    onboarding-secure-phrase.html (+ .png, --checked) v0.1.2
    onboarding-secure-quiz.html (+ .png, --mismatch)  v0.1.2
    onboarding-secure-reminder.html (+ .png, --expired) v0.1.2
    onboarding-done.html (+ .png)                     v0.1.2
    settings-backup.html (+ .png, --queued, --view-phrase-auth, --delete-confirm) v0.1.2
    media-missing.html (+ .png, --requested, --syncing, --restored, --reduced-motion, --video, --gone, --group, --dark, --ios, --ios-dark, --fr, --syncing-fr, --gone-fr, --sent, --wifi, --wifi-dark, --sent-fr, --wifi-fr, --ios-gone)  v0.1.11, v0.1.12, v0.1.16 HERO
    settings-media-backup.html (+ .png, --on, --uploading, --near-full, --full, --confirm-off, --deleting, --dark, --ios, --ios-dark, --fr, --confirm-off-fr, --near-full-fr, --deleting-fr)  v0.1.13 HERO
    restore-media.html (+ .png, --success, --partial, --partial-fr, --partial-dark, --dark, --ios, --success-dark, --success-ios, --fr, --success-fr)  v0.1.13, v0.1.14 HERO
    group-restored.html (+ .png, --members, --sheet, --dark, --members-dark, --ios, --ios-members, --ios-dark, --fr, --members-fr, --sheet-fr)  v0.1.15 HERO
    media-resend-note.html (+ .png, --empty, --sent, --dark, --sent-dark, --ios, --ios-sent, --fr, --sent-fr)  v0.1.16 HERO
    settings-storage.html (+ .png, --edit, --edit-custom, --edit-off, --off, --forever, --dark, --edit-dark, --ios, --ios-edit, --fr, --edit-fr, --off-fr)  v0.1.16 HERO
    restore-notice.html (+ .png, --sheet, --list, --self, --qr, --reverified, --list-reverified, --dark, --sheet-dark, --reverified-dark, --ios, --ios-list, --ios-self-dark, --ios-reverified, --fr, --sheet-fr, --list-fr, --self-fr, --qr-fr, --reverified-fr)  v0.1.12, v0.1.14 HERO
    settings-missing-media.html (+ .png, --on, --dark, --ios, --fr)  v0.1.12
    chats-restored.html (+ .png, --dark, --ios, --ios-dark, --fr)  v0.1.11
    restore-detect.html (+ .png, --ios, --dark, --ios-dark, --checking, --start-fresh, --fr, --ios-fr)  v0.1.10 (copy v0.1.11) HERO
    restore-source.html (+ .png, --ios, --picker, --ios-google-signin, --dark, --no-backup, --no-backup-ios, --no-backup-dark, --fr, --no-backup-fr)  v0.1.10
    restore-password.html (+ .png, --wrong, --lockout, --dark, --ios, --keychain, --keychain-missing, --keychain-dark, --fr, --lockout-fr, --keychain-fr)  v0.1.10 HERO
    restore-progress.html (+ .png, --download, --decrypt, --dark, --ios, --reduced-motion, --background-android, --background-ios, --fr)  v0.1.10 (copy v0.1.11) HERO
    restore-success.html (+ .png, --dark, --ios, --ios-dark, --fr)  v0.1.10 (copy v0.1.11, v0.1.12) HERO
    restore-error.html (+ .png, --permission, --quota, --corrupted, --too-new, --dark, --ios, --fr, --offline-fr)  v0.1.10
    restore-reinstall.html (+ .png, --password, --wrong-password, --error) v0.1.2 — visuals superseded by restore-* (v0.1.10)
    onboarding-restore.html (+ .png)                  24-word fallback
    onboarding-recovery.html (+ .png, --checked.png)  DEPRECATED
    onboarding-recovery-confirm.html (+ .png, --mismatch.png) DEPRECATED
    onboarding-permissions.html (+ .png, --bluetooth-denied.png) DEPRECATED
    contact-add-chooser.html (+ .png)
    qr-scan.html (+ .png, --not-omcp.png)
    qr-my-code.html (+ .png)
    contact-preview-safety.html (+ .png, --mismatch.png)
    file-picker-confirm.html (+ .png, --too-large.png)
    file-transfer.html (+ .png)
    chats-list.html (+ .png)
    chat-thread.html (+ .png)
    security-centre.html (+ .png)
    gateway-settings.html (+ .png)
    communities-home.html (+ .png)
    live-tab.html (+ .png)
```

## Constraints for this pack

- No GitHub push from design work; PROTOCOL.md untouched.
- Clarity of mesh vs online state over decorative chrome.
- Path indicators visible but calm (mesh is normal, not an alert).
