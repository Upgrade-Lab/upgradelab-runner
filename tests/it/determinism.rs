use crate::common::*;

#[test]
fn the_same_scenario_produces_a_byte_identical_report() {
    let a = run("vault-correct").to_json();
    let b = run("vault-correct").to_json();
    assert_eq!(a, b);
    let c = run("vault-broken-lose-balance").to_json();
    let d = run("vault-broken-lose-balance").to_json();
    assert_eq!(c, d);
    assert_ne!(a, c);
}

#[test]
fn a_report_contains_no_clock_or_machine_path() {
    let j = run("vault-correct").to_json();
    for needle in ["/home/", "\\\\Users\\\\", "timestamp", "generatedAt"] {
        assert!(!j.contains(needle), "report leaks `{needle}`");
    }
}

#[test]
fn replay_reproduces_a_recorded_report() {
    let report = run("vault-broken-double-balance");
    let (fresh, diffs) = upgradelab::replay(&report, &root(), None).unwrap();
    assert!(diffs.is_empty(), "{diffs:?}");
    assert_eq!(fresh.to_json(), report.to_json());
}

#[test]
fn replay_survives_a_json_round_trip_of_the_report() {
    let report = run("vault-correct");
    let parsed: upgradelab::report::Report = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(parsed, report);
    let (_, diffs) = upgradelab::replay(&parsed, &root(), None).unwrap();
    assert!(diffs.is_empty());
}

#[test]
fn replay_reports_every_difference_when_the_wasm_changed() {
    let report = run("vault-correct");
    // Copy the tree's WASM into a scratch root, but swap the new WASM for a broken one.
    let dir = scratch("replay-swapped");
    std::fs::create_dir_all(dir.join("fixtures/wasm")).unwrap();
    for f in ["vault_v1.wasm", "vault_v2_broken_lose_balance.wasm"] {
        std::fs::copy(root().join("fixtures/wasm").join(f), dir.join("fixtures/wasm").join(f)).unwrap();
    }
    std::fs::copy(
        root().join("fixtures/wasm/vault_v2_broken_lose_balance.wasm"),
        dir.join("fixtures/wasm/vault_v2_correct.wasm"),
    )
    .unwrap();
    let (fresh, diffs) = upgradelab::replay(&report, &dir, None).unwrap();
    assert!(!diffs.is_empty());
    assert!(diffs.iter().any(|d| d.starts_with("/wasm/new/sha256")), "{diffs:?}");
    assert_eq!(fresh.verdict.status, upgradelab::report::Status::Fail);
}

#[test]
fn replay_detects_a_tampered_report() {
    let mut report = run("vault-correct");
    report.invariants[3].summary = "tampered".into();
    let (_, diffs) = upgradelab::replay(&report, &root(), None).unwrap();
    assert_eq!(diffs.len(), 1);
    assert!(diffs[0].contains("summary"));
}

#[test]
fn contract_and_account_identities_are_stable_across_processes() {
    // Pinned values, computed independently with Python (sha256 + strkey + ed25519): if these change, every committed report would stop replaying.
    let ids = upgradelab::values::Identities::new(&["alice".to_string()], [0; 32], [0; 32]);
    assert_eq!(ids.contract_strkey(), "CCUASUGBECA42OLBI5XF5MHDRY6ECOAMMLEYNMRLGZTTUITOYDZUJLUZ");
    assert_eq!(ids.account_strkey("alice"), "GC54G35LIXEXRQ6WIAZHQ7DAQXCGNLU2JMJPXXCSOJ2U2FLMWVBMKD75");
}

#[test]
fn every_committed_evidence_report_still_replays_identically() {
    let dir = root().join("evidence/host");
    let native = root().join("evidence/native/cargo-test-fixtures.txt");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if !p.to_string_lossy().ends_with(".report.json") {
            continue;
        }
        let report: upgradelab::report::Report = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let log = report.categories.iter().any(|c| c.native.is_some()).then_some(native.as_path());
        let (_, diffs) = upgradelab::replay(&report, &root(), log).unwrap();
        assert!(diffs.is_empty(), "{}: {diffs:?}", p.display());
        n += 1;
    }
    assert_eq!(n, 6);
}
