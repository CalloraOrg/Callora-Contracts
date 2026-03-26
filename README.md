# Callora Contracts

Soroban smart contracts for the Callora API marketplace: prepaid vault (USDC) and balance deduction for pay-per-call.

## Tech stack

- **Rust** with **Soroban SDK** (Stellar)
- Contract compiles to WebAssembly and deploys to Stellar/Soroban

## What’s included

- **`callora-vault`** contract:
  - `init(owner, initial_balance)` — initialize vault for an owner with optional initial balance
  - `get_meta()` — view config (owner and current balance)
  - `set_allowed_depositor(caller, depositor)` — owner-only; set or clear a backend/allowed depositor that can deposit and manage pricing
  - `deposit(caller, amount)` — owner or allowed depositor increases ledger balance
  - `deduct(amount)` — decrease balance for an API call (backend uses this after metering usage)
  - `balance()` — current ledger balance
  - `set_metadata(caller, offering_id, metadata)` — owner-only; attach off-chain metadata reference (IPFS CID or URI) to an offering
  - `update_metadata(caller, offering_id, metadata)` — owner-only; update existing offering metadata
  - `get_metadata(offering_id)` — retrieve metadata reference for an offering
- **`callora-revenue-pool`** contract (settlement):
  - `init(admin, usdc_token)` — set admin and USDC token
  - `distribute(caller, to, amount)` — admin sends USDC from this contract to a developer
  - Flow: vault deduct → vault transfers USDC to revenue pool → admin calls `distribute(to, amount)`
  - `set_price(caller, api_id, price)` — owner or allowed depositor sets the **price per API call** for `api_id` in smallest USDC units (e.g. 1 = 1 cent)
  - `get_price(api_id)` — returns `Option<i128>` with the configured price per call for `api_id`

### API pricing resolution

The backend resolves `(vault_id, api_id) -> price` as follows:

1. Use the vault contract address as `vault_id`.
2. Call `get_price(api_id)` on that vault.
3. If a price is returned, use it as the per-call price (in smallest USDC units) before calling `deduct(amount)`.

Events are emitted for init, deposit, deduct, withdraw, and withdraw_to. See [EVENT_SCHEMA.md](EVENT_SCHEMA.md) for indexer/frontend use. Approximate gas/cost notes: [BENCHMARKS.md](BENCHMARKS.md). Upgrade and migration: [UPGRADE.md](UPGRADE.md).

We've enhanced the `CalloraVault` contract with robust input validation to prevent invalid transactions:

- **Amount Validation**: Both `deposit()` and `deduct()` now enforce `amount > 0`, rejecting zero and negative values before any state changes
- **Improved Error Messages**: Enhanced panic messages provide clear context (e.g., "insufficient balance: X requested but only Y available")
- **Early Validation**: Checks occur before storage writes, minimizing gas waste on invalid transactions
- **Comprehensive Test Coverage**: Added 5 new test cases covering edge cases:
  - `deposit_zero_panics()` — validates zero deposit rejection
  - `deposit_negative_panics()` — validates negative deposit rejection
  - `deduct_zero_panics()` — validates zero deduction rejection
  - `deduct_negative_panics()` — validates negative deduction rejection
  - `deduct_exceeds_balance_panics()` — validates insufficient balance checks with detailed error messages

All tests use `#[should_panic]` assertions for guaranteed validation. This resolves issue #9.

## Local setup

1. **Prerequisites:**
   - [Rust](https://rustup.rs/) (stable)
   - [Stellar Soroban CLI](https://developers.stellar.org/docs/smart-contracts/getting-started/setup) (`cargo install soroban-cli`)

2. **Build and test:**

   ```bash
   cd callora-contracts
   cargo build
   cargo test
   ```

3. **Build WASM (for deployment):**

   ```bash
   # Build vault contract
   cargo build --target wasm32-unknown-unknown --release -p callora-vault

   # Or use the convenience script from project root
   ./scripts/check-wasm-size.sh
   ```

   The vault contract WASM binary is optimized to ~17.5KB (17,926 bytes), well under Soroban's 64KB limit. The release profile in `Cargo.toml` uses aggressive size optimizations:
   - `opt-level = "z"` - optimize for size
   - `lto = true` - link-time optimization
   - `strip = "symbols"` - remove debug symbols
   - `codegen-units = 1` - better optimization at cost of compile time

   To verify the WASM size stays under 64KB, run:

   ```bash
   ./scripts/check-wasm-size.sh
   ```

## Development

Use one branch per issue or feature (e.g. `test/minimum-deposit-rejected`, `docs/vault-gas-notes`) to keep PRs small and reduce merge conflicts. Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` before pushing.
<<<<<<< HEAD

## Test coverage

The project enforces a **minimum of 95 % line coverage** on every push and pull-request via GitHub Actions.

### Run coverage locally

```bash
# First time only — the script auto-installs cargo-tarpaulin if absent
./scripts/coverage.sh
```

The script will:

1. Check for `cargo-tarpaulin`; install it automatically if it is missing.
2. Run all tests with instrumentation according to `tarpaulin.toml`.
3. Exit with a non-zero code if coverage drops below 95 %.
4. Write reports to the `coverage/` directory (git-ignored).

| Report file                      | Description                                     |
| -------------------------------- | ----------------------------------------------- |
| `coverage/tarpaulin-report.html` | Interactive per-file view — open in any browser |
| `coverage/cobertura.xml`         | Cobertura XML consumed by CI                    |

> **Tip:** You can also run `cargo tarpaulin` directly from the workspace root;
> the settings in `tarpaulin.toml` are picked up automatically.

### CI enforcement

`.github/workflows/coverage.yml` runs on every push and pull-request.
It installs tarpaulin, runs coverage, uploads the HTML report as a downloadable
artefact, and posts a coverage summary table as a PR comment.
A result below 95 % causes the workflow — and the required status check — to fail.



## Project layout

```
callora-contracts/
├── .github/workflows/
│   └── ci.yml              # CI: fmt, clippy, test, WASM build
<<<<<<< HEAD
├── Cargo.toml                        # Workspace and release profile
├── BENCHMARKS.md           # Vault operation gas/cost notes
├── EVENT_SCHEMA.md         # Event names, topics, and payload types
├── UPGRADE.md              # Vault upgrade and migration path
├── tarpaulin.toml                    # cargo-tarpaulin config (≥ 95 % enforced)
├── scripts/
│   └── coverage.sh                   # One-command local coverage runner
├── .github/
│   └── workflows/
│       └── coverage.yml              # CI: enforces 95 % on every push / PR
└── contracts/
    └── vault/
        ├── Cargo.toml
        └── src/
            ├── lib.rs                # Contract logic
            └── test.rs               # Unit tests (covers all code paths)
=======
├── Cargo.toml              # Workspace and release profile
├── BENCHMARKS.md           # Vault operation gas/cost notes
├── EVENT_SCHEMA.md         # Event names, topics, and payload types
├── UPGRADE.md              # Vault upgrade and migration path
├── contracts/
│   ├── vault/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs      # Contract logic
│   │       └── test.rs     # Unit tests
│   └── revenue_pool/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs      # Settlement contract
│           └── test.rs     # Unit tests
└── README.md
>>>>>>> b0229e42e4d4517da9f548ea3e374a5886304bf2
```

## Security Notes

- **Checked arithmetic**: All balance mutations use `checked_add` / `checked_sub` — overflow and underflow cause an immediate panic rather than silent wrapping.
- **Input validation**: `deposit` and `deduct` reject zero and negative amounts (`amount > 0`). `init` rejects negative initial balances.
- **`overflow-checks`**: Enabled for **both** `[profile.dev]` and `[profile.release]` in the workspace `Cargo.toml`, ensuring overflow bugs are caught in tests as well as production.
- **Max balance**: `i128::MAX` (≈ 1.7 × 10³⁸ stroops). Deposits that would exceed this limit will panic.

## Documentation

- [Core Contracts](contracts/README.md)
- [Vault Access Control Model](contracts/vault/ACCESS_CONTROL.md)
- [Security Audit Notes](docs/SECURITY.md)

## 🔐 Security

See [SECURITY.md](SECURITY.md) for the Vault Security Checklist and audit recommendations.

## Deployment

Use Soroban CLI or Stellar Laboratory to deploy the built WASM to testnet/mainnet and configure the vault (owner, optional initial balance and optional pricing). The backend will:

1. Call `get_price(api_id)` on the vault (`vault_id`) to fetch the price per call.
2. Multiply by the number of billable calls to get the total amount.
3. Call `deduct(amount)` on the same vault to charge the user.

This repo is part of [Callora](https://github.com/your-org/callora). Frontend: `callora-frontend`. Backend: `callora-backend`.
