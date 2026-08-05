# Emergency Contract — Storage TTL Bump (issue #709)

Closes #709 — bump storage TTL on hot read paths in the emergency contract to
avoid archival pressure.

## Background

On Stellar/Soroban, every storage entry has a **time-to-live (TTL)** expressed
in ledgers. An entry whose TTL reaches zero is archived by the network; it can
no longer be read until a fee is paid to restore it. Write operations
automatically extend the TTL, but **read-only paths do not** — so a contract
that is frequently queried but seldom written will gradually approach archival.

The `callora-emergency` contract exposes several hot read-only entrypoints
(`capabilities`, `get_current`, `version`, `is_upgrade_authorised`). These
are polled by monitoring tools and off-chain clients on every block. Without
an explicit bump these calls would do nothing to prevent archival.

## Change Summary

All four hot read paths now call

```rust
env.storage()
    .instance()
    .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
```

before returning their value. The call is a no-op when the remaining TTL is
already above the threshold; it is only active when TTL has fallen below the
threshold, making it cheap in the common case.

### New public constants

| Constant | Value | Description |
|---|---|---|
| `LEDGERS_PER_DAY` | `17_280` | Ledgers/day at 5 s close cadence |
| `INSTANCE_BUMP_THRESHOLD` | `LEDGERS_PER_DAY * 30` | Remaining-TTL floor (~30 days) that triggers an extension |
| `INSTANCE_BUMP_AMOUNT` | `LEDGERS_PER_DAY * 60` | Target TTL after extension (~60 days) |
| `PERSISTENT_BUMP_THRESHOLD` | same as `INSTANCE_BUMP_THRESHOLD` | For future persistent-key use |
| `PERSISTENT_BUMP_AMOUNT` | same as `INSTANCE_BUMP_AMOUNT` | For future persistent-key use |

The constants are re-exported from the crate root (`callora-emergency`) so
integration tests and off-chain monitors can import them without reaching into
sub-modules.

### Changed entrypoints

| Entrypoint | Module | Change |
|---|---|---|
| `capabilities()` | `CalloraEmergency` / `views` | Bumps instance TTL before returning bitmap |
| `get_current()` | `EmergencyMigrate` / `migrate` | Bumps instance TTL before returning migrated state |
| `version()` | `EmergencyMigrate` / `migrate` | Bumps instance TTL before returning version |
| `is_upgrade_authorised()` | `EmergencyMigrate` / `migrate` | Bumps instance TTL before returning bool |

No function signatures changed. All four entrypoints remain no-auth view
functions; adding a bump call is not a breaking API change.

## Security Considerations

- The bump call uses `extend_ttl` (not `set_ttl`) — it never *reduces* TTL.
- No new storage keys are created.
- No authentication is required (all four paths were already no-auth views).
- No arithmetic is performed; there is no overflow risk.

## Testing

A dedicated test file `contracts/emergency/src/test_ttl_bump.rs` covers:

| Test | What it verifies |
|---|---|
| `ttl_constants_have_expected_values` | Constants match documented values |
| `capabilities_bumps_instance_ttl` | `capabilities()` resets TTL to `INSTANCE_BUMP_AMOUNT` |
| `capabilities_is_idempotent_across_calls` | Multiple calls return the same bitmap |
| `get_current_bumps_instance_ttl_before_migration` | Bump fires even when `None` is returned |
| `get_current_bumps_instance_ttl_after_migration` | Bump fires when migrated state is present |
| `version_bumps_instance_ttl` | `version()` resets TTL when initialised |
| `version_bumps_instance_ttl_before_init` | `version()` resets TTL even on uninitialised contract |
| `is_upgrade_authorised_bumps_instance_ttl_when_false` | Bump fires when returning `false` |
| `is_upgrade_authorised_bumps_instance_ttl_when_true` | Bump fires when returning `true` |
| `is_upgrade_authorised_bumps_ttl_when_uninitialised` | Bump fires on uninitialised contract |
| `repeated_view_calls_prevent_archival` | End-to-end: polling keeps contract alive past original TTL |

Run with:

```bash
cargo test -p callora-emergency
```
