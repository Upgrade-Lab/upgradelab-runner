//! A backend executes operations and reads state. The engine and the invariants
//! are backend-independent; the in-process Soroban host is the default backend and
//! the Stellar CLI against testnet is the opt-in one.

use crate::report::{ObservedAuth, Outcome, ProbeReading};
use crate::scenario::{Op, Probe};

pub struct InvokeRecord {
    pub outcome: Outcome,
    pub observed_auth: Vec<ObservedAuth>,
    pub tx_hash: Option<String>,
}

#[derive(Debug)]
pub struct RunnerError(pub String);
impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RunnerError {}

pub trait Backend {
    /// "compiled-wasm" or "testnet-rpc".
    fn category(&self) -> &'static str;
    /// Runner-performed setup steps already done (deploy old, upload new), as
    /// system operations to prepend to the executed-operations list.
    fn setup_steps(&self) -> Vec<crate::report::ExecutedOp>;
    fn invoke(&mut self, op: &Op) -> Result<InvokeRecord, RunnerError>;
    fn probe(&mut self, probe: &Probe) -> ProbeReading;
    /// sha256 (hex) of the contract's current executable, if it can be read.
    fn executable_hash(&mut self) -> Option<String>;
}
