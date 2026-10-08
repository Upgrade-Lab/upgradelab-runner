# Testnet mode (opt-in)

`upgradelab testnet scenarios/testnet/vault-correct.testnet.json --out report.json` shells out to the Stellar CLI with throwaway keys under the alias prefix `ul-` in the CLI's own key store (never in this repo), on **testnet only** (mainnet is refused). It deploys v1, seeds, upgrades, migrates and reads state back through RPC simulation.

Recorded run (`evidence/testnet/`): contract `CC6TBNXUS5NFBKDPEULVFXB4ORDYRYN6JRFYQWEC6YLG6EJ5JNHJMLM4`, protocol 29, 12 transactions, all re-checked against the public RPC by `scripts/verify-testnet-evidence.sh` (`rpc-getTransaction.tsv`, all `SUCCESS`); the upgrade transaction is `84b53e15c9a583430b1a1aebb33b819617f19da9aef8d1e42c48f05bf3168afa`. Verdict PASS (14/14). Limits of this mode: not replayable; raw-storage probes unsupported; the Stellar CLI refuses to sign for an account whose key it lacks, so strangers' attacks are stopped by the CLI rather than rejected by the network and are left to the in-process host.
