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
- **Primary Power**: Can call `set_authorized_caller`, `set_allowed_depositor`, and `transfer_ownership`.
- **Withdrawal**: Only the Owner (or the contract itself via the Admin) can withdraw funds.

### 2. Admin

The **Admin** role is designed for tactical system management. By default, the Admin is the Owner upon initialization.

- **Management**: The current Admin can transfer the role via `set_admin`.
- **Primary Power**: Can call `distribute` and `set_settlement`.
- **Use Case**: Used by settlement services or automated distribution logic.

### 3. Authorized Caller

An **Authorized Caller** is an account (typically a backend service or matching engine) permitted to call `deduct` or `batch_deduct`.

- **Management**: Set by the **Owner** via `set_authorized_caller`.
- **Permission**: Required to call deduction entrypoints.
- **Implicit Perrmission**: The **Owner** is always an implicit Authorized Caller.

### 4. Allowed Depositor

**Allowed Depositors** are a set of addresses permitted to call the `deposit` function.

- **Management**: Added/Removed by the **Owner** via `set_allowed_depositor`.
- **Permission**: If configured, only these addresses (and the **Owner**) can deposit funds.
- **Note**: If `AllowedDepositor` storage is empty, only the **Owner** can deposit.

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
| `set_allowed_depositor`            |  ✅   |   -   |         -         |         -         |
| `set_metadata` / `update_metadata` |  ✅   |   -   |         -         |         -         |
| `transfer_ownership`               |  ✅   |   -   |         -         |         -         |

---

## Role Lifecycle

### Ownership Transfer

The `transfer_ownership` function allows the current owner to hand over full control to a new address. This is a critical operation and should be done with caution.

### Admin Transition

The `set_admin` function allows the current admin (typically the owner initially) to delegate operational control (like settlement and distribution) to a dedicated service account.
