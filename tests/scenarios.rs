//! Expected outcomes come from the vault's specification (which invariant each
//! deliberate defect violates), not from whatever the runner printed.
mod common;
use common::*;
use upgradelab::report::Status;

const AUTH_AND_INIT: [&str; 6] = [
    "unauthorized-upgrade-rejected-old-contract",
    "unauthorized-upgrade-rejected-no-auth",
    "unauthorized-upgrade-rejected-self-signed",
    "reinitialize-rejected",
    "reinitialize-changes-nothing",
    "migration-idempotent",
];

#[test]
fn corrected_migration_passes_every_invariant() {
    let r = run("vault-correct");
    assert_eq!(r.verdict.status, Status::Pass, "{:?}", failing(&r));
    assert_eq!(r.verdict.failed, 0);
    assert_eq!(r.verdict.inconclusive, 0);
    assert_eq!(r.exit_code(), 0);
    assert!(r.invariants.len() >= 19);
}

#[test]
fn losing_a_balance_is_caught_by_the_balance_invariants_only() {
    let r = run("vault-broken-lose-balance");
    assert_eq!(r.verdict.status, Status::Fail);
    assert_eq!(r.exit_code(), 1);
    assert_eq!(status(&r, "seeded-balances-preserved-after-migration"), Status::Fail);
    assert_eq!(status(&r, "supply-equals-sum-after-migration"), Status::Fail);
    // The loss is in dave's balance (last holder): 40 of 540 disappears.
    let inv = r.invariants.iter().find(|i| i.id == "supply-equals-sum-after-migration").unwrap();
    assert!(inv.summary.contains("500") && inv.summary.contains("540"), "{}", inv.summary);
    // Things the defect does not touch still pass.
    for id in AUTH_AND_INIT {
        assert_eq!(status(&r, id), Status::Pass, "{id}");
    }
    assert_eq!(status(&r, "balances-preserved-across-upgrade"), Status::Pass);
    assert_eq!(status(&r, "mixed-format-balances-correct"), Status::Pass);
}

#[test]
fn doubling_a_balance_is_caught_and_the_partial_batch_already_shows_it() {
    let r = run("vault-broken-double-balance");
    assert_eq!(status(&r, "partial-migration-preserves-balances"), Status::Fail);
    assert_eq!(status(&r, "seeded-balances-preserved-after-migration"), Status::Fail);
    assert_eq!(status(&r, "migration-converts-every-entry"), Status::Fail);
    // Upgrading itself was fine and mixed-format reads before migration were fine.
    assert_eq!(status(&r, "balances-preserved-across-upgrade"), Status::Pass);
    assert_eq!(status(&r, "mixed-format-balances-correct"), Status::Pass);
    let bob = &r.checkpoints["after:p-migrate-partial"]["balance-bob"];
    assert_eq!(
        bob.value.as_ref().unwrap(),
        "500",
        "bob seeded 250 and must read back 500 only because the defect doubles it"
    );
}

#[test]
fn an_unguarded_reinitialize_is_caught_and_shows_the_takeover() {
    let r = run("vault-broken-reinit");
    assert_eq!(status(&r, "reinitialize-rejected"), Status::Fail);
    assert_eq!(status(&r, "reinitialize-changes-nothing"), Status::Fail);
    let inv = r.invariants.iter().find(|i| i.id == "reinitialize-changes-nothing").unwrap();
    assert!(inv.summary.contains("admin changed from admin to mallory"), "{}", inv.summary);
    assert!(inv.summary.contains("total-supply changed from 540 to 0"), "{}", inv.summary);
    assert_eq!(status(&r, "seeded-balances-preserved-after-migration"), Status::Pass);
}

#[test]
fn an_unauthorized_upgrade_path_is_caught_with_the_executable_swap() {
    let r = run("vault-broken-upgrade-auth");
    assert_eq!(failing(&r), vec!["unauthorized-upgrade-rejected-no-auth".to_string()]);
    let op = r.executed_ops.iter().find(|o| o.id == "attack-upgrade-no-auth").unwrap();
    assert_eq!(op.outcome.status, "ok", "the attack succeeded in the defective build");
    assert_eq!(
        op.executable_after.as_deref(),
        Some(r.wasm.old.sha256.as_str()),
        "the attacker rolled the code back to v1"
    );
    let check = r.auth_checks.iter().find(|a| a.op == "attack-upgrade-no-auth").unwrap();
    assert!(!check.rejected);
    assert_eq!(check.executable_unchanged, Some(false));
}

#[test]
fn a_non_idempotent_migration_is_caught_only_by_the_repeat_check() {
    let r = run("vault-broken-not-idempotent");
    assert_eq!(failing(&r), vec!["migration-idempotent".to_string()]);
}

#[test]
fn rejected_attacks_in_the_correct_build_leave_the_executable_alone() {
    let r = run("vault-correct");
    assert_eq!(r.auth_checks.len(), 4);
    for c in &r.auth_checks {
        assert!(c.rejected, "{c:?}");
        assert_eq!(c.executable_unchanged, Some(true), "{c:?}");
    }
    let classes: Vec<_> = r.auth_checks.iter().map(|c| c.error_class.clone().unwrap()).collect();
    assert_eq!(classes, ["auth", "auth", "auth", "contract"]);
    // Self-signed attack carried a real signature from the wrong account.
    let selfsigned = r.auth_checks.iter().find(|c| c.op == "attack-upgrade-self-auth").unwrap();
    assert_eq!(selfsigned.signers, ["mallory"]);
    assert_eq!(selfsigned.signature_scheme, "ed25519");
}

#[test]
fn observed_authorizations_are_recorded_for_signed_calls() {
    let r = run("vault-correct");
    let up = r.executed_ops.iter().find(|o| o.id == "upgrade").unwrap();
    assert_eq!(up.observed_auth.len(), 1);
    assert_eq!(up.observed_auth[0].address, "admin");
    assert_eq!(up.observed_auth[0].function, "upgrade");
    assert_eq!(up.observed_auth[0].contract, "subject");
}

#[test]
fn mixed_format_state_really_exists_at_the_checkpoint() {
    let r = run("vault-correct");
    let cp = &r.checkpoints["after:p-alice-deposit"];
    assert_eq!(cp["raw-v2-alice"].shape.as_deref(), Some("map"));
    assert_eq!(cp["raw-legacy-bob"].shape.as_deref(), Some("i128"));
    assert_eq!(cp["raw-legacy-bob"].value.as_ref().unwrap(), "250");
    // After the full migration every entry is new-format.
    let end = &r.checkpoints["after:p-migrate-rest"];
    assert_eq!(end["raw-legacy-bob"].shape.as_deref(), Some("absent"));
    assert_eq!(end["raw-v2-bob"].value.as_ref().unwrap()["amount"], "250");
}

#[test]
fn executable_hashes_in_the_report_match_the_wasm_files() {
    use sha2::{Digest, Sha256};
    let r = run("vault-correct");
    let old = std::fs::read(root().join(&r.wasm.old.path)).unwrap();
    let new = std::fs::read(root().join(&r.wasm.new.path)).unwrap();
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    assert_eq!(r.wasm.old.sha256, hex(&Sha256::digest(&old)));
    assert_eq!(r.wasm.new.sha256, hex(&Sha256::digest(&new)));
    let up = r.executed_ops.iter().find(|o| o.id == "upgrade").unwrap();
    assert_eq!(up.executable_after.as_deref(), Some(r.wasm.new.sha256.as_str()));
    let seed = r.executed_ops.iter().find(|o| o.id == "s-init").unwrap();
    assert_eq!(seed.executable_after.as_deref(), Some(r.wasm.old.sha256.as_str()));
}

#[test]
fn categories_are_reported_separately_and_native_is_not_merged() {
    let r = run("vault-correct");
    let status_of = |id: &str| r.categories.iter().find(|c| c.id == id).unwrap().status.clone();
    assert_eq!(status_of("compiled-wasm"), "executed");
    assert_eq!(status_of("native-sdk"), "not-run");
    assert_eq!(status_of("testnet-rpc"), "not-run");
    assert!(r.invariants.iter().all(|i| i.category == "compiled-wasm"));
}

#[test]
fn a_native_log_is_attached_as_its_own_category() {
    let dir = scratch("native-log");
    let log = dir.join("cargo-test.txt");
    std::fs::write(
        &log,
        "running 2 tests\ntest test::a ... ok\ntest test::b ... ok\ntest result: ok. 2 passed; 0 failed\n",
    )
    .unwrap();
    let r = upgradelab::run_host(&scenario("vault-correct"), &root(), Some(&log)).unwrap();
    let n = r.categories.iter().find(|c| c.id == "native-sdk").unwrap();
    assert_eq!(n.status, "recorded-input");
    assert_eq!(n.native.as_ref().unwrap().passed, 2);
    assert!(r.invariants.iter().all(|i| i.category == "compiled-wasm"));
}
