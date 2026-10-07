#![no_std]
//! UpgradeLab fixture: vault v1.
//!
//! A toy balance vault used to rehearse upgrades. NOT audited, NOT for production.
//! Storage layout (this is the "state the application depends on"):
//!   instance:   Admin -> Address
//!   persistent: Supply -> i128, Holders -> Vec<Address>, Balance(Address) -> i128

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, BytesN, Env,
    Vec,
};

#[contracttype]
pub enum DataKey {
    Admin,
    Supply,
    Holders,
    Balance(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InsufficientBalance = 3,
    InvalidAmount = 4,
}

#[contract]
pub struct VaultV1;

fn admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

fn read_balance(env: &Env, who: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(who.clone()))
        .unwrap_or(0)
}

fn add_holder(env: &Env, who: &Address) {
    let mut holders: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Holders)
        .unwrap_or_else(|| Vec::new(env));
    if !holders.contains(who) {
        holders.push_back(who.clone());
        env.storage().persistent().set(&DataKey::Holders, &holders);
    }
}

#[contractimpl]
impl VaultV1 {
    /// One-time initialisation. A second call fails with `AlreadyInitialized`.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Supply, &0i128);
        env.storage()
            .persistent()
            .set(&DataKey::Holders, &Vec::<Address>::new(&env));
    }

    pub fn deposit(env: Env, from: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let cur = read_balance(&env, &from);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(cur + amount));
        let supply: i128 = env.storage().persistent().get(&DataKey::Supply).unwrap_or(0);
        env.storage().persistent().set(&DataKey::Supply, &(supply + amount));
        add_holder(&env, &from);
    }

    pub fn withdraw(env: Env, from: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let cur = read_balance(&env, &from);
        if cur < amount {
            panic_with_error!(&env, Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(cur - amount));
        let supply: i128 = env.storage().persistent().get(&DataKey::Supply).unwrap_or(0);
        env.storage().persistent().set(&DataKey::Supply, &(supply - amount));
    }

    pub fn balance(env: Env, who: Address) -> i128 {
        read_balance(&env, &who)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().persistent().get(&DataKey::Supply).unwrap_or(0)
    }

    pub fn admin(env: Env) -> Address {
        admin(&env)
    }

    pub fn holders(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Holders)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Code version of this build.
    pub fn version(_env: Env) -> u32 {
        1
    }

    /// Replace this contract's executable. Admin only (`require_auth`).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        admin(&env).require_auth();
        // Deprecated in soroban-sdk 28 in favour of update_current_contract; same host
        // function underneath. Kept because it is the call the migration guides show.
        #[allow(deprecated)]
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test;
