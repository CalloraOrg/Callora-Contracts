#![no_std]

use soroban_sdk::{
    contract, contractimpl, token, Address, BytesN, Env, Symbol, Vec, String,
};

mod admin;
mod errors;
mod events;
mod limits;
pub mod migrate;
mod pagination;
mod timelock;
mod types;

pub use crate::errors::SettlementError;
pub use crate::types::{
    AdminBroadcast, AdminMigrationEvent, BalanceCreditedEvent, DailyWithdrawCapChanged,
    DailyWithdrawState, DeveloperBalance, DeveloperClaimWindow, DeveloperClaimWindowChanged,
    DeveloperForceCreditedEvent, DeveloperWithdrawEvent, GlobalPool, PaymentReceivedEvent,
    Severity, StorageEntryTtl, StorageKey, VaultAcceptedEvent, VaultProposedEvent, MAX_BATCH_SIZE,
    MAX_DEVELOPER_BALANCES_PAGE_SIZE, MAX_MESSAGE_LEN,
};
pub use crate::timelock::PendingDeveloperMigration;

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
            let current: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
            let new_balance = current
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));
            env.storage().persistent().set(&balance_key, &new_balance);

            // Extend TTL for the developer's balance entry
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


    /// Withdraw developer balance as USDC to a designated recipient.
    ///
    /// Requires the developer to authorize the request, the amount to be
    /// positive, the developer's optional claim window to be open, and the
    /// requested amount to be covered by the tracked developer balance.
    ///
    /// # Arguments
    /// * `developer` - Address of the developer withdrawing their balance.
    /// * `amount` - Amount to withdraw in USDC micro-units.
    /// * `to` - Optional recipient address; if `None`, defaults to `developer`.
    /// * `token` - The token contract address for this withdrawal.
    ///
    /// # Errors
    /// - `AmountNotPositive` if amount is <= 0.
    /// - `ClaimWindowClosed` if a developer claim window exists and the current
    ///   ledger timestamp is outside that inclusive window.
    /// - `InsufficientDeveloperBalance` if developer balance < amount.
    /// - `DailyWithdrawCapExceeded` if daily cap is exceeded.
    /// - `DeveloperBalanceUnderflow` if subtraction underflows.
    /// - `UsdcTokenNotConfigured` if USDC token not set.
    /// - `InsufficientContractBalance` if contract has insufficient USDC.
    /// - Panics if `to` is the contract's own address.
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

        Self::require_claim_window_open(&env, &developer)?;

        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(developer.clone(), token.clone()))
            .unwrap_or(0);
        limits::require_developer_min_balance(&env, &developer, current_balance)?;
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
                .unwrap_or(DailyWithdrawState {
                    day: today,
                    amount: 0,
                });
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

        let usdc_client = token::Client::new(&env, &token);

        if usdc_client.balance(&contract_address) < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        usdc_client.transfer(&contract_address, &recipient, &amount);

        env.storage().persistent().set(
            &StorageKey::DeveloperBalance(developer.clone(), token.clone()),
            &new_balance,
        );
        env.storage().persistent().extend_ttl(
            &StorageKey::DeveloperBalance(developer.clone(), token.clone()),
            50000,
            50000,
        );

        let today = env.ledger().timestamp() / 86400;
        let mut daily = env
            .storage()
            .persistent()
            .get::<_, DailyWithdrawState>(&StorageKey::WithdrawalToday(developer.clone()))
            .unwrap_or(DailyWithdrawState {
                day: today,
                amount: 0,
            });
        if daily.day != today {
            daily.day = today;
            daily.amount = 0;
        }
        daily.amount = daily.amount.saturating_add(amount);
        env.storage()
            .persistent()
            .set(&StorageKey::WithdrawalToday(developer.clone()), &daily);
        env.storage().persistent().extend_ttl(
            &StorageKey::WithdrawalToday(developer.clone()),
            50000,
            50000,
        );

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

    /// Configure the inclusive claim window for a developer.
    ///
    /// A configured window restricts `withdraw_developer_balance` so the
    /// developer can claim only when the current ledger timestamp is between
    /// `start_ts` and `end_ts`, inclusive. Developers with no configured
    /// window remain claimable at any time.
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Errors
    /// - `Unauthorized` if caller is not the current admin.
    /// - `InvalidClaimWindow` if `end_ts < start_ts`.
    ///
    /// # Events
    /// Emits `developer_claim_window_changed` with `enabled = true`.
    pub fn set_developer_claim_window(
        env: Env,
        caller: Address,
        developer: Address,
        start_ts: u64,
        end_ts: u64,
    ) -> Result<(), SettlementError> {
        caller.require_auth();
        Self::require_admin(env.clone(), caller)?;
        if end_ts < start_ts {
            return Err(SettlementError::InvalidClaimWindow);
        }

        let window = DeveloperClaimWindow { start_ts, end_ts };
        env.storage().persistent().set(
            &StorageKey::DeveloperClaimWindow(developer.clone()),
            &window,
        );
        env.storage().persistent().extend_ttl(
            &StorageKey::DeveloperClaimWindow(developer.clone()),
            50000,
            50000,
        );

        env.events().publish(
            (
                events::event_developer_claim_window_changed(&env),
                developer.clone(),
            ),
            crate::types::DeveloperClaimWindowChanged {
                developer,
                start_ts,
                end_ts,
                enabled: true,
            },
        );

        Ok(())
    }

    /// Clear a developer's claim window and restore unrestricted claiming.
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Errors
    /// - `Unauthorized` if caller is not the current admin.
    ///
    /// # Events
    /// Emits `developer_claim_window_changed` with `enabled = false`.
    pub fn clear_developer_claim_window(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        caller.require_auth();
        Self::require_admin(env.clone(), caller)?;

        env.storage()
            .persistent()
            .remove(&StorageKey::DeveloperClaimWindow(developer.clone()));

        env.events().publish(
            (
                events::event_developer_claim_window_changed(&env),
                developer.clone(),
            ),
            crate::types::DeveloperClaimWindowChanged {
                developer,
                start_ts: 0,
                end_ts: 0,
                enabled: false,
            },
        );

        Ok(())
    }

    /// Return the configured claim window for a developer, if one exists.
    pub fn get_developer_claim_window(
        env: Env,
        developer: Address,
    ) -> Option<DeveloperClaimWindow> {
        env.storage()
            .persistent()
            .get(&StorageKey::DeveloperClaimWindow(developer))
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
        env.storage().persistent().extend_ttl(
            &StorageKey::DailyWithdrawCap(developer.clone()),
            50000,
            50000,
        );

        env.events().publish(
            (events::event_daily_withdraw_cap_changed(&env), caller),
            DailyWithdrawCapChanged {
                developer,
                new_cap: cap,
            },
        );
    }

    /// Set the minimum accrued balance required before a developer may claim.
    ///
    /// Only the current admin may configure the per-developer minimum. A value
    /// of `0` clears the effective requirement and preserves the historical
    /// behavior for developers with no configured minimum.
    pub fn set_minimum_balance(env: Env, caller: Address, developer: Address, min_balance: i128) {
        limits::set_developer_min_balance(env, caller, developer, min_balance);
    }

    /// Return the minimum accrued balance required before `developer` may claim.
    ///
    /// Returns `0` when no per-developer minimum has been configured.
    pub fn get_minimum_balance(env: Env, developer: Address) -> i128 {
        limits::get_developer_min_balance(env, developer)
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

        env.storage().persistent().set(
            &balance_key,
            &new_balance,
        );
        env.storage().persistent().extend_ttl(
            &balance_key,
            50000,
            50000,
        );

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
            (
                Symbol::new(&env, "developer_force_credited"),
                developer.clone(),
            ),
            DeveloperForceCreditedEvent {
                developer,
                amount,
                reason,
                new_balance,
                token,
            },
        );
    }

    /// Get all developer balances for a specific token (admin only).
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
                .get(&StorageKey::DeveloperBalance(
                    address.clone(),
                    token.clone(),
                ))
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
                    .get(&StorageKey::DeveloperBalance(
                        address.clone(),
                        token.clone(),
                    ))
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

        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        pagination::get_page(&env, &index, cursor, limit, token)
    }

    /// Return the remaining TTL for each storage key category.
    pub fn get_storage_ttl(env: Env, developer_addresses: Vec<Address>) -> Vec<StorageEntryTtl> {
        #[cfg(any(test, feature = "testutils"))]
        use soroban_sdk::testutils::storage::{Instance, Persistent};

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
            // Check DeveloperMinBalance (Persistent)
            let min_bal_key = StorageKey::DeveloperMinBalance(dev.clone());
            if env.storage().persistent().has(&min_bal_key) {
                let ttl = {
                    #[cfg(any(test, feature = "testutils"))]
                    {
                        env.storage().persistent().get_ttl(&min_bal_key)
                    }
                    #[cfg(not(any(test, feature = "testutils")))]
                    {
                        50000
                    }
                };
                result.push_back(StorageEntryTtl {
                    category: String::from_str(&env, "DeveloperMinBalance"),
                    key_desc: String::from_str(&env, "DeveloperMinBalance"),
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
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    /// Nominate a new admin (admin only).
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

        env.events()
            .publish((events::event_admin_cancelled(&env), current, pending), ());
    }

    /// Propose a new vault address (admin only).
    pub fn set_vault(env: Env, caller: Address, new_vault: Address) {
        // Backwards-compatible alias: `set_vault` now behaves like `propose_vault`.
        Self::propose_vault(env, caller, new_vault);
    }

    /// Propose a new vault address (admin only).
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

    fn require_admin(env: Env, caller: Address) -> Result<(), SettlementError> {
        let admin = Self::get_admin(env);
        if caller != admin {
            return Err(SettlementError::Unauthorized);
        }
        Ok(())
    }

    fn require_claim_window_open(env: &Env, developer: &Address) -> Result<(), SettlementError> {
        let window: Option<DeveloperClaimWindow> = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperClaimWindow(developer.clone()));
        if let Some(window) = window {
            let now = env.ledger().timestamp();
            if now < window.start_ts || now > window.end_ts {
                return Err(SettlementError::ClaimWindowClosed);
            }
        }
        Ok(())
    }

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
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        // Persist the version marker for on-chain queries.
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &new_wasm_hash);

        // Emit an event for indexers / audit logs.
        env.events()
            .publish((Symbol::new(&env, "upgraded"), admin), new_wasm_hash);
    }

    /// Read the stored contract version (WASM hash) as last set by `upgrade`.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&StorageKey::ContractVersion)
    }

    /// Insert `addr` into `index` in sorted order (ascending by raw bytes).
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

    /// One-shot V1 -> V2 storage migration (admin only).
    pub fn migrate_v1_to_v2(env: Env, caller: Address) {
        migrate::migrate_v1_to_v2(&env, &caller);
    }

    /// Paginated V1 -> V2 storage migration (admin only).
    pub fn migrate_v1_to_v2_page(
        env: Env,
        caller: Address,
        offset: u32,
        batch_size: u32,
    ) -> (u32, bool) {
        migrate::migrate_v1_to_v2_page(&env, &caller, offset, batch_size)
    }

    /// Return the current storage-layout version.
    pub fn migration_storage_version(env: Env) -> u32 {
        migrate::storage_version(&env)
    }
}

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_invariant;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_multi_asset;

#[cfg(test)]
mod test_admin_migration;
