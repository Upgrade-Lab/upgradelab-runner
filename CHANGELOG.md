# Changelog

## 0.1.0 (unreleased)
- Vault fixture: v1, a corrected v2 migration, and five feature-gated defects (lost balance, doubled balance, unguarded re-initialization, unauthorized upgrade, non-idempotent migration); built WASM and sha256 manifest committed.
- Runner: in-process Soroban host backend running compiled WASM, with ed25519-signed, host-verified authorization (no mocked auth); scenario file v1; probes (view call or raw ledger entry); seven invariant kinds plus built-in upgrade checks; deterministic report v1 with JSON Schema; `run`, `replay`, `validate`, `check-report`, `schema`, opt-in `testnet`; documented exit codes.
- Evidence: six in-process reports with replays, native SDK test log, one real testnet run of the corrected path with RPC-verified transaction hashes.
