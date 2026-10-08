# The report

`schema/report.v1.schema.json` (generated from the Rust types with schemars; a test fails if it is stale). It embeds the scenario, every executed operation (signers, outcome, authorizations the host recorded, executable hash after), probe readouts at each checkpoint, authorization checks, invariant results with evidence, WASM sha256, tool and host versions, categories and a `limits` list. It has no timestamp or machine path: running the same scenario against the same WASM gives byte-identical output (tested twice over, plus `replay` of every committed report).
