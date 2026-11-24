# Deployments

## Stellar testnet

- Network: `Test SDF Network ; September 2015` (RPC `https://soroban-testnet.stellar.org`)
- Deployer / owner / admin: `GDXDZNSTNORCX4373EGGYS4Q3U3XVZMBSIMMRHG5S5GCCF7JHC7QZWAZ` (testnet-only key, generated for this deployment; the secret is not stored in the repository)
- Built with Stellar CLI 27.1.0, `stellar contract build` (target `wasm32v1-none`, soroban-sdk 22)

| Contract | Contract ID | Explorer |
|---|---|---|
| Vault | `CBJZLUKQBWDMZPKNJAVONZ7GCCWADSETRXWOPN5Z3XWAUNI4F2R7XDYQ` | https://stellar.expert/explorer/testnet/contract/CBJZLUKQBWDMZPKNJAVONZ7GCCWADSETRXWOPN5Z3XWAUNI4F2R7XDYQ |
| Settlement | `CB4BUGCX5LFS5KYWAO2J4ZQTALOQSGXFFAOVKKGAS32OFZLU7O7YYGHS` | https://stellar.expert/explorer/testnet/contract/CB4BUGCX5LFS5KYWAO2J4ZQTALOQSGXFFAOVKKGAS32OFZLU7O7YYGHS |
| Revenue pool | `CC5KINWRSLKSSQG4ZOTP3EZIOUVC6ER7HL5GZALYY4LVE6ZIMX4PMCSE` | https://stellar.expert/explorer/testnet/contract/CC5KINWRSLKSSQG4ZOTP3EZIOUVC6ER7HL5GZALYY4LVE6ZIMX4PMCSE |
| Test USDC (SAC for `USDC:GDXDZNST...WAZ`) | `CAC7CFZ42L3LTTZCI4C3XI6UF7AHS2PYTNJ5QEG2PSLRGDGMWRQOZOT5` | https://stellar.expert/explorer/testnet/contract/CAC7CFZ42L3LTTZCI4C3XI6UF7AHS2PYTNJ5QEG2PSLRGDGMWRQOZOT5 |

The USDC token is a Stellar asset contract for an asset issued by the deployer account, used only to exercise the vault on testnet.

## Initialisation

> The vault `init` requires the `owner` to authorize the call, so the deployer
> key that owns the vault must be the transaction source. The vault address is
> not a constructor argument: `init` remains a one-time entrypoint for
> compatibility with the already-initialised deployment below. If
> `initial_balance > 0`, the vault must already hold at least that much USDC
> on-ledger, otherwise `init` fails with `InsufficientBalance`.

| Contract | Call | Transaction |
|---|---|---|
| Settlement | `init(admin = deployer, vault_address = vault)` | [619aa61b...](https://stellar.expert/explorer/testnet/tx/619aa61bdf4189cc4f1ef91253851a4741abb803fe9207ce5b1ea5ccd2150b91) |
| Revenue pool | `init(admin = deployer, usdc_token = USDC)` | [d5346687...](https://stellar.expert/explorer/testnet/tx/d534668717876173900dbb043a8e47f6af9af762aceb80021c57f3cd0d19a051) |
| Vault | `init(owner = deployer, usdc_token = USDC, initial_balance = 0, authorized_caller = deployer, min_deposit = 1, revenue_pool = pool, max_deduct = 1000000000, settlement = settlement)` | [49287f4d...](https://stellar.expert/explorer/testnet/tx/49287f4dd907417b798f85fd2a12643b30ad8b30730e1889bf3fc41da07b1a7e) |

Settlement deploy transaction: [be8f4184...](https://stellar.expert/explorer/testnet/tx/be8f41844e0d00712f98f8b40ac414a7de30ec8ba440cae8fbd8322512079236).

## Smoke test

1. `deposit(caller = deployer, amount = 500000000)` on the vault: [f3c6779a...](https://stellar.expert/explorer/testnet/tx/f3c6779ab9a67c5e3c534c5fce5d7cdcf8600a165ffb36a686b1291ffdaf2422). `balance()` returned `500000000`.
2. `deduct(caller = deployer, amount = 100000000, request_id = 1)`, which transfers USDC to settlement and calls `settlement.record_deduction`: [cf4cd1a3...](https://stellar.expert/explorer/testnet/tx/cf4cd1a31c3590ef333e5507b169217d8c5cbe4bd6b81b9a04f67bfdce599093). `balance()` returned `400000000`.
3. Read calls `get_owner`, `get_admin`, `get_settlement`, `get_max_deduct` all returned the configured values.

## Reproducing

```bash
# Generate and fund a deployer key (first time only).
stellar keys generate my-deployer --network testnet --fund

# Preview every build, deploy and init command without network access.
./scripts/deploy-testnet.sh --source my-deployer --network testnet --dry-run

# Deploy and wire the contracts. This deploys USDC:my-deployer automatically.
./scripts/deploy-testnet.sh --source my-deployer --network testnet

# Or reuse an existing USDC SAC contract ID.
./scripts/deploy-testnet.sh --source my-deployer --network testnet --usdc <usdc-contract-id>
```

The script requires the Stellar CLI and `jq`, builds the three core contracts for
`wasm32v1-none`, deploys settlement, vault and revenue pool, initializes them in
dependency order, verifies the vault wiring, and prints a contract ID table.
It never accepts or writes a secret key. `Option<Address>` values are passed as
quoted JSON strings automatically.
