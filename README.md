# upgradelab-runner

Rehearse the upgrade using the state your application depends on.

A Soroban migration can compile, pass its own unit tests and still lose a balance, double one, let anyone call `initialize` again, or leave the upgrade function unguarded. UpgradeLab runs the **actual compiled old and new WASM**, seeded with the calls **you** write, performs the real `update_current_contract_wasm` upgrade and your migration calls, then judges **named invariants** over the state it reads back, and writes a **replayable report**. The companion viewer is [upgradelab-studio](../upgradelab-studio) (a separate repo).

## What a run looks like

The vault fixture (`fixtures/contracts`) is a toy: v1 stores balances per address; v2 stores a new struct, reads either format, and ships a batched, idempotent `migrate`. The same scenario run against the corrected v2 and a defective one (real output, `evidence/host/`):

```
$ upgradelab run scenarios/vault-correct.json
Verdict: PASS (19 passed, 0 failed, 0 inconclusive)                         # exit 0

$ upgradelab run scenarios/vault-broken-lose-balance.json
  FAIL  seeded-balances-preserved-after-migration
        balance-dave is 0 but expected 40
  FAIL  supply-equals-sum-after-migration
        sum of 4 parts is 500 but total-supply is 540 (difference 40)
Verdict: FAIL (16 passed, 3 failed, 0 inconclusive)                         # exit 1
```

| Scenario (`scenarios/`) | Defect compiled into v2 (`fixtures/contracts/vault-v2`) | Verdict | Caught by |
|---|---|---|---|
| `vault-correct` | none | pass (19/19) | |
| `vault-broken-lose-balance` | off-by-one deletes the last legacy balance | fail | expected values, supply == sum, shapes |
| `vault-broken-double-balance` | copy without delete, reader sums both formats | fail | already after the partial batch |
| `vault-broken-reinit` | `initialize` lost its guard (takeover, wipes accounting) | fail | rejected-op, state-unchanged |
| `vault-broken-upgrade-auth` | `upgrade` no longer calls `require_auth` | fail | rejected-op plus executable-hash change |
| `vault-broken-not-idempotent` | repeated `migrate` bumps a raw-storage counter | fail | stable-across-op on raw storage only |

Also exercised by the corrected build: **mixed-format state** (some entries old, some new, proven by raw-storage shapes), a **partial batched migration**, **repeated migration**, **repeated initialization**, and three **unauthorized upgrade attempts** (no authorization, signed by a non-admin, on both old and new code).

## Execution categories (kept distinct)

| Category | What ran | In a report |
|---|---|---|
| `compiled-wasm` | The `.wasm` files in the Soroban host linked into this runner (soroban-sdk 28.0.0 `testutils` `Env`). Not a network. | default; every invariant names its category |
| `testnet-rpc` | Real transactions on Stellar testnet via the Stellar CLI (opt-in `upgradelab testnet`) | separate report, own limits |
| `native-sdk` | Contract Rust compiled natively, mocked auth (`cargo test` in `fixtures/contracts`). The runner **never executes these**; `--native-log` records a log you supply, labeled `recorded-input` | listed beside, never merged into invariants |

Why the in-process host: Docker is unavailable here, so `stellar network container` is not an option. See [ADR 0002](docs/adr/0002-in-process-host-and-opt-in-testnet.md).

### Authorization is enforced, not mocked
`mock_all_auths` and `mock_auths` are never called (a test greps the source). A new host environment has no authorization entries, so `require_auth` fails. For each call the runner builds `SorobanAuthorizationEntry` values for the accounts the scenario names, signs the standard preimage with ed25519 and installs them with `Env::set_auths`; the host verifies the signature against the account's ledger entry and consumes the nonce. Tests show: no entry fails, a signature by the wrong key fails, a signature over other arguments fails, a replayed nonce fails. **Not covered:** multi-signer accounts and thresholds, custom account contracts, how a wallet builds an entry.

## Install and run
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

## Writing a scenario
A scenario (`schema/scenario.v1.schema.json`) is JSON you own: accounts by name, `ops` in order (seed ops on old WASM, exactly one `upgrade: true` op, post-upgrade ops), `probes` (a view call, or a raw ledger entry by durability + key) read before and after every op, and `invariants` over the readouts: `preserved`, `sumEquals`, `expectedValues`, `shapes`, `stableAcrossOp`, `opRejected`, `opSucceeded`. Expected values must come from your seed operations, not from the contract. See `scenarios/vault-correct.json`. Failed reads make an invariant `inconclusive`; they are never a pass or a mismatch.

## The report
`schema/report.v1.schema.json` (generated from the Rust types with schemars; a test fails if it is stale). It embeds the scenario, every executed operation (signers, outcome, authorizations the host recorded, executable hash after), probe readouts at each checkpoint, authorization checks, invariant results with evidence, WASM sha256, tool and host versions, categories and a `limits` list. It has no timestamp or machine path: running the same scenario against the same WASM gives byte-identical output (tested twice over, plus `replay` of every committed report).

## Testnet mode (opt-in)
`upgradelab testnet scenarios/testnet/vault-correct.testnet.json --out report.json` shells out to the Stellar CLI with throwaway keys under the alias prefix `ul-` in the CLI's own key store (never in this repo), on **testnet only** (mainnet is refused). It deploys v1, seeds, upgrades, migrates and reads state back through RPC simulation.

Recorded run (`evidence/testnet/`): contract `CC6TBNXUS5NFBKDPEULVFXB4ORDYRYN6JRFYQWEC6YLG6EJ5JNHJMLM4`, protocol 29, 12 transactions, all re-checked against the public RPC by `scripts/verify-testnet-evidence.sh` (`rpc-getTransaction.tsv`, all `SUCCESS`); the upgrade transaction is `84b53e15c9a583430b1a1aebb33b819617f19da9aef8d1e42c48f05bf3168afa`. Verdict PASS (14/14). Limits of this mode: not replayable; raw-storage probes unsupported; the Stellar CLI refuses to sign for an account whose key it lacks, so strangers' attacks are stopped by the CLI rather than rejected by the network and are left to the in-process host.

## Evidence
`evidence/host/*.report.{json,txt}` (six scenarios, exit codes in `exit-codes.txt`, `*.replay.txt` all REPRODUCED), `evidence/native/cargo-test-fixtures.txt`, `evidence/testnet/`. Regenerate with `scripts/record-evidence.sh`.

## Tests
`cargo test`: 68 passing (9 unit incl. forged-signature, nonce-replay and CLI-parsing tests; 59 integration in one binary: scenario outcomes derived from the vault spec, determinism and replay, validation, engine edge cases, CLI exit codes, source guards). Fixture native tests: `cd fixtures/contracts && cargo test` (7). CI: `.github/workflows/ci.yml`.

## What this does not do
- No cloning of live ledger state and no discovery of storage keys: state comes from your seed operations and probes read only what they name.
- No claim of exhaustive safety. A pass means the named invariants held in this scenario in this host.
- No fees, resource limits as a network enforces them, state archival or restore (entries do not expire during a run; balances in the fixture are persistent, never temporary), and the host protocol (28) can differ from the live network (testnet was 29).
- Not a static analyzer (use soroban-upgrade-safeguard for interface/layout diffs) and not a test library (Crucible); see [ADR 0001](docs/adr/0001-incremental-value-over-existing-tools.md).
- The vault is a teaching fixture, not an audited contract. No contract-security expert has reviewed the invariants.

## Status
v0.1.0, unreleased and unpublished: no GitHub repository, no crate release. Engineering complete for the declared scope; CI workflow written but not yet run on GitHub.

MIT licensed.
