# PR: Budget persistent and instance TTL extensions by operation — Closes #1056

## Summary
Implement Budget persistent and instance TTL extensions by operation as a production-ready improvement in this repository. Persisted state needs clear key ownership, lifecycle, TTL, archival, and migration semantics.

## Traceability & Acceptance Criteria Mapping

| Acceptance Criterion | Implementation Details | Test Coverage |
|----------------------|------------------------|---------------|
| **1. Keys, ownership, lifecycle, TTL/archival, and cleanup semantics are explicit** | Explicitly defined `INSTANCE_BUMP_THRESHOLD` (~30 days) and `INSTANCE_BUMP_AMOUNT` (~60 days) along with `bump_instance_ttl` helper in `contracts/fee/src/lib.rs`. | `test_ttl_constants`, `test_init_bumps_instance_ttl` |
| **2. Reads and writes are consistent before, during, and after expiry, archival, restore, or migration** | Every mutating write path (`init`, `set_fee`, `deposit`, `withdraw`) and hot read path (`get_fee_config`, `get_accumulated`, `get_admin`) extends TTL on invocation. | `test_set_fee_bumps_instance_ttl`, `test_deposit_and_withdraw_bump_instance_ttl`, `test_read_paths_bump_instance_ttl` |
| **3. Cross-tenant and cross-module access cannot reach unrelated state** | State isolation guaranteed under Soroban storage tiering; contracts and accounts cannot access or mutate each other's storage partitions. | `test_isolation_between_contract_instances`, `test_unauthorized_fails` |
| **4. Tests cover fresh, hot, expired/archived, recovery, and isolation paths** | Comprehensive test suite added in `contracts/fee/src/test_ttl_bump.rs` verifying ledger advancement and TTL re-extension. | `contracts/fee/src/test_ttl_bump.rs` (all tests) |

## Security and Failure-Mode Handling
- Authorization via `require_auth` is preserved across all mutating entrypoints.
- Read paths remain side-effect free on state while extending TTL to prevent archival under continuous read load.
- Overflow and underflow protections remain enforced via `checked_add` and `checked_sub`.
