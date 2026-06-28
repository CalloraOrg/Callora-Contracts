#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Vault,
    TotalSettled,
}

#[contract]
pub struct CalloraSettlement;

#[contractimpl]
impl CalloraSettlement {
    pub fn init(env: Env, vault: Address) {
        if env.storage().instance().has(&DataKey::Vault) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Vault, &vault);
        env.storage().instance().set(&DataKey::TotalSettled, &0i128);
    }

    pub fn record_deduction(env: Env, amount: i128, _request_id: u64) {
        let vault = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Vault)
            .unwrap();
        vault.require_auth();
        let total = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::TotalSettled)
            .unwrap_or(0);
        let new_total = total.checked_add(amount).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::TotalSettled, &new_total);
    }
}
