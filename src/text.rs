//! Plain-text rendering of a report for terminals and CI logs.

use crate::report::*;
use std::fmt::Write;

fn mark(s: Status) -> &'static str {
    match s {
        Status::Pass => "PASS",
        Status::Fail => "FAIL",
        Status::Inconclusive => "INCONCLUSIVE",
    }
}

pub fn render(r: &Report) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "UpgradeLab report v{}: {}", r.report_version, r.scenario.name);
    let _ = writeln!(o, "mode: {} | runner {} | soroban-sdk {} | host protocol {}", r.tool.mode, r.tool.runner_version, r.tool.soroban_sdk, r.tool.host_protocol);
    let _ = writeln!(o, "old wasm: {} sha256 {}", r.wasm.old.path, r.wasm.old.sha256);
    let _ = writeln!(o, "new wasm: {} sha256 {}", r.wasm.new.path, r.wasm.new.sha256);
    let _ = writeln!(o, "scenario sha256: {}", r.scenario.sha256);
    let _ = writeln!(o, "\nExecution categories (reported separately):");
    for c in &r.categories {
        let extra = c.native.as_ref().map(|n| format!(" ({} passed, {} failed)", n.passed, n.failed)).unwrap_or_default();
        let _ = writeln!(o, "  [{}] {}: {}{}", c.status, c.id, c.title, extra);
    }
    let _ = writeln!(o, "\nExecuted operations:");
    for e in &r.executed_ops {
        let out = match (&e.outcome.status[..], &e.outcome.error) {
            ("ok", _) => match &e.outcome.value {
                Some(v) if !v.is_null() => format!("ok -> {v}"),
                _ => "ok".to_string(),
            },
            (_, Some(err)) => format!("ERROR {} {} {}", err.class, err.host_error, err.message).trim_end().to_string(),
            _ => "ERROR".to_string(),
        };
        let args: Vec<String> = e.args.iter().map(|a| format!("{}={}", a.name, a.value)).collect();
        let signers = if e.signers.is_empty() { "unsigned".to_string() } else { format!("signed by {}", e.signers.join("+")) };
        let _ = writeln!(o, "  {:>2}. [{}] {} {}({}) {} => {}", e.seq, e.phase, e.id, e.function, args.join(", "), signers, out);
    }
    let _ = writeln!(o, "\nInvariants:");
    for i in &r.invariants {
        let tag = if i.builtin { " (built-in)" } else { "" };
        let _ = writeln!(o, "  {:<12} {}{}\n               {}\n               {}", mark(i.status), i.id, tag, i.title, i.summary);
    }
    if !r.auth_checks.is_empty() {
        let _ = writeln!(o, "\nAuthorization checks:");
        for a in &r.auth_checks {
            let _ = writeln!(
                o,
                "  op {} signers [{}] rejected={} class={} executableUnchanged={}",
                a.op,
                a.signers.join(","),
                a.rejected,
                a.error_class.clone().unwrap_or_else(|| "-".into()),
                a.executable_unchanged.map(|b| b.to_string()).unwrap_or_else(|| "unknown".into())
            );
        }
    }
    let v = &r.verdict;
    let _ = writeln!(o, "\nVerdict: {} ({} passed, {} failed, {} inconclusive)", mark(v.status), v.passed, v.failed, v.inconclusive);
    let _ = writeln!(o, "\nWhat this does not show:");
    for l in &r.limits {
        let _ = writeln!(o, "  - {l}");
    }
    o
}
