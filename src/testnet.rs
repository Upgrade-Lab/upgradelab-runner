//! Opt-in testnet backend. Drives the real Stellar CLI against the public testnet RPC.
//!
//! This is a second, distinct execution category ("testnet-rpc"). It is not
//! replayable (the network is shared and mutable), needs network access and throwaway
//! funded keys, and cannot evaluate raw-storage probes. The only network allowed is
//! `testnet`; mainnet is refused. Keys live in the Stellar CLI's own key store under
//! aliases, never in this repository.

use crate::backend::{Backend, InvokeRecord, RunnerError};
use crate::engine::{self, RunMeta, LIMITS_TESTNET};
use crate::report::*;
use crate::scenario::{Op, Probe, Scenario, Value, WhichWasm};
use crate::values::hex_encode;
use clap::Args;
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NETWORK: &str = "testnet";
pub const RPC_URL: &str = "https://soroban-testnet.stellar.org";

#[derive(Args)]
pub struct TestnetArgs {
    pub scenario: PathBuf,
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Write the JSON report here.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Prefix for the throwaway key aliases created in the Stellar CLI key store.
    #[arg(long, default_value = "ul")]
    pub key_prefix: String,
    /// Network name. Only `testnet` is accepted.
    #[arg(long, default_value = "testnet")]
    pub network: String,
    /// Path or name of the Stellar CLI.
    #[arg(long, default_value = "stellar")]
    pub stellar_bin: String,
    /// Account used as transaction source for operations that carry no authorization.
    #[arg(long, default_value = "mallory")]
    pub anonymous_source: String,
    /// Directory for temporary downloaded WASM (defaults to the system temp dir).
    #[arg(long)]
    pub scratch: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "text")]
    pub format: crate::Format,
}

struct CliOut {
    ok: bool,
    stdout: String,
    stderr: String,
}

pub struct TestnetBackend {
    bin: String,
    contract_id: String,
    aliases: BTreeMap<String, String>,
    addresses: BTreeMap<String, String>,
    names: BTreeMap<String, String>,
    anon: String,
    wasm_old_hash: String,
    wasm_new_hash: String,
    scratch: PathBuf,
    setup: Vec<ExecutedOp>,
    pub protocol: u32,
}

fn cli(bin: &str, args: &[&str]) -> Result<CliOut, RunnerError> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| RunnerError(format!("cannot run `{bin}`: {e} (is the Stellar CLI installed?)")))?;
    Ok(CliOut {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// The transaction hash printed by the CLI on submission.
pub fn tx_hash_from(stderr: &str) -> Option<String> {
    let after = stderr.split("/tx/").nth(1)?;
    let h: String = after.chars().take(64).collect();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

pub fn classify_stderr(stderr: &str) -> ErrorInfo {
    let line = stderr.lines().find(|l| l.contains("error")).unwrap_or("").trim();
    let line = line.trim_start_matches('\u{274c}').trim().trim_start_matches("error:").trim().to_string();
    let (class, host_error, code) = if stderr.contains("Missing signing key") {
        ("auth", "CliMissingSigningKey".to_string(), None)
    } else if let Some(i) = stderr.find("Error(Contract, #") {
        let n: String = stderr[i + 17..].chars().take_while(|c| c.is_ascii_digit()).collect();
        ("contract", format!("Contract({n})"), n.parse().ok())
    } else if stderr.contains("Error(Auth,") {
        ("auth", "Auth".to_string(), None)
    } else if stderr.contains("Error(WasmVm,") {
        ("trap", "WasmVm".to_string(), None)
    } else {
        ("other", "cli".to_string(), None)
    };
    ErrorInfo { class: class.into(), host_error, contract_code: code, message: line }
}

fn rename(j: &Json, names: &BTreeMap<String, String>) -> Json {
    match j {
        Json::String(s) => Json::String(names.get(s).cloned().unwrap_or_else(|| s.clone())),
        Json::Array(a) => Json::Array(a.iter().map(|x| rename(x, names)).collect()),
        Json::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.clone(), rename(v, names))).collect()),
        other => other.clone(),
    }
}

impl TestnetBackend {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        args: &TestnetArgs,
        scenario: &Scenario,
        wasm_old_path: &Path,
        wasm_new_path: &Path,
        old_hash: &str,
        new_hash: &str,
    ) -> Result<Self, RunnerError> {
        if args.network != NETWORK {
            return Err(RunnerError(format!(
                "network `{}` refused: this runner only talks to `{NETWORK}`",
                args.network
            )));
        }
        for op in &scenario.ops {
            if op.auth.len() > 1 {
                return Err(RunnerError(format!(
                    "op `{}` lists several signers; testnet mode supports one source account per call",
                    op.id
                )));
            }
        }
        if !scenario.accounts.contains(&args.anonymous_source) {
            return Err(RunnerError(format!("anonymous source `{}` is not a scenario account", args.anonymous_source)));
        }
        let bin = args.stellar_bin.clone();
        let mut aliases = BTreeMap::new();
        let mut addresses = BTreeMap::new();
        let mut names = BTreeMap::new();
        for a in &scenario.accounts {
            let alias = format!("{}-{}", args.key_prefix, a);
            let have = cli(&bin, &["keys", "address", &alias])?;
            let addr = if have.ok && have.stdout.starts_with('G') {
                have.stdout
            } else {
                let g = cli(&bin, &["keys", "generate", &alias, "--network", NETWORK, "--fund"])?;
                if !g.ok {
                    return Err(RunnerError(format!(
                        "could not create and fund throwaway key `{alias}`: {}",
                        g.stderr.trim()
                    )));
                }
                let r = cli(&bin, &["keys", "address", &alias])?;
                r.stdout
            };
            names.insert(addr.clone(), a.clone());
            addresses.insert(a.clone(), addr);
            aliases.insert(a.clone(), alias);
        }
        let deployer = aliases[&scenario.accounts[0]].clone();
        let mut setup = Vec::new();
        let p_old = wasm_old_path.to_string_lossy().to_string();
        let p_new = wasm_new_path.to_string_lossy().to_string();

        let up_old = cli(&bin, &["contract", "upload", "--wasm", &p_old, "--source", &deployer, "--network", NETWORK])?;
        if !up_old.ok {
            return Err(RunnerError(format!("upload of old WASM failed: {}", up_old.stderr.trim())));
        }
        if up_old.stdout != old_hash {
            return Err(RunnerError("network-reported hash of the old WASM differs from sha256 of the file".into()));
        }
        setup.push(step(1, "upload-old", "upload_contract_wasm", old_hash, tx_hash_from(&up_old.stderr)));

        let dep =
            cli(&bin, &["contract", "deploy", "--wasm-hash", old_hash, "--source", &deployer, "--network", NETWORK])?;
        if !dep.ok || !dep.stdout.starts_with('C') {
            return Err(RunnerError(format!("deploy failed: {}", dep.stderr.trim())));
        }
        let contract_id = dep.stdout.clone();
        names.insert(contract_id.clone(), "subject".to_string());
        setup.push(step(2, "deploy-old", "deploy", old_hash, tx_hash_from(&dep.stderr)));

        let up_new = cli(&bin, &["contract", "upload", "--wasm", &p_new, "--source", &deployer, "--network", NETWORK])?;
        if !up_new.ok {
            return Err(RunnerError(format!("upload of new WASM failed: {}", up_new.stderr.trim())));
        }
        if up_new.stdout != new_hash {
            return Err(RunnerError("network-reported hash of the new WASM differs from sha256 of the file".into()));
        }
        setup.push(step(3, "upload-new", "upload_contract_wasm", new_hash, tx_hash_from(&up_new.stderr)));

        let scratch = args.scratch.clone().unwrap_or_else(std::env::temp_dir);
        let protocol = rpc_protocol().unwrap_or(0);
        let mut b = TestnetBackend {
            bin,
            contract_id,
            aliases,
            addresses,
            names,
            anon: args.anonymous_source.clone(),
            wasm_old_hash: old_hash.to_string(),
            wasm_new_hash: new_hash.to_string(),
            scratch,
            setup,
            protocol,
        };
        let exec = b.executable_hash();
        for s in b.setup.iter_mut() {
            s.executable_after = exec.clone();
        }
        // Only the deploy and later steps have an executable; upload-old precedes it.
        b.setup[0].executable_after = None;
        let _ = (&b.wasm_old_hash, &b.wasm_new_hash);
        Ok(b)
    }

    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }
    pub fn addresses(&self) -> &BTreeMap<String, String> {
        &self.addresses
    }

    fn cli_value(&self, v: &Value) -> Result<String, RunnerError> {
        if let Some(a) = &v.account {
            return Ok(self.addresses[a].clone());
        }
        if v.contract.is_some() {
            return Ok(self.contract_id.clone());
        }
        if let Some(s) = v.i128.as_ref().or(v.u128.as_ref()).or(v.i64.as_ref()).or(v.u64.as_ref()) {
            return Ok(s.clone());
        }
        if let Some(n) = v.u32 {
            return Ok(n.to_string());
        }
        if let Some(n) = v.i32 {
            return Ok(n.to_string());
        }
        if let Some(b) = v.bool {
            return Ok(b.to_string());
        }
        if let Some(s) = v.symbol.as_ref().or(v.string.as_ref()).or(v.bytes.as_ref()) {
            return Ok(s.clone());
        }
        if let Some(w) = &v.wasm_hash {
            return Ok(match w {
                WhichWasm::Old => self.wasm_old_hash.clone(),
                WhichWasm::New => self.wasm_new_hash.clone(),
            });
        }
        Err(RunnerError("variant and vec values are not supported by testnet mode".into()))
    }

    fn invoke_raw(
        &self,
        source_alias: &str,
        function: &str,
        args: &[Value],
        send_yes: bool,
    ) -> Result<CliOut, RunnerError> {
        let mut owned: Vec<String> = vec![
            "contract".into(),
            "invoke".into(),
            "--id".into(),
            self.contract_id.clone(),
            "--source".into(),
            source_alias.into(),
            "--network".into(),
            NETWORK.into(),
        ];
        if send_yes {
            owned.push("--send=yes".into());
        }
        owned.push("--".into());
        owned.push(function.into());
        for a in args {
            let name = a.name.clone().unwrap_or_default();
            owned.push(format!("--{name}"));
            owned.push(self.cli_value(a)?);
        }
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        cli(&self.bin, &refs)
    }
}

fn step(seq: u32, id: &str, function: &str, hash: &str, tx: Option<String>) -> ExecutedOp {
    ExecutedOp {
        seq,
        id: id.into(),
        phase: "system".into(),
        function: function.into(),
        args: vec![ArgView { name: "wasmSha256".into(), shape: "string".into(), value: serde_json::json!(hash) }],
        signers: vec![],
        outcome: Outcome { status: "ok".into(), shape: None, value: None, error: None },
        expectation: "n/a".into(),
        observed_auth: vec![],
        executable_after: None,
        tx_hash: tx,
    }
}

fn rpc_protocol() -> Option<u32> {
    let out = Command::new("curl")
        .args([
            "-s",
            "-m",
            "20",
            "-X",
            "POST",
            "-H",
            "content-type: application/json",
            "-d",
            r#"{"jsonrpc":"2.0","id":1,"method":"getNetwork"}"#,
            RPC_URL,
        ])
        .output()
        .ok()?;
    let j: Json = serde_json::from_slice(&out.stdout).ok()?;
    j["result"]["protocolVersion"].as_u64().map(|n| n as u32)
}

impl Backend for TestnetBackend {
    fn category(&self) -> &'static str {
        CAT_TESTNET
    }

    fn setup_steps(&self) -> Vec<ExecutedOp> {
        self.setup.clone()
    }

    fn invoke(&mut self, op: &Op) -> Result<InvokeRecord, RunnerError> {
        let source = match op.auth.first() {
            Some(a) => self.aliases[a].clone(),
            None => self.aliases[&self.anon].clone(),
        };
        let out = self.invoke_raw(&source, &op.function, &op.args, true)?;
        if out.ok {
            let value: Json = serde_json::from_str(&out.stdout).unwrap_or(Json::Null);
            let value = rename(&value, &self.names);
            Ok(InvokeRecord {
                outcome: Outcome { status: "ok".into(), shape: Some("json".into()), value: Some(value), error: None },
                observed_auth: vec![],
                tx_hash: tx_hash_from(&out.stderr),
            })
        } else {
            Ok(InvokeRecord {
                outcome: Outcome {
                    status: "error".into(),
                    shape: None,
                    value: None,
                    error: Some(classify_stderr(&out.stderr)),
                },
                observed_auth: vec![],
                tx_hash: None,
            })
        }
    }

    fn probe(&mut self, probe: &Probe) -> ProbeReading {
        match probe {
            Probe::Storage(_) => ProbeReading {
                ok: false,
                shape: None,
                value: None,
                error: Some("raw storage probes are not supported in testnet mode".into()),
            },
            Probe::Call(c) => {
                let src = self.aliases[&self.anon].clone();
                match self.invoke_raw(&src, &c.function, &c.args, false) {
                    Ok(o) if o.ok => match serde_json::from_str::<Json>(&o.stdout) {
                        Ok(j) => ProbeReading {
                            ok: true,
                            shape: Some("json".into()),
                            value: Some(rename(&j, &self.names)),
                            error: None,
                        },
                        Err(e) => ProbeReading {
                            ok: false,
                            shape: None,
                            value: None,
                            error: Some(format!("unparseable CLI output: {e}")),
                        },
                    },
                    Ok(o) => {
                        let e = classify_stderr(&o.stderr);
                        ProbeReading {
                            ok: false,
                            shape: None,
                            value: None,
                            error: Some(format!("{} {} {}", e.class, e.host_error, e.message).trim().to_string()),
                        }
                    }
                    Err(e) => ProbeReading { ok: false, shape: None, value: None, error: Some(e.0) },
                }
            }
        }
    }

    fn executable_hash(&mut self) -> Option<String> {
        let path = self.scratch.join(format!("upgradelab-fetch-{}.wasm", &self.contract_id));
        let p = path.to_string_lossy().to_string();
        let o =
            cli(&self.bin, &["contract", "fetch", "--id", &self.contract_id, "--network", NETWORK, "--out-file", &p])
                .ok()?;
        if !o.ok {
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
        Some(hex_encode(&Sha256::digest(&bytes)))
    }
}

pub fn run_testnet(args: &TestnetArgs) -> Result<Report, RunnerError> {
    let text = std::fs::read_to_string(&args.scenario)
        .map_err(|e| RunnerError(format!("cannot read {}: {e}", args.scenario.display())))?;
    let scenario = Scenario::parse(&text).map_err(|e| RunnerError(format!("invalid scenario: {}", e.0)))?;
    let (_, old) = crate::read_wasm(&args.root, &scenario.wasm.old)?;
    let (_, new) = crate::read_wasm(&args.root, &scenario.wasm.new)?;
    let mut backend = TestnetBackend::new(
        args,
        &scenario,
        &args.root.join(&scenario.wasm.old),
        &args.root.join(&scenario.wasm.new),
        &old.sha256,
        &new.sha256,
    )?;
    let meta = RunMeta {
        tool: ToolInfo {
            name: "upgradelab-runner".into(),
            runner_version: crate::RUNNER_VERSION.into(),
            soroban_sdk: crate::host::SOROBAN_SDK_VERSION.into(),
            host_protocol: backend.protocol,
            mode: "testnet".into(),
            auth_enforcement: crate::AUTH_ENFORCEMENT_TESTNET.into(),
        },
        scenario_sha256: engine::scenario_sha256(&scenario),
        wasm: WasmInfo { old, new },
        categories: vec![
            Category {
                id: CAT_TESTNET.into(),
                title: "Stellar testnet".into(),
                status: "executed".into(),
                detail: "Real transactions on testnet through the Stellar CLI with throwaway keys; state read back through RPC simulation and contract fetch.".into(),
                native: None,
            },
            Category {
                id: CAT_COMPILED.into(),
                title: "Compiled WASM in the in-process Soroban host".into(),
                status: "not-run".into(),
                detail: "Not used for this report; run `upgradelab run` for it.".into(),
                native: None,
            },
            Category {
                id: CAT_NATIVE.into(),
                title: "Native SDK tests (Rust, mocked auth, not the VM)".into(),
                status: "not-run".into(),
                detail: "Not used for this report.".into(),
                native: None,
            },
        ],
        network: Some(NetworkInfo {
            network: NETWORK.into(),
            rpc_url: RPC_URL.into(),
            contract_id: backend.contract_id().to_string(),
            wasm_hash_old: scenario_hash(&args.root, &scenario.wasm.old)?,
            wasm_hash_new: scenario_hash(&args.root, &scenario.wasm.new)?,
            accounts: backend.addresses().clone(),
        }),
        limits: LIMITS_TESTNET.iter().map(|s| s.to_string()).collect(),
    };
    engine::run(&scenario, &mut backend, meta)
}

fn scenario_hash(root: &Path, rel: &str) -> Result<String, RunnerError> {
    Ok(crate::read_wasm(root, rel)?.1.sha256)
}

pub fn run_cli(args: TestnetArgs) -> i32 {
    match run_testnet(&args) {
        Ok(r) => {
            if let Some(p) = &args.out {
                if let Err(e) = std::fs::write(p, r.to_json()) {
                    eprintln!("cannot write {}: {e}", p.display());
                    return 4;
                }
            }
            match args.format {
                crate::Format::Text => print!("{}", crate::text::render(&r)),
                crate::Format::Json => print!("{}", r.to_json()),
            }
            r.exit_code()
        }
        Err(e) => {
            eprintln!("error: {e}");
            4
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_hash_is_taken_from_the_explorer_line() {
        let s = "ℹ️  Simulating\n🔗 https://stellar.expert/explorer/testnet/tx/2f32a70f68ebd5eb1f30d7ce254e4e77ad91257456e62662e6718556399b447d\n";
        assert_eq!(tx_hash_from(s).unwrap(), "2f32a70f68ebd5eb1f30d7ce254e4e77ad91257456e62662e6718556399b447d");
        assert!(tx_hash_from("no hash here").is_none());
        assert!(tx_hash_from("/tx/zz").is_none());
    }

    #[test]
    fn cli_failures_are_classified() {
        let c = classify_stderr("❌ error: transaction simulation failed: HostError: Error(Contract, #1)\n\nEvent log");
        assert_eq!((c.class.as_str(), c.contract_code), ("contract", Some(1)));
        let a = classify_stderr("❌ error: Missing signing key for account GABC");
        assert_eq!(a.class, "auth");
        assert_eq!(a.host_error, "CliMissingSigningKey");
        assert_eq!(classify_stderr("HostError: Error(Auth, InvalidAction)").class, "auth");
        assert_eq!(classify_stderr("something else broke").class, "other");
    }

    #[test]
    fn addresses_in_cli_output_are_mapped_back_to_names() {
        let mut names = BTreeMap::new();
        names.insert("GAAA".to_string(), "alice".to_string());
        let j: Json = serde_json::json!(["GAAA", "GBBB", 5, "100"]);
        assert_eq!(rename(&j, &names), serde_json::json!(["alice", "GBBB", 5, "100"]));
    }

    #[test]
    fn mainnet_is_refused_before_any_command_runs() {
        let args = TestnetArgs {
            scenario: PathBuf::from("x"),
            root: PathBuf::from("."),
            out: None,
            key_prefix: "ul".into(),
            network: "mainnet".into(),
            stellar_bin: "definitely-not-a-real-binary".into(),
            anonymous_source: "mallory".into(),
            scratch: None,
            format: crate::Format::Text,
        };
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(root.join("scenarios/vault-correct.json")).unwrap();
        let s = Scenario::parse(&text).unwrap();
        let err = TestnetBackend::new(&args, &s, Path::new("a"), Path::new("b"), "", "").err().unwrap();
        assert!(err.0.contains("refused"), "{}", err.0);
    }
}
