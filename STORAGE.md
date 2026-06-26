# Soroban Storage TTL and Archival Documentation

This document summarizes the storage TTL parameters, extension logic, and archival behaviours for the three core smart contracts in the Callora workspace.

The Soroban host enforces storage TTL limits to manage network state size. If an entry is not accessed (and its TTL extended) within its defined lifecycle window, it is archived. In Soroban SDK v22 test environments, this archival surfaces as a panic (`Error(Storage, InternalError)`) when the archived entry is subsequently accessed.

## 1. Vault Contract

The Vault contract relies heavily on TTL extensions on the write path to prevent instance archival.

### Constants
- `INSTANCE_BUMP_THRESHOLD`: `518,400` ledgers (~30 days at 5s/ledger).
- `INSTANCE_BUMP_AMOUNT`: `1,036,800` ledgers (~60 days at 5s/ledger).

### Behaviour
- **Reads (`balance()`)**: Do NOT trigger `extend_ttl`. If the vault is inactive for `INSTANCE_BUMP_AMOUNT` ledgers, reads will fail because the instance is archived.
- **Writes (`deposit()`, `deduct()`, `init()`)**: Call `env.storage().instance().extend_ttl()`. If these are called within the active TTL window, the window is reset forward by `INSTANCE_BUMP_AMOUNT`, preventing archival.
- **Conclusion**: The vault must experience a write operation at least once every 60 days to prevent its instance storage from being archived.

## 2. Revenue Pool Contract

The Revenue Pool uses a shorter TTL window but similar write-path extension logic.

### Constants
- `BUMP_AMOUNT`: `10,000` ledgers (~16 days at 5s/ledger).
- `LIFETIME_THRESHOLD`: `1,000` ledgers (~1.5 days at 5s/ledger).

### Behaviour
- **Writes (`distribute()`, `init()`)**: Call `env.storage().instance().extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT)`. Because `distribute()` first reads from instance storage, calling it *after* the window expires will fail.
- **Conclusion**: The contract admin must ensure a distribution (or other TTL-bumping transaction) is performed at least every `10,000` ledgers to avoid instance archival.

## 3. Settlement Contract

The Settlement contract has a unique profile regarding TTL extensions.

### Constants
- Instance storage has **no explicit TTL constants** and does not call `extend_ttl`.
- Persistent storage (for developer balances) bumps TTL by `50,000` ledgers on writes.

### Behaviour
- Because `receive_payment` and `init` do not call `extend_ttl` on the instance storage, the instance relies entirely on the default network minimum TTL (e.g., `4096` ledgers).
- After the default minimum TTL window elapses without external bumps, any call that reads instance state (like `receive_payment` reading the vault address) will panic with an archival error.
- **Conclusion**: In production, the settlement contract will either require an explicit update to include `extend_ttl` in `receive_payment`, or require periodic admin intervention to keep the instance storage alive.
