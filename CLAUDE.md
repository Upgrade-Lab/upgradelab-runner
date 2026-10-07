# upgradelab-runner: working notes

Commands: `export CARGO_TARGET_DIR=/home/gamp/.cache/stellar-cargo-target`; `cargo test -j2` (one integration binary, `tests/it`), `cargo clippy --all-targets`, `cargo fmt`, `scripts/build-fixtures.sh` (stellar CLI 28.1.0, builds ALL fixture WASM with `stellar contract build`), `scripts/record-evidence.sh`, `cargo run -q -- schema report > schema/report.v1.schema.json` (also scenario).
Constraints: soroban-sdk pinned `=28.0.0`; never mock auths (signed entries via `Env::set_auths`); reports deterministic (no clock/random/paths); native, compiled-WASM and testnet results are separate categories; failed read = inconclusive; local only (no GitHub repo, push or deploy); disk is tight, keep one target dir; no AI co-author trailers in commits; explicit `git add` paths.
Testnet mode: `stellar` on PATH, `~/.local/bin`; throwaway keys `ul-*` in ~/.config/stellar; testnet only.
Unfinished: CI never run on GitHub; no container local-network backend (Docker unavailable); no archival/restore simulation; testnet attacks by strangers are CLI-refused rather than network-rejected; no external review of fixture or invariants.
