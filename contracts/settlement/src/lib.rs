#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, token, Address, Env, Symbol, Vec};

/// Maximum number of items allowed in a single `batch_receive_payment` call.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum number of developer balances returned per page in paginated queries.
pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;

/// Typed errors for the settlement contract.
///
/// Using `#[contracterror]` encodes each variant as a stable `u32` code.
/// Callers and indexers can match on the code rather than parsing raw panic strings,
/// and the WASM binary shrinks because no error string literals are embedded.
///
/// | Code | Variant                      | When                                              |
/// |------|------------------------------|---------------------------------------------------|
/// | 1    | NotInitialized               | A function is called before `init`                |
/// | 2    | AlreadyInitialized           | `init` is called more than once                   |
/// | 3    | Unauthorized                 | Caller is not the vault or admin                  |
/// | 4    | AmountNotPositive            | `amount` is zero or negative                      |
/// | 5    | DeveloperRequired            | `to_pool=false` but no developer address supplied |
/// | 6    | DeveloperMustBeNone          | `to_pool=true` but a developer address was given  |
/// | 7    | PoolOverflow                 | Global pool `i128` addition would overflow        |
/// | 8    | DeveloperOverflow            | Developer balance `i128` addition would overflow  |
/// | 9    | UsdcTokenNotConfigured       | USDC token address not configured for withdrawals |
/// | 10   | InsufficientDeveloperBalance | Developer balance is less than withdrawal amount  |
/// | 11   | DeveloperBalanceUnderflow    | Developer balance subtraction would overflow      |
/// | 12   | InsufficientContractBalance  | Settlement contract lacks on-ledger USDC          |
/// | 13   | DailyWithdrawCapExceeded     | Developer's daily withdrawal cap would be exceeded|
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum SettlementError {
    NotInitialized               = 1,
    AlreadyInitialized           = 2,
    Unauthorized                 = 3,
    AmountNotPositive            = 4,
    DeveloperRequired            = 5,
    DeveloperMustBeNone          = 6,
    PoolOverflow                 = 7,
    DeveloperOverflow            = 8,
    UsdcTokenNotConfigured       = 9,
    InsufficientDeveloperBalance = 10,
    DeveloperBalanceUnderflow    = 11,
    InsufficientContractBalance  = 12,
    DailyWithdrawCapExceeded     = 13,
}

/// Persistent storage keys for settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Admin,
    Vault,
    PendingAdmin,
    PendingVault,
    DeveloperIndex,
    DeveloperBalance(Address),
    GlobalPool,
    Usdc,
    DailyWithdrawCap(Address),
    WithdrawalToday(Address),
}

/// Developer balance record in settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperBalance {
    pub address: Address,
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

/// Payment received event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentReceivedEvent {
    pub from_vault: Address,
    pub amount: i128,
    pub to_pool: bool, // true if credited to global pool, false if to specific developer
    pub developer: Option<Address>, // developer address if credited to specific developer
}

/// Balance credited event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BalanceCreditedEvent {
    pub developer: Address,
    pub amount: i128,
    pub new_balance: i128,
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
}

/// Emitted when the admin sets or changes a developer's daily withdrawal cap.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DailyWithdrawCapChanged {
    pub developer: Address,
    pub new_cap: i128,
}


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
    /// * `amount` - Payment amount in USDC micro-units; must be > 0
    /// * `to_pool` - If true, credit global pool; if false, credit a specific developer
    /// * `developer` - Required when `to_pool=false`; ignored when `to_pool=true`
    ///
    /// # Access Control
    /// Only the registered vault address or admin can call this function.
    ///
    /// # Persistent Storage Operations
    /// When crediting to developer balance:
    /// - Performs O(1) point-read from persistent storage for the developer
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
                },
            );
        } else {
            let dev_address = developer
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperRequired));

            // Read current balance from persistent storage
            let current_balance: i128 = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(dev_address.clone()))
                .unwrap_or(0i128);
            let new_balance = current_balance
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));
            
            // Write to persistent storage with TTL extension
            env.storage().persistent().set(
                &StorageKey::DeveloperBalance(dev_address.clone()),
                &new_balance,
            );

            // Extend TTL for the developer's balance entry (persistent storage live for 1 year)
            env.storage().persistent().extend_ttl(
                &StorageKey::DeveloperBalance(dev_address.clone()),
                50000,
                50000,
            );

            // Add developer to index if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            if !index.iter().any(|addr| addr == dev_address) {
                index.push_back(dev_address.clone());
                inst.set(&StorageKey::DeveloperIndex, &index);
            }

            env.events().publish(
                (events::event_payment_received(&env), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: false,
                    developer: Some(dev_address.clone()),
                },
            );
            env.events().publish(
                (events::event_balance_credited(&env), dev_address.clone()),
                BalanceCreditedEvent {
                    developer: dev_address,
                    amount,
                    new_balance,
                },
            );
        }
    }

    /// Atomically credit multiple developer balances in a single call.
    ///
    /// # Arguments
    /// * `caller` - Must be the registered vault address or admin
    /// * `items` - Vec of `(developer_address, amount)` pairs; 1–[`MAX_BATCH_SIZE`] entries
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
    pub fn batch_receive_payment(env: Env, caller: Address, items: Vec<(Address, i128)>) {
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
            let current: i128 = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(dev.clone()))
                .unwrap_or(0);
            let new_balance = current
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));
            env.storage()
                .persistent()
                .set(&StorageKey::DeveloperBalance(dev.clone()), &new_balance);
            env.storage()
                .persistent()
                .extend_ttl(&StorageKey::DeveloperBalance(dev.clone()), 50000, 50000);
            // Add to index if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            if !index.iter().any(|a| a == dev) {
                index.push_back(dev.clone());
                inst.set(&StorageKey::DeveloperIndex, &index);
            }
            env.events().publish(
                (events::event_balance_credited(&env), dev.clone()),
                BalanceCreditedEvent {
                    developer: dev.clone(),
                    amount: amount,
                    new_balance,
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

    /// Get developer balance
    ///
    /// Performs a direct O(1) persistent storage lookup for the specified developer's balance.
    /// This is the preferred method for querying individual balances as it uses point storage.
    ///
    /// # Arguments
    /// * `developer` - Developer address to query
    ///
    /// # Returns
    /// Balance in USDC micro-units, or 0 if no balance recorded
    ///
    /// # Safety
    /// Safe for all use cases; uses persistent storage with TTL.
    pub fn get_developer_balance(env: Env, developer: Address) -> i128 {
        if !env.storage().instance().has(&StorageKey::Admin) {
            env.panic_with_error(SettlementError::NotInitialized);
        }
        env.storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(developer))
            .unwrap_or(0)
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

    /// Withdraw developer balance as USDC to the requesting developer.
    ///
    /// Requires the developer to authorize the request and the requested amount
    /// to be positive and covered by the tracked developer balance.
    pub fn withdraw_developer_balance(
        env: Env,
        developer: Address,
        amount: i128,
    ) -> Result<(), SettlementError> {
        developer.require_auth();
        if amount <= 0 {
            return Err(SettlementError::AmountNotPositive);
        }

        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(developer.clone()))
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

        let usdc_address = Self::get_usdc_token(env.clone())?;
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();

        if usdc.balance(&contract_address) < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        usdc.transfer(&contract_address, &developer, &amount);

        env.storage()
            .persistent()
            .set(&StorageKey::DeveloperBalance(developer.clone()), &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&StorageKey::DeveloperBalance(developer.clone()), 50000, 50000);

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

    /// Get all developer balances (admin only)
    ///
    /// **CRITICAL**: Uses developer index for iteration; order is based on index insertion order.
    /// Use this function only for administrative queries or reporting purposes.
    /// For production integrations with many developers (>100), implement off-chain indexing
    /// by listening to `BalanceCreditedEvent` and maintaining a local database.
    ///
    /// # Arguments
    /// * `caller` - Must be the current admin address.
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

        let mut result = Vec::new(&env);
        for address in index.iter() {
            let address_key = address.clone();
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(address_key))
                .unwrap_or(0i128);
            result.push_back(DeveloperBalance {
                address: address.clone(),
                balance,
            });
        }
        Ok(result)
    }

    /// Get a paginated slice of developer balances (admin only).
    ///
    /// This method avoids expensive full-index iteration by returning
    /// a bounded window of developer balance records. Use it for
    /// admin dashboards and off-chain pagination.
    pub fn get_developer_balances_page(
        env: Env,
        caller: Address,
        start: u32,
        limit: u32,
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
                    .get(&StorageKey::DeveloperBalance(address.clone()))
                    .unwrap_or(0);
                result.push_back(DeveloperBalance {
                    address: address.clone(),
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

    /// Return the pending admin address, or `None` if no transfer is in progress.
    ///
    /// Integrators can poll this to detect an in-flight two-step admin handover
    /// before `accept_admin` is called.
    ///
    /// # Returns
    /// `Some(Address)` of the nominated admin, or `None` when no transfer is pending.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
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
}

mod events;

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;
