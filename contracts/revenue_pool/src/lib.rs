#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
}

#[contract]
pub struct CalloraRevenuePool;

#[contractimpl]
impl CalloraRevenuePool {
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current_admin = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap();
        if caller != current_admin {
            panic!("Not admin");
        }
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("changed")),
            (current_admin, new_admin.clone()),
        );
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }
}
