# Access Control

## 1. Vault Access Control

### Overview
The Callora Vault implements role-based access control for deposit operations to ensure only authorized parties can increase the vault balance.

### Roles
- **Owner**: Set during contract initialization. Exclusive authority to manage allowed depositors and withdraw funds.
- **Allowed Depositor**: Addresses approved by the owner to handle automated deposits.
- **Authorized Caller**: Optional address permitted to trigger `deduct` operations.

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

Run tests with:
```bash
cargo test -p callora-settlement
```
