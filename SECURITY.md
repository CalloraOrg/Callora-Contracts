# Security

This document outlines security best practices and checklist items for Callora vault contracts to improve audit readiness and reviewer confidence.

## 🔐 Vault Security Checklist

### Access Control

- [ ] All privileged functions protected by onlyOwner / role-based access
- [ ] No public or external access to admin functions
- [ ] Ownership transfer tested and documented

### Arithmetic Safety

- [ ] No integer overflow/underflow possible
- [ ] Solidity ^0.8.x overflow checks relied upon or SafeMath used where required
- [ ] For Soroban/Rust: `checked_add` / `checked_sub` used for all balance mutations
- [ ] `overflow-checks` enabled in both dev and release profiles

### Initialization / Re-initialization

- [ ] Initializer protected against multiple calls
- [ ] Upgradeable patterns use initializer guards
- [ ] No unprotected re-init functions
- [ ] `init` validates all input parameters (rejects negative values where appropriate)

### Pause / Circuit Breaker

- [ ] Emergency pause mechanism implemented
- [ ] Paused state blocks fund movement
- [ ] Pause/unpause flows tested

### Ownership Transfer

- [ ] Ownership transfer is two-step (optional but recommended)
- [ ] Ownership transfer emits events
- [ ] Renounce ownership reviewed and justified

### External Calls

- [ ] Checks-effects-interactions pattern followed
- [ ] Reentrancy protection where external calls exist
- [ ] No untrusted delegatecalls
- [ ] Token transfers use safe transfer patterns

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

- [ ] WASM compilation verified and reproducible
- [ ] Stellar network parameters validated (fees, limits)
- [ ] Cross-contract call security reviewed
- [ ] Storage patterns optimized and secure

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
