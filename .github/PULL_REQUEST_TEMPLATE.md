## What and why

## Verification
- [ ] `cargo fmt --check && cargo clippy --all-targets && cargo test`
- [ ] Schemas regenerated if report or scenario types changed
- [ ] Evidence regenerated (`scripts/record-evidence.sh`) if reports changed
- [ ] No mocked auth; reports still deterministic; categories still separate
- [ ] New invariant or defect: pass test, fail test, inconclusive test
