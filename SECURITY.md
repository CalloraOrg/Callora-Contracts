# Security

This document outlines security best practices and checklist items for Callora vault contracts to improve audit readiness and reviewer confidence.

## 🔐 Vault Security Checklist

### Access Control

- [ ] All privileged functions protected by `require_auth()` or `require_auth_for_args()` via `Address`
- [ ] Admin state stored securely (e.g., using `env.storage().instance()`)
- [ ] Admin rotation/transfer tested and documented

### Arithmetic Safety

- [x] No integer overflow/underflow possible
- [ ] Solidity ^0.8.x overflow checks relied upon or SafeMath used where required
- [x] For Soroban/Rust: `checked_add` / `checked_sub` used for all balance mutations
- [x] `overflow-checks` enabled in both dev and release profiles

> All balance mutations in `callora-vault` (`deposit`, `deduct`, `batch_deduct`, `withdraw`, `withdraw_to`) and `callora-revenue-pool` (`batch_distribute`) use `checked_add` / `checked_sub` and panic with a descriptive message on overflow. `callora-settlement` (`receive_payment`) does the same. The workspace `Cargo.toml` sets `overflow-checks = true` for both `dev` and `release` profiles, so even plain arithmetic would trap in debug builds — the explicit checked calls make the intent clear and guarantee the same behaviour in all build configurations.

Additional hardening note:
- Removed a duplicated `get_max_deduct` entrypoint declaration in `callora-vault` to avoid ambiguous review surfaces and keep ABI-facing code paths singular.

### Initialization / Re-initialization

- [ ] `initialize` function protected against multiple calls (e.g., checking if admin key exists in `instance()` storage)
- [ ] Contract upgrades (`env.deployer().update_current_contract_wasm()`) protected by `require_auth()`
- [ ] No unprotected re-init functions
- [ ] `initialize` validates all input parameters

### Pause / Circuit Breaker

- [x] Emergency pause mechanism implemented via state flag in `instance()` storage
- [x] Paused state blocks fund movement (e.g., reverting via `panic_with_error!`)
- [x] Pause/unpause flows tested
- [x] `is_paused()` view function exposed for off-chain monitoring
- [x] View function is read-only, deterministic, and non-panicking
- [x] Safe default state (returns `false` when unset)

### Admin Transfer

- [x] Ownership transfer is two-step (optional but recommended)
- [ ] Ownership transfer emits events
- [ ] Renounce ownership reviewed and justified

### Authorized Caller Role Management

The vault exposes a dedicated `authorized_caller` role (stored in `VaultMeta`
and settable via `set_authorized_caller`) that is permitted to invoke
balance-mutating operations such as `deduct` and `batch_deduct`. This role is
distinct from `owner` and `admin`, and reviewers should confirm the following
controls are in place:

- [x] `authorized_caller` is stored in `VaultMeta` under the `Meta` instance
  storage key and is not duplicated in any other location
- [x] Only the current `owner` can set or rotate `authorized_caller` via
  `set_authorized_caller` (enforced by `meta.owner.require_auth()`)
- [x] `set_authorized_caller` emits a `set_auth_caller` event with the owner
  as topic and the new caller address as data, enabling off-chain monitoring
  of role changes
- [x] `deduct` and `batch_deduct` reject callers that are not the currently
  configured `authorized_caller` (panic: `unauthorized: caller is not the authorized caller`)
- [x] When `authorized_caller` is `None`, privileged caller-only operations
  are rejected rather than defaulting to owner/admin, preventing accidental
  over-privileged execution
- [ ] Rotation flow (set → use → rotate → old caller rejected) covered by
  unit tests in `contracts/vault/src/test.rs`
- [ ] Role changes are reviewed as part of the operational runbook; the new
  caller address is verified off-chain (e.g. multisig or governance) before
  the owner signs `set_authorized_caller`
- [ ] `authorized_caller` is scoped strictly to deduct-class operations and
  does **not** grant the ability to withdraw, distribute, pause, or upgrade
  the contract

> **Security note:** `authorized_caller` is intentionally a narrow-privilege
> role meant for the off-chain billing/settlement driver. It can spend vault
> balance via `deduct` / `batch_deduct` within the configured `max_deduct`
> limit, so the owning key should rotate it immediately if the off-chain
> driver's signing key is suspected of compromise. Because rotation is a
> single-call owner-only operation with an emitted event, recovery is
> observable and atomic.


### External Calls

- [ ] Token transfers strictly rely on `soroban_sdk::token::Client`
- [ ] Cross-contract calls handle potential errors/panics gracefully
- [ ] State changes are persisted before making cross-contract calls to mitigate subtle state-caching issues
- [ ] Checks-effects-interactions pattern followed

### Revenue Routing External Transfers (Issue #110)

The vault performs USDC transfers to configurable counterpart addresses on every
`deduct` and `batch_deduct` call. These external transfers are justified as follows:

- **settlement address**: set and updated exclusively by the on-chain admin via
  `set_settlement`. Transfers to this address implement the documented
  `Vault → Settlement` revenue flow described in `SETTLEMENT_IMPLEMENTATION.md`.
- **revenue_pool address**: set and updated exclusively by the on-chain admin via
  `set_revenue_pool`. Transfers to this address route product revenue to the
  designated pool contract.
- **Priority rule**: when both are configured, `settlement` takes priority and
  `revenue_pool` is not used in the same deduct. This prevents "half updated"
  routing states where funds could be split unexpectedly across two recipients.
- **CRITICAL - Routing Required**: At least one routing address (settlement OR
  revenue_pool) MUST be configured before any deduct operations can succeed.
  If neither is configured, `deduct()` and `batch_deduct()` will panic with
  `"routing not configured: set settlement or revenue_pool address"`. This
  prevents silent fund retention and ensures explicit routing configuration.
- **Address Validation**: Both `set_settlement()` and `set_revenue_pool()` validate
  that the provided address is NOT the vault's own address, preventing
  self-referential routing loops.
- **Atomic Updates**: Each address is updated atomically in a single storage write,
  ensuring no partial update is observable by other callers.
- **Audit Trail**: All routing configuration changes emit events:
  - `set_settlement(admin) → address` when setting settlement
  - `set_revenue_pool(admin) → address` when setting revenue pool
  - `clear_revenue_pool(admin) → ()` when clearing revenue pool

### Vault-Specific Risks

- [ ] Deposit/withdraw invariants tested
- [ ] Vault balance accounting verified
- [ ] Funds cannot be locked permanently
- [ ] Minimum deposit requirements enforced
- [ ] Maximum deduction limits enforced
- [x] Revenue pool transfers validated
- [ ] Batch operations respect individual limits

### Revenue Pool Security Assumptions

The Revenue Pool contract (`contracts/revenue_pool`) operates under the following security assumptions and threat models:

- **Malicious Admin:** The `admin` role has the authority to distribute funds and replace the admin address. A compromised or malicious admin could drain the pool's USDC balance.
  - *Mitigation:* The `admin` should always be a heavily guarded multisig account or a rigorously audited governance contract.

- **Wrong USDC Token Initialization:** The `usdc_token` address is set once during `init`. If initialized with a malicious or incorrect token address, the pool will process the wrong asset.
  - *Mitigation:* The deployment process must verify the official Stellar USDC (or appropriate wrapped USDC) contract address before initialization. The `init` function guards against re-initialization.

- **Operational Griefing (Balances):** Anyone can effectively transfer USDC to the revenue pool. If an attacker sends unsolicited funds, it increases the `balance()` but does not disrupt the `distribute` logic, as distribution is explicitly controlled by the admin.
  - *Mitigation:* The pool does not rely on strict balance equality invariants for its core operations, mitigating balance-based operational griefing. The `receive_payment` entrypoint is admin-only and event-only (no token movement), so indexers should reconcile `receive_payment` logs with actual token transfers.

### Input Validation

- [ ] All amounts validated to be > 0
- [ ] Address/parameter validation on all public functions
- [ ] Boundary conditions tested (max values, zero values)
- [ ] Error messages provide clear context for debugging
- `callora-vault::init` enforces `min_deposit > 0`; omitted values default to `1`.

### Event Logging

- [ ] All state changes emit appropriate events
- [ ] Event schema documented and indexed
- [ ] Critical operations (deposit, withdraw, deduct) logged with full context

### Testing Coverage

- [x] Unit tests cover all public functions
- [x] Edge cases and boundary conditions tested
- [x] Panic scenarios tested with `#[should_panic]`
- [ ] Integration tests for complete user flows
- [x] Minimum 95% test coverage maintained (enforced via `cargo tarpaulin` with `fail-under = 95.0`)

## External Audit Recommendation

Before any mainnet deployment:

- **Engage an independent third-party security auditor**
  - Choose auditors with experience in Soroban/Stellar smart contracts
  - Ensure auditor understands vault-specific risk patterns

- **Perform a full smart contract audit**
  - Review all contract code for security vulnerabilities
  - Analyze upgrade patterns and migration paths
  - Validate mathematical correctness of balance operations

- **Address all high and medium severity findings**
  - Create tracking system for audit findings
  - Implement fixes for all H/M severity issues
  - Document rationale for any low severity findings that won't be fixed

- **Publish audit report for transparency**
  - Make audit report publicly available
  - Include summary of findings and remediation steps
  - Provide evidence of test coverage and validation

## Additional Security Considerations

### Soroban-Specific Security

- [ ] WASM compilation verified and reproducible (`stellar contract build` / `cargo build --target wasm32-unknown-unknown --release`)
- [ ] Storage lifespan (`extend_ttl`) implemented to prevent state archiving for critical data
- [ ] Stellar network parameters validated (budget, CPU/RAM limits)
- [ ] Cross-contract call security and generic type usage (`Val`) reviewed
- [ ] Storage patterns optimized and secure (e.g., correct usage of `persistent` vs `instance` vs `temporary` keys)

### Economic Security

- [ ] Fee structures reviewed for economic attacks
- [ ] Revenue pool distribution validated
- [ ] Maximum loss scenarios analyzed
- [ ] Slippage and market impact considered

### Operational Security

- [ ] Deployment process documented and automated
- [ ] Key management procedures established
- [ ] Monitoring and alerting configured
- [ ] Incident response plan prepared

## Security Resources

- [Stellar Security Best Practices](https://developers.stellar.org/docs/security/)
- [Soroban Documentation](https://developers.stellar.org/docs/smart-contracts/)
- [Smart Contract Weakness Classification Registry](https://swcregistry.io/)

---

**Note**: This checklist should be reviewed and updated regularly as new security patterns emerge and the codebase evolves.

## require_auth() Audit (Issue #160)

All privileged entrypoints across `vault`, `revenue_pool`, and `settlement` contracts
have been audited for `require_auth()` coverage as part of Issue #160.

### Findings
- All privileged functions call `require_auth()` on the caller before executing. ✅
- Negative tests added to each crate's `test.rs` confirming unauthenticated calls are rejected.

### Intentional Exceptions
| Contract   | Function         | Reason |
|------------|------------------|--------|
| settlement | `init()`         | One-time initializer guarded by already-initialized panic; no auth required by design. |
| vault      | `require_owner()`| Internal helper using `assert!` for address equality. All public callers invoke `caller.require_auth()` before calling this helper, so host-level auth is enforced transitively. Documented gap: `require_owner` itself does not call `require_auth()`. |

### Cross-reference
- Audit branch: `test/require-auth-sweep`
- Tests: `contracts/vault/src/test.rs`, `contracts/revenue_pool/src/test.rs`, `contracts/settlement/src/test.rs`

## Authorization Matrix Update (Settlement)

As part of the authorization matrix hardening for the `callora-settlement` contract:
- `get_all_developer_balances` now requires `admin` authorization via `require_auth()`. This prevents bulk data scraping while allowing administrative oversight.
- Comprehensive negative tests have been added to `contracts/settlement/src/test.rs` covering `receive_payment`, `set_admin`, `set_vault`, and `get_all_developer_balances`.
- Admin rotation (two-step) has been verified to correctly gate access during the transition period.
