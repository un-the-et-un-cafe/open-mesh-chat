# Hopla News — contrast audit (v0.1.20)

Re-run of `a11y/dark-contrast.py` over **all** mockups, both themes (system python3 Playwright, /usr/bin/google-chrome). Text 4.5:1, large text 3:1, icons / UI / focus 3:1; exemptions = the v0.1.18 list (none in news-*).

| Theme | Pack measurements | Pack failures | News measurements | News failures | Worst news text | Worst news UI/icon |
|---|---|---|---|---|---|---|
| Light | 11772 | **0** | 2386 | **0** | 5.22:1 (text, #8a5a10 on #faf0d8, news-catchup--default) | 3.50:1 (ui, #73849a on #f3f5f8, news-interests-location--default) |
| Dark | 4500 | **0** | 2386 | **0** | 4.96:1 (text, #95a3b4 on #143836, news-share-bubble--dark) | 3.82:1 (ui, #687990 on #151d28, news-alert--dark) |

Thumbnails are decorative (`aria-hidden`, gradient fills) and carry no text. v0.1.20 also fixes the shared-card hero overflowing under the card text (`.nshare .hero` clipped).
