# Vault Access Control

## Overview

The Callora Vault implements role-based access control for deposit operations to ensure only authorized parties can increase the vault balance.

## Roles

### Owner
- Set during contract initialization via `init()`
- Immutable after initialization
- Always permitted to deposit
- Exclusive authority to manage the allowed depositor
- Typically represents the end user's account in production

### Allowed Depositor
- One or more addresses explicitly approved by the owner via `set_allowed_depositor()`
- Mutable — addresses can be added or the entire list cleared at any time by the owner
- Commonly used for backend service hot-wallets that handle automated deposits
- When added, each address has the same deposit privileges as the owner
- The full allowlist is publicly auditable via `get_allowed_depositors()`

### Unauthorized Addresses
- Any address that is neither the owner nor the allowed depositor
- Deposit attempts are rejected with: `"unauthorized: only owner or allowed depositor can deposit"`

## Production Usage

In a typical production deployment:

1. **User Account (Owner)**: The end user's wallet address is set as the owner during initialization
2. **Backend Service (Allowed Depositor)**: A trusted backend service address is set as the allowed depositor to handle automated deposits on behalf of users
3. **Access Control**: Only these two addresses can increase the vault balance

## Managing the Allowed Depositor

### Setting or Updating
```rust
// Owner sets the allowed depositor
vault.set_allowed_depositor(owner_address, Some(backend_service_address));
```

### Clearing (Revoking Access)
```rust
// Owner revokes depositor access
vault.set_allowed_depositor(owner_address, None);
```

### Rotating the Depositor
```rust
// Owner can change the allowed depositor at any time
vault.set_allowed_depositor(owner_address, Some(new_backend_address));
```

## Security Model

### Trust Assumptions
- The owner has full control over deposit permissions
- The allowed depositor is a trusted address (typically a backend service under the owner's control)
- Access can be revoked instantly by the owner at any time

### Authorization Flow
1. Caller invokes `deposit()` with their address
2. Contract verifies caller is either:
   - The owner (always authorized), OR
   - The currently set allowed depositor (if any)
3. If neither condition is met, the transaction fails with an authorization error

### Best Practices
- Rotate the allowed depositor address periodically for security
- Clear the allowed depositor when not actively needed
- Monitor deposit events to detect unauthorized access attempts
- Use secure key management for both owner and depositor addresses

## API Reference

### `set_allowed_depositor(caller: Address, depositor: Option<Address>)`
Owner-only function to manage the allowed depositor.

**Parameters:**
- `caller`: Must be the owner address (authenticated via `require_auth()`)
- `depositor`: 
  - `Some(address)` - Sets or updates the allowed depositor
  - `None` - Clears the allowed depositor (revokes access)

**Errors:**
- Panics with `"unauthorized: owner only"` if caller is not the owner

### `deposit(caller: Address, amount: i128) -> i128`
Increases the vault balance by the specified amount.

**Parameters:**
- `caller`: Must be either the owner or allowed depositor (authenticated via `require_auth()`)
- `amount`: Amount to add to the balance

**Returns:**
- The new balance after deposit

**Errors:**
- Panics with `"unauthorized: only owner or allowed depositor can deposit"` if caller is not authorized

## Test Coverage

The implementation includes comprehensive tests covering:
- ✅ Owner can deposit successfully
- ✅ Allowed depositor can deposit successfully
- ✅ Unauthorized addresses cannot deposit (expect auth error)
- ✅ Owner can set and clear allowed depositor
- ✅ Non-owner cannot call `set_allowed_depositor`
- ✅ Deposit after allowed depositor is cleared is rejected
- ✅ All existing tests continue to pass

Run tests with:
```bash
cargo test --manifest-path contracts/vault/Cargo.toml
```
