# Install and run

Rust 1.96 (stable). Nothing is published; build from a clone.

```bash
git clone <this repo> && cd upgradelab-runner
export CARGO_TARGET_DIR=/path/to/a/target/dir            # optional; the debug build is large
cargo run -- run scenarios/vault-correct.json             # text report, exit code per below
cargo run -- run scenarios/vault-broken-lose-balance.json --format json --out report.json
cargo run -- replay report.json                           # re-run the embedded scenario; REPRODUCED or DIFFERENT
cargo run -- validate scenarios/vault-correct.json
cargo run -- check-report report.json
cargo run -- schema report > report.v1.schema.json
```

Scenario WASM paths are relative to `--root` (default `.`) and may not contain `..`.

Exit codes: `0` every invariant passed; `1` an invariant failed (or `replay` differed); `2` invalid input (scenario, report, path, usage); `3` inconclusive (nothing failed but something could not be evaluated, for example a probe that could not be read); `4` environment error (unreadable WASM, missing Stellar CLI, network failure).

### Rebuilding the fixture WASM
The built `.wasm` files and their sha256 are committed (`fixtures/wasm`, `MANIFEST.json`), so tests need no Stellar CLI. To rebuild: `scripts/build-fixtures.sh` (needs `stellar` 28.x and target `wasm32v1-none`; uses `stellar contract build`). Hashes can change with another rustc or CLI; a report always records the hash of the file it executed.
