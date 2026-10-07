#!/usr/bin/env bash
# Rebuild every fixture WASM with `stellar contract build` and refresh fixtures/wasm/MANIFEST.json.
# Needs: stellar CLI 28.x, rust 1.96 with target wasm32v1-none.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/gamp/.cache/stellar-cargo-target}"
out=fixtures/wasm
mkdir -p "$out"
tmp="$(mktemp -d)"
manifest="$tmp/manifest.tsv"
: > "$manifest"

build() { # name package features
  local name="$1" pkg="$2" feat="$3"
  local d="$tmp/$name"
  mkdir -p "$d"
  if [ -n "$feat" ]; then
    stellar contract build --manifest-path fixtures/contracts/Cargo.toml --package "$pkg" --features "$feat" --locked --out-dir "$d"
  else
    stellar contract build --manifest-path fixtures/contracts/Cargo.toml --package "$pkg" --locked --out-dir "$d"
  fi
  cp "$d/${pkg//-/_}.wasm" "$out/$name.wasm"
  printf '%s\t%s\t%s\n' "$name" "$pkg" "$feat" >> "$manifest"
}

build vault_v1 vault-v1 ""
build vault_v2_correct vault-v2 ""
build vault_v2_broken_lose_balance vault-v2 broken-lose-balance
build vault_v2_broken_double_balance vault-v2 broken-double-balance
build vault_v2_broken_reinit vault-v2 broken-reinit
build vault_v2_broken_upgrade_auth vault-v2 broken-upgrade-auth

{
  echo '{'
  echo '  "note": "sha256 of the committed fixture WASMs. Hashes can differ if rebuilt with another rustc/stellar CLI; reports always record the hash of the file actually executed.",'
  echo '  "toolchain": {"stellar": "'"$(stellar --version | head -1)"'", "rustc": "'"$(rustc --version)"'"},'
  echo '  "wasm": ['
  first=1
  while IFS=$'\t' read -r name pkg feat; do
    sum="$(sha256sum "$out/$name.wasm" | cut -d' ' -f1)"
    size="$(stat -c %s "$out/$name.wasm")"
    [ $first -eq 1 ] || echo ','
    first=0
    printf '    {"file": "%s.wasm", "package": "%s", "features": "%s", "bytes": %s, "sha256": "%s"}' "$name" "$pkg" "$feat" "$size" "$sum"
  done < "$manifest"
  echo
  echo '  ]'
  echo '}'
} > "$out/MANIFEST.json"
echo "built; temp dir $tmp left in place (remove it yourself)"
