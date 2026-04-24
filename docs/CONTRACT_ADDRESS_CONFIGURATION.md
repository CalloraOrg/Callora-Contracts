# Contract Address Configuration Guide for Backend Operators

This guide explains how to configure and verify the three contract addresses that the
**`callora-vault`** contract uses to route deducted USDC after each API call.

---

## Background

When a backend operator calls `deduct` or `batch_deduct`, the vault reduces the
caller's on-chain balance **and** transfers the corresponding USDC to a downstream
contract. The destination is determined by two configurable addresses stored in the
vault:

| Address slot   | Storage key  | Purpose                                                              |
|----------------|--------------|----------------------------------------------------------------------|
| `settlement`   | `Settlement` | `callora-settlement` contract; tracks per-developer balances         |
| `revenue_pool` | `RevenuePool`| `callora-revenue-pool` contract; simple admin-controlled distribution|

**Priority rule**: when both are configured, **`settlement` takes priority** and
`revenue_pool` is not used in the same deduct call.
If neither address is set, the deducted amount stays inside the vault (balance is
reduced but no USDC transfer occurs).

---

## Addresses at a glance

```
callora-vault
├── usdc_token      ← set at init(); never changes
├── settlement      ← set via set_settlement();   read via get_settlement()
└── revenue_pool    ← set via set_revenue_pool(); read via get_revenue_pool()
```

All three can be read in one call with `get_contract_addresses()`.

---

## Step-by-step deployment checklist

### 1. Deploy or locate the USDC token contract

On **Stellar testnet**, USDC is available at:

```bash
stellar contract id asset \
    --asset USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 \
    --network testnet
```

On **mainnet**, use the canonical Circle USDC issuer:

```
Issuer:  GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN
Asset:   USDC
```

Record the resulting contract ID — this is your `usdc_token` argument for `init`.

---

### 2. Deploy the settlement contract (recommended for production)

```bash
stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/callora_settlement.wasm \
    --source <OPERATOR_KEY> \
    --network testnet
# → SETTLEMENT_CONTRACT_ID
```

Initialize it:

```bash
stellar contract invoke \
    --id <SETTLEMENT_CONTRACT_ID> \
    --source <OPERATOR_KEY> \
    --network testnet \
    -- init \
    --admin <ADMIN_ADDRESS> \
    --vault_address <VAULT_CONTRACT_ID>
```

---

### 3. Deploy the revenue pool (optional)

```bash
stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/callora_revenue_pool.wasm \
    --source <OPERATOR_KEY> \
    --network testnet
# → REVENUE_POOL_CONTRACT_ID
```

Initialize it:

```bash
stellar contract invoke \
    --id <REVENUE_POOL_CONTRACT_ID> \
    --source <OPERATOR_KEY> \
    --network testnet \
    -- init \
    --admin <ADMIN_ADDRESS> \
    --usdc_token <USDC_TOKEN_ID>
```

---

### 4. Deploy the vault and call `init`

```bash
stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/callora_vault.wasm \
    --source <OPERATOR_KEY> \
    --network testnet
# → VAULT_CONTRACT_ID

stellar contract invoke \
    --id <VAULT_CONTRACT_ID> \
    --source <OPERATOR_KEY> \
    --network testnet \
    -- init \
    --owner <OWNER_ADDRESS> \
    --usdc_token <USDC_TOKEN_ID> \
    --revenue_pool <REVENUE_POOL_CONTRACT_ID>
```

> **Note:** `settlement` is not passed to `init`; register it with `set_settlement`
> after deployment (see step 5).

---

### 5. Register the settlement contract in the vault

Only the vault admin may call `set_settlement`:

```bash
stellar contract invoke \
    --id <VAULT_CONTRACT_ID> \
    --source <ADMIN_KEY> \
    --network testnet \
    -- set_settlement \
    --caller <ADMIN_ADDRESS> \
    --settlement_address <SETTLEMENT_CONTRACT_ID>
```

---

### 6. Verify all addresses with `get_contract_addresses`

```bash
stellar contract invoke \
    --id <VAULT_CONTRACT_ID> \
    --source <OPERATOR_KEY> \
    --network testnet \
    -- get_contract_addresses
```

Expected output (JSON):

```json
[
  "C<USDC_TOKEN_ID>",
  "C<SETTLEMENT_CONTRACT_ID>",
  "C<REVENUE_POOL_CONTRACT_ID>"
]
```

`null` appears for any slot that has not been configured yet.

You can also query each slot individually:

```bash
# Settlement address
stellar contract invoke --id <VAULT_CONTRACT_ID> -- get_settlement

# Revenue pool address
stellar contract invoke --id <VAULT_CONTRACT_ID> -- get_revenue_pool
```

---

## Updating addresses after deployment

All setters are admin-only and can be called at any time after `init`.

| Goal                               | Function                                         |
|------------------------------------|--------------------------------------------------|
| Change / set the settlement address | `set_settlement(caller, settlement_address)`    |
| Change / set the revenue pool       | `set_revenue_pool(caller, Some(new_address))`   |
| Remove revenue pool routing         | `set_revenue_pool(caller, None)`                |

> ⚠️ Address changes take effect **immediately** on the next `deduct` call.
> Coordinate with your monitoring stack before switching in production.

---

## TypeScript / stellar-sdk example

```ts
import { Contract, SorobanRpc, xdr, scValToNative } from "@stellar/stellar-sdk";

const server  = new SorobanRpc.Server("https://soroban-testnet.stellar.org");
const vault   = new Contract(VAULT_CONTRACT_ID);

const sim = await server.simulateTransaction(
    buildTransaction(vault.call("get_contract_addresses"))
);

const [usdcToken, settlement, revenuePool] =
    scValToNative(sim.result.retval);

console.log({ usdcToken, settlement, revenuePool });

if (!settlement) {
    console.warn("settlement address not configured — USDC stays in vault");
}
```

---

## Security considerations

- **Admin key**: `set_settlement` and `set_revenue_pool` both call `require_auth()`
  and check the stored admin address. Use a hardware wallet or multisig for the admin.
- **Address validation**: The vault does **not** verify that configured addresses are
  valid contracts. Before calling `set_settlement`, confirm the settlement contract is
  deployed, initialized, and has the vault address registered via `set_vault`.
- **Atomicity**: Each address change is a single storage write; no partial update is
  observable by concurrent callers.
- **Testnet vs mainnet**: USDC token IDs differ across networks. Always confirm with
  `get_contract_addresses` after deployment.

---

## See also

- [`docs/ACCESS_CONTROL.md`](ACCESS_CONTROL.md) — role matrix for all privileged functions
- [`SECURITY.md`](../SECURITY.md) — security checklist and threat model
- [`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md) — events emitted by `set_settlement` / `set_revenue_pool`
- [`SETTLEMENT_IMPLEMENTATION.md`](../SETTLEMENT_IMPLEMENTATION.md) — end-to-end settlement flow