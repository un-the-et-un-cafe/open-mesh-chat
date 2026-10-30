# OMCP Smartphone Apps — Design Spec v0.1.22

**Platforms:** Android 12+ (Kotlin UI + Rust core), iOS 17+ (SwiftUI + Rust core)  
**Date:** 2026-10-24 (v0.1.1; v0.1.0 2026-10-23)  
**Consumer app name:** Hopla (protocol remains OMCP)  
**References:** PROTOCOL.md §13 (apps), §13.5 (copy), §20 (communities & livestreams); apps/REQUIREMENTS.md

---

## 1. Design principles

1. **Mesh-first clarity.** The user always knows whether a message rides local BLE, a neighbour’s Internet gateway, or is queued. Path state is calm and informative — never alarmist for ordinary mesh use.
2. **Trust through concreteness.** Show verifiable facts (“Verified contact”, “End-to-end · Noise hybrid”) instead of marketing superlatives. Forbidden: “military grade”, “unhackable”, “NSA-proof”.
3. **Privacy by default.** Notification previews off; gateway and store-and-forward opt-in with plain disclosure; recovery phrase shown once with clear “write this down” UX.
4. **One-thumb, low chrome.** Primary actions ≥44×44 pt; list rows ≥56 pt; decorative shadows/gradients avoided; system fonts only.
5. **Bilingual from day one.** English and French string parity; layouts tolerate +40 % text expansion; no text baked into images.
6. **Accessible.** WCAG 2.1 AA contrast; Dynamic Type / fontScale to 200 %; TalkBack / VoiceOver labels on path chips, toggles, and verification states; colour never the sole status carrier (icon + text).
7. **Platform honest.** Surface OS limits (especially iOS background BLE) in product copy, not as buried footnotes.

---

## 2. Design tokens

Implemented in `mockups/tokens.css` as CSS custom properties. Map 1:1 into native themes.

### 2.1 Colour (WCAG AA)

| Token | Light | Dark | Role |
|-------|-------|------|------|
| `--bg` | `#f3f5f8` | `#0c1219` | App background |
| `--surface` | `#ffffff` | `#151d28` | Bars, cards, bubbles (them) |
| `--surface-2` | `#eef1f5` | `#1c2633` | Inputs, secondary fills |
| `--ink` | `#0f1b2d` (≥12:1 on bg) | `#e8eef5` (≥12:1) | Primary text |
| `--ink-2` | `#3b495a` (9.2:1 on surface, ≥7.8:1 on every light tint, v0.1.18) | `#b4c0ce` (≥6.2:1 on every dark tint, v0.1.17) | Secondary text |
| `--ink-3` | `#515e6d` (6.6:1 on surface, ≥5.6:1 on every light tint and hatch, v0.1.18) | `#95a3b4` (≥4.7:1 on every dark tint and hatch, v0.1.17) | Tertiary / timestamps |
| `--line` | `#73849a` (3.8:1 on surface, ≥3.25:1 on every light tint, v0.1.18) | `#687990` (≥3:1 on bg/surface/surface-2, v0.1.17) | Dividers, input / control boundaries |
| `--line-strong` | `#617289` (4.9:1 on surface, v0.1.18) | `#7a8ba1` (v0.1.17) | Toggle off-track, checkbox and field borders |
| `--brand` | `#0b6e6a` (white on brand ≥4.5:1) | `#3dbdb6` | Primary actions, active tab |
| `--brand-soft` | `#e0f3f1` | `#143836` | Sent bubble, soft fills |
| `--ok` / soft | `#0f6b3a` / `#e3f4ea` | `#5dca8e` / `#143525` | Verified |
| `--warn` / soft | `#8a5a10` / `#faf0d8` | `#e0b45a` / `#3a2e14` | BLE-only live, queue (no longer used for unverified contacts, v0.1.3) |
| `--danger` / soft | `#9b1c2e` / `#fce8eb` | `#f07178` / `#3a1820` | Block, destructive |
| `--focus` | `#0a66c2` | `#8ec8ff` | Focus ring ≥3 px |

**Path colours** (calm; mesh must not read as “warning”):

| Path | Token pair | Meaning |
|------|------------|---------|
| Mesh | `--path-mesh` `#0b6e6a` / soft `#e0f3f1` | Local BLE only |
| Gateway | `--path-gateway` `#1a5f9e` / soft `#e4eef8` | Via neighbour Internet |
| Mixed | `--path-mixed` `#5a4a9e` / soft `#eeebf8` | Combination in thread |
| Queued | `--path-queue` `#8a5a10` / soft `#faf0d8` | Waiting for a path |

Dark mode mirrors with lighter chroma on soft backgrounds (see `tokens.css`).

### 2.2 Typography

System stack only:

```
system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif
```

Mono (safety numbers, PeerID): `ui-monospace, "SF Mono", Menlo, Consolas, monospace`.

| Step | Size / line | Weight | Use |
|------|-------------|--------|-----|
| xs | 11–12 / 1.4 | 600 | Tab labels, path chip micro |
| sm | 13 / 1.45 | 400–600 | Meta, badges, timestamps |
| body | 15–16 / 1.45 | 400 | Messages, settings desc (≥16 px inputs on iOS) |
| title | 17–18 / 1.3 | 600–700 | Nav titles, row titles |
| display | 22–28 / 1.2 | 700 | Onboarding headlines |

Respect Dynamic Type / `sp` scaling to 200 %. Do not truncate safety numbers; wrap in mono grid.

### 2.3 Spacing, radii, hit targets

- 4-pt scale: 4 / 8 / 12 / 16 / 24 / 32 / 48.
- Radii: 6 (sm), 10 (controls), 14 (cards), pill 999.
- Min touch target 44×44 pt; list row min height 56–64.
- Separation via 1 px borders; avoid heavy shadows.

---

## 3. Information architecture

```
Root tabs (MVP + v0.2 shell)
├── Chats          ← MVP primary
│   ├── Chat list (path chips, unread, verified mark)
│   ├── 1:1 thread (+ path banner, composer, file progress)
│   ├── Group thread (MLS ≤32)
│   ├── Contact add (QR / NFC / safety number)
│   └── Chat security sheet → Security centre
├── News           ← v0.4 (Top · Local · World · topics; reader; alerts; interests + city; share + threads) §5R
├── Communities    ← v0.2 (from v0.4 also hosts the Live entry, §5.107) (tab visible but empty/gated in MVP builds optional)
│   ├── Community home (announcement + topics)
│   ├── Channel / topic thread
│   └── Admin tools (roles, slow mode, tombstone)
├── Live           ← v0.2
│   ├── Live list + BLE-only empty / limited state
│   └── Start/join (lan-relay / internet-sfu)
└── You            ← MVP
    ├── Profile / PeerID / QR of self
    ├── Gateway settings
    ├── Security & privacy (global)
    ├── Backup / restore
    ├── Notifications (previews OFF default)
    └── About / open source / licences
```

**Onboarding (pre-tabs, v0.1.2):** Welcome → Profile → Secure your account (cloud and/or Write it down, or Remind me later) → Done (QR + nearby, Bluetooth asked in context) → Home. Fresh install with backup → Restore on reinstall.

**Modal / sheets:** Safety number compare, gateway disclosure on first enable, file send confirm (size), block confirm, session reset confirm.

---

## 4. Navigation

| Pattern | Android | iOS |
|---------|---------|-----|
| Root | `NavigationBar` 4 destinations (Material 3) | `TabView` 4 tabs |
| Stack | Nav host per tab | `NavigationStack` per tab |
| Chat → Security | Push or modal sheet | Push |
| Add contact | Full-screen cover (camera/NFC) | Full-screen cover |
| Gateway first-on | Modal disclosure (must scroll + confirm) | Same |

Back behaviour: system back / swipe; path banner is not a navigation element.

**MVP shipping tip:** If Communities/Live are not ready, keep tabs but show a short “Coming in v0.2” empty state rather than removing tabs (avoids IA churn). Product may instead hide those two tabs behind a feature flag — decide before beta (open question Q1).

---

## 5. Screen-by-screen specs

### 5.1 Onboarding — identity + recovery backup (MVP, revised v0.1.2)

**Goals:** a new user is chatting in under 60 s with no account, phone number or email; nobody loses their identity because they skipped writing down 24 words; the phrase is never uploaded in plaintext.

**Flow (v0.1.2, ≤ 4 screens — REQUIREMENTS O1):**

1. **Welcome** (§5.14) — value proposition, **Get started**; secondary **Restore** (24-word fallback).
2. **Profile** (§5.25) — display name + photo, both optional; identity key + phrase generated in background.
3. **Secure your account** (§5.26) — one or more of Google Drive (Android default) / iCloud (iOS default) / Write it down (§5.27); “Remind me later” = 24 h with banner (§5.29).
4. **Done** (§5.28) — your QR code + nearby add over BLE (Bluetooth asked here, in context) → Chats.

**Restore:** fresh install with a cloud backup → §5.31; otherwise Welcome › Restore → 24 words (§5.17). Management: Settings › Backup (§5.30).

**Superseded (v0.1.1):** separate Recovery phrase (§5.15), Confirm phrase (§5.16) and Permissions (§5.18) steps — kept for reference, mockups labelled deprecated.

**Copy anchors (EN):** “Write these 24 words down and keep them private.” / “Nobody can read your messages — not even gateways.”  
**FR:** “Notez ces 24 mots et gardez-les privés.” / “Personne ne peut lire vos messages — pas même les passerelles.”

### 5.2 Chats list (MVP) — mockup `chats-list`

- Title “Chats”; actions Search, Add.
- **Status strip** under nav: aggregate `PathChip` (e.g. “Mesh · 4 nearby”) + optional “N queued”.
- Rows: avatar, title + verified check, preview, time, unread pill, **per-row path micro-chip** when not default mesh.
- Verified contacts: mint `VerifiedBadge` right after the name. Unverified contacts: **nothing** in the row (v0.1.3, see §5.32).
- Empty: illustration-free; “Add someone nearby with QR or NFC.”

### 5.3 1:1 chat thread (MVP) — mockup `chat-thread`

- Nav: back, name + `VerifiedBadge` (verified) or `VerifyGhostSeal` (unverified), “Nearby · last seen…”, shield → Security centre. Unverified threads add `VerifyHintRow` (§5.33).
- **Path banner** (full width, calm colour): e.g. “Via mesh · 2 hops · encrypted end-to-end” / “Sent via neighbour’s Internet” / “Waiting for mesh”.
- Bubbles: them = surface+border; me = brand-soft. Meta line: time · delivery (Sending / Queued / Sent / Delivered / Read) · path word if differs from banner.
- File bubble: type tile, name, size, progress bar, cancel while sending.
- Composer: attach, field, send. No GIFs/stickers in MVP.

### 5.4 Group chat (MVP)

Same as 1:1 with: group avatar (rounded square), member count subtitle, “N members · MLS”, admin overflow (add/remove member, leave). Path banner reflects group fan-out (often Mixed when some members via gateway). Member list links to per-contact safety numbers.

### 5.5 Contact add — QR / NFC / safety number (MVP)

1. **Chooser:** Scan QR | My QR | NFC tap | Enter PeerID (advanced).
2. **Scan:** Camera with OMCP QR frame; on hit → preview sheet: display name (if any), PeerID, **Safety number**, actions Verify & chat / Cancel.
3. **NFC:** “Hold phones together”; same preview sheet.
4. **Safety number UI:** 60 decimal digits in 12 groups of 5 (PROTOCOL §3.3), mono, large tap-to-mark-verified. States: Unverified | Verified | Revoked.
5. After verify → chat opens; verified mark appears on list/thread.

**v0.1.1 detail:** §5.19–5.22 (mockups `contact-add-chooser`, `qr-scan`, `qr-my-code`, `contact-preview-safety`).

### 5.6 File transfer progress (MVP)

- In-thread card (see mockup chat-thread).
- Optional persistent notification on Android while transfer ≥ few seconds (content: “Sending file…” — **no filename** if notification previews off).
- Failures: “Couldn’t finish — will retry on mesh” with Retry / Cancel. Max 50 MB; oversize blocked before send with plain size message.
- **v0.1.1 detail:** §5.23 `file-picker-confirm`, §5.24 `file-transfer`.

### 5.7 Gateway toggle + battery/privacy disclosure (MVP) — mockup `gateway-settings`

- Entry: You → Gateway.
- Master toggle **Act as gateway** — first enable opens mandatory disclosure sheet:
  - Helps nearby peers reach the Internet.
  - Forwards **encrypted** envelopes only; you cannot read content.
  - Uses battery and may use mobile data.
  - Confirm CTA: “Enable gateway”.
- Settings: Allow on mobile data (off default), daily data cap + meter, store-and-forward (off default on mobile), legend of path chips.
- Android: enabling gateway implies willingness to run foreground service (see §8).

### 5.8 Path indicators + offline queue (MVP)

| State | Chip / banner label (EN) | Label (FR) |
|-------|--------------------------|------------|
| Mesh | Via mesh | Via le mesh |
| Gateway | Via neighbour’s Internet / Gateway | Via Internet d’un voisin / Passerelle |
| Mixed | Mixed paths | Chemins mixtes |
| Queued | Waiting for mesh / Queued | En attente du mesh / En file |

- Queue: messages stay in composer-sent state with queue chip; auto-send on path restore; user can delete from queue long-press.
- Never use red/danger styling for mesh or queue; danger reserved for block / security failure.

### 5.9 Security centre (MVP) — mockup `security-centre`

Per-contact (from chat shield) and global (You → Security):

- Identity header: avatar, name, PeerID, verification badge.
- Safety number block + Mark verified / Scan QR.
- Session: encryption summary (E2EE · Noise hybrid · PQ ready when negotiated), last refresh, **Reset session**.
- Block contact (danger button + confirm).
- Global: notification preview toggle (OFF default), passphrase lock (optional), backup, blocked list, “What gateways can see” explainer.

### 5.10 Communities home (v0.2) — mockup `communities-home`

- List of communities user belongs to; each expands or navigates to: Announcement channel + topic channels (`#general`, etc.).
- Role chip on header (Owner / Admin / Moderator / Member / Guest).
- Join via invite link / QR only (no global directory in v0.2).
- Roster encrypted to members; public communities may show cleartext name/description only (PROTOCOL §20.1).

### 5.11 Channel / topic (v0.2)

- Announcement: admin-only compose; members read + react; signed posts.
- Topic: MLS group UX like group chat; lazy join; invite = signed capability token.
- Path banner still applies for delivery path.

### 5.12 Live tab (v0.2) — mockup `live-tab`

- Lists streams from communities.
- **BLE-only empty / limited state (normative UX):** callout “Video needs Wi‑Fi or Internet” + live cards in `mesh-beacon` mode show title/status only, badge “No video path”, CTA “Connect to Wi‑Fi to go live / watch”.
- Modes: `mesh-beacon` | `lan-relay` | `internet-sfu` (PROTOCOL §20.3). Media keys never on SFU.
- Start stream gated until lan-relay or internet-sfu available.

### 5.13 Admin tools sketch (v0.2)

From community → Admin:

- Roles management (capability tokens signed by community identity key).
- Slow mode duration.
- Remove member / ban (encrypted control frames).
- Delete-for-all (admin-signed tombstone).
- No plaintext moderation API on relays — UI copy must not imply server-side takedown.

---

## 5A. v0.1.1 per-screen specs — onboarding, contact add, file transfer

Added 2026-10-24. Consumer app name is **Hopla**; the protocol stays **OMCP** (use “OMCP” only where the user meets a protocol artefact, e.g. “not an OMCP code”, `omcp1:` PeerIDs, About). Logo is a **neutral placeholder tile** until `brand/hopla-v2/` lands — implement as a swappable `AppLogo` slot. Mockups: `mockups/<screen>.html`; extra states render as `mockups/<screen>--<state>.png`.

Shared rules for every screen below:
- Light + dark tokens from `tokens.css`; mockups render light; dark is token-driven, no separate art.
- Hit targets ≥ 48 dp / 44 pt; Dynamic Type / `sp` to 200 % — word grids reflow 3 → 2 columns at ≥ 150 %.
- Disabled primary buttons use `.btn.disabled` (surface-2 fill, ink-3 text) and expose `aria-disabled` / `isEnabled=false` **plus** a hint saying why.
- Never “military grade”, “unhackable”, “completely anonymous”.
- Onboarding progress bar = 5 steps: Welcome · Phrase · Confirm · Permissions · Done (name step from §5.1 sits between Welcome and Phrase but is not counted as a step; it is optional).

### 5.14 Onboarding — Welcome — `onboarding-welcome` (updated v0.1.2)

**Purpose:** one screen, value proposition, **Get started**. No account, phone number or email. Secondary **Restore** opens the 24-word restore (§5.17); cloud restore is offered automatically by §5.31 when a backup is detected.

| State | Notes |
|-------|-------|
| Default | Stepper 1/4, Hopla logo (`brand/hopla-v2/icon.svg`, 88 pt, inlined), app name, tagline, body, reassurance callout, Get started, Restore, footnote |
| Backup detected | Not shown — §5.31 replaces this screen |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Tagline | Chat over Bluetooth mesh. Internet optional. | Discutez via le mesh Bluetooth. Internet facultatif. | צ׳אט ברשת Mesh בבלוטות׳. אינטרנט לא חובה. |
| Body | No account, phone number or email needed. Your identity is created on this phone. | Pas de compte, de numéro de téléphone ni d’e-mail. Votre identité est créée sur ce téléphone. | בלי חשבון, בלי מספר טלפון ובלי אימייל. הזהות שלך נוצרת בטלפון הזה. |
| Reassurance | Nobody can read your messages — not even gateways. | Personne ne peut lire vos messages — pas même les passerelles. | אף אחד לא יכול לקרוא את ההודעות שלך — גם לא השערים (gateways). |
| Primary | Get started | Commencer | בואו נתחיל |
| Secondary | Restore | Restaurer | שחזור |
| Footnote | Built on the open OMCP protocol | Basé sur le protocole ouvert OMCP | מבוסס על פרוטוקול OMCP הפתוח |

**A11y:** logo `role=img` “Hopla logo”; heading level 1 = “Hopla”. Focus order: heading → body → Get started → Restore.
**Android:** edge-to-edge, no toolbar; system back exits app. **iOS:** no nav bar; swipe-back disabled on this root.

### 5.15 Onboarding — Recovery phrase — `onboarding-recovery` (+ `--checked`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** show 24 BIP-39-style words **once**, get a written copy. Never uploaded, never in backups in plaintext.

| State | Notes |
|-------|-------|
| Unchecked (default) | Grid 3×8 numbered words (mono), screenshot warning callout, checkbox off, **Continue disabled** |
| Checked | Checkbox on, Continue enabled |
| Screenshot detected | Toast + callout turns danger: “Screenshot taken. Delete it from your photos and any cloud backup.” (iOS `userDidTakeScreenshotNotification`; Android 14+ `ScreenCaptureCallback`) |
| Back pressed | Confirm dialog “Leave without saving your phrase? You’ll get a new one next time.” |

Copy button: **absent** by default (spec §5.1 “Copy disabled”); optional long-press-to-enable is out of v0.1.1 mockups.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Recovery phrase | Phrase de récupération | ביטוי שחזור |
| Headline | Write these 24 words down and keep them private. | Notez ces 24 mots et gardez-les privés. | רשמו את 24 המילים האלה ושמרו אותן בסוד. |
| Body | On paper, in order. They are the only way to restore your identity on a new phone. Hopla never uploads them. | Sur papier, dans l’ordre. C’est le seul moyen de restaurer votre identité sur un nouveau téléphone. Hopla ne les envoie jamais. | על נייר, לפי הסדר. הן הדרך היחידה לשחזר את הזהות שלך בטלפון חדש. Hopla אף פעם לא מעלה אותן. |
| Warning | Don’t take a screenshot — screenshots can sync to cloud photo backups. | Ne faites pas de capture d’écran — elles peuvent être synchronisées avec vos sauvegardes photo en ligne. | לא לצלם מסך — צילומי מסך עלולים להסתנכרן לגיבוי התמונות בענן. |
| Checkbox | I wrote it down and stored it somewhere safe. | Je l’ai notée et rangée en lieu sûr. | רשמתי ושמרתי במקום בטוח. |
| Disabled hint | Tick the box to continue | Cochez la case pour continuer | סמנו את התיבה כדי להמשיך |
| CTA | Continue | Continuer | המשך |

**A11y:** grid `role=list`, each word read as “Word 1, anchor”; TalkBack/VoiceOver reads in numeric order (column-major visual reading is not used — row-major 1-2-3). Checkbox is a single target including its label. Continue disabled hint is announced.
**Android:** `WindowManager.LayoutParams.FLAG_SECURE` on this screen (blocks screenshots and recents thumbnail) — the warning is still shown for photos taken with another device. **iOS:** cannot block screenshots; hide words in app switcher snapshot (blur overlay on `sceneWillResignActive`). *(v0.1.5: superseded by §5.57 — app-wide secure layer on iOS too; keep the warning, reword to “Don’t photograph these words with another device” in a future pass.)*

### 5.16 Onboarding — Confirm phrase — `onboarding-recovery-confirm` (+ `--mismatch`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** verify 3 random positions (never consecutive, never all from the same row).

| State | Notes |
|-------|-------|
| In progress | Field per position; correct field gets ok border + check; focused field shows autocomplete chips (prefix match, max 3) |
| Mismatch | Wrong field danger border + inline error; others keep state; Confirm disabled |
| All correct | Confirm enabled → Permissions |
| 3 failed attempts | Callout suggests “Show phrase again” (no lockout; phrase still on device) |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Check your phrase | Vérifiez votre phrase | בדיקת הביטוי |
| Headline | Enter 3 words from your list | Saisissez 3 mots de votre liste | הזינו 3 מילים מהרשימה שלכם |
| Body | Positions are random each time. This proves the phrase is written down correctly. | Les positions changent à chaque fois. Cela confirme que la phrase est bien notée. | המיקומים אקראיים בכל פעם. כך מוודאים שהביטוי נרשם נכון. |
| Field label | Word #{n} | Mot n° {n} | מילה מס׳ {n} |
| Error | That’s not word #{n}. Check your paper copy. | Ce n’est pas le mot n° {n}. Vérifiez votre copie papier. | זו לא מילה מס׳ {n}. בדקו בעותק הכתוב. |
| Secondary | Show phrase again | Revoir la phrase | להציג שוב את הביטוי |
| CTA | Confirm | Confirmer | אישור |

**A11y:** errors `role=alert` / `announceForAccessibility`; chips are buttons “Suggestion: fossil”. Keyboard: `autocorrect off`, `autocapitalize none`, `textContentType`/`importantForAutofill = no`, incognito keyboard flag on Android (`IME_FLAG_NO_PERSONALIZED_LEARNING`).

### 5.17 Onboarding — Restore — `onboarding-restore`

**Purpose:** enter 24 words, restore identity + local encrypted backup if present (PROTOCOL §13.6.5).

| State | Notes |
|-------|-------|
| Empty | 24 dashed slots, first focused |
| Entering | Filled slots solid; focused slot with brand outline; chips for prefix matches; tap chip or space/enter commits |
| Invalid word | Slot danger + “Word {n} “{w}” isn’t in the word list. Did you mean “{s}”?” |
| Checksum fail (all 24 valid) | Callout danger “These 24 words don’t form a valid phrase. Check the order.” |
| Paste | Pasting 24 space-separated words fills all slots; clipboard cleared after 60 s (Android 13+ also hides clipboard preview via `EXTRA_IS_SENSITIVE`) |
| Restoring | Full-screen progress “Restoring identity…” then “Restored 14 contacts” |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Restore identity | Restaurer l’identité | שחזור זהות |
| Headline | Enter your 24 words | Saisissez vos 24 mots | הזינו את 24 המילים שלכם |
| Body | Type each word in order. Suggestions come from the standard word list only. | Saisissez chaque mot dans l’ordre. Les suggestions viennent uniquement de la liste standard. | הקלידו כל מילה לפי הסדר. ההצעות מגיעות רק מרשימת המילים הסטנדרטית. |
| Backup note | Your 24 words restore your identity and contacts, not your chats. To get your chats back, restore from your Google Drive or iCloud backup. | Vos 24 mots restaurent votre identité et vos contacts, pas vos discussions. Pour retrouver vos discussions, restaurez depuis votre sauvegarde Google Drive ou iCloud. | 24 המילים משחזרות את הזהות ואנשי הקשר, לא את הצ׳אטים. כדי לקבל בחזרה את הצ׳אטים, שחזרו מהגיבוי ב-Google Drive או ב-iCloud. | *(v0.1.17: replaces the “local encrypted backup” wording; only the cloud backup holds chats.)* *(v0.1.14: your own verifications of others are restored, but every contact must re-verify **you**, because your safety number changes on restore, §5.98.)*|
| CTA | Restore | Restaurer | שחזור |

**A11y:** slot labels “Word 7 of 24, invalid”; progress announced “13 of 24 words entered”. Same keyboard flags as §5.16.

### 5.18 Onboarding — Permissions — `onboarding-permissions` (+ `--bluetooth-denied`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** pre-prompt explainer before each OS dialog. Decided defaults: notification **previews off**.

| State | Notes |
|-------|-------|
| Default | 3 cards: Bluetooth (Required), Notifications (Optional), Local network (Optional · iOS only — card hidden on Android) |
| Granted | Card shows ok badge “Allowed” |
| Bluetooth denied | Replace screen with blocking explainer: icon, reason, how-to path, Open Settings / Try again. Continue unavailable — user can’t reach mesh features |
| Bluetooth off (radio) | Same explainer, CTA “Turn on Bluetooth” (Android `ACTION_REQUEST_ENABLE`) |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Headline | Let Hopla reach people nearby | Laissez Hopla joindre les personnes à proximité | אפשרו ל-Hopla להגיע לאנשים בסביבה |
| Sub | We explain each one before your phone asks. | Nous expliquons chaque autorisation avant que votre téléphone ne la demande. | נסביר כל הרשאה לפני שהטלפון יבקש אותה. |
| Bluetooth | Needed to send and receive messages over the mesh. Required. | Nécessaire pour envoyer et recevoir des messages via le mesh. Obligatoire. | נדרש כדי לשלוח ולקבל הודעות ברשת ה-Mesh. חובה. |
| Notifications | Know when a message arrives. Previews stay off: you’ll see “New Hopla message”, not the content. Change in Settings. | Soyez averti à l’arrivée d’un message. Les aperçus restent désactivés : vous verrez « Nouveau message Hopla », pas le contenu. Modifiable dans les réglages. | לדעת כשמגיעה הודעה. התצוגה המקדימה כבויה: יופיע ״הודעה חדשה ב-Hopla״, בלי התוכן. אפשר לשנות בהגדרות. |
| Local network | iOS only. Lets Hopla find phones on the same Wi‑Fi for faster transfers later. | iOS uniquement. Permet à Hopla de trouver les téléphones sur le même Wi‑Fi pour des transferts plus rapides. | ב-iOS בלבד. מאפשר ל-Hopla למצוא טלפונים באותה רשת Wi‑Fi להעברות מהירות יותר בהמשך. |
| Denied title | Hopla needs Bluetooth to work | Hopla a besoin du Bluetooth pour fonctionner | Hopla צריכה בלוטות׳ כדי לעבוד |
| Denied body | Messages travel phone-to-phone over Bluetooth. Without it, you can’t join the mesh or add contacts nearby. | Les messages passent de téléphone à téléphone via Bluetooth. Sans lui, impossible de rejoindre le mesh ou d’ajouter des contacts à proximité. | ההודעות עוברות מטלפון לטלפון בבלוטות׳. בלעדיו אי אפשר להצטרף לרשת ה-Mesh או להוסיף אנשי קשר בסביבה. |
| Denied CTAs | Open Settings · Try again | Ouvrir les réglages · Réessayer | פתיחת ההגדרות · לנסות שוב |

**A11y:** each card is a group with heading; badges read as text (“Required”). Denied screen title is the focused heading on appear.
**Android:** request `BLUETOOTH_SCAN` (`neverForLocation`), `BLUETOOTH_CONNECT`, `BLUETOOTH_ADVERTISE` together (Nearby devices); `POST_NOTIFICATIONS` on 13+; after 2 denials the OS stops prompting → go straight to explainer + `ACTION_APPLICATION_DETAILS_SETTINGS`. Foreground-service notification (§8.1) starts after BLE granted. **iOS:** `NSBluetoothAlwaysUsageDescription`, `NSLocalNetworkUsageDescription`; Bluetooth denial is final until Settings → explainer path `Settings › Hopla › Bluetooth`; notification request uses `.alert, .badge, .sound` with `UNNotificationContent` showing generic body (previews off).

### 5.19 Contact add — Chooser — `contact-add-chooser`

Full-screen cover from Chats “+”. Four options; Enter PeerID grouped under **Advanced**.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Add contact | Ajouter un contact | הוספת איש קשר |
| Intro | Meet in person for the strongest check. Each option shows a safety number before you chat. | Rencontrez-vous en personne pour la vérification la plus sûre. Chaque option affiche un numéro de sécurité avant de discuter. | הכי בטוח לבדוק פנים אל פנים. כל אפשרות מציגה מספר בטיחות לפני שמתחילים לשוחח. |
| Scan | Scan QR code — Point your camera at their Hopla code | Scanner un QR code — Visez le code Hopla de votre contact | סריקת קוד QR — כוונו את המצלמה לקוד ה-Hopla של איש הקשר |
| My QR | Show my QR code — Let them scan you | Afficher mon QR code — Laissez-le vous scanner | הצגת קוד ה-QR שלי — שיסרקו אתכם |
| NFC | Tap phones (NFC) — Hold the backs of both phones together | Toucher les téléphones (NFC) — Collez le dos des deux téléphones | הצמדת טלפונים (NFC) — הצמידו את הגב של שני הטלפונים |
| PeerID | Enter PeerID — Paste an omcp1:… ID. You’ll need to compare safety numbers separately. | Saisir un PeerID — Collez un identifiant omcp1:… Vous devrez comparer les numéros de sécurité séparément. | הזנת PeerID — הדביקו מזהה ‎omcp1:…‎. תצטרכו להשוות מספרי בטיחות בנפרד. |

**States:** NFC row hidden when hardware absent; on iOS NFC row shows only if reader session available (iPhone reads; HCE emulation is Android-only → on iOS, NFC = read-only, other phone must be Android). **A11y:** rows are buttons with combined label+description.

### 5.20 Contact add — Scan QR — `qr-scan` (+ `--not-omcp`)

| State | Notes |
|-------|-------|
| Scanning | Dark camera, white corner viewfinder 240 pt, hint, torch / from photo / my code buttons |
| Torch on | Torch button inverted (white fill), `aria-pressed=true` |
| Not OMCP | Toast danger at top, viewfinder corners turn danger for 2 s, scanning continues |
| Camera denied | Empty state “Camera access is off” + Open Settings + “Show my QR code instead” |
| Hit | Haptic + transition to preview sheet (§5.22) |

Valid code = URI `omcp:` / `omcp1:` PeerID + SAS payload (PROTOCOL §3.3). Anything else (URLs, Wi‑Fi, other apps) → not-OMCP error; **never** open URLs from scanned codes.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Hint | Point at their Hopla QR code. It scans automatically. | Visez le QR code Hopla. Le scan est automatique. | כוונו לקוד ה-QR של Hopla. הסריקה אוטומטית. |
| Low light | Too dark? Turn on the torch. | Trop sombre ? Allumez la lampe. | חשוך מדי? הדליקו את הפנס. |
| Error title | Not a Hopla contact code | Ce n’est pas un code contact Hopla | זה לא קוד איש קשר של Hopla |
| Error body | This QR isn’t an OMCP identity code. Ask them to open Add contact › Show my QR code. | Ce QR n’est pas un code d’identité OMCP. Demandez-lui d’ouvrir Ajouter un contact › Afficher mon QR code. | קוד ה-QR הזה אינו קוד זהות של OMCP. בקשו מאיש הקשר לפתוח הוספת איש קשר › הצגת קוד ה-QR שלי. |
| Buttons | Torch · From photo · My code | Lampe · Depuis une photo · Mon code | פנס · מתמונה · הקוד שלי |

**A11y:** viewfinder `role=img` “Camera viewfinder”; error announced as alert; VoiceOver hint “Move phone slowly over the code”. **Android:** CameraX + ML Kit barcode (bundled model, no Play-services download dependency for de-Googled devices — fallback ZXing). **iOS:** `AVCaptureMetadataOutput` `.qr`; `DataScannerViewController` acceptable on iOS 17.

### 5.21 Contact add — My QR — `qr-my-code`

| Element | Notes |
|---------|-------|
| QR | Black on white always (even dark mode), quiet zone ≥ 4 modules, centre logo slot (placeholder tile ≤ 20 % area, ECC level H) |
| Name + PeerID | PeerID short form `omcp1:xxxx…xxxx` (first 4 / last 4 of base32); “Copy full PeerID” |
| Brightness | Raise to max on appear, restore on leave; hint callout |
| Actions | Share (system share sheet: text `omcp1:…` + QR PNG), Scan theirs |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | My QR code | Mon QR code | קוד ה-QR שלי |
| Brightness hint | Screen brightness is raised while this code is shown. Ask them to scan from Hopla › Add contact. | La luminosité est augmentée pendant l’affichage du code. Demandez-lui de scanner depuis Hopla › Ajouter un contact. | בהירות המסך מוגברת בזמן שהקוד מוצג. בקשו מאיש הקשר לסרוק מ-Hopla › הוספת איש קשר. |
| Privacy note | Your code contains only your public PeerID — no phone number or location. | Votre code ne contient que votre PeerID public — ni numéro ni position. | הקוד שלך כולל רק את ה-PeerID הציבורי — בלי מספר טלפון ובלי מיקום. |
| Actions | Share · Scan theirs · Copy full PeerID | Partager · Scanner le sien · Copier le PeerID complet | שיתוף · סריקת הקוד שלהם · העתקת ה-PeerID המלא |

**A11y:** QR `role=img` “Your Hopla contact QR code”; PeerID read character-grouped. **Android:** `window.attributes.screenBrightness = 1f`. **iOS:** `UIScreen.main.brightness` save/restore.

### 5.22 Contact add — Preview + safety number — `contact-preview-safety` (+ `--mismatch`)

Bottom sheet over the camera after a scan/NFC hit.

| State | Notes |
|-------|-------|
| New contact (unverified) | Avatar, name (if shared), PeerID short, warn badge Unverified, `SafetyNumberBlock` 4×3 grid of 12 groups × 5 digits, Verify & chat / Chat without verifying / Cancel |
| Mismatch (known contact, number changed) | Danger badge + callout, differing groups underlined + danger; actions Scan again / Don’t trust this code / Cancel. No “chat” action |
| Verified | Success haptic → thread opens with verified mark |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Section | Safety number | Numéro de sécurité | מספר בטיחות |
| Compare | Compare these 60 digits with the number on {name}’s screen. If they match, nobody is in the middle. | Comparez ces 60 chiffres avec le numéro affiché chez {name}. S’ils correspondent, personne ne s’est interposé. | השוו את 60 הספרות האלה למספר שמופיע במסך של {name}. אם הם זהים, אף אחד לא נמצא באמצע. |
| Primary | Numbers match — Verify & chat | Les numéros correspondent — Vérifier et discuter | המספרים תואמים — אימות וצ׳אט |
| Secondary | Chat without verifying | Discuter sans vérifier | צ׳אט בלי אימות |
| Mismatch title | Safety numbers don’t match | Les numéros de sécurité ne correspondent pas | מספרי הבטיחות לא תואמים |
| Mismatch body | This code is different from the one saved for {name}. Their identity may have changed, or someone may be impersonating them. Check in person before chatting. | Ce code diffère de celui enregistré pour {name}. Son identité a peut-être changé, ou quelqu’un se fait passer pour lui/elle. Vérifiez en personne avant de discuter. | הקוד הזה שונה מזה ששמור עבור {name}. ייתכן שהזהות שלו השתנתה, או שמישהו מתחזה. בדקו פנים אל פנים לפני שמשוחחים. |
| Mismatch actions | Scan again · Don’t trust this code | Scanner à nouveau · Ne pas faire confiance | סריקה חוזרת · לא לסמוך על הקוד הזה |

**A11y:** safety number read group-by-group (“Group 1: 3 3 7 6 6”), each group a focusable element; at 200 % the grid reflows to 2 columns (never horizontal scroll). Mismatched groups announced “differs”. Sheet is modal; focus trapped; Escape/back = Cancel. **Android:** `ModalBottomSheet`, `skipPartiallyExpanded=true`. **iOS:** `.sheet` with `.presentationDetents([.large])`.

### 5.23 File send — Picker confirm — `file-picker-confirm` (+ `--too-large`)

After the system picker (Android `ACTION_OPEN_DOCUMENT` / Photo Picker; iOS `PHPickerViewController` / `UIDocumentPickerViewController`), before encryption starts.

| State | Notes |
|-------|-------|
| Confirm | File tile, name, type; rows Size (“{x} MB of 50 MB max”), Type, Route (`PathChip`), Estimated time; path hint callout; Cancel / Send |
| Route = gateway | Callout: “This goes via a neighbour’s Internet. It counts toward their daily gateway allowance.” (gateway default 500 MB/day, Wi‑Fi only) |
| Route = queue | Callout: “{name} isn’t reachable now. The file will send when they’re back in range.” Send label → “Queue” |
| Too large (> 50 MB) | Danger file card, callout with exact size; Send replaced by “Choose another”. Checked **before** any encryption/chunking |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Send file to {name}? | Envoyer le fichier à {name} ? | לשלוח קובץ ל{name}? |
| Size row | {x} MB of 50 MB max | {x} Mo sur 50 Mo max | {x} מ״ב מתוך 50 מ״ב לכל היותר |
| Mesh hint | Via mesh is slower for large files. It keeps sending in the background and resumes if {name} moves out of range. | Via le mesh, les gros fichiers sont plus lents. L’envoi continue en arrière-plan et reprend si {name} s’éloigne. | דרך Mesh איטי יותר לקבצים גדולים. השליחה ממשיכה ברקע ומתחדשת אם {name} יוצא/ת מהטווח. |
| Too-large title | File is too large | Fichier trop volumineux | הקובץ גדול מדי |
| Too-large body | {x} MB is over the 50 MB limit. Hopla can send files up to 50 MB. Try a shorter clip or compress it first. | {x} Mo dépasse la limite de 50 Mo. Hopla envoie des fichiers jusqu’à 50 Mo. Essayez un extrait plus court ou compressez-le. | {x} מ״ב חורג מהמגבלה של 50 מ״ב. Hopla יכולה לשלוח קבצים עד 50 מ״ב. נסו קטע קצר יותר או דחסו אותו קודם. |
| Actions | Cancel · Send · Choose another | Annuler · Envoyer · Choisir un autre | ביטול · שליחה · בחירת קובץ אחר |

Estimated time: core supplies throughput estimate; show “~{n} min nearby” only when ≥ 1 min, otherwise hide row. Units: decimal MB (1 MB = 10⁶ B) — 50 MB limit is 50 000 000 B to match core; confirm with Proggy (see Q).
**A11y:** sheet heading is title; Route chip label “Route: mesh, local Bluetooth only”.

### 5.24 File transfer in thread — `file-transfer`

Extends §5.3 file bubble / §5.6. One mockup shows all states. PROTOCOL §7: encrypted chunks (16 KiB) with per-chunk AEAD; resume = re-request missing chunks, so **paused is resumable without restart**.

| State | Line copy (EN) | FR | Progress | Actions |
|-------|----------------|----|----------|---------|
| Completed (received) | {size} · Received · meta “Integrity checked” | {size} · Reçu · « Intégrité vérifiée » | full brand | tap = open |
| Sending | Sending · {a} of {b} MB · ~{n} min | Envoi · {a} sur {b} Mo · ~{n} min | brand | Pause, Cancel (icon buttons) |
| Receiving | Receiving · {a} of {b} MB | Réception · {a} sur {b} Mo | brand | Cancel |
| Paused (out of range) | Paused · out of range · {a} of {b} MB · meta “Waiting for mesh · resumes automatically” | En pause · hors de portée · … · « En attente du mesh · reprise automatique » | `--path-queue` | Resume now |
| Queued | Queued · {size} · next in line | En file d’attente · {size} · prochain envoi | empty | — |
| Failed | Couldn’t finish — will retry on mesh | Échec — nouvel essai via le mesh | danger, frozen at last % | Retry, Cancel |
| Integrity fail (receiver) | File didn’t match — ask {name} to resend | Le fichier ne correspond pas — demandez à {name} de le renvoyer | danger | Delete |

Rules: one active upload per contact (others queue); transfers survive app background on Android via foreground service; notification text “Sending file…” / « Envoi d’un fichier… » with **no filename** while previews are off. Meta path word follows §5.3 (only when differs from banner).
**A11y:** progress `role=progressbar` with value text “41 percent, about 3 minutes left”; announce state changes politely (not every %, max every 10 %). Icon buttons labelled with filename (“Pause sending market-flyer.jpg”).
**Android:** foreground service type `dataSync`/`connectedDevice`; WorkManager for gateway leg. **iOS:** background BLE limits → banner “iOS may pause transfers in background — keep Hopla open to finish faster” when app backgrounds mid-transfer.

---

## 5B. v0.1.2 — Simplified onboarding + recovery backup

Source of truth: `apps/REQUIREMENTS.md` § “Onboarding & recovery backup (v0.1.1)”. Acceptance: O1 fresh install → chat list in **≤ 4 screens, ≤ 60 s** with a cloud backup. Stepper = `OnboardingStepper` with 4 segments. Write-it-down screens are a sub-flow **inside step 3** (stepper stays at 3/4) and don’t count toward O1 for cloud users.

```
Welcome (1) → Profile (2) → Secure your account (3) ─┬─ cloud: password sheet → success / failure ─┐
                                                      ├─ Write it down → 3-word check ─────────────┤→ Done (4) → Chats
                                                      └─ Remind me later → banner 24 h → blocking ─┘
Fresh install with a backup found → Restore on reinstall (replaces Welcome)
```

**Security claims allowed in copy (and only these):** phrase is encrypted on the phone before upload; the provider (Google/Apple) can’t read it; the cloud backup holds the recovery phrase **and all chats** (text; media only if media backup is on); the written phrase alone restores identity and contacts, **not chats**; anyone with the cloud account **and** the backup password can restore; forgotten password ⇒ written phrase needed; backups use only the platform cloud over the Internet, never mesh/gateways; offline ⇒ queued and retried. Do **not** claim “zero-knowledge”, “unhackable”, “military grade”, or that a cloud backup can’t be restored by anyone else.

### 5.25 Profile — `onboarding-profile` (+ `--filled`)

| State | Notes |
|-------|-------|
| Empty | Avatar placeholder (camera glyph), name field placeholder “Your name (optional)”, primary reads **Skip**; footnote “Creating your ID…” while identity key + 24-word phrase generate in background |
| Filled | Initials or photo, primary **Continue**, footnote shows PeerID short form |
| Key-gen failed | Callout danger “Couldn’t create your identity. Try again.” (same as §5.14) |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Your profile | Votre profil | הפרופיל שלך |
| Heading | How should contacts see you? | Comment vos contacts vous verront-ils ? | איך אנשי הקשר יראו אותך? |
| Sub | Both are optional. You can change them later. | Les deux sont facultatifs. Vous pourrez les modifier plus tard. | שניהם אופציונליים. אפשר לשנות אחר כך. |
| Photo | Add photo / Change photo | Ajouter une photo / Changer la photo | הוספת תמונה / החלפת תמונה |
| Field | Display name · Your name (optional) | Nom affiché · Votre nom (facultatif) | שם תצוגה · השם שלך (אופציונלי) |
| Privacy | Your name and photo are shared only with your contacts — never with a server or people nearby you haven’t added. | Votre nom et votre photo ne sont partagés qu’avec vos contacts — jamais avec un serveur ni avec des personnes à proximité que vous n’avez pas ajoutées. | השם והתמונה שלך משותפים רק עם אנשי הקשר שלך — אף פעם לא עם שרת או עם אנשים בסביבה שלא הוספת. |
| Primary | Skip / Continue | Passer / Continuer | דילוג / המשך |

**A11y:** photo button label “Add profile photo, optional”; field label “Display name, optional”. Footnote is `aria-live=polite` so “Your ID is ready” is announced.  
**Android:** Photo Picker (`PickVisualMedia`), no storage permission. **iOS:** `PhotosPicker`, no library permission prompt.

### 5.26 Secure your account — `onboarding-secure` (+ `--ios`, `--password-android`, `--password-ios`, `--success`, `--failure`)

Multi-select list (`BackupMethodOption`, checkbox, 72 pt min). At least one method must complete before leaving onboarding, or “Remind me later”. Platform default is pre-selected and badged **Recommended**: Android → Google Drive; iOS → iCloud, with Google Drive also offered on iOS. **No cloud method is enabled without this explicit tap** (preselection is visual; upload only starts after Continue + password sheet). If the user deselects everything, Continue is disabled.

| State | Notes |
|-------|-------|
| Android default | Google Drive (Recommended, selected), Write it down |
| iOS default | iCloud (Recommended, selected), Google Drive, Write it down |
| Password sheet — Drive | Password + confirm, 4-segment strength meter (zxcvbn ≥ 3 and ≥ 8 chars required; meter green only when met), CTA “Back up to Google Drive” disabled until valid. Google account picked via Credential Manager before the sheet (system UI) |
| Password sheet — iCloud | Same + checkbox “No password — protect with a key in iCloud Keychain instead”; when checked, the fields collapse |
| Uploading | CTA spinner “Encrypting and uploading…”, sheet not dismissible (Argon2id ≈ 1 s on mid-range) |
| Success | Check mark, “Backed up to {service}”, time, kv card (method, protected by, account), secondary “Also write it down”, primary Continue → Done |
| Failure — offline | “Backup didn’t finish”, callout: queued + auto-retry, never over mesh/gateways. Actions: Try again, Write it down instead, Continue — retry later. Continue counts as a method **pending**; banner from §5.29 applies until upload succeeds |
| Failure — auth/quota | Same layout; body “{service} said no: {reason}.” e.g. “Your Google Drive is full.” No queueing |
| Write it down selected | Continue opens §5.27 |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Secure your account | Sécurisez votre compte | אבטחת החשבון |
| Heading | Don’t lose your identity | Ne perdez pas votre identité | לא לאבד את הזהות שלך |
| Sub | Pick one or more ways to back up Hopla. A cloud backup saves your recovery phrase and all your chats. | Choisissez une ou plusieurs façons de sauvegarder Hopla. Une sauvegarde cloud enregistre votre phrase de récupération et toutes vos discussions.| בחרו דרך אחת או יותר לגבות את Hopla. גיבוי בענן שומר את ביטוי השחזור ואת כל הצ׳אטים שלך. |
| Drive (Android) | Google Drive · Encrypted on this phone with a backup password before upload. Google can’t read it. | Google Drive · Chiffrée sur ce téléphone avec un mot de passe de sauvegarde avant l’envoi. Google ne peut pas la lire. | Google Drive · מוצפן בטלפון הזה עם סיסמת גיבוי לפני ההעלאה. Google לא יכולה לקרוא אותו. |
| Drive (iOS) | Google Drive · Also available on iPhone. Needs a backup password. | Google Drive · Aussi disponible sur iPhone. Nécessite un mot de passe de sauvegarde. | Google Drive · זמין גם ב-iPhone. נדרשת סיסמת גיבוי. |
| iCloud | iCloud · Encrypted on this phone before upload. Apple can’t read it. | iCloud · Chiffrée sur ce téléphone avant l’envoi. Apple ne peut pas la lire. | iCloud · מוצפן בטלפון הזה לפני ההעלאה. Apple לא יכולה לקרוא אותו. |
| Write it down | See your 24 words, then answer a quick 3-word check. | Affichez vos 24 mots, puis répondez à une vérification rapide de 3 mots. | הצגת 24 המילים שלך, ואחר כך בדיקה קצרה של 3 מילים. |
| Badge | Recommended | Recommandé | מומלץ |
| Honest note | Anyone with your {Google/iCloud} account **and** your backup password can restore your identity and all your chats. If you forget the password, your written phrase still restores your identity and contacts, but not your chats. | Toute personne ayant accès à votre compte {Google/iCloud} **et** à votre mot de passe de sauvegarde peut restaurer votre identité et toutes vos discussions. Si vous oubliez le mot de passe, votre phrase écrite restaure quand même votre identité et vos contacts, mais pas vos discussions.| כל מי שיש לו גישה לחשבון {Google/iCloud} שלך **וגם** לסיסמת הגיבוי יכול לשחזר את הזהות שלך ואת כל הצ׳אטים. אם שכחת את הסיסמה, הביטוי הכתוב עדיין משחזר את הזהות ואנשי הקשר, אבל לא את הצ׳אטים. |
| Actions | Continue · Remind me later | Continuer · Me le rappeler plus tard | המשך · להזכיר לי מאוחר יותר |
| Sheet title | Create a backup password | Créez un mot de passe de sauvegarde | יצירת סיסמת גיבוי |
| Sheet body (Drive) | Your phrase and chats are encrypted with this password on your phone, then saved in a hidden Hopla folder in your Google Drive. *(iCloud: “…then saved in iCloud.”)* | Votre phrase et vos discussions sont chiffrées avec ce mot de passe sur votre téléphone, puis enregistrées dans un dossier Hopla masqué de votre Google Drive. *(iCloud : « …puis enregistrées dans iCloud. »)*| הביטוי והצ׳אטים שלך מוצפנים עם הסיסמה הזו בטלפון, ואז נשמרים בתיקיית Hopla מוסתרת ב-Google Drive שלך. *(iCloud: ״…ואז נשמרים ב-iCloud.״)* |
| Sheet body (iCloud) | …then saved in iCloud. | …puis enregistrée dans iCloud. | …ואז נשמרים ב-iCloud. |
| No password | No password — protect with a key in iCloud Keychain instead | Sans mot de passe — protéger avec une clé dans le trousseau iCloud | בלי סיסמה — הגנה במפתח ב-iCloud Keychain במקום |
| Hint | At least 8 characters. Hopla can’t reset it for you. | Au moins 8 caractères. Hopla ne peut pas le réinitialiser. | לפחות 8 תווים. Hopla לא יכולה לאפס אותה בשבילך. |
| Strength | Too weak · Strong password | Trop faible · Mot de passe robuste | חלשה מדי · סיסמה חזקה |
| CTA | Back up to {service} | Sauvegarder sur {service} | גיבוי ל-{service} |
| Success | Backed up to {service} · Encrypted on this phone before upload. | Sauvegardé sur {service} · Chiffré sur ce téléphone avant l’envoi. | גובה ל-{service} · הוצפן בטלפון הזה לפני ההעלאה. |
| Failure | Backup didn’t finish · We couldn’t reach {service}. Nothing was uploaded. · You’re offline. Your encrypted backup is queued and will upload automatically when you’re back online. Hopla never sends backups over the mesh or gateways. | La sauvegarde n’a pas abouti · Impossible de joindre {service}. Rien n’a été envoyé. · Vous êtes hors ligne. Votre sauvegarde chiffrée est en attente et sera envoyée automatiquement dès votre retour en ligne. Hopla n’envoie jamais de sauvegarde via le mesh ou les passerelles. | הגיבוי לא הסתיים · לא הצלחנו להגיע ל-{service}. שום דבר לא הועלה. · אין חיבור לאינטרנט. הגיבוי המוצפן ממתין בתור ויועלה אוטומטית כשהחיבור יחזור. Hopla אף פעם לא שולחת גיבויים דרך רשת ה-Mesh או השערים. |
| Failure actions | Try again · Write it down instead · Continue — retry later | Réessayer · L’écrire plutôt · Continuer — réessayer plus tard | לנסות שוב · לרשום על נייר במקום · המשך — לנסות שוב אחר כך |

**A11y:** each option is one focusable checkbox: “{title}, {description}, recommended, checked”. Honest note is read after the options, before Continue. Strength meter exposes text (“Strong password”), not colour alone. Password fields support password managers but `autocomplete="new-password"`; no paste block.  
**Android:** Credential Manager account picker → OAuth scope **`drive.appdata` only**; blob stored in **`appDataFolder`** (hidden from the user’s Drive UI and other apps — copy says “hidden Hopla folder”, never “invisible to Google”). Don’t request full Drive scope. **iOS:** blob in **CloudKit private database** (`OMCPBackup` record). No-password key in **iCloud Keychain** (`kSecAttrSynchronizable`, `kSecAttrAccessibleAfterFirstUnlock`) — “end-to-end encrypted by Apple” is only said of the Keychain key, not of CloudKit. If the user isn’t signed in to iCloud, iCloud option shows “Sign in to iCloud in Settings” and is disabled. Google Drive on iOS uses Google Sign-In.

### 5.27 Write it down — `onboarding-secure-phrase` (+ `--checked`) and `onboarding-secure-quiz` (+ `--mismatch`)

Adapts deprecated §5.15/§5.16: same grid, screenshot warning, checkbox and 3-random-word check (autocomplete from BIP-39 list). Changes: stepper fixed at 3/4; heading copy unchanged; body no longer says “Hopla never uploads them” (cloud backup now uploads an *encrypted* copy) → “They can always restore your identity, even without a cloud backup.”; CTA “Check my words”; “Show words again”. No skip on the quiz itself — the way out is Back to §5.26 (choose another method or Remind me later). Success → Done (or back to §5.26 success if a cloud method is also selected).

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Write it down / Check your words | Notez-la / Vérifiez vos mots | לרשום על נייר / בדיקת המילים |
| Body | On paper, in order. They always restore your identity and contacts, even without a cloud backup. Your chats come back only from a cloud backup. | Sur papier, dans l’ordre. Ils restaurent toujours votre identité et vos contacts, même sans sauvegarde cloud. Vos discussions ne reviennent qu’avec une sauvegarde cloud.| על נייר, לפי הסדר. הן תמיד משחזרות את הזהות ואנשי הקשר שלך, גם בלי גיבוי בענן. הצ׳אטים חוזרים רק מגיבוי בענן. |
| CTA | Check my words | Vérifier mes mots | בדיקת המילים שלי |
| Quiz | Enter 3 words from your list · Positions are random each time. | Saisissez 3 mots de votre liste · Les positions changent à chaque fois. | הזינו 3 מילים מהרשימה שלכם · המיקומים אקראיים בכל פעם. |
| Error | That’s not word #{n}. Check your paper copy. | Ce n’est pas le mot n° {n}. Vérifiez votre copie papier. | זו לא מילה מס׳ {n}. בדקו בעותק הכתוב. |

**A11y / platform:** as §5.15–5.16 (`FLAG_SECURE` on Android; iOS screenshot notification → warning toast). *(v0.1.5: superseded by §5.57.)*

### 5.28 Done — `onboarding-done`

Hopla logo (`brand/hopla-v2/icon.svg`, inlined), “You’re all set”, `MyQrCard` (QR with logo, name, PeerID), `ContactAddOption` “Add someone nearby” (BLE). Bluetooth permission is requested **here, in context** (replaces deprecated §5.18): tapping the nearby option shows the system prompt; deny → `BlockingExplainer` from §5.18. Notifications are asked on first received message. Primary “Start chatting” → Chats.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Heading | You’re all set | Tout est prêt | הכול מוכן |
| Sub | Show this code so people can add you. | Montrez ce code pour qu’on puisse vous ajouter. | הציגו את הקוד הזה כדי שיוכלו להוסיף אתכם. |
| Nearby | Add someone nearby · Find people next to you over Bluetooth. We’ll ask for Bluetooth access first. | Ajouter quelqu’un à proximité · Trouvez les personnes autour de vous via Bluetooth. Nous vous demanderons d’abord l’accès au Bluetooth. | הוספת מישהו בסביבה · למצוא אנשים לידך דרך בלוטות׳. קודם נבקש גישה לבלוטות׳. |
| Primary | Start chatting | Commencer à discuter | להתחיל לשוחח |

**A11y:** QR `role=img` “Your Hopla contact QR code”; brightness raised as §5.21. **Android:** `BLUETOOTH_SCAN`/`CONNECT`/`ADVERTISE` with `neverForLocation`. **iOS:** `NSBluetoothAlwaysUsageDescription` at first nearby tap.

### 5.29 Remind me later — `onboarding-secure-reminder` (+ `--expired`)

“Remind me later” goes straight to Done. For 24 h a persistent warn banner sits under the Chats nav (not dismissible; tap → §5.26). After 24 h an `alertdialog` sheet blocks use until a method completes (O6). A queued-but-not-uploaded cloud backup counts as pending (banner text “Waiting for Internet to finish your backup”), not as complete.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Banner | Back up your account — {n} h left before Hopla pauses. · Back up | Sauvegardez votre compte — encore {n} h avant que Hopla se mette en pause. · Sauvegarder | גבו את החשבון — נשארו {n} שע׳ עד ש-Hopla תושהה. · גיבוי |
| Blocking | Back up to keep using Hopla · It’s been 24 hours. Choose a backup method so you can’t lose your identity. A cloud backup also keeps your chats; until then they’re only on this phone. · Choose a backup method | Sauvegardez pour continuer à utiliser Hopla · 24 heures se sont écoulées. Choisissez une méthode de sauvegarde pour ne pas perdre votre identité. Une sauvegarde cloud garde aussi vos discussions ; d’ici là, elles ne sont que sur ce téléphone. · Choisir une méthode | גבו כדי להמשיך להשתמש ב-Hopla · עברו 24 שעות. בחרו שיטת גיבוי כדי לא לאבד את הזהות שלך. גיבוי בענן שומר גם את הצ׳אטים; עד אז הם נמצאים רק בטלפון הזה. · בחירת שיטת גיבוי |

**A11y:** banner `role=status`; blocking sheet `role=alertdialog`, focus trapped, system back doesn’t dismiss (Android) / no swipe-down (iOS). Incoming messages keep being received while blocked (open question 1).

### 5.30 Settings › Backup — `settings-backup` (+ `--queued`, `--view-phrase-auth`, `--delete-confirm`)

Entry: Settings › Backup and Security centre (§5.9) row “Backup”.

| Item | Notes |
|------|-------|
| Status card | Badge Backed up / Waiting for Internet / Not backed up (danger) / Failed; last-backup date+time; **Back up now** (secondary) |
| Methods | Per method row: name, account, protection (password / iCloud Keychain key), On badge. Written phrase row shows date the quiz was passed. “Add or remove a method” → §5.26 list in edit mode. Removing the last method triggers the §5.29 24 h rule |
| Change backup password | Old password → new password sheet (§5.26 rules) → re-encrypt and re-upload |
| View recovery phrase | Gated by biometric **or device PIN** (`BiometricPrompt` with `DEVICE_CREDENTIAL` / `LAPolicy.deviceOwnerAuthentication`), then §5.27 grid read-only |
| Delete cloud backup | Danger; confirm sheet; deletes the remote record (O5); shows success toast “Backup deleted from {service}” |
| Honest note | As §5.26 plus “never the mesh or gateways” |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Title | Backup | Sauvegarde | גיבוי |
| Status | Backed up · Waiting for Internet · Last backup: {date} · new backup queued | Sauvegardé · En attente d’Internet · Dernière sauvegarde : {date} · nouvelle sauvegarde en attente | מגובה · ממתין לאינטרנט · גיבוי אחרון: {date} · גיבוי חדש בתור |
| Method lines (v0.1.17) | {Cloud}: Phrase + all chats · {account} · {protection} — Written phrase: Identity + contacts, not chats · {date checked} | {Cloud} : Phrase + toutes les discussions · {compte} · {protection} — Phrase écrite : Identité + contacts, sans les discussions · {date de vérification} | {Cloud}: ביטוי + כל הצ׳אטים · {account} · {protection} — ביטוי כתוב: זהות + אנשי קשר, בלי צ׳אטים · {date checked} |
| Honest note (v0.1.17) | Anyone with your {service} account **and** backup password can restore your identity and all your chats. Backups use only {provider}’s servers over the Internet — never the mesh or gateways. | Toute personne ayant accès à votre compte {service} **et** à votre mot de passe de sauvegarde peut restaurer votre identité et toutes vos discussions. Les sauvegardes passent uniquement par les serveurs de {provider} via Internet — jamais par le mesh ni les passerelles. | כל מי שיש לו גישה לחשבון {service} שלך **וגם** לסיסמת הגיבוי יכול לשחזר את הזהות שלך ואת כל הצ׳אטים. הגיבויים עוברים רק דרך השרתים של {provider} באינטרנט — אף פעם לא דרך רשת ה-Mesh או השערים. |
| Actions | Back up now · Add or remove a method · Change backup password · View recovery phrase · Delete cloud backup | Sauvegarder maintenant · Ajouter ou retirer une méthode · Changer le mot de passe de sauvegarde · Afficher la phrase de récupération · Supprimer la sauvegarde cloud | גיבוי עכשיו · הוספה או הסרה של שיטה · שינוי סיסמת הגיבוי · הצגת ביטוי השחזור · מחיקת הגיבוי בענן |
| Auth | Confirm it’s you · Use your fingerprint or device PIN to show your recovery phrase. · Use PIN | Confirmez votre identité · Utilisez votre empreinte ou le code de l’appareil pour afficher votre phrase de récupération. · Utiliser le code | אימות זהות · השתמשו בטביעת אצבע או בקוד המכשיר כדי להציג את ביטוי השחזור. · שימוש בקוד |
| Delete | Delete Google Drive backup? · The backup, including your chats, is removed from Google Drive. Your written phrase still restores your identity and contacts, but not your chats. If you have no other method, you’ll need to set one up within 24 hours. · Delete backup · Cancel | Supprimer la sauvegarde Google Drive ? · La sauvegarde, discussions comprises, est supprimée de Google Drive. Votre phrase écrite restaure quand même votre identité et vos contacts, mais pas vos discussions. Sans autre méthode, vous devrez en configurer une sous 24 heures. · Supprimer · Annuler | למחוק את הגיבוי ב-Google Drive? · הגיבוי, כולל הצ׳אטים, יימחק מ-Google Drive. הביטוי הכתוב עדיין משחזר את הזהות ואנשי הקשר, אבל לא את הצ׳אטים. אם אין לכם שיטה אחרת, תצטרכו להגדיר אחת תוך 24 שעות. · מחיקת הגיבוי · ביטול |

**A11y:** status badge text, not colour; destructive row announced “Delete cloud backup, button, destructive”. **Android/iOS:** same layout; iOS shows “iCloud” and “iCloud Keychain key” in the protection line when no password.

### 5.31 Restore on reinstall — `restore-reinstall` (+ `--password`, `--wrong-password`, `--error`) — *extended v0.1.10 by §5L (new frames supersede these visuals; rules here stay normative)*

On first launch the app checks Drive `appDataFolder` (after Credential Manager account pick) / CloudKit private DB. If a backup exists this screen replaces Welcome; otherwise Welcome shows. The 24-word restore (`onboarding-restore`, §5.17) is always reachable.

| State | Notes |
|-------|-------|
| Detected | Logo, “Welcome back”, kv card (account, created, device hint from blob), Restore from {service} · Use my 24 words instead · Start fresh |
| Password | Shown only if the blob has a password KDF; iCloud no-password backups skip straight to restoring |
| Wrong password | Inline error with remaining tries; after 3 failures, 5-minute cooldown with countdown (O4 rate limit); phrase fallback always enabled |
| Restoring | Progress “Restoring contacts and verification…” then Chats (O3) | *(v0.1.14: “verification” means your verifications of others; contacts' verification of you resets, §5.98.)*
| Error | Danger callout (offline / not signed in / record unreadable); Try again · Use my 24 words instead · Start fresh |
| Start fresh | Confirm: “Start fresh? Your old identity stays in {service} until you delete it.” |

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Heading | Welcome back | Bon retour | ברוכים השבים |
| Detected | We found a Hopla backup in your {service}. | Nous avons trouvé une sauvegarde Hopla dans votre {service}. | מצאנו גיבוי של Hopla ב-{service} שלך. |
| Actions | Restore from {service} · Use my 24 words instead · Start fresh | Restaurer depuis {service} · Utiliser mes 24 mots · Repartir de zéro | שחזור מ-{service} · שימוש ב-24 המילים שלי במקום · להתחיל מחדש |
| Password | Enter the backup password you chose for this backup. · Forgot it? Hopla can’t reset it. Restore with your 24 words instead; your chats won’t come back. | Saisissez le mot de passe choisi pour cette sauvegarde. · Oublié ? Hopla ne peut pas le réinitialiser. Restaurez plutôt avec vos 24 mots ; vos discussions ne reviendront pas. | הזינו את סיסמת הגיבוי שבחרתם לגיבוי הזה. · שכחת? ל-Hopla אין אפשרות לאפס אותה. אפשר לשחזר עם 24 המילים במקום; הצ׳אטים לא יחזרו. |
| Wrong | Wrong password. {n} tries left, then wait 5 minutes. | Mot de passe incorrect. Encore {n} essais, puis patientez 5 minutes. | סיסמה שגויה. נשארו {n} ניסיונות, ואז המתנה של 5 דקות. |
| Error (iOS) | We couldn’t load your iCloud backup. · Check that you’re online and signed in to iCloud with the same Apple Account, then try again. | Impossible de charger votre sauvegarde iCloud. · Vérifiez que vous êtes en ligne et connecté à iCloud avec le même compte Apple, puis réessayez. | לא הצלחנו לטעון את הגיבוי מ-iCloud. · ודאו שיש חיבור לאינטרנט ושאתם מחוברים ל-iCloud עם אותו Apple Account, ונסו שוב. |

**A11y:** error `role=alert`; cooldown countdown announced once per minute, not every second.

## 5C. v0.1.3 — Verified badge and verify invitation

Decision (the maintainer, 2026-10-24): “Chat without verifying” stays on QR/NFC adds. Verification is framed as a **reward**: verified contacts earn a mint badge; unverified contacts get a friendly invitation in the thread only. No red, no amber (amber is the queue path colour), no warning copy.

### 5.32 `VerifiedBadge` — `chats-list` (+`--dark`, `--ios`), `chat-thread` (+`--dark`, `--ios`, `--ios-dark`, `--verified`, `--safety-details`)

| Property | Value |
|---|---|
| Shape | 12-point scalloped seal, filled, with knocked-out check (stroke 2.4 on 24 grid). Hopla’s own shape — not Instagram’s blue rosette. |
| Fill / check | `--brand` (light `#0b6e6a`, dark `#3dbdb6`) / `--surface` |
| Size | 15 px in list row (row-title 15–16 px), 16 px in header; scales with Dynamic Type / font scale (`1em`-ish). 4 px gap after name. |
| Placement | Immediately after the display name, before any truncation ellipsis (name truncates, badge never does). |
| Header hit target | 44×44 pt (iOS) / 48×48 dp (Android) around the 16 px glyph → opens **Safety-number details** sheet (`--safety-details`): “Verified contact”, date + method, 12 groups of 5 digits, “Scan Alex’s code again”, “Remove verification”. |
| List row | Not separately tappable (row opens the chat). |
| A11y | Row: appended to the name — “Alex Rivera, Verified”. Header: button “Verified. Show safety number” / “Vérifié. Afficher le numéro de sécurité”. Contrast vs surface: 6.4:1 light, 8.1:1 dark (≥3:1 non-text). |
| Path indicators | No clash: path indicators are pill chips with a dot + label living in the row aside / banner; the badge is a glyph attached to the name and never carries a label or pill. Mesh shares the brand hue by design, shape and position separate them. |
| Groups | No badge on group rows (MLS groups have no single safety number). |

### 5.33 Unverified: `VerifyGhostSeal` + `VerifyHintRow` — `chat-thread--unverified` (+`-dark`, `-ios`)

- **Header ghost seal:** same seal outline, dashed (2.6/2), stroke `--ink-3`, check in `--ink-3`, no fill. 16 px, 44/48 hit target. Tap → verify flow (Scan QR / compare safety number, §5.22). A11y: “Not verified yet. Verify to get the badge” / “Pas encore vérifié. Vérifiez pour obtenir le badge”. Contrast `--ink-3` vs surface 6.6:1 light, 6.6:1 dark (v0.1.18).
- **List row: nothing.** Chosen over a tiny ghost seal because: (1) at 15 px a dashed outline reads as noise next to the path chips and unread pills; (2) the list then has one meaning — “mint seal = verified” — that users learn faster than a two-state glyph; (3) unverified is a normal state, not something to flag on every row. The invitation lives where the user can act on it (the thread).
- **Hint row** (below nav, above path banner): `--surface` background, 1 px `--line` bottom, brand shield 18 px, text 13 px `--ink`, `btn-primary btn-sm` “Verify”, ✕ dismiss (36 px glyph, 44/48 target). Neutral background on purpose, so it never looks like the mesh path banner beneath it. `role="note"`.
  - Dismiss: hides for this contact for 30 days (ghost seal stays). Reappears after 30 days or on next in-person proximity, max once.
- Vanishes (ghost seal and hint) the moment the contact is verified.

### 5.34 “Earn the badge” moment — `chat-thread--verified` (end frame)

On return from a successful verify (QR match or safety number marked):
1. 0 ms — hint row collapses (height → 0, opacity → 0, 200 ms ease-out).
2. 120 ms — ghost seal dashes close (stroke-dashoffset to solid, 180 ms), then fill grows from centre in `--brand` (scale 0.6→1.0 of the fill, 240 ms, `cubic-bezier(.2,.8,.2,1)`), check turns `--surface`.
3. 540 ms — single 1.0→1.12→1.0 pop (160 ms). Success haptic (Android `CONFIRM`, iOS `.success`).
4. Toast “Alex is verified” / “Alex est vérifié”, 2.5 s, `role="status"` (announced politely).
Total ≈ 700 ms. **Reduced motion** (Android `ANIMATOR_DURATION_SCALE = 0` / remove animations, iOS Reduce Motion): no scale or dash animation — 150 ms cross-fade from ghost to filled seal; haptic and toast kept. The animation plays once per contact; reopening the chat shows the static badge.

### 5.35 Copy (EN / FR)

| Key | EN | FR | HE (draft) |
|---|---|---|---|
| badge.verified.a11y | Verified | Vérifié | מאומת |
| badge.verified.header.a11y | Verified. Show safety number | Vérifié. Afficher le numéro de sécurité | מאומת. הצגת מספר הבטיחות |
| badge.ghost.a11y | Not verified yet. Verify to get the badge | Pas encore vérifié. Vérifiez pour obtenir le badge | עדיין לא מאומת. אמתו כדי לקבל את התג |
| hint.verify | Verify {name} to get the verified badge | Vérifiez {name} pour obtenir le badge vérifié | אמתו את {name} כדי לקבל את תג האימות |
| hint.verify.cta | Verify | Vérifier | אימות |
| hint.dismiss.a11y | Dismiss | Masquer | סגירה |
| toast.verified | {name} is verified | {name} est vérifié·e | {name} אומת/ה |
| sheet.title | Verified contact | Contact vérifié | איש קשר מאומת |
| sheet.body | You and {name} verified this safety number in person on {date} ({method}). | Vous et {name} avez vérifié ce numéro de sécurité en personne le {date} ({method}). | את/ה ו{name} אימתתם את מספר הבטיחות הזה פנים אל פנים ב-{date} ({method}). |
| sheet.rescan | Scan {name}’s code again | Scanner à nouveau le code de {name} | סריקה חוזרת של הקוד של {name} |
| sheet.remove | Remove verification | Retirer la vérification | הסרת האימות |

Platform: Android renders the seal as an `ImageVector` tinted with `MaterialTheme` brand role; iOS as an SF-Symbol-style custom symbol (`hopla.seal.check` / `.dashed`) with `.symbolRenderingMode(.palette)`. Both honour light/dark via tokens.


## 5D. v0.1.4 — Notifications (REQUIREMENTS § Notifications v0.1.2)

Mockups: `notif-lockscreen`, `settings-notifications`, `chat-info-notifications`. Generator `/workspace/omcp-v014/gen.py`.

### 5.36 Notification modes — `notif-lockscreen` (+`--ios`, `--private`, `--private-ios`, `--hidden`, `--hidden-ios`, `--fr`, `--private-fr-ios`, `--burn`, `--shade-private`, `--shade-private-dark`)

| Mode | Title | Body | Avatar | Lock screen |
|---|---|---|---|---|
| **Full** (default, N1) | Sender name (group: “Group · Sender”) | Text preview | Sender avatar | Same as unlocked |
| **Private** (N2) | `Hopla` | `New message` | None (app icon only) | Same — nothing to hide |
| **Hide on lock screen** | Full or Private when unlocked | — | — | Android `VISIBILITY_PRIVATE` with public version “Hopla · New message” (count only); (never SECRET — v0.1.5). iOS `hiddenPreviewsBodyPlaceholder` = “New message”, with `showsPreview`-style category |

- Private applies everywhere the OS stores a notification: banner, lock screen, shade, Android notification history, iOS Notification Centre, Wear/CarPlay mirroring. The app never posts the name or text, so the OS cannot log it (N2).
- Calls in Private: “Hopla · Incoming call” (Decision 1, §5.56). Mentions and livestream starts in Private: `Hopla` / `New message`.
- Burn-after-read content never appears in a preview, even in Full: body = “🔥 Burn-after-read message” (§5.42).
- Delivery: mesh/local raise notifications on-device. Internet wake-ups are content-free FCM/APNs pushes (opaque mailbox id only); decrypt in the iOS Notification Service Extension / Android FCM handler (N3). Settings explains this in one callout.

### 5.37 Settings › Notifications — `settings-notifications` (+`--dark`, `--private`, `--ios`, `--ios-dark`, `--fr`)
Groups: **Messages** (Message notifications · Private notifications · Hide on lock screen · live Preview card) → **Also notify** (Calls · Mentions · Livestream starts) → Quiet hours (start–end) · Communities default level (All / Mentions / None) → content-free push callout. The Preview card re-renders immediately when Private changes, so the user sees exactly what will show.

### 5.38 Chat info › Notifications — `chat-info-notifications` (+`--private`, `--dark`, `--ios`, `--mute`, `--fr`)
Mute (Off / 1 hour / 8 hours / 1 week / Forever, bottom sheet) · Private notifications (per chat; caption “Follows your default (Off)” or “Overrides your default (Off)”) · Hide on lock screen (per chat) · Preview card. Per-chat setting wins in both directions (v0.1.5, §5.56).

### 5.39 Copy (EN / FR)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| notif.private.title / body | Hopla / New message | Hopla / Nouveau message (rendered “Hopla : Nouveau message”) | Hopla / הודעה חדשה |
| notif.private.call | Incoming call | Appel entrant | שיחה נכנסת |
| settings.private | Private notifications | Notifications privées | התראות פרטיות |
| settings.private.desc | Show only “Hopla: New message”. No name, preview or photo. | Affiche seulement « Hopla : Nouveau message ». Ni nom, ni aperçu, ni photo. | להציג רק ״Hopla: הודעה חדשה״. בלי שם, תצוגה מקדימה או תמונה. |
| settings.hideLock | Hide on lock screen | Masquer sur l’écran verrouillé | הסתרה במסך הנעילה |
| settings.hideLock.desc | Nothing shows until your phone is unlocked. | Rien ne s’affiche tant que le téléphone est verrouillé. | שום דבר לא מוצג עד שהטלפון לא נעול. |
| settings.quiet | Quiet hours | Heures calmes | שעות שקטות |
| settings.push | Internet wake-ups go through Google or Apple with no content. Your phone decrypts the message itself, so they never see who sent it or what it says. | Les notifications Internet passent par Google ou Apple sans aucun contenu : votre téléphone déchiffre le message lui-même. Ils ne voient jamais l’expéditeur ni le texte. | התעוררויות דרך האינטרנט עוברות דרך Google או Apple בלי תוכן. הטלפון שלך מפענח את ההודעה בעצמו, כך שהם אף פעם לא רואים מי שלח אותה או מה כתוב בה. |
| chat.private.follow / override | Follows your default (Off). / Overrides your default (Off). | Suit votre réglage général (désactivé). / Remplace votre réglage général (désactivé). | לפי ברירת המחדל שלך (כבוי). / עוקף את ברירת המחדל שלך (כבוי). |
| chat.mute.* | Mute Alex · Messages still arrive, without sound or banners. · 1 hour / 8 hours / 1 week / Forever | Mettre Alex en sourdine · Les messages arrivent, sans son ni bannière. · 1 heure / 8 heures / 1 semaine / Toujours | השתקת נועה · ההודעות עדיין מגיעות, בלי צליל ובלי באנרים. · שעה / 8 שעות / שבוע / לתמיד |

### 5.40 Accessibility & platform
- Toggles are `role=switch` with the visible title as label; descriptions linked as hint (`contentDescription` / `accessibilityHint`). Preview card announced as “Preview: Hopla, New message”.
- In Private mode the screen-reader announcement of a notification is also only “Hopla, New message” (the spoken text equals the posted text — no hidden `tickerText`/`accessibilityLabel` with the name).
- Contrast: lock-screen cards are system-drawn; mockups approximate. Toggles meet 3:1 non-text contrast.
- Android: one channel per type (Messages, Calls, Mentions, Live); Private sets no `largeIcon`, no `MessagingStyle` person, no `shortcutId` (otherwise conversation bubbles leak the name). Hide on lock = `setVisibility(PRIVATE)` + `setPublicVersion(“Hopla · New message”)`; Never SECRET (Decision 3).
- iOS: NSE builds content; Private omits `INSendMessageIntent` (communication notification avatar) and uses `threadIdentifier` without names. Hide on lock = category `hiddenPreviewsBodyPlaceholder` “New message” + `.hiddenPreviewsShowTitle` off.

---

## 5E. v0.3 — Hopla Nearby: dating (REQUIREMENTS § Hopla Nearby, opt-in) — **all v0.3**

Mockups: `nearby-tab`, `nearby-onboarding`, `nearby-profile`, `nearby-discover`, `nearby-match`, `nearby-chat`, `nearby-safety`. Every frame carries a violet **v0.3** tag. Tone: warm (amber→violet wash built from `--warn-soft` / `--path-mixed-soft`), Hopla teal for primary actions, rounded cards, no flames, no hearts raining. Avatars and photos are **illustrated abstract placeholders** only.

Tab bar in v0.3 becomes 5 tabs: Chats · Communities · **Nearby** · Live · You (icon = ripple).

### 5.41 Nearby entry / off — `nearby-tab` (+`--dark`, `--ios`, `--fr`)
Off on a fresh install (D1): ripple illustration, 3 promises (18+ only · separate profile · never exact location), **Turn on Nearby**, footnote “Off. Nothing is sent until you turn it on.” No packets until onboarding completes.

### 5.42 18+ onboarding — `nearby-onboarding` (+`--under-18`, `--rules`, `--privacy`, `--dark`, `--ios`, `--fr`)
4 steps: (1) date of birth → only age range is shared, DOB stays on device; under 18 → blocking callout, **Back to chats**, Nearby stays off on device (store age-estimation check slots in here if required); (2) community rules, “I agree”; (3) how privacy works (daily key, rough distance, secret likes, hidden identity); (4) profile editor.

### 5.43 Profile editor — `nearby-profile` (+`--dark`, `--ios`, `--fr`)
Up to 6 photos (first = Main; re-encode + EXIF/GPS strip notice), first name/nickname, age shown as range, bio, interests (tags), Looking for (Dating / Friends / Both segmented), Show me (gender, age range), Visibility (Pause · Only shared interests · Hide from my contacts — hashed, local).

### 5.44 Discover — `nearby-discover` (+`--swiping`, `--reduced-motion`, `--dark`, `--ios`, `--empty`, `--limit`, `--fr`)
- Card: photo, distance **bucket only** chip (“< 1 km · mesh”, “< 5 km”, “in your area”) — never metres, never a map (D2). Name + age range, intent pill, bio, interests with shared ones highlighted, flag button (block/report, one tap).
- **Buttons always visible**: Pass · Profile · Like (64 px / 48 px, labelled). Swipe is an optional shortcut. Likes-left pill (100/day); limit state; empty state.
- Motion: swipe tilts ≤ 7° with PASS/LIKE stamp. `prefers-reduced-motion` / Remove animations: no tilt, no fling — 150 ms cross-fade.
- A like is sealed; UI never says “X liked you” before a mutual like (D3).

### 5.45 Match — `nearby-match` (+`--dark`, `--ios`, `--reduced-motion`, `--fr`)
“It’s mutual” · “You and Robin matched” · explains new E2EE chat and hidden identities · **Say hello** / Keep browsing. Confetti dots fade in once (600 ms); reduced motion = static. Match notification follows §5.36 (Private → “Hopla / New message”).

### 5.46 Match chat + Reveal identity — `nearby-chat` (+`--reveal-confirm`, `--reveal-pending`, `--reveal-incoming`, `--revealed`, `--photo-blurred`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
Header: placeholder avatar, “Nearby match · identity hidden”, flag (block/report). Violet banner with **Reveal identity**. Confirm sheet lists what is shared (Hopla name, profile photo, contact key; not number, not location). States: pending (“Nothing is shared until Robin taps too”), incoming (“Reveal too”), revealed (system row + contact card, not verified yet). First-contact photos blurred, tap to view. No messages possible before a match.

### 5.47 Safety — `nearby-safety` (+`--block`, `--report`, `--report-confirm`, `--done`, `--tips`, `--share-date`, `--dark`, `--tips-dark`, `--ios`, `--fr`)
- Block/report one tap from profile card (flag), chat header (flag), match list (swipe actions + screen-reader custom actions).
- Block sheet: hidden across key rotations, they aren’t told; optional “Also report”. Undo toast.
- Report: step 1 reason; step 2 review bundle (profile card required, messages opt-in), moderator shown and changeable, **Send report** — nothing sent before (D5).
- Safety tips shown before first meet-up; **Share my date** sends time + typed place to one trusted contact as an E2EE message.

### 5.48 Copy (EN / FR) — key strings
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| tab | Nearby | À proximité | בסביבה |
| entry.cta / foot | Turn on Nearby / Off. Nothing is sent until you turn it on. | Activer À proximité / Désactivé. Rien n’est envoyé tant que vous ne l’activez pas. | הפעלת ״בסביבה״ / כבוי. שום דבר לא נשלח עד שתפעילו. |
| age.title / under | How old are you? / Nearby is for adults 18+ | Quel âge avez-vous ? / Réservé aux 18 ans et plus | בן/בת כמה את/ה? / ״בסביבה״ מיועד למבוגרים מגיל 18 |
| dist | < 1 km · < 5 km · in your area | < 1 km · < 5 km · dans votre secteur | פחות מ-1 ק״מ · פחות מ-5 ק״מ · באזור שלך |
| btn | Pass · Profile · Like | Passer · Profil · Like | לדלג · פרופיל · לייק |
| limit | That’s all for today. You’ve used your 100 likes. | C’est tout pour aujourd’hui. Vous avez utilisé vos 100 likes. | זהו להיום. השתמשת ב-100 הלייקים שלך. |
| match | You and Robin matched / Say hello | Robin et vous, c’est un match / Dire bonjour | את/ה ורוני התאמתם / להגיד שלום |
| reveal | Reveal identity · Reveal your identity to Robin? · Offer to reveal · Waiting for Robin… · Reveal too | Révéler mon identité · Révéler votre identité à Robin ? · Proposer de révéler · En attente de Robin… · Révéler aussi | חשיפת זהות · לחשוף את הזהות שלך לרוני? · להציע חשיפה · ממתינים לרוני… · לחשוף גם |
| block | Block Robin? Robin won’t see you in Nearby again, even after their keys change. Robin isn’t told. | Bloquer Robin ? Robin ne vous verra plus dans À proximité, même après le changement de clés. Robin n’est pas prévenu. | לחסום את רוני? רוני לא יראה אותך שוב ב״בסביבה״, גם אחרי שהמפתחות שלו ישתנו. לרוני לא נמסר על כך. |
| report.confirm | Check before sending · Nothing is sent until you tap Send report. | Vérifiez avant d’envoyer · Rien n’est envoyé avant que vous touchiez Envoyer. | בדיקה לפני שליחה · שום דבר לא נשלח עד שתקישו על שליחת דיווח. |
| tips / share | Before your first meet-up / Share my date | Avant votre premier rendez-vous / Partager mon rendez-vous | לפני המפגש הראשון / שיתוף הדייט שלי |

### 5.49 Accessibility & platform
- Every swipe has a button; TalkBack/VoiceOver custom actions on the card: Like, Pass, View profile, Block or report. Card announced “Robin, 25 to 29, less than 1 km, Dating, 2 shared interests”.
- Placeholder art has `aria-label`; real photos need user-written alt text option (optional field, v0.3.1).
- Reduced motion respected for swipe, match confetti, card entrance. Targets ≥ 44/48 px.
- Android: Nearby BLE advertising uses its own foreground service notification (“Nearby is on”) only while enabled. iOS: background BLE limits mean discovery mostly in foreground; say so in onboarding step 3 if confirmed (§12 Q7).
- Screenshots of Nearby profiles are not blocked (no claim made).

---

## 5F. v0.1.4 — Burn after read (REQUIREMENTS § Burn after read v0.1.3)

Mockups: `burn-composer`, `burn-bubbles`, `burn-viewer`, `burn-chat-info`. Accent: `--warn` (amber) + 🔥, never red/danger.

### 5.50 Composer — `burn-composer` (+`--armed`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
Long-press **Send** → menu: 🔥 Burn after read toggle (next message or image), Delete unread after 1 h / 24 h / **7 d** (default), Schedule. Armed: amber outlined field, amber 🔥 send button, status line “Burn after read on · deletes unread after 7 d · Turn off”. Disarms after one send (per-message).

### 5.51 Bubbles — `burn-bubbles` (+`--sender`, `--open`, `--placeholders`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
- Receiver: dashed amber bubble, blurred text, **Tap to read**, unread expiry. Photo: blurred, “Photo · view once”.
- Sender: same shape, “Not opened yet”; honest one-line note under the first burn message per chat.
- Opened: text visible, countdown ring + m:ss; at 0 the bubble collapses (no animation under reduced motion) to “🔥 Message read and deleted” on both sides.
- Placeholders: “🔥 Photo opened”, “🔥 Message read and deleted”, “Expired unread”. Save / forward / quote / copy disabled on burn bubbles (long-press menu shows them disabled).
- Screenshots: **superseded v0.1.5** — no notices; screenshots are blocked app-wide (§5.57).

### 5.52 View-once viewer — `burn-viewer` (+`--ios`, `--ios-recording`, `--fr`)
Full-screen black, close, sender, “🔥 View once”, ring + seconds. Footer (v0.1.5): “Save, forward and quote are off. Screenshots are blocked, but another device could still photograph the screen.” iOS while captured: “Hidden while the screen is recorded or mirrored”. Closing = gone; second tap shows placeholder (B2).

### 5.53 Chat info timer — `burn-chat-info` (+`--dark`, `--ios`, `--fr`)
Timer after opening: Off · On open (deleted when closed) · 10 s · 1 min · 1 h. Delete unread after: 1 h / 24 h / 7 d. Callout “What it can’t do”. Footnote: never in previews, backups, search, share sheets. Groups: per-member burn; hidden in channels > 256 members (row replaced by “Not available in large channels — use stories”).

### 5.54 Copy (EN / FR)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| burn.name | Burn after read | Lecture unique | מחיקה אחרי קריאה |
| menu.desc | Next message deletes itself once read | Le prochain message s’efface après lecture | ההודעה הבאה תימחק מעצמה אחרי שתיקרא |
| unread | Delete unread after · 1 h · 24 h · 7 d | S’efface sans lecture après · 1 h · 24 h · 7 j | מחיקה אם לא נקראה אחרי · שעה · 24 שע׳ · 7 ימים |
| bubble.tap | Tap to read | Toucher pour lire | הקישו כדי לקרוא |
| bubble.sent | Not opened yet | Pas encore ouvert | עדיין לא נפתחה |
| photo | Photo · view once / 🔥 Photo opened | Photo · voir une fois / 🔥 Photo ouverte | תמונה · צפייה חד־פעמית / 🔥 התמונה נפתחה |
| burned / expired | 🔥 Message read and deleted / Expired unread | 🔥 Message lu et effacé / Expiré sans être lu | 🔥 ההודעה נקראה ונמחקה / פג תוקף בלי קריאה |
| honest | Burn after read stops saving in the app. It can’t stop someone photographing the screen with another device. | Lecture unique empêche la sauvegarde dans l’app, pas une photo de l’écran prise avec un autre appareil. | מחיקה אחרי קריאה מונעת שמירה באפליקציה. היא לא יכולה למנוע ממישהו לצלם את המסך במכשיר אחר. |
| notif | 🔥 Burn-after-read message | 🔥 Message éphémère | 🔥 הודעה שנמחקת אחרי קריאה |

### 5.55 Accessibility & platform
- Countdown ring is `role=timer`; screen readers announce remaining time on open, every 5 s (≤ 10 s timers: every second for the last 3), and “Message deleted” at 0 (polite live region). Longer timers (1 min / 1 h) announce on open and at 10 s.
- Sealed bubble label: “Burn after read message from Alex. Double-tap to read. Timer starts when opened.”
- Reduced motion: no ring sweep animation (number updates only), no dissolve on delete.
- Ring + number never colour-only; amber has text label 🔥.
- Android: `FLAG_SECURE` app-wide (v0.1.5); content excluded from Auto Backup, search, share targets. iOS: secure-capture layer + `UIScreen.isCaptured` cover (no screenshot notification); content in in-memory store; excluded from iCloud backup and Spotlight.

## 5G. v0.1.5 — the maintainer’s decisions (REQUIREMENTS § Maintainer decisions, 2026-10-24)

Generator `/workspace/omcp-v015/gen.py` (supersedes v0.1.4 generator; same helpers).

### 5.56 Notification changes (Decisions 1–3)
- **Private calls:** title `Hopla`, body `Incoming call` → rendered “Hopla · Incoming call” / « Hopla · Appel entrant ». No caller name, no avatar. Everything else: “Hopla · New message”. `notif-lockscreen--private`, `--private-ios`, `--private-fr-ios`, `--shade-private`.
- **Per-chat opt-out:** a chat may turn Private **off** while the global toggle is on. Caption: “Off for this chat. Your default is On.” / « Désactivé pour cette discussion. Votre réglage général : activé. » Preview card then shows sender + text. `chat-info-notifications--opt-out` (+`-dark`, `-ios`, `-fr`). Replaces the v0.1.4 “privacy only tightens” rule.
- **Android Hide on lock screen:** always `VISIBILITY_PRIVATE` + public version “Hopla · New message” (never blank, no SECRET). `notif-lockscreen--hidden`.

### 5.57 No screenshots anywhere (Decision 5) — `settings-privacy`, `app-switcher-cover`
- Android: `FLAG_SECURE` on every window → screenshots, recordings, casting and recents thumbnail are black. Recents keeps the system app label/icon above a black card (`app-switcher-cover--android`).
- iOS: sensitive views inside the secure-capture layer (blank in captures). App switcher: blurred brand cover — Hopla icon on teal radial gradient (`app-switcher-cover`). While `UIScreen.isCaptured`: full-screen cover “Hopla is hidden while your screen is recorded or mirrored · Stop recording to continue.” (`--ios-recording`, `-fr`).
- **All screenshot notices removed** (📸 row and Android info row; `burn-bubbles--screenshot-*` deleted). Sender is never told; there is nothing to tell.
- Settings › Privacy › Screen: **Screenshot protection · Always on** — “Screenshots and screen recordings of Hopla come out black or blank, and the app switcher shows a cover. Another device can still take a photo of your screen.” / « Les captures et enregistrements d’écran de Hopla sont noirs ou vides, et l’aperçu des apps récentes est masqué. Un autre appareil peut quand même photographier votre écran. » Footnote: cannot be turned off. *(v0.1.6: superseded — user toggle, §5.70.)*
- Banned copy: “screenshot-proof”, “impossible to capture”, “100 % private”.

### 5.58 Nearby premium — session, area mode, radar, instant start (Decision 4) — v0.3
**Go Nearby session** — `nearby-session` (+`--live`, `--ios-live-activity`, `--ios-island`, `--ios-island-expanded`, `--android-ongoing`, `--android-ongoing-dark`, `--dark`, `--ios`, `--fr`, `--fr-live-activity`)
- Sheet (rev. v0.1.8): radar glyph, “Go Nearby”, “You stay visible until you turn Nearby off”, auto-pause rules, **Go Nearby**. See §5.75.
- iOS Live Activity / Dynamic Island: see §5.75 (active and paused states; counts only, never names or photos).
- Android: foreground service (`connectedDevice`) ongoing notification — see §5.75 (actions Open · Turn off).
- In-app **status chip** (header, right): green dot “Live nearby”, grey “Paused”, or pin “Area mode”. Tap → Nearby sheet. No OS-limitation warnings anywhere.

**Area mode** — `nearby-area` (+`--sheet`, `--dark`, `--ios`, `--fr`): outside a session, gateway + coarse geohash cell; sheet explains in one sentence, Area mode toggle, radius (< 1 km / < 5 km / Area), Go Nearby. Matches/likes arrive as normal notifications (NX3 ≤ 5 min).

**Live radar empty** — `nearby-radar` (+`--dark`, `--reduced-motion`, `--live`, `--ios`, `--fr`): “Nobody nearby yet · We’re quietly looking around you.” Concentric rings, soft sweep (4 s linear loop, 14 % violet), centre pulse (2.4 s ease-in-out, scale 1→1.15, opacity .10→0). No fake blips. CTAs: Go Nearby · 60 min + Widen radius (during a session only Widen radius). Reduced motion: no sweep, no pulse, static rings; status text changes are announced politely.

**Instant start** — `nearby-skeleton` (+`--blurhash`, `--resolved`, `--dark`, `--blurhash-dark`)
| t (from tab open) | State | Rule |
|---|---|---|
| 0 ms | Cached card painted from local store (skeleton only if cache empty) | No spinner, ever |
| ≤ 120 ms | Skeleton shimmer starts *only* if still empty (1.2 s linear loop; off under reduced motion) | Avoid flash: skeleton shown ≥ 200 ms or not at all |
| ≤ 300 ms | Card text + blurhash photo | Buttons enabled immediately |
| ≤ 900 ms | Sharp photo cross-fades over blurhash (240 ms ease-out) | NX1: first card < 1 s |
| > 1 s, no cache | Radar empty state | — |

### 5.59 Cards, match and motion spec
Card (refined): 28 px radius, photo 372 px with page bars, frosted flag button, bottom scrim with name 26/800, age range, frosted chips (distance bucket, intent); body: bio + interests, shared ones marked ✦. `nearby-discover` (+ states), `nearby-match` (glow, overlapping avatars, icebreaker chips → prefill composer).

| Motion | Duration | Easing | Reduced motion |
|---|---|---|---|
| Drag follow | 1:1 with finger, 60/120 fps | — | Buttons only animate |
| Card tilt | max 7° at 40 % width | linear to drag | none |
| Like / Pass fling | 280 ms | cubic-bezier(.2,.8,.2,1) | 150 ms cross-fade |
| Snap back (cancel) | 360 ms | spring (damping .8, response .35) | 150 ms fade |
| Next card rise (scale .96→1, y 8→0) | 220 ms | cubic-bezier(.2,.8,.2,1) | instant |
| Button press | 90 ms scale .92 | ease-out | colour only |
| Match: avatars slide in from sides | 420 ms | spring (damping .7, response .45), 60 ms stagger | 200 ms fade |
| Match: heart pop (0→1.15→1) | 320 ms | cubic-bezier(.34,1.56,.64,1) | static |
| Match: glow breathe | 2.4 s loop ×2 then stop | ease-in-out | none |
| Sheet in / out | 300 / 220 ms | cubic-bezier(.2,.8,.2,1) / ease-in | fade 150 ms |
| Haptics | light on Like/Pass commit, success on match, none on drag | iOS UIImpactFeedback / Android `CONFIRM` | unchanged (system setting) |

### 5.60 Copy (EN / FR) — v0.1.5 keys
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| chip.live / paused | Live nearby / Paused | Visible à proximité / En pause | פעיל בסביבה / מושהה |
| chip.area | Area mode | Mode secteur | מצב אזור |
| go.title / cta | Go Nearby / Go Nearby | Go Nearby / Go Nearby | יוצאים ל״בסביבה״ / יוצאים ל״בסביבה״ |
| go.body | Discover people around you live, even with your phone locked. | Découvrez les gens autour de vous en direct, même téléphone verrouillé. | לגלות אנשים סביבך בזמן אמת, גם כשהטלפון נעול. |
| la.counts | people nearby · new likes | personnes autour · nouveaux likes | אנשים בסביבה · לייקים חדשים |
| radar | Nobody nearby yet · We’re quietly looking around you. New profiles will appear here. · Widen radius | Personne pour l’instant · On cherche en douceur autour de vous. Les nouveaux profils apparaîtront ici. · Élargir le rayon | עדיין אין אף אחד בסביבה · אנחנו מחפשים בשקט סביבך. פרופילים חדשים יופיעו כאן. · הרחבת הרדיוס |
| area.title | Area mode keeps going | Le mode secteur reste actif | מצב אזור ממשיך לפעול |
| ice | Break the ice | Pour briser la glace | לשבור את הקרח |
| privacy.ss | Screenshot protection · Always on | Protection contre les captures · Toujours active | הגנה מצילומי מסך · תמיד מופעלת |

A11y: chip is `role=status`; Live Activity/Island read “Live nearby, 5 people nearby, 2 new likes”; skeleton is `aria-busy` with “Loading nearby profiles”; radar has a text alternative.

## 5H. v0.1.6 — Moderation & reputation (PROTOCOL §23, **v0.2**) and Block-screenshots toggle

Generator `/workspace/omcp-v016/gen.py`. All moderation frames carry a blue **v0.2** tag. Tone: calm, explanatory, never shaming. Every banner says *why* and *what happens next*. Colours: review = `--path-mixed`, Reported = `--warn` (amber), Restricted = neutral `--ink-2`; never `--danger` for a community state.

### 5.61 Report sheet — `mod-report` (+`--sent`, `--limit`, `--dark`, `--ios`, `--fr`)
Reasons: Illegal content · Harassment or hate · Scam or spam · Unwanted sexual content · Violence · Something else. Anonymity line: sent under the per-community pseudonym (§23.2; MR6). Sent: “Your report adds to others… several independent people need to report the same thing.” Limit: 10/day explained as protection against coordinated campaigns. Reporter weight is never shown.

### 5.62 Vouch + reputation indicator — `mod-community` (+`--vouch`, `--vouched`, `--reputation`, `--dark`, `--ios`, `--fr`)
- Vouch (♥) in the community header; sheet: signed by community pseudonym, once per 30 days, weight depends on network trust (EigenTrust). Vouched state = secondary button, `aria-pressed=true`.
- Reputation indicator: 4-bar pill with a word (New community / Building trust / Well supported / Strongly supported), mapping R bands 0–.25/.25–.5/.5–.75/.75–1. Never a number, never member names. Explainer sheet lists the 5 inputs (qualitative bars), 14-day smoothing, “higher reputation → more reports needed”, and “never protects against known-illegal-content matching”.

### 5.63 State banners — `mod-states` (+`--reported`, `--reported-why`, `--restricted`, `--reported-dark`, `--restricted-dark`, `--ios`, `--fr`)
| Stage (§23.4) | Who sees | UI |
|---|---|---|
| Under review (≥ 0.5·T) | **Admins only** | Violet banner “Some reports came in. Nothing changes for members.” + See details |
| Reported (≥ T) | Everyone | Amber banner “Photos and videos are paused for now. Text is still readable. The owner can appeal.” Media → “Media paused” placeholder. Insiders get counter-signal “It’s fine” (weighted, capped 50 %). Why? sheet: what changes / what happens next (14-day decay, appeal). |
| Restricted (≥ 3·T + upheld by followed labeler) | Everyone | Neutral banner naming the labeler confirmation; **text only** (media not rendered: “1 photo not shown”); joins closed; no Vouch button. |
| CSAM hash match (§23.1.5) | — | Media blocked + reported regardless of R; per-item placeholder “This media can’t be shown” (no mockup, no appeal path in UI). |

### 5.64 Join warning — `mod-join-warning` (+`--restricted`, `--dark`, `--ios`, `--fr`)
Reported: “Before you join” notice, **Join anyway** / Go back. Restricted: “Joining is closed for now” + Manage my moderation lists.

### 5.65 Stream blur — `mod-stream` (+`--continue`, `--reduced-motion`, `--ios`, `--fr`)
Threshold 0.3·T for host R. New viewers: blurred video, “Blurred for new viewers · Viewers reported it during the stream” + Why? / Leave. Current viewers: “Continue watching?” sheet (Continue watching / Leave stream / Report it too). Blur ramps 0→24 px over 400 ms ease-out; **reduced motion: instant**. Chat stays visible.

### 5.66 Appeal (owner) — `mod-appeal` (+`--pending`, `--cleared`, `--dark`, `--ios`, `--fr`)
Explains jury (15 random verified, independent, never reported/vouched), 72 h, ⅔, once per 30 days, signed aggregate votes. Pending: timeline + “9 / 15 jurors have voted” (direction hidden). Cleared: “12 of 15 votes”, Reported cleared, track record improves, Check the votes. (Upheld result: same layout, neutral icon, “The jury upheld the reports”, next appeal date.)

### 5.67 Jury vote — `mod-jury` (+`--voted`, `--dark`, `--ios`, `--fr`)
“You’re on a jury · Is this community fine?” Context card (main reason, owner statement, reported posts with media blurred), time left, Not fine / Fine (equal weight, Fine primary only for layout consistency), “I can’t judge this (step aside)”.

### 5.68 Copy (EN / FR) — key strings
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| vouch | Vouch / Vouched | Recommander / Recommandée | לערוב / ערבת |
| rep.levels | New community · Building trust · Well supported · Strongly supported | Nouvelle · En construction · Bien soutenue · Très soutenue | קהילה חדשה · בונה אמון · נתמכת היטב · נתמכת מאוד |
| review | Under review | En cours d’examen | בבדיקה |
| reported | Reported by participants · Photos and videos are paused for now. Text is still readable. The owner can appeal. | Signalée par des participants · Images et vidéos sont en pause pour l’instant. Le texte reste lisible. Le propriétaire peut faire appel. | דווח על ידי משתתפים · תמונות וסרטונים מושהים כרגע. הטקסט עדיין קריא. הבעלים יכולים לערער. |
| restricted | Restricted · Only text is shown; new joins are closed. | Restreinte · Seul le texte s’affiche ; les nouvelles adhésions sont fermées. | מוגבלת · מוצג רק טקסט; הצטרפויות חדשות סגורות. |
| counter | It’s fine | Elle est correcte | זה בסדר |
| join | Before you join · Join anyway | Avant de rejoindre · Rejoindre quand même | לפני שמצטרפים · להצטרף בכל זאת |
| stream | Blurred for new viewers · Continue watching? | Live flouté pour les nouveaux spectateurs · Continuer à regarder ? | מטושטש לצופים חדשים · להמשיך לצפות? |
| appeal | Appeal this decision · Send appeal · The jury found your community fine | Faire appel · Envoyer l’appel · Le jury a jugé votre communauté correcte | ערעור על ההחלטה · שליחת ערעור · חבר המושבעים מצא שהקהילה שלך תקינה |
| jury | Is this community fine? · Fine · Not fine · I can’t judge this (step aside) | Cette communauté est-elle correcte ? · Correcte · Pas correcte · Je ne peux pas juger (se retirer) | האם הקהילה הזו תקינה? · תקינה · לא תקינה · אני לא יכול/ה לשפוט (לפנות את המקום) |

### 5.69 Accessibility & platform
Banners `role=status` (announced once on entry, not on every scroll). Reputation pill `role=img` “Reputation: Well supported, 3 of 4”. Blurred media has text alternative “Media paused”. “Continue watching?” is `alertdialog` with focus on Continue. Jury buttons equal size, no colour-only meaning. Android: banners as Material `Card`; iOS: inset grouped section header. Reduced motion: blur and banner slide-ins become instant/150 ms fades.

### 5.70 Settings › Privacy › Block screenshots (the maintainer, v0.1.6) — `settings-privacy` (+`--dark`, `--ios`, `--confirm-off`, `--confirm-off-dark`, `--confirm-off-ios`, `--off`, `--off-dark`, `--off-ios`, `--fr`, `--confirm-off-fr`, `--off-fr`)
- Toggle **Block screenshots**, default **On** (= §5.57 behaviour). On copy unchanged (honest second-camera line).
- Turning off → confirm sheet “Allow screenshots?”: app switcher will show chats; captures may land in photo backups; contacts aren’t notified; burn-after-read and view-once stay protected. Allow screenshots / Keep blocking (default focus).
- Off: Android clears `FLAG_SECURE` on normal windows; iOS removes the secure-capture layer and app-switcher cover. **Burn viewer, opened burn text and view-once media keep `FLAG_SECURE` / secure layer / capture cover.** Off copy: superseded by §5.71 (v0.1.7).
- Supersedes §5.57’s “cannot be turned off” footnote.

| Key | EN | FR | HE (draft) |
|---|---|---|---|
| toggle | Block screenshots | Bloquer les captures d’écran | חסימת צילומי מסך |
| off.desc | *(v0.1.7 → §5.71)* Screenshots are allowed. Burn-after-read and view-once content is always protected. Another device can still take a photo of your screen. | Captures autorisées. Les messages éphémères et les médias à voir une fois restent toujours protégés. Un autre appareil peut quand même photographier votre écran. | *(v0.1.7 → §5.71)* צילומי מסך מותרים. תוכן שנמחק אחרי קריאה ותוכן לצפייה חד־פעמית תמיד מוגנים. מכשיר אחר עדיין יכול לצלם את המסך שלך. |
| confirm.title | Allow screenshots? | Autoriser les captures d’écran ? | לאפשר צילומי מסך? |
| confirm.cta | Allow screenshots / Keep blocking | Autoriser les captures / Garder le blocage | לאפשר צילומי מסך / להמשיך לחסום |

## 5I. v0.1.7 — Approved decisions (Jakob, 2026-10-24)

Generator `/workspace/omcp-v017/gen.py`.

### 5.71 Block screenshots — aligned with REQUIREMENTS Decision 5 / SS1
- Toggle **Block screenshots**, **on by default** (Settings › Privacy). On copy now starts “On by default.”; both On and Off copy keep the second-camera line (Decision 5: the app says plainly that a second camera can still photograph the screen).
- Off copy: “Screenshots are allowed. Burn-after-read and view-once content is always protected. Another device can still take a photo of your screen.” / « Captures autorisées. Les messages éphémères et les médias à voir une fois restent toujours protégés. Un autre appareil peut quand même photographier votre écran. »
- Confirm bullet: “Burn-after-read and view-once content is always protected.” / « Les messages éphémères et le contenu à voir une fois restent toujours protégés. » `settings-privacy` (all 12 frames re-rendered).
- SS1: toggle on → every screen black/blank; toggle off → burn/view-once still black/blank (B3).

### 5.72 App switcher by toggle state — `app-switcher-cover` (+`--android`, `--ios-off`, `--android-off`, `--ios-recording`, `-fr`)
| Toggle | Android recents | iOS app switcher |
|---|---|---|
| **On** (default) | Black card (`FLAG_SECURE`), system app label | Blurred brand privacy cover |
| **Off** | Normal preview (system snapshot) | Normal preview |
Open burn/view-once screens keep `FLAG_SECURE` / secure layer, so their snapshot stays black/blank even when Off.

### 5.73 Nearby indicator — Private notifications — `nearby-session`
With **Private notifications on**, the Live Activity, Dynamic Island and Android ongoing notification **hide the counts** (“Nearby is on · Visible until you turn it off”). Pause reasons are still shown (they reveal nothing about other people). Session-length rules: superseded by §5.75.

### 5.74 Moderation confirmations
Confirmed (unchanged since v0.1.6): Reported and Restricted follow PROTOCOL §23.4 exactly (§5.63); Under review is shown to **admins only**; reputation uses **four word levels**, no number (§5.62).

## 5J. v0.1.8 — Nearby stays on until turned off (REQUIREMENTS Decision 4 + D6) — v0.3

Generator `/workspace/omcp-v018/gen.py`. Mockups: `nearby-session` (rebuilt), `nearby-checkin` (new), `settings-nearby` (new); `nearby-discover`, `nearby-radar`, `nearby-area`, `nearby-skeleton`, `nearby-tab` regenerated with the new chip / CTA.

### 5.75 Visibility model
- One tap on **Go Nearby** makes you visible **until you turn it off**. No duration picker, no extension action, no countdown anywhere.
- **Auto-pause** (quiet, no alert, within 60 s of the condition — D6) and **auto-resume**:
| Reason | Indicator title | Sub-line | Icon |
|---|---|---|---|
| 2 h without motion | Paused: no motion for 2 h | Resumes when you move | standing figure |
| 01:00–07:00 local | Paused: night (01:00–07:00) | Resumes at 07:00 | moon |
| Battery < 20 % | Paused: battery below 20% | Resumes when charging or above 20% | battery |
While paused nothing is broadcast; the indicator stays (greyed), so the user always knows Nearby is on.
- **Check-in** every 4 h of *visible* time (paused time doesn’t count): notification + in-app prompt “Still up for meeting people?” with **Yes** / **Turn off**. No answer within 15 min → Nearby turns off; a quiet notification and an in-app note say so (“You didn’t answer the check-in, so we turned it off. Your matches and chats are kept.”), with **Turn back on**.
- **Persistent indicator** whenever Nearby is on (active or paused) — D6: iOS Live Activity + Dynamic Island; Android foreground-service ongoing notification (non-dismissible), actions Open · Turn off.

### 5.76 Indicator states — `nearby-session--…`
| State | iOS Live Activity | Dynamic Island compact | Android ongoing |
|---|---|---|---|
| Active | ripple + “Live nearby · Visible until you turn it off”, tiles (people nearby, new likes), Turn off | ripple · “5 ♥2” | “Live nearby” / “5 people nearby · 2 new likes” |
| Active + Private | “Nearby is on”, no tiles, “counts hidden” line | ripple only | “Hopla · Nearby is on” / “Visible until you turn it off” |
| Paused (any reason, normal or Private) | reason icon (grey) + reason title + resume line | reason icon · “Paused” | reason title / resume line (Private: prefixed “Hopla ·”) |
Frames: `--ios-live-activity[-paused-{motion,night,battery}][-private]`, `--android-ongoing[-paused-…][-private]`, `--ios-island[-private|-paused]`, `--ios-island-expanded[-paused]`, dark and FR variants; `--live` / `--paused` chip frames.
Platform: iOS Live Activities are ended by the system after 8 h (12 h on the lock screen); the app restarts the activity on each check-in “Yes” and on resume, so the indicator never lapses (D6). Android uses `FOREGROUND_SERVICE_CONNECTED_DEVICE`, `setOngoing(true)`, low-importance channel “Nearby status”.

### 5.77 Check-in — `nearby-checkin` (+`--android`, `--in-app`, `--in-app-dark`, `--in-app-ios`, `--turned-off`, `--turned-off-dark`, `--turned-off-notif`, `--fr`, `--in-app-fr`, `--turned-off-fr`)
Notification with actions Yes / Turn off and line “No answer in 15 min turns Nearby off.” In-app `alertdialog`: Yes, stay visible (primary, default focus) / Turn off Nearby; time it turns off shown as a plain time (not a ticking countdown; `aria-live=off`). Under Private notifications the check-in body is still generic (no names), so it’s shown as is.

### 5.78 Settings › Nearby — `settings-nearby` (+`--dark`, `--ios`, `--fr`)
Visible nearby toggle (“Stays on until you turn it off”), Auto-pause list (3 rules + check-in), indicator explanation, My profile, Area mode, Safety tips. Rules are informational (not user-configurable in v0.3).

### 5.79 Copy (EN / FR)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| go.body | You stay visible until you turn Nearby off. An indicator is always shown while you are. | Vous restez visible jusqu’à ce que vous désactiviez À proximité. Un indicateur reste toujours affiché. | תישארו גלויים עד שתכבו את ״בסביבה״. כל עוד אתם גלויים, מוצג חיווי. |
| active.sub | Visible until you turn it off | Visible jusqu’à ce que vous désactiviez | גלוי עד שתכבו |
| pause.motion | Paused: no motion for 2 h · Resumes when you move | Pause : pas de mouvement depuis 2 h · Reprend dès que vous bougez | מושהה: אין תנועה כבר שעתיים · יתחדש כשתזוזו |
| pause.night | Paused: night (01:00–07:00) · Resumes at 07:00 | Pause : nuit (01:00–07:00) · Reprend à 07:00 | מושהה: לילה (01:00–07:00) · יתחדש ב-07:00 |
| pause.battery | Paused: battery below 20% · Resumes when charging or above 20% | Pause : batterie sous 20 % · Reprend en charge ou au-dessus de 20 % | מושהה: סוללה מתחת ל-20% · יתחדש בטעינה או מעל 20% |
| checkin | Still up for meeting people? · Yes · Turn off · No answer in 15 min turns Nearby off. | Toujours partant·e pour faire des rencontres ? · Oui · Désactiver · Sans réponse sous 15 min, À proximité se désactive. | עדיין רוצים להכיר אנשים? · כן · לכבות · בלי תשובה תוך 15 דקות, ״בסביבה״ ייכבה. |
| off | Nearby is off: no answer to the check-in. · Turn back on | À proximité est désactivé : pas de réponse au point de contrôle. · Réactiver | ״בסביבה״ כבוי: לא הייתה תשובה לבדיקה. · להפעיל שוב |

### 5.80 Accessibility
Indicator is `role=status`; state changes (paused/resumed) announced politely once, never on a timer. Pause reason is in text, icons are decorative. Check-in prompt traps focus, Yes first; actions ≥ 48 px. Reduced motion: radar/ripple static in paused state; no pulsing indicator dot.

## 5K. v0.1.9 — Area-mode indicator; check-in counts visible time (REQUIREMENTS Decision 4, Q12/Q13) — v0.3

Generator `/workspace/omcp-v019/gen.py`.

### 5.81 Persistent indicator in area mode — `nearby-area-indicator`
Area mode (gateway visibility outside Go Nearby) also shows the persistent indicator (D6). Label **Area mode** / **Mode secteur**; glyph = hexagonal cell with centre dot (the coarse geohash cell), tinted `--path-mixed` violet instead of the teal ripple — distinct, calm, no pulse.
| State | iOS Live Activity | Dynamic Island compact | Android ongoing |
|---|---|---|---|
| Active | hex + “Area mode · Visible in your area (~5 km) until you turn it off”, tiles (profiles in your area, new likes), Turn off | hex · “3 ♥1” | “Area mode” / “3 profiles in your area · 1 new like” |
| Active + Private | “Area mode is on”, no tiles | hex only | “Hopla · Area mode is on” |
| Paused | reason icon + “Area mode · Paused: …” + resume line | reason icon · “Area · Paused” | “Area mode · Paused: …” (Private prefixed “Hopla ·”) |
Frames: `nearby-area-indicator` (+`--ios-live-activity[-paused-night][-private]`, `--android-ongoing[-paused-night][-private]`, `--ios-island`, `--ios-island-private`, `--ios-island-expanded`, `--ios-island-paused`, `--android-ongoing-dark`, `--android-ongoing-paused-motion-dark`, `--fr-live-activity`, `--fr-android-ongoing-private`, `--fr-live-activity-paused-night`). Same auto-pause rules and check-in apply. Switching from area mode to Go Nearby morphs glyph hex→ripple (240 ms cross-fade; instant under reduced motion).

### 5.82 Check-in counts visible time only
- The 4 h counter runs only while Nearby (Go Nearby or area mode) is actually visible; any pause stops it. No check-in fires while paused (e.g. overnight).
- Copy: Settings › Nearby “Check-in every 4 h of visibility — Counts visible time only, not pauses. Next one after 2 h 40 min more visible time.”; Go Nearby sheet “After every 4 h of visible time, we check you’re still up for it”; in-app prompt “You’ve been visible for 4 h (pauses not counted).”
- The remaining visible time is shown **only** in Settings › Nearby (static, rounded to 10 min), never on the lock screen or as a ticking countdown.
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| checkin.rule | Check-in every 4 h of visibility | Point de contrôle toutes les 4 h visibles | בדיקה כל 4 שעות של נראות |
| checkin.desc | Counts visible time only, not pauses. Next one after {t} more visible time. | Compte seulement le temps visible (pas les pauses). Prochain dans {t} de visibilité. | נספר רק זמן גלוי, לא השהיות. הבדיקה הבאה אחרי עוד {t} של זמן גלוי. |
| area.title / sub | Area mode · Visible in your area (~5 km) until you turn it off | Mode secteur · Visible dans votre secteur (~5 km) jusqu’à désactivation | מצב אזור · גלוי באזור שלך (כ-5 ק״מ) עד שתכבו |

## 5L. v0.1.10 — Restore from backup (reinstall-first) — extends §5.17, §5.26, §5.30, §5.31

Generator `/workspace/omcp-v0110/gen.py` (render: `/workspace/omcp-v0110/render.py`, system Chrome). Mockups: `restore-detect`, `restore-source`, `restore-password`, `restore-progress`, `restore-success`, `restore-error`. They supersede the visual frames of `restore-reinstall` (§5.31 rules stay normative: lockout, Start fresh, phrase fallback). **Hero screens:** detected backup card, password, progress, Welcome back.

### 5.83 Journey and entry points
1. **Primary — auto-detect on first launch** (`restore-detect--checking` → `restore-detect`). Before onboarding, the app checks the signed-in account: Android = Credential Manager account already on device → Drive `appDataFolder` (`drive.appdata` only); iOS = CloudKit private DB `OMCPBackup` (and Drive if a Google account was previously linked — only via the manual path). Timeout 3 s; if nothing found or offline, show Welcome (no error on first launch).
2. **Fallback — Welcome › Restore** (`restore-source`): iOS lists iCloud (recommended), Google Drive, My 24 words; Android lists Google Drive, My 24 words. *(v0.1.17: the “My 24 words” row reads “Identity and contacts only, no chats” / « Identité et contacts seulement, sans les discussions » — it no longer says “Always works, no cloud needed”.)* Google → system account picker (Android Credential Manager sheet / iOS `ASWebAuthenticationSession` Google sign-in). **No backup found** → Try another account · Use my 24 words · Start fresh.
3. **Settings › Backup › Restore:** not offered (dropped). Restoring over a live identity is “replace identity”, out of scope; Settings keeps §5.30 management only.

### 5.84 Detected backup — `restore-detect` (+`--ios`, `--dark`, `--ios-dark`, `--checking`, `--start-fresh`, `--fr`, `--ios-fr`)
“Welcome back · We found your Hopla backup. Restore your chats, contacts and verified badges. *(v0.1.11 wording)*” Card: service, Found badge, Account, Backed up (`created_at`), Device (`device_hint`), Size, Protection (Password / iCloud Keychain key). CTA **Restore**; secondary **Start fresh** (confirm sheet: new identity, contacts must re-add you, old backup stays until deleted; default focus **Restore instead**); links Use my 24 words · Other account.

### 5.85 Password and lockout — `restore-password` (+`--wrong`, `--lockout`, `--dark`, `--ios`, `--keychain`, `--keychain-missing`, `--keychain-dark`, `--fr`, `--lockout-fr`, `--keychain-fr`)
- Shown only for password-KDF blobs. Argon2id runs on device (expect 0.5–1.5 s; button shows spinner, not progress).
- Wrong: inline `role=alert` “Wrong password. 2 tries left, then a 5-minute wait.” Lockout (§5.31): after 3 fails, 5-min pause, escalating per round (5 → 15 → 60 min); field and Restore disabled; remaining time in minutes, announced once per minute; 24-word path always enabled. Lockout survives app restart (stored locally).
- iCloud no-password: “No password needed · Key found in iCloud Keychain” → Restore. Keychain not synced yet: steps to enable Passwords & Keychain sync, Try again, Use my 24 words.

### 5.86 Progress — `restore-progress` (+`--download`, `--decrypt`, `--dark`, `--ios`, `--reduced-motion`, `--background-android`, `--background-ios`, `--fr`)
Stages: **Download** (2.4 MB from {service}, text only) → **Decrypt** (on your phone) → **Contacts and chats** (v0.1.11: 23 chats, 1,284 messages — text history restored). Determinate bar + stage list (`aria-current=step`). “You can leave the app. We’ll keep going in the background.” Android: `WorkManager` expedited job + ongoing notification “Restoring Hopla · step 3 of 3”. iOS: `BGProcessingTask`/background URLSession; local notification “Hopla is restored. Welcome back!” when done. Reduced motion: no ring pulse, bar jumps per stage.

### 5.87 Welcome back — `restore-success` (+`--dark`, `--ios`, `--ios-dark`, `--fr`)
“Welcome back, Sam · You’re all set. Your contacts will recognise you.” **Restored:** identity, contacts, verified badges (v0.1.14: “Still verified on your side”. Your verifications of others carry over, but contacts must re-verify you, §5.98), profile and settings. ~~Not restored: message history~~ → *v0.1.11: chats and message text are restored; media comes back later (§5.91).* Not restored: burn-after-read / view-once (never backed up), Nearby profile (separate rotating keys). CTA Open my chats.

### 5.88 Errors — `restore-error` (+`--permission`, `--quota`, `--corrupted`, `--too-new`, `--dark`, `--ios`, `--fr`, `--offline-fr`)
| Error | Title | Primary / secondary |
|---|---|---|
| Offline | You’re offline (mesh and gateways are never used for backups) | Try again / Use my 24 words |
| Permission revoked | {service} access was removed | Allow access / Use my 24 words |
| Quota full | {service} storage is full — restore still works, next backup can’t | Restore anyway / Use my 24 words |
| Corrupted | This backup can’t be read — nothing changed on your phone | Use my 24 words / Try another account |
| Version too new | Update Hopla to restore (blob `v` > supported) | Update Hopla / Use my 24 words |
On first-launch auto-detect, errors fall back silently to Welcome; error screens appear only after the user chose Restore.

### 5.89 Copy (EN / FR)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| detect.title / body | Welcome back · We found your Hopla backup. Restore your chats, contacts and verified badges. *(v0.1.11 wording)* | Bon retour · Nous avons trouvé votre sauvegarde Hopla. Restaurez vos discussions, vos contacts et vos vérifications. | ברוכים השבים · מצאנו את הגיבוי שלך ב-Hopla. אפשר לשחזר את הצ׳אטים, אנשי הקשר ותגי האימות. *(נוסח v0.1.11)* |
| checking | Looking for your backup… | Recherche de votre sauvegarde… | מחפשים את הגיבוי שלך… |
| fresh | Start fresh? You’ll get a new identity, so your contacts will need to add you again. Your old backup stays in {service} until you delete it. · Restore instead · Create a new identity | Repartir de zéro ? Vous aurez une nouvelle identité : vos contacts devront vous rajouter. Votre ancienne sauvegarde reste dans {service} jusqu’à ce que vous la supprimiez. · Restaurer plutôt · Créer une nouvelle identité | להתחיל מחדש? תקבלו זהות חדשה, ואנשי הקשר יצטרכו להוסיף אתכם שוב. הגיבוי הישן נשאר ב-{service} עד שתמחקו אותו. · לשחזר במקום · יצירת זהות חדשה |
| nobackup | No backup found · There’s no Hopla backup in {service} for {account}. It may be on another account. | Aucune sauvegarde trouvée · Aucune sauvegarde Hopla dans le {service} de {account}. Elle est peut-être sur un autre compte. | לא נמצא גיבוי · אין גיבוי של Hopla ב-{service} עבור {account}. ייתכן שהוא בחשבון אחר. |
| pw | Enter your backup password · Forgot it? Hopla can’t reset it. Your 24 words still restore your identity and contacts, but not your chats. | Saisissez votre mot de passe · Oublié ? Hopla ne peut pas le réinitialiser. Vos 24 mots restaurent quand même votre identité et vos contacts, mais pas vos discussions.| הזינו את סיסמת הגיבוי · שכחת? ל-Hopla אין אפשרות לאפס אותה. 24 המילים עדיין משחזרות את הזהות ואנשי הקשר, אבל לא את הצ׳אטים. |
| lockout | Security pause: {n} min · Too many tries… Each new round of mistakes makes the wait longer. | Pause de sécurité : {n} min · Trop d’essais… Chaque nouvelle série d’erreurs allonge l’attente. | השהיית אבטחה: {n} דק׳ · יותר מדי ניסיונות… כל סבב טעויות נוסף מאריך את ההמתנה. |
| keychain | No password needed · Key not here yet | Aucun mot de passe nécessaire · Clé introuvable pour l’instant | לא נדרשת סיסמה · המפתח עוד לא כאן |
| progress | Restoring your Hopla · You can leave the app. We’ll keep going in the background. | Restauration en cours · Vous pouvez quitter l’app, on continue en arrière-plan. | משחזרים את ה-Hopla שלך · אפשר לצאת מהאפליקציה. נמשיך ברקע. |
| success | Welcome back, {name} · You’re all set. Your contacts will recognise you. · Open my chats | Bon retour, {name} · Tout est prêt. Vos contacts vous reconnaîtront. · Ouvrir mes discussions | ברוך/ה השב/ה, {name} · הכול מוכן. אנשי הקשר יזהו אותך. · פתיחת הצ׳אטים שלי |

### 5.90 Accessibility & reduced motion
Hero headings are `h1`; backup card is a list of label/value pairs; errors `role=alert`; progress `role=progressbar` with stage text announced on change only. Password field supports password managers (`autofill=password`, iOS `textContentType=.password`) and paste. Reduced motion: no pulse, no cross-fades between stages (instant), success check appears without scale.

## 5M. v0.1.11 — Text chat backup: restored chats and missing-media placeholders

Follows REQUIREMENTS “Encrypted chat backup (v0.1.4, text only)”. Generator `/workspace/omcp-v0111/gen.py`. New mockups `media-missing`, `chats-restored`; `restore-detect`, `restore-progress`, `restore-success` re-rendered with the new copy.

### 5.91 What the backup now holds
All message text (1:1, groups, joined community chats, reactions, edits, timestamps, contact names) is backed up, encrypted on device (HKDF `hopla/chat-backup/v1`, XChaCha20-Poly1305). It's incremental, padded to 64 KiB buckets, Wi-Fi only by default, at most hourly. Images, videos, voice notes and files are **not** backed up. Burn-after-read and view-once messages are never backed up. **Restore copy changes (supersedes §5.84/§5.86/§5.87 wording):** detect body is “Restore your chats, contacts and verified badges”; backup card size is “2.4 MB · text only”; progress stage 3 reads “23 chats, 1,284 messages”; success subtitle is “Your chats are back”, with three groups: **Restored** (identity, chats and messages, contacts, verified badges, profile/settings), **Coming back later** (photos, videos, voice notes, files: “come back when a contact who has them is nearby (mesh or gateway)”), **Not restored** (burn-after-read and view-once, Nearby profile).

### 5.92 Missing-media placeholder — `media-missing` (+`--requested`, `--syncing`, `--restored`, `--reduced-motion`, `--video`, `--gone`, `--group`, `--dark`, `--ios`, `--ios-dark`, `--fr`, `--syncing-fr`, `--gone-fr`)
- **Scope (v0.1.13):** only for media not in the backup (§5.97).
- **Anatomy.** The bubble keeps its side, sender (groups), time and caption. Photo and video use a calm diagonal-hatch tile (brand 7% over `--surface-2`) at the original aspect ratio, a centred type glyph on a surface disc, and a small caps tag “NOT RESTORED YET”. Under it: type name and size (video adds duration). Voice notes and files use a compact 44 px hatched tile with name, size/duration and the tag. No red, no warning icons.
- **States.**
  | State | Status line (EN / FR) | Action |
  |---|---|---|
  | Waiting | Comes back when {name} is nearby / Revient quand {name} est à proximité | “Request again” pill (whole bubble is the target) |
  | Requested | Requested · we’ll fetch it as soon as it’s in reach / Demandé · on le récupère dès qu’il est à portée + toast “Requested” (v0.1.12) | none (tap is a no-op for 30 s) |
  | Syncing | Getting it via mesh · 62% / Récupération via le mesh · 62 % (or gateway / une passerelle) + 4 px bar | none |
  | Restored | “Restored” chip on the media, fades after 2 s | normal media |
  | Partner no longer has it | {name} doesn’t have this anymore. | “Ask to resend” — one tap, optional note (v0.1.16, §5.100) |
- **“Nearby”** means reachable over the BLE mesh or a gateway. Groups: “when a member is nearby”, and any member holding the file can serve it.
- **Auto-fetch.** When a partner holding the file is reachable, the app requests it by stored locator and hash (§7, 64 KiB chunks), verifies the hash, then swaps. Target: within 10 s of a partner coming in range (CB3). A tap only moves the request to the front of the queue.
- **Swap motion.** Placeholder crossfades to the media over 240 ms ease-out, with a 12→0 px blur-up. Bubble height animates over 200 ms if the aspect ratio differs. The “Restored” chip fades out after 2 s. **Reduced motion:** instant swap, no blur, no height animation; the chip is still shown and announced.
- **Burn-after-read / view-once.** These are never in the backup, so they leave no placeholder and no trace in restored history.
- **Chat banner** (top of thread, dismissible per chat): “3 media waiting for Alex · Your text came back. Photos and files return when Alex is nearby (mesh or gateway).” For groups: “12 media waiting in this group”. Hidden once the count is 0.
- **A11y.** The bubble is `role=button` with label “{type}, {size}, not restored yet. {status}. Tap to request again.” Status changes go through `aria-live=polite` (Requested, Getting it, Restored), and progress is announced at most every 25%. The hatch is decorative; contrast of the text on the tile is ≥ 4.5:1 in light and dark.

### 5.93 Chats list after restore — `chats-restored` (+`--dark`, `--ios`, `--ios-dark`, `--fr`)
A dismissible card at the top: “Chats restored · 1,284 messages in 23 chats”, with an inset “12 photos and 4 files waiting for partners · They come back when your contacts are nearby.” Rows with waiting media show “📷 Photo · waiting” as the preview and a small clock pill with the count. The card auto-hides when everything is restored or after 7 days.

### 5.94 v0.1.12 — Ask to resend, privacy, data use (REQUIREMENTS “Encrypted chat backup”, approved 2026-10-24)
Generator `/workspace/omcp-v0112/gen.py`. `media-missing` adds the frames `--confirm`, `--sent`, `--wifi`, `--wifi-dark`, `--confirm-fr`, `--sent-fr`, `--wifi-fr` and `--ios-confirm`. There's a new mockup `settings-missing-media` (+`--on`, `--dark`, `--ios`, `--fr`).
- **Partner no longer has it** (`--gone`): “{name} doesn’t have this anymore.” / « {name} ne l’a plus sur son téléphone. » One-tap pill **Ask to resend** / **Demander un renvoi**. This state replaces the v0.1.11 “Ask {name}” wording.
- *(Superseded v0.1.16, §5.100: one-tap request, no confirm sheet; `--confirm`, `--confirm-fr`, `--ios-confirm` removed.)* **Confirm sheet** (`--confirm`): “Ask Alex to resend? · Alex will get this message in your chat:” shows a quoted media reference (type, date, caption) and an editable message preview, “Hi! Could you send me this photo again? 🙂” / « Salut ! Tu peux me renvoyer cette photo ? 🙂 ». Below it: “Nothing else is shared. You can edit the text before sending.” Actions: **Send request** (primary) and Edit message. Nothing is sent before the user confirms. The message doesn't mention the restore.
- **Sent** (`--sent`): toast “Sent” / « Envoyé ». The placeholder reads “Request sent to {name} · it’ll appear here if they resend it.” The resent file is matched by hash and swaps in like any other restore.
- **Privacy (revised in this bump, superseding the earlier “partner never told” decision):** the restore itself is announced to contacts (§5.95). Media re-requests carry no extra metadata. The toast still says “Requested”, and the Settings footnote saying contacts aren't told was removed.
- **Data use:** over direct BLE mesh, media always re-downloads. Over a gateway it's Wi-Fi only unless **Settings › Backup › Missing media › Use mobile data for missing media** is on (off by default). The placeholder state `--wifi` reads “Waiting for Wi-Fi · available via a gateway” / « En attente du Wi-Fi · disponible via une passerelle », with the link “Use mobile data…” going to that setting. Items available over mesh keep syncing at the same time. Settings shows the waiting count and size, plus a fixed row “Nearby devices (Bluetooth mesh) · Always”.
- **A11y:** Ask to resend is a button; the sheet is a `role=dialog` with focus on the title; the message preview is an editable field labelled “Message to Alex”; “Sent” is announced via `role=status`. The Wi-Fi link has `role=link` and the label “Use mobile data for missing media, setting”.

### 5.95 v0.1.12 — Restore indicator (security), `restore-notice` (+`--sheet`, `--list`, `--self`, `--dark`, `--sheet-dark`, `--ios`, `--ios-list`, `--ios-self-dark`, `--fr`, `--sheet-fr`, `--list-fr`, `--self-fr`)
Follows REQUIREMENTS “Restore indicator (security)” and RS1. The restored device creates a new device key, signed by the identity key, with `restored_at`. The old device key is marked superseded, and the restored device can't suppress the notice.
- **Contact side, in-chat system notice** (centred, neutral surface, not red): “Alex restored Hopla on a new device · Oct 24” / « Alex a restauré Hopla sur un nouvel appareil · 24 oct. », with the link “Not expected? Check it” / « Ce n’était pas Alex ? Vérifier ». It must show within 5 s of syncing (RS1).
- **Sheet** (tap the notice): “Alex restored Hopla · On {date} at {time}, on a new device, using their 24 words. That’s normal after a phone change. If Alex didn’t mention it, check with them in person or another way.” Rows: Safety number (Verified {date} · re-check suggested), Old device (Superseded). Actions: **Verify safety number** (primary), **Block Alex** (danger text, secondary), It was Alex (dismisses). *(Superseded v0.1.14, §5.98: safety number changed; primary action Verify in person (QR).)*
- **Restored badge:** a neutral grey pill with a sync glyph, “Restored” / « Restauré », shown for 7 days in the chat row and the chat header. It sits **after** the mint verified badge. Per REQUIREMENTS, verification isn't reset (O3 restores verification state; the identity key is unchanged), so the verified badge stays, and the pill plus the sheet suggest a re-check, as with a safety-number change. Chat row preview shows the notice text until a newer message arrives. Screen readers hear “Alex Rivera, verified, restored recently”. *(Superseded v0.1.14, §5.98: the verified badge is removed and the pill reads “Restored, not verified” until re-verified.)*
- **Restored phone:** a dismissible banner at the top of each chat: “You restored this account. Your contacts were notified.” / « Vous avez restauré ce compte. Vos contacts ont été prévenus. » It sits above the media-waiting banner. `restore-success` adds the note “Your contacts will see you restored Hopla on a new device.” *(v0.1.14 copy: “…Your contacts were notified and need to verify you again in person (QR). Show my QR”.)*
- **A11y:** the notice is `role=button` with a full label including the date; the sheet is a `role=dialog` with focus on the title; Block needs the standard block confirmation (§5.6x moderation); the banner is `role=status`, announced once. No motion beyond the standard sheet; reduced motion has no effect.

## 5N. v0.1.13 — Media backup setting (REQUIREMENTS “Include media”, CB5)

Generator `/workspace/omcp-v0113/gen.py`. New mockups: `settings-media-backup` and `restore-media`.

### 5.96 Settings › Backup › Photos, videos and files — `settings-media-backup` (+`--on`, `--uploading`, `--near-full`, `--full`, `--confirm-off`, `--deleting`, `--dark`, `--ios`, `--ios-dark`, `--fr`, `--confirm-off-fr`, `--near-full-fr`, `--deleting-fr`)
- **Master toggle, off by default:** “Also back up photos, videos and files · Added to the same encrypted backup. Backed up daily on Wi-Fi.” / « Sauvegarder aussi photos, vidéos et fichiers · Ajoutés à la même sauvegarde chiffrée. Sauvegarde quotidienne en Wi-Fi. » *(v0.1.14: media runs as a separate daily pass, Wi-Fi only by default; chats stay at most hourly.)* When it's off, the footnote reads: “Off: only your chats are backed up (2.4 MB). Photos and files come back from contacts when they’re nearby.” No media bytes are uploaded (CB5).
- **On:**
  - Per-type toggles (Photos, Videos, Voice notes, Files), each showing its local size. Turning on the master toggle selects all types by default except Videos (size; Q21 resolved).
  - Estimated backup size card: “Estimated backup size 1.6 GB · Chats 2.4 MB (hourly) + selected media (daily). Free in {service}: 3.1 GB”.
  - Row “Back up media on mobile data · Off: daily on Wi-Fi only (default).” REQUIREMENTS (v0.1.15) now explicitly accepts this optional toggle, off by default.
- **First upload** (`--uploading`): brand-soft card “First media backup · 34% · 550 MB / 1.6 GB · On Wi-Fi, in the background. Paused while on mobile data.” Android uses a WorkManager long-running job with a foreground notification; iOS uses a `BGProcessingTask` with background URLSession.
- **Quota:**
  - Nearly full (< 2× the remaining estimate): warn callout “{service} is almost full”, explaining that chats are backed up first.
  - Full: callout “{service} is full · Media backup is paused. Your chats are still backed up.” with a Manage storage link. Chats always take priority over media.
  - Warn style, never danger.
- **Turn off** (`--confirm-off`): `alertdialog` “Stop backing up media? · This deletes the 1.6 GB of photos, videos and files already in {service}. Your chats stay backed up, and media on this phone isn’t touched. After a future restore, media comes back only from contacts.” Primary danger button **Turn off and delete media backup**; Keep backing up. Default focus is Keep.
- **Deleting** (`--deleting`): neutral progress card “Deleting media backup… 60% · You can leave this screen. Your chats backup is unaffected.” The toggle shows off immediately. If deletion fails offline, it's retried and the status says so.
- **A11y:** toggles are `role=switch` with the full type name; progress cards are `role=progressbar` with text; quota callouts are `role=status` (near full) or `role=alert` (full); the danger button label repeats “delete”.
- The mobile-data rule for **re-fetching missing media from contacts** stays in §5.94 (separate setting).

### 5.97 Restore with media in the backup — `restore-media` (+`--success`, `--dark`, `--ios`, `--success-dark`, `--success-ios`, `--fr`, `--success-fr`)
- **Detect:** the body reads “Restore your chats, photos, voice notes, files, contacts and verified badges.” The card size reads “1.6 GB · chats + media”. When media backup was off, the existing `restore-detect` copy applies (“2.4 MB · text only”).
- **Progress:** same stages. The download is larger, so stage 1 shows MB/GB and the background notice matters more.
- **Success:** **Restored** adds “Photos, voice notes, files · 842 items, each checked” (hash verified, CB5). **Coming back later** lists only the types that were off, e.g. “Videos (not in your backup)”. **Not restored** is unchanged.
- **Partial media restore** (`--partial`, `--partial-fr`, `--partial-dark`, v0.1.14, Q23): Restored shows “Photos, voice notes, files · 610 of 842 restored, each checked”. Coming back later shows “232 items didn’t download + videos (not in backup) · Shown as placeholders · they refill when a contact who has them is nearby”. The missing items use the §5.92 placeholder flow automatically.
- **Placeholders (§5.92) apply only to media that isn't in the backup** (types that were off, or items added after the last backup). Media that was backed up appears directly, with no placeholder and no swap animation.

### 5.98 v0.1.14 — Re-verification after restore (REQUIREMENTS “Re-verification after restore”, RS2)
Generator `/workspace/omcp-v0114/gen.py`. `restore-notice` was re-rendered (20 frames: `default`, `sheet`, `list`, `self`, `qr`, `reverified`, `list-reverified`, `dark`, `sheet-dark`, `reverified-dark`, `ios`, `ios-list`, `ios-self-dark`, `ios-reverified`, `fr`, `sheet-fr`, `list-fr`, `self-fr`, `qr-fr`, `reverified-fr`), along with `restore-success` and `restore-media`.
- **Safety number** = identity key + current device key, so it changes after a restore. The old QR code no longer verifies (RS2).
- **Contact side:**
  - The mint verified badge is removed for every contact.
  - The pill reads **“Restored, not verified” / « Restauré, non vérifié »**, in the chat header and chat row. It stays **until re-verified**, not for 7 days: the 7-day rule covers only the “Restored” notice emphasis, and REQUIREMENTS says “until then” for the unverified state.
  - Row preview: “Alex restored Hopla · safety number changed”.
  - Notice link: “Safety number changed · Verify”.
- **Sheet:** “…That’s normal after a phone change, but Alex’s safety number has changed, so they’re no longer verified. Scan Alex’s QR code in person to verify again.” Safety number row: “Changed Oct 24 · not verified”. Primary **Verify in person (QR)**, secondary Block Alex, tertiary It was Alex (dismisses; Alex stays not verified). Calm neutral tone, no red except the Block text.
- **QR re-verify** (`--qr`): “Scan Alex’s QR code · On their new phone, in person” → “Safety numbers match”. Note: “Alex’s old QR code no longer works; it must be the one from their new device.”
- **Badge earned back** (`--reverified`, `--list-reverified`): a system line “✓ You verified Alex in person · today 4:40 PM” (mint tint). The pill is removed and the mint verified badge returns in the header and row.
- **Restored phone:**
  - Banner: “You restored this account. Your contacts were notified and need to verify you again in person (QR). Show my QR” / « …Vos contacts ont été prévenus et doivent vous revérifier en personne (QR). Montrer mon QR ».
  - `restore-success` note: “Your safety number changed. Contacts need to verify you again in person (QR).”
  - Verified badges row: “Still verified on your side”. O3 restores *your* verifications of others, since their keys didn't change.
- **A11y:** the pill label is “Restored, not verified”; the row reads “Alex Rivera, restored, not verified”; the QR match is announced via `role=status`; the re-verified system line is `role=status`. Colour is never the only carrier: the pill has text and a glyph.

### 5.99 v0.1.15 — Restored tag in groups and communities — `group-restored` (+`--members`, `--sheet`, `--dark`, `--members-dark`, `--ios`, `--ios-members`, `--ios-dark`, `--fr`, `--members-fr`, `--sheet-fr`)
Follows REQUIREMENTS “Groups and communities” (Q25). Generator `/workspace/omcp-v0115/gen.py`.
- **Compact tag:** a smaller version of the 1:1 pill (10 px text, 16 px tall, 9 px sync glyph, neutral `--surface-2` with a `--line` border): “Restored, not verified” / « Restauré, non vérifié ». It appears after the member's name on **every message sender name** and in the **member list**. It replaces the verified badge until that member is re-verified (§5.98), then disappears.
- **No system notice in groups or communities.** The full “<name> restored Hopla on a new device” notice appears only in 1:1 chats.
- **Tap:** the tag opens the same verify sheet as §5.98 (safety number changed · Verify in person (QR) · Block · It was Alex).
- **Member list:** a footnote explains the tag: “‘Restored, not verified’: this member restored Hopla on a new device. Tap the tag to verify in person.”
- **A11y:** the tag is `role=button` labelled “Restored, not verified. Tap to verify”. The sender line reads “Alex Rivera, restored, not verified”. The touch target is extended to 44 pt via padding on the name row.

## 5O. v0.1.16 — One-tap Ask to resend; keep media you sent (approved by Jakob, 2026-10-24)

Generator `/workspace/omcp-v0116/gen.py` (renders with `render.py`, system python3 Playwright + `/usr/bin/google-chrome`). New mockups `media-resend-note`, `settings-storage`; `media-missing` re-rendered.

### 5.100 Ask to resend — one tap with optional note — `media-missing` (`--gone`, `--sent`, `--ios-gone`, `--gone-fr`, `--sent-fr`), `media-resend-note` (+`--empty`, `--sent`, `--dark`, `--sent-dark`, `--ios`, `--ios-sent`, `--fr`, `--sent-fr`)
Replaces the v0.1.12 confirm sheet (§5.94).
- **Gone state:** “{name} doesn’t have this anymore.” with the pill **Ask to resend** / **Demander un renvoi** and a secondary text button **Add a note** / **Ajouter un mot**.
- **One tap sends.** Tapping Ask to resend sends the request right away: a fixed system request that carries the media reference (type, date, hash locator) and nothing else. It doesn't mention the restore. Status in the bubble: “Request sent to {name} · it’ll appear here if they resend it.” Toast: “Request sent to {name}” with **Undo** for 5 s (Undo recalls the request before delivery; after that it's a no-op and the toast is gone).
- **Optional note:** Add a note expands an inline field in the bubble (label “Note to {name} · optional”, placeholder “Add a note (optional)”, max 140 characters, counter, helper “Sent with the photo reference only”). Actions: filled **Ask to resend** and Cancel. An empty note sends the same as one tap.
- **Sent with note:** the note is quoted under the status (“Your note: “…””). The recipient sees the request with the note in the chat.
- The resent file is matched by hash and swaps in like any other restore (§5.92). One active request per item; tapping the pill again after sending does nothing (it's replaced by the status).
- **A11y:** the pill is a button “Ask {name} to resend this photo”; Add a note is a button that moves focus into the field; the field is a multiline textbox labelled “Note to {name}, optional”; the counter is announced on reaching 120; Request sent and Undo go through `role=status`, and Undo is reachable by keyboard / switch access for its 5 s window (extended to 10 s when a screen reader is on).

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Pill | Ask to resend | Demander un renvoi | לבקש שליחה חוזרת |
| Secondary | Add a note | Ajouter un mot | הוספת הערה |
| Field | Note to {name} · optional | Mot pour {name} · facultatif | הערה ל{name} · אופציונלי |
| Toast | Request sent to {name} · Undo | Demande envoyée à {name} · Annuler | הבקשה נשלחה ל{name} · ביטול |
| Status | Request sent to {name} · it’ll appear here if they resend it | Demande envoyée à {name} · elle apparaîtra ici si elle est renvoyée | הבקשה נשלחה ל{name} · הקובץ יופיע כאן אם ישלחו אותו שוב |

### 5.101 Settings › Storage › Keep media you sent — `settings-storage` (+`--edit`, `--edit-custom`, `--edit-off`, `--off`, `--forever`, `--dark`, `--edit-dark`, `--ios`, `--ios-edit`, `--fr`, `--edit-fr`, `--off-fr`)
- **Storage screen:** usage card (“2.6 GB used by Hopla”, bar split into Media you sent / Media you received / Chats), then section “Media you sent” with the row **Keep media you sent · 90 days or 2 GB ›** and the explanation “Copies of photos, videos and files you sent, so you can resend them when a contact asks (for example after they restore Hopla).” Footnote: oldest sent media is removed first when a limit is reached; messages and captions stay. Other rows: Media you received, Review large files, Auto-download on mobile data.
- **Defaults:** on, **90 days or 2 GB, whichever limit is reached first.** Both can be changed.
- **Edit sheet** (`--edit`): title “Sent media”, master toggle **Keep media you sent** (“Lets you resend files to contacts who lost them.”), then two segmented controls: **Keep for** 30 days · **90 days** · 1 year · Forever, and **Up to** 500 MB · **2 GB** · 5 GB · No limit. A live summary says what the combination does and current usage. Actions: Save, Reset to default (90 days or 2 GB). Forever + No limit is allowed (row reads “Forever”).
- **Off** (`--edit-off`): toggle off greys out the limits; warn callout “Sent media is removed after delivery. 1.4 GB will be freed. Contacts who lose a file can’t ask you to resend it.” Primary becomes **Turn off and free 1.4 GB**; secondary Keep media (default focus). On the Storage screen the row reads Off and a warn callout repeats the consequence (`--off`). Warn style, never danger.
- **Link with §5.100:** when the sender no longer has the file (pruned or off), the requester's request still arrives; the sender sees “You no longer have this file” and can reply in chat. Lowering a limit prunes oldest-first on Save.
- **A11y:** the row is a button reading the current value; the toggle is `role=switch`; each segmented control is a `radiogroup` labelled “Keep for” / “Up to”; the summary is `role=status`; the usage bar is `role=img` with a full text label.

| Copy | EN | FR | HE (draft) |
|------|----|----|---|
| Row | Keep media you sent | Garder les médias envoyés | שמירת מדיה ששלחת |
| Default value | 90 days or 2 GB | 90 jours ou 2 Go | 90 יום או 2 ג״ב |
| Groups | Keep for / Up to | Durée / Jusqu’à | לשמור למשך / עד |
| Presets | 30 days, 90 days, 1 year, Forever · 500 MB, 2 GB, 5 GB, No limit | 30 j, 90 j, 1 an, Toujours · 500 Mo, 2 Go, 5 Go, Sans limite | 30 יום, 90 יום, שנה, לתמיד · 500 מ״ב, 2 ג״ב, 5 ג״ב, ללא הגבלה |


## 5P. v0.1.17 — Dark-mode WCAG AA overhaul; backup copy covers chats (requested by Jakob, 2026-10-24)

Generator `/workspace/omcp-v0117/` (`gen.py` patches the v0.1.16 pack, `render.py`, `contrast.py`, `table.py`, `compose.py`). All 70 mockups re-rendered (445 frames).

### 5.102 Dark theme contrast
- **Rule:** every dark frame meets WCAG 2.1 AA: text 4.5:1 (large 3:1), icons, control boundaries, toggle tracks/thumbs and focus rings 3:1. Disabled states are exempt but target 3:1.
- **Dark token values changed** (light values and brand hues untouched): `--ink-2` #a3b1c2 → #b4c0ce, `--ink-3` #7a8a9c → #95a3b4, `--line` #2a3646 → #687990, `--line-strong` #3d4d60 → #7a8ba1.
- **Switch (dark):** the thumb on the on-track uses `--brand-ink` (dark thumb on teal track, as Material 3 dark does); white on `--brand` was 2.3:1.
- **On-accent text:** text and icons on solid `--brand`, `--ok`, `--danger`, `--warn` fills use `--brand-ink`, not literal `#fff` (`#fff` = `--brand-ink` in light, so light is unchanged). Proggy: map to `onPrimary` / an on-accent colour, never hard-code white.
- **Audit:** `a11y/dark-contrast.md` (every dark fg/bg pair, before → after), `a11y/dark-before-after.png`, `a11y/light-spot-check.png`.
- **Known gap:** the dimmed (opacity .4) action buttons in `nearby-skeleton--dark` stay at 1.3–2.5:1; they are disabled while loading (exempt). Fixing them needs a different disabled treatment than opacity, which would also change light.

### 5.103 Backup contents in copy
- **Meaning:** the cloud backup (Google Drive / iCloud) holds the recovery phrase **and all chats** (text; photos, voice notes and files only if media backup is on, §5.96). The written 24-word phrase alone restores **identity and contacts, not chats**.
- Applied to `onboarding-secure` (sub, honest note, password sheet), `onboarding-secure-phrase`, `onboarding-secure-reminder`, `onboarding-restore`, `settings-backup` (method lines, honest note, delete sheet), `restore-source` (“My 24 words” row), `restore-error--corrupted`, `restore-password` (forgot hint, lockout), `restore-reinstall` (forgot hint). Copy tables §5.21, §5.26–5.30, §5.31, §5.89 updated in place (EN/FR).
- No Arabic copy exists in the pack yet (EN/FR only), so there is nothing to change in AR; RTL stays a §5 checklist item. *(v0.1.18: Hebrew added as the first RTL locale, §5.105–5.106.)*

## 5Q. v0.1.18 — Light-mode WCAG AA matched to dark; Hebrew (RTL) locale draft (requested by Jakob, 2026-10-24)

Generator `/workspace/omcp-v0118/` (`gen.py` light tokens + disabled/scrim fixes, `he.py` + `he_strings.py` Hebrew frames, `he_spec.py` Hebrew copy column, `contrast.py`, `table18.py`, `compose18.py`, `docs18.py`, `render.py`). All mockups re-rendered.

### 5.104 Light theme contrast (light token values changed with Jakob’s approval)
- **Rule:** every light frame meets WCAG 2.1 AA like dark (§5.102): text 4.5:1, icons, control boundaries, toggle tracks/thumbs, focus rings 3:1. Disabled controls target 3:1 in both themes (no `opacity` dimming).
- **Light token values:** `--ink-2` #4a5b70 → #3b495a, `--ink-3` #6b7c90 → #515e6d, `--line` #d8dee8 → #73849a, `--line-strong` #b8c2d0 → #617289. Dark values, soft tints and brand hues unchanged.
- **Matched pair:** each light step has the same contrast on light `--surface` as the dark step on dark `--surface`: ink-2 9.18 / 9.18, ink-3 6.62 / 6.60, line 3.82 / 3.82, line-strong 4.91 / 4.87. Table and side-by-side in `a11y/light-vs-dark.png`.
- **Disabled:** `nearby-skeleton` loading row → `.is-loading` + `aria-disabled` (`--surface-2` fill, `--line-strong` ring, `--ink-3` glyph); `restore-password` lockout and `settings-storage` edit-off use `.btn.disabled` / `.is-disabled`. ≥ 3.5:1 light, ≥ 4.0:1 dark.
- **Inverse toasts** (`background:var(--ink)`): accent = the other theme’s teal (`#3dbdb6` on light’s near-black toast, `#0b6e6a` on dark’s near-white toast), like Material `inversePrimary`.
- **Scrims over photos:** ≥ .62 alpha behind white glyphs/labels (report button, “Restored” pill).
- **Audit:** `a11y/light-contrast.md` (new), `a11y/dark-contrast.md` (re-measured), `a11y/light-before-after.png`, `a11y/light-vs-dark.png`.

### 5.105 Hebrew (he) — DRAFT
- **Status: DRAFT — pending native-speaker review.** All Hebrew strings (frames and the “HE (draft)” column in every copy table) were written without a native reviewer. Do not ship before review, especially gendered forms.
- **Frames** (27, suffix `--he`, same naming as `--fr`): `onboarding-welcome--he.png`, `onboarding-permissions--he.png`, `onboarding-profile--he.png`, `onboarding-secure--he.png`, `onboarding-secure-phrase--he.png`, `onboarding-secure-quiz--he.png`, `onboarding-done--he.png`, `onboarding-restore--he.png`, `chats-list--he.png`, `chats-list--he-dark.png`, `chat-thread--he.png`, `chat-thread--he-dark.png`, `settings-privacy--he.png`, `settings-backup--he.png`, `settings-storage--he.png`, `settings-notifications--he.png`, `restore-detect--he.png`, `restore-source--he.png`, `restore-password--he.png`, `restore-progress--he.png`, `restore-success--he.png`, `nearby-area--he.png`, `nearby-chat--he.png`, `burn-bubbles--he.png`, `burn-composer--he.png`, `mod-community--he.png`, `mod-report--he.png`.
- **Font:** Rubik (variable, Google Fonts, installed locally) → fallbacks Noto Sans Hebrew, Heebo, system-ui. Every Hebrew code point used is in Rubik (checked with fontTools + Chrome platform-font report); only ✦, U+2011 and emoji fall back. No tofu. Apps: Android uses the system Hebrew font (Noto Sans Hebrew); iOS uses SF Hebrew. Rubik is for mockups only unless the brand adopts it.
- **Tone:** informal, plural / infinitive where possible to avoid gendered address (“הזינו”, “להמשיך”); “את/ה” where a singular is unavoidable. Buttons use nouns/infinitives (“שליחת דיווח”, “להתחיל מחדש”).
- **Fictional names (frames):** Sam Okafor → טל מזרחי (tal.mizrahi@gmail.com), Alex Rivera → נועה שרון, Jules Bernard → יואב ברק, Sam Kim → שירה כץ, Morgan Reed → עומר רז, Maya → מאיה, Robin → רוני, Ines → ענבל, Teo → תום, Riverside Gardeners → גינת הנחל, Neighbourhood watch → משמר השכונה. Léa Dubois stays (French contact; tests mixed-direction text).
- **Kept Latin:** Hopla, OMCP, Google Drive, iCloud, Face ID, PeerIDs (`omcp1:…`), emails, the 24 BIP39 recovery words (English standard list; there is no Hebrew BIP39 list) and their numbering grid.

### 5.106 RTL mirroring rules (normative for he, and for ar/fa later)
- **Set** `dir="rtl"` and `lang="he"` on the root (Android `android:supportsRtl="true"` + `start/end` everywhere; iOS leading/trailing + `semanticContentAttribute` default).
- **Mirror:** whole layout order (nav: back button at the right, actions at the left; tab bar order; list rows: avatar right, time/badges left); back chevron points **right** and list/forward chevrons (› rows) point **left**; send icon; message bubbles (incoming on the **right**, own on the **left**, tail corner mirrored); progress bars and step indicators fill **from the right**; stacked storage bar starts at the right; toggles (off = thumb right, on = thumb left); segmented controls and button rows (Pass/Profile/Like) reverse; overlays pinned to a side (report flag, menus, badges) swap sides; swipe directions swap (swipe **left** = forward/next, swipe-to-reply from the left edge; Nearby “Like” gesture = swipe toward the Like button, i.e. left in RTL).
- **Do not mirror:** photos, video, avatars and media; logos and the app icon; the status bar and clock; digits and numbers (always LTR, e.g. 9:41, 1,284, 2.6); phone numbers, PeerIDs, emails, URLs, file names; QR codes (direction forced LTR); the recovery-word grid and safety-number grid (Latin/numeric, read left-to-right); timer rings and checkmarks; media playback controls (play/seek stay LTR); non-directional icons (search, bell, shield, flame, heart, flag).
- **Bidi:** wrap Latin/number runs inside Hebrew text in isolates (`<bdi>`, `\u2066…\u2069`, Android `BidiFormatter`, iOS `NSLocale`-aware formatting) so “Merci, à demain !”, “< 1 km”, “22:00 – 07:00” and IDs keep their order; prefer Hebrew-first phrasing for mixed chips (“4 בסביבה · Mesh”). Units in Hebrew: ג״ב, מ״ב, ק״מ, דק׳, שע׳.
- **Dates:** “12 באוק׳ 2026, 21:40” (24 h clock). Week days as letters with geresh (א׳ … ש׳).

## 5R. Hopla News (REQUIREMENTS § Hopla News v0.4, draft, revised 2026-10-24 — **all v0.4**) — v0.1.19, revised v0.1.20 (Jakob’s decisions, 2026-10-24)

Generator `/workspace/omcp-v0120/` (`gen.py`, `strings.py`, `news.css`, `render.py`, `a11y/dark-contrast.py`, `docs20.py`); v0.1.19 generator kept at `/workspace/omcp-v0119/`. 18 mockups × 6 frames (EN, FR, HE draft; light + dark) = **108 PNGs**. No tokens added or changed (`news.css` composes existing tokens); `tokens.css` and `brand/` untouched. All people, contacts, places and stories are fictional; images are generated abstract art.

**v0.1.20 removed (REQUIREMENTS revision):** publisher/curator signature checks and trust levels, curators and curator follow UI, the “Verified publisher” / “Signed · via” badges, the trust sheet (`news-trust`), Sources and curators (`news-sources`), hidden-story counts, the “Story unavailable” stub, full-text reader (`news-reader-summary`; the reader is always summary + link now), and any publish affordance.

### 5.107 Navigation — News is a root tab (decided)
- Bar: **Chats · News · Communities · Nearby · You** (REQUIREMENTS “In the app”). **Live** (“Coming in v0.2”) moves to the **top of Communities**. Builds without Nearby drop only Nearby. RTL mirrors the order. *(Q26 resolved.)*

### 5.108 News tab — `news-top`, `news-local`, `news-local-empty`, `news-world`, `news-top-city`
- Nav: “News”, Search, **News settings** (sliders → §5.115). Chips (`role=tablist`): Top · Local · World · Politics · Tech · Sport · Health · Emergencies.
- Status strip: mesh `PathChip` + “Updated 4 min ago · 3 neighbours”. Pinned emergency alerts first (§5.111), then stories by **recency, then how many phones relayed the story** (“Relayed by N phones” when N ≥ 2). No engagement ranking.
- **Default:** newest world headlines out of the box (`news-world`, header “World headlines — Default feed · newest world headlines from Google News”).
- **Local / city:** city bar “City: Riverside — Typed by you” or “From your location · only the city name is used”, with Change. `news-top-city`: Top leads with the city’s stories, then matching topics.

### 5.109 Story card — source label + relaying contact
- Card: topic · time, headline, **source label** + “via ‹contact›”, optional “Relayed by N phones”, thumbnail 72 px (≤ 20 KB WebP; none → no thumbnail).
- **Source label** (`SourceLabel`): neutral text pill **“Google News”** — text on `--surface`, 1 px `--ink-2` border, no logo or brand colour (not the official logo). `role=img`, label “Source: Google News”.
- **Ready for a second source:** today the only source is **Google News RSS**. An X news RSS feed will come later from a separate project; `SourceLabel` is text-only and sized to its content (any short name, no per-source colour or icon), the card/reader/bubble layouts wrap the label + via line, and the reader link key is per source (`news.read_gn`; a future `news.read_<source>`). No layout change is needed to add it.
- **“via ‹contact›”**: the relaying phone, from the normal envelope signature — contact name if known, else short PeerID (`via omcp1:7f3a…`, LTR). Omitted when this phone fetched the story itself. No per-story verification is done or shown.
- One focus stop: “headline, source, via contact, time”.

### 5.110 Reader — `news-reader`
- Nav title = source label. Topic · time, headline, source + via, “Fetched by a phone with internet”, hero thumbnail, **Summary** (2–3 sentences), secondary button **Read on Google News** (FR “Lire sur Google News”, HE “לקריאה ב־Google News”) with “Opens your browser and leaves Hopla. Needs Internet.”
- Bottom bar: **Share** (primary, §5.113) and More (mute this topic, copy link). Reading is local; nothing is prefetched or reported.

### 5.111 Emergency alerts — `news-alert` (+ pinned card in Top, Local, catch-up)
- Alerts are stories in the **Emergencies** topic from Google News (no “official key” any more): `role=alert` card, 2 px `--danger` border, warning glyph, “EMERGENCY”, pin + “Pinned”, source label + via + “Emergencies topic”, “stays pinned for 7 days” (envelope expiry 7 d).
- Detail: body, shelter row, **Read on Google News**, toggle **Notify me about emergency alerts — on by default** (“You can turn this off”), mirrored in Settings › Notifications. *(Q27, Q30 resolved.)*

### 5.112 Sources — Google News RSS only (replaces Sources and curators)
- There is no source management: for now the only source is **Google News RSS** (X dropped from v0.1.20 by Jakob’s correction; an X news RSS feed comes later from a separate project — see §5.109). Phones with internet fetch them and relay stories over the mesh in normal envelopes; duplicates (same source + link) appear once (NW2).
- **Relay settings — placeholder:** the Relay settings screen will follow once the open relay questions are answered. Not designed yet.

### 5.113 Share to chat or community — `news-share`, `news-share-bubble`
- Sheet over the reader: story preview (headline, source, via), sections **Chats** and **Communities** (single select), note “Sends a news card. A discussion thread starts under it.”, **Send**.
- Chat bubble: thumbnail, source label + via, headline, “Open story”.

### 5.114 Discussion threads — `news-chat-thread`, `news-community-thread`
- The shared news card heads a **Discussion** thread (“N replies”, “Reply in thread…”), same pattern as community posts (NW6). In communities: “Community rules and moderation apply to this discussion.” (§23) and “News can’t be posted by users. Discuss it in your communities.”
- **No citizen reports:** there is no compose/publish entry anywhere in News. *(Q34 resolved.)*

### 5.115 Interests — `news-onboarding-interests` (optional, skippable), `news-interests`, `news-interests-location`, `news-location-permission`
- Topics (multi-select chips, check / + glyph), Languages (EN / FR / HE), **City**: secondary button **Use my location**, “or type your city”, text field. Note: “Uses the same location permission as Nearby. Only the city name is used to subscribe to its news channel; your location is never sent.”
- **Use my location** asks the OS location permission only if Nearby hasn’t been granted it (same permission, `news-location-permission`): “Allow Hopla to use your location?” + why + “Same permission as Nearby”; Allow while using / Allow once / Don’t allow. Granted → field shows “Riverside · from your location” (editable). Denied → manual entry stays; no re-prompt loop.
- Settings adds **Muted topics** (long-press a topic chip to mute) and **News storage** (≤ 20 MB, oldest first).
- Onboarding: “Step 4 of 5 · optional”, Continue / Skip; skipping keeps the world headlines default.

### 5.116 Offline / mesh catch-up — `news-catchup`
- Queue chip “Offline for 3 days”, `role=status` card “Catching up from Maya’s phone”, “38 of 120 stories” `progressbar` (fills from the right in RTL), “News spreads phone to phone. Chats always go first.” (NW3), “New since you were offline”, “Older stories expired (48 h; breaking news 7 days).” (NW1).

### 5.117 Empty and loading — `news-empty`, `news-local-empty`, `news-loading`
- “No news yet — Stories arrive from nearby phones that have internet.” + **Choose interests**; Local empty “No local news for Riverside yet” + Change city; loading = 4 skeleton cards, `aria-busy`, hidden “Loading stories”.

### 5.118 Accessibility, RTL
- **Contrast (whole pack, re-run):** light 11772 measurements, **0 failures**; dark 4500, **0 failures**. News only: light 2386 / 0, worst text 5.22:1 (text, #8a5a10 on #faf0d8, news-catchup--default), worst UI/icon 3.50:1 (ui, #73849a on #f3f5f8, news-interests-location--default); dark 2386 / 0, worst text 4.96:1 (text, #95a3b4 on #143836, news-share-bubble--dark), worst UI/icon 3.82:1 (ui, #687990 on #151d28, news-alert--dark). `a11y/news-contrast.md`.
- Source labels and state chips are text (no colour-only meaning); alert = glyph + word + border. Targets ≥ 44 pt. HE per §5.106: Latin source names, times and PeerIDs isolated (`<bdi>`). HE copy is a **DRAFT**.
- Frames: `news-alert--dark.png`, `news-alert--fr-dark.png`, `news-alert--fr.png`, `news-alert--he-dark.png`, `news-alert--he.png`, `news-alert.png`, `news-catchup--dark.png`, `news-catchup--fr-dark.png`, `news-catchup--fr.png`, `news-catchup--he-dark.png`, `news-catchup--he.png`, `news-catchup.png`, `news-chat-thread--dark.png`, `news-chat-thread--fr-dark.png`, `news-chat-thread--fr.png`, `news-chat-thread--he-dark.png`, `news-chat-thread--he.png`, `news-chat-thread.png`, `news-community-thread--dark.png`, `news-community-thread--fr-dark.png`, `news-community-thread--fr.png`, `news-community-thread--he-dark.png`, `news-community-thread--he.png`, `news-community-thread.png`, `news-empty--dark.png`, `news-empty--fr-dark.png`, `news-empty--fr.png`, `news-empty--he-dark.png`, `news-empty--he.png`, `news-empty.png`, `news-interests--dark.png`, `news-interests--fr-dark.png`, `news-interests--fr.png`, `news-interests--he-dark.png`, `news-interests--he.png`, `news-interests-location--dark.png`, `news-interests-location--fr-dark.png`, `news-interests-location--fr.png`, `news-interests-location--he-dark.png`, `news-interests-location--he.png`, `news-interests-location.png`, `news-interests.png`, `news-loading--dark.png`, `news-loading--fr-dark.png`, `news-loading--fr.png`, `news-loading--he-dark.png`, `news-loading--he.png`, `news-loading.png`, `news-local--dark.png`, `news-local--fr-dark.png`, `news-local--fr.png`, `news-local--he-dark.png`, `news-local--he.png`, `news-local-empty--dark.png`, `news-local-empty--fr-dark.png`, `news-local-empty--fr.png`, `news-local-empty--he-dark.png`, `news-local-empty--he.png`, `news-local-empty.png`, `news-local.png`, `news-location-permission--dark.png`, `news-location-permission--fr-dark.png`, `news-location-permission--fr.png`, `news-location-permission--he-dark.png`, `news-location-permission--he.png`, `news-location-permission.png`, `news-onboarding-interests--dark.png`, `news-onboarding-interests--fr-dark.png`, `news-onboarding-interests--fr.png`, `news-onboarding-interests--he-dark.png`, `news-onboarding-interests--he.png`, `news-onboarding-interests.png`, `news-reader--dark.png`, `news-reader--fr-dark.png`, `news-reader--fr.png`, `news-reader--he-dark.png`, `news-reader--he.png`, `news-reader.png`, `news-share--dark.png`, `news-share--fr-dark.png`, `news-share--fr.png`, `news-share--he-dark.png`, `news-share--he.png`, `news-share-bubble--dark.png`, `news-share-bubble--fr-dark.png`, `news-share-bubble--fr.png`, `news-share-bubble--he-dark.png`, `news-share-bubble--he.png`, `news-share-bubble.png`, `news-share.png`, `news-top--dark.png`, `news-top--fr-dark.png`, `news-top--fr.png`, `news-top--he-dark.png`, `news-top--he.png`, `news-top-city--dark.png`, `news-top-city--fr-dark.png`, `news-top-city--fr.png`, `news-top-city--he-dark.png`, `news-top-city--he.png`, `news-top-city.png`, `news-top.png`, `news-world--dark.png`, `news-world--fr-dark.png`, `news-world--fr.png`, `news-world--he-dark.png`, `news-world--he.png`, `news-world.png`.

### 5.119 Copy (EN / FR / HE draft)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| `news.news` | News | Actus | חדשות |
| `news.top` | Top | À la une | מובילות |
| `news.local` | Local | Local | מקומי |
| `news.world` | World | Monde | עולם |
| `news.emerg` | Emergencies | Urgences | חירום |
| `news.updated` | Updated 4 min ago · 3 neighbours | Mis à jour il y a 4 min · 3 voisins | עודכן לפני 4 דק׳ · 3 שכנים |
| `news.pinned` | Pinned | Épinglé | מוצמד |
| `news.emergency` | Emergency | Urgence | חירום |
| `news.src_gn` | Google News | Google News | Google News |
| `news.via` | via ‹contact› | via ‹contact› | דרך ‹contact› |
| `news.relayed` | Relayed by N phones | Relayé par N téléphones | הועבר על ידי N טלפונים |
| `news.fetched_by` | Fetched by a phone with internet | Récupéré par un téléphone connecté | נאסף על ידי טלפון עם אינטרנט |
| `news.region` | City: Riverside | Ville : Riverside | עיר: ריברסייד |
| `news.region_note` | Typed by you | Saisie par vous | הוקלד על ידיכם |
| `news.region_note_loc` | From your location · only the city name is used | Depuis votre position · seul le nom de la ville est utilisé | לפי המיקום שלכם · רק שם העיר בשימוש |
| `news.summary_lbl` | Summary | Résumé | תקציר |
| `news.read_gn` | Read on Google News | Lire sur Google News | לקריאה ב־Google News |
| `news.link_note` | Opens your browser and leaves Hopla. Needs Internet. | Ouvre votre navigateur et quitte Hopla. Internet requis. | נפתח בדפדפן ויוצא מ־Hopla. נדרש אינטרנט. |
| `news.share_btn` | Share to chat | Partager | שיתוף בצ׳אט |
| `news.sh_title` | Share | Partager | שיתוף |
| `news.sh_note` | Sends a news card. A discussion thread starts under it. | Envoie une carte d’actu. Un fil de discussion démarre dessous. | נשלח כרטיס חדשות. שרשור דיון נפתח מתחתיו. |
| `news.chats_h` | Chats | Discussions | צ׳אטים |
| `news.communities_h` | Communities | Communautés | קהילות |
| `news.send` | Send | Envoyer | שליחה |
| `news.open_story` | Open story | Ouvrir l’article | לפתיחת הכתבה |
| `news.alert_title` | Emergency alert | Alerte d’urgence | התרעת חירום |
| `news.a_from` | Emergencies topic | Rubrique Urgences | נושא: חירום |
| `news.alert_issued` | 25 min ago · stays pinned for 7 days | il y a 25 min · reste épinglée 7 jours | לפני 25 דק׳ · מוצמד למשך 7 ימים |
| `news.alert_notif` | Notify me about emergency alerts | Me notifier des alertes d’urgence | התראות על מצבי חירום |
| `news.alert_notif_d` | On by default. Sound and banner, even when other news is quiet. You can turn this off. | Activé par défaut. Son et bannière, même si les autres actus sont silencieuses. Désactivable. | מופעל כברירת מחדל. צליל ובאנר, גם כשחדשות אחרות שקטות. אפשר לכבות. |
| `news.whl` | World headlines | Titres du monde | כותרות העולם |
| `news.whl_d` | Default feed · newest world headlines from Google News | Fil par défaut · derniers titres du monde depuis Google News | פיד ברירת מחדל · כותרות העולם האחרונות מ־Google News |
| `news.settings` | News settings | Réglages des actus | הגדרות חדשות |
| `news.int_d` | Pick topics, languages and your city. Personalisation runs on this phone only. | Choisissez sujets, langues et votre ville. La personnalisation reste sur ce téléphone. | בחרו נושאים, שפות ואת העיר שלכם. ההתאמה האישית מתבצעת רק בטלפון הזה. |
| `news.topics` | Topics | Sujets | נושאים |
| `news.langs` | Languages | Langues | שפות |
| `news.city` | City | Ville | עיר |
| `news.use_loc` | Use my location | Utiliser ma position | שימוש במיקום שלי |
| `news.or_type` | or type your city | ou saisissez votre ville | או הקלידו את העיר |
| `news.loc_note` | Uses the same location permission as Nearby. Only the city name is used to subscribe to its news channel; your location is never sent. | Utilise la même autorisation de localisation qu’À proximité. Seul le nom de la ville sert à s’abonner à son canal ; votre position n’est jamais envoyée. | משתמש באותה הרשאת מיקום כמו ״בסביבה״. רק שם העיר משמש להרשמה לערוץ שלה; המיקום שלכם לא נשלח אף פעם. |
| `news.loc_got` | Riverside · from your location | Riverside · depuis votre position | ריברסייד · לפי המיקום שלכם |
| `news.perm_t` | Allow Hopla to use your location? | Autoriser Hopla à utiliser votre position ? | לאפשר ל־Hopla להשתמש במיקום? |
| `news.perm_d` | Hopla uses this to find your city for local news and for Nearby. Only the city name leaves this step; your exact location is never sent. | Hopla s’en sert pour trouver votre ville (actus locales) et pour À proximité. Seul le nom de la ville est utilisé ; votre position exacte n’est jamais envoyée. | ‏Hopla משתמשת בזה כדי למצוא את העיר שלכם לחדשות מקומיות ול״בסביבה״. רק שם העיר נשמר; המיקום המדויק לא נשלח אף פעם. |
| `news.perm_shared` | Same permission as Nearby | Même autorisation qu’À proximité | אותה הרשאה כמו ״בסביבה״ |
| `news.perm_ok` | Allow while using the app | Autoriser pendant l’utilisation | לאפשר בזמן השימוש באפליקציה |
| `news.perm_once` | Allow once | Autoriser une fois | לאפשר פעם אחת |
| `news.perm_no` | Don’t allow | Ne pas autoriser | לא לאפשר |
| `news.mute_topics` | Muted topics | Sujets en sourdine | נושאים מושתקים |
| `news.mute_none` | None — long-press a topic chip to mute it | Aucun — appui long sur un sujet pour le mettre en sourdine | אין — לחיצה ארוכה על נושא משתיקה אותו |
| `news.muted_t` | Muted | En sourdine | מושתק |
| `news.storage` | News storage | Stockage des actus | אחסון חדשות |
| `news.storage_d` | Up to 20 MB on this phone. Oldest stories are removed first. | Jusqu’à 20 Mo sur ce téléphone. Les plus anciens articles sont supprimés en premier. | עד 20 מ״ב בטלפון. הכתבות הישנות נמחקות ראשונות. |
| `news.ob_title` | What news do you want? | Quelles actus voulez-vous ? | אילו חדשות מעניינות אתכם? |
| `news.step` | Step 4 of 5 · optional | Étape 4 sur 5 · facultatif | שלב 4 מתוך 5 · לא חובה |
| `news.skip` | Skip | Passer | דילוג |
| `news.cont` | Continue | Continuer | המשך |
| `news.save` | Save | Enregistrer | שמירה |
| `news.your_city` | Your city: Riverside | Votre ville : Riverside | העיר שלכם: ריברסייד |
| `news.thread` | Discussion | Discussion | דיון |
| `news.replies` | N replies | N réponses | N תגובות |
| `news.reply_ph` | Reply in thread… | Répondre dans le fil… | תגובה בשרשור… |
| `news.modnote` | Community rules and moderation apply to this discussion. | Les règles et la modération de la communauté s’appliquent à cette discussion. | כללי הקהילה והמודרציה חלים על הדיון הזה. |
| `news.nopub` | News can’t be posted by users. Discuss it in your communities. | Les utilisateurs ne peuvent pas publier d’actus. Discutez-en dans vos communautés. | משתמשים לא יכולים לפרסם חדשות. אפשר לדון בהן בקהילות. |
| `news.offline` | Offline for 3 days | Hors ligne depuis 3 jours | לא מחובר כבר 3 ימים |
| `news.cu_title` | Catching up from Maya’s phone | Rattrapage depuis le téléphone de Maya | מתעדכנים מהטלפון של מאיה |
| `news.cu_prog` | 38 of 120 stories | 38 articles sur 120 | 38 מתוך 120 כתבות |
| `news.cu_note` | News spreads phone to phone. Chats always go first. | Les actus passent de téléphone en téléphone. Les discussions passent toujours en premier. | חדשות עוברות מטלפון לטלפון. צ׳אטים תמיד קודמים. |
| `news.cu_new` | New since you were offline | Nouveau depuis votre absence | חדש מאז שהייתם מנותקים |
| `news.cu_expired` | Older stories expired (48 h; breaking news 7 days). | Les articles plus anciens ont expiré (48 h ; 7 jours pour l’info urgente). | כתבות ישנות יותר פגו (48 שע׳; מבזקים 7 ימים). |
| `news.empty_t` | No news yet | Pas encore d’actus | עדיין אין חדשות |
| `news.empty_d` | Stories arrive from nearby phones that have internet. Pick your interests to get started. | Les articles arrivent des téléphones proches qui ont Internet. Choisissez vos centres d’intérêt pour commencer. | כתבות מגיעות מטלפונים קרובים שיש להם אינטרנט. כדי להתחיל, בחרו תחומי עניין. |
| `news.empty_btn` | Choose interests | Choisir mes intérêts | בחירת תחומי עניין |
| `news.empty_hint` | Nobody nearby has news right now. | Personne à proximité n’a d’actus pour l’instant. | לאף אחד בסביבה אין חדשות כרגע. |
| `news.loading` | Loading stories | Chargement des articles | טוענים כתבות |
| `news.le_t` | No local news for Riverside yet | Pas encore d’actus locales pour Riverside | עדיין אין חדשות מקומיות לריברסייד |
| `news.le_d` | Try another city or check back when you meet someone nearby. | Essayez une autre ville ou revenez quand vous croisez quelqu’un. | אפשר לנסות עיר אחרת או לחזור כשתפגשו מישהו בסביבה. |
| `news.le_btn` | Change city | Changer de ville | שינוי עיר |

## 5S. v0.1.21 — WCAG 2.2 additions (2.5.8, 2.4.11, 2.5.7) and glass-bar contrast (requested by Jakob, 2026-10-26)

Applies to every screen. Normative for Proggy.

**2.5.8 Target Size (Minimum) — pass.** No interactive target is under 24×24 CSS px. Several are below the **platform guidance (44 pt iOS / 48 dp Android)**: `.icon-btn` and `.send-btn` 40 px, `.btn-sm` and news `.nbtn` 36 px, news `.nchip` 32 px, toggle 48×28. **Rule:** keep the visual size and extend the touch area to ≥ 44 pt / 48 dp (iOS `contentEdgeInsets` / larger `frame` with `.contentShape`; Android `minimumInteractiveComponentSize()` / TouchDelegate). Adjacent hit areas must not overlap. Mockups model this for news chips and buttons with an invisible hit-slop (`news.css`); there is no visual change.

**2.4.11 Focus Not Obscured (Minimum) — was missing, now specified.** Focus ring: 3 px `var(--focus)` outline, 2 px offset (`:focus-visible`, `.focus-ring` in `news.css`, same as the existing `.word.focus` / `.field-box.focus`). **Rule:** when an element gets keyboard/switch/screen-reader focus, scroll it fully clear of the sticky nav bar, chips row, tab bar, bottom action bar (`.nbar`), path banner, any open sheet and the software keyboard (scroll margin = bar height + 8 px; iOS `keyboardLayoutGuide`, Android `imePadding()` + `BringIntoViewRequester`). Non-modal sheets never cover the focused item; modal sheets trap focus.

**2.5.7 Dragging Movements — failures found, fixed:**
| Gesture | Single-pointer alternative |
|---|---|
| News: long-press a topic chip to mute | **New “Mute a topic” button** in News settings › Muted topics (`news-interests*`, re-rendered); long-press stays a shortcut |
| Match list swipe actions (§5.x Nearby) | Visible ⋯ overflow button on each row with the same actions (custom actions alone are not enough) |
| Nearby card swipe | Pass / Like buttons already present — pass |
| Offline queue long-press delete | Tap the queued bubble → menu with Delete |
| Any slider (if added) | Tappable track + −/+ steppers |
| Any reorder | Move up / Move down in the row menu |
| Swipe-back / sheet swipe-down | Back button / Close button always visible |

**Glass bars (iOS translucent tab bar / nav bar).** Mockups use opaque `--surface` bars, so measured contrast is the opaque one. **Rule:** if the iOS build uses system material/“Liquid Glass” bars, add a scrim under the blur of at least **85 % `--surface` (light) / 85 % `--surface` (dark)** or use `.systemThickMaterial`/opaque appearance, so that over the worst content behind (white photo in dark mode, black photo in light mode) labels and selected/unselected tab icons keep **≥ 4.5:1 (text) / ≥ 3:1 (icons)**. Verify with screenshots over a pure white and a pure black image in both themes; if it fails, fall back to an opaque bar. With Reduce Transparency on, bars are opaque.

## 5T. v0.1.22 — Communities with Live (PR #19; PROTOCOL §23.9 kinds 3/4) — requested by Jakob, 2026-10-29

All screens: 6 frames (EN/FR/HE-draft × light/dark). Names of people and communities are fictional. Files: `mockups/comm-*.html`, `mockups/live-*.html` (generator `omcp-v0122/live.py`).

### 5.120 Communities tab — `comm-list`, `comm-detail`
Live now sits at the top of Communities (§5.107 nav decision): one card per active live (host, title, community, Live badge with icon + text, **Listen**). Below: your communities with `community_sub` / `community_sub_live`. Detail shows active lives, **Go live**, the encryption note (`community_live_body`), members. Empty live state uses `community_no_live`.

### 5.121 Create, invite, accept — `comm-create`, `comm-invite`, `comm-invite-full`, `comm-invite-accept`
Founder-only invite (non-founders don't see the + button). Counter “N of 32 members” as text **and** progress bar (aria-valuenow/max=32). At 32: warn callout, Invite buttons `aria-disabled` (still focusable so the reason is read out), bar turns danger. Invitee sees community, inviter, member count, Join / Not now.

### 5.122 Remove from live (host) — `live-host-members`, `live-remove-confirm`, `live-removed-toast`
Each listener row has a visible **Remove from live** button (44 pt / 48 dp) plus a ⋯ menu with the same action — **no swipe/drag-only path** (WCAG 2.5.7). Confirmation sheet (danger + Cancel, focus starts on the sheet). After removal, a status toast confirms the rekey (“New key sent to N people”). Focus ring shown on the first Remove button.

### 5.123 Live taken down by the Hopla team — `live-takedown-host`, `live-takedown-viewer`
Triggered by a signed §23.9 `TAKEDOWN_LIVE` (kind 3). Host sees reason + **Appeal** (→ `mod-appeal`); viewer sees `live_taken_down` only (no reason, no appeal). Shield icon + text, never colour alone.

### 5.124 Live-start notification, default (non-Private) mode — `live-notif-shade`, `live-notif-lock`
Unlocked shade: “{host} is live in {community}” + title, actions **Listen** / **Mute community**. Lock screen stays generic “Hopla · New message” (§5.36; PR `MeshNotifications`/`NotificationPolicy`). In Private mode the shade is generic too (unchanged §5.36).

### 5.125 Community taken down — `comm-removed`
Danger callout at top of the community (`community_removed` / `community_removed_body`) + “How moderation works”; Go live disabled; member list still readable.

### 5.126 Report this live — `live-report`
Shows the stream ID (16 bytes = 32 hex, grouped in 4s, monospace, `dir=ltr` also in HE, selectable) with a 48 dp **Copy** button; reason radio list; **Send report**. The team signs `TAKEDOWN_LIVE` with target `stream_id ‖ 0[16]` (`team_mod takedown-live`).

### 5.127 Copy table (EN / FR / HE draft)
| Key | EN | FR | HE (draft) |
|---|---|---|---|
| `communities` | Communities | Communautés | קהילות |
| `live_now` | Live now | En direct | בשידור חי |
| `your_comms` | Your communities | Vos communautés | הקהילות שלך |
| `new` | New community | Nouvelle communauté | קהילה חדשה |
| `members` | {n} members | {n} membres | {n} חברים |
| `members_live` | {n} members · {l} live now | {n} membres · {l} en direct | {n} חברים · {l} בשידור חי |
| `t1` | Rehearsal warm-up | Échauffement de répétition | חימום לחזרה |
| `t2` | Sunday ride check-in | Point avant la sortie du dimanche | עדכון לפני רכיבת יום ראשון |
| `join` | Join | Rejoindre | הצטרפות |
| `listen` | Listen | Écouter | האזנה |
| `start_live` | Go live | Lancer un direct | לשדר בחי |
| `no_live` | Nobody in this community is live right now. | Personne n’est en direct dans cette communauté. | אף אחד בקהילה לא משדר כרגע. |
| `live_body` | Every member can listen and gets a notification. The key comes from the community’s encrypted group, so phones in between can’t listen. | Tous les membres peuvent écouter et reçoivent une notification. La clé vient du groupe chiffré de la communauté : les téléphones intermédiaires ne peuvent pas écouter. | כל החברים יכולים להאזין ומקבלים התראה. המפתח מגיע מהקבוצה המוצפנת של הקהילה, כך שטלפונים בדרך לא יכולים להאזין. |
| `create_t` | Create a community | Créer une communauté | יצירת קהילה |
| `create_d` | Up to 32 people you invite from your contacts. Everything stays end-to-end encrypted. | Jusqu’à 32 personnes invitées parmi vos contacts. Tout reste chiffré de bout en bout. | עד 32 אנשים שתזמינו מאנשי הקשר. הכול נשאר מוצפן מקצה לקצה. |
| `name` | Name | Nom | שם |
| `create` | Create | Créer | יצירה |
| `invite_h` | Invite a contact | Inviter un contact | הזמנת איש קשר |
| `invite` | Invite | Inviter | הזמנה |
| `invited` | Invited | Invité | הוזמן |
| `counter` | {n} of 32 members | {n} membres sur 32 | {n} מתוך 32 חברים |
| `founder` | Only you, the founder, can invite. | Seul·e le fondateur ou la fondatrice peut inviter. | רק את/ה, המייסד/ת, יכול/ה להזמין. |
| `full_t` | This community is full | Cette communauté est complète | הקהילה מלאה |
| `full_d` | 32 members is the limit. Someone has to leave before you can invite anyone else. | La limite est de 32 membres. Quelqu’un doit partir avant que vous puissiez inviter. | המגבלה היא 32 חברים. מישהו צריך לעזוב לפני שאפשר להזמין עוד. |
| `inv_t` | Join {c}? | Rejoindre {c} ? | להצטרף ל{c}? |
| `inv_from` | Invited by {h} | Invitation de {h} | הזמנה מ{h} |
| `accept` | Join | Rejoindre | הצטרפות |
| `decline` | Not now | Plus tard | לא עכשיו |
| `inv_note` | Members see your name and can listen to lives here. You can leave at any time. | Les membres voient votre nom et peuvent écouter les directs ici. Vous pouvez partir à tout moment. | החברים רואים את שמך ויכולים להאזין לשידורים כאן. אפשר לעזוב בכל עת. |
| `host_t` | Your live | Votre direct | השידור שלך |
| `viewers` | Listening ({n}) | À l’écoute ({n}) | מאזינים ({n}) |
| `you_l` | You | Vous | את/ה |
| `host` | Host | Hôte | מארח/ת |
| `remove` | Remove from live | Retirer du direct | הסרה מהשידור |
| `more` | More options for {h} | Plus d’options pour {h} | אפשרויות נוספות עבור {h} |
| `host_note` | Tap Remove from live next to a member to stop them listening. The key changes for everyone else. | Touchez Retirer du direct à côté d’un membre pour l’empêcher d’écouter. La clé change pour tous les autres. | הקישו על הסרה מהשידור ליד חבר כדי להפסיק את ההאזנה שלו. המפתח מתחלף לכל השאר. |
| `rm_t` | Remove {h} from this live? | Retirer {h} de ce direct ? | להסיר את {h} מהשידור? |
| `rm_d` | They stop hearing this live straight away. They stay in the community and can join the next one. | Cette personne n’entend plus le direct immédiatement. Elle reste dans la communauté et peut rejoindre le prochain. | ההאזנה תיפסק מיד. הם נשארים בקהילה ויכולים להצטרף לשידור הבא. |
| `cancel` | Cancel | Annuler | ביטול |
| `rm_done` | {h} removed. New key sent to 3 people. | {h} retiré·e. Nouvelle clé envoyée à 3 personnes. | {h} הוסר/ה. מפתח חדש נשלח ל־3 אנשים. |
| `end` | End live | Arrêter le direct | סיום השידור |
| `td_t` | Live taken down | Direct retiré | השידור הוסר |
| `td_host` | The Hopla team took your live down. It stopped on every phone. Your community is not affected. | L’équipe Hopla a retiré votre direct. Il s’est arrêté sur tous les téléphones. Votre communauté n’est pas concernée. | צוות Hopla הסיר את השידור שלך. הוא נעצר בכל הטלפונים. הקהילה שלך לא מושפעת. |
| `td_view` | The Hopla team took this live down. It stopped on this phone. | L’équipe Hopla a retiré ce direct. Il s’est arrêté sur ce téléphone. | צוות Hopla הסיר את השידור הזה. הוא נעצר בטלפון הזה. |
| `why` | Reason: {r} | Motif : {r} | סיבה: {r} |
| `reason` | Harassment | Harcèlement | הטרדה |
| `appeal` | Appeal | Contester | ערעור |
| `ok` | OK | OK | אישור |
| `learn` | How moderation works | Comment fonctionne la modération | איך עובד הפיקוח |
| `n_app` | Hopla | Hopla | Hopla |
| `n_now` | now | maintenant | עכשיו |
| `n_title` | {h} is live in {c} | {h} est en direct dans {c} | {h} בשידור חי ב{c} |
| `n_body` | {t} · Tap to listen | {t} · Touchez pour écouter | {t} · הקישו להאזנה |
| `n_lock` | New message | Nouveau message | הודעה חדשה |
| `n_act` | Listen | Écouter | האזנה |
| `n_mute` | Mute community | Couper la communauté | השתקת הקהילה |
| `rem_t` | Removed by the Hopla team | Retirée par l’équipe Hopla | הוסרה על ידי צוות Hopla |
| `rem_d` | The Hopla team removed this community. Its lives can’t be started or watched, and its posts aren’t passed on. | L’équipe Hopla a retiré cette communauté. Ses directs ne peuvent plus être lancés ni regardés, et ses publications ne sont plus relayées. | צוות Hopla הסיר את הקהילה הזו. אי אפשר להתחיל או לצפות בשידורים שלה, והפוסטים שלה לא מועברים. |
| `rep_t` | Report this live | Signaler ce direct | דיווח על השידור |
| `rep_d` | Your report goes to the Hopla team. They need the stream ID below to sign a takedown. | Votre signalement va à l’équipe Hopla. Elle a besoin de l’identifiant ci-dessous pour signer un retrait. | הדיווח נשלח לצוות Hopla. הם צריכים את מזהה השידור שלמטה כדי לחתום על הסרה. |
| `sid` | Stream ID | Identifiant du direct | מזהה השידור |
| `copy` | Copy stream ID | Copier l’identifiant | העתקת המזהה |
| `copied` | Copied | Copié | הועתק |
| `rep_why` | What’s wrong? | Quel est le problème ? | מה הבעיה? |
| `r1` | Harassment or hate | Harcèlement ou haine | הטרדה או שנאה |
| `r2` | Violence or threats | Violence ou menaces | אלימות או איומים |
| `r3` | Sexual content | Contenu sexuel | תוכן מיני |
| `r4` | Spam or scam | Spam ou arnaque | ספאם או הונאה |
| `r5` | Something else | Autre chose | משהו אחר |
| `send` | Send report | Envoyer | שליחת דיווח |
| `rep_note` | The host won’t know who reported. Reporting does not stop the live by itself. | L’hôte ne saura pas qui a signalé. Signaler n’arrête pas le direct à lui seul. | המארח/ת לא יידע/תדע מי דיווח. הדיווח לבדו לא עוצר את השידור. |

**Accessibility:** AA contrast 0 failures (light + dark, `a11y/dark-contrast.py` on the 15 new files); targets ≥ 44 pt / 48 dp; `:focus-visible` ring (news.css); no drag-only actions.

## 6. Path indicator component (normative UX)

**`PathChip`:** pill, 7 px dot + label, colours from §2.1. Used in list rows (micro), status strip, settings legend.

**`PathBanner`:** full-width bar under chat nav; one line; updates live as routing changes; VoiceOver: “Delivery path: mesh, 2 hops, end-to-end encrypted.”

Do not animate path changes as errors; cross-fade colour/label 200 ms.

---

## 7. Copy principles (EN / FR) — PROTOCOL §13.5

- Plain language for security states.
- Prefer verbs users understand: verify, wait, send via neighbour’s Internet, block.
- Concrete crypto only when useful: “End-to-end encrypted”, “Post-quantum hybrid when both sides support it”.
- **Never:** military grade, unhackable, completely anonymous, guaranteed delivery worldwide.

| Concept | EN | FR | HE (draft) |
|---------|----|----|---|
| Verified contact | Verified contact | Contact vérifié | איש קשר מאומת |
| Unverified | Not verified yet | Pas encore vérifié | עדיין לא מאומת |
| Waiting for mesh | Waiting for mesh | En attente du mesh | ממתין ל-Mesh |
| Via neighbour Internet | Sent via neighbour’s Internet | Envoyé via l’Internet d’un voisin | נשלח דרך האינטרנט של שכן |
| Gateway help | Help nearby peers reach the Internet | Aider les pairs proches à rejoindre Internet | לעזור לעמיתים בסביבה להגיע לאינטרנט |
| Notification default | New Hopla message | Nouveau message Hopla | הודעה חדשה ב-Hopla |
| Not an OMCP code | Not a Hopla contact code | Ce n’est pas un code contact Hopla | זה לא קוד איש קשר של Hopla |
| Safety mismatch | Safety numbers don’t match | Les numéros de sécurité ne correspondent pas | מספרי הבטיחות לא תואמים |
| File too large | {x} MB is over the 50 MB limit | {x} Mo dépasse la limite de 50 Mo | {x} מ״ב חורג מהמגבלה של 50 מ״ב |
| Transfer paused | Paused · out of range | En pause · hors de portée | מושהה · מחוץ לטווח |
| BLE-only live | Video needs Wi‑Fi or Internet | La vidéo nécessite le Wi‑Fi ou Internet | וידאו דורש Wi‑Fi או אינטרנט |
| Recovery | Write these 24 words down… | Notez ces 24 mots… | רשמו את 24 המילים האלה… |

Keep FR translations reviewed by a human before store submission; table is design intent, not final L10n freeze.

---

## 8. Platform notes

### 8.1 Android 12+

- **Foreground service** for mesh participation with a **persistent notification** explaining BLE use (user-visible, not dismissible while mesh is active). Suggested text: “Hopla is using Bluetooth so you can chat nearby.” Action: Open / Pause mesh.
- Permissions: `BLUETOOTH_SCAN`, `CONNECT`, `ADVERTISE`; location only as required by OS version — justify in-app before system prompt.
- Keystore + StrongBox when available for identity keys.
- WorkManager for deferred gateway fetch when Internet returns.
- Notification content: generic title unless user opts into previews.

### 8.2 iOS 17+

- Background BLE central/peripheral as allowed; App Store justification documented.
- **Tell users clearly** when background mesh is limited (banner on Chats: “iOS may pause mesh in background — open the app to sync”).
- Secure Enclave key wrapping; Keychain accessibility reviewed for background needs.
- MultipeerConnectivity research deferred to v0.2 additional transport; BLE remains mandatory for Android interop.
- Local Network permission for lan-relay livestreams.

---

## 9. Component inventory (Proggy mapping)

| Component | Responsibility | Android sketch | SwiftUI sketch |
|-----------|----------------|----------------|----------------|
| `OmcpTheme` | Tokens light/dark | Compose `MaterialTheme` wrapper | `Environment` + Asset colors |
| `PathChip` | Path pill | Composable | `View` |
| `PathBanner` | Thread path bar | Composable | `View` |
| `ChatRow` | List row + unread + path | LazyColumn item | `List` row |
| `AvatarView` | User / group | — | — |
| `VerifiedBadge` | Filled mint seal + check (§5.32) | Icon / IconButton in header | Image / Button in header |
| `VerifyGhostSeal` | Dashed muted seal, header only (§5.33) | IconButton | Button |
| `VerifyHintRow` | Slim dismissible hint + Verify (§5.33) | Row | HStack |
| `SafetyNumberBlock` | 12×5 digit grid | — | — |
| `MessageBubble` | Text + meta | — | — |
| `FileTransferCard` | Sending / receiving / paused / queued / failed / done | — | — |
| `ComposerBar` | Attach + field + send | — | — |
| `GatewayDisclosure` | First-enable sheet | Dialog / ModalBottomSheet | sheet |
| `ToggleSettingRow` | Title, desc, switch | — | — |
| `Callout` | Info / warn banners | — | — |
| `EmptyState` | Icon + title + body + CTA | — | — |
| `PrimaryButton` / `DangerButton` | 48 pt height | — | — |
| `TabRoot` | 4 tabs | NavigationBar | TabView |
| `RecoveryPhraseGrid` | 24 words once | `LazyVerticalGrid` + `FLAG_SECURE` | `LazyVGrid` + snapshot blur |
| `AppLogo` | Placeholder slot until hopla-v2 | — | — |
| `OnboardingStepper` | 5-step progress | — | — |
| `PhraseWordField` | Word entry + autocomplete chips | `TextField` + chip row | `TextField` + chips |
| `PermissionCard` | Pre-prompt explainer | — | — |
| `BlockingExplainer` | BLE denied / camera denied | — | — |
| `ContactAddOption` | Chooser row | — | — |
| `MyQrCard` | Own QR + PeerID short | ZXing encoder | CoreImage `CIQRCodeGenerator` |
| `ContactPreviewSheet` | Scan result + safety number + mismatch | `ModalBottomSheet` | `.sheet` |
| `FileSendConfirmSheet` | Size/type/route/limit | `ModalBottomSheet` | `.sheet` |
| `QrScannerScreen` | Contact add | CameraX | AVFoundation |
| `LiveStreamCard` | v0.2 beacon / live | — | — |
| `RoleChip` | Community role | — | — |
| `AdminToolsList` | v0.2 sketch | — | — |
| `BackupMethodOption` (v0.1.2) | Multi-select backup method row, Recommended badge | `Row` + `Checkbox`, `toggleable()` | `Toggle` with `.toggleStyle(.checkbox-like)` row |
| `BackupPasswordSheet` (v0.1.2) | Password + confirm, zxcvbn meter, iCloud no-password option | `ModalBottomSheet` | `.sheet` with `.presentationDetents([.large])` |
| `BackupStatusCard` (v0.1.2) | Status badge, last backup, Back up now | `Card` | `GroupBox` |
| `BackupReminderBanner` (v0.1.2) | 24 h countdown banner + blocking sheet | `Surface` under `TopAppBar` | `safeAreaInset(edge: .top)` |

Shared Rust core exposes state (path, verification, queue depth); UI never re-implements crypto decisions.

---

## 10. Accessibility checklist

- [ ] All path chips have accessibility labels including path name + meaning.
- [x] Contrast ≥ 4.5:1 for text; ≥ 3:1 for large/UI graphics — both themes audited (`a11y/light-contrast.md`, `a11y/dark-contrast.md`, v0.1.18).
- [ ] Focus order: nav → path banner → messages → composer.
- [ ] Reduce Motion: disable non-essential transitions.
- [x] RTL-ready layout (logical margins). v0.1.18 adds Hebrew (he) RTL frames (v0.1.19 News frames too) — rules in §5.106; Hebrew copy is a DRAFT pending native review.
- [ ] Safety number readable at 200 % zoom without horizontal clip.

---

## 11. Out of scope (do not design into MVP)

Desktop clients, Tor transport, voice/video calls (non-livestream), public global directory, payments, stickers/GIF keyboards, “read receipts” as a social flex (delivery states are OK and already specified).

---

## 12. Open product questions

Resolved in v0.1.2: v0.1.1 Q1 (skip phrase confirm) — superseded by REQUIREMENTS: no skip on the quiz, but “Remind me later” gives 24 h. v0.1.1 Q2 (unverified chat) and Q3 (50 MB unit) remain open, parked behind:

1. **24 h lock scope:** after the deadline (§5.29), should incoming messages still be received and readable behind the blocking sheet, or is the whole app paused? (Mockup assumes receive continues, UI blocked.)
2. **Pre-selected cloud default vs explicit opt-in:** REQUIREMENTS says “Android default / iOS default” *and* “no cloud backup without explicit opt-in”. Mockups pre-select and badge it Recommended, but nothing uploads until Continue + password. Is a pre-checked box acceptable, or must it start unchecked?
3. **Wrong-password limits:** O4 asks for rate-limited retries; mockups assume 3 tries then 5-minute cooldown (escalating). Confirm numbers with the Rust core.
Resolved in v0.1.5 by the maintainer (REQUIREMENTS § Decisions): Q4 calls → “Hopla · Incoming call”; Q5 per-chat opt-out allowed; Q6 Android lock → “Hopla · New message”; Q7 Nearby must be a pleasure on iOS → sessions + Live Activity + area mode (§5.58); Q8 no screenshots anywhere (§5.57).

Resolved 2026-10-24 (Jakob approved; REQUIREMENTS Decision 5 / SS1 updated by Writey):
- ~~Q4 Live Activity on Private notifications~~ → counts hidden (§5.73).
- ~~Q5 Android recents~~ → black when Block screenshots is on, normal preview when off (§5.72).
- ~~Q6 Session length~~ → superseded 2026-10-24: no time limit; auto-pause + 4 h check-in (§5.75).
- ~~Q7 Block-screenshots toggle vs REQUIREMENTS~~ → REQUIREMENTS now has the toggle, on by default; burn/view-once always protected (§5.71).
- ~~Q8 Restricted text~~ → follow §23 (§5.63).
- ~~Q9 Under review visibility~~ → admins only.
- ~~Q10 Reputation display~~ → four word levels.

Resolved 2026-10-24:
- ~~Q11 SS2 wording vs toggle Off~~ → fixed in REQUIREMENTS (Writey, commit 6a0ee67).
- ~~Q12 Area mode and D6~~ → indicator shows in area mode too (§5.81).
- ~~Q13 Check-in while paused~~ → counts visible time only; paused time doesn’t count (§5.82).

No open questions. (Note: REQUIREMENTS Decision 4 “Honest status” still quotes an old chip example with minutes left; the pack uses “Live nearby” / “Paused” / “Area mode”.)

Open (v0.1.10):
14. ~~**Message history in backup**~~ → **Resolved v0.1.11:** all message text is backed up (REQUIREMENTS “Encrypted chat backup”, v0.1.4 text only); media comes back via missing-media placeholders (§5M).
15. **Where contacts/settings live:** the blob format `{v,kdf,salt,nonce,ct,created_at,device_hint}` carries the phrase; O3 requires contacts + verification restored. Is a second encrypted contacts record uploaded alongside (size shown as 4 KB assumes identity + contacts)? *(v0.1.14: O3’s verification state is your verifications of others; contacts' verification of you resets, RS2.)*
16. **Lockout escalation:** 5 → 15 → 60 min is a design assumption (O4 says only “rate-limited”). Confirm with the Rust core.
17. ~~**Partner no longer has the file:**~~ → **Resolved 2026-10-24** (§5.94) REQUIREMENTS only covers re-fetch from a partner who still holds it. `media-missing--gone` assumes the protocol gets a “not found” reply for the hash. Is there such a reply, or does the placeholder wait forever (and for how long)?
18. ~~**Requester privacy:**~~ → **Resolved 2026-10-24** (§5.94; superseded by the restore indicator §5.95: the restore is announced, and re-requests carry no extra metadata) does an auto re-request reveal to the partner that I restored (and when)? Copy assumes it's fine.
19. ~~**Chunk resume / Wi-Fi:**~~ → **Resolved 2026-10-24** (§5.94) should media re-fetch over a gateway respect the backup's Wi-Fi-only default, or always use mesh/gateway freely?
20. ~~**Safety number after a restore**~~ → **Resolved 2026-10-24 (Jakob: yes)**: the safety number covers the identity key + current device key, a restore resets every contact's verified badge, and re-verification is in person by QR (§5.98, RS2).
21. ~~**Default types when media backup is turned on:**~~ → **Resolved 2026-10-24:** every type is pre-selected except Videos, as mocked.  the mockup pre-selects everything except Videos (size). Confirm, or select all?
22. ~~**Media backup cadence:**~~ → **Resolved 2026-10-24:** media is backed up in a separate daily pass, Wi-Fi only by default. Copy: “Backed up daily on Wi-Fi” (§5.96).  the chat backup runs at most hourly, padded to 64 KiB buckets. Should media follow the same cadence and padding (padding large videos gets costly), or use a separate daily pass?
23. ~~**Partial restore:**~~ → **Resolved 2026-10-24:** a partial media restore falls back to placeholders that refill from nearby contacts (`restore-media--partial`, §5.97).  if media download fails mid-restore (quota or offline), do the remaining items fall back to placeholders and contact re-fetch automatically? Assumed yes.
24. ~~**REQUIREMENTS O3 wording:**~~ → **Resolved 2026-10-24:** REQUIREMENTS O3 is reworded (“your verifications of contacts are restored; contacts' verification of you resets, RS2”).
25. ~~**Group / community chats:**~~ → **Resolved 2026-10-24:** compact tag in groups and communities, with the system notice in 1:1 only (§5.99).

Hopla News questions (v0.1.19) — closed in v0.1.20 by Jakob’s decisions and the revised REQUIREMENTS (2026-10-24):
- ~~Q26 tab bar~~ → News tab; Live at the top of Communities (§5.107).
- ~~Q27 alert notification default~~ → on by default (§5.111).
- ~~Q28 unsigned/invalid states~~, ~~Q29 verified vs signed~~ → obsolete: no per-story signature checks or trust levels (§5.109).
- ~~Q30 official keys~~ → obsolete: alerts are Emergencies-topic stories from Google News (§5.111).
- ~~Q31 city~~ → typed, or from location with the Nearby permission; only the city name is used (§5.115).
- ~~Q32 storage cap~~ → ≤ 20 MB, row in News settings (range still configurable per REQUIREMENTS).
- ~~Q33 reporting stories~~ → obsolete: no story reports; discussion moderation in communities follows §23.
- ~~Q34 citizen reports~~ → none; users can’t publish into News (§5.114).
- ~~Q35 Read more online~~ → link opens the browser with an inline “leaves Hopla” note, no confirm.
- Q36 Relay settings → still a placeholder until the relay questions are answered.
Open (v0.1.20):
37. **REQUIREMENTS still lists the X API** as a second source (and its account/tier as an open decision); the design follows Jakob’s correction (Google News RSS only). REQUIREMENTS should be updated to match.
38. **Mute a topic:** mocked as long-press on a chip + a Muted topics row; confirm, or add an explicit Mute in the topic’s overflow menu.
39. **City from location:** reverse-geocoding to a city name needs a geocoder; on-device dataset vs OS geocoder (Apple/Google, which sends coordinates) — the copy says the location is never sent, so this must be on-device or the copy changes.

---

## Document control

- v0.1.22 (2026-10-29): §5T Communities with Live (PR #19).

| Version | Date | Notes |
|---------|------|-------|
| 0.1.0 | 2026-10-23 | First design pack: tokens, IA, MVP + v0.2 sketches, 6 HTML mockups |
| 0.1.1 | 2026-10-24 | Hopla naming; §5.14–5.24 onboarding, contact add, file transfer; 11 new mockups (17 PNG frames); resolved v0.1.0 questions |
| 0.1.2 | 2026-10-24 | 4-screen onboarding + recovery backup (§5.1, §5.14 updated; §5.25–5.31 new); §5.15/5.16/5.18 deprecated; 8 new + 1 updated mockups (24 PNG frames); new §12 questions |
| 0.1.3 | 2026-10-24 | Verified badge §5C |
| 0.1.15 | 2026-10-24 | Restored tag in groups and communities (§5.99), `group-restored`; Q24–Q25 resolved; mobile-data toggle confirmed by REQUIREMENTS |
| 0.1.16 | 2026-10-24 | One-tap Ask to resend with optional note (§5.100, supersedes §5.94 confirm sheet); Settings › Storage › Keep media you sent, default 90 days or 2 GB (§5.101); 2 new + 1 updated mockups |
| 0.1.17 | 2026-10-24 | Dark-mode WCAG AA overhaul (dark token values, switch thumb, on-accent ink) §5.102; backup copy says cloud backup holds the phrase and all chats, written phrase restores identity and contacts only §5.103 |
| 0.1.18 | 2026-10-24 | Light-mode WCAG AA (light token values matched to the dark steps, semantic disabled states, inverse-toast accent) §5.104; Hebrew (he, RTL) frames and HE copy column — DRAFT pending native review §5.105; RTL mirroring rules §5.106 |
| 0.1.19 | 2026-10-24 | Hopla News (v0.4): News tab, signature badges, reader, emergency alerts, sources/curators, interests, share + discussion threads, catch-up, empty/loading §5R (5.107–5.119); 19 mockups, 114 PNGs; Q26–Q36 |
| 0.1.20 | 2026-10-24 | Hopla News revised to REQUIREMENTS 2026-10-24: Google News RSS as the only source with “via contact” (X later, separate project), no signatures/curators/trust, alerts on by default, city by location or typing, share to chats and communities, no publishing; removed news-trust, news-sources, news-reader-summary; Q26–Q35 closed, Q37–Q39 |
| 0.1.14 | 2026-10-24 | Re-verification after restore (§5.98, RS2): verified badge reset, “Restored, not verified” until QR re-verify, badge earned back; restore copy; Q20 resolved; Q24–Q25; media daily Wi-Fi pass copy, partial media restore frame; Q21–Q23 resolved |
| 0.1.13 | 2026-10-24 | Media backup setting (§5N, 5.96–5.97): toggles, size, quota, turn-off delete, first upload; `restore-media`; placeholders only for media not in the backup; Q21–Q23 |
| 0.1.12 | 2026-10-24 | Ask to resend + confirm/sent, Wi-Fi-only gateway re-fetch with a mobile-data setting, re-requests carry no extra metadata (§5.94); Q17–Q19 resolved; restore indicator: contact notice, sheet, 7-day Restored badge, restored-phone banner (§5.95) |
| 0.1.11 | 2026-10-24 | Text chat backup: missing-media placeholders §5M (5.91–5.93), `media-missing`, `chats-restored`; restore copy now says chats come back; Q14 resolved; Q17–Q19 |
| 0.1.10 | 2026-10-24 | Restore from backup, reinstall-first §5.83–5.90; 6 new mockups (52 PNG frames); §12 Q14–Q16 |
| 0.1.9 | 2026-10-24 | Area-mode persistent indicator §5.81; check-in counts visible time §5.82; Q11–Q13 resolved |
| 0.1.8 | 2026-10-24 | Nearby stays on until turned off: auto-pause, 4 h check-in, persistent indicator §5.75–5.80; `nearby-checkin`, `settings-nearby` new |
| 0.1.7 | 2026-10-24 | Approved decisions §5.71–5.74: toggle copy aligned to REQUIREMENTS, private Live Activity/Island, recents by toggle, session length; §12 Q4–Q10 resolved |
| 0.1.6 | 2026-10-24 | Moderation & reputation (§23, v0.2) §5.61–5.69; Block screenshots toggle §5.70; 7 new + 1 updated mockups; §12 Q7–Q10 |
| 0.1.5 | 2026-10-24 | the maintainer decisions: §5.56–5.60 (calls, per-chat opt-out, Android lock, no screenshots, Nearby session/area/radar/skeleton/motion); 6 new mockups; screenshot notices removed |
| 0.1.4 | 2026-10-24 | Notifications §5.36–5.40; Hopla Nearby dating (v0.3) §5.41–5.49; Burn after read §5.50–5.55; 14 new mockups (96 PNG frames); §12 Q4–Q8 |
