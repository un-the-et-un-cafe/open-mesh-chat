# Community discovery (v0.1.41, PROTOCOL §20.8, TM25–TM31, TM36)

## v0.1.41 — approval-only joins and icons deferred past v1.0

- **Send:** `announce()` always sets `join_policy = 1` and `icon_hash = None` (`has_icon = 0`). There is no approval-only toggle on the founder listing sheet.
- **Receive:** an announcement with `join_policy` other than 1, or with `has_icon = 1`, is stored, relayed and shown. `discovery_join` refuses a non-open listing (no pending Join, no control sent). Discover cards show the non-interactive `u_comm_join_invite` line under the meta (no Join button; Hide and Report stay). `icon_hash` is ignored; nothing is fetched; the 5 MB icon cache stays unused.
- **Default icon (TM36, §5.156):** `CommunityIcon` letter and colour rules as in DESIGN_SPEC §5Y (grapheme skip, `Locale.ROOT` uppercase, SHA-256 colour index, 40/32/56 sizes). The Community type badge stays a hash glyph. Private groups are unchanged.
- **Still deferred / skipped:** the request/approve protocol and any icon set/fetch/verify protocol stay past v1.0. §23.4 Reported *threshold* (score, T, R, counter-votes) still has no v1.0 wire format — §23.3 (2026-11-09) parks gossiped `REPORT{}`, attestations, endorsements and counter-votes past v1.0; reports stay evidence files. The local hide hook for an already-Reported `community_ref` remains (`ctx.reported`, MR61).


## v0.1.40 — language filter and battery-saver pause

- **Language filter:** `DiscoveryIndex::search` already ranked/filtered by announced `langs`; the FFI and Discover UI now pass a selected BCP 47 tag, and the founder can set up to 3 languages on a listing (local only, MR55).
- **Battery saver:** `RelayEngine::set_battery_saver` pauses discovery-tag digest ranges (the discovery exchange of §20.8(5)(b)) while the OS saver is on. Apps report Android battery saver / iOS Low Power Mode on each mesh tick.
- **Still deferred / skipped:** §23.4 Reported *threshold* (score, T, R, counter-votes) has no v1.0 wire format — §23.3 (2026-11-09) parks gossiped `REPORT{}`, attestations, endorsements and counter-votes past v1.0; reports stay evidence files. The local hide hook for an already-Reported `community_ref` remains (`ctx.reported`, MR61). Community `icon_hash` fetch and approval-only join protocol were still skipped in this release; v0.1.41 records the v1.0 deferral (MR76, TM36).


## v0.1.37 — digest pacing wired

- Discovery-tag ranges are included in digests at most once per neighbour per 6 hours unless the local discovery index bumped (`DiscoveryPacing` in `refresh_digest`).

## v0.1.36 — expiry and discovery-tag pacing

- §20.8(4)(d): listings expire 14 days from the newest valid announcement's signed `issued_ms`; hearing the same `seq` again does **not** extend life (test `mr_discovery_same_seq_does_not_extend_expiry`).
- §20.8(4)(a): discovery-tag ranges to each neighbour at most once per 6 h unless the local index changed (`DiscoveryPacing`, per-neighbour timestamp + generation).
- Announcements sign with a distinct `community_sk` when present (owner handover); roster check matches `owner_pk` only.

- **Listed is off by default** (`LISTED_DEFAULT = false`); only the founder can turn it on, after the TM27 notice (`comm-listed-notice`, focus on "Keep unlisted"). Turning it off sends a signed delist. Unlisted communities are never announced (MR53).
- **Wire:** `CommunityAnnouncementV1` (type `0x01`) and `CommunityDelist` (`0x02`), signed by `community_pk` then `owner_pk`, in public envelopes on the discovery tag, flags `RELAY_OK | STORE_OK`, header TLV `0x5D` (version, community_ref, seq). In v1.0 both keys are the founder's identity key (the key that signs the community genesis/roster).
- **Limits:** payload ≤ 1,024 B; description ≤ 140 code points (MR54); ttl ≤ 6 hops (own listings start at 5, so they reach 6 hops; MR56); expiry 14 d after `issued_ms`, refresh every 7 d (MR57); index ≤ 2 MB / 5,000 entries in its own relay class, outside the relay cap and community history (MR58); ≤ 200 discovery envelopes forwarded per hour; one new seq per ref per 24 h; ≤ 3 listings per owner/community key (MR59).
- **Moderation (MR60–62):** team takedown / any community tombstone, team MEMBER_REMOVE of the owner, or an owner that doesn't match the known roster: not stored, not relayed, not shown. Joining from discovery checks the genesis signer against the listing's keys and distrusts the ref on mismatch. "Report community" gives only the signed announcement bytes (reasons 1–7, 9, 10).
- **Search is local** (MR55): `DiscoveryIndex::search` ranks by text match, then "Near you", then hop count, then recency; size only breaks ties; no badges (MR61).
- **Near you** (MR63): heard directly over the local radio (not a gateway link) within 2 hops in the last 7 days; computed and kept on this phone only.
- **UI:** Discover at the top of Communities (search, All + 8 category chips, Nearby filter, "Near you first", owner-estimate note, private-groups note); cards say "Members and size shown after you join"; comm-detail header shows the size range; founder alert banner on comm-detail.
- **Later tightened in v0.1.41:** community `icon_hash` is still absent on send; approval-only listings are shown but never joined from discovery. §23.4 Reported score/T/R (v1.0 must not gossip reports, §23.3); local Hide still hides, and `ctx.reported` still hides a ref if set.
