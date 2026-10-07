#![no_std]
//! UpgradeLab fixture: vault v2.
//!
//! Same entry points as v1, plus a new per-account format and a batched `migrate`.
//!
//! Storage (additions over v1 in bold in the docs):
//!   persistent: BalanceV2(Address) -> Account { amount, deposits }   (new format)
//!               Balance(Address)   -> i128                           (legacy format from v1)
//!   instance:   SchemaVersion -> u32 (set to 2 when `migrate` has converted everything)
//!
//! Reads fall back from the new format to the legacy format, so state written by v1
//! (never touched after the upgrade) and state written by v2 coexist ("mixed format").
//! The default build is the CORRECT migration. Each `broken-*` feature compiles one
//! deliberate defect so the runner has something real to catch. NOT audited.

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
    BalanceV2(Address),
    SchemaVersion,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    pub amount: i128,
    pub deposits: u32,
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
pub struct VaultV2;

fn admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

fn legacy(env: &Env, who: &Address) -> Option<i128> {
    env.storage().persistent().get(&DataKey::Balance(who.clone()))
}

fn modern(env: &Env, who: &Address) -> Option<Account> {
    env.storage().persistent().get(&DataKey::BalanceV2(who.clone()))
}

/// Effective balance as seen by readers.
#[cfg(not(feature = "broken-double-balance"))]
fn read_balance(env: &Env, who: &Address) -> i128 {
    if let Some(a) = modern(env, who) {
        a.amount
    } else {
        legacy(env, who).unwrap_or(0)
    }
}

/// DEFECT (`broken-double-balance`): assumes the two formats are disjoint and sums
/// them. After `migrate` copies a legacy entry without deleting it, both exist and
/// the balance is reported twice.
#[cfg(feature = "broken-double-balance")]
fn read_balance(env: &Env, who: &Address) -> i128 {
    modern(env, who).map(|a| a.amount).unwrap_or(0) + legacy(env, who).unwrap_or(0)
}

fn write_balance(env: &Env, who: &Address, amount: i128) {
    let deposits = modern(env, who).map(|a| a.deposits).unwrap_or(0);
    env.storage().persistent().set(
        &DataKey::BalanceV2(who.clone()),
        &Account { amount, deposits: deposits + 1 },
    );
    // Writes always use the new format and drop the legacy entry.
    env.storage().persistent().remove(&DataKey::Balance(who.clone()));
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
impl VaultV2 {
    pub fn initialize(env: Env, admin: Address) {
        #[cfg(not(feature = "broken-reinit"))]
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        // DEFECT under `broken-reinit`: the guard above is missing, so anyone can call
        // initialize again, take over the admin role and wipe the accounting.
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
        write_balance(&env, &from, cur + amount);
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
        write_balance(&env, &from, cur - amount);
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

    pub fn version(_env: Env) -> u32 {
        2
    }

    /// 1 until `migrate` has converted every legacy entry, then 2.
    pub fn schema_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::SchemaVersion).unwrap_or(1)
    }

    /// Convert up to `limit` legacy balances to the new format. Admin only.
    /// Returns how many holders still have a legacy entry. Safe to call repeatedly:
    /// when nothing is left to convert it changes nothing.
    pub fn migrate(env: Env, limit: u32) -> u32 {
        admin(&env).require_auth();
        let holders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Holders)
            .unwrap_or_else(|| Vec::new(&env));
        let mut converted = 0u32;
        for (i, who) in holders.iter().enumerate() {
            if converted >= limit {
                break;
            }
            let Some(old) = legacy(&env, &who) else { continue };

            // DEFECT (`broken-lose-balance`): off-by-one. The legacy entry of the LAST
            // holder is deleted without its amount ever being written to the new format.
            #[cfg(feature = "broken-lose-balance")]
            if i as u32 + 1 == holders.len() {
                env.storage().persistent().remove(&DataKey::Balance(who.clone()));
                converted += 1;
                continue;
            }
            let _ = i;

            if modern(&env, &who).is_none() {
                env.storage().persistent().set(
                    &DataKey::BalanceV2(who.clone()),
                    &Account { amount: old, deposits: 0 },
                );
            }
            // DEFECT (`broken-double-balance`): the legacy entry is left behind.
            #[cfg(not(feature = "broken-double-balance"))]
            env.storage().persistent().remove(&DataKey::Balance(who.clone()));
            converted += 1;
        }
        let mut remaining = 0u32;
        for who in holders.iter() {
            if legacy(&env, &who).is_some() && modern(&env, &who).is_none() {
                remaining += 1;
            }
        }
        if remaining == 0 {
            env.storage().instance().set(&DataKey::SchemaVersion, &2u32);
        }
        remaining
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        // DEFECT under `broken-upgrade-auth`: the admin is read but never required to
        // authorize, so any account can replace the contract code.
        #[cfg(not(feature = "broken-upgrade-auth"))]
        admin(&env).require_auth();
        #[cfg(feature = "broken-upgrade-auth")]
        let _ = admin(&env);
        // Deprecated in soroban-sdk 28 in favour of update_current_contract; same host
        // function underneath. Kept because it is the call the migration guides show.
        #[allow(deprecated)]
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test;
