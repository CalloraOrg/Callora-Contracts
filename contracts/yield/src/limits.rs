//! Per-account state caps for the Callora Yield surface.
//!
//! # Problem
//!
//! Yield-bearing products (bets, positions, subscriptions) can be abused by a
//! single account farming thousands of small positions to drain fees or grief
//! indexers. The Callora yield surface therefore enforces per-account caps on
//! open-bets, open-positions, and active-subscriptions.
//!
//! # Architecture
//!
//! Two [`contracttype`] structs drive the limits surface:
//!
//! | Type           | Purpose                                                       |
//! |----------------|---------------------------------------------------------------|
//! | [`AccountLimits`] | The configured `(max_bets, max_positions, max_subscriptions)` caps for an account. |
//! | [`AccountState`]  | The current `(bets, positions, subscriptions)` counters for an account.          |
//!
//! Both types are `Clone + Debug + PartialEq` so off-chain consumers can
//! round-trip them through the SDK.
//!
//! # Storage Layout
//!
//! - [`AccountLimits`] overrides are stored in **persistent** storage under
//!   [`StorageKey::AccountLimits`]`(Address)` and their TTL is extended on
//!   every read and write ([`CAPS_BUMP_THRESHOLD`] /
//!   [`CAPS_BUMP_AMOUNT`]). They are sparse overrides; an account with no
//!   explicit override falls back to the global [`DEFAULT_LIMITS`] constant.
//!
//!   Limits used to live in **instance** storage, which is loaded on every
//!   call and bounded in size. With one entry per capped account they grew the
//!   instance entry without bound and evicted the caps of existing accounts as
//!   new ones were configured (#1414). Reads still fall back to the legacy
//!   instance slot and migrate it to persistent storage on first touch, so an
//!   upgrade does not drop existing overrides.
//!
//! - [`AccountState`] counters are stored in **persistent** storage under
//!   [`StorageKey::AccountState`]`(Address)`. Persistent storage lets the
//!   contract scale to many accounts and keeps counters alive across the
//!   typical ledger archival window via TTL extensions on every
//!   increment/decrement.
//!
//! - Registered operators are stored as a persistent flag under
//!   [`StorageKey::Operator`]`(Address)` with the same TTL policy.
//!
//! # Rolling windows (Issue #1522)
//!
//! The concurrent caps above never reset, so an account that opens and
//! closes bets repeatedly can still churn unbounded activity. An optional
//! rolling window adds a second, time-scoped layer:
//!
//! - [`WindowLimits`] caps how many bets / positions / subscriptions may be
//!   **opened** inside one window.
//! - `window_ledgers` (global, admin-set) is the window length in ledgers;
//!   `0` disables windows and restores the pre-#1522 behaviour exactly.
//! - [`WindowState`] stores `{ window_start, bets_opened, positions_opened,
//!   subscriptions_added }` under [`StorageKey::WindowState`]`(Address)` in
//!   persistent storage. The window rolls lazily on the next mutation when
//!   `ledger.sequence() >= window_start + window_ledgers`, so no keeper is
//!   required.
//! - Decrements (`clear_bet`, `close_position`, `unsubscribe`) never refund
//!   window counts; the window caps only bound activity *inside* a window.
//!
//! # Auth Model
//!
//! Counter mutations are **operator-gated**, not caller-gated (#1414). An
//! account can no longer mint or clear its own counters, so a capped account
//! cannot reset its own cap by calling `clear_bet` and then `place_bet` again.
//! The operator must sign the call and be registered with
//! [`CalloraYieldLimits::set_operator`]; the *cap* is always enforced on the
//! account named in the call.
//!
//! | Entrypoint                                       | Authorized by                              |
//! |--------------------------------------------------|-------------------------------------------|
//! | `init`                                           | `admin.require_auth()`                    |
//! | `set_admin`, `accept_admin`, `upgrade`           | admin (`caller == admin`)                |
//! | `cancel_admin_transfer`                          | admin (`caller == admin`)                |
//! | `set_default_limits`, `set_account_limits`, `clear_account_limits` | admin (`caller == admin`) |
//! | `reset_account_state`, `batch_clear_accounts`     | admin (`caller == admin`)                |
//! | `set_operator`, `remove_operator`                | admin (`caller == admin`)                |
//! | `place_bet`, `clear_bet`, `open_position`,       | registered operator; cap enforced on the |
//! | `close_position`, `subscribe`, `unsubscribe`     | account named in the call                |
//!
//! Error precedence for counter mutations is: operator `require_auth`,
//! then operator registration, then the per-account cap.
//!
//! Read-only views (`get_admin`, `get_default_limits`, `get_account_limits`,
//! `get_account_state`, `get_operator`, `can_*`) do **not** call
//! `require_auth`.
//!
//! # Overflow Safety
//!
//! Every count mutation uses `u32::checked_add` / `u32::checked_sub`. The
//! outcomes are folded into typed [`YieldLimitError`] variants
//! ([`YieldLimitError::Overflow`] / [`YieldLimitError::CounterUnderflow`]) so
//! production code paths never invoke `unwrap()`.

use callora_storage_migration::StorageMigrationValidator;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

use crate::errors::YieldLimitError;
use crate::events;

// ---------------------------------------------------------------------
// Constants — storage-layout versioning
// ---------------------------------------------------------------------

/// Storage-layout version recorded by [`StorageMigrationValidator`] for this
/// contract. Bumped whenever the on-ledger schema changes so the pre-upgrade
/// validation gate can enforce ordered, single-step migrations.
const STORAGE_MIGRATION_VERSION: u32 = 2;

/// Human-readable descriptor of the on-ledger storage layout.
///
/// The descriptor is hashed by [`storage_layout_hash`] and recorded at `init`
/// time and on every upgrade. Any change to the shape of [`AccountLimits`],
/// [`AccountState`], or the [`StorageKey`] enum MUST be reflected here so the
/// [`StorageMigrationValidator`] can detect an accidental silent layout change
/// (`StorageMigrationError::SilentLayoutChange`).
const STORAGE_LAYOUT_DESCRIPTOR: &str = concat!(
    "callora-yield-limits:v2:",
    "StorageKey[Admin,PendingAdmin,DefaultLimits,AccountLimits,AccountState,WasmVersion,Operator];",
    "AccountLimits[max_bets:u32,max_positions:u32,max_subscriptions:u32];",
    "AccountState[bets:u32,positions:u32,subscriptions:u32]"
);

/// Deterministic SHA-256 hash of the current on-ledger storage layout.
///
/// This is the "real layout hash" passed to [`StorageMigrationValidator`] on
/// both the pre-upgrade validation and the post-validation finalize call, so a
/// same-version re-deploy cannot silently change the layout.
pub fn storage_layout_hash(env: &Env) -> BytesN<32> {
    callora_storage_migration::layout_hash(env, STORAGE_LAYOUT_DESCRIPTOR)
}

// ---------------------------------------------------------------------
// Constants — TTL and defaults
// ---------------------------------------------------------------------

/// Per-day ledger count at a 5-second close cadence (matches vault).
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL bump threshold for persistent storage keys (`AccountState`).
///
/// When the remaining TTL of the key falls below this value the contract
/// re-extends the TTL on every increment / decrement so account counters do
/// not silently archive.
pub const STATE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;

/// TTL bump amount for persistent storage keys (`AccountState`).
pub const STATE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;

/// TTL bump threshold for instance storage keys (`AccountLimits`,
/// `DefaultLimits`, config).
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// TTL bump amount for instance storage keys.
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// TTL bump threshold for persistent cap overrides (`AccountLimits`) and the
/// operator registry (`Operator`).
///
/// Caps used to live in instance storage, whose lifetime was tied to the
/// contract config entry. They are now persistent and must be re-extended on
/// read/write once their remaining TTL falls below this threshold.
pub const CAPS_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;

/// TTL bump amount for persistent cap overrides and operator flags (~30 days).
pub const CAPS_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;

/// Global default per-account limits applied when no explicit override exists
/// for an account.
///
/// # Default rationale
///
/// - `100` open bets caps the worst-case bet-farming at 100× per account.
/// - `50` open positions caps positions similarly while leaving headroom for
///   legitimate power-users.
/// - `20` active subscriptions caps subscription-farming while still
///   allowing routine yield-vault subscriptions.
///
/// These values are intentionally conservative so the contract is safe-by-default
/// even before the admin sets global defaults via
/// [`CalloraYieldLimits::set_default_limits`].
pub const DEFAULT_LIMITS: AccountLimits = AccountLimits {
    max_bets: 100,
    max_positions: 50,
    max_subscriptions: 20,
};

/// Maximum allowable value for any single cap dimension.
///
/// Counts are stored as `u32`, so this matches the practical ceiling. The
/// cap itself is never compared against `u32::MAX` so this is a conservative
/// sanity ceiling rather than the absolute type ceiling.
pub const MAX_CAP: u32 = 1_000_000;

/// Maximum number of accounts accepted by
/// [`CalloraYieldLimits::batch_clear_accounts`].
///
/// The cap bounds the CPU and ledger-entry budget of a single transaction.
/// Duplicate addresses are permitted; each occurrence is processed in order
/// and emits its own event, so a repeated address simply resets an already
/// zeroed counter.
pub const MAX_BATCH_CLEAR: u32 = 50;

/// Smallest non-zero rolling-window length, in ledgers.
pub const MIN_WINDOW_LEDGERS: u32 = 1;

/// Largest allowable rolling-window length, in ledgers.
///
/// `1_000_000` ledgers is roughly 58 days at a 5-second close cadence — a
/// conservative sanity ceiling that keeps a mistaken admin from configuring
/// windows that effectively never roll.
pub const MAX_WINDOW_LEDGERS: u32 = 1_000_000;

/// Per-window caps applied when the admin enables windows but does not set
/// explicit [`WindowLimits`].
///
/// The default is deliberately permissive (every dimension at [`MAX_CAP`]) so
/// merely enabling a window does not change behaviour until the admin also
/// configures window caps.
pub const DEFAULT_WINDOW_LIMITS: WindowLimits = WindowLimits {
    max_bets: MAX_CAP,
    max_positions: MAX_CAP,
    max_subscriptions: MAX_CAP,
};

// ---------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------

/// Instance / persistent storage keys for the yield per-account limits
/// contract.
///
/// Using a single [`contracttype`] enum keeps the key space tidy and protects
/// against accidental key collisions across this module and future modules.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Admin address (set by [`CalloraYieldLimits::init`]).
    Admin,
    /// Pending admin awaiting acceptance (two-step transfer).
    PendingAdmin,
    /// Global default caps applied when an account has no explicit override.
    DefaultLimits,
    /// Per-account cap override (instance storage; sparse).
    AccountLimits(Address),
    /// Per-account live counters (persistent storage).
    AccountState(Address),
    /// Hash of the most recently installed WASM (`BytesN<32>`, instance
    /// storage).
    ///
    /// Deliberately named `WasmVersion` (rather than `Version`) to avoid
    /// colliding with the validator's `SmVersion` marker written by
    /// [`StorageMigrationValidator`] in the same instance-storage namespace.
    WasmVersion,
    /// Registered operator flag (`bool`, persistent storage).
    ///
    /// Only registered operators may mutate per-account counters, and only the
    /// admin may register or remove them. Appended last so the discriminants of
    /// every pre-existing key stay stable across the v1 → v2 layout change.
    Operator(Address),
}

// ---------------------------------------------------------------------
// Aux structs
// ---------------------------------------------------------------------

/// Per-account state caps configured by the admin.
///
/// Each field sets the maximum allowed concurrent state of that kind for a
/// single account. A value of `0` disables that kind entirely (no new bets,
/// positions, or subscriptions can be opened while the cap is `0`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountLimits {
    /// Maximum concurrent open bets.
    pub max_bets: u32,
    /// Maximum concurrent open positions.
    pub max_positions: u32,
    /// Maximum concurrent active subscriptions.
    pub max_subscriptions: u32,
}

impl AccountLimits {
    /// Construct a new [`AccountLimits`] with all three caps set to `count`.
    pub const fn uniform(count: u32) -> Self {
        Self {
            max_bets: count,
            max_positions: count,
            max_subscriptions: count,
        }
    }

    /// Return `true` if any individual cap exceeds [`MAX_CAP`] or any cap is
    /// non-sensical (no upper bound on `u32` makes a `-1`-style value
    /// impossible, but the function reserves headroom for future validation
    /// rules).
    pub fn is_valid(&self) -> bool {
        self.max_bets <= MAX_CAP
            && self.max_positions <= MAX_CAP
            && self.max_subscriptions <= MAX_CAP
    }
}

/// Per-account per-window caps configured by the admin.
///
/// Each field caps how many of that kind may be **opened** during a single
/// rolling window. Decrements never reduce these counters, so closing a bet
/// does not free up window capacity. A value of `0` blocks all opens of that
/// kind for the duration of the window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowLimits {
    /// Maximum bets opened per window.
    pub max_bets: u32,
    /// Maximum positions opened per window.
    pub max_positions: u32,
    /// Maximum subscriptions added per window.
    pub max_subscriptions: u32,
}

impl WindowLimits {
    /// Construct a uniform [`WindowLimits`] with all three caps at `count`.
    pub const fn uniform(count: u32) -> Self {
        Self {
            max_bets: count,
            max_positions: count,
            max_subscriptions: count,
        }
    }

    /// Return `true` when every cap is within [`MAX_CAP`].
    pub fn is_valid(&self) -> bool {
        self.max_bets <= MAX_CAP
            && self.max_positions <= MAX_CAP
            && self.max_subscriptions <= MAX_CAP
    }
}

/// Remaining state capacity for an account across all concurrent limit dimensions.
///
/// Each field indicates how many more operations of that kind an account can
/// open before reaching its effective limit. Saturating subtraction guarantees
/// that values never underflow, even if caps are reduced below active counts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemainingCapacity {
    /// Remaining open-bet capacity before reaching `max_bets`.
    pub bets: u32,
    /// Remaining open-position capacity before reaching `max_positions`.
    pub positions: u32,
    /// Remaining active-subscription capacity before reaching `max_subscriptions`.
    pub subscriptions: u32,
}

/// Per-account rolling-window counters.
///
/// `window_start` is the ledger sequence at which the current window began.
/// All three counters are monotonically increasing within a window and reset
/// together when the window rolls over.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct WindowState {
    /// Ledger sequence at which the current window started (`0` = absent).
    pub window_start: u32,
    /// Bets opened in the current window.
    pub bets_opened: u32,
    /// Positions opened in the current window.
    pub positions_opened: u32,
    /// Subscriptions added in the current window.
    pub subscriptions_added: u32,
}

impl WindowState {
    /// Return a zeroed [`WindowState`] (no window recorded).
    pub const fn zero() -> Self {
        Self {
            window_start: 0,
            bets_opened: 0,
            positions_opened: 0,
            subscriptions_added: 0,
        }
    }

    /// Return a zeroed [`WindowState`] that starts at `window_start`.
    pub const fn started_at(window_start: u32) -> Self {
        Self {
            window_start,
            bets_opened: 0,
            positions_opened: 0,
            subscriptions_added: 0,
        }
    }

    /// Reset the counters and anchor the window at `window_start`.
    pub fn roll(&mut self, window_start: u32) {
        self.window_start = window_start;
        self.bets_opened = 0;
        self.positions_opened = 0;
        self.subscriptions_added = 0;
    }

    /// Return `true` when `current` has reached or passed the end of this
    /// window. Uses saturating arithmetic so a sequence near `u32::MAX`
    /// cannot panic or wrap into a spurious roll.
    pub fn is_expired(&self, current: u32, window_ledgers: u32) -> bool {
        window_ledgers == 0 || current >= self.window_start.saturating_add(window_ledgers)
    }

    /// Record one opened bet using `checked_add`.
    pub fn add_bet(&mut self) -> Result<(), YieldLimitError> {
        self.bets_opened = self
            .bets_opened
            .checked_add(1)
            .ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }

    /// Record one opened position using `checked_add`.
    pub fn add_position(&mut self) -> Result<(), YieldLimitError> {
        self.positions_opened = self
            .positions_opened
            .checked_add(1)
            .ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }

    /// Record one added subscription using `checked_add`.
    pub fn add_subscription(&mut self) -> Result<(), YieldLimitError> {
        self.subscriptions_added = self
            .subscriptions_added
            .checked_add(1)
            .ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }
}

/// Return `true` when `window_ledgers` is a legal configuration value.
pub fn is_valid_window_ledgers(window_ledgers: u32) -> bool {
    window_ledgers == 0 || (MIN_WINDOW_LEDGERS..=MAX_WINDOW_LEDGERS).contains(&window_ledgers)
}

/// Per-account live state counters.
///
/// Only the contract mutates fields on this struct. Counter arithmetic is
/// performed via `checked_add` / `checked_sub` so a buggy caller can never
/// drive any field into `u32::MAX + 1`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct AccountState {
    /// Current number of open bets.
    pub bets: u32,
    /// Current number of open positions.
    pub positions: u32,
    /// Current number of active subscriptions.
    pub subscriptions: u32,
}

impl AccountState {
    /// Return a zeroed [`AccountState`].
    pub const fn zero() -> Self {
        Self {
            bets: 0,
            positions: 0,
            subscriptions: 0,
        }
    }

    /// Increment the bet counter using `checked_add`.
    ///
    /// # Errors
    /// - [`YieldLimitError::Overflow`] — counter would saturate `u32::MAX`.
    pub fn add_bet(&mut self) -> Result<(), YieldLimitError> {
        self.bets = self.bets.checked_add(1).ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }

    /// Decrement the bet counter using `checked_sub`.
    ///
    /// # Errors
    /// - [`YieldLimitError::CounterUnderflow`] — counter is already 0.
    pub fn sub_bet(&mut self) -> Result<(), YieldLimitError> {
        self.bets = self
            .bets
            .checked_sub(1)
            .ok_or(YieldLimitError::CounterUnderflow)?;
        Ok(())
    }

    /// Increment the position counter using `checked_add`.
    pub fn add_position(&mut self) -> Result<(), YieldLimitError> {
        self.positions = self
            .positions
            .checked_add(1)
            .ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }

    /// Decrement the position counter using `checked_sub`.
    pub fn sub_position(&mut self) -> Result<(), YieldLimitError> {
        self.positions = self
            .positions
            .checked_sub(1)
            .ok_or(YieldLimitError::CounterUnderflow)?;
        Ok(())
    }

    /// Increment the subscription counter using `checked_add`.
    pub fn add_subscription(&mut self) -> Result<(), YieldLimitError> {
        self.subscriptions = self
            .subscriptions
            .checked_add(1)
            .ok_or(YieldLimitError::Overflow)?;
        Ok(())
    }

    /// Decrement the subscription counter using `checked_sub`.
    pub fn sub_subscription(&mut self) -> Result<(), YieldLimitError> {
        self.subscriptions = self
            .subscriptions
            .checked_sub(1)
            .ok_or(YieldLimitError::CounterUnderflow)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Free functions — storage helpers
// ---------------------------------------------------------------------

/// Read the admin address from instance storage.
///
/// # Errors
/// - [`YieldLimitError::NotInitialized`] — `Admin` key is absent.
pub fn read_admin(env: &Env) -> Result<Address, YieldLimitError> {
    env.storage()
        .instance()
        .get::<_, Address>(&StorageKey::Admin)
        .ok_or(YieldLimitError::NotInitialized)
}

/// Assert `caller` equals the stored admin.
///
/// Runs `caller.require_auth()` first so misconfigured callers are rejected
/// deterministically without consuming the underlying signature.
///
/// # Errors
/// - [`YieldLimitError::Unauthorized`] — caller is not the stored admin.
/// - [`YieldLimitError::NotInitialized`] — admin has never been set.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), YieldLimitError> {
    let admin = read_admin(env)?;
    caller.require_auth();
    if *caller != admin {
        return Err(YieldLimitError::Unauthorized);
    }
    Ok(())
}

/// Read the global default caps (or fall back to [`DEFAULT_LIMITS`]) and
/// extend instance TTL.
pub fn read_default_limits(env: &Env) -> AccountLimits {
    let caps: AccountLimits = env
        .storage()
        .instance()
        .get::<_, AccountLimits>(&StorageKey::DefaultLimits)
        .unwrap_or(DEFAULT_LIMITS);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    caps
}

/// Persist the global default caps and extend instance TTL.
///
/// # Errors
/// - [`YieldLimitError::InvalidLimit`] — any cap exceeds [`MAX_CAP`].
pub fn write_default_limits(env: &Env, caps: &AccountLimits) -> Result<(), YieldLimitError> {
    if !caps.is_valid() {
        return Err(YieldLimitError::InvalidLimit);
    }
    env.storage()
        .instance()
        .set(&StorageKey::DefaultLimits, caps);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    Ok(())
}

/// Read the per-account override or fall back to [`read_default_limits`].
///
/// Per-account overrides live in **persistent** storage so configuring caps for
/// one account never grows (or evicts) the shared instance entry. The entry's
/// TTL is extended on every read.
///
/// # Migration fallback
///
/// Overrides written by the pre-#1414 layout are still in *instance* storage
/// under the same key. When the persistent slot is absent but the instance slot
/// is present, the value is migrated to persistent storage (and the instance
/// slot removed) before being returned. The migration is therefore lazy, does
/// not require a dedicated entrypoint, and keeps working if an account is first
/// touched long after the WASM upgrade.
pub fn read_account_limits(env: &Env, account: &Address) -> AccountLimits {
    let key = StorageKey::AccountLimits(account.clone());
    if let Some(caps) = env.storage().persistent().get::<_, AccountLimits>(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, CAPS_BUMP_THRESHOLD, CAPS_BUMP_AMOUNT);
        return caps;
    }

    // Legacy (v1) instance-storage slot: migrate it on first touch.
    if let Some(caps) = env.storage().instance().get::<_, AccountLimits>(&key) {
        env.storage().persistent().set(&key, &caps);
        env.storage()
            .persistent()
            .extend_ttl(&key, CAPS_BUMP_THRESHOLD, CAPS_BUMP_AMOUNT);
        env.storage().instance().remove(&key);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        return caps;
    }

    read_default_limits(env)
}

/// Persist a per-account override in persistent storage and extend its TTL.
///
/// Any legacy instance-storage slot for the same account is removed so the two
/// layouts cannot disagree.
///
/// # Errors
/// - [`YieldLimitError::InvalidLimit`] — any cap exceeds [`MAX_CAP`].
pub fn write_account_limits(
    env: &Env,
    account: &Address,
    caps: &AccountLimits,
) -> Result<(), YieldLimitError> {
    if !caps.is_valid() {
        return Err(YieldLimitError::InvalidLimit);
    }
    let key = StorageKey::AccountLimits(account.clone());
    env.storage().persistent().set(&key, caps);
    env.storage()
        .persistent()
        .extend_ttl(&key, CAPS_BUMP_THRESHOLD, CAPS_BUMP_AMOUNT);
    if env.storage().instance().has(&key) {
        env.storage().instance().remove(&key);
    }
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    Ok(())
}

/// Remove any per-account override (persistent and legacy instance slots).
/// After this call the account falls back to the global default caps.
pub fn clear_account_limits(env: &Env, account: &Address) {
    let key = StorageKey::AccountLimits(account.clone());
    env.storage().persistent().remove(&key);
    env.storage().instance().remove(&key);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

/// Return `true` when `operator` is a registered operator.
///
/// Extends the persistent flag's TTL when it exists so an operator that is only
/// used occasionally does not silently drop out of the registry.
pub fn read_operator(env: &Env, operator: &Address) -> bool {
    let key = StorageKey::Operator(operator.clone());
    let registered = env
        .storage()
        .persistent()
        .get::<_, bool>(&key)
        .unwrap_or(false);
    if registered {
        env.storage()
            .persistent()
            .extend_ttl(&key, CAPS_BUMP_THRESHOLD, CAPS_BUMP_AMOUNT);
    }
    registered
}

/// Authenticate `operator` and require that it is registered.
///
/// # Error precedence
/// 1. `operator.require_auth()` — an unsigned call never reaches contract
///    logic.
/// 2. [`YieldLimitError::Unauthorized`] — the signer is not a registered
///    operator.
///
/// The per-account cap is evaluated by the caller after this check, so cap
/// errors are never reported to an unregistered caller.
pub fn require_operator(env: &Env, operator: &Address) -> Result<(), YieldLimitError> {
    operator.require_auth();
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    if !read_operator(env, operator) {
        return Err(YieldLimitError::Unauthorized);
    }
    Ok(())
}

/// Read the live per-account counters, returning a zero struct if the
/// caller has no recorded state yet. Bumps persistent TTL on read.
pub fn read_account_state(env: &Env, account: &Address) -> AccountState {
    let key = StorageKey::AccountState(account.clone());
    match env.storage().persistent().get::<_, AccountState>(&key) {
        Some(state) => {
            env.storage()
                .persistent()
                .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
            state
        }
        // No recorded state yet: a zero struct is the correct default, and
        // there is no entry whose TTL could (or should) be extended.
        None => AccountState::zero(),
    }
}

/// Persist the live per-account counters and bump persistent TTL.
pub fn write_account_state(env: &Env, account: &Address, state: &AccountState) {
    let key = StorageKey::AccountState(account.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
}

/// Zero `account`'s counters and return the previous state.
///
/// An account with no recorded state is left untouched (no ledger entry is
/// created) and reports an all-zero previous state.
pub fn reset_account_counters(env: &Env, account: &Address) -> AccountState {
    let key = StorageKey::AccountState(account.clone());
    let previous = read_account_state(env, account);
    if env.storage().persistent().has(&key) {
        write_account_state(env, account, &AccountState::zero());
    }
    previous
}

// ---------------------------------------------------------------------
// Free functions — gate checks
// ---------------------------------------------------------------------

/// Return `true` if `account` can place another bet without exceeding the
/// concurrent cap or the current window cap (when windows are enabled).
pub fn can_place_bet(env: &Env, account: &Address) -> bool {
    let caps = read_account_limits(env, account);
    let state = read_account_state(env, account);
    if state.bets >= caps.max_bets {
        return false;
    }
    let window_ledgers = read_window_ledgers(env);
    if window_ledgers == 0 {
        return true;
    }
    let window_caps = read_window_limits(env);
    effective_window_state(env, account, window_ledgers).bets_opened < window_caps.max_bets
}

/// Return `true` if `account` can open another position without exceeding the
/// concurrent cap or the current window cap (when windows are enabled).
pub fn can_open_position(env: &Env, account: &Address) -> bool {
    let caps = read_account_limits(env, account);
    let state = read_account_state(env, account);
    if state.positions >= caps.max_positions {
        return false;
    }
    let window_ledgers = read_window_ledgers(env);
    if window_ledgers == 0 {
        return true;
    }
    let window_caps = read_window_limits(env);
    effective_window_state(env, account, window_ledgers).positions_opened
        < window_caps.max_positions
}

/// Return `true` if `account` can subscribe again without exceeding the
/// concurrent cap or the current window cap (when windows are enabled).
pub fn can_subscribe(env: &Env, account: &Address) -> bool {
    let caps = read_account_limits(env, account);
    let state = read_account_state(env, account);
    if state.subscriptions >= caps.max_subscriptions {
        return false;
    }
    let window_ledgers = read_window_ledgers(env);
    if window_ledgers == 0 {
        return true;
    }
    let window_caps = read_window_limits(env);
    effective_window_state(env, account, window_ledgers).subscriptions_added
        < window_caps.max_subscriptions
}

// ---------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------

/// Callora Yield per-account limits enforcement contract.
///
/// `init` registers an `admin`; the admin (and only the admin) may configure
/// per-account caps and the global default caps. End-users may increment or
/// decrement their own counters via `place_bet`, `open_position`,
/// `subscribe`, `clear_bet`, `close_position`, `unsubscribe`.
#[contract]
pub struct CalloraYieldLimits;

#[contractimpl]
impl CalloraYieldLimits {
    // -----------------------------------------------------------------
    // init + lifecycle
    // -----------------------------------------------------------------

    /// Initialize the contract with an admin address.
    ///
    /// # Arguments
    /// * `admin` — Address authorised to mutate per-account caps, swap
    ///   admins, upgrade the contract, and pause/unpause the surface.
    ///
    /// # Errors
    /// - [`YieldLimitError::AlreadyInitialized`] — admin already set.
    pub fn init(env: Env, admin: Address) -> Result<(), YieldLimitError> {
        // Authenticate before touching storage. Without this, whoever calls
        // `init` first between deploy and initialization becomes the admin and
        // permanently controls every admin entrypoint, including `upgrade`
        // (#1414).
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(YieldLimitError::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        // Record the deployed storage-layout version + hash so the upgrade
        // path can enforce ordered, single-step migrations against a known
        // schema from the very first deployment.
        StorageMigrationValidator::record_deployed_schema(
            &env,
            STORAGE_MIGRATION_VERSION,
            &storage_layout_hash(&env),
        )
        .map_err(|_| YieldLimitError::UpgradeRejected)?;

        env.events().publish((events::event_init(&env), admin), ());
        Ok(())
    }

    /// Return the stored admin address.
    ///
    /// # Errors
    /// - [`YieldLimitError::NotInitialized`] — `init` has not been called.
    pub fn get_admin(env: Env) -> Result<Address, YieldLimitError> {
        let admin = read_admin(&env)?;
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Ok(admin)
    }

    // -----------------------------------------------------------------
    // Two-step admin rotation
    // -----------------------------------------------------------------

    /// Initiate a two-step admin transfer (`caller` must be current admin).
    ///
    /// Re-nominating the current admin is permitted — once the pending
    /// transfer is accepted the admin role remains effectively unchanged,
    /// which is safe because every other admin-gated path still requires a
    /// fresh `require_auth` round-trip through the nominated address.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events()
            .publish((events::event_admin_nominated(&env), caller), new_admin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Ok(())
    }

    /// Complete a pending admin transfer (`caller` must be the pending admin).
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        let pending = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::PendingAdmin)
            .ok_or(YieldLimitError::Unauthorized)?;
        caller.require_auth();
        if caller != pending {
            return Err(YieldLimitError::Unauthorized);
        }
        env.storage().instance().set(&StorageKey::Admin, &caller);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_accepted(&env), caller.clone()), ());
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Ok(())
    }

    /// Cancel a pending admin transfer (`caller` must be current admin).
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let pending = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::PendingAdmin)
            .ok_or(YieldLimitError::Unauthorized)?;
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_cancelled(&env), caller), pending);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Ok(())
    }

    // -----------------------------------------------------------------
    // Limits configuration
    // -----------------------------------------------------------------

    /// Replace the global default caps (admin only).
    pub fn set_default_limits(
        env: Env,
        caller: Address,
        max_bets: u32,
        max_positions: u32,
        max_subscriptions: u32,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let caps = AccountLimits {
            max_bets,
            max_positions,
            max_subscriptions,
        };
        write_default_limits(&env, &caps)?;
        env.events().publish(
            (events::event_default_limits_set(&env), caller, caps.clone()),
            caps,
        );
        Ok(())
    }

    /// Set explicit per-account caps overriding the global default
    /// (admin only).
    pub fn set_account_limits(
        env: Env,
        caller: Address,
        account: Address,
        max_bets: u32,
        max_positions: u32,
        max_subscriptions: u32,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let caps = AccountLimits {
            max_bets,
            max_positions,
            max_subscriptions,
        };
        write_account_limits(&env, &account, &caps)?;
        env.events().publish(
            (
                events::event_account_limits_set(&env),
                caller,
                account.clone(),
            ),
            caps,
        );
        Ok(())
    }

    /// Remove any per-account override so the account falls back to global
    /// defaults (admin only).
    pub fn clear_account_limits(
        env: Env,
        caller: Address,
        account: Address,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        clear_account_limits(&env, &account);
        env.events().publish(
            (events::event_account_limits_cleared(&env), caller, account),
            (),
        );
        Ok(())
    }

    /// Reset a single account's counters to zero (admin only).
    ///
    /// This is a recovery tool for the case where an operator misses a
    /// decrement and leaves an account pinned at its cap. All three counters
    /// (`bets`, `positions`, `subscriptions`) are zeroed and the previous
    /// counters are published in the `account_state_reset` event.
    ///
    /// Resetting an account with no recorded state performs no storage write;
    /// the event is still emitted with an all-zero previous state so indexers
    /// observe every reset attempt.
    ///
    /// # Errors
    /// - [`YieldLimitError::Unauthorized`] — caller is not the admin.
    /// - [`YieldLimitError::NotInitialized`] — `init` has not been called.
    pub fn reset_account_state(
        env: Env,
        caller: Address,
        account: Address,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let previous = reset_account_counters(&env, &account);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events().publish(
            (events::event_account_state_reset(&env), caller, account),
            previous,
        );
        Ok(())
    }

    /// Reset many accounts' counters to zero in one atomic call (admin only).
    ///
    /// `accounts` must contain between `1` and [`MAX_BATCH_CLEAR`] entries;
    /// the bounds are checked before any counter is touched, so a rejected
    /// batch modifies nothing. Each account emits its own
    /// `account_state_reset` event with its previous counters. Duplicate
    /// addresses are processed in order: the first occurrence resets the
    /// counters and later occurrences reset the already-zeroed state.
    ///
    /// # Errors
    /// - [`YieldLimitError::Unauthorized`] — caller is not the admin.
    /// - [`YieldLimitError::NotInitialized`] — `init` has not been called.
    /// - [`YieldLimitError::EmptyBatch`] — `accounts` is empty.
    /// - [`YieldLimitError::BatchTooLarge`] — `accounts` exceeds [`MAX_BATCH_CLEAR`].
    pub fn batch_clear_accounts(
        env: Env,
        caller: Address,
        accounts: Vec<Address>,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;

        let len = accounts.len();
        if len == 0 {
            return Err(YieldLimitError::EmptyBatch);
        }
        if len > MAX_BATCH_CLEAR {
            return Err(YieldLimitError::BatchTooLarge);
        }

        for account in accounts.iter() {
            let previous = reset_account_counters(&env, &account);
            env.events().publish(
                (
                    events::event_account_state_reset(&env),
                    caller.clone(),
                    account,
                ),
                previous,
            );
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Ok(())
    }

    /// Set the global rolling-window length in ledgers (admin only).
    ///
    /// `0` disables rolling windows and restores the pre-#1522 behaviour
    /// exactly. Any non-zero value must lie within
    /// [`MIN_WINDOW_LEDGERS`]`..=`[`MAX_WINDOW_LEDGERS`].
    ///
    /// Existing window counters are left untouched; the next mutation rolls
    /// them lazily against the new length.
    pub fn set_window_ledgers(
        window_ledgers: u32,
        write_window_ledgers(&env, window_ledgers)?;
            (events::event_window_ledgers_set(&env), caller),
            window_ledgers,
        );
    }

    /// Set the global per-window caps (admin only).
    ///
    /// Window caps apply only while [`Self::set_window_ledgers`] is non-zero.
    /// A cap of `0` blocks all opens of that kind inside a window.
    pub fn set_window_limits(
        max_bets: u32,
        max_positions: u32,
        max_subscriptions: u32,
        let caps = WindowLimits {
            max_bets,
            max_positions,
            max_subscriptions,
        };
        write_window_limits(&env, &caps)?;
            (events::event_window_limits_set(&env), caller, caps.clone()),
            caps,
        );
    }

    // -----------------------------------------------------------------
    // Operator registry (admin only)
    // -----------------------------------------------------------------

    /// Register `operator` as an authorised counter mutator (admin only).
    ///
    /// Re-registering an operator refreshes its persistent TTL and emits the
    /// event again; it is otherwise a no-op.
    pub fn set_operator(
        env: Env,
        caller: Address,
        operator: Address,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let key = StorageKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, CAPS_BUMP_THRESHOLD, CAPS_BUMP_AMOUNT);
        env.events().publish(
            (events::event_operator_set(&env), caller, operator.clone()),
            (),
        );
        Ok(())
    }

    /// Remove an operator from the registry (admin only).
    ///
    /// Removing an unknown operator is a safe no-op and emits no event.
    /// Counter mutations from the removed address fail with `Unauthorized`
    /// immediately afterwards; existing counters are untouched.
    pub fn remove_operator(
        env: Env,
        caller: Address,
        operator: Address,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let key = StorageKey::Operator(operator.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().remove(&key);
            env.events().publish(
                (
                    events::event_operator_removed(&env),
                    caller,
                    operator.clone(),
                ),
                (),
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // Operator-gated counter mutators
    // -----------------------------------------------------------------

    /// Increment `account`'s open-bet counter after enforcing operator auth
    /// and the account's per-account cap.
    ///
    /// Error precedence: operator auth, then operator registration, then the
    /// cap (`BetsAtCap`).
    pub fn place_bet(env: Env, operator: Address, account: Address) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let caps = read_account_limits(&env, &account);
        let mut state = read_account_state(&env, &account);
        // Pre-check: surfacing BetsAtCap as a typed error gives callers a
        // stable code to branch on rather than a generic panic.
        if state.bets >= caps.max_bets {
            return Err(YieldLimitError::BetsAtCap);
        }

        // Rolling window: enforced in addition to the concurrent cap. A
        // disabled window (`0`) skips all window reads/writes so behaviour
        // and storage usage are identical to the pre-window contract.
        let window_ledgers = read_window_ledgers(&env);
        if window_ledgers != 0 {
            let window_caps = read_window_limits(&env);
            let (mut window_state, rolled) =
                load_window_for_mutation(&env, &caller, window_ledgers);
            if rolled {
                env.events().publish(
                    (events::event_window_rolled(&env), caller.clone()),
                    (window_state.window_start, window_ledgers),
                );
            }
            if window_state.bets_opened >= window_caps.max_bets {
                return Err(YieldLimitError::BetsWindowCap);
            }
            window_state.add_bet()?;
            write_window_state(&env, &caller, &window_state);
        }

        state.add_bet()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (events::event_bet_placed(&env), operator, account.clone()),
            (state.bets, caps.max_bets),
        );
        Ok(())
    }

    /// Decrement `account`'s open-bet counter (operator only).
    pub fn clear_bet(env: Env, operator: Address, account: Address) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let mut state = read_account_state(&env, &account);
        state.sub_bet()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (events::event_bet_cleared(&env), operator, account.clone()),
            state.bets,
        );
        Ok(())
    }

    /// Increment `account`'s open-position counter after enforcing operator
    /// auth and the account's per-account cap.
    pub fn open_position(
        env: Env,
        operator: Address,
        account: Address,
    ) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let caps = read_account_limits(&env, &account);
        let mut state = read_account_state(&env, &account);
        if state.positions >= caps.max_positions {
            return Err(YieldLimitError::PositionsAtCap);
        }

        let window_ledgers = read_window_ledgers(&env);
        if window_ledgers != 0 {
            let window_caps = read_window_limits(&env);
            let (mut window_state, rolled) =
                load_window_for_mutation(&env, &caller, window_ledgers);
            if rolled {
                env.events().publish(
                    (events::event_window_rolled(&env), caller.clone()),
                    (window_state.window_start, window_ledgers),
                );
            }
            if window_state.positions_opened >= window_caps.max_positions {
                return Err(YieldLimitError::PositionsWindowCap);
            }
            window_state.add_position()?;
            write_window_state(&env, &caller, &window_state);
        }

        state.add_position()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (
                events::event_position_opened(&env),
                operator,
                account.clone(),
            ),
            (state.positions, caps.max_positions),
        );
        Ok(())
    }

    /// Decrement `account`'s open-position counter (operator only).
    pub fn close_position(
        env: Env,
        operator: Address,
        account: Address,
    ) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let mut state = read_account_state(&env, &account);
        state.sub_position()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (
                events::event_position_closed(&env),
                operator,
                account.clone(),
            ),
            state.positions,
        );
        Ok(())
    }

    /// Increment `account`'s subscription counter after enforcing operator
    /// auth and the account's per-account cap.
    pub fn subscribe(env: Env, operator: Address, account: Address) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let caps = read_account_limits(&env, &account);
        let mut state = read_account_state(&env, &account);
        if state.subscriptions >= caps.max_subscriptions {
            return Err(YieldLimitError::SubscriptionsAtCap);
        }

        let window_ledgers = read_window_ledgers(&env);
        if window_ledgers != 0 {
            let window_caps = read_window_limits(&env);
            let (mut window_state, rolled) =
                load_window_for_mutation(&env, &caller, window_ledgers);
            if rolled {
                env.events().publish(
                    (events::event_window_rolled(&env), caller.clone()),
                    (window_state.window_start, window_ledgers),
                );
            }
            if window_state.subscriptions_added >= window_caps.max_subscriptions {
                return Err(YieldLimitError::SubscriptionsWindowCap);
            }
            window_state.add_subscription()?;
            write_window_state(&env, &caller, &window_state);
        }

        state.add_subscription()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (
                events::event_subscription_added(&env),
                operator,
                account.clone(),
            ),
            (state.subscriptions, caps.max_subscriptions),
        );
        Ok(())
    }

    /// Decrement `account`'s subscription counter (operator only).
    pub fn unsubscribe(
        env: Env,
        operator: Address,
        account: Address,
    ) -> Result<(), YieldLimitError> {
        require_operator(&env, &operator)?;
        let mut state = read_account_state(&env, &account);
        state.sub_subscription()?;
        write_account_state(&env, &account, &state);
        env.events().publish(
            (
                events::event_subscription_removed(&env),
                operator,
                account.clone(),
            ),
            state.subscriptions,
        );
        Ok(())
    }

    // -----------------------------------------------------------------
    // Read-only views
    // -----------------------------------------------------------------

    /// Read the global default caps.
    pub fn get_default_limits(env: Env) -> AccountLimits {
        read_default_limits(&env)
    }

    /// Read the effective per-account caps.
    pub fn get_account_limits(env: Env, account: Address) -> AccountLimits {
        read_account_limits(&env, &account)
    }

    /// Read the live per-account counters.
    pub fn get_account_state(env: Env, account: Address) -> AccountState {
        read_account_state(&env, &account)
    }

    /// Read the remaining state capacity for `account`.
    ///
    /// Computes `saturating_sub(cap, state)` for each dimension against the account's
    /// effective caps (explicit override or default). Does not require auth and never underflows.
    pub fn get_remaining_capacity(env: Env, account: Address) -> RemainingCapacity {
        let limits = read_account_limits(&env, &account);
        let state = read_account_state(&env, &account);
        RemainingCapacity {
            bets: limits.max_bets.saturating_sub(state.bets),
            positions: limits.max_positions.saturating_sub(state.positions),
            subscriptions: limits.max_subscriptions.saturating_sub(state.subscriptions),
        }
    }

    /// Return `true` when `operator` may mutate counters.
    pub fn get_operator(env: Env, operator: Address) -> bool {
        read_operator(&env, &operator)
    }

    /// Dry-run check: would `place_bet` succeed for `account`?
    pub fn can_place_bet(env: Env, account: Address) -> bool {
        can_place_bet(&env, &account)
    }

    /// Dry-run check: would `open_position` succeed for `account`?
    pub fn can_open_position(env: Env, account: Address) -> bool {
        can_open_position(&env, &account)
    }

    /// Dry-run check: would `subscribe` succeed for `account`?
    pub fn can_subscribe(env: Env, account: Address) -> bool {
        can_subscribe(&env, &account)
    }

    // -----------------------------------------------------------------
    // Upgrade
    // -----------------------------------------------------------------

    /// Replace the WASM, run the storage-migration guard, and persist the new
    /// hash under [`StorageKey::WasmVersion`] (admin only).
    ///
    /// The guard runs in the *current* (old) code, before
    /// `update_current_contract_wasm`, and never mutates business state. It
    /// rejects an all-zero WASM hash, enforces ordered single-step migrations,
    /// and detects a silent layout change to [`AccountLimits`]/[`AccountState`].
    ///
    /// # Errors
    /// - [`YieldLimitError::Unauthorized`] — caller is not the current admin.
    /// - [`YieldLimitError::NotInitialized`] — `init` has not been called.
    /// - [`YieldLimitError::UpgradeRejected`] — the migration validator
    ///   rejected the upgrade (all-zero hash, version skip, rollback without a
    ///   backup, or a silent storage-layout change).
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;

        // ── Pre-upgrade storage-migration validation ──────────────────────
        let layout_hash = storage_layout_hash(&env);
        StorageMigrationValidator::validate_before_upgrade(
            &env,
            STORAGE_MIGRATION_VERSION,
            &layout_hash,
            &callora_storage_migration::zero_layout_hash(&env),
            &new_wasm_hash,
            false,
        )
        .map_err(|_| YieldLimitError::UpgradeRejected)?;
        StorageMigrationValidator::finalize_migration(
            &env,
            STORAGE_MIGRATION_VERSION,
            &layout_hash,
            &new_wasm_hash,
        )
        .map_err(|_| YieldLimitError::UpgradeRejected)?;

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.storage()
            .instance()
            .set(&StorageKey::WasmVersion, &new_wasm_hash);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events()
            .publish((events::event_upgraded(&env), caller), new_wasm_hash);
        Ok(())
    }

    /// Return the last upgraded WASM hash, or `None` if never upgraded.
    ///
    /// This is the on-chain source of truth for "which code is live" after an
    /// upgrade, mirroring `get_version` on the revenue pool and checkpoint
    /// contracts.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage().instance().get(&StorageKey::WasmVersion)
    }
}
