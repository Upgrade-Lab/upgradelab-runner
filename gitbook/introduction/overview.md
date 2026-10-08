# Overview

Published on crates.io: `cargo install upgradelab-runner`.

Rehearse the upgrade using the state your application depends on.

A Soroban migration can compile, pass its own unit tests and still lose a balance, double one, let anyone call `initialize` again, or leave the upgrade function unguarded. UpgradeLab runs the **actual compiled old and new WASM**, seeded with the calls **you** write, performs the real `update_current_contract_wasm` upgrade and your migration calls, then judges **named invariants** over the state it reads back, and writes a **replayable report**. The companion viewer is [upgradelab-studio](https://github.com/Upgrade-Lab/upgradelab-studio) (a separate repo).

Source: [upgradelab-runner on GitHub](https://github.com/Upgrade-Lab/upgradelab-runner). Releases: [GitHub releases](https://github.com/Upgrade-Lab/upgradelab-runner/releases).
