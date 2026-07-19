# nyora-aidoku

Real Aidoku extensions that parse **on-device**.

This replaces the previous contents of this repo — 390 `.aix` proxy shims whose
WASM contained nothing but `https://api.hasanraza.tech` and forwarded every call
to the helper. Nothing here talks to a server.

No server is involved at any point: the WASM module talks directly to the manga
site. Cloudflare interstitials are solved by Aidoku's own host-side WebView
handler (`CloudflareHandler`), which is applied automatically to every extension
request — so the device relay isn't needed either.

## Building

Needs only the Rust toolchain — no external CLI:

```sh
rustup target add wasm32-unknown-unknown     # one time
python3 tools/generate.py                    # data/ -> one crate per source
./tools/package-all.sh                       # build all + assemble public/
```

`package-all.sh` compiles every source in one parallel `cargo build`, zips each
into a `.aix`, and `tools/build-list.py` writes `public/index.min.json`. The
Aidoku SDK crate is fetched from git (declared in each `Cargo.toml`); nothing
else is required.

`public/` is the installable source list — add its `index.min.json` URL in
Aidoku under Browse -> Source Lists.

## Why this is tractable

The 1346 kotatsu sources are overwhelmingly *template instances*, not bespoke
code. `madara` alone is 553 of them, and a concrete source is typically nothing
but a constructor:

```
class YaoiScan(context) : MadaraParser(context, YAOISCAN, "yaoiscan.com", 20)
```

`nyora-data-driven` already extracted that into JSON (551 Madara rows, averaging
1.3 config keys each). This repo turns those rows into Aidoku sources: one
hand-written Rust template + generated per-source config.

```
data/                   vendored source definitions (34 engines, 1071 rows)
templates/common/       date parsing + image-src ladder, shared by all engines
templates/madara/       553 kotatsu sources (218 live)
templates/mangareader/  265 kotatsu sources (118 live)
templates/zeistmanga/   50 kotatsu sources  (31 live, Blogger feed API)
templates/hotcomics/    13 kotatsu sources  (TooMics; langPath composition)
templates/onemanga/     24 kotatsu sources  (single-series Elementor sites)
templates/mmrcms/       16 kotatsu sources  (dt/dd label pairs, dual browse)
tools/generate.py       data/*.json -> one crate per source
tools/icons.py          per-source favicons -> icons/
tools/liveness.py       engine-aware probe -> data-liveness.json
tools/package-all.sh    build all + assemble .aix + source list
sources/<id>/           generated; do not edit by hand
```

`generate.py` emits **907** crates — the full catalogue. `tools/liveness.py`
records what each source actually returns, and `--live-only` narrows the output
to the 388 that answered; it is a testing aid, not the shipping set.

That sweep probes **the engine's own entry point** — the Blogger feed for
zeistmanga, the browse path for the HTML engines — not the homepage. Probing `/`
is not enough: `klmanhua.com` is a parked domain and `nekoproject.org` has
migrated off Blogger, and both answer 200 at the root while being unusable. It
also fingerprints parking pages and, importantly, records Cloudflare
challenges as `cf` rather than dead — those 134 sources work in the app even
though curl can't reach them.

## Usage

```sh
python3 tools/generate.py                 # full catalogue (907)
python3 tools/generate.py --live-only     # only sources that answered (388)
python3 tools/generate.py --id MANGAREAD  # one
python3 tools/liveness.py                 # refresh data-liveness.json

cd sources/en.mangaread && cargo test --release   # live test against the site
```

The `.aix` packaging is done by `tools/package-all.sh` + `tools/build-list.py` (pure Rust + Python).

## Status

**madara** — verified end-to-end against the live site for `en.mangaread`: 20 search results
with covers, details (title/authors/6 tags/description/status), 3864 chapters
with numbering and parsed dates, and 13 page URLs — all parsed on-device.

Across a 14-source sample: 5 passed, 2 unreachable, 7 failed. **Every failure
was traced to a dead upstream site, not the template**:

- `factmanga.com` — parked domain, for sale
- `bookmanga.com`, `darkscans.com` — domains squatted, now gambling sites
- `aryascans` — 301, domain moved
- `dragontea` — Cloudflare 403, which the *app* solves host-side but the test
  runner cannot (it has no `CloudflareHandler`)

Regression suite: 8/8 of the reachable madara sources pass.

**zeistmanga** — verified end-to-end against `yokai-team.blogspot.com`: entries
from the Blogger feed JSON API with `/w600/` cover rewriting, details, status
and tags, **120 chapters** with Arabic titles and parsed dates, and 17 pages.
11 of 12 sampled sources pass the search+chapters test — the best rate of the
three engines.

**mangareader** — verified end-to-end against `en-thunderscans.com`: 30 entries,
details (title/status/5 tags/description), 44 chapters with numbering and parsed
dates, and **18 pages extracted from the embedded `ts_reader.run({...})` JSON**,
which is how this family ships page lists rather than as `<img>` tags.

That last point matters when reading test results: **the Aidoku test runner is
weaker than the real app.** It also has no `:contains()` support and parses
dates with `chrono::NaiveDateTime`, which cannot match a date-only string. Both
are worked around in the template (status is matched by iterating rows with
plain CSS; dates retry with a zero time appended), but it means a red test is
not proof of a broken source.

The `broken` flags in `madara.json` are also stale — several rows marked live
are dead domains today. A re-verification pass against the live web would give a
truer denominator than the 314 currently generated.

## Implemented

Search, listings (Popular / Latest / New), sort orders, genre filtering and
discovery, details (title, cover, authors, artists, description, tags, status,
alt titles), chapters (with dedup, numbering and dates) and page lists.

Alt titles are folded into the description as an `Alternative:` line, because
Aidoku's `Manga` struct has no field for them and dropping them would lose the
native/romanised names people search by.

## Known gaps

- AES `chapter-protector` pages are not handled; affected chapters return no
  pages rather than failing loudly. Frequency is unknown — it's detected at
  runtime, not recorded in config.
- Author/artist search (`authorSearchSupported`) is parsed into config but not
  wired to a filter.
- Tag exclusion and year/status filters (the `tax_query`/`meta_query` branches
  of the ajax payload) are not implemented.
## Frozen

Six engines, 907 sources. Further engines (`wpcomics` 17, `galleryadults` 13,
`keyoapp` 13, `foolslide` 9, `pizzareader` 8) are scoped but not built.

## Source count

The generated catalogue is **907 sources**, matching how nyora-android and
nyora-aidoku ship: every known source is listed, and a dead one simply fails
rather than silently going missing.

| set | count | flag |
|---|---:|---|
| full catalogue | **907** | default |
| only sources that answered a live probe | **388** | `--live-only` |

`--live-only` is useful for testing (it skips ~500 sources that are dead,
parked or moved) but is not what you ship.

The Android app and `nyora-aidoku`'s `index.min.json` both list ~960 sources, so
parity is **907 of 960 (95%)** with six engines ported. Closing the last ~53
needs four more, all already scoped:

    wpcomics 17 · galleryadults 13 · keyoapp 13 · foolslide 9 · pizzareader 8

- Six of 34 engines are ported: madara, mangareader, zeistmanga, hotcomics,
  onemanga, mmrcms.
- zeistmanga's `encodedSrc` reader variant and `tagScrape` key/title modes are
  simplified rather than fully ported.
- **galleryadults is deliberately deferred.** Its reader resolves each page's
  image by fetching that page's HTML, which Aidoku's `get_page_list` cannot do
  lazily — so it is ~30 requests per chapter, or a URL heuristic. I probed
  asmhentai to see whether page URLs are inferable from page 1 and could not
  establish the reader URL shape (`/g/{id}/{n}/` 404s). 13 rows is not worth an
  uncertain, request-heavy design; revisit with a live reader URL in hand.
- The 31 engines left hold only ~90 live sources between them, and half of that
  is galleryadults (9), pizzareader (8), onemanga (6) and hotcomics (5).
  Diminishing returns — scoping for hotcomics and liliana is done if wanted.
- mangareader: `encodedSrc` (3 rows, base64 reader blob) and the 5 live
  `needsCustomLogic` rows are not handled.

## Cloudflare

**Nothing to integrate — the app already handles it for every source.**
`InterpreterConfiguration.defaultConfig(for:)` in the fork
(`Shared/Extensions/AidokuRunner.swift:61`) installs a `requestHandler` that
wraps *every* request the WASM interpreter makes. When a response carries
`Server: cloudflare`, a blocked status code and a challenge element,
`CloudflareHandler` opens a `WKWebView`, solves it, and returns the real bytes
to the extension. The extension never sees the challenge and needs no code for
it.

This is the single biggest advantage of the WASM path over the Kotlin
native-engine path, which has no interstitial solver on iOS at all — it is why
`NyoraDeviceRelay` exists, and why these extensions don't need it.

Note the AidokuRunner *package* has no Cloudflare handling of its own; the
interception lives in the app layer. If these extensions are ever loaded by a
host without that wiring, the ~138 Cloudflare-walled sources will fail.

## Cloudflare skews the test numbers

61% of shippable mangareader rows (90 of 148) sit behind Cloudflare, vs a much
smaller share for madara. The test runner has **no** `CloudflareHandler`, so
those all fail locally while working in the real app. A sample of 12 mangareader
sources gave 2 pass / 8 parse-fail — and every one of the 8 inspected was
Cloudflare-gated, three returning outright 403. Judge this family in the app,
not in `cargo test`.
