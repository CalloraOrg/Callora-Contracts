# BatchClaim Contract Storage Documentation

This document outlines the storage keys used by the BatchClaim contract, their storage tiers (Instance, Persistent, or Temporary), and the rationale behind each choice in accordance with Soroban best practices.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., reentrancy guards, short-lived scratchpads, idempotency markers for batch operations).

---

## Storage Keys

### 1. `Admin`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The administrator address governs access control for all privileged operations (pausing, asset configuration, emergency actions). As a core configuration parameter that must survive as long as the contract, it shares the contract instance lifecycle. Instance storage guarantees the admin cannot be silently archived, which would permanently lock the contract.

### 2. `PendingAdmin`
- **Tier**: **Instance**
- **Data Type**: `Address` (Optional / Nullable)
- **Rationale**: Holds the nominated successor during a secure two-step admin transfer. This is transient governance state tied to the contract's administration lifecycle; instance storage is appropriate because the value is meaningful only while a transfer is pending and should share the contract instance TTL.

### 3. `Paused`
- **Tier**: **Instance**
- **Data Type**: `bool`
- **Rationale**: Circuit-breaker flag that gates all batch-claim operations during emergencies. It must be globally accessible and share the contract instance TTL so that a paused state is never archived while the contract remains active.

### 4. `Asset`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The token contract address (e.g., USDC) that claimants receive. This is a one-time configuration parameter set during initialization and must remain available for the entire contract lifetime. Instance storage ensures it is never dropped.

### 5. `MaxBatchSize`
- **Tier**: **Instance**
- **Data Type**: `u32`
- **Rationale**: The maximum number of claims that can be processed in a single `batch_claim` invocation. This is an administrative configuration parameter that limits per-transaction resource usage. Stored in instance storage because it is a global, infrequently changed setting.

### 6. `ClaimWindow`
- **Tier**: **Instance**
- **Data Type**: `Option<ClaimWindow>`

  ```rust
  struct ClaimWindow {
      pub opens_at: u64,  // Ledger timestamp when claims become active
      pub closes_at: u64, // Ledger timestamp after which claims are blocked
  }
  ```

- **Rationale**: Defines the time window during which claims may be executed. When `None`, claims are always permitted (subject to other guards). This is a global configuration value that controls contract behaviour for all users, making instance storage the correct tier.

### 7. `Claim(Address, u64)`
- **Tier**: **Persistent**
- **Data Type**: `ClaimData`

  ```rust
  struct ClaimData {
      pub amount: i128,
      pub claimed: bool,
  }
  ```

- **Rationale**: Per-user claim records indexed by a sequential claim ID. Each entry represents a distributable amount allocated to a specific address. `Persistent` storage is required because:
  - Claims are user-specific and can number in the thousands; instance storage cannot accommodate variable-sized user data.
  - Claim records must persist until explicitly claimed or administratively revoked.
  - Each entry has an independent lifecycle and TTL, allowing inactive records to archive naturally without affecting active ones.

### 8. `UserCursor(Address)`
- **Tier**: **Persistent**
- **Data Type**: `Cursor`

  ```rust
  struct Cursor {
      pub tail: u64, // Oldest unclaimed index
      pub head: u64, // Next index to insert
  }
  ```

- **Rationale**: Tracks the FIFO range of claim indices for each user. This enables efficient batch iteration without scanning the entire key space. Persistent storage is appropriate because cursor state is user-specific, updated on every batch claim, and must survive independently of the contract instance TTL.

### 9. `ProcessedBatchNonce(Symbol)`
- **Tier**: **Temporary**
- **Data Type**: `bool`
- **Rationale**: Idempotency marker that prevents replay of a previously processed batch nonce. Temporary storage is chosen because:
  - The marker is meaningful only for a short window (the TTL of the underlying claim data).
  - Using Temporary storage reduces state bloat and protocol fees since the network can naturally drop the marker after its TTL expires.
  - If a nonce is replayed after the marker has been archived, the underlying claims would already be marked `claimed: true`, providing a secondary defence against double-claims.

### 10. `TotalClaimsProcessed`
- **Tier**: **Instance**
- **Data Type**: `u64`
- **Rationale**: A monotonically increasing global counter of successfully processed claims. Used for external indexing and analytics. Stored in instance storage because it is a single global value updated on every successfully processed claim, and bumping the instance TTL on each write prevents the counter from archiving.

### 11. `ContractVersion`
- **Tier**: **Instance**
- **Data Type**: `BytesN<32>`
- **Rationale**: Records the WASM hash after a successful contract upgrade. This enables off-chain verifiers to confirm the deployed bytecode. Instance storage is appropriate because it is global configuration that should persist as long as the contract instance.

## Storage Tier Summary

| Key                    | Tier        | Category       |
|------------------------|-------------|----------------|
| `Admin`                | Instance    | Governance     |
| `PendingAdmin`         | Instance    | Governance     |
| `Paused`               | Instance    | Circuit-breaker|
| `Asset`                | Instance    | Configuration  |
| `MaxBatchSize`         | Instance    | Configuration  |
| `ClaimWindow`          | Instance    | Configuration  |
| `Claim(Address, u64)`  | Persistent  | User Data      |
| `UserCursor(Address)`  | Persistent  | User Data      |
| `ProcessedBatchNonce(Symbol)` | Temporary | Idempotency |
| `TotalClaimsProcessed` | Instance    | Metrics        |
| `ContractVersion`      | Instance    | Upgradeability |

## TTL Configuration

The following TTL constants should be defined for the BatchClaim contract, following the patterns established by other contracts in this repository:

- **Instance TTL threshold**: `17_280` ledgers (~1 day at 5 s/ledger)
- **Instance TTL amount**: `518_400` ledgers (~30 days)
- **Persistent TTL threshold**: `17_280` ledgers (~1 day)
- **Persistent TTL amount**: `518_400` ledgers (~30 days)
- **Temporary TTL threshold**: `1_000` ledgers (~1.5 hours)
- **Temporary TTL amount**: `10_000` ledgers (~16 hours)
