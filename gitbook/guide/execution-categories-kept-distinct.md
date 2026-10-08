# Execution categories (kept distinct)

| Category | What ran | In a report |
|---|---|---|
| `compiled-wasm` | The `.wasm` files in the Soroban host linked into this runner (soroban-sdk 28.0.0 `testutils` `Env`). Not a network. | default; every invariant names its category |
| `testnet-rpc` | Real transactions on Stellar testnet via the Stellar CLI (opt-in `upgradelab testnet`) | separate report, own limits |
| `native-sdk` | Contract Rust compiled natively, mocked auth (`cargo test` in `fixtures/contracts`). The runner **never executes these**; `--native-log` records a log you supply, labeled `recorded-input` | listed beside, never merged into invariants |

Why the in-process host: Docker is unavailable here, so `stellar network container` is not an option. See [ADR 0002](https://github.com/Upgrade-Lab/upgradelab-runner/blob/main/docs/adr/0002-in-process-host-and-opt-in-testnet.md).

### Authorization is enforced, not mocked
`mock_all_auths` and `mock_auths` are never called (a test greps the source). A new host environment has no authorization entries, so `require_auth` fails. For each call the runner builds `SorobanAuthorizationEntry` values for the accounts the scenario names, signs the standard preimage with ed25519 and installs them with `Env::set_auths`; the host verifies the signature against the account's ledger entry and consumes the nonce. Tests show: no entry fails, a signature by the wrong key fails, a signature over other arguments fails, a replayed nonce fails. **Not covered:** multi-signer accounts and thresholds, custom account contracts, how a wallet builds an entry.
