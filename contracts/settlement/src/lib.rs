#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Persistent storage keys for settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Admin,
    Vault,
    PendingAdmin,
    DeveloperIndex,
    DeveloperBalance(Address),
    GlobalPool,
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

/// Emitted when the registered vault address is changed via `set_vault()`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultChangedEvent {
    pub old_vault: Address,
    pub new_vault: Address,
}

/// Storage key for the registered vault address.
const VAULT_KEY: &str = "vault";
/// Storage key for the admin address.
const ADMIN_KEY: &str = "admin";
const PENDING_ADMIN_KEY: &str = "pending_admin";
const DEVELOPER_BALANCES_KEY: &str = "developer_balances";
/// Storage key for the global pool state.
const GLOBAL_POOL_KEY: &str = "global_pool";

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
    /// - `StorageKey::DeveloperIndex`
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
            panic!("settlement contract already initialized");
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
        inst.set(&Symbol::new(&env, ADMIN_KEY), &admin);
        inst.set(&Symbol::new(&env, VAULT_KEY), &vault_address);
        let empty_balances: Map<Address, i128> = Map::new(&env);
        inst.set(&Symbol::new(&env, DEVELOPER_BALANCES_KEY), &empty_balances);
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
            panic!("amount must be positive");
        }
        let inst = env.storage().instance();
        if to_pool {
            if developer.is_some() {
                panic!("developer address must be None when to_pool=true");
            }
            let mut global_pool = Self::get_global_pool(env.clone());
            global_pool.total_balance = global_pool
                .total_balance
                .checked_add(amount)
                .unwrap_or_else(|| panic!("pool balance overflow"));
            global_pool.last_updated = env.ledger().timestamp();
            inst.set(&StorageKey::GlobalPool, &global_pool);
            env.events().publish(
                (Symbol::new(&env, "payment_received"), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: true,
                    developer: None,
                },
            );
        } else {
            let dev_address = developer
                .unwrap_or_else(|| panic!("developer address required when to_pool=false"));



            // Read current balance from persistent storage
            let current_balance = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(dev_address.clone()))
                .unwrap_or(0);
            let new_balance = current_balance
                .checked_add(amount)
                .unwrap_or_else(|| panic!("developer balance overflow"));
            
            // Write to persistent storage with TTL extension
            env.storage()
                .persistent()
                .set(&StorageKey::DeveloperBalance(dev_address.clone()), &new_balance);
            
            // Extend TTL for the developer's balance entry (persistent storage live for 1 year)
            env.storage()
                .persistent()
                .extend_ttl(&StorageKey::DeveloperBalance(dev_address.clone()), 50000, 50000);
            
            // Add developer to index if not already present
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            if !index.iter().any(|addr| addr == &dev_address) {
                index.push_back(dev_address.clone());
                inst.set(&StorageKey::DeveloperIndex, &index);
            }


            env.events().publish(
                (Symbol::new(&env, "payment_received"), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: false,
                    developer: Some(dev_address.clone()),
                },
            );
            env.events().publish(
                (Symbol::new(&env, "balance_credited"), dev_address.clone()),
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
        let mut balances: Map<Address, i128> = inst
            .get(&Symbol::new(&env, DEVELOPER_BALANCES_KEY))
            .unwrap_or_else(|| Map::new(&env));

        for item in items.iter() {
            let (dev, amount) = item;
            let current = balances.get(dev.clone()).unwrap_or(0);
            let new_balance = current
                .checked_add(amount)
                .unwrap_or_else(|| panic!("developer balance overflow"));
            balances.set(dev.clone(), new_balance);
            env.events().publish(
                (Symbol::new(&env, "balance_credited"), dev.clone()),
                BalanceCreditedEvent {
                    developer: dev,
                    amount,
                    new_balance,
                },
            );
        }

        inst.set(&Symbol::new(&env, DEVELOPER_BALANCES_KEY), &balances);
    }

    /// Get current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
    }

    /// Get registered vault address
    pub fn get_vault(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Vault)
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
    }

    /// Get global pool information
    pub fn get_global_pool(env: Env) -> GlobalPool {
        env.storage()
            .instance()
            .get(&StorageKey::GlobalPool)
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
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
            panic!("settlement contract not initialized");
        }
        env.storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(developer))
            .unwrap_or(0)
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
    /// Vec of DeveloperBalance records. Iteration order is based on index insertion order.
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
    pub fn get_all_developer_balances(env: Env, caller: Address) -> Vec<DeveloperBalance> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));
        
        let mut result = Vec::new(&env);
        for address in index.iter() {
            let balance = env
                .storage()
                .persistent()
                .get(&StorageKey::DeveloperBalance(address))
                .unwrap_or(0);
            result.push_back(DeveloperBalance {
                address: address.clone(),
                balance,
            });
        }
        result
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
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events().publish(
            (
                Symbol::new(&env, "admin_nominated"),
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
            .publish((Symbol::new(&env, "admin_accepted"), current, pending), ());
    }

    /// Update vault address (admin only).
    ///
    /// # Arguments
    /// * `caller` - Current admin address; must match stored admin
    /// * `new_vault` - New vault contract address to register
    ///
    /// # Access Control
    /// Only the current admin can call this function.
    ///
    /// # Security
    /// The vault address controls which contract can send payments to
    /// the settlement contract. Only trusted addresses should be set.
    /// Changing the vault address immediately revokes access from the
    /// old vault, so coordinate carefully during migrations.
    ///
    /// # Events
    /// Emits `vault_changed` event with the old and new vault addresses.
    ///
    /// # Panics
    /// Panics if caller is not the current admin.
    pub fn set_vault(env: Env, caller: Address, new_vault: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        let old_vault = Self::get_vault(env.clone());
        inst.set(&Symbol::new(&env, VAULT_KEY), &new_vault);

        env.events().publish(
            (Symbol::new(&env, "vault_changed"), caller.clone()),
            VaultChangedEvent {
                old_vault: old_vault.clone(),
                new_vault: new_vault.clone(),
            },
        );
    }

    /// Internal function to require authorized caller (vault or admin)
    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone());
        let admin = Self::get_admin(env.clone());
        if caller != vault && caller != admin {
            panic!("unauthorized: caller must be vault or admin");
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;
