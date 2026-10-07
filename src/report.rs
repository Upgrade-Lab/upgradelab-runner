//! The replayable report (version 1). Contains no timestamps or machine paths, so
//! re-running the same scenario against the same WASM reproduces it byte for byte.

use crate::scenario::{Expect, Phase, Scenario};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::BTreeMap;

pub const REPORT_VERSION: u32 = 1;
pub const REPORT_KIND: &str = "upgradelab-report";
pub const CAT_COMPILED: &str = "compiled-wasm";
pub const CAT_TESTNET: &str = "testnet-rpc";
pub const CAT_NATIVE: &str = "native-sdk";

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub report_version: u32,
    /// Always "upgradelab-report".
    pub kind: String,
    pub tool: ToolInfo,
    pub scenario: ScenarioInfo,
    pub wasm: WasmInfo,
    /// Execution categories, kept distinct on purpose. Only the category that has
    /// status "executed" produced results in this report.
    pub categories: Vec<Category>,
    pub executed_ops: Vec<ExecutedOp>,
    /// Probe readouts keyed by `before:<opId>` / `after:<opId>`.
    pub checkpoints: BTreeMap<String, BTreeMap<String, ProbeReading>>,
    pub auth_checks: Vec<AuthCheck>,
    pub invariants: Vec<InvariantResult>,
    pub verdict: Verdict,
    /// What this report does not establish.
    pub limits: Vec<String>,
    /// Present only for testnet runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkInfo>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub runner_version: String,
    pub soroban_sdk: String,
    /// Ledger protocol version the in-process host reported (the host linked into
    /// this runner), or the version reported by the RPC for testnet runs.
    pub host_protocol: u32,
    /// "in-process-host" or "testnet".
    pub mode: String,
    /// What authorization enforcement means in this run, stated precisely.
    pub auth_enforcement: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioInfo {
    pub name: String,
    /// sha256 of the canonical (re-serialised) scenario JSON.
    pub sha256: String,
    /// The full scenario, embedded so the report can be replayed.
    pub definition: Scenario,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WasmArtifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WasmInfo {
    pub old: WasmArtifact,
    pub new: WasmArtifact,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    /// "compiled-wasm", "testnet-rpc" or "native-sdk".
    pub id: String,
    pub title: String,
    /// "executed", "recorded-input" or "not-run".
    pub status: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeSummary>,
}

/// Parsed from a `cargo test` log that you supply with --native-log. The runner does
/// not execute native tests itself; it records what the log says, with its hash.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeSummary {
    pub log_sha256: String,
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub tests: Vec<NativeTest>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeTest {
    pub name: String,
    pub result: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArgView {
    pub name: String,
    pub shape: String,
    pub value: Json,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    /// "auth", "contract", "trap" or "other".
    pub class: String,
    /// The host error, for example "Auth(InvalidAction)" or "Contract(1)".
    pub host_error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_code: Option<u32>,
    pub message: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// "ok" or "error".
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObservedAuth {
    pub address: String,
    pub contract: String,
    #[serde(rename = "fn")]
    pub function: String,
    pub args: Vec<Json>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutedOp {
    pub seq: u32,
    pub id: String,
    /// "seed", "upgrade", "postUpgrade", or "system" for runner-performed steps.
    pub phase: String,
    #[serde(rename = "fn")]
    pub function: String,
    pub args: Vec<ArgView>,
    /// Accounts that signed this call.
    pub signers: Vec<String>,
    pub outcome: Outcome,
    /// "met", "unmet" or "any".
    pub expectation: String,
    /// Authorizations the host recorded as consumed by this call.
    pub observed_auth: Vec<ObservedAuth>,
    /// sha256 of the contract's executable after this operation, when readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReading {
    /// False when the read itself failed; then `error` is set. A failed read is
    /// never treated as a mismatch or a pass: it makes the invariant inconclusive.
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthCheck {
    pub invariant: String,
    pub op: String,
    pub signers: Vec<String>,
    pub signature_scheme: String,
    pub rejected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    /// True/false when the executable hash could be read before and after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_unchanged: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Pass,
    Fail,
    Inconclusive,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InvariantResult {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: Status,
    /// Which execution category produced this result.
    pub category: String,
    /// One line a human can act on.
    pub summary: String,
    /// The values that were compared.
    pub evidence: Json,
    /// True for invariants the runner adds itself (not written in the scenario).
    pub builtin: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub status: Status,
    pub passed: u32,
    pub failed: u32,
    pub inconclusive: u32,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub network: String,
    pub rpc_url: String,
    pub contract_id: String,
    pub wasm_hash_old: String,
    pub wasm_hash_new: String,
    /// Accounts are throwaway testnet keys; their public keys.
    pub accounts: BTreeMap<String, String>,
}

pub fn phase_str(p: Phase) -> &'static str {
    match p {
        Phase::Seed => "seed",
        Phase::Upgrade => "upgrade",
        Phase::PostUpgrade => "postUpgrade",
    }
}

pub fn expect_str(e: Expect) -> &'static str {
    match e {
        Expect::Ok => "ok",
        Expect::Error => "error",
        Expect::Any => "any",
    }
}

impl Report {
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("report serialises");
        s.push('\n');
        s
    }

    pub fn exit_code(&self) -> i32 {
        match self.verdict.status {
            Status::Pass => 0,
            Status::Fail => 1,
            Status::Inconclusive => 3,
        }
    }
}

pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Report);
    let mut v = serde_json::to_value(&schema).expect("schema serialises");
    if let Some(o) = v.as_object_mut() {
        o.insert("title".into(), Json::String("UpgradeLab report v1".into()));
    }
    let mut s = serde_json::to_string_pretty(&v).unwrap();
    s.push('\n');
    s
}

pub fn scenario_schema_json() -> String {
    let schema = schemars::schema_for!(Scenario);
    let mut v = serde_json::to_value(&schema).expect("schema serialises");
    if let Some(o) = v.as_object_mut() {
        o.insert("title".into(), Json::String("UpgradeLab scenario v1".into()));
    }
    let mut s = serde_json::to_string_pretty(&v).unwrap();
    s.push('\n');
    s
}
