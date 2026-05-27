# Access Control

## 1. Vault Access Control

### Overview
The Callora Vault implements role-based access control for deposit operations to ensure only authorized parties can increase the vault balance.

### Roles
- **Owner**: Set during contract initialization. Exclusive authority to manage allowed depositors and withdraw funds.
- **Allowed Depositor**: Addresses approved by the owner to handle automated deposits.
- **Authorized Caller**: Optional address permitted to trigger `deduct` operations.
- **Pending Owner**: Nominee awaiting acceptance of the owner role.
- **Pending Admin**: Nominee awaiting acceptance of the admin role.

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
| `pause` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `unpause` | ✅ | ❌ | ❌ | ❌ | ❌ |

### Security Model
- **Two-Step Owner Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Two-Step Admin Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Cancellation Safety**: Provides `cancel_ownership_transfer` and `cancel_admin_transfer` functions to abort mistaken nominations before acceptance.
- **Restricted Depositors**: Only owner and explicitly allowed depositors can increase vault balance.

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

### Roles
- **Admin**: Primary authority over contract configuration and sensitive data.
- **Vault**: The registered vault contract authorized to send payments.
- **Pending Admin**: Nominee awaiting acceptance of the admin role.

### Authorization Matrix

| Function | Admin | Vault | Pending Admin | Others |
|----------|-------|-------|---------------|--------|
| `receive_payment` | ✅ | ✅ | ❌ | ❌ |
| `set_admin` | ✅ | ❌ | ❌ | ❌ |
| `accept_admin` | ❌ | ❌ | ✅ | ❌ |
| `set_vault` | ✅ | ❌ | ❌ | ❌ |
| `get_all_developer_balances` | ✅ | ❌ | ❌ | ❌ |

### Security Model
- **Two-Step Admin Rotation**: Prevents accidental loss of control by requiring the nominee to explicitly accept the role.
- **Restricted Views**: Sensitive batch queries like `get_all_developer_balances` are restricted to the admin to prevent unnecessary exposure of the full ledger via the contract interface.

## Test Coverage
The implementation includes comprehensive tests covering:
- ✅ Admin and Vault can call `receive_payment`
- ✅ Unauthorized callers are rejected from `receive_payment`
- ✅ Only Admin can call `set_admin` and `set_vault`
- ✅ Only Pending Admin can call `accept_admin`
- ✅ Only Admin can call `get_all_developer_balances`
- ✅ All rotation and update logic preserves state integrity
- ✅ Only current owner can call `cancel_ownership_transfer`
- ✅ Only current admin can call `cancel_admin_transfer`
- ✅ Cancel functions clear pending state and emit events
- ✅ Cancel functions fail when no transfer is pending
- ✅ Cancel functions fail for unauthorized callers
- ✅ After cancellation, new nominations can be made

Run tests with:
```bash
cargo test -p callora-settlement
cargo test -p callora-vault
```
