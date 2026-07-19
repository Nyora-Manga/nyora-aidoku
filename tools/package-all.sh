#!/usr/bin/env bash
# Build every source and assemble the .aix packages + source list.
#
# Packaging uses the official Aidoku CLI (`aidoku package` / `aidoku build`) so
# the .aix and index.min.json are exactly the format Aidoku installs — never a
# hand-rolled zip. (Hand-zipping added a Payload/ directory entry that Aidoku's
# installer rejected on "Get".)
#
# `aidoku package` normally rebuilds a crate's whole dependency tree (~18s each,
# ~4.5h for 900 sources). To avoid that, everything is built ONCE up front in a
# single parallel workspace `cargo build`; `aidoku package` then reuses that
# build via cargo's up-to-date check and runs in well under a second per crate.
#
# Requires: rustup target add wasm32-unknown-unknown, and the Aidoku CLI:
#   cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
set -uo pipefail
cd "$(dirname "$0")/.."

if ! command -v aidoku >/dev/null 2>&1; then
  echo "error: the 'aidoku' CLI is required." >&2
  echo "  cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli" >&2
  exit 1
fi

WASM_DIR=target/wasm32-unknown-unknown/release
OUT=dist
rm -f /tmp/aix-missing.txt

echo "building all sources (parallel)…"
cargo build --release --target wasm32-unknown-unknown --keep-going 2>&1 \
  | grep -E "^error" | sort -u | head -20
echo "compiled: $(ls "$WASM_DIR"/*.wasm 2>/dev/null | wc -l | tr -d ' ') wasm modules"

rm -rf "$OUT" && mkdir -p "$OUT"

package_one() {
  # aidoku package reuses the workspace build (done above), so this is a fast
  # up-to-date check plus a package step, not a fresh compile.
  (cd "sources/$1" && aidoku package >/dev/null 2>&1) \
    && [ -f "sources/$1/package.aix" ] \
    && mv "sources/$1/package.aix" "$OUT/$1.aix"
}

ok=0
for dir in sources/*/; do
  id=$(basename "$dir")
  if package_one "$id"; then ok=$((ok+1)); else echo "$id" >> /tmp/aix-missing.txt; fi
done

# Retry once — packaging under load occasionally trips on a build lock, and a
# gap here silently drops that source from the published list.
if [ -f /tmp/aix-missing.txt ]; then
  retry=$(sort -u /tmp/aix-missing.txt); : > /tmp/aix-missing.txt
  for id in $retry; do
    if package_one "$id"; then ok=$((ok+1)); else echo "$id" >> /tmp/aix-missing.txt; fi
  done
fi
fail=$([ -f /tmp/aix-missing.txt ] && wc -l < /tmp/aix-missing.txt | tr -d ' ' || echo 0)
echo "packaged $ok  (failed $fail)"
[ "$fail" != 0 ] && echo "  still failing: $(tr '\n' ' ' < /tmp/aix-missing.txt)"

echo "building source list…"
rm -rf public   # clean, so no stale package survives a rename
aidoku build "$OUT"/*.aix -o public -n "Nyora Local" 2>&1 | grep -v "no icon" | tail -2
echo "list: $(ls public/sources 2>/dev/null | wc -l | tr -d ' ') packages, \
$(ls public/icons 2>/dev/null | wc -l | tr -d ' ') icons"
