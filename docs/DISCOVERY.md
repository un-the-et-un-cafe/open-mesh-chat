# Community discovery (v0.1.33, PROTOCOL §20.8, TM25–TM31)

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
- **Deferred:** §22.2 set-reconciliation pacing (6 h per neighbour) uses the normal digest sync today; battery-saver pause; icons (`icon_hash` is always absent); approval-only joins (`join_policy` 2) and language filter UI; Reported state from §23.4 (local Hide/Report hides).
