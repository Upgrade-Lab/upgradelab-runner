//! The in-process Soroban host backend.
//!
//! Executes the ACTUAL compiled WASM (the old file, then the new one after the
//! contract upgrades itself) inside the Soroban host VM that is linked into this
//! runner (soroban-env-host through soroban-sdk's testutils `Env`). It is not a
//! network and not a validator: no consensus, no fees, no transaction envelope, no
//! state archival or restore. See docs/adr/0002-in-process-host.md.
//!
//! Authorization is ENFORCED, not mocked. The environment starts with no
//! authorization entries. Each call is given real `SorobanAuthorizationEntry`s that
//! the named accounts sign with ed25519 over the standard signature payload, and the
//! host verifies those signatures against account ledger entries (master key weight
//! 1) and consumes a nonce. `mock_all_auths` / `mock_auths` are never used. What it
//! does NOT test: multi-signer accounts and thresholds, custom account contracts,
//! signature expiration beyond the fixed +100 ledgers, and nothing about how a real
//! wallet builds the entry.

use crate::backend::{Backend, InvokeRecord, RunnerError};
use crate::report::*;
use crate::scenario::{Durability, Op, Phase, Probe, Value};
use crate::values::{hex_encode, render, to_scval, Identities};
use ed25519_dalek::Signer;
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{EnvTestConfig, Ledger as _};
use soroban_sdk::xdr::{
    AccountEntry, AccountEntryExt, ContractExecutable, ContractEventBody, HashIdPreimage,
    HashIdPreimageSorobanAuthorization, InvokeContractArgs, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyAccount, Limits, ScAddress, ScError, ScMap, ScMapEntry,
    ScSymbol, ScVal, ScVec, SequenceNumber, SorobanAddressCredentials, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, Thresholds,
    WriteXdr,
};
use soroban_sdk::{Address, Bytes, Env, Symbol, TryFromVal, Val, Vec as SVec};

pub const SOROBAN_SDK_VERSION: &str = "28.0.0";
const SIG_EXPIRY_LEDGERS: u32 = 100;

pub struct HostBackend {
    env: Env,
    contract: Address,
    ids: Identities,
    nonce: i64,
    setup: Vec<ExecutedOp>,
    pub protocol: u32,
}

fn sha256_hex(b: &[u8]) -> String {
    hex_encode(&Sha256::digest(b))
}

impl HostBackend {
    pub fn new(
        accounts: &[String],
        wasm_old: &[u8],
        wasm_new: &[u8],
    ) -> Result<Self, RunnerError> {
        let old_hash: [u8; 32] = Sha256::digest(wasm_old).into();
        let new_hash: [u8; 32] = Sha256::digest(wasm_new).into();
        let ids = Identities::new(accounts, old_hash, new_hash);
        let contract_strkey = ids.contract_strkey();

        // Start from a default ledger and add one account entry per named account,
        // so the host can verify real ed25519 signatures against them.
        let base = Env::default();
        let mut snap: LedgerSnapshot = base.to_ledger_snapshot();
        for name in accounts {
            let id = ids.account_id(name);
            let entry = LedgerEntry {
                last_modified_ledger_seq: 0,
                data: LedgerEntryData::Account(AccountEntry {
                    account_id: id.clone(),
                    balance: 1_000_000_000,
                    seq_num: SequenceNumber(0),
                    num_sub_entries: 0,
                    inflation_dest: None,
                    flags: 0,
                    home_domain: Default::default(),
                    thresholds: Thresholds([1, 0, 0, 0]),
                    signers: Default::default(),
                    ext: AccountEntryExt::V0,
                }),
                ext: LedgerEntryExt::V0,
            };
            snap.ledger_entries
                .push((Box::new(LedgerKey::Account(LedgerKeyAccount { account_id: id })), (Box::new(entry), None)));
        }
        let env = Env::from_ledger_snapshot(snap);
        // The SDK writes a test_snapshots/*.json file when an Env drops, unless told not to.
        // (EnvTestConfig is non-exhaustive in spirit; we set the one field we need.)
        let mut env = env;
        env.set_config(EnvTestConfig { capture_snapshot_at_drop: false });
        drop(base);

        let contract = Address::from_str(&env, &contract_strkey);
        <&[u8] as soroban_sdk::testutils::Register>::register(wasm_old, &env, &contract, ());

        let uploaded = env.deployer().upload_contract_wasm(Bytes::from_slice(&env, wasm_new));
        if uploaded.to_array() != new_hash {
            return Err(RunnerError("host-reported hash of the new WASM differs from sha256 of the file".into()));
        }
        let protocol = env.ledger().get().protocol_version;
        let mut b = HostBackend { env, contract, ids, nonce: 0, setup: vec![], protocol };
        b.setup = vec![
            system_step(1, "deploy-old", "register_wasm", &sha256_hex(wasm_old), b.executable_hash()),
            system_step(2, "upload-new", "upload_contract_wasm", &sha256_hex(wasm_new), b.executable_hash()),
        ];
        Ok(b)
    }

    fn exec_val_args(&self, args: &[Value]) -> Result<Vec<ScVal>, RunnerError> {
        args.iter().map(|a| to_scval(a, &self.ids).map_err(RunnerError)).collect()
    }

    fn sign_entry(&mut self, signer: &str, function: &str, args: &[ScVal]) -> Result<SorobanAuthorizationEntry, RunnerError> {
        self.sign_entry_with(signer, signer, function, args)
    }

    /// `address_of` is the account the entry claims to authorize; `key_of` is whose
    /// private key signs it. They differ only in the forged-signature test.
    fn sign_entry_with(
        &mut self,
        address_of: &str,
        key_of: &str,
        function: &str,
        args: &[ScVal],
    ) -> Result<SorobanAuthorizationEntry, RunnerError> {
        let signer = address_of;
        self.nonce += 1;
        let nonce = self.nonce;
        let exp = self.env.ledger().sequence() + SIG_EXPIRY_LEDGERS;
        let ScAddress::Contract(cid) = ScAddress::try_from(&self.contract).map_err(|e| RunnerError(format!("{e:?}")))? else {
            return Err(RunnerError("subject is not a contract address".into()));
        };
        let invocation = SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: ScAddress::Contract(cid),
                function_name: ScSymbol(function.try_into().map_err(|_| RunnerError(format!("bad fn `{function}`")))?),
                args: args.to_vec().try_into().map_err(|_| RunnerError("too many args".into()))?,
            }),
            sub_invocations: Default::default(),
        };
        let network_id = soroban_sdk::xdr::Hash(self.env.ledger().get().network_id);
        let preimage = HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
            network_id,
            nonce,
            signature_expiration_ledger: exp,
            invocation: invocation.clone(),
        });
        let digest = Sha256::digest(preimage.to_xdr(Limits::none()).map_err(|e| RunnerError(e.to_string()))?);
        let key = &self.ids.keys[key_of];
        let sig = key.sign(&digest);
        let map = ScMap::sorted_from(vec![
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("public_key".try_into().unwrap())),
                val: ScVal::Bytes(key.verifying_key().to_bytes().to_vec().try_into().unwrap()),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("signature".try_into().unwrap())),
                val: ScVal::Bytes(sig.to_bytes().to_vec().try_into().unwrap()),
            },
        ])
        .map_err(|e| RunnerError(format!("{e:?}")))?;
        Ok(SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: ScAddress::Account(self.ids.account_id(signer)),
                nonce,
                signature_expiration_ledger: exp,
                signature: ScVal::Vec(Some(ScVec(vec![ScVal::Map(Some(map))].try_into().unwrap()))),
            }),
            root_invocation: invocation,
        })
    }

    fn call(&mut self, function: &str, args: &[ScVal], signers: &[String]) -> Result<InvokeRecord, RunnerError> {
        let mut entries = Vec::new();
        for s in signers {
            entries.push(self.sign_entry(s, function, args)?);
        }
        // An empty slice installs enforcing mode with zero authorizations.
        self.env.set_auths(&entries);
        self.run_call(function, args)
    }

    fn run_call(&mut self, function: &str, args: &[ScVal]) -> Result<InvokeRecord, RunnerError> {
        let mut vals = SVec::<Val>::new(&self.env);
        for a in args {
            vals.push_back(Val::try_from_val(&self.env, a).map_err(|e| RunnerError(format!("arg conversion: {e:?}")))?);
        }
        if !function.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(RunnerError(format!("function name `{function}` is not a valid Soroban symbol")));
        }
        let before = self.diag_len();
        let res = self
            .env
            .try_invoke_contract::<Val, soroban_sdk::Error>(&self.contract, &Symbol::new(&self.env, function), vals);
        let outcome = match res {
            Ok(Ok(v)) => {
                let sc = ScVal::try_from_val(&self.env, &v).map_err(|e| RunnerError(format!("result conversion: {e:?}")))?;
                let (shape, value) = render(&sc, &self.ids.names);
                Outcome { status: "ok".into(), shape: Some(shape), value: Some(value), error: None }
            }
            Ok(Err(e)) => Outcome {
                status: "error".into(),
                shape: None,
                value: None,
                error: Some(ErrorInfo { class: "other".into(), host_error: format!("result conversion: {e:?}"), contract_code: None, message: "return value did not convert".into() }),
            },
            Err(e) => {
                let fallback = match &e {
                    Ok(err) => format!("{err:?}"),
                    Err(ie) => format!("{ie:?}"),
                };
                Outcome { status: "error".into(), shape: None, value: None, error: Some(self.error_from_events(before, &fallback)) }
            }
        };
        let observed_auth = self.observed_auths();
        // Leave the environment with no authorization so probes can never ride on it.
        self.env.set_auths(&[]);
        Ok(InvokeRecord { outcome, observed_auth, tx_hash: None })
    }

    fn diag_len(&self) -> usize {
        self.env.host().get_diagnostic_events().map(|e| e.0.len()).unwrap_or(0)
    }

    /// Classifies a failed call from the host's diagnostic events. The class and host
    /// error come from the LAST error event (the error the caller actually received);
    /// the message comes from the FIRST one that carries text (the root cause). Example:
    /// a forged signature first raises an internal "signer does not belong to account"
    /// error, which `require_auth` then escalates to Auth(InvalidAction).
    fn error_from_events(&self, from: usize, fallback: &str) -> ErrorInfo {
        let events = self.env.host().get_diagnostic_events().map(|e| e.0).unwrap_or_default();
        let mut last: Option<ScError> = None;
        let mut message = String::new();
        for ev in events.iter().skip(from) {
            let ContractEventBody::V0(body) = &ev.event.body;
            let topics: &[ScVal] = body.topics.as_slice();
            let is_error = matches!(topics.first(), Some(ScVal::Symbol(s)) if s.to_string() == "error");
            if !is_error {
                continue;
            }
            if let Some(ScVal::Error(e)) = topics.get(1) {
                last = Some(e.clone());
                if message.is_empty() {
                    message = match &body.data {
                        ScVal::String(s) => s.to_string(),
                        ScVal::Vec(Some(v)) => match v.first() {
                            Some(ScVal::String(s)) => s.to_string(),
                            _ => String::new(),
                        },
                        _ => String::new(),
                    };
                }
            }
        }
        match last {
            Some(e) => {
                let (class, code, host_error) = match &e {
                    ScError::Contract(n) => ("contract", Some(*n), format!("Contract({n})")),
                    ScError::Auth(c) => ("auth", None, format!("Auth({c:?})")),
                    ScError::WasmVm(c) => ("trap", None, format!("WasmVm({c:?})")),
                    other => ("other", None, format!("{other:?}")),
                };
                ErrorInfo { class: class.into(), host_error, contract_code: code, message }
            }
            None => ErrorInfo { class: "other".into(), host_error: fallback.to_string(), contract_code: None, message: "no diagnostic error event captured".into() },
        }
    }

    fn observed_auths(&self) -> Vec<ObservedAuth> {
        use soroban_sdk::testutils::AuthorizedFunction;
        self.env
            .auths()
            .into_iter()
            .map(|(addr, inv)| {
                let (contract, function, args) = match &inv.function {
                    AuthorizedFunction::Contract((c, f, a)) => {
                        let args = a
                            .iter()
                            .filter_map(|v| ScVal::try_from_val(&self.env, &v).ok())
                            .map(|s| render(&s, &self.ids.names).1)
                            .collect();
                        (self.name_of(c), f.to_string(), args)
                    }
                    other => (String::new(), format!("{other:?}"), vec![]),
                };
                ObservedAuth { address: self.name_of(&addr), contract, function, args }
            })
            .collect()
    }

    fn name_of(&self, a: &Address) -> String {
        let s = a.to_string();
        let s = s.to_string();
        self.ids.names.get(&s).cloned().unwrap_or(s)
    }

    fn reading_from_scval(&self, sc: &ScVal) -> ProbeReading {
        let (shape, value) = render(sc, &self.ids.names);
        ProbeReading { ok: true, shape: Some(shape), value: Some(value), error: None }
    }
}

fn system_step(seq: u32, id: &str, function: &str, detail: &str, exec: Option<String>) -> ExecutedOp {
    ExecutedOp {
        seq,
        id: id.into(),
        phase: "system".into(),
        function: function.into(),
        args: vec![ArgView { name: "wasmSha256".into(), shape: "string".into(), value: serde_json::json!(detail) }],
        signers: vec![],
        outcome: Outcome { status: "ok".into(), shape: None, value: None, error: None },
        expectation: "n/a".into(),
        observed_auth: vec![],
        executable_after: exec,
        tx_hash: None,
    }
}

impl Backend for HostBackend {
    fn category(&self) -> &'static str {
        CAT_COMPILED
    }

    fn setup_steps(&self) -> Vec<ExecutedOp> {
        self.setup.clone()
    }

    fn invoke(&mut self, op: &Op) -> Result<InvokeRecord, RunnerError> {
        let args = self.exec_val_args(&op.args)?;
        self.call(&op.function, &args, &op.auth)
    }

    fn probe(&mut self, probe: &Probe) -> ProbeReading {
        match probe {
            Probe::Call(c) => {
                let (function, args) = (&c.function, &c.args);
                let args = match self.exec_val_args(args) {
                    Ok(a) => a,
                    Err(e) => return ProbeReading { ok: false, shape: None, value: None, error: Some(e.0) },
                };
                match self.call(function, &args, &[]) {
                    Ok(r) if r.outcome.status == "ok" => ProbeReading {
                        ok: true,
                        shape: r.outcome.shape,
                        value: r.outcome.value,
                        error: None,
                    },
                    Ok(r) => {
                        let e = r.outcome.error.unwrap();
                        ProbeReading { ok: false, shape: None, value: None, error: Some(format!("{}: {} {}", e.class, e.host_error, e.message).trim().to_string()) }
                    }
                    Err(e) => ProbeReading { ok: false, shape: None, value: None, error: Some(e.0) },
                }
            }
            Probe::Storage(sp) => {
                let (durability, key) = (&sp.durability, &sp.key);
                let sc = match to_scval(key, &self.ids) {
                    Ok(s) => s,
                    Err(e) => return ProbeReading { ok: false, shape: None, value: None, error: Some(e) },
                };
                let key_val = match Val::try_from_val(&self.env, &sc) {
                    Ok(v) => v,
                    Err(e) => return ProbeReading { ok: false, shape: None, value: None, error: Some(format!("key conversion: {e:?}")) },
                };
                let env = &self.env;
                let found: Option<Val> = env.as_contract(&self.contract, || match durability {
                    Durability::Persistent => env.storage().persistent().get::<Val, Val>(&key_val),
                    Durability::Instance => env.storage().instance().get::<Val, Val>(&key_val),
                });
                match found {
                    None => ProbeReading { ok: true, shape: Some("absent".into()), value: Some(serde_json::Value::Null), error: None },
                    Some(v) => match ScVal::try_from_val(&self.env, &v) {
                        Ok(sc) => self.reading_from_scval(&sc),
                        Err(e) => ProbeReading { ok: false, shape: None, value: None, error: Some(format!("value conversion: {e:?}")) },
                    },
                }
            }
        }
    }

    fn executable_hash(&mut self) -> Option<String> {
        let snap = self.env.to_ledger_snapshot();
        let ScAddress::Contract(cid) = ScAddress::try_from(&self.contract).ok()? else { return None };
        for (key, (entry, _)) in snap.ledger_entries.iter() {
            if let LedgerKey::ContractData(k) = key.as_ref() {
                if k.contract == ScAddress::Contract(cid.clone()) && k.key == ScVal::LedgerKeyContractInstance {
                    if let LedgerEntryData::ContractData(d) = &entry.data {
                        if let ScVal::ContractInstance(inst) = &d.val {
                            return match &inst.executable {
                                ContractExecutable::Wasm(h) => Some(hex_encode(&h.0)),
                                _ => None,
                            };
                        }
                    }
                }
            }
        }
        None
    }
}

pub fn phase_for(op: &Op) -> &'static str {
    match op.phase {
        Phase::Seed => "seed",
        Phase::Upgrade => "upgrade",
        Phase::PostUpgrade => "postUpgrade",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> HostBackend {
        let old = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/wasm/vault_v1.wasm")).unwrap();
        let new = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/wasm/vault_v2_correct.wasm")).unwrap();
        HostBackend::new(&["admin".to_string(), "mallory".to_string()], &old, &new).unwrap()
    }

    impl HostBackend {
        fn call_with_current_auths(&mut self, function: &str, args: &[ScVal]) -> InvokeRecord {
            self.run_call(function, args).expect("call executes")
        }
    }

    fn admin_arg(b: &HostBackend) -> Vec<ScVal> {
        vec![ScVal::Address(ScAddress::Account(b.ids.account_id("admin")))]
    }

    #[test]
    fn a_correct_signature_is_accepted() {
        let mut b = backend();
        let args = admin_arg(&b);
        let e = b.sign_entry("admin", "initialize", &args).unwrap();
        b.env.set_auths(&[e]);
        let r = b.call_with_current_auths("initialize", &args);
        assert_eq!(r.outcome.status, "ok", "{:?}", r.outcome);
    }

    #[test]
    fn a_signature_made_with_the_wrong_private_key_is_rejected_by_the_host() {
        let mut b = backend();
        let args = admin_arg(&b);
        // The entry claims to be admin's authorization but mallory's key signed it.
        let forged = b.sign_entry_with("admin", "mallory", "initialize", &args).unwrap();
        b.env.set_auths(&[forged]);
        let r = b.call_with_current_auths("initialize", &args);
        assert_eq!(r.outcome.status, "error");
        assert_eq!(r.outcome.error.unwrap().class, "auth");
    }

    #[test]
    fn a_signature_over_different_arguments_is_rejected() {
        let mut b = backend();
        let args = admin_arg(&b);
        let other = vec![ScVal::Address(ScAddress::Account(b.ids.account_id("mallory")))];
        // Signed for initialize(mallory) but presented for initialize(admin).
        let e = b.sign_entry("admin", "initialize", &other).unwrap();
        b.env.set_auths(&[e]);
        let r = b.call_with_current_auths("initialize", &args);
        assert_eq!(r.outcome.status, "error");
        assert_eq!(r.outcome.error.unwrap().class, "auth");
    }

    #[test]
    fn a_nonce_cannot_be_replayed() {
        let mut b = backend();
        let args = admin_arg(&b);
        let e = b.sign_entry("admin", "initialize", &args).unwrap();
        b.env.set_auths(&[e.clone()]);
        assert_eq!(b.call_with_current_auths("initialize", &args).outcome.status, "ok");
        // Same signed entry again: the contract would reject a second initialize anyway,
        // so replay it against deposit-free admin() view is not meaningful; instead
        // replay against a function that requires auth and cannot fail for other reasons.
        let dep_args = vec![
            ScVal::Address(ScAddress::Account(b.ids.account_id("admin"))),
            ScVal::I128(soroban_sdk::xdr::Int128Parts { hi: 0, lo: 5 }),
        ];
        let e2 = b.sign_entry("admin", "deposit", &dep_args).unwrap();
        b.env.set_auths(&[e2.clone()]);
        assert_eq!(b.call_with_current_auths("deposit", &dep_args).outcome.status, "ok");
        b.env.set_auths(&[e2]);
        let replay = b.call_with_current_auths("deposit", &dep_args);
        assert_eq!(replay.outcome.status, "error", "a consumed nonce must not authorize a second call");
        assert_eq!(replay.outcome.error.unwrap().class, "auth");
    }

    #[test]
    fn with_no_authorization_entries_require_auth_fails() {
        let mut b = backend();
        let args = admin_arg(&b);
        let r = b.call("initialize", &args, &[]).unwrap();
        assert_eq!(r.outcome.error.unwrap().class, "auth");
    }
}
