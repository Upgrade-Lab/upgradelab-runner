# Tests

`cargo test`: 68 passing (9 unit incl. forged-signature, nonce-replay and CLI-parsing tests; 59 integration in one binary: scenario outcomes derived from the vault spec, determinism and replay, validation, engine edge cases, CLI exit codes, source guards). Fixture native tests: `cd fixtures/contracts && cargo test` (7). CI: `.github/workflows/ci.yml`.
