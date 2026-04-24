# Access Control Model: Callora Vault

The Callora Vault uses a multi-role access control model to ensure security while maintaining flexibility for integrators and automated services.

## Roles Overview

| Role                  | Responsibility                                        | Scope                  |
| --------------------- | ----------------------------------------------------- | ---------------------- |
| **Owner**             | Full vault management and ownership transfer.         | Multi-Vault Management |
| **Admin**             | System-wide settings and fund distribution.           | Tactical Operations    |
| **Authorized Caller** | Permissions to deduct funds (e.g., matching engines). | Execution Services     |
| **Allowed Depositor** | Permission to deposit funds into the vault.           | Funding Services       |

---

## Role Details

### 1. Owner

The **Owner** has the highest level of privilege. They are responsible for the metadata, ownership transfers, and managing lower-level permissions.

- **Initialization**: Set during `init`.
- **Primary Power**: Can call `set_authorized_caller`, `add_address`, `clear_all`, and `transfer_ownership`.
- **Withdrawal**: Only the Owner (or the contract itself via the Admin) can withdraw funds.
- **Deposit**: The Owner is always permitted to deposit, regardless of the allowlist state.

### 2. Admin

The **Admin** role is designed for tactical system management. By default, the Admin is the Owner upon initialization.

- **Management**: The current Admin can transfer the role via `set_admin`.
- **Primary Power**: Can call `distribute` and `set_settlement`.
- **Use Case**: Used by settlement services or automated distribution logic.

### 3. Authorized Caller

An **Authorized Caller** is an account (typically a backend service or matching engine) permitted to call `deduct` or `batch_deduct`.

- **Management**: Set by the **Owner** via `set_authorized_caller`.
- **Permission**: Required to call deduction entrypoints.
- **Implicit Permission**: The **Owner** is always an implicit Authorized Caller.

### 4. Allowed Depositor

**Allowed Depositors** are a set of addresses permitted to call the `deposit` function.

- **Management**: Added by the **Owner** via `add_address`, removed via `clear_all`.
- **Permission**: If configured, only these addresses (and the **Owner**) can deposit funds.
- **Note**: If the allowlist is empty, only the **Owner** can deposit.
- **Duplicate Prevention**: The system automatically prevents duplicate entries in the allowlist.

---

## Allowlist Management

The vault implements a robust allowlist system for controlling deposit permissions:

### Adding Addresses

Use `add_address(caller: Address, address: Address)` to add a single address to the allowlist:

```rust
// Owner adds a backend service to the allowlist
vault.add_address(&owner, &backend_service_address);
```

- **Access Control**: Only the Owner can add addresses
- **Duplicate Prevention**: Attempting to add an address already in the allowlist is a no-op
- **Event Emission**: Emits `("allowlist_add", owner, address)` on success

### Clearing the Allowlist

Use `clear_all(caller: Address)` to remove all addresses from the allowlist:

```rust
// Owner clears all allowed depositors
vault.clear_all(&owner);
```

- **Access Control**: Only the Owner can clear the allowlist
- **Idempotent**: Safe to call multiple times, even when the allowlist is already empty
- **Event Emission**: Emits `("allowlist_clear", owner)` on success
- **Owner Unaffected**: The Owner can still deposit after clearing the allowlist

### Querying the Allowlist

Use `get_allowlist()` to retrieve the current list of allowed depositors:

```rust
// Get all allowed depositors
let allowed: Vec<Address> = vault.get_allowlist();
```

- **Public Access**: Anyone can query the allowlist
- **Returns**: A vector of addresses; empty if no depositors are configured

### Backward Compatibility

The legacy `set_allowed_depositor(caller: Address, depositor: Option<Address>)` function is maintained for backward compatibility:

- `Some(address)`: Adds the address to the allowlist (same as `add_address`)
- `None`: Clears the entire allowlist (same as `clear_all`)

**Recommendation**: New integrations should use `add_address` and `clear_all` for clarity.

---

## Permission Matrix

| Function                           | Owner | Admin | Authorized Caller | Allowed Depositor |
| ---------------------------------- | :---: | :---: | :---------------: | :---------------: |
| `deposit`                          |  ✅   |   -   |         -         |        ✅         |
| `withdraw` / `withdraw_to`         |  ✅   |   -   |         -         |         -         |
| `deduct` / `batch_deduct`          |  ✅   |   -   |        ✅         |         -         |
| `distribute`                       |   -   |  ✅   |         -         |         -         |
| `set_settlement`                   |   -   |  ✅   |         -         |         -         |
| `set_admin`                        |   -   |  ✅   |         -         |         -         |
| `set_authorized_caller`            |  ✅   |   -   |         -         |         -         |
| `add_address`                      |  ✅   |   -   |         -         |         -         |
| `clear_all`                        |  ✅   |   -   |         -         |         -         |
| `get_allowlist`                    |  ✅   |  ✅   |        ✅         |        ✅         |
| `set_allowed_depositor` (legacy)   |  ✅   |   -   |         -         |         -         |
| `set_metadata` / `update_metadata` |  ✅   |   -   |         -         |         -         |
| `transfer_ownership`               |  ✅   |   -   |         -         |         -         |

---

## Trust Assumptions

### Backend Services as Depositors

In production deployments, the allowlist typically contains backend service addresses that handle automated deposits on behalf of end users. This model assumes:

1. **Backend Service Security**: The backend service's private keys are securely managed (HSM, KMS, or secure enclave).
2. **Service Authentication**: The backend service properly authenticates end users before initiating deposits on their behalf.
3. **Audit Trail**: The backend service maintains an off-chain audit trail linking deposits to specific user actions.
4. **Owner Oversight**: The vault owner can revoke backend service access at any time via `clear_all` or by removing specific addresses.

### Trust Boundaries

| Actor           | Trusted For                                                                 |
| --------------- | --------------------------------------------------------------------------- |
| Owner           | Full vault control, allowlist management, ownership transfer                |
| Admin           | Settlement configuration, fund distribution to developers                   |
| Backend Service | Authenticated deposit operations on behalf of end users                     |
| End User        | Indirect deposit via backend service (not directly on-chain)                |

### Security Considerations

1. **Owner Key Compromise**: If the owner's private key is compromised, an attacker can:
   - Add malicious addresses to the allowlist
   - Clear the allowlist to deny service
   - Transfer ownership to themselves
   - Withdraw all funds

   **Mitigation**: Use a hardware wallet or multisig for the owner role in production.

2. **Backend Service Compromise**: If a backend service's private key is compromised, an attacker can:
   - Deposit funds into the vault (limited impact, as this benefits the vault)
   - Cannot withdraw funds (only owner can withdraw)

   **Mitigation**: Rotate backend service keys regularly and monitor deposit patterns.

3. **Allowlist Audit**: The allowlist is stored in persistent storage and can be audited at any time via `get_allowlist`. Off-chain monitoring should alert on unexpected changes.

---

## Role Lifecycle

### Ownership Transfer

The `transfer_ownership` function allows the current owner to hand over full control to a new address. This is a critical operation and should be done with caution.

```rust
// Transfer ownership to a new address
vault.transfer_ownership(&new_owner);
```

- **Irreversible**: The old owner loses all privileges immediately
- **Event Emission**: Emits `("transfer_ownership", old_owner, new_owner)`
- **Allowlist Preserved**: The allowlist remains unchanged after ownership transfer

### Admin Transition

The `set_admin` function allows the current admin (typically the owner initially) to delegate operational control (like settlement and distribution) to a dedicated service account.

```rust
// Delegate admin role to a service account
vault.set_admin(&current_admin, &new_admin);
```

- **Operational Separation**: Allows separation of tactical operations from strategic control
- **Revocable**: The new admin can transfer the role back or to another address

### Allowlist Lifecycle

1. **Initialization**: Vault starts with an empty allowlist (only owner can deposit)
2. **Addition**: Owner adds backend service addresses via `add_address`
3. **Operation**: Backend services deposit on behalf of users
4. **Rotation**: Owner can clear and rebuild the allowlist as needed
5. **Revocation**: Owner can remove all access via `clear_all` in case of compromise

---

## Example Workflows

### Adding a Backend Service

```rust
// 1. Owner initializes vault
vault.init(&owner, &usdc_token, &None, &None, &None, &None, &None);

// 2. Owner adds backend service to allowlist
vault.add_address(&owner, &backend_service);

// 3. Backend service can now deposit
vault.deposit(&backend_service, &amount);
```

### Rotating Backend Services

```rust
// 1. Add new backend service
vault.add_address(&owner, &new_backend_service);

// 2. Verify new service works
vault.deposit(&new_backend_service, &test_amount);

// 3. Clear all and re-add only the new service
vault.clear_all(&owner);
vault.add_address(&owner, &new_backend_service);
```

### Emergency Revocation

```rust
// In case of backend service compromise:
vault.clear_all(&owner);

// Only owner can deposit until new services are added
vault.deposit(&owner, &amount);
```

---

## Audit and Compliance

### On-Chain Auditability

All allowlist operations emit events that can be indexed and monitored:

- `("allowlist_add", owner, address)`: Address added to allowlist
- `("allowlist_clear", owner)`: Allowlist cleared
- `("deposit", caller, amount)`: Deposit made by caller

### Off-Chain Monitoring

Recommended monitoring alerts:

1. **Allowlist Changes**: Alert when `allowlist_add` or `allowlist_clear` events are emitted
2. **Unexpected Depositors**: Alert if a deposit comes from an address not in the allowlist (should be impossible)
3. **High-Frequency Additions**: Alert if multiple addresses are added in a short time window
4. **Ownership Transfer**: Alert immediately on `transfer_ownership` events

### Compliance Considerations

For regulated deployments:

1. **KYC/AML**: Backend services should perform KYC/AML checks before depositing on behalf of users
2. **Audit Trail**: Maintain off-chain records linking on-chain deposits to specific user identities
3. **Access Logs**: Log all allowlist management operations with timestamps and operator identities
4. **Periodic Review**: Regularly review the allowlist and remove unused backend services
