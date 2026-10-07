use crate::common::*;
use serde_json::{json, Value};
use upgradelab::scenario::Scenario;

fn base() -> Value {
    serde_json::from_str(&scenario_text("vault-correct")).unwrap()
}

fn reject(v: Value, needle: &str) {
    let err = Scenario::parse(&v.to_string()).expect_err("must be rejected").0;
    assert!(err.contains(needle), "`{err}` should mention `{needle}`");
}

#[test]
fn every_shipped_scenario_is_valid() {
    for n in [
        "vault-correct",
        "vault-broken-lose-balance",
        "vault-broken-double-balance",
        "vault-broken-reinit",
        "vault-broken-upgrade-auth",
        "vault-broken-not-idempotent",
    ] {
        Scenario::parse(&scenario_text(n)).unwrap_or_else(|e| panic!("{n}: {e}"));
    }
}

#[test]
fn unknown_top_level_and_nested_fields_are_rejected() {
    let mut v = base();
    v["extra"] = json!(1);
    reject(v, "unknown field");
    let mut v = base();
    v["ops"][1]["authz"] = json!(["alice"]);
    reject(v, "unknown field");
    let mut v = base();
    v["invariants"][0]["frm"] = json!("x");
    reject(v, "unknown field");
    let mut v = base();
    v["probes"][0]["fnn"] = json!("x");
    reject(v, "unknown field");
}

#[test]
fn version_must_be_1() {
    let mut v = base();
    v["scenarioVersion"] = json!(2);
    reject(v, "scenarioVersion must be 1");
}

#[test]
fn exactly_one_upgrade_op() {
    let mut v = base();
    v["ops"].as_array_mut().unwrap().retain(|o| o["id"] != "upgrade");
    reject(v, "exactly one operation");
    let mut v = base();
    let ops = v["ops"].as_array_mut().unwrap();
    let at = ops.iter().position(|o| o["id"] == "upgrade").unwrap();
    let mut second = ops[at].clone();
    second["id"] = json!("upgrade-2");
    ops.insert(at + 1, second);
    reject(v, "exactly one operation");
}

#[test]
fn phases_must_be_ordered() {
    let mut v = base();
    let ops = v["ops"].as_array_mut().unwrap();
    let first = ops.remove(0);
    ops.push(first); // a seed op after post-upgrade ops
    reject(v, "phases must be ordered");
}

#[test]
fn unknown_accounts_checkpoints_probes_and_ops_are_rejected() {
    let mut v = base();
    v["ops"][1]["auth"] = json!(["nobody"]);
    reject(v, "unknown account `nobody`");
    let mut v = base();
    v["invariants"][0]["from"] = json!("before:nope");
    reject(v, "unknown checkpoint");
    let mut v = base();
    v["invariants"][0]["probes"] = json!(["nope"]);
    reject(v, "unknown probe");
    let mut v = base();
    let i = v["invariants"].as_array().unwrap().iter().position(|i| i["kind"] == "stableAcrossOp").unwrap();
    v["invariants"][i]["op"] = json!("ghost");
    reject(v, "unknown op");
}

#[test]
fn values_need_exactly_one_field_and_args_need_a_name() {
    let mut v = base();
    v["ops"][1]["args"][0] = json!({"name": "from", "account": "alice", "u32": 1});
    reject(v, "exactly one value field");
    let mut v = base();
    v["ops"][1]["args"][0] = json!({"name": "from"});
    reject(v, "exactly one value field");
    let mut v = base();
    v["ops"][1]["args"][0] = json!({"account": "alice"});
    reject(v, "`name`");
}

#[test]
fn duplicate_ids_and_bad_function_names_are_rejected() {
    let mut v = base();
    v["ops"][2]["id"] = v["ops"][1]["id"].clone();
    reject(v, "duplicated");
    let mut v = base();
    v["ops"][1]["fn"] = json!("dep osit");
    reject(v, "fn must be");
    let mut v = base();
    v["accounts"] = json!(["a", "a"]);
    reject(v, "duplicated");
}

#[test]
fn wasm_paths_cannot_escape_the_root() {
    let mut s = scenario("vault-correct");
    s.wasm.old = "../outside.wasm".into();
    let err = upgradelab::run_host(&s, &root(), None).err().unwrap().0;
    assert!(err.contains("may not contain"), "{err}");
    s.wasm.old = "/etc/passwd".into();
    let err = upgradelab::run_host(&s, &root(), None).err().unwrap().0;
    assert!(err.contains("relative"), "{err}");
}

#[test]
fn a_file_that_is_not_wasm_is_refused() {
    let dir = scratch("not-wasm");
    std::fs::write(dir.join("a.wasm"), b"hello").unwrap();
    std::fs::write(dir.join("b.wasm"), b"hello").unwrap();
    let mut s = scenario("vault-correct");
    s.wasm.old = "a.wasm".into();
    s.wasm.new = "b.wasm".into();
    let err = upgradelab::run_host(&s, &dir, None).err().unwrap().0;
    assert!(err.contains("not a WebAssembly module"), "{err}");
}
