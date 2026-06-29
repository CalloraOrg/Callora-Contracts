#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
}

#[contract]
pub struct CalloraRevenuePool;

/// Contract implementation block for [`RevenuePool`].
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
}
