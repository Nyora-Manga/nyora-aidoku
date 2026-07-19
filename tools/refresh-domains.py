#!/usr/bin/env python3
"""Follow each source's domain and update data/ when it has permanently moved.

Manga sites change domain constantly, and a stale domain looks identical to a
dead source: the extension installs and returns nothing. This probes every live
row, follows redirects, and rewrites `domain` when the site now answers on a
different host — keeping the old one in `altDomains` so nothing is lost.

Redirects to a domain PARKER (hugedomains, afternic, sedo…) mean the domain was
sold, not moved, so those are marked `broken` instead of followed — otherwise
the source would point at a for-sale page.

    python3 tools/refresh-domains.py            # report only
    python3 tools/refresh-domains.py --apply    # rewrite data/*.json
"""
import argparse
import concurrent.futures as cf
import json
import pathlib
import subprocess
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
UA = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15"
ENGINES = ("madara", "mangareader", "zeistmanga", "hotcomics", "onemanga", "mmrcms")

# A redirect here means the domain was sold, not that the site moved.
PARKERS = (
    "hugedomains.com", "afternic.com", "sedo.com", "dan.com", "undeveloped.com",
    "buydomains.com", "domainmarket.com", "namecheap.com", "abovedomains.com",
)


def norm(host):
    return (host or "").lower().removeprefix("www.")


def probe(row):
    dom = row["domain"]
    try:
        p = subprocess.run(
            ["curl", "-s", "-o", "/dev/null", "-w", "%{http_code} %{url_effective}",
             "--max-time", "12", "-A", UA, "-L", f"https://{dom}/"],
            capture_output=True, text=True, timeout=25,
        )
        code, _, eff = p.stdout.partition(" ")
        host = urllib.parse.urlparse(eff.strip()).hostname or ""
    except Exception:
        return None
    if code != "200" or not host or norm(host) == norm(dom):
        return None
    parked = any(p in norm(host) for p in PARKERS)
    return {"engine": row["_e"], "id": row["id"], "old": dom, "new": host, "parked": parked}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true")
    a = ap.parse_args()

    rows = []
    for e in ENGINES:
        d = json.loads((DATA / f"{e}.json").read_text())
        d = d if isinstance(d, list) else d.get("sources", [])
        for r in d:
            if r.get("domain") and not r.get("broken"):
                r["_e"] = e
                rows.append(r)

    print(f"probing {len(rows)} live sources…")
    moves = []
    with cf.ThreadPoolExecutor(max_workers=10) as ex:
        for m in ex.map(probe, rows):
            if m:
                moves.append(m)

    moved = [m for m in moves if not m["parked"]]
    sold = [m for m in moves if m["parked"]]
    print(f"\n{len(moved)} moved:")
    for m in moved:
        print(f"  {m['id']:22} {m['old']:28} -> {m['new']}")
    print(f"\n{len(sold)} sold (will be marked broken):")
    for m in sold:
        print(f"  {m['id']:22} {m['old']:28} -> {m['new']}")

    if not a.apply:
        print("\n(report only — pass --apply to rewrite data/)")
        return

    by_engine = {}
    for m in moves:
        by_engine.setdefault(m["engine"], []).append(m)
    for engine, items in by_engine.items():
        f = DATA / f"{engine}.json"
        d = json.loads(f.read_text())
        rows_ = d if isinstance(d, list) else d.get("sources", [])
        index = {r["id"]: r for r in rows_}
        for m in items:
            r = index.get(m["id"])
            if not r:
                continue
            if m["parked"]:
                r["broken"] = True
                r["brokenReason"] = f"domain sold; redirects to {m['new']}"
            else:
                alts = r.setdefault("altDomains", [])
                if m["old"] not in alts:
                    alts.append(m["old"])
                r["domain"] = m["new"]
                # the primary must never also be an altDomain
                r["altDomains"] = [a for a in alts if a != r["domain"]]
        f.write_text(json.dumps(d, indent=1, ensure_ascii=False) + "\n")
        print(f"  updated data/{engine}.json ({len(items)} rows)")
    print("\nregenerate to pick these up:  python3 tools/generate.py")


if __name__ == "__main__":
    main()
