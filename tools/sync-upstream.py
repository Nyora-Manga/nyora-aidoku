#!/usr/bin/env python3
"""Sync source definitions from the published upstream (nyora-data-driven's
extracted repo/*.json) into data/, without losing this repo's local patches.

The upstream is the single source of truth for WHICH sources exist and their
engine-behaviour config (datePattern, tag_prefix, listing paths…). This repo
additionally carries connectivity patches that are MORE current than upstream —
`domain`/`altDomains` rewritten by refresh-domains.py, and `broken` flags — plus
aidoku-only engines (mangafire) that upstream doesn't have. The merge keeps both:

  * a source present upstream  -> upstream row wins, but this repo's domain /
    altDomains / broken / brokenReason are overlaid back on (live-domain truth)
  * a source new upstream      -> added verbatim
  * a source dropped upstream  -> dropped here too (it went away / was ported out)
  * an engine absent upstream  -> left untouched (e.g. data/mangafire.json)

Usage (wired into rebuild-sources.yml, gated on the UPSTREAM_DATA_URL var):

    python3 tools/sync-upstream.py https://raw.githubusercontent.com/<owner>/<repo>/<ref>/repo

The argument is the base of the upstream repo/ directory; per-engine files are
fetched as <base>/<engine>.json. An engine that 404s or fails to fetch is left
exactly as-is, so a partial upstream never truncates the local catalogue.
"""
import json
import pathlib
import sys
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"

# Connectivity fields this repo owns: refresh-domains.py keeps these fresher than
# upstream, so they are preserved across a sync for sources that already exist.
LOCAL_WINS = ("domain", "altDomains", "broken", "brokenReason")


def rows_of(doc):
    """data/*.json is either a bare list or {"sources": [...]}. Return (rows, wrap)
    where wrap(rows) rebuilds the original shape."""
    if isinstance(doc, list):
        return doc, (lambda r: r)
    src = doc.get("sources", [])
    return src, (lambda r: {**doc, "sources": r})


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "nyora-aidoku-sync"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode())


def merge(local_rows, upstream_rows):
    local_by_id = {r["id"]: r for r in local_rows if "id" in r}
    out = []
    for up in upstream_rows:
        rid = up.get("id")
        cur = local_by_id.get(rid)
        if cur:
            for k in LOCAL_WINS:
                if k in cur:
                    up = {**up, k: cur[k]}
        out.append(up)
    return out


def main():
    if len(sys.argv) != 2 or not sys.argv[1].strip():
        sys.exit("usage: sync-upstream.py <base-url-of-upstream-repo-dir>")
    base = sys.argv[1].rstrip("/")

    changed = 0
    skipped = 0
    for f in sorted(DATA.glob("*.json")):
        engine = f.stem
        try:
            up_doc = fetch(f"{base}/{engine}.json")
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, ValueError) as e:
            print(f"  skip {engine}: upstream unavailable ({e})")
            skipped += 1
            continue

        local_doc = json.loads(f.read_text())
        local_rows, wrap = rows_of(local_doc)
        up_rows, _ = rows_of(up_doc)

        merged = merge(local_rows, up_rows)
        new_text = json.dumps(wrap(merged), indent=1, ensure_ascii=False) + "\n"
        if new_text != f.read_text():
            f.write_text(new_text)
            added = len({r.get("id") for r in merged} - {r.get("id") for r in local_rows})
            removed = len({r.get("id") for r in local_rows} - {r.get("id") for r in merged})
            print(f"  {engine}: {len(merged)} sources (+{added} -{removed})")
            changed += 1

    print(f"synced {changed} engine file(s), skipped {skipped} (kept local)")


if __name__ == "__main__":
    main()
