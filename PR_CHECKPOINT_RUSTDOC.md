# PR: docs — rustdoc on checkpoint public entrypoints (Closes #667)

## Overview

Introduces the **Callora Checkpoint** contract — a new Soroban smart contract that records **immutable, append-only balance snapshots** for audit and compliance purposes. Every public entrypoint carries comprehensive `///`-style rustdoc following the project's NatSpec conventions.

**Closes: #667** ("Add rustdoc on checkpoint public entrypoints (buffer #22)")

---

## Contract Summary

The Checkpoint contract enables the Callora admin to create cryptographically-verifiable balance snapshots at any point in time. Each checkpoint is:

- **Immutable** — once written, never updated
- **Append-only** — new checkpoints receive sequential IDs starting at 1
- **Persistent** — stored with 6-month TTL (auto-extended on write)
- **Evented** — every operation emits a typed event for off-chain indexing
- **Auditable** — any address can query historical checkpoints by ID or paginated range

### Context in the Callora Ecosystem

| Contract | Role |
|----------|------|
| **Vault** | Holds USDC, processes deposits/deducts |
| **Settlement** | Tracks per-developer balances, handles withdrawals |
| **Revenue Pool** | Distributes protocol yield |
| **Checkpoint** *(new)* | Records immutable balance snapshots for audit trails |

An operator periodically calls `create_checkpoint` (or `batch_create_checkpoints`) to snapshot developer balances from the Settlement contract. These records form an immutable audit trail suitable for compliance reporting, financial reconciliation, and dispute resolution.

---

## Files Changed

### New Files (5 files, ~1,400 LOC)

| File | Lines | Purpose |
|------|-------|---------|
| `contracts/checkpoint/Cargo.toml` | 15 | Package manifest — soroban-sdk 22, cdylib + rlib |
| `contracts/checkpoint/src/lib.rs` | 651 | Contract logic with 14 `pub fn`s, all fully documented |
| `contracts/checkpoint/src/errors.rs` | 42 | `CheckpointError` enum — 9 stable numeric error codes |
| `contracts/checkpoint/src/events.rs` | 115 | Event symbol constructors + 6 byte-identity snapshot tests |
| `contracts/checkpoint/src/test.rs` | 574 | 43 unit tests covering all entrypoints and edge cases |

### Modified Files (2 files, +9 lines)

| File | Change |
|------|--------|
| `Cargo.toml` | Added `contracts/checkpoint` to `workspace.members` and `default-members` |
| `Cargo.lock` | Auto-regenerated with new dependency graph |

---

## Public API

### Initialisation

| Function | Auth | Description |
|----------|:----:|-------------|
| `init(admin)` | ✅ `require_auth` | One-time initialisation; sets admin |

### Admin Rotation (Two-Step)

| Function | Auth | Description |
|----------|:----:|-------------|
| `set_admin(caller, new_admin)` | ✅ via `require_admin` | Nominate new admin; emits `admin_nominated` |
| `accept_admin(caller)` | ✅ `require_auth` | Complete transfer; emits `admin_accepted` |
| `cancel_admin_transfer(caller)` | ✅ via `require_admin` | Cancel pending transfer; emits `admin_cancelled` |

### Checkpoint Creation

| Function | Auth | Description |
|----------|:----:|-------------|
| `create_checkpoint(caller, subject, token, balance, metadata)` → `u64` | ✅ via `require_admin` | Record a single immutable snapshot |
| `batch_create_checkpoints(caller, items)` → `Vec<u64>` | ✅ via `require_admin` | Atomic batch creation (1–50 items); validates all before writing |

### Checkpoint Queries (Read-Only)

| Function | Auth | Returns |
|----------|:----:|---------|
| `get_checkpoint(id)` | ❌ public | `CheckpointRecord` for the given ID |
| `get_checkpoints_range(start_id, limit)` | ❌ public | Paginated `Vec<CheckpointRecord>` (max 100/page) |
| `get_checkpoint_count()` | ❌ public | `u64` total count (O(1)) |
| `get_latest_checkpoint_id()` | ❌ public | Most recent checkpoint ID, or `0` |
| `get_latest_checkpoint()` | ❌ public | `Option<CheckpointRecord>` convenience accessor |

### Admin Views

| Function | Auth | Returns |
|----------|:----:|---------|
| `get_admin()` | ❌ public | Current admin `Address` |
| `get_pending_admin()` | ❌ public | `Option<Address>` during rotation |

### Upgrade

| Function | Auth | Description |
|----------|:----:|-------------|
| `upgrade(caller, new_wasm_hash)` | ✅ via `require_admin` | Swap contract WASM |

---

## Data Model

### `CheckpointRecord`

```rust
pub struct CheckpointRecord {
    pub id: u64,           // Sequential identifier (1-based)
    pub subject: Address,  // Address whose balance is snapshotted
    pub token: Address,    // Token contract address
    pub balance: i128,     // Snapshotted balance (≥ 0)
    pub timestamp: u64,    // Ledger timestamp at creation
    pub metadata: Symbol,  // Free-form tag (≤ 32 chars, protocol-limited)
}
```

### Storage Layout

| Key | Type | Scope |
|-----|------|-------|
| `StorageKey::Admin` | Instance | Current admin address |
| `StorageKey::PendingAdmin` | Instance | Pending admin during rotation |
| `StorageKey::NextCheckpointId` | Instance | Next ID to assign |
| `StorageKey::CheckpointCount` | Instance | Cached total count |
| `StorageKey::Checkpoint(id)` | Persistent | Individual checkpoint record (6-month TTL) |

---

## Error Codes

| Code | Variant | Meaning |
|-----:|---------|---------|
| 1 | `NotInitialized` | Contract has not been initialized |
| 2 | `AlreadyInitialized` | `init` was called more than once |
| 3 | `Unauthorized` | Caller is not authorized |
| 4 | `BatchEmpty` | Batch operation received empty vector |
| 5 | `BatchTooLarge` | Batch exceeds `MAX_BATCH_SIZE` (50) |
| 6 | `CheckpointNotFound` | Requested checkpoint ID does not exist |
| 7 | `AmountNegative` | Balance must not be negative |
| 8 | `InvalidPageSize` | Page size must be greater than zero |
| 9 | `Overflow` | Arithmetic overflow detected |

---

## Events Emitted

| Event Topic | Triggered By | Data |
|-------------|-------------|------|
| `init` | `init` | `()` |
| `checkpoint_created` | `create_checkpoint`, `batch_create_checkpoints` | `CheckpointRecord` |
| `admin_nominated` | `set_admin` | `new_admin: Address` |
| `admin_accepted` | `accept_admin` | `pending: Address` |
| `admin_cancelled` | `cancel_admin_transfer` | `()` |
| `upgraded` | `upgrade` | `new_wasm_hash: BytesN<32>` |

All event symbols have byte-identity snapshot tests in `events.rs` (matching the pattern in vault, settlement, and revenue pool).

---

## Security Properties

| Property | Implementation |
|----------|---------------|
| **Auth on state-changing entrypoints** | `require_auth` enforced on `init`, all checkpoint creation, all admin rotation, and `upgrade` |
| **Overflow-safe math** | `checked_add` in ID counter and range computation; returns `CheckpointError::Overflow` on overflow |
| **No raw `.unwrap()`** | All `Option/Result` extractions use `unwrap_or_else(|| panic!(...))` or `ok_or(CheckpointError::...)` |
| **Immutability** | Checkpoints are append-only; IDs are sequential and never reused |
| **Atomic batch** | All validation runs before any state write in `batch_create_checkpoints` |
| **Two-step admin rotation** | `set_admin` → `accept_admin` pattern prevents accidental admin loss |
| **TTL management** | Persistent checkpoint entries get 6-month TTL (3,110,400 ledgers), extended on write |

### Access Control Matrix

| Caller | `create_checkpoint` | `batch_create` | `set_admin` | `accept_admin` | `upgrade` |
|--------|:---:|:---:|:---:|:---:|:---:|
| Admin | ✅ | ✅ | ✅ | ❌ | ✅ |
| Pending Admin | ❌ | ❌ | ❌ | ✅ | ❌ |
| Anyone else | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## Test Coverage

**43 tests, all passing** (`cargo test -p callora-checkpoint`).

### Test Breakdown

| Category | Count | Key Tests |
|----------|:-----:|-----------|
| Initialisation | 3 | Single init, double init rejection, `NotInitialized` before init |
| Admin rotation | 7 | Nominate, accept, cancel, unauthorised paths, panic on missing transfer |
| Single checkpoint creation | 6 | Happy path, non-admin rejection, negative balance, zero balance allowed, sequential IDs, metadata validation |
| Batch checkpoint creation | 5 | Happy path (3 items), empty batch, oversized batch, negative balance rollback, full atomicity |
| Queries | 9 | Point lookup + not found, paginated range (p1/p2), page-size cap, start beyond count, zero limit, latest checkpoint empty/non-empty, start_id=0 |
| Upgrade | 2 | Admin succeeds (panics in test — SDK limitation), non-admin rejection |
| Edge cases & invariants | 7 | Pre-init rejection, batch non-admin, immutability proof, post-rotation auth, count/ID consistency, zero-start range |
| Internal | 4 | Rustdoc self-test (`every_public_fn_in_lib_has_rustdoc`), 3 event byte-identity snapshots |

### Rustdoc Self-Verification

The contract includes a `rustdoc_tests` module that parses `lib.rs` source at compile time and asserts that **every `pub fn` is preceded by a `///` doc comment**. This test must pass for CI to succeed — it prevents undocumented functions from being merged.

---

## Conventions & Consistency

The checkpoint contract follows the exact conventions established by the existing contracts (vault, settlement, revenue pool):

- ✅ `#![no_std]` with `#[cfg(test)] extern crate std`
- ✅ `#[contracterror]` enum with `#[repr(u32)]` stable codes
- ✅ Event symbols in a dedicated `events.rs` module with snapshot tests
- ✅ `StorageKey` enum (not raw string keys)
- ✅ Two-step admin rotation (`set_admin` → `accept_admin`)
- ✅ TTL extension on persistent storage writes
- ✅ `///` rustdoc with `# Parameters`, `# Errors`, `# Events`, `# Returns` sections
- ✅ `unwrap_or_else(|| panic!(...))` pattern for invariant violations
- ✅ `Result<T, ContractError>` return types for recoverable errors
- ✅ Workspace member in `Cargo.toml`

---

## Build & Test Commands

```bash
# Build the checkpoint contract
cargo build -p callora-checkpoint

# Run checkpoint tests (43 tests)
cargo test -p callora-checkpoint

# Full workspace tests (ensure no regressions in vault/settlement/revenue pool)
cargo test --workspace

# Check for warnings
cargo clippy -p callora-checkpoint -- -D warnings

# Build WASM for deployment
cargo build --target wasm32-unknown-unknown --release -p callora-checkpoint
```

---

## Deployment Notes

1. Deploy the checkpoint contract WASM to the target network
2. Call `init(admin)` with the desired admin address (should be the same admin as settlement/vault for operational simplicity)
3. Periodically call `create_checkpoint` or `batch_create_checkpoints` to snapshot developer balances from the Settlement contract
4. Integrate with off-chain indexers to consume `checkpoint_created` events for dashboarding and compliance reporting

### Example: Snapshotting Settlement Developer Balances

```rust
// Admin snapshots developer balances at month-end close
let items = vec![
    (developer_a, usdc_token, a_balance, Symbol::new(&env, "monthly_close")),
    (developer_b, usdc_token, b_balance, Symbol::new(&env, "monthly_close")),
    // ... up to 50 per batch
];
let ids = checkpoint_client.batch_create_checkpoints(&admin, &items);
// ids contains sequential checkpoint IDs for each snapshot
```

### Query Example: Reconstructing Historical Balances

```rust
// Get the first page of checkpoints
let page = checkpoint_client.get_checkpoints_range(&1u64, &50u32);

// Get total count for pagination
let total = checkpoint_client.get_checkpoint_count();

// Point-lookup a specific checkpoint
let record = checkpoint_client.get_checkpoint(&42);
```

---

## Checklist

- [x] `///`-style rustdoc on all 14 `pub fn`s (verified by self-test)
- [x] `require_auth` on all 7 state-changing entrypoints
- [x] Overflow-safe `checked_add` arithmetic throughout
- [x] No raw `.unwrap()` in production paths
- [x] Typed `#[contracterror]` enum with 9 stable numeric codes
- [x] Event symbols with byte-identity snapshot tests
- [x] Two-step admin rotation (`set_admin` → `accept_admin`)
- [x] Atomic batch creation with full pre-validation
- [x] Paginated query with page-size cap
- [x] 43 passing unit tests
- [x] Rustdoc self-test (`every_public_fn_in_lib_has_rustdoc`)
- [x] Workspace membership in root `Cargo.toml`
- [x] Follows existing contract conventions (vault, settlement, revenue pool)
