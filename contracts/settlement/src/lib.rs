#![no_std]
#![allow(dead_code, unused_imports)]
pub mod archive;
mod errors;
mod timelock;
mod types;
mod events;
mod admin;
mod migrate;
pub mod limits;
pub mod pagination;

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, BytesN, Env, Symbol, Vec,
};

/// Maximum number of items allowed in a single `batch_receive_payment` call.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum number of developer balances returned per page in paginated queries.
pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
extern crate alloc;

mod admin;
mod errors;
mod limits;
mod pagination;
mod timelock;
mod types;

/// Persistent storage keys for settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Admin,
    Vault,
    PendingAdmin,
    PendingVault,
    DeveloperIndex,
    DeveloperBalance(Address, Address),
    DeveloperBalanceV1(Address),
    GlobalPool,
    Usdc,
    DailyWithdrawCap(Address),
    WithdrawalToday(Address),
    DeveloperClaimWindow(Address),
    PendingDeveloperMigration(Address),
    DeveloperMinBalance(Address),
    ContractVersion,
    StorageVersion,
    Checkpoint,
    CheckpointCounter,
}

/// Checkpoint snapshot for bounded storage growth.
///
/// Captures a consistent point-in-time view of the global pool and
/// developer index so that archival/pruning logic can be applied
/// without pausing the contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub checkpoint_id: u64,
    pub total_pool_balance: i128,
    pub developer_count: u32,
    pub ledger_timestamp: u64,
    pub timestamp: u64,
}

/// Developer balance record in settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperBalance {
    pub address: Address,
    pub token: Address,
    pub balance: i128,
}

/// Global pool balance tracking.
///
/// `last_updated` is set to `env.ledger().timestamp()` on every
/// `receive_payment` call that credits the pool (`to_pool = true`).
/// It is also set at `init` time. It is **not** updated when payments
/// are routed to individual developer balances.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalPool {
    pub total_balance: i128,
    /// Ledger timestamp of the last pool credit. Useful for analytics
    /// and staleness checks.
    pub last_updated: u64,
}

/// Tracks a developer's cumulative withdrawal amount for a given epoch day.
///
/// `day` is `timestamp / 86400` (UTC epoch day). When the current call's day
/// differs from the stored day the accumulator is silently reset.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DailyWithdrawState {
    pub day: u64,
    pub amount: i128,
}

/// Timestamp range during which a developer may claim accrued balance.
///
/// `start_ts` and `end_ts` are ledger timestamps in seconds. The window is
/// inclusive on both ends: a withdrawal is allowed when
/// `start_ts <= env.ledger().timestamp() <= end_ts`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperClaimWindow {
    pub start_ts: u64,
    pub end_ts: u64,
}

/// Payment received event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentReceivedEvent {
    pub from_vault: Address,
    pub amount: i128,
    pub to_pool: bool,
    pub developer: Option<Address>,
    pub token: Address,
}

/// Balance credited event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BalanceCreditedEvent {
    pub developer: Address,
    pub amount: i128,
    pub new_balance: i128,
    pub token: Address,
}

/// Emitted when a new vault address is proposed via `propose_vault()`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultProposedEvent {
    pub current_vault: Address,
    pub proposed_vault: Address,
}

/// Emitted when the proposed vault is accepted via `accept_vault()`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultAcceptedEvent {
    pub old_vault: Address,
    pub new_vault: Address,
    pub accepted_by: Address,
}

/// Emitted when a developer withdraws their balance.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperWithdrawEvent {
    pub developer: Address,
    pub amount: i128,
    pub remaining_balance: i128,
    pub to: Address,
}

/// Emitted when the admin sets or changes a developer's daily withdrawal cap.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DailyWithdrawCapChanged {
    pub developer: Address,
    pub new_cap: i128,
}

/// Emitted when the admin sets or clears a developer claim window.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperClaimWindowChanged {
    pub developer: Address,
    pub start_ts: u64,
    pub end_ts: u64,
    pub enabled: bool,
}

/// Emitted when an admin force-credits a developer balance (escape hatch).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperForceCreditedEvent {
    pub developer: Address,
    pub amount: i128,
    pub reason: Symbol,
    pub new_balance: i128,
}

/// Storage entry TTL information for diagnostics.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StorageEntryTtl {
    pub category: soroban_sdk::String,
    pub key_desc: soroban_sdk::String,
    pub storage_type: soroban_sdk::String,
    pub ttl: u32,
    pub threshold: u32,
    pub bump_amount: u32,
}

/// Pending developer balance migration record.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDeveloperMigration {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub proposed_at: u64,
    pub execute_after: u64,
}

#[contract]
pub struct CalloraSettlement;

#[contractimpl]
impl CalloraSettlement {
    pub fn init(env: Env, admin: Address, vault: Address) {
        if env.storage().instance().has(&DataKey::Vault) {
            panic!("Already initialized");
        }
        env.storage()
            .instance()
            .set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Vault, &vault);
        env.storage().instance().set(&DataKey::TotalSettled, &0i128);
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
        ledger_seq: u32,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());
        if amount <= 0 {
            env.panic_with_error(SettlementError::AmountNotPositive);
        }

        // Replay guard: reject duplicate / out-of-order settlement claims.
        if to_pool {
            replay_guard::check_pool(&env, ledger_seq)
                .unwrap_or_else(|e| env.panic_with_error(e));
        } else {
            let dev = developer.clone().unwrap_or_else(|| {
                env.panic_with_error(SettlementError::DeveloperRequired)
            });
            replay_guard::check_developer(&env, &dev, ledger_seq)
                .unwrap_or_else(|e| env.panic_with_error(e));
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
            Self::insert_if_absent(&env, &mut index, dev_address.clone());
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
        ledger_seq: u32,
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

        // Replay guard: validate ALL developer HWMs before any state change.
        for item in items.iter() {
            let (dev, _) = item;
            replay_guard::check_developer(&env, &dev, ledger_seq)
                .unwrap_or_else(|e| { env.panic_with_error(e) });
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
            env.storage().persistent().set(
                &StorageKey::DeveloperBalance(dev.clone(), token.clone()),
                &new_balance,
            );
            env.storage().persistent().extend_ttl(
                &StorageKey::DeveloperBalance(dev.clone(), token.clone()),
                50000,
                50000,
            );
            // Add to index in sorted order if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            Self::insert_if_absent(&env, &mut index, dev.clone());
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
            .get::<_, GlobalPool>(&StorageKey::GlobalPool)
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
        env.storage()
            .persistent()
            .get(&StorageKey::PendingDeveloperMigration(from))
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

    /// Withdraw developer balance as USDC to a designated recipient.
    ///
    /// Requires the developer to authorize the request, the amount to be
    /// positive, and the requested amount to be covered by the tracked
    /// developer balance and on-ledger USDC holdings.
    pub fn withdraw_developer_balance(
        env: Env,
        developer: Address,
        amount: i128,
        to: Option<Address>,
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

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), usdc_address.clone());
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&balance_key)
            .unwrap_or(0);
        if current_balance < amount {
            return Err(SettlementError::InsufficientDeveloperBalance);
        }

        if usdc.balance(&contract_address) < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        let new_balance = current_balance
            .checked_sub(amount)
            .ok_or(SettlementError::DeveloperBalanceUnderflow)?;
        env.storage().persistent().set(&balance_key, &new_balance);
        usdc.transfer(&contract_address, &recipient, &amount);

        env.events().publish(
            (events::event_developer_withdraw(&env), developer.clone()),
            DeveloperWithdrawEvent {
                developer: developer.clone(),
                amount,
                remaining_balance: new_balance,
                to: recipient,
                token: usdc_address,
            },
        );
        Ok(())
    }

    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    /// Batch-withdraw developer balances with a cursor for pagination.
    ///
    /// # Access Control
    /// Only the nominated pending admin can call this function.
    ///
    /// # Security
    /// This is the second step of the two-step admin transfer process.
    /// The nominated admin must explicitly accept, proving control of
    /// their private keys before gaining admin privileges.
    ///
    /// Returns `(next_cursor, is_complete)`. When `is_complete` is `true` the
    /// full list has been processed.
    pub fn batch_withdraw_devs(
        env: Env,
        developers: Vec<Address>,
        amounts: Vec<i128>,
        cursor: u32,
        limit: u32,
    ) -> Result<(u32, bool), SettlementError> {
        let count = developers.len();
        if count != amounts.len() {
            return Err(SettlementError::AmountNotPositive);
        }
    }

    fn sorted_insert(env: &Env, index: &mut soroban_sdk::Vec<Address>, address: Address) {
        if !index.contains(&address) {
            index.push_back(address);
        }
    }

    fn require_claim_window_open(env: &Env, developer: &Address) -> Result<(), SettlementError> {
        let window: Option<crate::types::DeveloperClaimWindow> = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperClaimWindow(developer.clone()));
        if let Some(w) = window {
            let now = env.ledger().timestamp();
            if now < w.start_ts || now > w.end_ts {
                return Err(SettlementError::ClaimWindowClosed);
            }
        }
        Ok(())
    }

    pub fn batch_settle(
        env: Env,
        settlements: soroban_sdk::Vec<batch::SettleInput>,
    ) -> soroban_sdk::Vec<batch::SettleOutcome> {
        batch::batch_settle(&env, settlements)
    }


    pub fn migrate_v1_to_v2(env: Env, caller: Address) {
        migrate::migrate_v1_to_v2(&env, &caller);
    }

    pub fn migrate_v1_to_v2_page(
        env: Env,
        caller: Address,
        offset: u32,
        limit: u32,
    ) -> (u32, bool) {
        migrate::migrate_v1_to_v2_page(&env, &caller, offset, limit)
    }

        for i in start..end {
            let developer = developers
                .get(i as u32)
                .ok_or(SettlementError::InsufficientDeveloperBalance)?;
            let amount = amounts
                .get(i as u32)
                .ok_or(SettlementError::AmountNotPositive)?;
            Self::withdraw_developer_balance(env.clone(), developer, amount, None)?;
        }

        let next_cursor = end as u32;
        let is_complete = next_cursor >= count;
        Ok((next_cursor, is_complete))
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone());
        let admin = Self::get_admin(env.clone());
        if caller != vault && caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
    }

    fn insert_if_absent(_env: &Env, index: &mut Vec<Address>, addr: Address) {
        for i in 0..index.len() {
            if index.get(i).unwrap() == addr {
                return;
            }
        }
        index.push_back(addr);
    }

    fn require_claim_window_open(env: &Env, developer: &Address) -> Result<(), SettlementError> {
        let window: Option<DeveloperClaimWindow> = env
            .storage()
            .instance()
            .get(&StorageKey::DeveloperClaimWindow(developer.clone()));
        if let Some(w) = window {
            let now = env.ledger().timestamp();
            if now < w.start_ts || now > w.end_ts {
                return Err(SettlementError::ClaimWindowClosed);
            }
        }
        Ok(())
    }
}

mod events;
pub mod migrate;

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
