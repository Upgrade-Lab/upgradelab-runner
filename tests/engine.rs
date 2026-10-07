//! Behaviour of the engine on small purpose-built scenarios: it must never turn
//! missing evidence into a pass and never turn a failed read into a mismatch.
mod common;
use common::*;
use serde_json::{json, Value};
use upgradelab::report::Status;
use upgradelab::scenario::Scenario;

fn mutated(f: impl FnOnce(&mut Value)) -> Scenario {
    let mut v: Value = serde_json::from_str(&scenario_text("vault-correct")).unwrap();
    f(&mut v);
    Scenario::parse(&v.to_string()).unwrap()
}

#[test]
fn a_wrong_expectation_written_by_the_app_team_fails() {
    let s = mutated(|v| {
        let i = v["invariants"]
            .as_array()
            .unwrap()
            .iter()
            .position(|i| i["id"] == "seeded-balances-preserved-after-migration")
            .unwrap();
        v["invariants"][i]["expect"]["balance-bob"] = json!("251");
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "seeded-balances-preserved-after-migration"), Status::Fail);
    let inv = r.invariants.iter().find(|i| i.id == "seeded-balances-preserved-after-migration").unwrap();
    assert!(inv.summary.contains("balance-bob is 250 but expected 251"), "{}", inv.summary);
}

#[test]
fn a_probe_that_cannot_be_read_makes_the_invariant_inconclusive_not_failed_or_passed() {
    // schema_version does not exist on the old WASM, so reading it before the upgrade fails.
    let s = mutated(|v| {
        v["probes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind": "call", "id": "schema", "fn": "schema_version", "args": []}));
        v["invariants"].as_array_mut().unwrap().push(json!({"kind": "preserved", "id": "schema-same", "title": "schema version preserved", "from": "before:upgrade", "to": "after:upgrade", "probes": ["schema"]}));
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "schema-same"), Status::Inconclusive);
    let before = &r.checkpoints["before:upgrade"]["schema"];
    assert!(!before.ok && before.error.is_some());
    assert_eq!(r.verdict.status, Status::Inconclusive);
    assert_eq!(r.exit_code(), 3);
}

#[test]
fn a_failing_seed_operation_makes_the_run_inconclusive() {
    // Withdrawing more than the balance fails; the starting state was not established.
    let s = mutated(|v| {
        let ops = v["ops"].as_array_mut().unwrap();
        ops[2] = json!({"id": "s-bob", "phase": "seed", "fn": "withdraw", "args": [{"name": "from", "account": "bob"}, {"name": "amount", "i128": "1"}], "auth": ["bob"]});
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "seed-operations-succeeded"), Status::Inconclusive);
    assert_ne!(r.verdict.status, Status::Pass);
}

#[test]
fn an_op_that_must_succeed_but_traps_after_the_upgrade_fails_the_run() {
    // Calling a function that does not exist in the new WASM.
    let s = mutated(|v| {
        let ops = v["ops"].as_array_mut().unwrap();
        let i = ops.iter().position(|o| o["id"] == "p-migrate-again").unwrap();
        ops[i]["fn"] = json!("migrate_v3");
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "post-upgrade-operations-succeeded"), Status::Fail);
    assert_eq!(r.verdict.status, Status::Fail);
}

#[test]
fn a_rejected_call_with_the_wrong_expected_class_fails() {
    let s = mutated(|v| {
        let i = v["invariants"].as_array().unwrap().iter().position(|i| i["id"] == "reinitialize-rejected").unwrap();
        v["invariants"][i]["class"] = json!("auth");
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "reinitialize-rejected"), Status::Fail);
}

#[test]
fn an_upgrade_to_the_same_wasm_does_not_count_as_applied() {
    let s = mutated(|v| {
        v["wasm"]["new"] = v["wasm"]["old"].clone();
    });
    let r = run_scenario(&s);
    assert_eq!(status(&r, "upgrade-applied"), Status::Fail);
}

#[test]
fn authorization_is_never_mocked_an_unsigned_deposit_is_rejected_by_the_host() {
    let s = mutated(|v| {
        let ops = v["ops"].as_array_mut().unwrap();
        ops[1]["auth"] = json!([]);
        ops[1]["expect"] = json!("error");
    });
    let r = run_scenario(&s);
    let op = r.executed_ops.iter().find(|o| o.id == "s-alice-1").unwrap();
    assert_eq!(op.outcome.status, "error");
    assert_eq!(op.outcome.error.as_ref().unwrap().class, "auth");
    assert!(op.observed_auth.is_empty());
    assert_eq!(op.expectation, "met");
}

#[test]
fn signing_for_the_wrong_account_is_rejected() {
    let s = mutated(|v| {
        let ops = v["ops"].as_array_mut().unwrap();
        ops[1]["auth"] = json!(["bob"]); // alice's deposit authorized by bob
        ops[1]["expect"] = json!("error");
    });
    let r = run_scenario(&s);
    let op = r.executed_ops.iter().find(|o| o.id == "s-alice-1").unwrap();
    assert_eq!(op.outcome.status, "error");
    assert_eq!(op.outcome.error.as_ref().unwrap().class, "auth");
}

#[test]
fn raw_storage_probes_see_the_ledger_not_the_contracts_view_functions() {
    // balance() of the double-balance defect lies (500); the raw legacy entry does not.
    let r = run("vault-broken-double-balance");
    let cp = &r.checkpoints["after:p-migrate-partial"];
    assert_eq!(cp["balance-bob"].value.as_ref().unwrap(), "500");
    assert_eq!(cp["raw-legacy-bob"].value.as_ref().unwrap(), "250");
    assert_eq!(cp["raw-v2-bob"].value.as_ref().unwrap()["amount"], "250");
}
