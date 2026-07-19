#!/usr/bin/env python3
"""Fetch each source site's favicon into icons/<id>.png.

Aidoku shows `res/icon.png` from the package; without it every source renders
with a placeholder. generate.py copies from icons/ into each crate, so icons
survive regeneration and are only fetched once.

Sources tried in order: the site's own declared icon, then Google's favicon
cache (which reaches sites that 403 direct asset requests, as the iOS app does).
Results matching a known placeholder are rejected — shipping one generic image
across hundreds of sources is worse than shipping none.

Preference order matters: apple-touch-icon first because it's a real PNG at a
usable size, then the largest declared <link rel="icon">, then /favicon.ico.
Anything under 16px is rejected; Blogger and older WordPress themes only ship a
16px favicon.ico, which upscales acceptably and beats a placeholder.

    python3 tools/icons.py                 # all sources missing an icon
    python3 tools/icons.py --force         # refetch even if present
    python3 tools/icons.py --engine madara
"""
import argparse
import concurrent.futures as cf
import json
import pathlib
import re
import hashlib
import subprocess
import sys
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
ICONS = ROOT / "icons"
# NB: the old nyora-aidoku repo shipped an `icons-by-parser/` folder that looks
# like per-source artwork (363 files, 192x192). It is not — every one of them is
# a byte-identical copy of a single purple placeholder. Using it as a source
# gave hundreds of sources the same icon. Do not reintroduce it.
# What the iOS app uses (NyoraSource.swift): Google fetches and caches the
# favicon server-side, so it still works when the site 403s a direct request
# (Cloudflare-protected asset paths, notably).
GOOGLE = "https://www.google.com/s2/favicons?sz=128&domain={}"

# Generic "no icon" images, hashed AFTER normalisation. Google serves a globe
# when it has nothing cached, and several dead/parked hosts serve the same
# default circle. Shipping these is worse than shipping nothing: every source
# looks identical, which is exactly what happened before this check existed.
PLACEHOLDERS = {
    "8100e075952a4806ded83707277c9f744d22bbf410704d5e279530243c1e5a74",
    "3fe6cc108f44ddd704faca9d447f392f8e30d76b27305e68ab295063a6633155",
}
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"
SIZE = 128
# Accept 16x16. Blogger and older WordPress themes only publish a 16px
# favicon.ico; upscaled it is soft, but a recognisable site icon still beats
# Aidoku's generic placeholder.
MIN_SRC = 16


def curl(url, binary=False, timeout=15):
    try:
        p = subprocess.run(
            ["curl", "-sL", "--max-time", str(timeout), "-A", UA, url],
            capture_output=True, timeout=timeout + 10,
        )
        if p.returncode != 0 or not p.stdout:
            return None
        return p.stdout if binary else p.stdout.decode("utf-8", "ignore")
    except Exception:
        return None


def icon_candidates(html, base):
    """Declared icons, best first."""
    out = []
    for m in re.finditer(r"<link\b[^>]*>", html, re.I):
        tag = m.group(0)
        rel = re.search(r'rel=["\']([^"\']+)', tag, re.I)
        href = re.search(r'href=["\']([^"\']+)', tag, re.I)
        if not rel or not href or "icon" not in rel.group(1).lower():
            continue
        sizes = re.search(r'sizes=["\'](\d+)', tag, re.I)
        px = int(sizes.group(1)) if sizes else 0
        # apple-touch-icon is reliably a decent-sized PNG
        rank = 1000 if "apple-touch" in rel.group(1).lower() else px
        out.append((rank, urllib.parse.urljoin(base, href.group(1))))
    out.sort(key=lambda x: -x[0])
    seen, urls = set(), []
    for _, u in out:
        if u not in seen:
            seen.add(u)
            urls.append(u)
    return urls


def sniff_ext(data):
    """Extension from magic bytes.

    ImageMagick decides the decoder from the FILE EXTENSION when the container
    is ambiguous — writing the download to a generic name makes `identify` fail
    on .ico in particular, which silently loses every Blogger favicon.
    """
    if data[:4] == b"\x00\x00\x01\x00":
        return ".ico"
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return ".png"
    if data[:2] == b"\xff\xd8":
        return ".jpg"
    if data[:4] == b"GIF8":
        return ".gif"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return ".webp"
    head = data[:200].lstrip()
    if head[:5] == b"<?xml" or head[:4] == b"<svg":
        return ".svg"
    return ".png"


def convert(data, dest):
    """Normalise to a square PNG. Rejects sources too small to look right."""
    # NB: a distinct filename, not dest.with_suffix(). When the download is
    # already a .png that returns the SAME path as dest, and the cleanup below
    # then deletes the icon it just produced.
    tmp = dest.parent / f".tmp-{dest.stem}{sniff_ext(data)}"
    tmp.write_bytes(data)
    try:
        probe = subprocess.run(
            ["magick", "identify", "-format", "%w %h", f"{tmp}[0]"],
            capture_output=True, text=True, timeout=20,
        )
        w, h = (int(x) for x in probe.stdout.split()[:2])
        if max(w, h) < MIN_SRC:
            return False
        r = subprocess.run(
            ["magick", f"{tmp}[0]", "-background", "none",
             # tiny favicons get enlarged, so pick the filter that keeps small
             # pixel art legible rather than smearing it
             "-filter", "Lanczos", "-resize", f"{SIZE}x{SIZE}",
             "-gravity", "center", "-extent", f"{SIZE}x{SIZE}",
             "-strip", f"PNG32:{dest}"],
            capture_output=True, timeout=30,
        )
        if r.returncode != 0 or not dest.exists() or dest.stat().st_size == 0:
            return False
        if hashlib.sha256(dest.read_bytes()).hexdigest() in PLACEHOLDERS:
            dest.unlink(missing_ok=True)
            return False
        return True
    except Exception:
        return False
    finally:
        tmp.unlink(missing_ok=True)


def fetch(row):
    ident = row["_id"]
    dest = ICONS / f"{ident}.png"
    base = f"https://{row['domain']}"

    # 1. the site's own declared icon
    html = curl(base)
    urls = icon_candidates(html, base) if html else []
    urls.append(urllib.parse.urljoin(base, "/favicon.ico"))
    for u in urls[:4]:
        data = curl(u, binary=True)
        if data and len(data) > 100 and convert(data, dest):
            return ident, "site"

    # 2. Google's cache — reaches sites that block direct asset requests
    data = curl(GOOGLE.format(row["domain"]), binary=True, timeout=20)
    if data and len(data) > 100 and convert(data, dest):
        return ident, "google"
    return ident, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine")
    ap.add_argument("--force", action="store_true")
    a = ap.parse_args()

    ICONS.mkdir(exist_ok=True)
    rows = []
    for f in sorted(DATA.glob("*.json")):
        if a.engine and f.stem != a.engine:
            continue
        try:
            d = json.loads(f.read_text())
        except Exception:
            continue
        d = d if isinstance(d, list) else d.get("sources", [])
        for r in d:
            if not r.get("domain"):
                continue
            slug = re.sub(r"[^a-z0-9]", "", r["id"].lower())
            r["_id"] = f"{r.get('lang') or 'en'}.{slug}"
            if not a.force and (ICONS / f"{r['_id']}.png").exists():
                continue
            rows.append(r)

    if not rows:
        print("nothing to fetch (use --force to refetch)")
        return
    print(f"fetching {len(rows)} icons…")
    import collections
    by = collections.Counter()
    with cf.ThreadPoolExecutor(max_workers=6) as ex:
        for _ident, src in ex.map(fetch, rows):
            by[src or "none"] += 1
    ok = sum(v for k, v in by.items() if k != "none")
    have = len(list(ICONS.glob("*.png")))
    detail = " ".join(f"{k}={v}" for k, v in by.most_common())
    print(f"got {ok}/{len(rows)} ({detail}); {have} cached in {ICONS.relative_to(ROOT)}/")


if __name__ == "__main__":
    main()
