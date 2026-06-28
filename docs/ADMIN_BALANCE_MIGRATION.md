# Admin developer balance migration

The settlement contract supports compliance recovery when a developer must move
their accrued balance to a replacement address. Recovery is a two-transaction,
admin-only workflow with a fixed 24-hour timelock.

## Workflow

1. Call `propose_balance_migration(admin, from, to)`.
2. Record and review the emitted `admin_migration_proposed` event.
3. Wait until the proposal's `execute_after` timestamp. The proposal can be
   queried with `get_balance_migration(from)`.
4. Call `execute_balance_migration(admin, from)`.
5. Verify the `admin_migration` event and both balances.

Both state-changing calls invoke `admin.require_auth()`. When `Admin` is a
Stellar multisig account, Stellar's account thresholds and signer weights are
therefore enforced for each transaction. Operators should configure the admin
account's medium threshold to match their governance policy.

## Security semantics

- A proposal snapshots the positive source balance. Credits arriving after the
  proposal remain at the source and require a later proposal.
- Re-proposing the same source replaces the proposal and restarts the full
  delay. This provides a safe correction path for an incorrect target.
- The destination may already have a balance; addition uses checked `i128`
  arithmetic. All writes are atomic under Soroban transaction semantics.
- Successful execution deletes the proposal, preventing replay.
- The settlement contract itself cannot be the destination, and source and
  destination must differ.
- If the source spends enough of the approved balance before execution,
  execution fails with `MigrationBalanceChanged`; governance must re-propose.

The migration changes internal settlement accounting only. It does not transfer
USDC on-ledger because those funds remain held by the settlement contract.

## Events

`admin_migration_proposed` uses topics `(event, from)` and stores the complete
`PendingDeveloperMigration` as event data.

`admin_migration` uses topics `(event, from, to)` and stores
`AdminMigrationEvent { from, to, amount, executed_at }` as event data.
