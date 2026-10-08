# Contributing

```bash
export CARGO_TARGET_DIR=...        # the debug build with the Soroban host is large; share one target dir
cargo fmt --check && cargo clippy --all-targets && cargo test
cargo run -q -- schema report   > schema/report.v1.schema.json     # after changing report types; commit the output
cargo run -q -- schema scenario > schema/scenario.v1.schema.json   # after changing scenario types
scripts/record-evidence.sh                                          # after changing behavior that shows in reports
```

- Never use `mock_all_auths` / `mock_auths` in the runner; a test enforces it. Authorization stays signed and enforced.
- Reports must stay deterministic: no clock, randomness, machine paths or map-order dependence. A test replays every committed report.
- A new invariant kind needs: a scenario type with `deny_unknown_fields`, a pass test, a fail test with a defect that is not the invariant author's own output, and an `inconclusive` test for unreadable evidence.
- A failed read is never a mismatch and missing evidence is never a pass.
- A new fixture defect goes behind one cargo feature in `vault-v2`, gets its own scenario and a test stating which invariants must fail and which must not. Rebuild with `scripts/build-fixtures.sh` and commit the WASM and manifest together.
- Keep the three execution categories separate; do not merge native-test results into invariants.
- Do not add commands that fetch live ledger state or publish anything.
- One logical change per commit. AI-assisted changes are welcome if you understand and verified them.
