# Access Control

## 1. Vault Access Control

### Overview
The Callora Vault implements role-based access control for deposit operations to ensure only authorized parties can increase the vault balance.

### Roles
- **Owner**: Set during contract initialization. Exclusive authority to manage allowed depositors, withdraw funds, and propose revenue pool changes.
- **Allowed Depositor**: Addresses approved by the owner to handle automated deposits.
- **Authorized Caller**: Optional address permitted to trigger `deduct` operations.
- **Pending Owner**: Nominee awaiting acceptance of the owner role.
- **Pending Admin**: Nominee awaiting acceptance of the admin role.
- **Pending Revenue Pool**: Proposed revenue pool address awaiting acceptance.

### Authorization Matrix

| Function | Owner | Allowed Depositor | Authorized Caller | Pending Owner | Others |
|----------|-------|-------------------|-------------------|---------------|--------|
| `deposit` | ✅ | ✅ | ❌ | ❌ | ❌ |
| `withdraw` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `withdraw_to` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `deduct` | ❌ | ❌ | ✅ | ❌ | ❌ |
| `batch_deduct` | ❌ | ❌ | ✅ | ❌ | ❌ |
| `set_allowed_depositor` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `clear_allowed_depositors` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `set_authorized_caller` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `transfer_ownership` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `accept_ownership` | ❌ | ❌ | ❌ | ✅ | ❌ |
| `cancel_ownership_transfer` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `set_admin` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `accept_admin` | ❌ | ❌ | ❌ | ❌ | ✅ |
| `cancel_admin_transfer` | ❌ | ❌ | ❌ | ❌ | ✅ |
| `propose_revenue_pool` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `accept_revenue_pool` | ❌ | ❌ | ❌ | ❌ | ✅ |
| `cancel_revenue_pool` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `pause` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `unpause` | ✅ | ❌ | ❌ | ❌ | ❌ |

### Security Model
- **Two-Step Owner Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Two-Step Admin Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Cancellation Safety**: Provides `cancel_ownership_transfer` and `cancel_admin_transfer` functions to abort mistaken nominations before acceptance.
- **Restricted Depositors**: Only owner and explicitly allowed depositors can increase vault balance.
- **Nonce-Bound Authorized-Caller Rotation**: `set_authorized_caller` requires the caller to supply the current monotonic nonce (see below), preventing a leaked owner signature from being replayed to reinstate a stale `authorized_caller`.

### Authorized-Caller Replay Protection

`set_authorized_caller` maintains a monotonic `u64` nonce stored under
`StorageKey::AuthorizedCallerNonce` in instance storage.

| Step | Who | Action |
|------|-----|--------|
| 1 | Integrator | Call `get_authorized_caller_nonce()` to read the current nonce (defaults to `0`). |
| 2 | Owner | Call `set_authorized_caller(new_caller, expected_nonce)` with the value from step 1. |
| 3 | Contract | Verifies `expected_nonce == stored_nonce`; rejects with `VaultError::StaleNonce` if not. |
| 4 | Contract | Increments the stored nonce (`wrapping_add(1)`) and emits it in the event payload. |

**Replay resistance**: a captured owner signature contains a fixed `expected_nonce`.
After one successful rotation the stored nonce advances, so the captured signature is
permanently invalid.

**Event payload**: the `set_authorized_caller` event now carries
`(old_caller, new_caller, consumed_nonce)` as data, allowing off-chain indexers to
detect nonce gaps.

**Nonce wrap**: the nonce wraps to `0` after `u64::MAX` rotations (2^64 calls) — a
practical impossibility, but handled safely by `wrapping_add`.

### Cancellation Functions

#### cancel_ownership_transfer
Allows the current owner to cancel a pending ownership transfer before the nominee accepts it. This provides a safety mechanism to abort mistaken nominations.

**Access Control**: Only the current owner can call this function.
**Behavior**: 
- Removes the `PendingOwner` from storage
- Emits `ownership_cancelled` event with current owner and cancelled nominee
- Panics with "no ownership transfer pending" if no transfer is pending

#### cancel_admin_transfer
Allows the current admin to cancel a pending admin transfer before the nominee accepts it. This provides a safety mechanism to abort mistaken nominations.

**Access Control**: Only the current admin can call this function.
**Behavior**: 
- Removes the `PendingAdmin` from storage
- Emits `admin_cancelled` event with current admin and cancelled nominee
- Panics with "no admin transfer pending" if no transfer is pending

---

## 2. Settlement Access Control

### Overview
The Callora Settlement contract tracks individual developer balances and global protocol revenue. It enforces strict access control for incoming payments and administrative updates.

Developer address compliance recoveries use `propose_balance_migration` and
`execute_balance_migration`. Both calls require authorization by the current
admin address, including its native Stellar multisig thresholds, and execution
is delayed by 24 hours. See [Admin developer balance migration](ADMIN_BALANCE_MIGRATION.md).

### Roles
- **Admin**: Primary authority over contract configuration and sensitive data.
- **Vault**: The registered vault contract authorized to send payments.
- **Pending Admin**: Nominee awaiting acceptance of the admin role.
- **Pending Vault**: Proposed vault awaiting acceptance.

### Authorization Matrix

| Function | Admin | Vault | Pending Admin | Others |
|----------|-------|-------|---------------|--------|
| `receive_payment` | ✅ | ✅ | ❌ | ❌ |
| `set_admin` | ✅ | ❌ | ❌ | ❌ |
| `accept_admin` | ❌ | ❌ | ✅ | ❌ |
| `cancel_admin_transfer` | ✅ | ❌ | ❌ | ❌ |
| `propose_vault` | ✅ | ❌ | ❌ | ❌ |
| `accept_vault` | ✅ | ✅ | ❌ | ❌ |
| `set_vault` (alias of `propose_vault`) | ✅ | ❌ | ❌ | ❌ |
| `get_all_developer_balances` | ✅ | ❌ | ❌ | ❌ |

### Security Model
- **Two-Step Admin Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Two-Step Vault Rotation**: Prevents accidentally misrouting settlement credits by requiring the proposed vault to accept (or the admin to finalize).
- **Restricted Views**: Sensitive batch queries like `get_all_developer_balances` are restricted to the admin to prevent unnecessary exposure of the full ledger via the contract interface.
- **Cancellation Safety**: The admin can invoke `cancel_admin_transfer` to clear a mistaken nomination.

---

## 3. Revenue Pool Access Control

### Overview
The Callora Revenue Pool contract processes USDC distribution to developer wallets. Like Settlement and Vault, it implements standard administrative roles and rotation procedures.

### Roles
- **Admin**: Handles revenue distributions and nominates administrative successions.
- **Pending Admin**: A nominated account that has to explicitly accept the role to become the Admin.
- **Pause Guardian**: Optional emergency role that may pause the revenue pool without receiving any distribution, unpause, upgrade, or admin-management authority.

### Authorization Matrix

| Function | Admin | Pending Admin | Pause Guardian | Others |
|----------|-------|---------------|----------------|--------|
| `distribute` | ✅ | ❌ | ❌ | ❌ |
| `batch_distribute` | ✅ | ❌ | ❌ | ❌ |
| `pause` | ✅ | ❌ | ✅ | ❌ |
| `unpause` | ✅ | ❌ | ❌ | ❌ |
| `set_pause_guardian` | ✅ | ❌ | ❌ | ❌ |
| `clear_pause_guardian` | ✅ | ❌ | ❌ | ❌ |
| `set_admin` | ✅ | ❌ | ❌ | ❌ |
| `accept_admin` | ❌ | ✅ | ❌ | ❌ |
| `claim_admin` (alias of `accept_admin`) | ❌ | ✅ | ❌ | ❌ |
| `cancel_admin_transfer` | ✅ | ❌ | ❌ | ❌ |

### Cancellation Safety
The current admin can call `cancel_admin_transfer` to abort a pending admin nomination.

### Pause Guardian Safety
The current admin can call `set_pause_guardian` to delegate emergency pause authority to a narrow role, and `clear_pause_guardian` to remove that role. The pause guardian can only call `pause`; it cannot unpause, distribute funds, rotate admin, change caps, clear or replace itself, or upgrade the contract.

---

## Test Coverage
The implementation includes comprehensive tests covering:
- ✅ `set_authorized_caller` default nonce is `0` before first rotation
- ✅ First rotation with nonce `0` succeeds and advances stored nonce to `1`
- ✅ Replaying a consumed nonce is rejected with `VaultError::StaleNonce`
- ✅ Supplying a future nonce is rejected with `VaultError::StaleNonce`
- ✅ Three sequential rotations each advance the nonce correctly
- ✅ Nonce wraps at `u64::MAX` via `wrapping_add`
- ✅ Failed rotations do not advance the stored nonce
- ✅ Successful rotation emits `(old, new, consumed_nonce)` in the event payload
- ✅ Vault self-address is rejected as `new_caller`
- ✅ Admin and Vault can call `receive_payment`
- ✅ Unauthorized callers are rejected from `receive_payment`
- ✅ Only Admin can call `set_admin` and `propose_vault` (and the `set_vault` alias)
- ✅ Only Admin or Pending Vault can call `accept_vault`
- ✅ Only Pending Admin can call `accept_admin`
- ✅ Only Admin can call `get_all_developer_balances`
- ✅ All rotation and update logic preserves state integrity
- ✅ Only current owner can call `cancel_ownership_transfer`
- ✅ Only current admin can call `cancel_admin_transfer` in Vault, Settlement, and Revenue Pool
- ✅ Cancel functions clear pending state and emit events
- ✅ Cancel functions fail when no transfer is pending
- ✅ Cancel functions fail for unauthorized callers
- ✅ After cancellation, new nominations can be made

Run tests with:
```bash
cargo build --workspace --release --target=wasm32-unknown-unknown
```
