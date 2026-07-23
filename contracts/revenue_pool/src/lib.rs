#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol, Vec};

mod emergency;
mod events;

const USDC_KEY: &str = "usdc";
const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_AMOUNT_NOT_POSITIVE: &str = "amount must be positive";
const ERR_NOT_INITIALIZED: &str = "revenue pool not initialized";
const ERR_INSUFFICIENT_BALANCE: &str = "insufficient USDC balance";
const LIFETIME_THRESHOLD: u32 = 50000;
const BUMP_AMOUNT: u32 = 50000;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    UsdcToken,
    Paused,
}

#[contract]
pub struct CalloraRevenuePool;

/// Contract implementation block for [`RevenuePool`].
#[contractimpl]
impl CalloraRevenuePool {
    pub fn init(env: Env, admin: Address, usdc_token: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::UsdcToken, &usdc_token);
        env.storage().instance().set(&DataKey::Paused, &false);
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

    /// Propose an emergency drain of USDC from the revenue pool to a designated address.
    ///
    /// Only the current admin may call this function. The drain is subject to a
    /// 24-hour timelock before it can be executed via [`Self::execute_emergency_drain`].
    /// If a previous drain proposal exists, it is replaced.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin; must authorize.
    /// * `to` - Address that will receive the drained USDC (typically the treasury).
    /// * `amount` - Amount of USDC in base units to drain. Must be positive.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If `amount` is zero or negative.
    /// * If `to` is the contract itself.
    /// * If the revenue pool has not been initialized.
    ///
    /// # Events
    /// Emits `emergency_drain_proposed` with `admin` as topic and a
    /// [`PendingEmergencyDrain`] as data.
    pub fn propose_emergency_drain(env: Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }
        // Validate initialization by reading USDC address.
        env.storage()
            .instance()
            .get::<_, Address>(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        if to == env.current_contract_address() {
            panic!("invalid recipient: cannot drain to the contract itself");
        }

        let proposed_at = env.ledger().timestamp();
        let execute_after = proposed_at
            .checked_add(emergency::EMERGENCY_DRAIN_TIMELOCK_SECONDS)
            .expect("timelock overflow");

        let drain = emergency::PendingEmergencyDrain {
            to: to.clone(),
            amount,
            proposed_at,
            execute_after,
        };

        let inst = env.storage().instance();
        inst.set(
            &Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY),
            &drain,
        );
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events().publish(
            (events::event_emergency_drain_proposed(&env), admin),
            drain,
        );
    }

    /// Execute a previously proposed emergency drain after the timelock has expired.
    ///
    /// Only the current admin may call this function. Transfers the proposed
    /// USDC amount from this contract to the destination address specified in the
    /// pending proposal. The proposal is consumed on success to prevent replay.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin; must authorize.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If no emergency drain proposal is pending.
    /// * If the 24-hour timelock has not yet expired.
    /// * If the contract's USDC balance is less than the proposed amount.
    /// * If the revenue pool has not been initialized.
    ///
    /// # Events
    /// Emits `emergency_drain_executed` with `admin` as topic and
    /// `(to, amount, proposed_at, executed_at)` as data.
    pub fn execute_emergency_drain(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }

        let inst = env.storage().instance();
        let drain: emergency::PendingEmergencyDrain = inst
            .get(&Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY))
            .expect("no pending emergency drain");

        let executed_at = env.ledger().timestamp();
        if executed_at < drain.execute_after {
            panic!("emergency drain timelock has not expired");
        }

        let usdc_address: Address = inst
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();

        if usdc.balance(&contract_address) < drain.amount {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
        }

        // Consume the proposal before transferring to prevent replay.
        inst.remove(&Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        usdc.transfer(&contract_address, &drain.to, &drain.amount);

        env.events().publish(
            (events::event_emergency_drain_executed(&env), admin),
            (drain.to, drain.amount, drain.proposed_at, executed_at),
        );
    }

    /// Cancel a pending emergency drain proposal.
    ///
    /// Only the current admin may call this function.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin; must authorize.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If no emergency drain proposal is pending.
    ///
    /// # Events
    /// Emits `emergency_drain_cancelled` with `admin` as topic and the cancelled
    /// [`PendingEmergencyDrain`] as data.
    pub fn cancel_emergency_drain(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }

        let inst = env.storage().instance();
        let drain: emergency::PendingEmergencyDrain = inst
            .get(&Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY))
            .expect("no pending emergency drain");

        inst.remove(&Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events().publish(
            (events::event_emergency_drain_cancelled(&env), admin),
            drain,
        );
    }

    /// Return the pending emergency drain proposal, or `None` if none is pending.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    ///
    /// # Returns
    /// `Some(PendingEmergencyDrain)` with the proposal details, or `None`.
    pub fn get_pending_emergency_drain(env: Env) -> Option<emergency::PendingEmergencyDrain> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, emergency::EMERGENCY_DRAIN_KEY))
    }

    /// Return the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .expect("revenue pool not initialized")
    }

    /// Return the configured USDC token address.
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .expect(ERR_NOT_INITIALIZED)
    }

    /// Return this contract's on-ledger USDC balance.
    pub fn balance(env: Env) -> i128 {
        let usdc_addr = Self::get_usdc_token(env.clone());
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.balance(&env.current_contract_address())
    }

    /// Pause the revenue pool, blocking distributions.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    /// Unpause the revenue pool.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Check if the revenue pool is paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Distribute USDC from this contract to a developer wallet.
    pub fn distribute(env: Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if Self::is_paused(env.clone()) {
            panic!("revenue pool is paused");
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }
        if to == env.current_contract_address() {
            panic!("invalid recipient: cannot distribute to the contract itself");
        }
        let usdc_addr = Self::get_usdc_token(env.clone());
        let usdc = token::Client::new(&env, &usdc_addr);
        let contract_addr = env.current_contract_address();
        if usdc.balance(&contract_addr) < amount {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
        }
        usdc.transfer(&contract_addr, &to, &amount);
    }

    /// Distribute USDC from this contract to multiple developer wallets atomically.
    pub fn batch_distribute(env: Env, caller: Address, payments: Vec<(Address, i128)>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if Self::is_paused(env.clone()) {
            panic!("revenue pool is paused");
        }
        let n = payments.len();
        if n == 0 {
            panic!("batch_distribute requires at least one payment");
        }
        let contract_addr = env.current_contract_address();
        let mut total: i128 = 0;
        for payment in payments.iter() {
            let (to, amount) = payment;
            if amount <= 0 {
                panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
            }
            if to == contract_addr {
                panic!("invalid recipient: cannot distribute to the contract itself");
            }
            total = total.checked_add(amount).expect("total overflow");
        }
        let usdc_addr = Self::get_usdc_token(env.clone());
        let usdc = token::Client::new(&env, &usdc_addr);
        if usdc.balance(&contract_addr) < total {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
        }
        for payment in payments.iter() {
            let (to, amount) = payment;
            usdc.transfer(&contract_addr, &to, &amount);
        }
    }
}
