use soroban_sdk::{contracttype, Address, Symbol};

/// The maximum message length in bytes allowed for `broadcast` calls.
pub const MAX_MESSAGE_LEN: u32 = 256;

/// Maximum number of items allowed in a single `batch_receive_payment` call.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum number of developer balance records returned in a single
/// non-cursor-based query (gas guard).
pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;

/// Persistent storage keys for settlement contract.
///
/// # Migration note
/// Discriminant 5 was the original `DeveloperBalance(Address)` (single-token, now
/// `DeveloperBalanceV1` — kept for migration reads only).  New per-token entries use
/// `DeveloperBalance(Address, Address)` at discriminant 6.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Admin,
    Vault,
    PendingAdmin,
    PendingVault,
    DeveloperIndex,
    /// Legacy single-token balance — kept for V1 → V2 migration reads only.
    /// Do **not** use for new writes; new per-token credits go to
    /// [`StorageKey::DeveloperBalance`].
    DeveloperBalanceV1(Address),
    /// Per-token developer balance `(developer, token)`.
    DeveloperBalance(Address, Address),
    DeveloperMinBalance(Address),
    GlobalPool,
    Usdc,
    DailyWithdrawCap(Address),
    WithdrawalToday(Address),
    ContractVersion,
    /// Pending timelock'd developer balance migration record.
    /// Key: source developer address.
    PendingDeveloperMigration(Address),
    /// Storage-layout version marker (u32).
    /// Absent   → V1 (pre-migration, no version tracking).
    /// Value 2  → V2 (single-token → per-token migration complete).
    StorageVersion,
}

/// Severity levels for admin broadcast messages.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

/// Payload for the `admin_broadcast` event.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AdminBroadcast {
    pub severity: Severity,
    pub message: soroban_sdk::String,
}

/// Developer balance record in settlement contract.
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

/// Payment received event.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentReceivedEvent {
    pub from_vault: Address,
    pub amount: i128,
    pub to_pool: bool,
    pub developer: Option<Address>,
    pub token: Address,
}

/// Balance credited event.
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
    pub token: Address,
}

/// Emitted when the admin sets or changes a developer's daily withdrawal cap.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DailyWithdrawCapChanged {
    pub developer: Address,
    pub new_cap: i128,
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

/// Emitted when the admin proposes or executes a timelock'd developer balance
/// migration (address rotation).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AdminMigrationEvent {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub executed_at: u64,
}
