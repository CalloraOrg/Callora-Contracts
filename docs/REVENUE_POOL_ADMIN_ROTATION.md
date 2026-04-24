# Revenue Pool Admin Rotation Procedures
 
This document outlines the operational procedures for rotating the admin address of the Callora Revenue Pool contract.
 
## Overview
 
The Revenue Pool implements a **two-step admin transfer** process to maximize security and prevent accidental loss of administrative control. A transfer must be initiated by the current admin and then accepted by the nominated successor.
 
## Roles
 
- **Current Admin**: The address currently holding administrative privileges.
- **Pending Admin**: The address nominated by the current admin to take over.
 
## Rotation Process
 
### Step 1: Nomination
 
The current admin initiates the transfer by calling `set_admin` with the address of the proposed successor.
 
```rust
pool.set_admin(current_admin_address, proposed_new_admin_address);
```
 
- **Action**: Sets the `PENDING_ADMIN` storage key.
- **Auth**: Requires signature from `current_admin`.
- **Event**: Emits `admin_transfer_started(current_admin, pending_admin)`.
- **State**: The current admin retains all privileges until Step 2 is completed.
 
### Step 2: Acceptance
 
The nominated successor must explicitly claim the admin role by calling `claim_admin`.
 
```rust
pool.claim_admin(proposed_new_admin_address);
```
 
- **Action**: Updates `ADMIN` to the caller's address and clears `PENDING_ADMIN`.
- **Auth**: Requires signature from the `proposed_new_admin`.
- **Event**: Emits `admin_transfer_completed(new_admin)`.
- **State**: Administrative control is fully transferred to the new address.
 
## Security Considerations
 
- **No Accidental Lockouts**: If an incorrect address is nominated in Step 1, the transfer will never be completed because only the nominated address can claim the role. The current admin can overwrite the nomination at any time by calling `set_admin` again with a different address.
- **Audit Trails**: Both steps of the rotation are explicitly logged via contract events, providing a clear audit trail for operations and indexers.
- **Immediate Effect**: Once `claim_admin` succeeds, the old admin immediately loses all administrative privileges.
 
## Emergency Procedures
 
If the current admin keys are lost before a transfer is initiated, the contract administrative functions will be permanently locked. It is recommended to use a multi-signature wallet or a hardware security module (HSM) for the admin role in production environments.
