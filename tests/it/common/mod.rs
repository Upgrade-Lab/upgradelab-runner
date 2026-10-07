#![allow(dead_code)]
use std::path::{Path, PathBuf};
use upgradelab::report::{Report, Status};
use upgradelab::scenario::Scenario;

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn scenario_text(name: &str) -> String {
    std::fs::read_to_string(root().join("scenarios").join(format!("{name}.json"))).unwrap()
}

pub fn scenario(name: &str) -> Scenario {
    Scenario::parse(&scenario_text(name)).unwrap()
}

pub fn run(name: &str) -> Report {
    upgradelab::run_host(&scenario(name), &root(), None).unwrap()
}

pub fn run_scenario(s: &Scenario) -> Report {
    upgradelab::run_host(s, &root(), None).unwrap()
}

pub fn status(r: &Report, id: &str) -> Status {
    r.invariants.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no invariant {id}")).status
}

pub fn failing(r: &Report) -> Vec<String> {
    r.invariants.iter().filter(|i| i.status == Status::Fail).map(|i| i.id.clone()).collect()
}

pub fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}
