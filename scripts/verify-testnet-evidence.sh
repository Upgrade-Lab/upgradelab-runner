#!/usr/bin/env bash
# Independently re-check every transaction hash in a testnet report against the public RPC.
# Usage: scripts/verify-testnet-evidence.sh evidence/testnet/vault-correct.testnet.report.json
# Needs curl and jq. Prints: txHash <TAB> status <TAB> ledger. Exit 1 if any is not SUCCESS.
set -euo pipefail
report="${1:?report json}"
rpc="$(jq -r '.network.rpcUrl' "$report")"
bad=0
for tx in $(jq -r '.executedOps[].txHash // empty' "$report"); do
  res="$(curl -s -m 30 -X POST -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":{\"hash\":\"$tx\"}}" "$rpc")"
  status="$(echo "$res" | jq -r '.result.status')"
  ledger="$(echo "$res" | jq -r '.result.ledger // "-"')"
  printf '%s\t%s\t%s\n' "$tx" "$status" "$ledger"
  [ "$status" = "SUCCESS" ] || bad=1
done
exit $bad
