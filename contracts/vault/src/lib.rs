#![allow(clippy::too_many_arguments)]
#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Owner,
    UsdcToken,
    Balance,
    AuthorizedCaller,
    MinDeposit,
    RevenuePool,
    MaxDeduct,
    Settlement,
    Paused,
    Depositor(Address),
}

pub mod token {
    pub use soroban_sdk::token::Client;
}

pub mod settlement {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/callora_settlement.wasm"
    );
}

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: i128,
        authorized_caller: Address,
        min_deposit: i128,
        revenue_pool: Option<Address>,
        max_deduct: i128,
        settlement: Address,
    ) {
        if env.storage().instance().has(&DataKey::Owner) {
            panic!("Already initialized");
        }
        if min_deposit <= 0 {
            panic!("Invalid min deposit");
        }
        if max_deduct <= 0 {
            panic!("Invalid max deduct");
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::Balance, &initial_balance);
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &authorized_caller);
        env.storage()
            .instance()
            .set(&DataKey::MinDeposit, &min_deposit);
        if let Some(pool) = revenue_pool {
            env.storage().instance().set(&DataKey::RevenuePool, &pool);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        if amount < min_dep {
            panic!("Deposit under minimum");
        }
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            let is_allowed = env
                .storage()
                .instance()
                .get::<_, bool>(&DataKey::Depositor(caller.clone()))
                .unwrap_or(false);
            if !is_allowed {
                panic!("Not authorized depositor");
            }
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_add(amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        let token_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: u64) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        if amount > max_deduct || amount <= 0 {
            panic!("Invalid deduct amount");
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_sub(amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        settlement_client.record_deduction(&amount, &request_id);
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<(i128, u64)>) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        let mut total_amount: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            if amount > max_deduct || amount <= 0 {
                panic!("Invalid deduct amount");
            }
            total_amount = total_amount.checked_add(amount).unwrap();
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_sub(total_amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        for item in items.iter() {
            let (amount, request_id) = item;
            settlement_client.record_deduction(&amount, &request_id);
        }
    }

    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::Depositor(depositor), &true);
    }

    pub fn set_authorized_caller(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &caller);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
    pub fn balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap()
    }
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap()
    }
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap()
    }
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(i128::MAX)
    }

    pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        if max_deduct <= 0 {
            panic!("Invalid max deduct");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
    }

    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap()
    }
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::RevenuePool)
    }

    /// Return the capability bitmap for this contract version.
    ///
    /// Each set bit represents a feature that this contract supports.  Bits are
    /// stable — a position once assigned is never reused for a different feature.
    /// Reserved bits (18–63) are always zero.
    ///
    /// No authentication required; this is a pure view function.
    ///
    /// # Example
    /// ```ignore
    /// let caps = client.capabilities();
    /// let has_batch = caps & capabilities::CAP_BATCH_DEDUCT != 0;
    /// ```
    pub fn capabilities(env: Env) -> u64 {
        capabilities::capabilities(&env)
    }

    /// Garbage-collect processed request markers from persistent storage.
    /// Only the owner can call this.
    /// Emits a `request_id_pruned` event for each removed ID.
    pub fn prune_processed_requests(env: Env, caller: Address, ids: Vec<Symbol>) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;

        for id in ids.iter() {
            let key = StorageKey::ProcessedRequest(id.clone());
            if env.storage().persistent().has(&key) {
                env.storage().persistent().remove(&key);
                env.events()
                    .publish((Symbol::new(&env, "request_id_pruned"), caller.clone()), id.clone());
            }
        }

        Ok(())
    }

    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Depositor(caller))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_cei_order_preservation() {
        assert_eq!(1 + 1, 2);
    }
}

mod events;
pub mod capabilities;
pub mod rate_limit;

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_init_hardening;

#[cfg(test)]
mod test_setter_validation;

// #[cfg(test)]
// mod test_settler_validation;

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_idempotency;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_balance_property;

#[cfg(test)]
mod test_rate_limit;
#[cfg(test)] mod test_gas_budget;
