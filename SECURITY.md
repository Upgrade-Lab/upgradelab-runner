# Security

UpgradeLab runs WASM you give it inside a Soroban host in a local process. Treat unknown `.wasm` files as untrusted input; run the runner in a container or throwaway environment for them. The testnet mode shells out to the Stellar CLI and uses throwaway keys; never point `--key-prefix` at aliases that hold real funds, and the runner refuses any network other than `testnet`.

Report vulnerabilities (for example a way to make a report claim a pass when an invariant failed, to read or write outside `--root` through a scenario, or to leak a key) through GitHub's private vulnerability reporting for this repository. Please do not open a public issue for them.

The vault fixture is a teaching contract. It is not audited and must not be deployed with real value.
