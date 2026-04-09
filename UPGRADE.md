# Multi-Contract Upgrade and Migration Playbook

This document describes how the Callora smart contracts are deployed, how state is stored, and how to upgrade or migrate each contract. Soroban contract upgradeability is limited: you cannot replace the code of an existing contract instance in place. Migration is done by deploying a new contract and migrating state and traffic.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Contract Storage Layouts](#contract-storage-layouts)
3. [Upgrade Choreography](#upgrade-choreography)
4. [Backend Coordination](#backend-coordination)
5. [Admin Key Handling](#admin-key-handling)
6. [Rollback Stance](#rollback-stance)
7. [Stellar Network Procedures](#stellar-network-procedures)
8. [Verification Checklist](#verification-checklist)

---

## Architecture Overview

The Callora workspace consists of three independently deployed Soroban smart contracts:

| Contract | Crate | Purpose |
|----------|-------|---------|
| **callora-vault** | `contracts/vault` | USDC vault for prepaid API calls; tracks per-user balance, min deposits, and authorized callers |
| **callora-revenue-pool** | `contracts/revenue_pool` | Receives USDC from vault deducts and distributes to developers |
| **callora-settlement** | `contracts/settlement` | Developer balance tracking and global pool settlement |

### Deploy Model

- **One WASM per contract type**: Each contract is built as a separate Soroban WASM module.
- **One instance per logical entity**: Each vault/revenue-pool/settlement is a separate contract instance identified by its contract address.
- **No in-place upgrades**: Soroban does not support replacing contract code in place. To change behavior, you must deploy a new WASM and migrate state.

### Dependency Flow

```
User deposits USDC
        │
        ▼
  ┌─────────┐      deduct       ┌──────────────┐      distribute      ┌─────────────┐
  │  Vault  │ ───────────────► │ Revenue Pool │ ─────────────────► │ Developer   │
  │         │   (if revenue_    │              │                    │ Wallet      │
  └─────────┘    pool set)      └──────────────┘                    └─────────────┘

  ┌─────────┐      deduct       ┌────────────┐
  │  Vault  │ ───────────────► │ Settlement │ (alternative to revenue pool)
  └─────────┘                  └────────────┘
```

---

## Contract Storage Layouts

### Vault (`contracts/vault/src/lib.rs`)

The vault uses **instance storage** with the following keys:

| StorageKey | Type | Description |
|------------|------|-------------|
| `Meta` | `VaultMeta` | Owner address, tracked balance, authorized caller, min deposit |
| `AllowedDepositors` | `Vec<Address>` | List of addresses permitted to deposit |
| `Admin` | `Address` | Admin address (defaults to owner at init) |
| `UsdcToken` | `Address` | USDC token contract address |
| `Settlement` | `Option<Address>` | Optional settlement contract for deduct flow |
| `RevenuePool` | `Option<Address>` | Optional revenue pool address for deduct flow |
| `MaxDeduct` | `i128` | Maximum amount per single deduct (default `i128::MAX`) |
| `Metadata(String)` | `String` | Per-offering metadata (IPFS CID or URI) |

**VaultMeta structure** (defined in `lib.rs:46-51`):

```rust
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
    pub authorized_caller: Option<Address>,
    pub min_deposit: i128,
}
```

**Init signature** (`lib.rs:93-131`):

```rust
pub fn init(
    env: Env,
    owner: Address,
    usdc_token: Address,
    initial_balance: Option<i128>,
    authorized_caller: Option<Address>,
    min_deposit: Option<i128>,
    revenue_pool: Option<Address>,
    max_deduct: Option<i128>,
) -> VaultMeta
```

### Revenue Pool (`contracts/revenue_pool/src/lib.rs`)

| Key | Type | Description |
|-----|------|-------------|
| `admin` | `Address` | Admin address; may call `distribute` and `set_admin` |
| `usdc` | `Address` | USDC token contract address |

**Init signature** (`lib.rs:28-39`):

```rust
pub fn init(env: Env, admin: Address, usdc_token: Address)
```

### Settlement (`contracts/settlement/src/lib.rs`)

| Key | Type | Description |
|-----|------|-------------|
| `admin` | `Address` | Admin address; may call `set_admin`, `set_vault` |
| `vault` | `Address` | Registered vault address |
| `developer_balances` | `Map<Address, i128>` | Per-developer balance tracking |
| `global_pool` | `GlobalPool` | Total balance and last updated timestamp |

**GlobalPool structure** (`lib.rs:16-19`):

```rust
pub struct GlobalPool {
    pub total_balance: i128,
    pub last_updated: u64,
}
```

**Init signature** (`lib.rs:51-65`):

```rust
pub fn init(env: Env, admin: Address, vault_address: Address)
```

---

## Upgrade Choreography

Because contracts reference each other by address, upgrades must be sequenced carefully to maintain consistency.

### Recommended Upgrade Order

```
1. Settlement (if changing)
       │
       ▼
2. Revenue Pool (if changing)
       │
       ▼
3. Vault (always upgrade last if updating references)
```

**Rationale**: The vault can reference either a settlement contract or a revenue pool. Update the target first, then update the vault's reference.

### Per-Contract Upgrade Steps

#### A. Upgrading Vault

1. **Export state**
   ```bash
   # Read current state via RPC or CLI
   soroban contract invoke --contract-id <VAULT_ID> -- get_meta
   soroban contract invoke --contract-id <VAULT_ID> -- get_admin
   soroban contract invoke --contract-id <VAULT_ID> -- get_settlement  # if set
   ```

2. **Deploy new vault WASM**
   ```bash
   cargo build --target wasm32-unknown-unknown --release -p callora-vault
   soroban contract deploy --wasm target/wasm32-unknown-unknown/release/callora_vault.wasm --source <OWNER_ACCOUNT>
   ```

3. **Initialize new vault** (same owner, same USDC token, migrate balance)
   ```bash
   soroban contract invoke --contract-id <NEW_VAULT_ID> -- init \
     --owner <OWNER> \
     --usdc_token <USDC_TOKEN> \
     --initial_balance <CURRENT_BALANCE> \
     --authorized_caller <AUTH_CALLER> \
     --min_deposit <MIN_DEPOSIT> \
     --revenue_pool <REVENUE_POOL_OR_NONE> \
     --max_deduct <MAX_DEDUCT>
   ```

4. **Transfer actual USDC** (if balance was real USDC)
   ```bash
   # From old vault owner, withdraw to self, then deposit to new vault
   soroban contract invoke --contract-id <OLD_VAULT_ID> -- withdraw --amount <BALANCE>
   # Then deposit from owner to new vault
   soroban contract invoke --contract-id <NEW_VAULT_ID> -- deposit --caller <OWNER> --amount <BALANCE>
   ```

5. **Update backend config** (see Backend Coordination below)

6. **Decommission old vault** (stop using; do not delete)

#### B. Upgrading Revenue Pool

1. **Export state**
   ```bash
   soroban contract invoke --contract-id <RP_ID> -- get_admin
   soroban contract invoke --contract-id <RP_ID> -- balance
   ```

2. **Deploy new revenue pool WASM**
   ```bash
   cargo build --target wasm32-unknown-unknown --release -p callora-revenue-pool
   soroban contract deploy --wasm target/wasm32-unknown-unknown/release/callora_revenue_pool.wasm --source <ADMIN_ACCOUNT>
   ```

3. **Initialize new revenue pool**
   ```bash
   soroban contract invoke --contract-id <NEW_RP_ID> -- init \
     --admin <ADMIN> \
     --usdc_token <USDC_TOKEN>
   ```

4. **Transfer USDC balance** (if applicable)
   - Revenue pool holds actual USDC tokens
   - Transfer from old contract to new via token `transfer`

5. **Update vault references** (if vault points to this revenue pool)

6. **Decommission old revenue pool**

#### C. Upgrading Settlement

1. **Export state**
   ```bash
   soroban contract invoke --contract-id <SETTLE_ID> -- get_admin
   soroban contract invoke --contract-id <SETTLE_ID> -- get_global_pool
   soroban contract invoke --contract-id <SETTLE_ID> -- get_all_developer_balances
   ```

2. **Deploy new settlement WASM**
   ```bash
   cargo build --target wasm32-unknown-unknown --release -p callora-settlement
   soroban contract deploy --wasm target/wasm32-unknown-unknown/release/callora_settlement.wasm --source <ADMIN_ACCOUNT>
   ```

3. **Initialize new settlement**
   ```bash
   soroban contract invoke --contract-id <NEW_SETTLE_ID> -- init \
     --admin <ADMIN> \
     --vault_address <VAULT_ADDRESS>
   ```

4. **Re-credit developer balances**
   - Call `receive_payment` for each developer with their balance
   - Or implement a migration helper contract

5. **Update vault references**

6. **Decommission old settlement**

---

## Backend Coordination

When any contract is upgraded, the backend must update its configuration:

### Configuration Changes Required

| Upgrade Type | Backend Config Update |
|-------------|----------------------|
| New vault instance | Update `vault_contract_id` per user/API |
| New revenue pool | Update `revenue_pool_contract_id` in vault (via `set_settlement`) |
| New settlement | Update `settlement_contract_id` in vault (via `set_settlement`) |
| Vault points to new revenue pool | Call `vault.set_revenue_pool(new_address)` |
| Vault points to new settlement | Call `vault.set_settlement(new_address)` |

### Backend Update Sequence

```
1. Deploy new contract(s)
2. Initialize with migrated state
3. Update backend configuration (new contract addresses)
4. Verify backend can reach new contracts
5. Point traffic to new contract (gradual or atomic switchover)
6. Monitor for 24-48 hours
7. Decommission old contract (stop calls, archive address)
```

### Health Checks After Upgrade

- Verify `get_meta()` / `get_admin()` / `balance()` return expected values
- Run a small test transaction before full traffic switchover
- Monitor error rates and revert if anomalies detected

---

## Admin Key Handling

### Key Management Expectations

Admin keys for all three contracts should be managed with care:

| Contract | Admin Role | Key Type Recommendation |
|----------|-----------|------------------------|
| Vault | Sets distribution recipients, authorized callers, min deposits | Hardware wallet or multisig |
| Revenue Pool | Calls `distribute`, `batch_distribute`, `set_admin` | Hardware wallet or multisig |
| Settlement | Calls `set_admin`, `set_vault`, receives payments | Hardware wallet or multisig |

### Key Rotation Procedure

To rotate an admin key:

1. **Ensure new admin key is accessible** (test in non-production first)
2. **Call `set_admin`** on each affected contract:
   ```bash
   soroban contract invoke --contract-id <CONTRACT_ID> -- set_admin \
     --caller <OLD_ADMIN> \
     --new_admin <NEW_ADMIN>
   ```
3. **Verify** by calling `get_admin()` and confirming new address
4. **Update backend** to use new admin key for signing transactions
5. **Archive old admin key** (do not delete; retain for audit purposes)

### Multisig Considerations

- If using a Stellar multisig account (e.g., 2-of-3), all admin operations require sufficient signers
- Coordinate multisig transactions carefully to avoid being locked out
- Test multisig threshold changes on testnet before mainnet

---

## Rollback Stance

**Rollback is not supported as a first-class operation.** Due to Soroban's immutability design, there is no mechanism to revert a contract instance to previous code. Instead, rollback is achieved through redeployment.

### Rollback Procedure

If an upgrade causes issues:

1. **Do not attempt to modify the upgraded contract** — it cannot be changed
2. **Deploy the previous WASM as a new instance**:
   ```bash
   # Get previous WASM (from git history or artifact store)
   git checkout <PREVIOUS_COMMIT>
   cargo build --target wasm32-unknown-unknown --release -p callora-vault
   soroban contract deploy --wasm target/wasm32-unknown-unknown/release/callora_vault.wasm
   ```
3. **Migrate state back** (export from current, import to previous)
4. **Update backend** to point to the previous contract instance
5. **Investigate** the issue in the new contract separately (do not delete the new contract yet)

### Rollback Decision Matrix

| Scenario | Rollback Recommended? | Alternative |
|----------|----------------------|-------------|
| Critical bug affecting funds | Yes | Deploy hotfix and migrate |
| Non-critical bug | No | Deploy fix in next release cycle |
| Performance regression | No | Optimize and redeploy |
| Feature removal | No | Communicate to users; deprecate |

### Prevention

- Always test upgrades on testnet first
- Run full test suite (`cargo test`) and coverage (`./scripts/coverage.sh`) before any upgrade
- Use gradual traffic switchover (e.g., 5% → 25% → 100%) to catch issues early

---

## Stellar Network Procedures

### Soroban Upgrade Constraints

Soroban smart contracts on Stellar have the following upgrade characteristics:

1. **No in-place code replacement**: Once a contract is deployed, its WASM code cannot be changed.
2. **Contract addresses are deterministic**: The address is derived from the deployer's public key and sequence number, not from the WASM code hash.
3. **Storage persists independently**: Contract storage exists separately from the WASM code and travels with the contract address.

### WASM Size Limits

Soroban enforces a **64 KB WASM size limit**. The Callora contracts are optimized to stay under this limit:

```bash
# Check WASM size
./scripts/check-wasm-size.sh

# Build optimized WASM
cargo build --target wasm32-unknown-unknown --release -p callora-vault
```

Current optimized sizes should be approximately:
- `callora-vault`: ~17-18 KB
- `callora-revenue-pool`: ~15-16 KB
- `callora-settlement`: ~16-17 KB

### Deployment on Stellar

1. **Build the WASM**
   ```bash
   cargo build --target wasm32-unknown-unknown --release -p <crate-name>
   ```

2. **Deploy using Soroban CLI or Stellar Laboratory**
   ```bash
   soroban contract deploy \
     --wasm target/wasm32-unknown-unknown/release/<contract>.wasm \
     --source <DEPLOYER_ACCOUNT>
   ```

3. **Initialize the contract**
   ```bash
   soroban contract invoke --contract-id <NEW_ID> -- init <args>
   ```

4. **Verify on-chain**
   ```bash
   soroban contract invoke --contract-id <NEW_ID> -- get_meta  # or other view function
   ```

### Network Selection

| Network | Use For |
|---------|---------|
| **Testnet** | Development, testing upgrades, integration testing |
| **Mainnet** | Production deployment |

**Never deploy experimental code directly to mainnet.**

---

## Verification Checklist

Before and after any upgrade, verify the following:

### Pre-Upgrade Verification

- [ ] All tests pass: `cargo test`
- [ ] Coverage above 95%: `./scripts/coverage.sh`
- [ ] Clippy clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Format clean: `cargo fmt -- --check`
- [ ] WASM size under limit: `./scripts/check-wasm-size.sh`
- [ ] State export from old contracts completed
- [ ] Backend configuration backup taken
- [ ] Rollback plan documented and tested (if critical)

### Post-Upgrade Verification

- [ ] `get_meta()` / `get_admin()` / `balance()` return expected values
- [ ] Test transaction executed successfully
- [ ] Backend can communicate with new contracts
- [ ] Error rates nominal (compare to pre-upgrade baseline)
- [ ] Event emissions correct (verify emitted events match expected)
- [ ] Monitoring dashboards updated (if applicable)
- [ ] Old contract marked as decommissioned (no new traffic)

### Test and Commit Requirements

Per the contribution guidelines:

1. Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` from workspace root
2. For WASM builds: `cargo build --target wasm32-unknown-unknown --release -p callora-vault` (adjust `-p` as needed)
3. Run `./scripts/coverage.sh` (or `cargo tarpaulin` per `tarpaulin.toml`)
4. Include summarized test output in PR description

---

## Summary

| Aspect | Recommendation |
|--------|----------------|
| **Upgrade approach** | Deploy new contract, migrate state, redirect traffic |
| **Upgrade order** | Settlement → Revenue Pool → Vault |
| **Rollback** | Not supported; deploy previous WASM as new instance |
| **Admin keys** | Hardware wallet or multisig; rotate via `set_admin` |
| **Testing** | Testnet first, then gradual mainnet rollout |
| **Verification** | Run full test suite and coverage before any upgrade |

For detailed storage layouts, see:
- [Vault Storage Layout](contracts/vault/STORAGE.md)
- [Vault Access Control](contracts/vault/ACCESS_CONTROL.md)
- [Core Contracts README](contracts/README.md)
