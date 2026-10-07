#!/usr/bin/env bash
# Regenerate the committed in-process evidence: native SDK test log, one report per
# scenario (JSON + text), and the exit code of each run.
# Usage: scripts/record-evidence.sh   (from anywhere; needs cargo)
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/gamp/.cache/stellar-cargo-target}"
mkdir -p evidence/host evidence/native

# Native SDK category: the fixture crates' own `cargo test` (native Rust, mocked auth).
# Only the per-test lines are kept; timing lines are dropped so the log is stable.
(cd fixtures/contracts && cargo test -j2 2>&1 | grep -E '^test .* \.\.\. (ok|FAILED|ignored)') > evidence/native/cargo-test-fixtures.txt

cargo build -q -j2
bin="$CARGO_TARGET_DIR/debug/upgradelab"
: > evidence/host/exit-codes.txt
for s in scenarios/vault-*.json; do
  name="$(basename "$s" .json)"
  extra=()
  if [ "$name" = "vault-correct" ]; then extra=(--native-log evidence/native/cargo-test-fixtures.txt); fi
  set +e
  "$bin" run "$s" --out "evidence/host/$name.report.json" "${extra[@]}" > "evidence/host/$name.report.txt"
  code=$?
  set -e
  echo "$name $code" >> evidence/host/exit-codes.txt
  # Replays must reproduce; record that too.
  "$bin" replay "evidence/host/$name.report.json" "${extra[@]}" > "evidence/host/$name.replay.txt"
done
cat evidence/host/exit-codes.txt
