# ADR 0002: Execute compiled WASM in an in-process host, with an opt-in testnet mode

Status: accepted, 2026-10-07.

## Context
The ideal rehearsal runs on a local Stellar network (`stellar network container`). Docker is not usable in the build environment (socket permission denied), so a container-based local network is not available. Testnet is public, shared, rate-limited and mutable, so it cannot be the only way to run (not deterministic, needs funds and network).

## Options
1. Container local network: unavailable here.
2. Native SDK tests only (register the contract's Rust type in a test `Env`): fast, but the contract runs as native Rust, not as WASM in the VM, so it cannot exercise `update_current_contract_wasm` between two real artifacts and does not prove the compiled build behaves.
3. **In-process host with compiled WASM**: `soroban-sdk` 28.0.0 with `testutils` links the Soroban host (`soroban-env-host`). Registering WASM bytes (`register_at` with `&[u8]`, `deployer().upload_contract_wasm`) makes the host execute the compiled module in its VM, and `update_current_contract_wasm` swaps executables between uploaded artifacts exactly as on a ledger.
4. Testnet through the Stellar CLI as a second, opt-in mode.

## Decision
Implement 3 as the default (category `compiled-wasm`) and 4 as an explicit subcommand (category `testnet-rpc`). Native SDK test results are a third category, `native-sdk`; the runner never produces them itself. It can attach a `cargo test` log as a recorded input, labeled as such. A report's invariants carry the category that produced them and categories are never merged.

Why a Rust crate: the host that matters is a Rust library; the report generator and CLI sit in the same process. A Rust+TypeScript split would add an IPC boundary and two toolchains for no benefit. The viewer (a separate repo) is TypeScript.

## What the in-process host does and does not do
Does: run the actual `.wasm` bytes in the host VM; keep persistent, instance and temporary storage in an in-memory ledger; let the contract replace its own executable; verify authorization.

Does not: model consensus, fees, transaction envelopes, resource limits as a network enforces them, state archival or restore (entries do not expire during a run), or the exact protocol of the live network. The host's reported protocol is 28 (the linked sdk); testnet was at protocol 29 when this was written, and the report records the host protocol it actually used.

## Authorization
`mock_all_auths` and `mock_auths` are not used anywhere (a test greps the sources). A freshly created `Env` has no authorization entries, so `require_auth` fails. `mock_auths` would also be a poor test: it registers a stub account contract at the address that accepts any signature. Instead, for each call the runner builds `SorobanAuthorizationEntry` values for the accounts the scenario names, signs the standard preimage (network id, nonce, expiry ledger, invocation) with ed25519, and installs them with `Env::set_auths`. The host then checks the signature against the account's ledger entry (which the runner creates with master weight 1), matches the invocation tree and consumes the nonce. Tests prove that: no entry fails, a signature by the wrong key fails ("signer does not belong to account"), a signature over different arguments fails, a replayed nonce fails. Not covered: multi-signer accounts, thresholds, custom account contracts, and how a wallet constructs the entry.

## Testnet mode
Shells out to the Stellar CLI (`upload`, `deploy`, `invoke`, `fetch`) with throwaway keys created under an alias prefix in the CLI's key store, never in the repo. Only `testnet` is accepted as network (a test checks mainnet is refused). Limits, stated in every testnet report: not replayable; raw-storage probes unsupported (reported inconclusive); the CLI refuses to sign for an account whose key it lacks, so an "unauthorized" attempt by a stranger is stopped by the CLI before the network (classified `CliMissingSigningKey`) rather than rejected by the network, so the shipped testnet scenario leaves those attacks to the in-process host.

## Consequences
- Deterministic CI is possible: names derive keys, the contract address is derived from a fixed string, nonces count from one, ed25519 signing is deterministic, reports contain no clock or path, and a determinism test compares two runs byte for byte.
- The runner proves "this scenario, this host version"; the testnet evidence adds one real-network run of the corrected path.
- Moving to a container local network later means adding a third backend behind the same `Backend` trait.
