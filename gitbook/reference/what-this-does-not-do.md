# What this does not do

- No cloning of live ledger state and no discovery of storage keys: state comes from your seed operations and probes read only what they name.
- No claim of exhaustive safety. A pass means the named invariants held in this scenario in this host.
- No fees, resource limits as a network enforces them, state archival or restore (entries do not expire during a run; balances in the fixture are persistent, never temporary), and the host protocol (28) can differ from the live network (testnet was 29).
- Not a static analyzer (use soroban-upgrade-safeguard for interface/layout diffs) and not a test library (Crucible); see [ADR 0001](https://github.com/Upgrade-Lab/upgradelab-runner/blob/main/docs/adr/0001-incremental-value-over-existing-tools.md).
- The vault is a teaching fixture, not an audited contract. No contract-security expert has reviewed the invariants.
