//! Source-level guards for claims the README makes.
use std::path::Path;

fn sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect()
}

#[test]
fn authorization_is_never_mocked_in_the_runner() {
    for (name, text) in sources() {
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !code.contains(".mock_all_auths")
                    && !code.contains(".mock_auths")
                    && !code.contains("mock_all_auths_allowing"),
                "{name}:{}: {line}",
                n + 1
            );
        }
    }
}

#[test]
fn the_runner_has_no_clock_or_randomness() {
    for (name, text) in sources() {
        for banned in ["SystemTime", "Instant::now", "rand::", "thread_rng", "getrandom"] {
            assert!(!text.contains(banned), "{name} uses {banned}; reports must be reproducible");
        }
    }
}

#[test]
fn soroban_sdk_is_pinned_exactly() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(manifest.contains("soroban-sdk = { version = \"=28.0.0\""));
    assert_eq!(upgradelab::host::SOROBAN_SDK_VERSION, "28.0.0");
    let fixtures =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/contracts/Cargo.toml")).unwrap();
    assert!(fixtures.contains("soroban-sdk = \"=28.0.0\""));
}

#[test]
fn committed_wasm_matches_its_manifest() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/wasm");
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("MANIFEST.json")).unwrap()).unwrap();
    let files = m["wasm"].as_array().unwrap();
    assert!(files.len() >= 7);
    for f in files {
        let bytes = std::fs::read(root.join(f["file"].as_str().unwrap())).unwrap();
        let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, f["sha256"].as_str().unwrap(), "{}", f["file"]);
        assert_eq!(bytes.len() as u64, f["bytes"].as_u64().unwrap());
    }
}
