//! One integration-test binary: every file is a module, so the Soroban host is linked
//! once instead of once per file (the debug binary is large and disk is tight).
mod cli;
mod common;
mod determinism;
mod engine;
mod guards;
mod scenarios;
mod units;
mod validation;
