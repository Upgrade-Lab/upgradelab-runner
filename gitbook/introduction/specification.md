# Specification

## User
A team that owns a Soroban contract with persistent state (balances, positions, configuration) and is about to ship a new version that changes how that state is stored. They want to rehearse the upgrade with state shaped like the state their application depends on, before touching a real network, and to keep a report they or a reviewer can re-run.

## Scope (v1)
1. **Application-provided seed operations.** A scenario file lists calls to make on the old WASM (for example `deposit` by named test accounts). The runner makes them, with real signed authorization.
2. **The actual upgrade.** One operation is marked `upgrade`; it is an ordinary call to the contract's own upgrade function, which calls `update_current_contract_wasm` with the hash of the new WASM, uploaded by the runner.
3. **Post-upgrade operations.** Migration calls, further deposits (which create mixed old/new entries), repeated migration, repeated initialization, attack attempts.
4. **Probes.** Named reads of state: a view-function call, or a raw ledger entry of the contract read by storage key (durability + key), taken before and after every operation.
5. **Named invariants** judged over probe readouts and operation outcomes: `preserved`, `sumEquals`, `expectedValues`, `shapes`, `stableAcrossOp`, `opRejected`, `opSucceeded`, plus three built-ins (`seed-operations-succeeded`, `upgrade-applied`, `post-upgrade-operations-succeeded`).
6. **Authorization checks.** A call with no authorization, or authorized by the wrong account, must be rejected by the host's real authorization check; the report records signers, error class and whether the contract's executable changed.
7. **A replayable report.** Versioned JSON with a JSON Schema, plus text. Contains the scenario, the executed operations, probe readouts at every checkpoint, authorization checks, invariant results with evidence, the WASM sha256 hashes, tool and host versions, and the execution categories. Re-running the embedded scenario reproduces the report byte for byte.
8. **CLI** with documented exit codes.
9. **Opt-in testnet mode** that runs the same scenario with the Stellar CLI against Stellar testnet and records transaction hashes.

## Non-goals
- No cloning, fetching or importing of live ledger state, and no discovery of storage keys. State comes from seed operations; probes read only what they name.
- No claim of exhaustive safety, soundness, or audit. A pass means the named invariants held in this scenario.
- No static analysis of WASM (see ADR 0001), no fuzzing, no gas/fee estimation, no archival or restore simulation, no multi-contract dependency graphs, no custom account contracts or multi-signer accounts.
- No hosted execution of user code (the viewer repo shows reports only).
- No GitHub, registry or network publishing from this repository.

## Data model
- **Scenario v1** (`schema/scenario.v1.schema.json`): `scenarioVersion`, `name`, `wasm {old,new}` (paths relative to `--root`, no `..`), `accounts` (names), `ops`, `probes`, `invariants`. Unknown fields are rejected. Values are typed (`account`, `i128`, `u32`, `symbol`, `wasmHash`, `variant`, ...); exactly one value field per value.
- **Report v1** (`schema/report.v1.schema.json`): `reportVersion`, `kind`, `tool`, `scenario {name, sha256, definition}`, `wasm {old,new}`, `categories`, `executedOps`, `checkpoints`, `authChecks`, `invariants`, `verdict`, `limits`, optional `network`.
- **Categories**: `compiled-wasm` (executed by default), `testnet-rpc` (testnet mode), `native-sdk` (never executed by the runner; a supplied `cargo test` log is recorded as input). Every invariant result names its category. Categories are not merged.
- **Identities**: account keys are `ed25519(sha256("upgradelab/account/" + name))`; the contract address is derived from `sha256("upgradelab/contract/subject")`. These exist only in the in-process ledger and are not secrets.

## Interfaces
```
upgradelab run <scenario.json> [--root DIR] [--format text|json] [--out FILE] [--native-log FILE]
upgradelab replay <report.json> [--root DIR]
upgradelab validate <scenario.json>
upgradelab check-report <report.json>
upgradelab schema report|scenario
upgradelab testnet <scenario.json> [--out FILE] [--key-prefix P] [--anonymous-source ACCOUNT]
```
Exit codes: 0 all invariants passed; 1 an invariant failed (or a replay differed); 2 invalid input; 3 inconclusive (nothing failed, but something could not be evaluated); 4 environment error (unreadable file, missing CLI, network failure).

## Failure classes the runner is designed to catch
| Class | Example in the vault fixture | Caught by |
|---|---|---|
| Balance lost in migration | `broken-lose-balance` (off-by-one deletes the last legacy entry) | `expectedValues`, `sumEquals`, `shapes` |
| Balance doubled | `broken-double-balance` (copy without delete, reader sums both formats) | `expectedValues` at the partial-migration checkpoint, `sumEquals`, `shapes` |
| Re-initialization wipes state / takeover | `broken-reinit` | `opRejected`, `stableAcrossOp` |
| Unauthorized upgrade | `broken-upgrade-auth` | `opRejected` (+ executable hash) |
| Non-idempotent migration | `broken-not-idempotent` | `stableAcrossOp` over raw storage |
| Mixed old/new entries read incorrectly | (correct build exercises it) | `shapes` proves mixed state exists, `expectedValues` checks reads |

## Honesty rules
- A failed read is never a mismatch; missing evidence is never a pass. An invariant that cannot read what it needs is `inconclusive` (exit 3).
- An expectation in the scenario is derived from the seed operations by the author, not from the contract.
- The report carries its own `limits` list and the text output prints it.

## Architecture
`scenario` (types, validation) -> `engine` (runs ops against a `Backend`, reads probes before and after each op, evaluates invariants) -> `report`. Backends: `host` (in-process Soroban host; default) and `testnet` (Stellar CLI). `values` converts typed values to XDR and renders results; `text` prints; `native` parses a supplied cargo test log.

## Acceptance
- `cargo test` passes (unit, scenario, determinism, validation, engine, CLI, guard tests).
- The correct vault scenario passes every invariant; each defective build fails the invariants listed above and no unrelated ones.
- Two runs of the same scenario give identical bytes; `replay` reports REPRODUCED.
- The committed schemas equal the generated ones.
- Evidence for the in-process runs and one real testnet run of the corrected path is committed under `evidence/`.
