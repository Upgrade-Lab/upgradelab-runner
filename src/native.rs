//! Parses the text output of `cargo test` so a native-SDK result can be attached to a
//! report as its own category. The runner never executes native tests itself.

use crate::report::{NativeSummary, NativeTest};
use sha2::{Digest, Sha256};

pub fn parse_log(text: &str) -> NativeSummary {
    let mut tests = Vec::new();
    let (mut passed, mut failed, mut ignored) = (0u32, 0u32, 0u32);
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("test ") {
            if let Some((name, result)) = rest.rsplit_once(" ... ") {
                let result = result.trim();
                let norm = if result.starts_with("ok") {
                    passed += 1;
                    "ok"
                } else if result.starts_with("FAILED") {
                    failed += 1;
                    "failed"
                } else if result.starts_with("ignored") {
                    ignored += 1;
                    "ignored"
                } else {
                    continue;
                };
                tests.push(NativeTest { name: name.trim().to_string(), result: norm.to_string() });
            }
        }
    }
    tests.sort_by(|a, b| a.name.cmp(&b.name));
    NativeSummary { log_sha256: crate::values::hex_encode(&Sha256::digest(text.as_bytes())), passed, failed, ignored, tests }
}
