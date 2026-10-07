//! Backend-independent execution of a scenario and evaluation of its invariants.

use crate::backend::{Backend, RunnerError};
use crate::report::*;
use crate::scenario::{ErrorClass, Expect, ExpectedValues, Invariant, OpRejected, OpSucceeded, Phase, Preserved, Scenario, Shapes, StableAcrossOp, SumEquals};
use crate::values::hex_encode;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const LIMITS_HOST: &[&str] = &[
    "Not a network: the compiled WASM runs in the Soroban host linked into this runner (soroban-sdk testutils Env). There is no consensus, no fees, no transaction envelope, no state archival or restore, and the host's protocol version may differ from the live network's.",
    "Authorization is enforced with real ed25519 signatures over the standard payload for single-key accounts only; multi-signer accounts, thresholds and custom account contracts are not exercised.",
    "State comes from the seed operations you wrote. Live ledger state is never cloned or fetched, and storage keys are not discovered: only the probes you name are read.",
    "A pass means the named invariants held for this scenario's operations. It is not a proof that the migration is safe for all states or inputs, and not an audit.",
    "Results from native SDK tests (if attached) are a different category from compiled-WASM execution and are never merged with it.",
];

pub const LIMITS_TESTNET: &[&str] = &[
    "Testnet is a shared public test network: its protocol version and resource limits can differ from mainnet and change over time. Throwaway keys only.",
    "Only probes expressible as view calls are evaluated; raw storage probes and executable-hash checks are reported as inconclusive here (use the in-process host for them).",
    "State comes from the seed operations you wrote. Live ledger state is never cloned, and storage keys are not discovered.",
    "A pass means the named invariants held for this one run. It is not a proof that the migration is safe in general, and not an audit.",
];

pub struct RunMeta {
    pub tool: ToolInfo,
    pub scenario_sha256: String,
    pub wasm: WasmInfo,
    pub categories: Vec<Category>,
    pub network: Option<NetworkInfo>,
    pub limits: Vec<String>,
}

pub fn scenario_sha256(s: &Scenario) -> String {
    let text = serde_json::to_string(s).expect("scenario serialises");
    hex_encode(&Sha256::digest(text.as_bytes()))
}

type Checkpoint = BTreeMap<String, ProbeReading>;

struct OpTrace {
    exec_before: Option<String>,
    exec_after: Option<String>,
}

fn read_all(b: &mut dyn Backend, s: &Scenario) -> Checkpoint {
    s.probes.iter().map(|p| (p.id().to_string(), b.probe(p))).collect()
}

pub fn run(scenario: &Scenario, backend: &mut dyn Backend, meta: RunMeta) -> Result<Report, RunnerError> {
    scenario.validate().map_err(|e| RunnerError(e.0))?;
    let category = backend.category();
    let mut executed = backend.setup_steps();
    let mut seq = executed.len() as u32;
    let mut checkpoints: BTreeMap<String, Checkpoint> = BTreeMap::new();
    let mut traces: BTreeMap<String, OpTrace> = BTreeMap::new();
    let ids = identities_for_render(scenario, &meta);

    for op in &scenario.ops {
        let before = read_all(backend, scenario);
        let exec_before = backend.executable_hash();
        let rec = backend.invoke(op)?;
        let exec_after = backend.executable_hash();
        let after = read_all(backend, scenario);
        checkpoints.insert(format!("before:{}", op.id), before);
        checkpoints.insert(format!("after:{}", op.id), after);
        seq += 1;
        let ok = rec.outcome.status == "ok";
        let expectation = match op.expect {
            Expect::Any => "any",
            Expect::Ok if ok => "met",
            Expect::Error if !ok => "met",
            _ => "unmet",
        };
        let mut args = Vec::new();
        for a in &op.args {
            let sc = crate::values::to_scval(a, &ids).map_err(RunnerError)?;
            let (shape, value) = crate::values::render(&sc, &ids.names);
            args.push(ArgView { name: a.name.clone().unwrap_or_default(), shape, value });
        }
        executed.push(ExecutedOp {
            seq,
            id: op.id.clone(),
            phase: phase_str(op.phase).into(),
            function: op.function.clone(),
            args,
            signers: op.auth.clone(),
            outcome: rec.outcome,
            expectation: expectation.into(),
            observed_auth: rec.observed_auth,
            executable_after: exec_after.clone(),
            tx_hash: rec.tx_hash,
        });
        traces.insert(op.id.clone(), OpTrace { exec_before, exec_after });
    }

    let mut results = builtin_results(scenario, &executed, &traces, &meta, category);
    let mut auth_checks = Vec::new();
    for inv in &scenario.invariants {
        let (status, summary, evidence) = evaluate(inv, &checkpoints, &executed, &traces, &mut auth_checks);
        results.push(InvariantResult {
            id: inv.id().into(),
            title: inv.title().into(),
            kind: inv.kind().into(),
            status,
            category: category.into(),
            summary,
            evidence,
            builtin: false,
        });
    }

    let count = |s: Status| results.iter().filter(|r| r.status == s).count() as u32;
    let (passed, failed, inconclusive) = (count(Status::Pass), count(Status::Fail), count(Status::Inconclusive));
    let status = if failed > 0 {
        Status::Fail
    } else if inconclusive > 0 {
        Status::Inconclusive
    } else {
        Status::Pass
    };
    Ok(Report {
        report_version: REPORT_VERSION,
        kind: REPORT_KIND.into(),
        tool: meta.tool,
        scenario: ScenarioInfo { name: scenario.name.clone(), sha256: meta.scenario_sha256, definition: scenario.clone() },
        wasm: meta.wasm,
        categories: meta.categories,
        executed_ops: executed,
        checkpoints,
        auth_checks,
        invariants: results,
        verdict: Verdict { status, passed, failed, inconclusive },
        limits: meta.limits,
        network: meta.network,
    })
}

// Rendering op arguments needs the same name table the host uses; it is derived from
// the scenario alone, so no backend access is needed.
fn identities_for_render(s: &Scenario, meta: &RunMeta) -> crate::values::Identities {
    let dec = |h: &str| -> [u8; 32] {
        let mut out = [0u8; 32];
        if let Ok(b) = crate::values::hex_decode(h) {
            if b.len() == 32 {
                out.copy_from_slice(&b);
            }
        }
        out
    };
    crate::values::Identities::new(&s.accounts, dec(&meta.wasm.old.sha256), dec(&meta.wasm.new.sha256))
}

fn builtin_results(
    scenario: &Scenario,
    executed: &[ExecutedOp],
    traces: &BTreeMap<String, OpTrace>,
    meta: &RunMeta,
    category: &str,
) -> Vec<InvariantResult> {
    let mut out = Vec::new();
    // Seed preconditions: a failing seed operation means the scenario's starting
    // state was never established, which says nothing about the migration.
    let failed_seed: Vec<&str> = scenario
        .ops
        .iter()
        .filter(|o| o.phase == Phase::Seed && o.expect == Expect::Ok)
        .filter(|o| executed.iter().any(|e| e.id == o.id && e.outcome.status != "ok"))
        .map(|o| o.id.as_str())
        .collect();
    out.push(InvariantResult {
        id: "seed-operations-succeeded".into(),
        title: "Seed operations on the old WASM established the starting state".into(),
        kind: "builtin".into(),
        status: if failed_seed.is_empty() { Status::Pass } else { Status::Inconclusive },
        category: category.into(),
        summary: if failed_seed.is_empty() {
            "every seed operation succeeded".into()
        } else {
            format!("seed operation(s) failed, so the starting state was not established: {}", failed_seed.join(", "))
        },
        evidence: json!({ "failedSeedOps": failed_seed }),
        builtin: true,
    });

    let up = scenario.upgrade_op();
    let t = &traces[&up.id];
    let outcome_ok = executed.iter().find(|e| e.id == up.id).map(|e| e.outcome.status == "ok").unwrap_or(false);
    let (status, summary) = match (&t.exec_before, &t.exec_after) {
        (Some(b), Some(a)) => {
            if outcome_ok && *b == meta.wasm.old.sha256 && *a == meta.wasm.new.sha256 {
                (Status::Pass, format!("executable changed from {} to {}", short(b), short(a)))
            } else if !outcome_ok {
                (Status::Fail, "the upgrade operation was rejected or trapped".to_string())
            } else {
                (Status::Fail, format!("executable is {} after the upgrade, expected the new WASM {}", short(a), short(&meta.wasm.new.sha256)))
            }
        }
        _ => (Status::Inconclusive, "the contract's executable hash could not be read in this mode".to_string()),
    };
    out.push(InvariantResult {
        id: "upgrade-applied".into(),
        title: "The upgrade operation replaced the old WASM with the new WASM".into(),
        kind: "builtin".into(),
        status,
        category: category.into(),
        summary,
        evidence: json!({ "executableBefore": t.exec_before, "executableAfter": t.exec_after, "expectedOld": meta.wasm.old.sha256, "expectedNew": meta.wasm.new.sha256, "upgradeOutcomeOk": outcome_ok }),
        builtin: true,
    });

    let unmet: Vec<&str> = scenario
        .ops
        .iter()
        .filter(|o| o.phase == Phase::PostUpgrade && o.expect == Expect::Ok)
        .filter(|o| executed.iter().any(|e| e.id == o.id && e.outcome.status != "ok"))
        .map(|o| o.id.as_str())
        .collect();
    out.push(InvariantResult {
        id: "post-upgrade-operations-succeeded".into(),
        title: "Operations expected to succeed after the upgrade (including migration calls) succeeded".into(),
        kind: "builtin".into(),
        status: if unmet.is_empty() { Status::Pass } else { Status::Fail },
        category: category.into(),
        summary: if unmet.is_empty() { "all expected-ok post-upgrade operations succeeded".into() } else { format!("failed unexpectedly: {}", unmet.join(", ")) },
        evidence: json!({ "failedOps": unmet }),
        builtin: true,
    });
    out
}

fn short(h: &str) -> String {
    h.chars().take(12).collect()
}

fn norm(v: &Json) -> Json {
    match v {
        Json::Number(n) => Json::String(n.to_string()),
        Json::Array(a) => Json::Array(a.iter().map(norm).collect()),
        Json::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
        other => other.clone(),
    }
}

fn cp<'a>(cps: &'a BTreeMap<String, Checkpoint>, name: &str) -> &'a Checkpoint {
    cps.get(name).expect("checkpoint validated")
}

/// Returns Err(unreadable descriptions) if any needed probe could not be read.
fn readings<'a>(c: &'a Checkpoint, probes: &[&str], at: &str) -> Result<Vec<(&'a str, &'a ProbeReading)>, Vec<String>> {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for p in probes {
        match c.get_key_value(*p) {
            Some((k, r)) if r.ok => out.push((k.as_str(), r)),
            Some((_, r)) => bad.push(format!("{p} @ {at}: {}", r.error.clone().unwrap_or_default())),
            None => bad.push(format!("{p} @ {at}: not read")),
        }
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(bad)
    }
}

fn inconclusive(bad: Vec<String>) -> (Status, String, Json) {
    (Status::Inconclusive, format!("could not read: {}", bad.join("; ")), json!({ "unreadable": bad }))
}

fn show(v: &Option<Json>) -> String {
    match v {
        Some(Json::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "none".into(),
    }
}

fn evaluate(
    inv: &Invariant,
    cps: &BTreeMap<String, Checkpoint>,
    executed: &[ExecutedOp],
    traces: &BTreeMap<String, OpTrace>,
    auth_checks: &mut Vec<AuthCheck>,
) -> (Status, String, Json) {
    match inv {
        Invariant::Preserved(Preserved { from, to, probes, .. }) => {
            let ids: Vec<&str> = probes.iter().map(|s| s.as_str()).collect();
            let a = match readings(cp(cps, from), &ids, from) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let b = match readings(cp(cps, to), &ids, to) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let mut rows = Vec::new();
            let mut diffs = Vec::new();
            for ((p, x), (_, y)) in a.iter().zip(b.iter()) {
                let eq = x.value == y.value && x.shape == y.shape;
                if !eq {
                    diffs.push(format!("{p} changed from {} to {}", show(&x.value), show(&y.value)));
                }
                rows.push(json!({ "probe": p, "from": x.value, "to": y.value, "equal": eq }));
            }
            let st = if diffs.is_empty() { Status::Pass } else { Status::Fail };
            let summary = if diffs.is_empty() { format!("{} probe(s) identical between {from} and {to}", rows.len()) } else { diffs.join("; ") };
            (st, summary, json!({ "from": from, "to": to, "rows": rows }))
        }
        Invariant::SumEquals(SumEquals { at, parts, total, .. }) => {
            let mut ids: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            ids.push(total.as_str());
            let r = match readings(cp(cps, at), &ids, at) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let as_i128 = |x: &ProbeReading| -> Option<i128> { x.value.as_ref()?.as_str()?.parse().ok() };
            let mut sum: i128 = 0;
            let mut rows = Vec::new();
            for (p, x) in r.iter().take(parts.len()) {
                match as_i128(x) {
                    Some(n) => {
                        rows.push(json!({ "probe": p, "value": n.to_string() }));
                        match sum.checked_add(n) {
                            Some(s) => sum = s,
                            None => return (Status::Inconclusive, "sum overflowed i128".into(), json!({})),
                        }
                    }
                    None => return inconclusive(vec![format!("{p} @ {at}: not an i128 value")]),
                }
            }
            let Some(t) = as_i128(r[parts.len()].1) else {
                return inconclusive(vec![format!("{total} @ {at}: not an i128 value")]);
            };
            let ok = sum == t;
            let summary = if ok { format!("sum of {} parts is {sum}, equal to {total} ({t})", parts.len()) } else { format!("sum of {} parts is {sum} but {total} is {t} (difference {})", parts.len(), t - sum) };
            (if ok { Status::Pass } else { Status::Fail }, summary, json!({ "at": at, "parts": rows, "sum": sum.to_string(), "total": { "probe": total, "value": t.to_string() } }))
        }
        Invariant::ExpectedValues(ExpectedValues { at, expect, .. }) => {
            let ids: Vec<&str> = expect.keys().map(|s| s.as_str()).collect();
            let r = match readings(cp(cps, at), &ids, at) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let mut rows = Vec::new();
            let mut diffs = Vec::new();
            for (p, x) in r.iter() {
                let want = &expect[*p];
                let eq = x.value.as_ref().map(norm) == Some(norm(want));
                if !eq {
                    diffs.push(format!("{p} is {} but expected {}", show(&x.value), show(&Some(want.clone()))));
                }
                rows.push(json!({ "probe": p, "expected": want, "actual": x.value, "equal": eq }));
            }
            let summary = if diffs.is_empty() { format!("{} value(s) match expectations at {at}", rows.len()) } else { diffs.join("; ") };
            (if diffs.is_empty() { Status::Pass } else { Status::Fail }, summary, json!({ "at": at, "rows": rows }))
        }
        Invariant::Shapes(Shapes { at, expect, .. }) => {
            let ids: Vec<&str> = expect.keys().map(|s| s.as_str()).collect();
            let r = match readings(cp(cps, at), &ids, at) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let mut rows = Vec::new();
            let mut diffs = Vec::new();
            for (p, x) in r.iter() {
                let want = &expect[*p];
                let got = x.shape.clone().unwrap_or_default();
                if &got != want {
                    diffs.push(format!("{p} has shape {got}, expected {want}"));
                }
                rows.push(json!({ "probe": p, "expected": want, "actual": got, "value": x.value }));
            }
            let summary = if diffs.is_empty() { format!("{} storage shape(s) as expected at {at}", rows.len()) } else { diffs.join("; ") };
            (if diffs.is_empty() { Status::Pass } else { Status::Fail }, summary, json!({ "at": at, "rows": rows }))
        }
        Invariant::StableAcrossOp(StableAcrossOp { op, probes, .. }) => {
            let ids: Vec<&str> = probes.iter().map(|s| s.as_str()).collect();
            let bn = format!("before:{op}");
            let an = format!("after:{op}");
            let a = match readings(cp(cps, &bn), &ids, &bn) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let b = match readings(cp(cps, &an), &ids, &an) {
                Ok(r) => r,
                Err(b) => return inconclusive(b),
            };
            let outcome = executed.iter().find(|e| e.id == *op).map(|e| e.outcome.status.clone()).unwrap_or_default();
            let mut rows = Vec::new();
            let mut diffs = Vec::new();
            for ((p, x), (_, y)) in a.iter().zip(b.iter()) {
                let eq = x.value == y.value && x.shape == y.shape;
                if !eq {
                    diffs.push(format!("{p} changed from {} to {}", show(&x.value), show(&y.value)));
                }
                rows.push(json!({ "probe": p, "before": x.value, "after": y.value, "equal": eq }));
            }
            let summary = if diffs.is_empty() { format!("{} probe(s) unchanged across `{op}` (the call {})", rows.len(), if outcome == "ok" { "succeeded" } else { "was rejected" }) } else { format!("`{op}` changed state: {}", diffs.join("; ")) };
            (if diffs.is_empty() { Status::Pass } else { Status::Fail }, summary, json!({ "op": op, "opOutcome": outcome, "rows": rows }))
        }
        Invariant::OpRejected(OpRejected { id, op, class, .. }) => {
            let e = executed.iter().find(|e| e.id == *op).expect("validated");
            let t = &traces[op];
            let rejected = e.outcome.status == "error";
            let err_class = e.outcome.error.as_ref().map(|x| x.class.clone());
            let class_ok = match (class, &err_class) {
                (None, _) => true,
                (Some(c), Some(got)) => class_name(*c) == got,
                (Some(_), None) => false,
            };
            let unchanged = match (&t.exec_before, &t.exec_after) {
                (Some(a), Some(b)) => Some(a == b),
                _ => None,
            };
            auth_checks.push(AuthCheck {
                invariant: id.clone(),
                op: op.clone(),
                signers: e.signers.clone(),
                signature_scheme: if e.signers.is_empty() { "none".into() } else { "ed25519".into() },
                rejected,
                error_class: err_class.clone(),
                executable_unchanged: unchanged,
            });
            let (st, summary) = if !rejected {
                (Status::Fail, format!("`{op}` was ACCEPTED (signers: {}); it should have been rejected", if e.signers.is_empty() { "none".to_string() } else { e.signers.join(", ") }))
            } else if !class_ok {
                (Status::Fail, format!("`{op}` was rejected, but as {:?} instead of the expected class", err_class))
            } else if unchanged == Some(false) {
                (Status::Fail, format!("`{op}` was rejected yet the executable changed"))
            } else {
                let er = e.outcome.error.as_ref().unwrap();
                (Status::Pass, format!("`{op}` rejected ({} {}){}", er.class, er.host_error, if unchanged == Some(true) { "; executable unchanged" } else { "; executable hash not readable in this mode" }))
            };
            (st, summary, json!({ "op": op, "signers": e.signers, "outcome": e.outcome, "executableBefore": t.exec_before, "executableAfter": t.exec_after }))
        }
        Invariant::OpSucceeded(OpSucceeded { op, .. }) => {
            let e = executed.iter().find(|e| e.id == *op).expect("validated");
            let ok = e.outcome.status == "ok";
            (if ok { Status::Pass } else { Status::Fail }, if ok { format!("`{op}` succeeded") } else { format!("`{op}` failed: {}", e.outcome.error.as_ref().map(|x| format!("{} {}", x.class, x.host_error)).unwrap_or_default()) }, json!({ "op": op, "outcome": e.outcome }))
        }
    }
}

fn class_name(c: ErrorClass) -> &'static str {
    match c {
        ErrorClass::Auth => "auth",
        ErrorClass::Contract => "contract",
        ErrorClass::Trap => "trap",
    }
}
