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
//! - [`AccountLimits`] are stored in **instance** storage under
//!   [`StorageKey::AccountLimits`]`(Address)`. They are sparse overrides; an
//!   account with no explicit override falls back to the global
//!   [`DEFAULT_LIMITS`] constant.
//!
//! - [`AccountState`] counters are stored in **persistent** storage under
//!   [`StorageKey::AccountState`]`(Address)`. Persistent storage lets the
//!   contract scale to many accounts (instance storage is small and shared
//!   with config) and keeps counters alive across the typical 7-day ledger
//!   archival window via TTL extensions on every increment/decrement.
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
//! | Entrypoint                                       | Authorized by                              |
//! |--------------------------------------------------|-------------------------------------------|
//! | `init`, `set_admin`, `accept_admin`, `upgrade`   | admin (`caller == admin`)                |
//! | `cancel_admin_transfer`                          | admin (`caller == admin`)                |
//! | `set_default_limits`, `set_account_limits`, `clear_account_limits` | admin (`caller == admin`) |
//! | `set_window_ledgers`, `set_window_limits`        | admin (`caller == admin`)                |
//! | `place_bet`, `clear_bet`, `open_position`, `close_position`,       | caller (their own counter) |
//! | `subscribe`, `unsubscribe`                      | caller (their own counter)                |
//!
//! Read-only views (`get_admin`, `get_default_limits`, `get_account_limits`,
//! `get_account_state`, `get_window_ledgers`, `get_window_limits`,
//! `get_window_state`, `can_*`) do **not** call `require_auth`.
//!
//! # Overflow Safety
//!
//! Every count mutation uses `u32::checked_add` / `u32::checked_sub`. The
//! outcomes are folded into typed [`YieldLimitError`] variants
//! ([`YieldLimitError::Overflow`] / [`YieldLimitError::CounterUnderflow`]) so
//! production code paths never invoke `unwrap()`.

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

use crate::errors::YieldLimitError;
use crate::events;

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
    /// Global rolling-window length in ledgers (`0` disables windows).
    WindowLedgers,
    /// Global per-window caps (instance storage).
    DefaultWindowLimits,
    /// Per-account rolling-window counters (persistent storage, additive key).
    WindowState(Address),
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
/// Per-account overrides stored in instance storage survive across calls
/// along with other config; persistent storage is reserved for the live
/// state counters.
pub fn read_account_limits(env: &Env, account: &Address) -> AccountLimits {
    if let Some(caps) = env
        .storage()
        .instance()
        .get::<_, AccountLimits>(&StorageKey::AccountLimits(account.clone()))
    {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        return caps;
    }
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    read_default_limits(env)
}

/// Persist a per-account override and extend instance TTL.
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
    env.storage()
        .instance()
        .set(&StorageKey::AccountLimits(account.clone()), caps);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    Ok(())
}

/// Remove any per-account override. After this call the account falls back
/// to the global default caps.
pub fn clear_account_limits(env: &Env, account: &Address) {
    env.storage()
        .instance()
        .remove(&StorageKey::AccountLimits(account.clone()));
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

/// Read the global rolling-window length in ledgers (`0` = disabled).
pub fn read_window_ledgers(env: &Env) -> u32 {
    let ledgers = env
        .storage()
        .instance()
        .get::<_, u32>(&StorageKey::WindowLedgers)
        .unwrap_or(0);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    ledgers
}

/// Persist the global rolling-window length and extend instance TTL.
///
/// # Errors
/// - [`YieldLimitError::InvalidLimit`] — length is neither `0` nor within
///   [`MIN_WINDOW_LEDGERS`]`..=`[`MAX_WINDOW_LEDGERS`].
pub fn write_window_ledgers(env: &Env, window_ledgers: u32) -> Result<(), YieldLimitError> {
    if !is_valid_window_ledgers(window_ledgers) {
        return Err(YieldLimitError::InvalidLimit);
    }
    env.storage()
        .instance()
        .set(&StorageKey::WindowLedgers, &window_ledgers);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    Ok(())
}

/// Read the global per-window caps, falling back to
/// [`DEFAULT_WINDOW_LIMITS`] (all dimensions at [`MAX_CAP`]).
pub fn read_window_limits(env: &Env) -> WindowLimits {
    let caps = env
        .storage()
        .instance()
        .get::<_, WindowLimits>(&StorageKey::DefaultWindowLimits)
        .unwrap_or(DEFAULT_WINDOW_LIMITS);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    caps
}

/// Persist the global per-window caps and extend instance TTL.
///
/// # Errors
/// - [`YieldLimitError::InvalidLimit`] — any window cap exceeds [`MAX_CAP`].
pub fn write_window_limits(env: &Env, caps: &WindowLimits) -> Result<(), YieldLimitError> {
    if !caps.is_valid() {
        return Err(YieldLimitError::InvalidLimit);
    }
    env.storage()
        .instance()
        .set(&StorageKey::DefaultWindowLimits, caps);
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    Ok(())
}

/// Read the stored rolling-window counters, if any.
pub fn read_window_state_opt(env: &Env, account: &Address) -> Option<WindowState> {
    let key = StorageKey::WindowState(account.clone());
    let state = env.storage().persistent().get::<_, WindowState>(&key);
    if state.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
    }
    state
}

/// Read the stored rolling-window counters, returning a zero struct if the
/// account has no recorded window yet. Bumps persistent TTL when present.
pub fn read_window_state(env: &Env, account: &Address) -> WindowState {
    read_window_state_opt(env, account).unwrap_or_else(WindowState::zero)
}

/// Persist the rolling-window counters and bump persistent TTL.
pub fn write_window_state(env: &Env, account: &Address, state: &WindowState) {
    let key = StorageKey::WindowState(account.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
}

/// Load the effective window counters for a mutation, lazily rolling them
/// when the current ledger has reached the end of the stored window.
///
/// Returns `(state, rolled)`. A brand-new account anchors its first window at
/// the current ledger sequence. The caller decides whether to emit a
/// `window_rolled` event (mutation) or discard the rolled state (view).
pub fn load_window_for_mutation(
    env: &Env,
    account: &Address,
    window_ledgers: u32,
) -> (WindowState, bool) {
    let current = env.ledger().sequence();
    match read_window_state_opt(env, account) {
        Some(mut state) => {
            if state.is_expired(current, window_ledgers) {
                state.roll(current);
                (state, true)
            } else {
                (state, false)
            }
        }
        None => (WindowState::started_at(current), false),
    }
}

/// Compute the effective window counters for a read-only view without
/// writing storage. An expired window reads as freshly zeroed.
pub fn effective_window_state(env: &Env, account: &Address, window_ledgers: u32) -> WindowState {
    let current = env.ledger().sequence();
    match read_window_state_opt(env, account) {
        Some(mut state) => {
            if state.is_expired(current, window_ledgers) {
                state.roll(current);
            }
            state
        }
        None => WindowState::started_at(current),
    }
}

/// Read the live per-account counters, returning a zero struct if the
/// caller has no recorded state yet. Bumps persistent TTL on read.
pub fn read_account_state(env: &Env, account: &Address) -> AccountState {
    let key = StorageKey::AccountState(account.clone());
    let stored = env.storage().persistent().get::<_, AccountState>(&key);
    // Only extend the TTL of an existing entry: extending a missing
    // persistent key fails with `Storage, MissingValue` on Soroban.
    if stored.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
    }
    stored.unwrap_or_else(AccountState::zero)
}

/// Persist the live per-account counters and bump persistent TTL.
pub fn write_account_state(env: &Env, account: &Address, state: &AccountState) {
    let key = StorageKey::AccountState(account.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, STATE_BUMP_THRESHOLD, STATE_BUMP_AMOUNT);
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
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(YieldLimitError::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
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

    /// Set the global rolling-window length in ledgers (admin only).
    ///
    /// `0` disables rolling windows and restores the pre-#1522 behaviour
    /// exactly. Any non-zero value must lie within
    /// [`MIN_WINDOW_LEDGERS`]`..=`[`MAX_WINDOW_LEDGERS`].
    ///
    /// Existing window counters are left untouched; the next mutation rolls
    /// them lazily against the new length.
    pub fn set_window_ledgers(
        env: Env,
        caller: Address,
        window_ledgers: u32,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        write_window_ledgers(&env, window_ledgers)?;
        env.events().publish(
            (events::event_window_ledgers_set(&env), caller),
            window_ledgers,
        );
        Ok(())
    }

    /// Set the global per-window caps (admin only).
    ///
    /// Window caps apply only while [`Self::set_window_ledgers`] is non-zero.
    /// A cap of `0` blocks all opens of that kind inside a window.
    pub fn set_window_limits(
        env: Env,
        caller: Address,
        max_bets: u32,
        max_positions: u32,
        max_subscriptions: u32,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        let caps = WindowLimits {
            max_bets,
            max_positions,
            max_subscriptions,
        };
        write_window_limits(&env, &caps)?;
        env.events().publish(
            (events::event_window_limits_set(&env), caller, caps.clone()),
            caps,
        );
        Ok(())
    }

    // -----------------------------------------------------------------
    // User-gated counter mutators
    // -----------------------------------------------------------------

    /// Increment the caller's open-bet counter after enforcing auth and
    /// the per-account cap.
    pub fn place_bet(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let caps = read_account_limits(&env, &caller);
        let mut state = read_account_state(&env, &caller);
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
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_bet_placed(&env), caller.clone()),
            (state.bets, caps.max_bets),
        );
        Ok(())
    }

    /// Decrement the caller's open-bet counter after enforcing auth.
    pub fn clear_bet(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let mut state = read_account_state(&env, &caller);
        state.sub_bet()?;
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_bet_cleared(&env), caller.clone()),
            state.bets,
        );
        Ok(())
    }

    /// Increment the caller's open-position counter after enforcing auth
    /// and the per-account cap.
    pub fn open_position(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let caps = read_account_limits(&env, &caller);
        let mut state = read_account_state(&env, &caller);
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
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_position_opened(&env), caller.clone()),
            (state.positions, caps.max_positions),
        );
        Ok(())
    }

    /// Decrement the caller's open-position counter after enforcing auth.
    pub fn close_position(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let mut state = read_account_state(&env, &caller);
        state.sub_position()?;
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_position_closed(&env), caller.clone()),
            state.positions,
        );
        Ok(())
    }

    /// Increment the caller's subscription counter after enforcing auth
    /// and the per-account cap.
    pub fn subscribe(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let caps = read_account_limits(&env, &caller);
        let mut state = read_account_state(&env, &caller);
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
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_subscription_added(&env), caller.clone()),
            (state.subscriptions, caps.max_subscriptions),
        );
        Ok(())
    }

    /// Decrement the caller's subscription counter after enforcing auth.
    pub fn unsubscribe(env: Env, caller: Address) -> Result<(), YieldLimitError> {
        caller.require_auth();
        let mut state = read_account_state(&env, &caller);
        state.sub_subscription()?;
        write_account_state(&env, &caller, &state);
        env.events().publish(
            (events::event_subscription_removed(&env), caller.clone()),
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

    /// Read the global rolling-window length in ledgers (`0` = disabled).
    pub fn get_window_ledgers(env: Env) -> u32 {
        read_window_ledgers(&env)
    }

    /// Read the global per-window caps.
    pub fn get_window_limits(env: Env) -> WindowLimits {
        read_window_limits(&env)
    }

    /// Read the stored per-account window counters.
    ///
    /// Returns a zeroed [`WindowState`] (`window_start == 0`) when the account
    /// has no recorded window yet. This view does not roll the window; use
    /// [`effective_window_state`] or the `can_*` views for the effective
    /// counters at the current ledger.
    pub fn get_window_state(env: Env, account: Address) -> WindowState {
        read_window_state(&env, &account)
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

    /// Replace the WASM and persist the new hash (admin only).
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), YieldLimitError> {
        require_admin(&env, &caller)?;
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.events()
            .publish((events::event_upgraded(&env), caller), new_wasm_hash);
        Ok(())
    }
}
