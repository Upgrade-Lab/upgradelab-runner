//! Native SDK tests (native Rust, mocked auths): a different category from the
//! runner's compiled-WASM rehearsal. They test the migration logic directly by
//! writing v1-format entries into storage, since a native test cannot upload v1 and
//! upgrade it.
use super::*;
use soroban_sdk::testutils::Address as _;

fn setup() -> (Env, VaultV2Client<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(VaultV2, ());
    let client = VaultV2Client::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

/// Write a v1-format entry exactly as v1 would have.
fn seed_legacy(env: &Env, id: &Address, who: &Address, amount: i128) {
    env.as_contract(id, || {
        env.storage().persistent().set(&DataKey::Balance(who.clone()), &amount);
        let mut h: Vec<Address> = env.storage().persistent().get(&DataKey::Holders).unwrap();
        h.push_back(who.clone());
        env.storage().persistent().set(&DataKey::Holders, &h);
        let s: i128 = env.storage().persistent().get(&DataKey::Supply).unwrap();
        env.storage().persistent().set(&DataKey::Supply, &(s + amount));
    });
}

#[cfg(not(any(
    feature = "broken-lose-balance",
    feature = "broken-double-balance",
    feature = "broken-reinit",
    feature = "broken-upgrade-auth",
    feature = "broken-not-idempotent"
)))]
mod correct {
    use super::*;

    #[test]
    fn migrate_preserves_balances_and_is_idempotent() {
        let (env, c, _) = setup();
        let (a, b) = (Address::generate(&env), Address::generate(&env));
        seed_legacy(&env, &c.address, &a, 100);
        seed_legacy(&env, &c.address, &b, 250);
        assert_eq!(c.balance(&a), 100);
        assert_eq!(c.migrate(&10), 0);
        assert_eq!((c.balance(&a), c.balance(&b), c.total_supply()), (100, 250, 350));
        assert_eq!(c.schema_version(), 2);
        assert_eq!(c.migrate(&10), 0);
        assert_eq!((c.balance(&a), c.balance(&b), c.total_supply()), (100, 250, 350));
    }

    #[test]
    fn batched_migration_reports_remaining() {
        let (env, c, _) = setup();
        let ids: [Address; 3] = [Address::generate(&env), Address::generate(&env), Address::generate(&env)];
        for (i, w) in ids.iter().enumerate() {
            seed_legacy(&env, &c.address, w, 10 * (i as i128 + 1));
        }
        assert_eq!(c.migrate(&2), 1);
        assert_eq!(c.schema_version(), 1);
        assert_eq!(c.migrate(&2), 0);
        assert_eq!(c.schema_version(), 2);
    }

    #[test]
    fn mixed_format_reads_and_writes() {
        let (env, c, _) = setup();
        let (a, b) = (Address::generate(&env), Address::generate(&env));
        seed_legacy(&env, &c.address, &a, 100);
        seed_legacy(&env, &c.address, &b, 40);
        c.deposit(&a, &50);
        assert_eq!((c.balance(&a), c.balance(&b)), (150, 40));
    }

    #[test]
    fn reinitialize_is_rejected() {
        let (env, c, _) = setup();
        assert!(c.try_initialize(&Address::generate(&env)).is_err());
    }
}
