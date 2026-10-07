//! UpgradeLab runner: executable Soroban migration rehearsal.
//!
//! See SPEC.md for scope and docs/adr for the design decisions.

pub mod backend;
pub mod engine;
pub mod host;
pub mod native;
pub mod report;
pub mod scenario;
pub mod testnet;
pub mod text;
pub mod values;

use backend::RunnerError;
use engine::{RunMeta, LIMITS_HOST};
use report::*;
use scenario::Scenario;
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Format {
    Text,
    Json,
}

pub const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const AUTH_ENFORCEMENT_HOST: &str = "Enforced, not mocked: no authorization is present unless a named account signs it. Each call carries SorobanAuthorizationEntry values signed with ed25519 over the standard payload (network id, nonce, expiry, invocation); the host verifies the signature against the account's ledger entry (master weight 1) and consumes the nonce. mock_all_auths and mock_auths are never used. Single-key accounts only.";

pub const AUTH_ENFORCEMENT_TESTNET: &str = "Real network authorization: transactions are built and signed by the Stellar CLI with throwaway testnet keys and simulated/submitted against the public RPC.";

fn sha256_hex(b: &[u8]) -> String {
    values::hex_encode(&Sha256::digest(b))
}

pub fn read_wasm(root: &Path, rel: &str) -> Result<(Vec<u8>, WasmArtifact), RunnerError> {
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(RunnerError(format!("wasm path `{rel}` must be relative to --root and may not contain `..`")));
    }
    let full = root.join(p);
    let bytes = std::fs::read(&full).map_err(|e| RunnerError(format!("cannot read {}: {e}", full.display())))?;
    if bytes.len() > 512 * 1024 {
        return Err(RunnerError(format!("{rel} is larger than 512 KiB")));
    }
    if !bytes.starts_with(b"\0asm") {
        return Err(RunnerError(format!("{rel} is not a WebAssembly module")));
    }
    let art = WasmArtifact { path: rel.to_string(), sha256: sha256_hex(&bytes), bytes: bytes.len() as u64 };
    Ok((bytes, art))
}

fn categories_host(native_summary: Option<NativeSummary>) -> Vec<Category> {
    let mut v = vec![Category {
        id: CAT_COMPILED.into(),
        title: "Compiled WASM in the in-process Soroban host".into(),
        status: "executed".into(),
        detail: "The old and new .wasm files were uploaded to an in-process ledger and every operation below ran inside the Soroban host VM.".into(),
        native: None,
    }];
    v.push(match native_summary {
        Some(n) => Category {
            id: CAT_NATIVE.into(),
            title: "Native SDK tests (Rust, mocked auth, not the VM)".into(),
            status: "recorded-input".into(),
            detail: format!("Not run by this runner. Parsed from a cargo test log you supplied (sha256 {}): {} passed, {} failed. These tests link the contract as native Rust and are a different category from the compiled-WASM results.", n.log_sha256, n.passed, n.failed),
            native: Some(n),
        },
        None => Category {
            id: CAT_NATIVE.into(),
            title: "Native SDK tests (Rust, mocked auth, not the VM)".into(),
            status: "not-run".into(),
            detail: "No native test log was supplied (--native-log). Native results, when present, are reported here separately and never merged into the invariants above.".into(),
            native: None,
        },
    });
    v.push(Category {
        id: CAT_TESTNET.into(),
        title: "Stellar testnet".into(),
        status: "not-run".into(),
        detail: "Opt-in `upgradelab testnet` mode; not used for this report.".into(),
        native: None,
    });
    v
}

/// Run a scenario in the in-process host and return the report.
pub fn run_host(scenario: &Scenario, root: &Path, native_log: Option<&Path>) -> Result<Report, RunnerError> {
    scenario.validate().map_err(|e| RunnerError(e.0))?;
    let (old_bytes, old) = read_wasm(root, &scenario.wasm.old)?;
    let (new_bytes, new) = read_wasm(root, &scenario.wasm.new)?;
    let native_summary = match native_log {
        Some(p) => {
            let text =
                std::fs::read_to_string(p).map_err(|e| RunnerError(format!("cannot read {}: {e}", p.display())))?;
            Some(native::parse_log(&text))
        }
        None => None,
    };
    let mut backend = host::HostBackend::new(&scenario.accounts, &old_bytes, &new_bytes)?;
    let meta = RunMeta {
        tool: ToolInfo {
            name: "upgradelab-runner".into(),
            runner_version: RUNNER_VERSION.into(),
            soroban_sdk: host::SOROBAN_SDK_VERSION.into(),
            host_protocol: backend.protocol,
            mode: "in-process-host".into(),
            auth_enforcement: AUTH_ENFORCEMENT_HOST.into(),
        },
        scenario_sha256: engine::scenario_sha256(scenario),
        wasm: WasmInfo { old, new },
        categories: categories_host(native_summary),
        network: None,
        limits: LIMITS_HOST.iter().map(|s| s.to_string()).collect(),
    };
    engine::run(scenario, &mut backend, meta)
}

/// Re-run the scenario embedded in `report` and list every path where the new report
/// differs from the old one. An empty list means the report reproduced exactly.
pub fn replay(report: &Report, root: &Path, native_log: Option<&Path>) -> Result<(Report, Vec<String>), RunnerError> {
    if report.tool.mode != "in-process-host" {
        return Err(RunnerError(
            "only in-process-host reports can be replayed; testnet reports record a network run".into(),
        ));
    }
    let fresh = run_host(&report.scenario.definition, root, native_log)?;
    let a = serde_json::to_value(report).unwrap();
    let b = serde_json::to_value(&fresh).unwrap();
    let mut diffs = Vec::new();
    diff_json("", &a, &b, &mut diffs);
    Ok((fresh, diffs))
}

fn diff_json(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value::*;
    match (a, b) {
        (Object(x), Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                let p = format!("{path}/{k}");
                match (x.get(k), y.get(k)) {
                    (Some(l), Some(r)) => diff_json(&p, l, r, out),
                    (Some(_), None) => out.push(format!("{p}: missing in the re-run")),
                    (None, Some(_)) => out.push(format!("{p}: new in the re-run")),
                    _ => {}
                }
            }
        }
        (Array(x), Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: length {} vs {}", x.len(), y.len()));
            }
            for (i, (l, r)) in x.iter().zip(y.iter()).enumerate() {
                diff_json(&format!("{path}/{i}"), l, r, out);
            }
        }
        (l, r) if l != r => out.push(format!("{path}: {l} vs {r}")),
        _ => {}
    }
}
