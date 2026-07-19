#!/usr/bin/env bash
# Build every source and assemble the .aix packages + source list.
#
# `aidoku package` builds one crate at a time, rebuilding the whole dependency
# tree per source (~18s each, ~4.5h for 900). Instead the sources are workspace
# members, so ONE cargo invocation compiles them all in parallel with deps built
# once, and this script zips the results itself.
#
# An .aix is just a zip: Payload/{source.json, main.wasm, icon.png}
set -uo pipefail
cd "$(dirname "$0")/.."

WASM_DIR=target/wasm32-unknown-unknown/release
OUT=dist
rm -f /tmp/aix-missing.txt

echo "building all sources (parallel)…"
cargo build --release --target wasm32-unknown-unknown --keep-going 2>&1 \
  | grep -E "^error" | sort -u | head -20
echo "compiled: $(ls "$WASM_DIR"/*.wasm 2>/dev/null | wc -l | tr -d ' ') wasm modules"

rm -rf "$OUT" && mkdir -p "$OUT"
ok=0; missing=0
for dir in sources/*/; do
  id=$(basename "$dir")
  # The wasm is named after the CRATE, not the directory: pkg_id() strips
  # separators ("ar.mangahublink") while crate_name() keeps them as hyphens
  # ("ar-mangahub-link"), so derive it from Cargo.toml rather than the dir.
  crate=$(sed -n 's/^name = "\(.*\)"/\1/p' "$dir/Cargo.toml" | head -1)
  wasm="$WASM_DIR/$(echo "$crate" | tr '.-' '__').wasm"
  if [ ! -f "$wasm" ]; then missing=$((missing+1)); echo "$id" >> /tmp/aix-missing.txt; continue; fi
  stage=$(mktemp -d); mkdir -p "$stage/Payload"
  cp "$wasm" "$stage/Payload/main.wasm"
  cp "$dir/res/source.json" "$stage/Payload/source.json" 2>/dev/null || { rm -rf "$stage"; continue; }
  [ -f "$dir/res/icon.png" ] && cp "$dir/res/icon.png" "$stage/Payload/icon.png"
  (cd "$stage" && zip -qr package.zip Payload) && mv "$stage/package.zip" "$OUT/$id.aix" && ok=$((ok+1))
  rm -rf "$stage"
done
echo "packaged $ok  (no wasm for $missing)"

echo "building source list…"
aidoku build "$OUT"/*.aix -o public -n "Nyora Local" 2>/dev/null | tail -2
echo "list: $(ls public/sources 2>/dev/null | wc -l | tr -d ' ') packages, $(ls public/icons 2>/dev/null | wc -l | tr -d ' ') icons"
