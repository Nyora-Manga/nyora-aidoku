#!/usr/bin/env python3
"""Probe every source and record whether it is actually usable.

Writes data-liveness.json, which generate.py uses to skip dead sources.

Probing `/` is not enough: parked domains and sites that have migrated off the
engine both answer 200 there. `klmanhua.com` (parked, for sale) and
`nekoproject.org` (no longer Blogger) both passed a homepage check and then
failed at runtime. So this hits the ENGINE'S OWN entry point — the Blogger feed
for zeistmanga, the browse path for the HTML engines — and additionally
fingerprints parked pages.

    python3 tools/liveness.py            # probe everything, rewrite the file
    python3 tools/liveness.py --engine zeistmanga
"""
import argparse
import concurrent.futures as cf
import json
import pathlib
import subprocess
import sys
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
OUT = ROOT / "data-liveness.json"
UA = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15"

# Markers that mean "this domain no longer serves the site" even though it
# returns 200. Domain-parking pages are the common case.
PARKED = (
    "abovedomains", "domain-for-sale", "buy this domain", "forsale.min.js",
    "sedoparking", "parkingcrew", "hugedomains", "afternic",
)


def probe_url(row):
    """The engine's real entry point, not the homepage."""
    cfg = row.get("config") or {}
    dom = row["domain"]
    eng = row["_engine"]
    if eng == "zeistmanga":
        cat = cfg.get("mangaCategory", "Series")
        return (f"https://{dom}/feeds/posts/default/-/"
                f"{urllib.parse.quote(cat)}?alt=json&max-results=2")
    if eng == "mangareader":
        lu = cfg["listUrl"] if "listUrl" in cfg else "/manga"
        return f"https://{dom}{lu}/"
    if eng == "madara":
        return f"https://{dom}/{cfg.get('listUrl', 'manga/')}"
    return f"https://{dom}/"


def classify(row):
    url = probe_url(row)
    try:
        p = subprocess.run(
            ["curl", "-s", "-L", "--max-time", "15", "-A", UA,
             "-w", "\n%{http_code}", url],
            capture_output=True, text=True, timeout=30,
        )
        body, _, code = p.stdout.rpartition("\n")
    except Exception:
        return "000"
    # 403/503 from these hosts is almost always a Cloudflare challenge, which
    # the Aidoku app solves in a WebView even though curl can't. Marking those
    # dead would drop ~128 sources that work fine on-device.
    if code in ("403", "503", "429"):
        return "cf"
    if code != "200":
        return code or "000"
    low = body[:20000].lower()
    if any(m in low for m in PARKED):
        return "parked"
    if row["_engine"] == "zeistmanga":
        # must be a Blogger feed, not an HTML page
        try:
            json.loads(body)
        except Exception:
            return "notfeed"
    elif len(body) < 2000:
        # a real listing page is never this small
        return "empty"
    return "200"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine")
    a = ap.parse_args()

    rows = []
    for f in sorted(DATA.glob("*.json")):
        try:
            d = json.loads(f.read_text())
        except Exception:
            continue
        d = d if isinstance(d, list) else d.get("sources", [])
        if a.engine and f.stem != a.engine:
            continue
        for r in d:
            if r.get("broken") or not r.get("domain"):
                continue
            r["_engine"] = f.stem
            rows.append(r)

    if not rows:
        sys.exit("no rows to probe")

    prev = json.loads(OUT.read_text()) if OUT.exists() else {}
    with cf.ThreadPoolExecutor(max_workers=24) as ex:
        for row, status in zip(rows, ex.map(classify, rows)):
            prev[f"{row['_engine']}:{row['id']}"] = status

    OUT.write_text(json.dumps(prev, indent=0, sort_keys=True))
    good = sum(1 for v in prev.values() if v == "200")
    print(f"probed {len(rows)}; {good}/{len(prev)} usable overall")
    import collections
    print(collections.Counter(v for v in prev.values()).most_common(8))


if __name__ == "__main__":
    main()
