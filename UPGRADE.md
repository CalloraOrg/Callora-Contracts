# Vault Upgrade and Migration Path

This document describes how the Callora Vault is deployed, how its state is stored, and how to migrate to a new contract if needed. Soroban contract upgradeability is limited: you cannot replace the code of an existing contract instance in place. Migration is done by deploying a new contract and moving state and traffic to it.

## Current Deploy Model

- **One contract WASM**: The vault is built as a single Soroban contract (`callora-vault`). Build with:
  ```bash
  cd contracts/vault && cargo build --target wasm32-unknown-unknown --release
  ```
- **One instance per vault**: Each vault is a separate contract instance created by deploying the same WASM and calling `init(owner, initial_balance, min_deposit)` once. The instance ID is the “vault address” used by the backend and frontend.
- **No in-place upgrades**: There is no built-in mechanism to change the code of an existing instance. To change behavior, you deploy a new contract (new WASM or new instance) and migrate.

## Storage Layout

The vault uses **instance storage** with a single key:

| Key    | Type        | Description                     |
| ------ | ----------- | ------------------------------- |
| `meta` | `VaultMeta` | Owner, balance, and min_deposit |

`VaultMeta` layout is documented in [contracts/vault/STORAGE.md](contracts/vault/STORAGE.md). Any migration must read this layout so existing state can be exported and re-imported.

## Migration Strategy: Deploy New Contract and Redirect

When you need to move to a new vault (e.g. new code or new instance):

1. **Export state from the old vault**
   - Read `get_meta()` from the current contract instance (owner, balance, min_deposit).
   - Optionally export event history or audit data for your records (from indexer/archives).

2. **Deploy the new contract**
   - Build and deploy the new WASM (or deploy a new instance of the same WASM).
   - Call `init(owner, Some(current_balance), Some(min_deposit))` with the exported owner and, if desired, the same balance and min_deposit.
   - If you are not moving balance on-chain automatically, you may init with `initial_balance: Some(0)` and treat the old vault as “drained” and the new one as the new ledger.

3. **Move balance (if applicable)**
   - If the old vault holds real assets (e.g. USDC), you must transfer them off the old vault (e.g. via owner `withdraw` or integration-specific flows) and then fund the new vault (e.g. deposit or token transfer). The current vault contract does not hold token balances; it tracks an internal balance. When token integration is added, migration would include withdrawing from the old vault and depositing into the new one.

4. **Redirect users and backend**
   - Point the backend (and any frontend) to the new contract instance ID for that vault.
   - Retire or deprecate the old instance (no further calls).

## Versioning and Compatibility

- **Storage compatibility**: New contract versions that add or change fields in `VaultMeta` (or add new keys) are not backward compatible with existing instance data. Migration must either:
  - Deploy a new instance and init with the desired state (recommended), or
  - Use a migration contract/tool that reads the old layout and writes the new one (advanced).
- **Interface compatibility**: Keep `init`, `deposit`, `deduct`, `balance`, `withdraw`, and `withdraw_to` semantics stable for the same instance ID, or treat a new instance as a new vault and migrate as above.

## Event Schema Stability

To support reliable indexing and real-time monitoring:

- **Deduct Event Topics**: The `deduct` event (emitted by both `deduct` and `batch_deduct`) always contains exactly 3 topics: `["deduct", caller, request_id]`.
- **Empty Symbol Convention**: If no `request_id` is provided by the caller, the contract emits an empty symbol (`""`) as the third topic. This ensures indexers can use a fixed topic count and consistent filtering patterns without breaking on optional fields.

## Summary

- **Deploy**: One WASM, one `init` per vault instance.
- **Storage**: Single `meta` key; see STORAGE.md for layout.
- **Upgrade**: No in-place code upgrade; deploy a new contract/instance, export state, re-init and move balance as needed, then redirect traffic to the new instance.
