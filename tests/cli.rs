//! Exit codes are part of the CLI contract (see README).
mod common;
use common::*;
use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_upgradelab"));
    c.current_dir(root());
    c
}

fn code(c: &mut Command) -> (i32, String, String) {
    let o = c.output().unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn exit_0_when_every_invariant_passes() {
    let (c, out, _) = code(bin().args(["run", "scenarios/vault-correct.json"]));
    assert_eq!(c, 0);
    assert!(out.contains("Verdict: PASS"));
}

#[test]
fn exit_1_when_an_invariant_fails() {
    let (c, out, _) = code(bin().args(["run", "scenarios/vault-broken-lose-balance.json"]));
    assert_eq!(c, 1);
    assert!(out.contains("Verdict: FAIL"));
    assert!(out.contains("balance-dave is 0 but expected 40"));
}

#[test]
fn exit_2_for_an_invalid_scenario() {
    let dir = scratch("cli-bad");
    let p = dir.join("bad.json");
    std::fs::write(&p, r#"{"scenarioVersion": 1, "nonsense": true}"#).unwrap();
    let (c, _, err) = code(bin().args(["run", p.to_str().unwrap()]));
    assert_eq!(c, 2);
    assert!(err.contains("invalid scenario"));
    let (c, _, _) = code(bin().args(["validate", p.to_str().unwrap()]));
    assert_eq!(c, 2);
    let (c, _, _) = code(bin().args(["validate", "scenarios/vault-correct.json"]));
    assert_eq!(c, 0);
}

#[test]
fn exit_2_for_an_unusable_report() {
    let dir = scratch("cli-badreport");
    let p = dir.join("r.json");
    std::fs::write(&p, "{}").unwrap();
    let (c, _, _) = code(bin().args(["check-report", p.to_str().unwrap()]));
    assert_eq!(c, 2);
    let (c, _, _) = code(bin().args(["replay", p.to_str().unwrap()]));
    assert_eq!(c, 2);
}

#[test]
fn exit_3_when_evidence_is_missing() {
    let dir = scratch("cli-inconclusive");
    let mut v: serde_json::Value = serde_json::from_str(&scenario_text("vault-correct")).unwrap();
    v["probes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"kind": "call", "id": "schema", "fn": "schema_version", "args": []}));
    v["invariants"].as_array_mut().unwrap().retain(|i| i["kind"] != "opRejected" && i["kind"] != "stableAcrossOp");
    v["invariants"].as_array_mut().unwrap().push(serde_json::json!({"kind": "preserved", "id": "schema-same", "title": "schema", "from": "before:upgrade", "to": "after:upgrade", "probes": ["schema"]}));
    let p = dir.join("s.json");
    std::fs::write(&p, v.to_string()).unwrap();
    let (c, out, _) = code(bin().args(["run", p.to_str().unwrap()]));
    assert_eq!(c, 3, "{out}");
    assert!(out.contains("INCONCLUSIVE"));
}

#[test]
fn exit_4_when_a_wasm_file_is_missing() {
    let dir = scratch("cli-nowasm");
    let (c, _, err) = code(bin().args(["run", "scenarios/vault-correct.json", "--root", dir.to_str().unwrap()]));
    assert_eq!(c, 4);
    assert!(err.contains("cannot read"));
}

#[test]
fn json_output_is_the_report_and_out_writes_the_same_bytes() {
    let dir = scratch("cli-json");
    let out_file = dir.join("report.json");
    let (c, stdout, _) = code(bin().args([
        "run",
        "scenarios/vault-correct.json",
        "--format",
        "json",
        "--out",
        out_file.to_str().unwrap(),
    ]));
    assert_eq!(c, 0);
    assert_eq!(stdout, std::fs::read_to_string(&out_file).unwrap());
    let (c, msg, _) = code(bin().args(["check-report", out_file.to_str().unwrap()]));
    assert_eq!(c, 0);
    assert!(msg.contains("verdict pass"));
    let (c, msg, _) = code(bin().args(["replay", out_file.to_str().unwrap()]));
    assert_eq!(c, 0);
    assert!(msg.starts_with("REPRODUCED"), "{msg}");
}

#[test]
fn replay_exits_1_and_lists_differences_when_the_report_was_altered() {
    let dir = scratch("cli-replay-diff");
    let out_file = dir.join("report.json");
    code(bin().args(["run", "scenarios/vault-correct.json", "--format", "json", "--out", out_file.to_str().unwrap()]));
    let text = std::fs::read_to_string(&out_file).unwrap().replacen("\"deposits\": 0", "\"deposits\": 7", 1);
    std::fs::write(&out_file, text).unwrap();
    let (c, msg, _) = code(bin().args(["replay", out_file.to_str().unwrap()]));
    assert_eq!(c, 1);
    assert!(msg.starts_with("DIFFERENT"), "{msg}");
}

#[test]
fn schema_commands_print_the_committed_schemas() {
    let (c, out, _) = code(bin().args(["schema", "report"]));
    assert_eq!(c, 0);
    assert_eq!(
        out,
        std::fs::read_to_string(root().join("schema/report.v1.schema.json")).unwrap(),
        "run: cargo run -q -- schema report > schema/report.v1.schema.json"
    );
    let (_, out, _) = code(bin().args(["schema", "scenario"]));
    assert_eq!(
        out,
        std::fs::read_to_string(root().join("schema/scenario.v1.schema.json")).unwrap(),
        "run: cargo run -q -- schema scenario > schema/scenario.v1.schema.json"
    );
}
