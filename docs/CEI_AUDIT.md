# CEI Audit Report

## Findings and Violations
1. **callora-vault (`deposit`)**: Token transfer interaction occurred before updating local storage balance. Fixed by performing storage updates prior to the token transfer call.
2. **callora-vault (`deduct` / `batch_deduct`)**: External contract settlement/token routing was initiated before mutating the user balance state. Fixed by executing checked internal state subtraction before cross-contract interactions.

## Verification
- Applied strict Checks-Effects-Interactions ordering across all state-mutating entrypoints.
- Added explicit state-mutation unit tests to ensure adherence to execution order.
