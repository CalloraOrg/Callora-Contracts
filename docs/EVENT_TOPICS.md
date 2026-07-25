# Deterministic Event Topic Catalog

This document is the **canonical, machine-readable catalog** of every event
topic emitted by the Callora smart contracts (`vault`, `settlement`,
`revenue_pool`). It is designed for indexer integrators who need to filter,
subscribe to, or decode Soroban events by topic.

> **Determinism guarantee.** Every topic string listed below is produced by a
> centralized `events::event_*()` constructor in the contract's `events.rs`
> module. The string is passed to `Symbol::new(env, "<topic>")` exactly once
> per constructor, and snapshot-tested to byte-identity. No inline
> `Symbol::new(...)` literals appear at publish call sites (enforced by
> `scripts/check-event-shape.sh`).

## How Soroban Events Work

Each `env.events().publish((topics), data)` call produces a ledger entry with:

- **Topics**: an ordered array of `ScVal` values (typically `Symbol` or
  `Address`). Indexers subscribe and filter by topic position.
- **Data**: a single `ScVal` carrying the event payload (struct, tuple, or
  scalar).

**Topic positions matter.** An indexer filtering on topic[0] = `"deposit"`
must match the exact byte representation of the Symbol. The constructors below
guarantee that representation is stable across contract versions.

## Cross-Contract Topic Overlap

Some topic strings (e.g. `"init"`, `"upgraded"`, `"admin_cancelled"`,
`"distribute"`) are shared across multiple contracts. To disambiguate:

1. **Filter by contract address first.** Soroban event subscriptions are
   already scoped per-contract by the emitting contract's address.
2. **Use topic[1] (subject) for caller identification.** Most events include
   the caller/admin address as topic[1].
3. **Use the data payload for full context.** Structured data fields (amounts,
   balances, flags) distinguish semantically different events that share a
   topic name.

## Topic Shape Reference

The target 3-topic shape (see `EVENTS_INDEX.md`) is:

| Position | Name       | Type     | Description                                     |
|----------|------------|----------|-------------------------------------------------|
| Topic 0  | `action`   | `Symbol` | Event name (e.g. `"deposit"`, `"deduct"`)       |
| Topic 1  | `subject`  | `Address`| Primary address (caller, owner, admin, etc.)    |
| Topic 2  | (varies)   | `Address`/`Symbol`| Additional context (recipient, request_id) |

Most existing events use 2 topics `(action, subject)`. Some events (e.g.
`deduct`, `withdraw_to`, `metadata_set`) include a third topic. See
[`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md) for per-event payload details.

---

## Vault Contract (`callora-vault`)

Source: [`contracts/vault/src/events.rs`](../contracts/vault/src/events.rs)

| #  | Topic String              | Constructor                    | Trigger                                       |
|----|---------------------------|--------------------------------|-----------------------------------------------|
| 1  | `init`                    | `event_init`                   | Vault initialization                          |
| 2  | `admin_nominated`         | `event_admin_nominated`        | Owner nominates a new admin                   |
| 3  | `admin_accepted`          | `event_admin_accepted`         | Nominated admin accepts                       |
| 4  | `admin_cancelled`         | `event_admin_cancelled`        | Admin cancels pending transfer                |
| 5  | `set_authorized_caller`   | `event_set_authorized_caller`  | Owner updates authorized caller               |
| 6  | `set_max_deduct`          | `event_set_max_deduct`         | Owner updates max deduct amount               |
| 7  | `vault_paused`            | `event_vault_paused`           | Vault paused (circuit-breaker)                |
| 8  | `vault_unpaused`          | `event_vault_unpaused`         | Vault unpaused                                |
| 9  | `deposit`                 | `event_deposit`                | USDC deposited into vault                     |
| 10 | `deduct`                  | `event_deduct`                 | Funds deducted from vault                     |
| 11 | `ownership_nominated`     | `event_ownership_nominated`    | Owner nominates new owner                     |
| 12 | `ownership_accepted`      | `event_ownership_accepted`     | Nominated owner accepts                       |
| 13 | `withdraw`                | `event_withdraw`               | Owner withdraws to self                       |
| 14 | `withdraw_to`             | `event_withdraw_to`            | Owner withdraws to recipient                  |
| 15 | `distribute`              | `event_distribute`             | Admin distributes funds                       |
| 16 | `set_revenue_pool`        | `event_set_revenue_pool`       | Owner configures revenue pool address         |
| 17 | `clear_revenue_pool`      | `event_clear_revenue_pool`     | Owner clears revenue pool address             |
| 18 | `set_settlement`          | `event_set_settlement`         | Admin sets settlement contract address        |
| 19 | `metadata_set`            | `event_metadata_set`           | Offering metadata stored                      |
| 20 | `price_set`               | `event_price_set`              | Offering price set                            |
| 21 | `price_removed`           | `event_price_removed`          | Offering price removed                        |
| 22 | `metadata_updated`        | `event_metadata_updated`       | Offering metadata replaced                    |
| 23 | `metadata_removed`        | `event_metadata_removed`       | Offering metadata deleted                     |
| 24 | `upgraded`                | `event_upgraded`               | Contract upgraded (legacy)                    |
| 25 | `upgrade_started`         | `event_upgrade_started`        | Upgrade begins (pre-WASM-swap)                |
| 26 | `upgrade_completed`       | `event_upgrade_completed`      | Upgrade completes (post-WASM-swap)            |
| 27 | `allowlist_add`           | `event_allowlist_add`          | Address added to deposit allowlist            |
| 28 | `allowlist_clear`         | `event_allowlist_clear`        | Deposit allowlist cleared                     |
| 29 | `revenue_pool_proposed`   | `event_revenue_pool_proposed`  | New revenue pool proposed                     |
| 30 | `revenue_pool_accepted`   | `event_revenue_pool_accepted`  | Proposed revenue pool accepted                |
| 31 | `revenue_pool_cancelled`  | `event_revenue_pool_cancelled` | Revenue pool proposal cancelled               |
| 32 | `request_id_pruned`       | `event_request_id_pruned`      | Expired idempotency request ID pruned         |
| 33 | `admin_broadcast`         | `event_admin_broadcast`        | Admin emergency broadcast                     |
| 34 | `reserve_cap_set`         | `event_reserve_cap_set`        | Token reserve cap set or updated              |
| 35 | `rescue_funds`            | `event_rescue_funds`           | Admin rescues accidentally sent tokens        |
| 36 | `swept`                   | `event_swept`                  | Owner sweeps surplus USDC to sibling contract |
| 37 | `pause_proposed`          | `event_pause_proposed`         | Admin stages a timelocked pause proposal      |
| 38 | `pause_executed`          | `event_pause_executed`         | Timelocked pause proposal executed            |
| 39 | `pause_cancelled`         | `event_pause_cancelled`        | Pending pause proposal cancelled              |
| 40 | `upgrade_proposed`        | `event_upgrade_proposed`       | Admin stages a timelocked upgrade proposal    |
| 41 | `upgrade_executed`        | `event_upgrade_executed`       | Timelocked upgrade proposal executed          |
| 42 | `upgrade_cancelled`       | `event_upgrade_cancelled`      | Pending upgrade proposal cancelled            |
| 43 | `sweep_proposed`          | `event_sweep_proposed`         | Admin stages a timelocked sweep proposal      |
| 44 | `sweep_executed`          | `event_sweep_executed`         | Timelocked sweep proposal executed            |
| 45 | `sweep_cancelled`         | `event_sweep_cancelled`        | Pending sweep proposal cancelled              |
| 46 | `tl_window_changed`       | `event_timelock_window_changed` | Timelock window length updated               |

**Total: 46 topics**

---

## Settlement Contract (`callora-settlement`)

Source: [`contracts/settlement/src/events.rs`](../contracts/settlement/src/events.rs)

| #  | Topic String                 | Constructor                        | Trigger                                        |
|----|------------------------------|------------------------------------|------------------------------------------------|
| 1  | `payment_received`           | `event_payment_received`           | Inbound payment from vault or admin            |
| 2  | `balance_credited`           | `event_balance_credited`           | Developer balance incremented                  |
| 3  | `developer_withdraw`         | `event_developer_withdraw`         | Developer withdraws accrued balance            |
| 4  | `daily_withdraw_cap_changed` | `event_daily_withdraw_cap_changed` | Developer daily withdrawal cap updated         |
| 5  | `claim_window_changed`       | `event_developer_claim_window_changed` | Developer claim window updated            |
| 6  | `admin_nominated`            | `event_admin_nominated`            | Current admin nominates successor              |
| 7  | `admin_accepted`             | `event_admin_accepted`             | Pending admin accepts role                     |
| 8  | `admin_cancelled`            | `event_admin_cancelled`            | Admin cancels pending transfer                 |
| 9  | `vault_proposed`             | `event_vault_proposed`             | Admin proposes new vault address               |
| 10 | `vault_accepted`             | `event_vault_accepted`             | Proposed vault accepts rotation                |
| 11 | `upgraded`                   | `event_upgraded`                   | Contract upgraded to new WASM                  |
| 12 | `developer_force_credited`   | `event_developer_force_credited`   | Admin force-credits developer balance          |
| 13 | `admin_broadcast`            | `event_admin_broadcast`            | Admin emergency broadcast                      |
| 14 | `admin_migration_proposed`   | `event_admin_migration_proposed`   | Developer balance migration proposed           |
| 15 | `admin_migration`            | `event_admin_migration`            | Developer balance migration executed           |
| 16 | `deposit`                    | `event_deposit`                    | Deposit made for a developer                   |
| 17 | `developer_min_balance_changed` | `event_developer_min_balance_changed` | Developer minimum balance threshold updated |

**Total: 17 topics**

---

## Revenue Pool Contract (`callora-revenue-pool`)

Source: [`contracts/revenue_pool/src/events.rs`](../contracts/revenue_pool/src/events.rs)

| #  | Topic String                    | Constructor                           | Trigger                                      |
|----|---------------------------------|---------------------------------------|----------------------------------------------|
| 1  | `init`                          | `event_init`                          | Revenue pool initialization                  |
| 2  | `admin_changed`                 | `event_admin_changed`                 | Admin change recorded (pre-transfer)         |
| 3  | `admin_transfer_started`        | `event_admin_transfer_started`        | Admin nominates successor                    |
| 4  | `admin_transfer_completed`      | `event_admin_transfer_completed`      | Pending admin accepts role                   |
| 5  | `admin_cancelled`               | `event_admin_cancelled`               | Admin cancels pending transfer               |
| 6  | `pause_guardian_set`            | `event_pause_guardian_set`            | Emergency pause guardian set                 |
| 7  | `pause_guardian_cleared`        | `event_pause_guardian_cleared`        | Emergency pause guardian cleared             |
| 8  | `pause_set`                     | `event_pause_set`                     | Pool pause state toggled                     |
| 9  | `receive_payment`               | `event_receive_payment`               | Inbound payment logged                       |
| 10 | `yield_deposited`               | `event_yield_deposited`               | Treasury deposits protocol yield             |
| 11 | `treasury_transfer_started`     | `event_treasury_transfer_started`     | Treasury role nominated                      |
| 12 | `treasury_transfer_completed`   | `event_treasury_transfer_completed`   | Treasury accepts role                        |
| 13 | `treasury_cancelled`            | `event_treasury_cancelled`            | Treasury nomination cancelled                |
| 14 | `set_max_distribute`            | `event_set_max_distribute`            | Per-leg distribution cap updated             |
| 15 | `distribute`                    | `event_distribute`                    | USDC distributed to a single developer       |
| 16 | `batch_distribute`              | `event_batch_distribute`              | One payment leg in a batch distribution      |
| 17 | `upgraded`                      | `event_upgraded`                      | Contract upgraded to new WASM                |
| 18 | `admin_broadcast`               | `event_admin_broadcast`               | Admin emergency broadcast                    |
| 19 | `emergency_drain_proposed`      | `event_emergency_drain_proposed`      | Timelocked emergency drain proposed          |
| 20 | `emergency_drain_executed`      | `event_emergency_drain_executed`      | Pending emergency drain executed             |
| 21 | `emergency_drain_cancelled`     | `event_emergency_drain_cancelled`     | Pending emergency drain cancelled            |

**Total: 21 topics**

---

## Indexer Quick-Reference

Subscribe by contract address + topic[0]:

```text
Vault:        GCONTRACT_VAULT...
Settlement:   GCONTRACT_SETTLEMENT...
RevenuePool:  GCONTRACT_REVENUE_POOL...
```

### Topic[0] Filter Patterns

**Vault-specific** (not shared with other contracts):
`deduct`, `deposit`, `vault_paused`, `vault_unpaused`,
`ownership_nominated`, `ownership_accepted`, `withdraw`, `withdraw_to`,
`set_authorized_caller`, `set_max_deduct`, `set_revenue_pool`,
`clear_revenue_pool`, `set_settlement`, `metadata_set`, `metadata_updated`,
`metadata_removed`, `price_set`, `price_removed`, `allowlist_add`,
`allowlist_clear`, `revenue_pool_proposed`, `revenue_pool_accepted`,
`revenue_pool_cancelled`, `request_id_pruned`, `reserve_cap_set`,
`rescue_funds`, `swept`

**Settlement-specific** (not shared with other contracts):
`payment_received`, `balance_credited`, `developer_withdraw`,
`daily_withdraw_cap_changed`, `claim_window_changed`, `vault_proposed`,
`vault_accepted`, `developer_force_credited`, `admin_migration_proposed`,
`admin_migration`

**Revenue Pool-specific** (not shared with other contracts):
`admin_changed`, `admin_transfer_started`, `admin_transfer_completed`,
`pause_guardian_set`, `pause_guardian_cleared`, `pause_set`,
`receive_payment`, `yield_deposited`, `treasury_transfer_started`,
`treasury_transfer_completed`, `treasury_cancelled`, `set_max_distribute`,
`batch_distribute`, `emergency_drain_proposed`, `emergency_drain_executed`,
`emergency_drain_cancelled`

**Shared across contracts** (disambiguate by contract address):
`init`, `upgraded`, `admin_nominated`, `admin_accepted`, `admin_cancelled`,
`admin_broadcast`, `distribute`

---

## Summary Statistics

| Contract      | Topics | Unique (not shared) | Shared |
|---------------|--------|---------------------|--------|
| vault         | 36     | 27                  | 9      |
| settlement    | 16     | 10                  | 6      |
| revenue_pool  | 21     | 15                  | 6      |
| **Total**     | **73** | **52**              | **21** |

> Shared count: each unique topic string that appears in more than one
> contract is counted once per contract it appears in. The 9 shared topic
> strings across all contracts are: `init`, `upgraded`, `admin_nominated`,
> `admin_accepted`, `admin_cancelled`, `admin_broadcast`, `distribute`,
> `vault_paused`/`vault_unpaused` (vault only), and `deposit` (vault +
> settlement).

---

## Adding a New Event

1. Add `pub fn event_<name>(env: &Env) -> Symbol` to the contract's
   `events.rs` with a rustdoc comment.
2. Add a snapshot test in `events.rs`'s `#[cfg(test)] mod tests` asserting
   byte-identity to the literal string.
3. Publish via `events::event_<name>(&env)` — never an inline
   `Symbol::new(...)`.
4. Add a row to the relevant table in this document.
5. Run `./scripts/check-event-shape.sh` before opening the PR.

## References

- [`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md) — Full per-event payload schemas
- [`EVENTS_INDEX.md`](EVENTS_INDEX.md) — Structured events index with
  backwards-compatibility ladder
- [`scripts/check-event-shape.sh`](../scripts/check-event-shape.sh) — CI gate
  enforcing centralized topic constructors
