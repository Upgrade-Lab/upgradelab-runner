//! Native SDK tests: the contract's Rust code runs natively, NOT as compiled WASM
//! in the Soroban VM, and auths are mocked. These are a different category from
//! the runner's compiled-WASM rehearsal.
use super::*;
use soroban_sdk::testutils::Address as _;

fn setup() -> (Env, VaultV1Client<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(VaultV1, ());
    let client = VaultV1Client::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn deposit_withdraw_track_supply() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    c.deposit(&a, &100);
    c.withdraw(&a, &30);
    assert_eq!(c.balance(&a), 70);
    assert_eq!(c.total_supply(), 70);
}

#[test]
fn initialize_twice_is_rejected() {
    let (env, c, _) = setup();
    let other = Address::generate(&env);
    assert!(c.try_initialize(&other).is_err());
}

#[test]
fn overdraw_is_rejected() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    c.deposit(&a, &10);
    assert!(c.try_withdraw(&a, &11).is_err());
}
