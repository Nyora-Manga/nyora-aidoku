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
  # `aidoku package` copies "the first .wasm it finds in the build dir" — and in
  # a Cargo *workspace* that build dir is the shared root target/ holding all 914
  # wasms, so every source shipped the same (alphabetically-first) wasm and every
  # install failed with a network error against the wrong site.
  #
  # The CLI walks up from the crate dir and stops at the first target/ it finds,
  # so a crate-local target/…/release/ containing ONLY this crate's wasm wins over
  # the shared root. aidoku package reuses the workspace build (no recompile) and
  # copies the one correct wasm.
  local id="$1"
  local crate rel
  crate=$(sed -n 's/^name = "\(.*\)"/\1/p' "sources/$id/Cargo.toml" | head -1)
  rel="$WASM_DIR/$(printf '%s' "$crate" | tr '.-' '__').wasm"
  [ -f "$rel" ] || return 1

  local local_rel="sources/$id/target/wasm32-unknown-unknown/release"
  rm -rf "sources/$id/target"
  mkdir -p "$local_rel"
  cp "$rel" "$local_rel/main.wasm"

  local rc=1
  if (cd "sources/$id" && aidoku package >/dev/null 2>&1) \
    && [ -f "sources/$id/package.aix" ]; then
    mv "sources/$id/package.aix" "$OUT/$id.aix"
    rc=0
  fi
  rm -rf "sources/$id/target"
  return $rc
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
