# nyora-aidoku

On-device [Aidoku](https://aidoku.app) manga sources — real WASM extensions that
parse on the user's device, with **no helper server**.

This replaces the previous contents of this repo: a source list of proxy shims
whose WASM did no parsing and only forwarded every request to a hosted helper.
Here the extension talks directly to the manga site, so nothing depends on
Nyora-operated infrastructure. Cloudflare-protected sites work because the
Aidoku app solves the interstitial in a WebView — see [Cloudflare](#cloudflare).

## Install

Add this URL in Aidoku under **Browse → Source Lists**:

```
https://raw.githubusercontent.com/Nyora-Manga/nyora-aidoku/main/public/index.min.json
```

**914 sources** across en / pt / tr / es / id / fr and more.

## How it works

The 1000+ kotatsu manga parsers are overwhelmingly *template instances*, not
bespoke code — a Madara source, for example, is usually one constructor:
`class YaoiScan(context) : MadaraParser(context, YAOISCAN, "yaoiscan.com", 20)`.
So instead of porting every parser, one Rust template is written per theme
family and every source is expressed as data.

```
data/          per-engine source definitions (35 engines, 1077 rows)
templates/     one Rust crate per theme family — the parsing logic
tools/         generate · package · icons · liveness · refresh-domains
icons/         cached favicons
public/        the built, installable source list
```

`tools/generate.py` reads `data/*.json` and emits one small Rust crate per
source — identical apart from a compiled-in `CONFIG` block. Each crate builds to
its own `.wasm`.

## Engines

Eight of the 35 kotatsu engines are ported, covering **920 sources**:

| engine | sources | notes |
|---|---:|---|
| madara | 551 | WP-Manga theme; the largest family |
| mangareader | 259 | MangaThemesia; page list embedded in `ts_reader.run(…)` |
| onemanga | 24 | single-series Elementor sites |
| mmrcms | 16 | `dt`/`dd` label pairs, dual browse endpoints |
| hotcomics | 13 | TooMics; language baked into the path |
| zeistmanga | 50 | Blogger sites — browses the feed JSON API |
| mangafire | 6 | pure JSON API; one catalogue × 6 languages |
| asurascans | 1 | Astro SSR; page list inside a serialised island |

The remaining engines (`wpcomics`, `galleryadults`, `keyoapp`, `foolslide`,
`pizzareader`, …) are present in `data/` for when they are ported.

## Building

Needs only the Rust toolchain — no external CLI.

```sh
rustup target add wasm32-unknown-unknown     # one time
python3 tools/generate.py                    # data/ -> one crate per source
./tools/package-all.sh                       # build all + assemble public/
```

`package-all.sh` compiles every source in a single parallel `cargo build`
(~4 min on 8 cores), zips each into a `.aix`, and `tools/build-list.py` writes
`public/index.min.json`. The Aidoku SDK crate is fetched from git (declared in
each `Cargo.toml`); nothing else is required.

To test a single source against its live site:

```sh
cd sources/en.mangaread && cargo test --release
```

## Keeping the catalogue current

Manga sites move and die constantly, and a stale domain is indistinguishable
from a dead source — the extension installs and returns nothing. Two tools keep
`data/` honest:

```sh
python3 tools/liveness.py                 # probe every source's real entry point
python3 tools/refresh-domains.py --apply  # follow redirects, rewrite moved domains
```

`liveness.py` hits each engine's actual entry point (the Blogger feed for
zeistmanga, the browse path for HTML engines) rather than the homepage, since
parked and migrated sites still answer `200` at `/`. It records Cloudflare
challenges separately from dead hosts. `--live-only` on the generator emits only
sources that answered.

`refresh-domains.py` follows each domain and rewrites it when the site has
permanently moved, keeping the old host in `altDomains`. A redirect to a domain
parker (hugedomains, sedo…) is treated as *sold* and marked broken rather than
followed.

## Cloudflare

**Handled by the app, not the extension.** Aidoku installs a request handler
that wraps every request the WASM interpreter makes; on a Cloudflare challenge
it opens a WebView, solves it, and returns the real bytes. The extension never
sees the challenge and needs no code for it.

This is the main advantage of the WASM path over a native Kotlin engine, which
has no interstitial solver on iOS. A large share of sources sit behind
Cloudflare and work fine in the app while failing a plain `curl` — so
`cargo test` (which has no solver) understates real coverage. Judge those
sources in the app.

## Icons

`tools/icons.py` fetches each source's favicon — the site's own declared icon
first, then Google's favicon cache (which reaches sites that block direct asset
requests, as the app does). Results matching a known generic placeholder are
rejected: an identical icon across hundreds of sources is worse than none. 487
of the 914 sources have a real icon; the rest are dead or parked domains with no
favicon anywhere.
