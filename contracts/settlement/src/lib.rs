#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, String, Symbol, Vec};

mod admin;
mod errors;
mod timelock;
pub use errors::SettlementError;
pub use timelock::{PendingDeveloperMigration, DEVELOPER_MIGRATION_TIMELOCK_SECONDS};

mod types;
pub use types::*;


#[contract]
pub struct CalloraSettlement;

#[contractimpl]
impl CalloraSettlement {
    /// Initialize the settlement contract with admin and vault address.
    ///
    /// Persists admin + registered vault, initializes an empty developer index,
    /// and stores a timestamped global pool.
    ///
    /// Storage keys written:
    /// - `StorageKey::Admin`
    /// - `StorageKey::Vault`
    /// - `StorageKey::GlobalPool`
    ///
    /// # Panics
    /// Panics if the contract is already initialized.
    /// Panics if admin and vault_address are the same.
    /// Panics if admin is the contract's own address.
    /// Panics if vault_address is the contract's own address.
    pub fn init(env: Env, admin: Address, vault_address: Address) {
        admin.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::Admin) {
            env.panic_with_error(SettlementError::AlreadyInitialized);
        }
        if admin == vault_address {
            panic!("invalid config: admin and vault_address must be distinct");
        }
        if admin == env.current_contract_address() {
            panic!("invalid config: admin cannot be the contract itself");
        }
        if vault_address == env.current_contract_address() {
            panic!("invalid config: vault_address cannot be the contract itself");
        }
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::Vault, &vault_address);
        let global_pool = GlobalPool {
            total_balance: 0,
            last_updated: env.ledger().timestamp(),
        };
        inst.set(&StorageKey::GlobalPool, &global_pool);
    }

    /// Receive payment from vault and credit to pool or developer balance.
    ///
    /// # Arguments
    /// * `caller` - Must be authorized vault address or admin
    /// * `amount` - Payment amount in token micro-units; must be > 0
    /// * `to_pool` - If true, credit global pool; if false, credit a specific developer
    /// * `developer` - Required when `to_pool=false`; ignored when `to_pool=true`
    /// * `token` - The token contract address for this payment
    ///
    /// # Access Control
    /// Only the registered vault address or admin can call this function.
    ///
    /// # Persistent Storage Operations
    /// When crediting to developer balance:
    /// - Performs O(1) point-read from persistent storage for the developer + token
    /// - Updates the specific developer's balance in persistent storage
    /// - Extends persistent TTL for the developer's balance entry
    /// - Adds developer to index if not already present
    /// - Does NOT iterate any maps; only point operations
    ///
    /// # Events
    /// Always emits `payment_received`. Also emits `balance_credited` when `to_pool=false`.
    ///
    /// # Arithmetic Safety
    /// Credits use checked arithmetic:
    /// - Pool credits panic with `"pool balance overflow"` on `i128` overflow.
    /// - Developer credits panic with `"developer balance overflow"` on `i128` overflow.
    pub fn receive_payment(
        env: Env,
        caller: Address,
        amount: i128,
        to_pool: bool,
        developer: Option<Address>,
        token: Address,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());
        if amount <= 0 {
            env.panic_with_error(SettlementError::AmountNotPositive);
        }
        let inst = env.storage().instance();
        if to_pool {
            if developer.is_some() {
                env.panic_with_error(SettlementError::DeveloperMustBeNone);
            }
            let mut global_pool = Self::get_global_pool(env.clone());
            global_pool.total_balance = global_pool
                .total_balance
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow));
            global_pool.last_updated = env.ledger().timestamp();
            inst.set(&StorageKey::GlobalPool, &global_pool);
            env.events().publish(
                (events::event_payment_received(&env), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: true,
                    developer: None,
                    token: token.clone(),
                },
            );
        } else {
            let dev_address = developer
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperRequired));

            // Per-token balance key: (developer, token)
            let balance_key = StorageKey::DeveloperBalance(dev_address.clone(), token.clone());

            // Read current balance from persistent storage
            let current_balance: i128 = env
                .storage()
                .persistent()
                .get(&balance_key)
                .unwrap_or(0i128);
            let new_balance = current_balance
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));

            // Write to persistent storage with TTL extension
            env.storage().persistent().set(&balance_key, &new_balance);

            // Extend TTL for the developer's balance entry (persistent storage live for 1 year)
            env.storage()
                .persistent()
                .extend_ttl(&balance_key, 50000, 50000);

            // Add developer to index in sorted order if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            Self::sorted_insert(&env, &mut index, dev_address.clone());
            inst.set(&StorageKey::DeveloperIndex, &index);

            env.events().publish(
                (events::event_payment_received(&env), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: false,
                    developer: Some(dev_address.clone()),
                    token: token.clone(),
                },
            );
            env.events().publish(
                (events::event_balance_credited(&env), dev_address.clone()),
                BalanceCreditedEvent {
                    developer: dev_address,
                    amount,
                    new_balance,
                    token,
                },
            );
        }
    }

    /// Atomically credit multiple developer balances in a single call.
    ///
    /// # Arguments
    /// * `caller` - Must be the registered vault address or admin
    /// * `items` - Vec of `(developer_address, amount)` pairs; 1–[`MAX_BATCH_SIZE`] entries
    /// * `token` - The token contract address for this batch payment
    ///
    /// # Access Control
    /// Only the registered vault address or admin can call this function.
    ///
    /// # Validation
    /// All amounts must be `> 0`. Empty and oversized batches are rejected before any state change.
    ///
    /// # Atomicity
    /// All validation runs before any state is written. A failure on any item leaves the
    /// contract state unchanged.
    ///
    /// # Events
    /// Emits `balance_credited` for each item in the batch.
    ///
    /// # Panics
    /// * `"batch_receive_payment requires at least one item"` — empty batch
    /// * `"batch too large"` — more than [`MAX_BATCH_SIZE`] items
    /// * `"amount must be positive"` — any amount ≤ 0
    /// * `"developer balance overflow"` — `i128` overflow on any developer balance
    pub fn batch_receive_payment(
        env: Env,
        caller: Address,
        items: Vec<(Address, i128)>,
        token: Address,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());

        let n = items.len();
        assert!(n > 0, "batch_receive_payment requires at least one item");
        assert!(n <= MAX_BATCH_SIZE, "batch too large");

        // Validate all amounts before touching state.
        for item in items.iter() {
            let (_, amount) = item;
            assert!(amount > 0, "amount must be positive");
        }

        let inst = env.storage().instance();

        for item in items.iter() {
            let (dev, amount) = item;
            let balance_key = StorageKey::DeveloperBalance(dev.clone(), token.clone());
            let current: i128 = env
                .storage()
                .persistent()
                .get(&balance_key)
                .unwrap_or(0);
            let new_balance = current
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));
            env.storage()
                .persistent()
                .set(&balance_key, &new_balance);
            env.storage()
                .persistent()
                .extend_ttl(&balance_key, 50000, 50000);
            // Add to index in sorted order if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            Self::sorted_insert(&env, &mut index, dev.clone());
            inst.set(&StorageKey::DeveloperIndex, &index);
            env.events().publish(
                (events::event_balance_credited(&env), dev.clone()),
                BalanceCreditedEvent {
                    developer: dev.clone(),
                    amount,
                    new_balance,
                    token: token.clone(),
                },
            );
        }
    }

    /// Get current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::NotInitialized))
    }

    /// Get registered vault address
    pub fn get_vault(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Vault)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::NotInitialized))
    }

    /// Get global pool information
    pub fn get_global_pool(env: Env) -> GlobalPool {
        env.storage()
            .instance()
            .get(&StorageKey::GlobalPool)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::NotInitialized))
    }

    /// Get developer balance for a specific token.
    ///
    /// Performs a direct O(1) persistent storage lookup for the specified
    /// developer's balance denominated in `token`.
    ///
    /// # Arguments
    /// * `developer` - Developer address to query
    /// * `token` - Token contract address
    ///
    /// # Returns
    /// Balance in token micro-units, or 0 if no balance recorded
    ///
    /// # Safety
    /// Safe for all use cases; uses persistent storage with TTL.
    pub fn get_developer_balance(env: Env, developer: Address, token: Address) -> i128 {
        if !env.storage().instance().has(&StorageKey::Admin) {
            env.panic_with_error(SettlementError::NotInitialized);
        }
        env.storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(developer, token))
            .unwrap_or(0)
    }

    /// Propose moving a developer's current balance to a replacement address.
    ///
    /// The current admin must authorize this state change. If the admin is a
    /// Stellar multisig account, `require_auth` enforces that account's signer
    /// thresholds. The proposal snapshots the source balance and becomes
    /// executable after [`DEVELOPER_MIGRATION_TIMELOCK_SECONDS`]. Re-proposing
    /// for the same source replaces the prior proposal and restarts the delay.
    ///
    /// # Errors
    /// Panics with a typed [`SettlementError`] when the caller is unauthorized,
    /// the addresses are equal or unsafe, the source balance is empty, or the
    /// execution timestamp cannot be represented.
    pub fn propose_balance_migration(env: Env, caller: Address, from: Address, to: Address) {
        admin::propose_balance_migration(&env, &caller, &from, &to);
    }

    /// Execute a matured developer balance migration proposal.
    ///
    /// The current admin must authorize execution independently of proposal.
    /// Exactly the amount approved at proposal time is moved; credits received
    /// afterward remain at `from`. The destination balance addition is checked
    /// for overflow, and the consumed proposal is removed to prevent replay.
    ///
    /// # Events
    /// Emits `admin_migration` with [`AdminMigrationEvent`] after success.
    pub fn execute_balance_migration(env: Env, caller: Address, from: Address) {
        admin::execute_balance_migration(&env, &caller, &from);
    }

    /// Return the pending migration for `from`, if one exists.
    pub fn get_balance_migration(env: Env, from: Address) -> Option<PendingDeveloperMigration> {
        timelock::get_pending_migration(&env, &from)
    }

    /// Configure the USDC token contract address.
    ///
    /// Only the current admin may set the on-chain USDC token address that this
    /// contract will use to execute withdrawals.
    pub fn set_usdc_token(env: Env, caller: Address, usdc_address: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        if usdc_address == env.current_contract_address() {
            panic!("invalid config: usdc_token cannot be the contract itself");
        }
        env.storage()
            .instance()
            .set(&StorageKey::Usdc, &usdc_address);
    }

    fn get_usdc_token(env: Env) -> Result<Address, SettlementError> {
        env.storage()
            .instance()
            .get(&StorageKey::Usdc)
            .ok_or(SettlementError::UsdcTokenNotConfigured)
    }

    /// Migrate a developer's balance from the legacy single-token format
    /// `DeveloperBalanceV1(dev)` to the new per-token format
    /// `DeveloperBalance(dev, usdc_token)`.
    ///
    /// After migration, the old entry is removed from storage.  This is a
    /// one-way, idempotent operation: calling it again for the same developer
    /// will see a zero legacy balance and be a no-op.
    ///
    /// # Arguments
    /// * `caller` – Must be the current admin.
    /// * `developer` – The developer address whose balance to migrate.
    ///
    /// # Errors
    /// * `SettlementError::Unauthorized` – caller is not admin.
    /// * `SettlementError::UsdcTokenNotConfigured` – no USDC token has been set
    ///   via `set_usdc_token`, which is required because the legacy format was
    ///   single-token (USDC).
    ///
    /// # Events
    /// Does not emit an event; the state change is observable via balance reads.
    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            return Err(SettlementError::Unauthorized);
        }

        let usdc = Self::get_usdc_token(env.clone())?;
        let legacy_key = StorageKey::DeveloperBalanceV1(developer.clone());
        let pers = env.storage().persistent();

        // Read legacy balance.
        let balance: i128 = pers.get(&legacy_key).unwrap_or(0);
        if balance == 0 {
            // Nothing to migrate — still ok, idempotent.
            return Ok(());
        }

        // Write new per-token entry.
        let new_key = StorageKey::DeveloperBalance(developer.clone(), usdc);
        pers.set(&new_key, &balance);
        pers.extend_ttl(&new_key, 50000, 50000);

        // Remove legacy entry.
        pers.remove(&legacy_key);

        Ok(())
    }

    /// Withdraw developer balance as a specific token to a designated recipient
    /// (defaults to the developer).
    ///
    /// Requires the developer to authorize the request and the requested amount
    /// to be positive and covered by the tracked developer balance.
    ///
    /// # Arguments
    /// * `developer` - Address of the developer withdrawing their balance
    /// * `amount` - Amount to withdraw in token micro-units
    /// * `to` - Optional recipient address; if `None`, defaults to `developer`
    /// * `token` - The token contract address to withdraw
    ///
    /// # Errors
    /// - `AmountNotPositive` if amount is ≤ 0
    /// - `InsufficientDeveloperBalance` if developer balance < amount
    /// - `DailyWithdrawCapExceeded` if daily cap is exceeded
    /// - `DeveloperBalanceUnderflow` if subtraction underflows
    /// - `InsufficientContractBalance` if contract has insufficient token balance
    /// - Panics if `to` is the contract's own address
    pub fn withdraw_developer_balance(
        env: Env,
        developer: Address,
        amount: i128,
        to: Option<Address>,
        token: Address,
    ) -> Result<(), SettlementError> {
        developer.require_auth();
        if amount <= 0 {
            return Err(SettlementError::AmountNotPositive);
        }

        let recipient = to.unwrap_or_else(|| developer.clone());
        let contract_address = env.current_contract_address();
        if recipient == contract_address {
            panic!("invalid recipient: cannot withdraw to contract itself");
        }

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), token.clone());
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&balance_key)
            .unwrap_or(0);
        if amount > current_balance {
            return Err(SettlementError::InsufficientDeveloperBalance);
        }

        let cap: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DailyWithdrawCap(developer.clone()))
            .unwrap_or(0);
        if cap > 0 {
            let today = env.ledger().timestamp() / 86400;
            let mut daily = env
                .storage()
                .persistent()
                .get::<_, DailyWithdrawState>(&StorageKey::WithdrawalToday(developer.clone()))
                .unwrap_or(DailyWithdrawState { day: today, amount: 0 });
            if daily.day != today {
                daily.day = today;
                daily.amount = 0;
            }
            if daily.amount.checked_add(amount).is_none_or(|sum| sum > cap) {
                return Err(SettlementError::DailyWithdrawCapExceeded);
            }
        }

        let new_balance = current_balance
            .checked_sub(amount)
            .ok_or(SettlementError::DeveloperBalanceUnderflow)?;

        let token_client = token::Client::new(&env, &token);

        if token_client.balance(&contract_address) < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        token_client.transfer(&contract_address, &recipient, &amount);

        env.storage().persistent().set(&balance_key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&balance_key, 50000, 50000);

        // Update daily withdrawal accumulator
        let today = env.ledger().timestamp() / 86400;
        let mut daily = env
            .storage()
            .persistent()
            .get::<_, DailyWithdrawState>(&StorageKey::WithdrawalToday(developer.clone()))
            .unwrap_or(DailyWithdrawState { day: today, amount: 0 });
        if daily.day != today {
            daily.day = today;
            daily.amount = 0;
        }
        daily.amount = daily.amount.saturating_add(amount);
        env.storage()
            .persistent()
            .set(&StorageKey::WithdrawalToday(developer.clone()), &daily);
        env.storage()
            .persistent()
            .extend_ttl(&StorageKey::WithdrawalToday(developer.clone()), 50000, 50000);

        env.events().publish(
            (events::event_developer_withdraw(&env), developer.clone()),
            DeveloperWithdrawEvent {
                developer,
                amount,
                remaining_balance: new_balance,
                to: recipient,
                token,
            },
        );

        Ok(())
    }

    /// Set the daily withdrawal cap for a developer (admin only).
    ///
    /// A cap of `0` means unlimited (no daily limit enforced).
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Events
    /// Emits `daily_withdraw_cap_changed` with the developer and new cap.
    pub fn set_daily_withdraw_cap(env: Env, caller: Address, developer: Address, cap: i128) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.storage()
            .persistent()
            .set(&StorageKey::DailyWithdrawCap(developer.clone()), &cap);
        env.storage()
            .persistent()
            .extend_ttl(&StorageKey::DailyWithdrawCap(developer.clone()), 50000, 50000);

        env.events().publish(
            (events::event_daily_withdraw_cap_changed(&env), caller),
            DailyWithdrawCapChanged { developer, new_cap: cap },
        );
    }

    /// Get the daily withdrawal cap for a developer.
    ///
    /// Returns `0` if no cap has been set (meaning unlimited).
    pub fn get_daily_withdraw_cap(env: Env, developer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::DailyWithdrawCap(developer))
            .unwrap_or(0)
    }

    /// Get the amount a developer has already withdrawn today.
    ///
    /// Returns `0` if no withdrawal has been made today.
    pub fn get_withdrawal_today(env: Env, developer: Address) -> i128 {
        let state: Option<DailyWithdrawState> = env
            .storage()
            .persistent()
            .get(&StorageKey::WithdrawalToday(developer));
        match state {
            Some(s) if s.day == env.ledger().timestamp() / 86400 => s.amount,
            _ => 0,
        }
    }

    /// Admin-only escape hatch to manually credit a developer balance for a
    /// specific token.
    ///
    /// This function is designed for operational edge cases where a developer
    /// must be credited outside the normal `receive_payment` flow (e.g.,
    /// off-chain payment reconciliation, dispute resolution). It does **not**
    /// move on-ledger tokens and is treated as an audited administrative inflow.
    ///
    /// # Arguments
    /// * `caller` - Must be the current admin address.
    /// * `developer` - Address of the developer to credit.
    /// * `amount` - Amount in token micro-units; must be `> 0`.
    /// * `token` - The token contract address for this credit.
    /// * `reason` - On-chain reason code (Symbol); used for auditability.
    ///   The Soroban SDK enforces a 32-byte maximum on Symbol values at
    ///   construction, so a reason Symbol received here is always ≤ 32 bytes.
    ///
    /// # Panics
    /// * `SettlementError::Unauthorized` — caller is not admin.
    /// * `SettlementError::AmountNotPositive` — amount is zero or negative.
    /// * `SettlementError::DeveloperOverflow` — i128 overflow on developer balance.
    ///
    /// # Events
    /// Emits `developer_force_credited` with
    /// `(developer, amount, token, reason, new_balance)`.
    pub fn force_credit_developer(
        env: Env,
        caller: Address,
        developer: Address,
        amount: i128,
        token: Address,
        reason: Symbol,
    ) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if amount <= 0 {
            env.panic_with_error(SettlementError::AmountNotPositive);
        }

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), token.clone());
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&balance_key)
            .unwrap_or(0i128);
        let new_balance = current_balance
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));

        env.storage().persistent().set(&balance_key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&balance_key, 50000, 50000);

        let mut index: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));
        if !index.iter().any(|addr| addr == developer) {
            index.push_back(developer.clone());
            env.storage()
                .instance()
                .set(&StorageKey::DeveloperIndex, &index);
        }

        env.events().publish(
            (Symbol::new(&env, "developer_force_credited"), developer.clone()),
            DeveloperForceCreditedEvent {
                developer,
                amount,
                reason,
                new_balance,
            },
        );
    }

    /// Get all developer balances for a specific token (admin only).
    ///
    /// **CRITICAL**: Uses developer index for iteration; order is based on index insertion order.
    /// Use this function only for administrative queries or reporting purposes.
    /// For production integrations with many developers (>100), implement off-chain indexing
    /// by listening to `BalanceCreditedEvent` and maintaining a local database.
    ///
    /// # Arguments
    /// * `caller` - Must be the current admin address.
    /// * `token` - Token contract address to query balances for.
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Iteration Behavior
    /// - Uses developer index Vec for iteration; order is based on credit insertion order
    /// - **Small index (< 100 entries)**: Safe to iterate; yields current state
    /// - **Large index (> 100 entries)**: Consider off-chain indexing to avoid excessive gas costs
    /// - **Order guarantees**: Based on insertion order (first credit = first in index)
    ///
    /// # Returns
    /// Result containing a Vec of DeveloperBalance records or a gas exhaustion error.
    /// Iteration order is based on index insertion order.
    ///
    /// # Use Cases
    /// ✅ Administrative dashboards and reporting
    /// ✅ Audit compliance queries
    /// ✅ Contract state verification
    /// ⚠️  Automatic routing based on iteration order (order is insertion-order stable but may not match business logic)
    /// ❌ Deterministic selection of developers
    ///
    /// # Performance
    /// Gas cost scales with number of developers:
    /// - 50 developers: ~500 gas
    /// - 100 developers: ~1,000 gas
    /// - 500 developers: ~5,000 gas (consider off-chain indexing)
    pub fn get_all_developer_balances(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<Vec<DeveloperBalance>, SettlementError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        // Guard against unbounded iteration on large indexes.
        // Callers with > 100 developers must use `get_developer_balances_page` instead.
        if index.len() > MAX_DEVELOPER_BALANCES_PAGE_SIZE {
            return Err(SettlementError::GasExhaustionRisk);
        }

        let mut result = Vec::new(&env);
        for address in index.iter() {
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(address.clone(), token.clone()))
                .unwrap_or(0i128);
            result.push_back(DeveloperBalance {
                address: address.clone(),
                token: token.clone(),
                balance,
            });
        }
        Ok(result)
    }

    /// Get a paginated slice of developer balances for a token (admin only).
    ///
    /// This method avoids expensive full-index iteration by returning
    /// a bounded window of developer balance records. Use it for
    /// admin dashboards and off-chain pagination.
    ///
    /// # Arguments
    /// * `caller` - Must be the current admin address.
    /// * `start` - Zero-based start index.
    /// * `limit` - Maximum records to return; capped at 100.
    /// * `token` - Token contract address to query balances for.
    pub fn get_developer_balances_page(
        env: Env,
        caller: Address,
        start: u32,
        limit: u32,
        token: Address,
    ) -> Result<Vec<DeveloperBalance>, SettlementError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }

        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        if limit == 0 || start >= index.len() {
            return Ok(Vec::new(&env));
        }

        let end = start
            .saturating_add(limit.min(MAX_DEVELOPER_BALANCES_PAGE_SIZE))
            .min(index.len());
        let mut result = Vec::new(&env);
        let mut cursor = 0;
        for address in index.iter() {
            if cursor >= start && cursor < end {
                let balance = env
                    .storage()
                    .persistent()
                    .get(&StorageKey::DeveloperBalance(address.clone(), token.clone()))
                    .unwrap_or(0);
                result.push_back(DeveloperBalance {
                    address: address.clone(),
                    token: token.clone(),
                    balance,
                });
            }
            if cursor >= end {
                break;
            }
            cursor += 1;
        }
        Ok(result)
    }

    /// Cursor-based paginated developer balances for a specific token (admin only).
    ///
    /// Returns up to `limit` developer balance records starting **after** the
    /// supplied `cursor` address (exclusive), or from the beginning of the
    /// sorted index when `cursor` is `None`.  The index is maintained in
    /// deterministic ascending order by address bytes, so pages are stable
    /// across interleaved `receive_payment` calls for developers that sort
    /// **after** the cursor.
    ///
    /// # Arguments
    /// * `caller`  – Must be the current admin; must authorize.
    /// * `cursor`  – Exclusive start position.  Pass `None` for the first page;
    ///               pass the `next_cursor` returned by the previous call for
    ///               subsequent pages.
    /// * `limit`   – Maximum records to return; capped at
    ///               [`MAX_DEVELOPER_BALANCES_PAGE_SIZE`] (100).
    /// * `token`   – Token contract address to query balances for.
    ///
    /// # Returns
    /// `(page, next_cursor)` where:
    /// * `page`         – Vec of [`DeveloperBalance`] for this page (may be empty).
    /// * `next_cursor`  – `Some(address)` of the last record returned, which can be
    ///                    passed as `cursor` on the next call; `None` when this is the
    ///                    last page.
    ///
    /// # Access Control
    /// Admin only.
    ///
    /// # Errors
    /// * [`SettlementError::NotInitialized`] – contract not yet initialised.
    /// * [`SettlementError::Unauthorized`]   – caller is not the admin.
    pub fn get_developer_balances_cursor(
        env: Env,
        caller: Address,
        cursor: Option<Address>,
        limit: u32,
        token: Address,
    ) -> (Vec<DeveloperBalance>, Option<Address>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }

        // Cap limit to the maximum safe page size.
        let effective_limit = if limit == 0 {
            return (Vec::new(&env), None);
        } else {
            limit.min(MAX_DEVELOPER_BALANCES_PAGE_SIZE)
        };

        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result: Vec<DeveloperBalance> = Vec::new(&env);
        // When a cursor is supplied we skip addresses up to and including it.
        // `past_cursor` becomes true once we have consumed the cursor entry (or
        // immediately when cursor is None).
        let mut past_cursor = cursor.is_none();
        let mut last_address: Option<Address> = None;

        for address in index.iter() {
            if !past_cursor {
                if let Some(ref c) = cursor {
                    if &address == c {
                        // We've reached the cursor entry; start collecting from next.
                        past_cursor = true;
                    }
                }
                continue;
            }

            let balance: i128 = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(address.clone(), token.clone()))
                .unwrap_or(0i128);
            result.push_back(DeveloperBalance {
                address: address.clone(),
                token: token.clone(),
                balance,
            });
            last_address = Some(address.clone());

            if result.len() >= effective_limit {
                break;
            }
        }

        // next_cursor is the address of the last record returned, provided the
        // page is full (meaning there may be more records).  When the page is
        // not full we have reached the end of the index.
        let next_cursor = if result.len() >= effective_limit {
            last_address
        } else {
            None
        };

        (result, next_cursor)
    }

    /// Return the remaining TTL for each storage key category.
    ///
    /// # Parameters
    /// - `developer_addresses` — optional list of developers to check. If empty, the index is used.
    pub fn get_storage_ttl(env: Env, developer_addresses: Vec<Address>) -> Vec<StorageEntryTtl> {
        let mut result = Vec::new(&env);

        // 1. Instance Storage
        let instance_ttl = {
            #[cfg(any(test, feature = "testutils"))]
            {
                env.storage().instance().get_ttl()
            }
            #[cfg(not(any(test, feature = "testutils")))]
            {
                17_280 * 60
            }
        };
        result.push_back(StorageEntryTtl {
            category: String::from_str(&env, "Instance"),
            key_desc: String::from_str(&env, "Instance"),
            storage_type: String::from_str(&env, "Instance"),
            ttl: instance_ttl,
            threshold: 17_280 * 30,
            bump_amount: 17_280 * 60,
        });

        // Determine which developer addresses to inspect
        let devs = if developer_addresses.len() > 0 {
            developer_addresses
        } else {
            env.storage()
                .instance()
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env))
        };

        for dev in devs.iter() {
            // Check DeveloperBalance (Persistent)
            let bal_key = StorageKey::DeveloperBalance(dev.clone());
            if env.storage().persistent().has(&bal_key) {
                let ttl = {
                    #[cfg(any(test, feature = "testutils"))]
                    {
                        env.storage().persistent().get_ttl(&bal_key)
                    }
                    #[cfg(not(any(test, feature = "testutils")))]
                    {
                        50000
                    }
                };
                result.push_back(StorageEntryTtl {
                    category: String::from_str(&env, "DeveloperBalance"),
                    key_desc: String::from_str(&env, "DeveloperBalance"),
                    storage_type: String::from_str(&env, "Persistent"),
                    ttl,
                    threshold: 50000,
                    bump_amount: 50000,
                });
            }

            // Check WithdrawalToday (Persistent)
            let today_key = StorageKey::WithdrawalToday(dev.clone());
            if env.storage().persistent().has(&today_key) {
                let ttl = {
                    #[cfg(any(test, feature = "testutils"))]
                    {
                        env.storage().persistent().get_ttl(&today_key)
                    }
                    #[cfg(not(any(test, feature = "testutils")))]
                    {
                        50000
                    }
                };
                result.push_back(StorageEntryTtl {
                    category: String::from_str(&env, "WithdrawalToday"),
                    key_desc: String::from_str(&env, "WithdrawalToday"),
                    storage_type: String::from_str(&env, "Persistent"),
                    ttl,
                    threshold: 50000,
                    bump_amount: 50000,
                });
            }

            // Check DailyWithdrawCap (Persistent)
            let cap_key = StorageKey::DailyWithdrawCap(dev.clone());
            if env.storage().persistent().has(&cap_key) {
                let ttl = {
                    #[cfg(any(test, feature = "testutils"))]
                    {
                        env.storage().persistent().get_ttl(&cap_key)
                    }
                    #[cfg(not(any(test, feature = "testutils")))]
                    {
                        50000
                    }
                };
                result.push_back(StorageEntryTtl {
                    category: String::from_str(&env, "DailyWithdrawCap"),
                    key_desc: String::from_str(&env, "DailyWithdrawCap"),
                    storage_type: String::from_str(&env, "Persistent"),
                    ttl,
                    threshold: 50000,
                    bump_amount: 50000,
                });
            }
        }

        result
    }

    /// Return the pending admin address, or `None` if no two-step admin transfer is in progress.
    ///
    /// Integrators can poll this to detect an in-flight admin handover
    /// before `accept_admin` is called.
    ///
    /// # Returns
    /// `Some(Address)` of the nominated admin, or `None` when no transfer is pending.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    /// Nominate a new admin (admin only).
    ///
    /// # Arguments
    /// * `caller` - Current admin address; must match stored admin
    /// * `new_admin` - Address to nominate as new admin
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Security
    /// This implements a two-step admin transfer process:
    /// 1. Current admin calls `set_admin()` to nominate new admin
    /// 2. Nominated admin must call `accept_admin()` to complete transfer
    ///
    /// This prevents accidental admin loss and ensures the new admin
    /// has control of their private keys before gaining privileges.
    ///
    /// # Events
    /// Emits `admin_nominated` event with current and new admin addresses.
    ///
    /// # Panics
    /// Panics if caller is not the current admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events().publish(
            (
                events::event_admin_nominated(&env),
                current_admin,
                new_admin,
            ),
            (),
        );
    }

    /// Accept the admin role (pending admin only).
    ///
    /// # Access Control
    /// Only the nominated pending admin can call this function.
    ///
    /// # Security
    /// This is the second step of the two-step admin transfer process.
    /// The nominated admin must explicitly accept, proving control of
    /// their private keys before gaining admin privileges.
    ///
    /// # Events
    /// Emits `admin_accepted` event with old and new admin addresses.
    ///
    /// # Panics
    /// Panics if there is no pending admin transfer (i.e., `set_admin()`
    /// was not called first).
    pub fn accept_admin(env: Env) {
        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&StorageKey::PendingAdmin)
            .expect("no admin transfer pending");
        pending.require_auth();

        let current = Self::get_admin(env.clone());
        inst.set(&StorageKey::Admin, &pending);
        inst.remove(&StorageKey::PendingAdmin);

        env.events()
            .publish((events::event_admin_accepted(&env), current, pending), ());
    }

    /// Cancel a pending admin transfer. Only the current admin may call this.
    ///
    /// # Arguments
    /// * `caller` - Current admin address; must match stored admin
    ///
    /// # Panics
    /// * Panics if caller is not the current admin.
    /// * Panics if no admin transfer is pending.
    pub fn cancel_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&StorageKey::PendingAdmin)
            .expect("no admin transfer pending");

        inst.remove(&StorageKey::PendingAdmin);

        env.events().publish(
            (events::event_admin_cancelled(&env), current, pending),
            (),
        );
    }

    /// Propose a new vault address (admin only).
    ///
    /// # Arguments
    /// * `caller` - Current admin address; must match stored admin
    /// * `new_vault` - New vault contract address to register
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    pub fn set_vault(env: Env, caller: Address, new_vault: Address) {
        // Backwards-compatible alias: `set_vault` now behaves like `propose_vault`.
        Self::propose_vault(env, caller, new_vault);
    }

    /// Propose a new vault address (admin only).
    ///
    /// This is the first step of a two-step vault rotation:
    /// 1. Admin calls `propose_vault()` to set `PendingVault`
    /// 2. Proposed vault (or admin) calls `accept_vault()` to activate it
    ///
    /// # Security
    /// This prevents a typo from instantly routing settlement credits to the wrong contract.
    ///
    /// # Events
    /// Emits `vault_proposed` with current and proposed vault addresses.
    ///
    /// # Panics
    /// - `"unauthorized: caller is not admin"` if caller is not admin
    /// - `"invalid config: vault cannot be the contract itself"` if proposed vault is this contract
    pub fn propose_vault(env: Env, caller: Address, new_vault: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if new_vault == env.current_contract_address() {
            panic!("invalid config: vault cannot be the contract itself");
        }

        let inst = env.storage().instance();
        let old_vault = Self::get_vault(env.clone());
        inst.set(&StorageKey::PendingVault, &new_vault);

        env.events().publish(
            (events::event_vault_proposed(&env), caller),
            VaultProposedEvent {
                current_vault: old_vault,
                proposed_vault: new_vault,
            },
        );
    }

    /// Accept the proposed vault and activate it.
    ///
    /// # Arguments
    /// * `caller` - Must be either the proposed vault address or the admin.
    ///
    /// # Events
    /// Emits `vault_accepted` with old vault, new vault, and acceptor.
    ///
    /// # Panics
    /// - `"no vault rotation pending"` if no `propose_vault()` was called
    /// - `"unauthorized: caller must be pending vault or admin"` if caller is neither
    pub fn accept_vault(env: Env, caller: Address) {
        caller.require_auth();

        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&StorageKey::PendingVault)
            .unwrap_or_else(|| panic!("no vault rotation pending"));

        let admin = Self::get_admin(env.clone());
        if caller != pending && caller != admin {
            panic!("unauthorized: caller must be pending vault or admin");
        }

        let old_vault = Self::get_vault(env.clone());
        inst.set(&StorageKey::Vault, &pending);
        inst.remove(&StorageKey::PendingVault);

        env.events().publish(
            (events::event_vault_accepted(&env), caller.clone()),
            VaultAcceptedEvent {
                old_vault,
                new_vault: pending,
                accepted_by: caller,
            },
        );
    }

    /// Internal function to require authorized caller (vault or admin)
    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone());
        let admin = Self::get_admin(env.clone());
        if caller != vault && caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
    }

    /// Admin-gated contract upgrade.
    ///
    /// Only the current admin may call. This will instruct the host to update
    /// the current contract WASM to `new_wasm_hash` and persist the version marker.
    /// Emits an `upgraded` event with the admin as topic and the new version as data.
    pub fn broadcast(env: Env, caller: Address, severity: Severity, message: String) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        let len = message.len();
        if len == 0 {
            panic!("message cannot be empty");
        }
        if len > MAX_MESSAGE_LEN {
            panic!("message length exceeds maximum of 256 characters");
        }
        env.events().publish(
            (events::event_admin_broadcast(&env), caller),
            AdminBroadcast { severity, message },
        );
    }

    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }

        // Perform the on-chain upgrade via the deployer interface.
        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());

        // Persist the version marker for on-chain queries.
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &new_wasm_hash);

        // Emit an event for indexers / audit logs.
        env.events()
            .publish((Symbol::new(&env, "upgraded"), admin), new_wasm_hash);
    }

    /// Read the stored contract version (WASM hash) as last set by `upgrade`.
    ///
    /// Returns `None` if no upgrade has been performed yet (initial deployment).
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&StorageKey::ContractVersion)
    }

    /// Insert `addr` into `index` in sorted order (ascending by raw bytes).
    ///
    /// Soroban's `Vec` does not expose a binary-search API, so we do a linear
    /// scan to find the insertion point.  The index is expected to be small
    /// (≤ `MAX_DEVELOPER_BALANCES_PAGE_SIZE`), so the O(n) cost is acceptable
    /// and the result is a deterministic, stable ordering that cursors can rely on.
    ///
    /// If `addr` is already present the index is left unchanged.
    pub(crate) fn sorted_insert(env: &Env, index: &mut Vec<Address>, addr: Address) {
        // Check for duplicates and find insertion position in one pass.
        let mut insert_pos: Option<u32> = None;
        for (i, existing) in index.iter().enumerate() {
            if existing == addr {
                // Already in index – nothing to do.
                return;
            }
            if insert_pos.is_none() && addr < existing {
                insert_pos = Some(i as u32);
            }
        }

        match insert_pos {
            Some(pos) => index.insert(pos, addr),
            None => index.push_back(addr),
        }
        let _ = env; // env available for future use
    }
}

mod events;

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_invariant;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_multi_asset;
