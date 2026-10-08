# What a run looks like

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
