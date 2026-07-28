# Vault Critical-Action Cooldown

The vault applies one global cool-off window between successful executions of
its three timelocked critical admin actions:

- `execute_pause`
- `execute_upgrade`
- `execute_sweep`

The global design is intentional. Pause, upgrade, and sweep proposals use
independent slots and may mature together; a per-action cooldown would still
allow all three to execute back-to-back. The shared guard permits the first
execution and rejects subsequent critical executions with
`VaultError::AdminCooldownActive` until the window expires.

## Configuration and views

The default window is 3,600 seconds. An authenticated current admin may call
`set_admin_cooldown(caller, seconds)` with a value from 1 second through 30
days. Values outside this range return `VaultError::InvalidAdminCooldown`.

Read-only integrations can use:

- `get_admin_cooldown()` for the configured window
- `admin_cooldown_remaining()` for seconds until the next execution
- `is_admin_action_ready()` for a direct readiness check
- `get_last_critical_admin_action()` for the last action tag and timestamp

Cooldown arithmetic is saturating. The guard is armed only after authorization,
proposal lookup, timelock expiry, and action-specific validation succeed. If the
subsequent contract operation fails, Soroban transaction rollback also reverts
the cooldown record.
