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

### Initialization / Re-initialization

- [ ] `initialize` function protected against multiple calls (e.g., checking if admin key exists in `instance()` storage)
- [ ] Contract upgrades (`env.deployer().update_current_contract_wasm()`) protected by `require_auth()`
- [ ] No unprotected re-init functions
- [ ] `initialize` validates all input parameters

### Pause / Circuit Breaker

- [ ] Emergency pause mechanism implemented via state flag in `instance()` storage
- [ ] Paused state blocks fund movement (e.g., reverting via `panic_with_error!`)
- [ ] Pause/unpause flows tested

### Admin Transfer

- [x] Ownership transfer is two-step (optional but recommended)
- [ ] Ownership transfer emits events
- [ ] Renounce ownership reviewed and justified

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
- **Unset behavior**: if neither address is configured the deducted amount stays
  inside the vault (balance is reduced but no token transfer occurs). This state
  is valid and explicitly documented—no funds are lost.
- Both addresses can only be changed by the admin in a single atomic storage
  write, ensuring no partial update is observable by other callers.

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
  - *Mitigation:* The pool does not rely on strict balance equality invariants for its core operations, mitigating balance-based operational griefing. Off-chain monitoring should track `receive_payment` events and native token transfers to reconcile expected vs. actual balances.

### Input Validation

- [ ] All amounts validated to be > 0
- [ ] Address/parameter validation on all public functions
- [ ] Boundary conditions tested (max values, zero values)
- [ ] Error messages provide clear context for debugging

### Event Logging

- [ ] All state changes emit appropriate events
- [ ] Event schema documented and indexed
- [ ] Critical operations (deposit, withdraw, deduct) logged with full context

### Testing Coverage

- [ ] Unit tests cover all public functions
- [ ] Edge cases and boundary conditions tested
- [ ] Panic scenarios tested with `#[should_panic]`
- [ ] Integration tests for complete user flows
- [ ] Minimum 95% test coverage maintained

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
