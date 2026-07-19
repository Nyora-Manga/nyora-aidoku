#!/usr/bin/env python3
"""Assemble the installable source list from the built .aix packages.

Replaces the one step that needed Aidoku's external `aidoku` CLI, so the repo
builds with nothing but the Rust toolchain. Each .aix is a zip of
Payload/{source.json, main.wasm, icon.png}; this lays them out as Aidoku's
source-list format expects:

    public/
      index.min.json               {name, sources:[…]}
      sources/<id>-v<version>.aix
      icons/<id>-v<version>.png

Run by tools/package-all.sh; standalone usage:

    python3 tools/build-list.py dist/*.aix
"""
import json
import pathlib
import shutil
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "public"
NAME = "Nyora Local"


def main(paths):
    src_dir = OUT / "sources"
    icon_dir = OUT / "icons"
    for d in (src_dir, icon_dir):
        if d.exists():
            shutil.rmtree(d)
        d.mkdir(parents=True)

    sources = []
    for p in sorted(paths):
        aix = pathlib.Path(p)
        try:
            with zipfile.ZipFile(aix) as z:
                info = json.loads(z.read("Payload/source.json"))["info"]
                has_icon = "Payload/icon.png" in z.namelist()
                sid = info["id"]
                ver = info.get("version", 1)
                shutil.copyfile(aix, src_dir / f"{sid}-v{ver}.aix")
                if has_icon:
                    (icon_dir / f"{sid}-v{ver}.png").write_bytes(z.read("Payload/icon.png"))
        except (KeyError, zipfile.BadZipFile, json.JSONDecodeError) as e:
            print(f"  skip {aix.name}: {e}", file=sys.stderr)
            continue

        entry = {
            "id": sid,
            "name": info.get("name", sid),
            "version": ver,
            "downloadURL": f"sources/{sid}-v{ver}.aix",
            "languages": info.get("languages", []),
            "contentRating": info.get("contentRating", 0),
            "baseURL": info.get("url", ""),
        }
        if has_icon:
            entry["iconURL"] = f"icons/{sid}-v{ver}.png"
        sources.append(entry)

    sources.sort(key=lambda s: s["id"])
    doc = {"name": NAME, "sources": sources}
    (OUT / "index.min.json").write_text(json.dumps(doc, separators=(",", ":")))
    (OUT / "index.json").write_text(json.dumps(doc, indent=2))
    print(f"list: {len(sources)} packages, "
          f"{sum(1 for s in sources if 'iconURL' in s)} icons")


if __name__ == "__main__":
    main(sys.argv[1:])
