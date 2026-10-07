//! Scenario file (version 1): everything the application team supplies.
//!
//! The runner knows nothing about vaults. The application provides the seed
//! operations, the upgrade, the migration calls, the probes that read state, and the
//! named invariants. A scenario is plain JSON; unknown fields are rejected.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SCENARIO_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Scenario {
    /// Must be 1.
    pub scenario_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub wasm: WasmRefs,
    /// Named test accounts. Each gets a deterministic ed25519 key derived from its
    /// name; the key and its account exist only inside the in-process ledger.
    pub accounts: Vec<String>,
    /// Operations in execution order: seed operations on the old WASM, exactly one
    /// upgrade operation, then post-upgrade operations on the new WASM.
    pub ops: Vec<Op>,
    /// Named reads of contract state, evaluated before and after every operation.
    pub probes: Vec<Probe>,
    pub invariants: Vec<Invariant>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WasmRefs {
    /// Path of the old (currently deployed) WASM, relative to `--root`.
    pub old: String,
    /// Path of the new WASM, relative to `--root`.
    pub new: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Seed,
    Upgrade,
    PostUpgrade,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Expect {
    /// The call must succeed (default). A failing seed operation aborts the run.
    #[default]
    Ok,
    /// The call is expected to be rejected (used for attack attempts).
    Error,
    /// Either outcome is acceptable; invariants judge the result.
    Any,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Op {
    pub id: String,
    pub phase: Phase,
    /// Contract function to invoke.
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub args: Vec<Value>,
    /// Accounts that sign the authorization of this call's root invocation with a
    /// real ed25519 signature. Empty means the call carries no authorization at all.
    #[serde(default)]
    pub auth: Vec<String>,
    #[serde(default)]
    pub expect: Expect,
    /// Marks the operation that replaces the contract's WASM. Exactly one.
    #[serde(default)]
    pub upgrade: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One typed value. Exactly one of the value fields must be set. `name` is the
/// contract parameter name; it is required for operation and probe arguments (the
/// testnet mode passes it to the Stellar CLI) and ignored inside nested values.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Value {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Address of a named test account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Address of the deployed contract under test (the only allowed value is "subject").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// Decimal string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i128: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub u128: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub u64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub u32: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i32: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bool: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string: Option<String>,
    /// Hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    /// The sha256 hash of the "old" or "new" WASM as uploaded to the ledger (BytesN<32>).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_hash: Option<WhichWasm>,
    /// A `#[contracttype]` enum variant: a vec of [symbol, ...values].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vec: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WhichWasm {
    Old,
    New,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Variant {
    pub name: String,
    #[serde(default)]
    pub values: Vec<Value>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Durability {
    Persistent,
    Instance,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Probe {
    /// Read through the contract: call a view function.
    Call(CallProbe),
    /// Read a raw ledger entry of the contract by storage key, bypassing the
    /// contract's own code. Only works in the in-process host.
    Storage(StorageProbe),
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CallProbe {
    pub id: String,
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub args: Vec<Value>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StorageProbe {
    pub id: String,
    pub durability: Durability,
    pub key: Value,
}

impl Probe {
    pub fn id(&self) -> &str {
        match self {
            Probe::Call(p) => &p.id,
            Probe::Storage(p) => &p.id,
        }
    }
}

/// A checkpoint is the full probe readout at a moment. `before:<opId>` and
/// `after:<opId>` exist for every operation.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Invariant {
    /// Every listed probe has the same value at two checkpoints.
    Preserved(Preserved),
    /// At a checkpoint, the sum of the `parts` probes equals the `total` probe (i128).
    SumEquals(SumEquals),
    /// At a checkpoint, each probe's value equals an expected value written by the
    /// application team (derived from the seed operations, not from the contract).
    ExpectedValues(ExpectedValues),
    /// At a checkpoint, each probe has the stated storage shape (for example "i128"
    /// for a legacy entry, "map" for a new struct, "absent" for a removed key).
    Shapes(Shapes),
    /// The listed probes are identical before and after an operation (idempotence,
    /// or "a rejected/duplicate call changes nothing").
    StableAcrossOp(StableAcrossOp),
    /// The operation must have been rejected (and, in the host, must not have changed
    /// the contract's executable). `class` optionally pins the kind of rejection.
    OpRejected(OpRejected),
    /// The operation must have succeeded.
    OpSucceeded(OpSucceeded),
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Preserved {
    pub id: String,
    pub title: String,
    pub from: String,
    pub to: String,
    pub probes: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SumEquals {
    pub id: String,
    pub title: String,
    pub at: String,
    pub parts: Vec<String>,
    pub total: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExpectedValues {
    pub id: String,
    pub title: String,
    pub at: String,
    pub expect: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Shapes {
    pub id: String,
    pub title: String,
    pub at: String,
    pub expect: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StableAcrossOp {
    pub id: String,
    pub title: String,
    pub op: String,
    pub probes: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpRejected {
    pub id: String,
    pub title: String,
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<ErrorClass>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpSucceeded {
    pub id: String,
    pub title: String,
    pub op: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorClass {
    Auth,
    Contract,
    Trap,
}

impl Invariant {
    pub fn id(&self) -> &str {
        match self {
            Invariant::Preserved(x) => &x.id,
            Invariant::SumEquals(x) => &x.id,
            Invariant::ExpectedValues(x) => &x.id,
            Invariant::Shapes(x) => &x.id,
            Invariant::StableAcrossOp(x) => &x.id,
            Invariant::OpRejected(x) => &x.id,
            Invariant::OpSucceeded(x) => &x.id,
        }
    }
    pub fn title(&self) -> &str {
        match self {
            Invariant::Preserved(x) => &x.title,
            Invariant::SumEquals(x) => &x.title,
            Invariant::ExpectedValues(x) => &x.title,
            Invariant::Shapes(x) => &x.title,
            Invariant::StableAcrossOp(x) => &x.title,
            Invariant::OpRejected(x) => &x.title,
            Invariant::OpSucceeded(x) => &x.title,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Invariant::Preserved(_) => "preserved",
            Invariant::SumEquals(_) => "sumEquals",
            Invariant::ExpectedValues(_) => "expectedValues",
            Invariant::Shapes(_) => "shapes",
            Invariant::StableAcrossOp(_) => "stableAcrossOp",
            Invariant::OpRejected(_) => "opRejected",
            Invariant::OpSucceeded(_) => "opSucceeded",
        }
    }
}

#[derive(Debug)]
pub struct ScenarioError(pub String);
impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ScenarioError {}

fn err<T>(m: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError(m.into()))
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Value {
    pub fn set_fields(&self) -> usize {
        [
            self.account.is_some(),
            self.contract.is_some(),
            self.i128.is_some(),
            self.u128.is_some(),
            self.i64.is_some(),
            self.u64.is_some(),
            self.u32.is_some(),
            self.i32.is_some(),
            self.bool.is_some(),
            self.symbol.is_some(),
            self.string.is_some(),
            self.bytes.is_some(),
            self.wasm_hash.is_some(),
            self.variant.is_some(),
            self.vec.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }
}

fn check_value(v: &Value, accounts: &BTreeSet<&str>, ctx: &str, need_name: bool) -> Result<(), ScenarioError> {
    if v.set_fields() != 1 {
        return err(format!("{ctx}: exactly one value field must be set, found {}", v.set_fields()));
    }
    if need_name && v.name.as_deref().map_or(true, |n| !is_ident(n)) {
        return err(format!("{ctx}: a `name` (the contract parameter name) is required"));
    }
    if let Some(a) = &v.account {
        if !accounts.contains(a.as_str()) {
            return err(format!("{ctx}: unknown account `{a}`"));
        }
    }
    if let Some(c) = &v.contract {
        if c != "subject" {
            return err(format!("{ctx}: `contract` must be \"subject\""));
        }
    }
    if let Some(var) = &v.variant {
        if var.name.is_empty() {
            return err(format!("{ctx}: variant name is empty"));
        }
        for (i, x) in var.values.iter().enumerate() {
            check_value(x, accounts, &format!("{ctx}.variant.values[{i}]"), false)?;
        }
    }
    if let Some(items) = &v.vec {
        for (i, x) in items.iter().enumerate() {
            check_value(x, accounts, &format!("{ctx}.vec[{i}]"), false)?;
        }
    }
    Ok(())
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario, ScenarioError> {
        let s: Scenario = serde_json::from_str(text).map_err(|e| ScenarioError(format!("scenario JSON: {e}")))?;
        s.validate()?;
        Ok(s)
    }

    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.scenario_version != SCENARIO_VERSION {
            return err(format!("scenarioVersion must be {SCENARIO_VERSION}, got {}", self.scenario_version));
        }
        if self.name.trim().is_empty() {
            return err("name is empty");
        }
        if self.accounts.is_empty() || self.accounts.len() > 32 {
            return err("accounts must list between 1 and 32 names");
        }
        let mut accounts = BTreeSet::new();
        for a in &self.accounts {
            if !is_ident(a) || !accounts.insert(a.as_str()) {
                return err(format!("account name `{a}` is invalid or duplicated"));
            }
        }
        if self.ops.is_empty() || self.ops.len() > 256 {
            return err("ops must contain between 1 and 256 operations");
        }
        let mut op_ids = BTreeSet::new();
        let mut upgrades = 0;
        let mut last_phase = 0;
        for (i, op) in self.ops.iter().enumerate() {
            let ctx = format!("ops[{i}] `{}`", op.id);
            if !is_ident(&op.id) || !op_ids.insert(op.id.as_str()) {
                return err(format!("{ctx}: id is invalid or duplicated"));
            }
            if op.function.is_empty() || op.function.len() > 32 || !op.function.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return err(format!("{ctx}: fn must be 1..=32 characters from [A-Za-z0-9_]"));
            }
            let rank = match op.phase {
                Phase::Seed => 0,
                Phase::Upgrade => 1,
                Phase::PostUpgrade => 2,
            };
            if rank < last_phase {
                return err(format!("{ctx}: phases must be ordered seed, upgrade, postUpgrade"));
            }
            last_phase = rank;
            if op.upgrade {
                upgrades += 1;
                if op.phase != Phase::Upgrade {
                    return err(format!("{ctx}: the upgrade operation must have phase `upgrade`"));
                }
            } else if op.phase == Phase::Upgrade {
                return err(format!("{ctx}: phase `upgrade` is reserved for the operation with upgrade=true"));
            }
            for (j, a) in op.args.iter().enumerate() {
                check_value(a, &accounts, &format!("{ctx}.args[{j}]"), true)?;
            }
            for a in &op.auth {
                if !accounts.contains(a.as_str()) {
                    return err(format!("{ctx}: auth names unknown account `{a}`"));
                }
            }
        }
        if upgrades != 1 {
            return err(format!("exactly one operation must set upgrade=true, found {upgrades}"));
        }
        let mut probe_ids = BTreeSet::new();
        for p in &self.probes {
            if !is_ident(p.id()) || !probe_ids.insert(p.id()) {
                return err(format!("probe id `{}` is invalid or duplicated", p.id()));
            }
            match p {
                Probe::Call(c) => {
                    if !c.function.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') || c.function.is_empty() {
                        return err(format!("probe `{}`: fn must match [A-Za-z0-9_]+", c.id));
                    }
                    for (j, a) in c.args.iter().enumerate() {
                        check_value(a, &accounts, &format!("probe `{}`.args[{j}]", c.id), true)?;
                    }
                }
                Probe::Storage(sp) => check_value(&sp.key, &accounts, &format!("probe `{}`.key", sp.id), false)?,
            }
        }
        let cp_ok = |c: &str| -> bool {
            c.strip_prefix("before:")
                .or_else(|| c.strip_prefix("after:"))
                .map_or(false, |op| op_ids.contains(op))
        };
        let mut inv_ids = BTreeSet::new();
        let probe_known = |p: &String, inv: &str| -> Result<(), ScenarioError> {
            if probe_ids.contains(p.as_str()) {
                Ok(())
            } else {
                err(format!("invariant `{inv}`: unknown probe `{p}`"))
            }
        };
        for inv in &self.invariants {
            let id = inv.id();
            if !is_ident(id) || !inv_ids.insert(id) {
                return err(format!("invariant id `{id}` is invalid or duplicated"));
            }
            if inv.title().trim().is_empty() {
                return err(format!("invariant `{id}`: title is empty"));
            }
            let need_cp = |c: &str| -> Result<(), ScenarioError> {
                if cp_ok(c) {
                    Ok(())
                } else {
                    err(format!("invariant `{id}`: unknown checkpoint `{c}` (use before:<opId> or after:<opId>)"))
                }
            };
            let need_op = |o: &str| -> Result<(), ScenarioError> {
                if op_ids.contains(o) {
                    Ok(())
                } else {
                    err(format!("invariant `{id}`: unknown op `{o}`"))
                }
            };
            match inv {
                Invariant::Preserved(x) => {
                    need_cp(&x.from)?;
                    need_cp(&x.to)?;
                    x.probes.iter().try_for_each(|p| probe_known(p, id))?;
                    if x.probes.is_empty() {
                        return err(format!("invariant `{id}`: probes is empty"));
                    }
                }
                Invariant::SumEquals(x) => {
                    need_cp(&x.at)?;
                    x.parts.iter().try_for_each(|p| probe_known(p, id))?;
                    probe_known(&x.total, id)?;
                    if x.parts.is_empty() {
                        return err(format!("invariant `{id}`: parts is empty"));
                    }
                }
                Invariant::ExpectedValues(x) => {
                    need_cp(&x.at)?;
                    x.expect.keys().try_for_each(|p| probe_known(p, id))?;
                    if x.expect.is_empty() {
                        return err(format!("invariant `{id}`: expect is empty"));
                    }
                }
                Invariant::Shapes(x) => {
                    need_cp(&x.at)?;
                    x.expect.keys().try_for_each(|p| probe_known(p, id))?;
                    if x.expect.is_empty() {
                        return err(format!("invariant `{id}`: expect is empty"));
                    }
                }
                Invariant::StableAcrossOp(x) => {
                    need_op(&x.op)?;
                    x.probes.iter().try_for_each(|p| probe_known(p, id))?;
                    if x.probes.is_empty() {
                        return err(format!("invariant `{id}`: probes is empty"));
                    }
                }
                Invariant::OpRejected(x) => need_op(&x.op)?,
                Invariant::OpSucceeded(x) => need_op(&x.op)?,
            }
        }
        Ok(())
    }

    pub fn upgrade_op(&self) -> &Op {
        self.ops.iter().find(|o| o.upgrade).expect("validated")
    }
}
