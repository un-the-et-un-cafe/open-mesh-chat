# OMCP Design Pack — Changelog

## v0.1.22 — 2026-10-29

Communities with Live, matched to PR #19. `DESIGN_SPEC.md` §5T (5.120–5.127).

### Added
- 15 screens × 6 frames (EN/FR/HE-draft, light/dark) = 90 PNGs: community list/detail, create, invite (32-member counter + full state), accept invite, host Remove from live (+confirm sheet, ⋯ menu alternative, rekey toast), takedown host/viewer, live-start notification (shade + generic lock screen), community-removed banner, Report this live with copyable stream ID.
- Copy table EN/FR/HE (draft). Generator `omcp-v0122/live.py`.
- `brand/` and `tokens.css` untouched. AA contrast failures: 0 (light + dark).

## v0.1.21 — 2026-10-26

WCAG 2.2 audit (2.5.8, 2.4.11, 2.5.7) + glass-bar contrast. `DESIGN_SPEC.md` §5S.

### Added
- “Mute a topic” button in News settings (single-pointer alternative to long-press; EN/FR/HE strings). Re-rendered `news-interests*` and `news-interests-location*` (12 PNGs).
- Focus-ring style (`:focus-visible`) and focus-not-obscured rule; invisible 44 px hit-slop on news chips/buttons (`news.css`, no visual change).
- Spec rules: platform touch size (44 pt / 48 dp), drag alternatives (match-list overflow, queue tap menu, sliders, reorder), glass bar minimum scrim.
- `brand/` and `tokens.css` untouched. AA contrast failures: 0.

## v0.1.20 — 2026-10-24

Hopla News revised to the 2026-10-24 REQUIREMENTS and maintainer decisions. `DESIGN_SPEC.md` §5R rewritten (5.107–5.119); Q26–Q35 closed, Q37–Q39 open.

### Changed
- The only source is **Google News RSS**, shown as a neutral “Google News” text label (no logo) plus “via ‹contact›” when relayed; “Relayed by N phones”. Reader = summary + “Read on Google News”. The label component takes a second source later (X news RSS, separate project) without layout changes.
- Emergency alerts: Emergencies-topic stories from the sources (no official key); notifications **on by default**.
- Interests (onboarding + News settings): **Use my location** (same permission as Nearby) or type the city; languages, muted topics, storage.
- Share sheet lists **chats and communities**; shared card + discussion thread in both; “News can’t be posted by users”.
- Shared-card thumbnail no longer overflows under the card text.
- Re-rendered (96): news-alert--dark.png, news-alert--fr-dark.png, news-alert--fr.png, news-alert--he-dark.png, news-alert--he.png, news-alert.png, news-catchup--dark.png, news-catchup--fr-dark.png, news-catchup--fr.png, news-catchup--he-dark.png, news-catchup--he.png, news-catchup.png, news-chat-thread--dark.png, news-chat-thread--fr-dark.png, news-chat-thread--fr.png, news-chat-thread--he-dark.png, news-chat-thread--he.png, news-chat-thread.png, news-community-thread--dark.png, news-community-thread--fr-dark.png, news-community-thread--fr.png, news-community-thread--he-dark.png, news-community-thread--he.png, news-community-thread.png, news-empty--dark.png, news-empty--fr-dark.png, news-empty--fr.png, news-empty--he-dark.png, news-empty--he.png, news-empty.png, news-interests--dark.png, news-interests--fr-dark.png, news-interests--fr.png, news-interests--he-dark.png, news-interests--he.png, news-interests.png, news-loading--dark.png, news-loading--fr-dark.png, news-loading--fr.png, news-loading--he-dark.png, news-loading--he.png, news-loading.png, news-local--dark.png, news-local--fr-dark.png, news-local--fr.png, news-local--he-dark.png, news-local--he.png, news-local-empty--dark.png, news-local-empty--fr-dark.png, news-local-empty--fr.png, news-local-empty--he-dark.png, news-local-empty--he.png, news-local-empty.png, news-local.png, news-onboarding-interests--dark.png, news-onboarding-interests--fr-dark.png, news-onboarding-interests--fr.png, news-onboarding-interests--he-dark.png, news-onboarding-interests--he.png, news-onboarding-interests.png, news-reader--dark.png, news-reader--fr-dark.png, news-reader--fr.png, news-reader--he-dark.png, news-reader--he.png, news-reader.png, news-share--dark.png, news-share--fr-dark.png, news-share--fr.png, news-share--he-dark.png, news-share--he.png, news-share-bubble--dark.png, news-share-bubble--fr-dark.png, news-share-bubble--fr.png, news-share-bubble--he-dark.png, news-share-bubble--he.png, news-share-bubble.png, news-share.png, news-top--dark.png, news-top--fr-dark.png, news-top--fr.png, news-top--he-dark.png, news-top--he.png, news-top-city--dark.png, news-top-city--fr-dark.png, news-top-city--fr.png, news-top-city--he-dark.png, news-top-city--he.png, news-top-city.png, news-top.png, news-world--dark.png, news-world--fr-dark.png, news-world--fr.png, news-world--he-dark.png, news-world--he.png, news-world.png.

### Added (12)
- news-interests-location--dark.png, news-interests-location--fr-dark.png, news-interests-location--fr.png, news-interests-location--he-dark.png, news-interests-location--he.png, news-interests-location.png, news-location-permission--dark.png, news-location-permission--fr-dark.png, news-location-permission--fr.png, news-location-permission--he-dark.png, news-location-permission--he.png, news-location-permission.png.

### Removed (18) — signatures, trust levels, curators, sources management, hidden counts, “Story unavailable”, full-text reader
- news-reader-summary--dark.png, news-reader-summary--fr-dark.png, news-reader-summary--fr.png, news-reader-summary--he-dark.png, news-reader-summary--he.png, news-reader-summary.png, news-sources--dark.png, news-sources--fr-dark.png, news-sources--fr.png, news-sources--he-dark.png, news-sources--he.png, news-sources.png, news-trust--dark.png, news-trust--fr-dark.png, news-trust--fr.png, news-trust--he-dark.png, news-trust--he.png, news-trust.png (and `news-trust.html`, `news-sources.html`, `news-reader-summary.html`).

### Accessibility
- Whole pack re-audited: light 0 failures (11772 measurements), dark 0 failures (4500).

### Constraints observed
- `brand/` and `tokens.css` untouched; no new tokens. Fictional people and stories; generated art. Not pushed. Generator `/workspace/omcp-v0120/`. Mirror updated.

## v0.1.19 — 2026-10-24

Hopla News (REQUIREMENTS § Hopla News, v0.4 draft, incl. maintainer decisions), requested by the maintainer. `DESIGN_SPEC.md` §5R (5.107–5.119).

### Added
- 19 mockups (`news-top`, `news-local`, `news-local-empty`, `news-world`, `news-top-city`, `news-empty`, `news-loading`, `news-reader`, `news-reader-summary`, `news-trust`, `news-alert`, `news-sources`, `news-interests`, `news-onboarding-interests`, `news-share`, `news-share-bubble`, `news-chat-thread`, `news-community-thread`, `news-catchup`), each EN/FR/HE (draft) × light/dark = 114 PNGs.
- `mockups/news.css` (components only, existing tokens). **No new tokens; `tokens.css` and `brand/` unchanged.**
- `SignatureBadge` with 5 shapes (verified, via curator, unsigned, invalid, official); unsigned/invalid never shown as stories (NW2).
- Nav decision §5.107: Chats · News · Communities · Nearby · You; Live moves into Communities (Q26).
- Copy table EN/FR/HE draft (§5.119), questions Q26–Q36, `a11y/news-contrast.md`.
- Placeholder: Relay settings screen will follow once the relay questions are answered.

### Accessibility
- Contrast re-run on the whole pack: light 0 failures (11822 measurements), dark 0 failures (4550).

### Constraints observed
- Fictional names, sources and generated abstract art only. Not pushed. Generator `/workspace/omcp-v0119/`. Mirror updated.

## v0.1.18 — 2026-10-24

Light mode to WCAG AA as the matched twin of dark, and a first RTL locale: Hebrew (requested by Jakob). `DESIGN_SPEC.md` §5.104–5.106.

### Changed — light theme (WCAG AA; light token change approved by Jakob)
- `tokens.css` `:root` only: `--ink-2` #4a5b70 → #3b495a, `--ink-3` #6b7c90 → #515e6d, `--line` #d8dee8 → #73849a, `--line-strong` #b8c2d0 → #617289. Each step now has the same contrast on light `--surface` as the v0.1.17 dark step on dark `--surface` (9.18 / 6.62 / 3.82 / 4.91 vs 9.18 / 6.60 / 3.82 / 4.87). Dark values, soft tints, brand hues unchanged.
- Disabled controls without `opacity`: `nearby-skeleton` loading row (`.is-loading`, `aria-disabled`), `restore-password` lockout, `settings-storage` edit-off. Disabled now ≥ 3.5:1 light, ≥ 4.0:1 dark (was 1.07 / 1.34).
- Inverse toasts use the other theme’s teal for their action/icon (was 2.84:1). `mod-stream` host line no longer fades the reputation chip. “Ask to resend” pill border = full `--brand`. Photo scrims ≥ .62.
- Result (excluding verified white-on-photo / OS-chrome artefacts, listed in the audit): light failing element pairs 1548 → **0**, worst non-disabled 1.15:1 → **3.25:1**, worst text 2.22:1 → **4.54:1**; dark failing 5 → **0** (the 5 were the disabled skeleton buttons), worst non-disabled 3.15:1 (unchanged), worst disabled 1.34:1 → 4.02:1.
- Added `a11y/light-contrast.md`, `a11y/light-before-after.png`, `a11y/light-vs-dark.png`; re-generated `a11y/dark-contrast.md`; `a11y/dark-contrast.py` now measures either theme.

### Added — Hebrew (he, RTL) — DRAFT, pending native-speaker review
- 27 Hebrew frames (`--he`, plus `--he-dark` for chats-list and chat-thread): onboarding-welcome--he.png, onboarding-permissions--he.png, onboarding-profile--he.png, onboarding-secure--he.png, onboarding-secure-phrase--he.png, onboarding-secure-quiz--he.png, onboarding-done--he.png, onboarding-restore--he.png, chats-list--he.png, chats-list--he-dark.png, chat-thread--he.png, chat-thread--he-dark.png, settings-privacy--he.png, settings-backup--he.png, settings-storage--he.png, settings-notifications--he.png, restore-detect--he.png, restore-source--he.png, restore-password--he.png, restore-progress--he.png, restore-success--he.png, nearby-area--he.png, nearby-chat--he.png, burn-bubbles--he.png, burn-composer--he.png, mod-community--he.png, mod-report--he.png.
- “HE (draft)” column in every EN/FR copy table of `DESIGN_SPEC.md` (216 rows).
- `tokens.css`: RTL rules (`.phone[dir=rtl]`: Rubik → Noto Sans Hebrew → Heebo; mirrored chevrons/send, bubble tails, toggles; status bar, QR, word and safety grids stay LTR).
- Spec §5.106 RTL mirroring rules (what mirrors, what never mirrors, bidi isolation, swipe directions).

### Re-rendered
- All mockups (system python3 Playwright, `/usr/bin/google-chrome`).

### Constraints observed
- `brand/` untouched; not pushed. Generator `/workspace/omcp-v0118/`. Mirror updated.

## v0.1.17 — 2026-10-24

Dark mode readable everywhere, and backup copy that says chats are backed up (requested by Jakob). `DESIGN_SPEC.md` §5.102, §5.103.

### Changed — dark theme (WCAG AA)
- `tokens.css`, dark values only (`@media (prefers-color-scheme: dark)` and `.phone.dark`): `--ink-2` #a3b1c2 → #b4c0ce, `--ink-3` #7a8a9c → #95a3b4, `--line` #2a3646 → #687990, `--line-strong` #3d4d60 → #7a8ba1. Light values and brand hues unchanged.
- `tokens.css`, dark-only rule: switch thumb on the on-track uses `--brand-ink` (was white, 2.29:1).
- Mockup HTML: 58 literal `color:#fff` on solid `--brand`/`--ok`/`--danger`/`--warn` fills → `color:var(--brand-ink)` (same white in light).
- Result: failing dark pairs 24 → 3 (all 3 disabled, in `nearby-skeleton--dark`); worst non-disabled ratio 1.14:1 → 3.15:1; worst text 2.04:1 → 4.71:1. Light: 0 regressions.
- Added `a11y/dark-contrast.md`, `a11y/dark-contrast.py`, `a11y/dark-before-after.png`, `a11y/light-spot-check.png`.

### Changed — backup copy (EN/FR, HTML + copy tables)
- Cloud backup = recovery phrase + all chats (text; media if media backup is on). Written phrase = identity + contacts, not chats.
- `onboarding-secure`, `onboarding-secure-phrase`, `onboarding-secure-reminder`, `onboarding-restore`, `settings-backup` (method lines, honest note, delete sheet), `restore-source`, `restore-error--corrupted`, `restore-password`, `restore-reinstall`.

### Re-rendered
- All 70 mockups (445 frames), using system python3 Playwright with `/usr/bin/google-chrome`.

### Constraints observed
- `brand/` untouched; light token values untouched; not pushed to git. Generator `/workspace/omcp-v0117/`. Mirror updated.

## v0.1.16 — 2026-10-24

One-tap Ask to resend, and Keep media you sent (decisions approved by Jakob). `DESIGN_SPEC.md` §5.100, §5.101.

### Changed
- `media-missing`: **Ask to resend** is now one tap with an optional **Add a note**; sent state shows status in the bubble and a toast with Undo. Re-rendered `--gone`, `--sent`, `--gone-fr`, `--sent-fr`; added `--ios-gone`. §5.94 confirm sheet marked superseded.

### Added
- `media-resend-note`: note field expanded (typed / empty) and sent-with-note state. 9 frames, EN/FR, light/dark, iOS.
- `settings-storage`: Settings › Storage with **Keep media you sent** (default 90 days or 2 GB, whichever first), edit sheet with presets (30 days–Forever, 500 MB–No limit), off state with warning. 13 frames.

### Removed
- `media-missing--confirm.png`, `--confirm-fr.png`, `--ios-confirm.png` (confirm-before-send dialog replaced).

### Constraints observed
- `brand/` and `tokens.css` unchanged. Fictional contacts only. Generator `/workspace/omcp-v0116/`. Mirror updated.

## v0.1.15 — 2026-10-24

Restored members in groups and communities (REQUIREMENTS, Q25). `DESIGN_SPEC.md` §5.99.

### Added
- `group-restored`: a compact “Restored, not verified” tag on sender names in group threads and in the member list; tapping it opens the same verify sheet; no system notice in groups. 11 frames, EN/FR, light/dark, iOS.

### Changed
- §12 Q24 (O3 reworded in REQUIREMENTS) and Q25 resolved. §5.96 notes that REQUIREMENTS accepts the optional “Back up media on mobile data” toggle.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.14 — 2026-10-24

Re-verification after restore (REQUIREMENTS, RS2). `DESIGN_SPEC.md` §5.98.

### Changed
- `restore-notice`: after a restore, contacts lose the verified badge. A “Restored, not verified” pill shows in the header and row until re-verified. The sheet says the safety number changed, with primary **Verify in person (QR)**. The restored phone's banner says contacts need to re-verify you, with Show my QR.
- `restore-success`, `restore-media`: “Your safety number changed. Contacts need to verify you again in person (QR).” Verified badges now read “Still verified on your side”.
- Earlier spec text saying verification carries over is annotated (§5.x backup note, Restoring row, §5.87, §5.95, Q15).

### Added
- QR re-verify frames (`--qr`, `--qr-fr`) and badge earned back (`--reverified`, `--list-reverified`, dark/iOS/FR).
- `restore-media--partial` (+`-fr`, `-dark`): a partial media restore falls back to placeholders that refill from nearby contacts.
- `settings-media-backup` copy: “Backed up daily on Wi-Fi”; the size card notes chats hourly, media daily.
- §12: Q20–Q23 resolved; Q24–Q25 added.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.13 — 2026-10-24

Media backup setting (REQUIREMENTS “Include media”, CB5). `DESIGN_SPEC.md` §5N (§5.96–5.97).

### Added
- `settings-media-backup`: “Also back up photos, videos and files” (off by default), per-type toggles, estimated size, Wi-Fi only with a mobile-data option, first-upload progress, nearly-full / full quota warnings, danger confirm to turn off and delete the media backup, and a deleting progress state. 14 frames, EN/FR, light/dark, Android/iOS.
- `restore-media`: detect and Welcome back when the backup includes media. Media is restored directly (hash checked); only types that were off come back via placeholders.

### Changed
- §5.92 placeholders now apply only to media not in the backup. §12 Q21–Q23 added.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.12 — 2026-10-24

Approved Q17–Q19 (REQUIREMENTS “Encrypted chat backup”). `DESIGN_SPEC.md` §5.94.

### Added
- `media-missing`: “Ask to resend” on the partner-no-longer-has-it state, a confirm sheet that previews the message before sending, a Sent state, and a “Waiting for Wi-Fi” state with a link to the setting (EN/FR, light/dark, iOS).
- `settings-missing-media`: Settings › Backup › Missing media, “Use mobile data for missing media” (off by default); Bluetooth mesh is always allowed.

- `restore-notice`: restore indicator (security). Contacts get an in-chat notice “<name> restored Hopla on a new device · <date>”, a sheet with Verify safety number / Block, and a 7-day “Restored” badge beside the verified badge in the row and header. The restored phone gets a banner: “You restored this account. Your contacts were notified.”

### Changed
- The restore is announced to contacts. Media re-requests carry no extra metadata (toast “Requested”). `restore-success` notes that contacts will see the restore. The Settings footnote saying contacts aren't told was removed.
- §12 Q17–Q19 resolved (2026-10-24). New Q20 (safety number after a restore).

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.11 — 2026-10-24

Text chat backup (REQUIREMENTS “Encrypted chat backup”, v0.1.4 text only). `DESIGN_SPEC.md` §5M (§5.91–5.93).

### Added
- `media-missing`: calm missing-media placeholders for photo, video, voice note and file. Keeps type, size, sender, time and caption. States: waiting, requested (with toast), syncing via mesh/gateway with progress, restored (swap animation 240 ms, reduced-motion instant), partner no longer has it, group. Thread banner “N media waiting”.
- `chats-restored`: chats list “Chats restored” card with “12 photos and 4 files waiting for partners”, plus per-row waiting pills.
- 14 + 5 frames in light/dark, Android/iOS, EN/FR.

### Changed
- `restore-detect`, `restore-progress`, `restore-success`: chats now come back. Size reads “2.4 MB · text only”. Success shows Restored / Coming back later / Not restored.
- §12 Q14 resolved. New Q17–Q19.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. No real names/photos. Mirror updated.

## v0.1.10 — 2026-10-24

Restore from backup for Google Drive and iCloud, reinstall-first. Extends §5.17/§5.26/§5.30/§5.31; `DESIGN_SPEC.md` §5L (§5.83–5.90).

### Added
- Hero screens: `restore-detect` (auto-detected backup card: date, size, device, protection; Start fresh confirm), `restore-password` (wrong, lockout, iCloud Keychain no-password, Keychain not synced), `restore-progress` (download → decrypt → contacts and chats, backgrounding on Android and iOS), `restore-success` (“Welcome back”, restored vs not restored).
- `restore-source` (Welcome › Restore fallback, system account picker / Google sign-in, No backup found), `restore-error` (offline, permission revoked, storage full, corrupted, version too new).
- 52 PNG frames, light/dark, Android/iOS, EN/FR. §12 Q14–Q16.

### Changed
- §5.31 `restore-reinstall` visuals superseded (rules unchanged). Settings › Backup has no Restore entry.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. No real names/photos. Mirror updated.

## v0.1.9 — 2026-10-24

Jakob resolved Q12/Q13; Q11 fixed in REQUIREMENTS (commit 6a0ee67). `DESIGN_SPEC.md` §5K (§5.81–5.82).

### Added
- `nearby-area-indicator`: persistent indicator in area mode (hex-cell glyph, violet, “Area mode” label) for iOS Live Activity, Dynamic Island and Android ongoing notification — normal, Private, paused, dark, FR.

### Changed
- Check-in counts visible time only: Settings › Nearby shows “Next one after 2 h 40 min more visible time”; Go Nearby sheet and in-app prompt copy updated (`settings-nearby`, `nearby-session`, `nearby-checkin` re-rendered).
- §12: Q11–Q13 resolved; no open questions.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.8 — 2026-10-24

Nearby stays on until the user turns it off (REQUIREMENTS Decision 4 + D6). `DESIGN_SPEC.md` §5J (§5.75–5.80).

### Added
- `nearby-checkin`: 4 h check-in notification (iOS, Android) and in-app prompt with Yes / Turn off; turned-off-after-no-answer note and notification.
- `settings-nearby`: Visible nearby toggle, auto-pause rules, check-in, indicator explanation.
- `nearby-session` indicator states: active and paused (no motion 2 h, night 01:00–07:00, battery < 20 %) for iOS Live Activity, Dynamic Island and Android ongoing notification, normal and Private, light/dark, EN/FR.

### Changed
- Go Nearby sheet: no duration choice; explains auto-pause and check-in. Status chip: “Live nearby” / “Paused” / “Area mode”; no countdown copy anywhere. Radar CTA “Go Nearby”.
- `nearby-discover`, `nearby-radar`, `nearby-area`, `nearby-skeleton`, `nearby-tab` regenerated.
- Older session-length notes reworded in this changelog.

### Removed
- Duration picker, extension action and countdown copy; their PNGs deleted from the pack and mirror.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.7 — 2026-10-24

Jakob approved the pending proposals; REQUIREMENTS Decision 5 / SS1 updated. `DESIGN_SPEC.md` §5I (§5.71–5.74).

### Changed
- `settings-privacy`: copy aligned to REQUIREMENTS (“On by default”; “Burn-after-read and view-once content is always protected”; second-camera line in On and Off). All 12 frames re-rendered.
- `nearby-session`: Private notifications hide counts on Live Activity, Dynamic Island and Android ongoing notification.
- `app-switcher-cover`: toggle-off states (normal preview) for iOS and Android.
- §12: Q4–Q10 resolved (2026-10-24); new Q11 (SS2 wording).

### Added
- PNGs: `nearby-session--ios-live-activity-private`, `--ios-island-private`, `--ios-island-expanded-private`, `--android-ongoing-private`, `--fr-live-activity-private`; `app-switcher-cover--ios-off`, `--android-off`.
- Generator `/workspace/omcp-v017/gen.py`.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. Mirror updated.

## v0.1.6 — 2026-10-24

Moderation & reputation (PROTOCOL §23, **v0.2**) and the maintainer’s Block-screenshots toggle. `DESIGN_SPEC.md` §5H (§5.61–5.70).

### Added
- `mod-report`, `mod-community` (Vouch, reputation indicator + explainer), `mod-states` (Under review — admins only, Reported, Restricted, Why? sheet, counter-signal), `mod-join-warning`, `mod-stream` (blur for new viewers, Continue watching?, reduced motion), `mod-appeal` (owner), `mod-jury`. 46 PNG frames.
- `settings-privacy` Block screenshots toggle: `--off`, `--confirm-off` (+ dark, iOS, FR). Burn-after-read and view-once stay protected when Off.
- §12 Q7–Q10.

### Changed
- `settings-privacy` regenerated (toggle replaces “Always on” badge); §5.57 footnote superseded.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. No real names/photos. Mirror updated.

## v0.1.5 — 2026-10-24

the maintainer’s decisions (REQUIREMENTS § Maintainer decisions, 2026-10-24). `DESIGN_SPEC.md` §5G (§5.56–5.60).

### Added
- `settings-privacy` (Screenshot protection · Always on, honest second-camera line), `app-switcher-cover` (iOS blurred brand cover, Android black FLAG_SECURE card, iOS recording/mirroring cover).
- Nearby v0.3: `nearby-session` (Go Nearby, iOS Live Activity, Dynamic Island compact + expanded, Android ongoing notification), `nearby-area`, `nearby-radar` (live radar empty, reduced motion), `nearby-skeleton` (0 / 300 / 900 ms timing). Status chip (“Live nearby · 42 min left” / “Area mode”). Motion table with durations + easing.
- `chat-info-notifications--opt-out` (+`-dark`, `-ios`, `-fr`).

### Changed
- Private calls: “Hopla · Incoming call”. Android Hide on lock screen: “Hopla · New message”, never SECRET. Per-chat Private can now be turned off.
- Nearby cards refined (photo scrim, frosted chips, page bars); likes counter moved under buttons; match screen refined (glow, icebreakers).
- Burn viewer copy mentions screenshot blocking + second camera.

### Removed
- All screenshot notices: `burn-bubbles--screenshot-ios.png`, `burn-bubbles--screenshot-android.png`, related copy keys.

### Constraints observed
- `brand/` and `tokens.css` unchanged (cover references `brand/hopla-v2/icon.svg` read-only). No GitHub push. No real names/photos. Mirror updated.

## v0.1.4 — 2026-10-24

Notifications (REQUIREMENTS § Notifications v0.1.2), Burn after read (§ Burn after read v0.1.3), and Hopla Nearby dating (§ Hopla Nearby, **v0.3**, opt-in).

### Added

- Notifications: `notif-lockscreen` (Full / Private / Hide on lock screen, Android + iOS, EN + FR, shade + history), `settings-notifications`, `chat-info-notifications` (incl. mute sheet). `DESIGN_SPEC.md` §5D (§5.36–5.40).
- Burn after read: `burn-composer` (long-press Send menu, armed), `burn-bubbles` (sealed / open with countdown ring / placeholders / iOS screenshot notice / Android FLAG_SECURE row), `burn-viewer` (view once, iOS recording), `burn-chat-info` (timer picker). §5F (§5.50–5.55).
- Nearby v0.3: `nearby-tab`, `nearby-onboarding` (18+ gate), `nearby-profile`, `nearby-discover` (buttons + optional swipe, reduced motion, empty, 100-likes limit), `nearby-match`, `nearby-chat` (Reveal identity: confirm / pending / incoming / revealed, blurred first photo), `nearby-safety` (match list, block, report 2-step, safety tips, share my date). §5E (§5.41–5.49). Illustrated abstract placeholders only.
- 14 HTML mockups, 96 PNG frames; generator `/workspace/omcp-v014/gen.py` (reuses v0.1.1 helpers; local `<style>` per page, tokens only).
- §12 open questions Q4–Q8.

### Changed

- v0.3 tab bar: 5 tabs (Nearby between Communities and Live). Chat info gains Notifications group and 🔥 Burn after read row.

### Constraints observed

- `brand/` and `tokens.css` unchanged. No GitHub push. No real people’s names or photos. Mirror updated.

## v0.1.3 — 2026-10-24

Verified badge as a reward; friendly invitation to verify. “Chat without verifying” stays on QR/NFC adds.

### Added

- `VerifiedBadge` (filled mint seal, `--brand`), `VerifyGhostSeal` (dashed, `--ink-3`, header only), `VerifyHintRow` (dismissible), “earn the badge” animation with reduced-motion fallback — `DESIGN_SPEC.md` §5C (§5.32–5.35), EN + FR copy.
- PNGs: `chats-list--dark`, `chats-list--ios`, `chat-thread--dark`, `--ios`, `--ios-dark`, `--unverified`, `--unverified-dark`, `--unverified-ios`, `--verified`, `--safety-details`.
- Generator `/workspace/omcp-v013/gen.py` (reuses v0.1.1 helpers); `chats-list.html` and `chat-thread.html` are now generated.

### Changed

- Chats list: amber “Unverified” badge on Morgan Reed removed; unverified rows show nothing. Verified mark redrawn as `VerifiedBadge` in brand mint (was `--ok` green).
- §2.1 `--warn` no longer used for unverified; §5.2, §5.3, §7, §9 updated.

### Constraints observed

- `brand/` and `tokens.css` unchanged (badge uses existing tokens; dark frames set `color:var(--ink)` inline). No GitHub push. Mirror updated.

## v0.1.2 — 2026-10-24

Simplified onboarding (4 screens: Welcome › Profile › Secure your account › Done) and recovery backup, per `apps/REQUIREMENTS.md` § Onboarding & recovery backup.

### Added

- Mockups + PNGs (390×844, `tokens.css`, Playwright/Chromium @2x): `onboarding-profile` (+`--filled`), `onboarding-secure` (+`--ios`, `--password-android`, `--password-ios`, `--success`, `--failure`), `onboarding-secure-phrase` (+`--checked`), `onboarding-secure-quiz` (+`--mismatch`), `onboarding-secure-reminder` (+`--expired`), `onboarding-done`, `settings-backup` (+`--queued`, `--view-phrase-auth`, `--delete-confirm`), `restore-reinstall` (+`--password`, `--wrong-password`, `--error`).
- `DESIGN_SPEC.md` §5B (§5.25–5.31): states, EN + FR copy, a11y, Android (Credential Manager, `drive.appdata`, `appDataFolder`) and iOS (CloudKit private DB, iCloud Keychain) notes; allowed security claims list; components `BackupMethodOption`, `BackupPasswordSheet`, `BackupStatusCard`, `BackupReminderBanner`.

### Changed

- `onboarding-welcome`: approved Hopla logo (`brand/hopla-v2/icon.svg`, inlined), “Get started” / “Restore”, 4-step stepper.
- §5.1 flow rewritten; §5.14 updated; §12 questions replaced.
- README screen index: new screens; `onboarding-recovery`, `onboarding-recovery-confirm`, `onboarding-permissions` marked **deprecated** (files kept, HTML titles/captions labelled).

### Constraints observed

- `brand/` and `tokens.css` unchanged; PROTOCOL.md not modified; no GitHub push. Mirror updated (excluding `brand/`).

## v0.1.1 — 2026-10-24

Screens the Android implementer needs next: onboarding with 24-word recovery, QR contact add with safety numbers, file transfer. Consumer app name is now **Hopla** (protocol stays OMCP).

### Added

- Mockups (390×844, `tokens.css`) + PNG renders (390 px @2x, Playwright/Chromium): `onboarding-welcome`, `onboarding-recovery` (+`--checked`), `onboarding-recovery-confirm` (+`--mismatch`), `onboarding-restore`, `onboarding-permissions` (+`--bluetooth-denied`), `contact-add-chooser`, `qr-scan` (+`--not-omcp`), `qr-my-code`, `contact-preview-safety` (+`--mismatch`), `file-picker-confirm` (+`--too-large`), `file-transfer`.
- `DESIGN_SPEC.md` §5A (§5.14–5.24): per-screen states, EN + FR copy, accessibility labels, Android/iOS notes; new components (`AppLogo`, `OnboardingStepper`, `PhraseWordField`, `PermissionCard`, `BlockingExplainer`, `ContactAddOption`, `MyQrCard`, `ContactPreviewSheet`, `FileSendConfirmSheet`).
- `tokens.css`: v0.1.1 component classes appended (no existing token values changed).
- `README.md`: screen index.

### Changed

- User-facing copy uses “Hopla” (notification default “New Hopla message”, Android foreground notification). Logo shown as a neutral placeholder tile; `brand/` untouched.
- §12 open questions: v0.1.0 tab visibility, gateway cap (500 MB/day, Wi‑Fi-only, warn on metered) and preview defaults recorded as decided; three new questions.
- README audience line de-duplicated (aligned with mirror).

### Constraints observed

- PROTOCOL.md not modified; no GitHub push; no “military grade” / “unhackable” copy.
- Mirror `/home/box/shared/omcp-design/` updated to match (excluding `brand/`).

## v0.1.0 — 2026-10-23

First strong design pass for OMCP smartphone apps (Android 12+, iOS 17+).

### Added

- `README.md` — entry point; MVP vs v0.2; Proggy consumption guide.
- `DESIGN_SPEC.md` — principles; light/dark WCAG AA tokens; type & spacing; IA; tab/stack navigation; screen specs for onboarding, chats, 1:1 & groups, contact add, file progress, gateway disclosure, path indicators & queue, security centre, communities, channels, live (incl. BLE-only empty state), admin tools sketch; Android/iOS platform notes; EN/FR copy principles (§13.5); component inventory for Kotlin/SwiftUI.
- `mockups/tokens.css` — shared design tokens.
- Static HTML phone frames (390×844) + PNG screenshots:
  - `chats-list`
  - `chat-thread` (path banner + file progress)
  - `security-centre`
  - `gateway-settings`
  - `communities-home` (v0.2 sketch)
  - `live-tab` (v0.2 sketch, BLE-only video callout)
- Mirror tree at `/home/box/shared/omcp-design/` for Proggy/Pipely.

### Constraints observed

- PROTOCOL.md not modified; no GitHub push.
- Path indicators calm (mesh ≠ alert).
- No “military grade” / “unhackable” copy.
