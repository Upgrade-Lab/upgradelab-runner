//! Conversion between scenario values, Soroban XDR values and the report's plain JSON.

use crate::scenario::{Value, WhichWasm};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use soroban_sdk::xdr::{
    AccountId, Hash, Int128Parts, PublicKey, ScAddress, ScBytes, ScMap, ScString, ScSymbol, ScVal, ScVec, UInt128Parts,
    Uint256,
};
use std::collections::BTreeMap;

/// Deterministic identities for a run: the same names always give the same keys.
#[derive(Clone)]
pub struct Identities {
    pub keys: BTreeMap<String, SigningKey>,
    pub contract_id: [u8; 32],
    pub wasm_old: [u8; 32],
    pub wasm_new: [u8; 32],
    /// strkey -> display name, used when rendering addresses in reports.
    pub names: BTreeMap<String, String>,
}

pub fn ed_strkey(b: [u8; 32]) -> String {
    format!("{}", stellar_strkey::ed25519::PublicKey(b))
}

pub fn contract_strkey(b: [u8; 32]) -> String {
    format!("{}", stellar_strkey::Contract(b))
}

pub fn account_seed(name: &str) -> [u8; 32] {
    Sha256::digest(format!("upgradelab/account/{name}").as_bytes()).into()
}

pub fn subject_contract_id() -> [u8; 32] {
    Sha256::digest(b"upgradelab/contract/subject").into()
}

impl Identities {
    pub fn new(accounts: &[String], wasm_old: [u8; 32], wasm_new: [u8; 32]) -> Self {
        let mut keys = BTreeMap::new();
        let mut names = BTreeMap::new();
        for a in accounts {
            let k = SigningKey::from_bytes(&account_seed(a));
            names.insert(ed_strkey(k.verifying_key().to_bytes()), a.clone());
            keys.insert(a.clone(), k);
        }
        let contract_id = subject_contract_id();
        names.insert(contract_strkey(contract_id), "subject".to_string());
        Identities { keys, contract_id, wasm_old, wasm_new, names }
    }

    pub fn account_id(&self, name: &str) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(self.keys[name].verifying_key().to_bytes())))
    }

    pub fn account_strkey(&self, name: &str) -> String {
        ed_strkey(self.keys[name].verifying_key().to_bytes())
    }

    pub fn contract_strkey(&self) -> String {
        contract_strkey(self.contract_id)
    }
}

fn parse_int<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse::<T>().map_err(|_| format!("`{s}` is not a valid {what}"))
}

pub fn to_scval(v: &Value, ids: &Identities) -> Result<ScVal, String> {
    if let Some(a) = &v.account {
        return Ok(ScVal::Address(ScAddress::Account(ids.account_id(a))));
    }
    if v.contract.is_some() {
        return Ok(ScVal::Address(ScAddress::Contract(soroban_sdk::xdr::ContractId(Hash(ids.contract_id)))));
    }
    if let Some(s) = &v.i128 {
        let n: i128 = parse_int(s, "i128")?;
        return Ok(ScVal::I128(Int128Parts { hi: (n >> 64) as i64, lo: n as u64 }));
    }
    if let Some(s) = &v.u128 {
        let n: u128 = parse_int(s, "u128")?;
        return Ok(ScVal::U128(UInt128Parts { hi: (n >> 64) as u64, lo: n as u64 }));
    }
    if let Some(s) = &v.i64 {
        return Ok(ScVal::I64(parse_int(s, "i64")?));
    }
    if let Some(s) = &v.u64 {
        return Ok(ScVal::U64(parse_int(s, "u64")?));
    }
    if let Some(n) = v.u32 {
        return Ok(ScVal::U32(n));
    }
    if let Some(n) = v.i32 {
        return Ok(ScVal::I32(n));
    }
    if let Some(b) = v.bool {
        return Ok(ScVal::Bool(b));
    }
    if let Some(s) = &v.symbol {
        return Ok(ScVal::Symbol(ScSymbol(s.as_str().try_into().map_err(|_| format!("invalid symbol `{s}`"))?)));
    }
    if let Some(s) = &v.string {
        return Ok(ScVal::String(ScString(s.as_str().try_into().map_err(|_| "string too long".to_string())?)));
    }
    if let Some(h) = &v.bytes {
        let b = hex_decode(h)?;
        return Ok(ScVal::Bytes(ScBytes(b.try_into().map_err(|_| "bytes too long".to_string())?)));
    }
    if let Some(w) = &v.wasm_hash {
        let h = match w {
            WhichWasm::Old => ids.wasm_old,
            WhichWasm::New => ids.wasm_new,
        };
        return Ok(ScVal::Bytes(ScBytes(h.to_vec().try_into().map_err(|_| "hash".to_string())?)));
    }
    if let Some(var) = &v.variant {
        let mut items = vec![ScVal::Symbol(ScSymbol(
            var.name.as_str().try_into().map_err(|_| format!("invalid variant name `{}`", var.name))?,
        ))];
        for x in &var.values {
            items.push(to_scval(x, ids)?);
        }
        return Ok(ScVal::Vec(Some(ScVec(items.try_into().map_err(|_| "vec too long".to_string())?))));
    }
    if let Some(items) = &v.vec {
        let mut out = Vec::new();
        for x in items {
            out.push(to_scval(x, ids)?);
        }
        return Ok(ScVal::Vec(Some(ScVec(out.try_into().map_err(|_| "vec too long".to_string())?))));
    }
    Err("empty value".to_string())
}

pub fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("`{s}` is not valid hex"));
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())).collect()
}

pub fn address_string(a: &ScAddress, names: &BTreeMap<String, String>) -> String {
    let s = match a {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(b)))) => ed_strkey(*b),
        ScAddress::Contract(soroban_sdk::xdr::ContractId(Hash(b))) => contract_strkey(*b),
        other => format!("{other:?}"),
    };
    names.get(&s).cloned().unwrap_or(s)
}

/// Plain rendering used in reports: (shape, value). Integers wider than 32 bits are
/// decimal strings so no precision is lost in JSON.
pub fn render(v: &ScVal, names: &BTreeMap<String, String>) -> (String, serde_json::Value) {
    use serde_json::{json, Value as J};
    match v {
        ScVal::Void => ("void".into(), J::Null),
        ScVal::Bool(b) => ("bool".into(), json!(b)),
        ScVal::U32(n) => ("u32".into(), json!(n)),
        ScVal::I32(n) => ("i32".into(), json!(n)),
        ScVal::U64(n) => ("u64".into(), json!(n.to_string())),
        ScVal::I64(n) => ("i64".into(), json!(n.to_string())),
        ScVal::U128(p) => ("u128".into(), json!((((p.hi as u128) << 64) | p.lo as u128).to_string())),
        ScVal::I128(p) => ("i128".into(), json!((((p.hi as i128) << 64) | p.lo as i128).to_string())),
        ScVal::Symbol(s) => ("symbol".into(), json!(s.to_string())),
        ScVal::String(s) => ("string".into(), json!(s.to_string())),
        ScVal::Bytes(b) => ("bytes".into(), json!(hex_encode(b.as_slice()))),
        ScVal::Address(a) => ("address".into(), json!(address_string(a, names))),
        ScVal::Vec(Some(items)) => ("vec".into(), J::Array(items.iter().map(|x| render(x, names).1).collect())),
        ScVal::Vec(None) => ("vec".into(), J::Array(vec![])),
        ScVal::Map(Some(m)) => ("map".into(), render_map(m, names)),
        ScVal::Map(None) => ("map".into(), json!({})),
        ScVal::Error(e) => ("error".into(), json!(format!("{e:?}"))),
        other => ("other".into(), json!(format!("{other:?}"))),
    }
}

fn render_map(m: &ScMap, names: &BTreeMap<String, String>) -> serde_json::Value {
    use serde_json::{json, Value as J};
    if m.iter().all(|e| matches!(e.key, ScVal::Symbol(_))) {
        let mut o = serde_json::Map::new();
        for e in m.iter() {
            if let ScVal::Symbol(k) = &e.key {
                o.insert(k.to_string(), render(&e.val, names).1);
            }
        }
        J::Object(o)
    } else {
        J::Array(m.iter().map(|e| json!([render(&e.key, names).1, render(&e.val, names).1])).collect())
    }
}
