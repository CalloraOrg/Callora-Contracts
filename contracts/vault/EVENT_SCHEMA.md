# Vault Upgrade Event Schema

This document describes the three events emitted by `upgrade()` in the Callora Vault contract,
their topic layout, data payloads, and guidance for indexers.

---

## Events emitted (in order, within the same transaction)

| # | Topic 0 | Topic 1 | Data type | Emitted when |
|---|---------|---------|-----------|--------------|
| 1 | `upgrade_started` | admin `Address` | `UpgradeStartedData` | Before `env.deployer().update_current_contract_wasm()` |
| 2 | `upgrade_completed` | admin `Address` | `UpgradeCompletedData` | After the WASM swap succeeds |
| 3 | `upgraded` | admin `Address` | `BytesN<32>` (new hash) | After the version marker is persisted (backward-compat) |

---

## `upgrade_started`

**Purpose:** signals upgrade intent. An `upgrade_started` without a subsequent `upgrade_completed`
in the same transaction indicates the WASM swap was never executed (e.g., authorization failure
or WASM validation error).

### Payload — `UpgradeStartedData`

```rust
#[contracttype]
pub struct UpgradeStartedData {
    pub new_wasm_hash: BytesN<32>,
    pub previous_version: Option<BytesN<32>>,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `new_wasm_hash` | `BytesN<32>` | WASM hash to be installed |
| `previous_version` | `Option<BytesN<32>>` | Hash from the previous upgrade, or `None` on first upgrade |

### JSON example

```json
{
  "topics": ["upgrade_started", "<admin-address>"],
  "data": {
    "new_wasm_hash": "aabbcc...3232",
    "previous_version": null
  }
}
```

---

## `upgrade_completed`

**Purpose:** confirms the WASM swap succeeded. The `new_wasm_hash` matches the value carried
in the corresponding `upgrade_started` event from the same transaction.

### Payload — `UpgradeCompletedData`

```rust
#[contracttype]
pub struct UpgradeCompletedData {
    pub new_wasm_hash: BytesN<32>,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `new_wasm_hash` | `BytesN<32>` | WASM hash that was successfully installed |

### JSON example

```json
{
  "topics": ["upgrade_completed", "<admin-address>"],
  "data": {
    "new_wasm_hash": "aabbcc...3232"
  }
}
```

---

## `upgraded` (backward compatibility)

Retained from before this PR. Emitted after `upgrade_completed` with the same `new_wasm_hash`
as a bare `BytesN<32>` in the data position. Indexers already consuming `upgraded` require
no changes.

---

## Indexer guidance

- **Event ordering guarantee:** `upgrade_started` always precedes `upgrade_completed` which always
  precedes `upgraded` within the same ledger close.
- **Detecting failed upgrades:** if `upgrade_started` appears without `upgrade_completed` in the
  same transaction, the upgrade was initiated but the WASM swap did not complete.
- **Version chain reconstruction:** collect all `upgrade_started` events and follow
  `previous_version` links to reconstruct the full upgrade history for a contract instance.
- **Deduplication:** correlate `upgrade_started` and `upgrade_completed` by matching
  `new_wasm_hash` within the same transaction hash.
