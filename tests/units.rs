use upgradelab::native::parse_log;
use upgradelab::scenario::{Value, WhichWasm};
use upgradelab::values::{render, to_scval, Identities};

fn ids() -> Identities {
    Identities::new(&["alice".to_string(), "bob".to_string()], [1; 32], [2; 32])
}

#[test]
fn i128_round_trips_through_xdr_for_edge_values() {
    let ids = ids();
    for n in [0i128, 1, -1, i128::MAX, i128::MIN, 1 << 64, -(1 << 64) - 1] {
        let v = Value { i128: Some(n.to_string()), ..Default::default() };
        let sc = to_scval(&v, &ids).unwrap();
        let (shape, json) = render(&sc, &ids.names);
        assert_eq!(shape, "i128");
        assert_eq!(json, serde_json::Value::String(n.to_string()));
    }
}

#[test]
fn bad_numbers_are_rejected_not_wrapped() {
    let ids = ids();
    let v = Value { i128: Some("340282366920938463463374607431768211456".into()), ..Default::default() };
    assert!(to_scval(&v, &ids).is_err());
    let v = Value { u64: Some("-1".into()), ..Default::default() };
    assert!(to_scval(&v, &ids).is_err());
}

#[test]
fn a_contracttype_variant_is_a_vec_of_symbol_then_values() {
    let ids = ids();
    let v: Value =
        serde_json::from_str(r#"{"variant": {"name": "Balance", "values": [{"account": "alice"}]}}"#).unwrap();
    let sc = to_scval(&v, &ids).unwrap();
    let (shape, json) = render(&sc, &ids.names);
    assert_eq!(shape, "vec");
    assert_eq!(json, serde_json::json!(["Balance", "alice"]));
}

#[test]
fn wasm_hash_values_are_the_32_byte_hashes() {
    let ids = ids();
    let v = Value { wasm_hash: Some(WhichWasm::New), ..Default::default() };
    let (shape, json) = render(&to_scval(&v, &ids).unwrap(), &ids.names);
    assert_eq!(shape, "bytes");
    assert_eq!(json, serde_json::json!("02".repeat(32)));
}

#[test]
fn the_same_name_always_gives_the_same_address_and_names_differ() {
    let a = ids();
    let b = ids();
    assert_eq!(a.account_strkey("alice"), b.account_strkey("alice"));
    assert_ne!(a.account_strkey("alice"), a.account_strkey("bob"));
    assert!(a.account_strkey("alice").starts_with('G'));
    assert!(a.contract_strkey().starts_with('C'));
}

#[test]
fn the_cargo_test_log_parser_counts_each_outcome() {
    let log = "running 4 tests\ntest a::ok_one ... ok\ntest a::bad ... FAILED\ntest a::skip ... ignored\ntest b::ok_two ... ok\n\ntest result: FAILED. 2 passed; 1 failed; 1 ignored\n";
    let s = parse_log(log);
    assert_eq!((s.passed, s.failed, s.ignored), (2, 1, 1));
    assert_eq!(s.tests.len(), 4);
    assert_eq!(s.tests[0].name, "a::bad");
    assert_eq!(s.tests[0].result, "failed");
    assert_eq!(s.log_sha256.len(), 64);
}

#[test]
fn an_empty_log_is_zero_not_a_pass() {
    let s = parse_log("");
    assert_eq!((s.passed, s.failed, s.tests.len()), (0, 0, 0));
}
