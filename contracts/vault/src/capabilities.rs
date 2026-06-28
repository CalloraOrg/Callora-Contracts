//! Capability bitmap for the Callora Vault contract.
//!
//! Each bit in the `u64` returned by [`capabilities`] represents a distinct,
//! stable feature supported by this contract version.  Bits are assigned once
//! and never reassigned to a different feature, so clients can rely on them
//! across upgrades.
//!
//! # Quick-start
//! ```ignore
//! let caps = client.capabilities();
//! if caps & CAP_BATCH_DEDUCT != 0 {
//!     // safe to call batch_deduct
//! }
//! ```
//!
//! See `docs/CAPABILITIES.md` for the full bit registry and integration guide.

use soroban_sdk::Env;

/// Bit 0 — Deposit: `deposit()` accepts USDC from allowlisted callers.
/// Introduced: v1.0.0
pub const CAP_DEPOSIT: u64 = 1 << 0;

/// Bit 1 — Withdraw: owner can withdraw USDC via `withdraw()` / `withdraw_to()`.
/// Introduced: v1.0.0
pub const CAP_WITHDRAW: u64 = 1 << 1;

/// Bit 2 — Deduct: authorized caller can deduct funds via `deduct()`.
/// Introduced: v1.0.0
pub const CAP_DEDUCT: u64 = 1 << 2;

/// Bit 3 — Batch deduct: authorized caller deducts multiple items atomically via `batch_deduct()`.
/// Introduced: v1.0.0
pub const CAP_BATCH_DEDUCT: u64 = 1 << 3;

/// Bit 4 — Pause circuit-breaker: admin or owner can pause/unpause the vault via
/// `pause()` / `unpause()`.  Deposits and deducts are blocked while paused.
/// Introduced: v1.0.0
pub const CAP_PAUSE: u64 = 1 << 4;

/// Bit 5 — Authorized caller: a designated address is permitted to invoke deduct
/// operations.  Configured via `set_authorized_caller()`.
/// Introduced: v1.0.0
pub const CAP_AUTHORIZED_CALLER: u64 = 1 << 5;

/// Bit 6 — Offering metadata: per-offering metadata stored and queried on-chain.
/// Managed via `set_metadata()`, `update_metadata()`, `remove_metadata()`.
/// Introduced: v1.0.0
pub const CAP_OFFERING_METADATA: u64 = 1 << 6;

/// Bit 7 — Price registry: per-offering prices stored on-chain.
/// Managed via `set_price()`, `get_price()`, `remove_price()`, `list_prices()`.
/// Introduced: v1.0.0
pub const CAP_PRICE_REGISTRY: u64 = 1 << 7;

/// Bit 8 — Request idempotency: `deduct` and `batch_deduct` accept an optional
/// `request_id` that prevents duplicate execution (at-least-once retry semantics).
/// Introduced: v1.0.0
pub const CAP_REQUEST_IDEMPOTENCY: u64 = 1 << 8;

/// Bit 9 — Two-step ownership transfer: ownership moves via `transfer_ownership()` /
/// `accept_ownership()`, preventing accidental loss.
/// Introduced: v1.0.0
pub const CAP_TWO_STEP_OWNERSHIP: u64 = 1 << 9;

/// Bit 10 — Two-step admin transfer: the admin role moves via `set_admin()` /
/// `accept_admin()` / `cancel_admin_transfer()`.
/// Introduced: v1.0.0
pub const CAP_TWO_STEP_ADMIN: u64 = 1 << 10;

/// Bit 11 — Settlement integration: deducted funds are forwarded to a settlement
/// contract via `receive_payment()`.  Configured via `set_settlement()`.
/// Introduced: v1.0.0
pub const CAP_SETTLEMENT: u64 = 1 << 11;

/// Bit 12 — Revenue pool: an optional revenue pool address is configurable via a
/// two-step `propose_revenue_pool()` / `accept_revenue_pool()` pattern.
/// Introduced: v1.0.0
pub const CAP_REVENUE_POOL: u64 = 1 << 12;

/// Bit 13 — Developer rate limiting: per-developer token-bucket rate limits are
/// enforced on deduct operations.  Configured via `set_developer_rate_limit()`.
/// Introduced: v1.0.0
pub const CAP_RATE_LIMIT: u64 = 1 << 13;

/// Bit 14 — Admin broadcast: admin can emit signed on-chain messages with severity
/// levels via `broadcast()`.
/// Introduced: v1.0.0
pub const CAP_ADMIN_BROADCAST: u64 = 1 << 14;

/// Bit 15 — Depositor allowlist: owner restricts deposits to approved addresses.
/// Managed via `add_address()`, `set_allowed_depositor()`, `clear_all()`,
/// `get_allowlist()`.
/// Introduced: v1.0.0
pub const CAP_DEPOSITOR_ALLOWLIST: u64 = 1 << 15;

/// Bit 16 — Slippage guard: `deduct` enforces a caller-supplied `max_fee_bps` cap
/// expressed as basis points of the current vault balance.
/// Introduced: v1.0.0
pub const CAP_SLIPPAGE_GUARD: u64 = 1 << 16;

/// Bit 17 — Contract upgrade: admin can replace the WASM via `upgrade()`.
/// Introduced: v1.0.0
pub const CAP_UPGRADE: u64 = 1 << 17;

// Bits 18–63 are reserved for future capabilities and are always zero.

/// Bitmask of all capabilities exposed by this contract version.
///
/// Combine individual `CAP_*` constants with `&` to test for a specific feature:
/// ```ignore
/// assert!(caps & CAP_DEPOSIT != 0);
/// ```
pub const ALL_CAPABILITIES: u64 = CAP_DEPOSIT
    | CAP_WITHDRAW
    | CAP_DEDUCT
    | CAP_BATCH_DEDUCT
    | CAP_PAUSE
    | CAP_AUTHORIZED_CALLER
    | CAP_OFFERING_METADATA
    | CAP_PRICE_REGISTRY
    | CAP_REQUEST_IDEMPOTENCY
    | CAP_TWO_STEP_OWNERSHIP
    | CAP_TWO_STEP_ADMIN
    | CAP_SETTLEMENT
    | CAP_REVENUE_POOL
    | CAP_RATE_LIMIT
    | CAP_ADMIN_BROADCAST
    | CAP_DEPOSITOR_ALLOWLIST
    | CAP_SLIPPAGE_GUARD
    | CAP_UPGRADE;

/// Return the capability bitmap for this contract.
///
/// Each set bit signals a supported feature.  Bits are stable across upgrades —
/// once assigned a bit position is never reused for a different feature.
/// Reserved bits (18–63) are always zero.
///
/// No authentication is required; this is a pure view function.
pub fn capabilities(_env: &Env) -> u64 {
    ALL_CAPABILITIES
}
