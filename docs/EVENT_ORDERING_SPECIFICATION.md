# Event Ordering Specification for Callora Lifecycle Transitions

## Overview

This document specifies the guarantees and mechanisms that ensure deterministic identity, ordering, payload shape, and compatibility for all lifecycle events emitted by Callora contracts.

Indexers and downstream consumers depend on:
1. **Deterministic event identity** — event topics must be stable and globally unique
2. **Deterministic ordering** — lifecycle events must publish in a predictable, reconstruct-able order
3. **Deterministic payload shape** — event payloads must have a consistent, versioned schema
4. **Compatibility** — consumers must be able to handle multiple versions without breaking
5. **Idempotency** — retries and duplicates must not create contradictory observations

## Canonical Event Structure

All Callora lifecycle events follow the same 2-topic, single-payload pattern:

```text
topics: [action: Symbol, subject: Address]
data:   <versioned event payload>
```

Where:
- `topic[0]` = the event action (e.g., "admin_init", "checkpoint_created")
- `topic[1]` = the primary actor or subject (caller, admin, account, etc.)
- `data` = event-specific payload struct carrying fields relevant to the transition

This canonical shape enables:
- Off-chain filtering by action or subject
- Deterministic topic construction
- Consistent indexer implementation across all event types

## Event Sequencing Mechanism

### Sequence Numbers for Lifecycle Ordering

Each contract maintains a **monotonically increasing event sequence counter** to guarantee ordering of lifecycle transitions:

```rust
/// Storage key for event sequence counter
const EVENT_SEQUENCE_KEY: &str = "event_seq";

/// Increment and return the next sequence number
fn next_event_sequence(env: &Env) -> u64 {
    let inst = env.storage().instance();
    let seq: u64 = inst.get(&Symbol::new(env, EVENT_SEQUENCE_KEY)).unwrap_or(0);
    let next = seq.checked_add(1).expect("sequence overflow");
    inst.set(&Symbol::new(env, EVENT_SEQUENCE_KEY), &next);
    next
}
```

### inclusion in Event Payloads

Lifecycle event payloads include a `sequence: u64` field that captures the sequence number at emission time. This field:
- Is immutable once emitted (captured at event time)
- Allows reconstruction of event order even if ledger events are delivered out-of-order
- Prevents duplicates from creating contradictory observations (same sequence = same event)

Example:
```rust
#[derive(Clone, Debug)]
pub struct LifecycleEventPayload {
    /// Monotonically increasing sequence number for ordering
    pub sequence: u64,
    /// Schema version for this event type
    pub version: u32,
    /// Event-specific fields...
}
```

## Version Numbers for Compatibility

### Schema Versioning

Each event type has a **stable, version-aware schema** to handle compatibility:

- `version: u32` field in every lifecycle event payload
- Incrementing version on breaking changes (new required fields, type changes)
- Consumers must handle multiple versions in-flight simultaneously
- New versions are backwards-compatible with respect to field order (only additions)

### Migration Path

When a new version of an event is introduced:
1. The event is published with the new `version` number
2. Old consumers see `version >= 2` and may skip or handle via fallback logic
3. Consumers are updated to understand new versions
4. After a grace period, old version consumers are deprecated

## Deterministic Topic Strings

### Topic Construction

All event topics are constructed from **fixed, pre-defined string literals** defined in `contracts/*/src/events.rs`:

```rust
pub fn event_admin_init(env: &Env) -> Symbol {
    Symbol::new(env, "admin_init")
}
```

This ensures:
- Topics are byte-identical across all invocations
- No runtime string construction or interpolation
- Prevents accidental topic drift or typos
- Indexers can hard-code topic filters

### Topic Naming Convention

Topics follow the pattern: `{contract}_{state}` or `{contract}_{action}`:
- `admin_init` — admin contract initialization
- `admin_nominated` — admin nomination proposed
- `admin_changed` — admin role transferred
- `checkpoint_created` — checkpoint record created
- `distribute_started` — distribution operation begun
- `distribute_completed` — distribution operation finished

## Event Ordering Guarantees

### Within-Transaction Ordering

Events emitted within a single transaction are ordered by:
1. **Publication order** — the order `env.events().publish()` is called
2. **Sequence number** — the monotonically increasing counter captured at each event
3. **Ledger entry** — part of the transaction's deterministic ledger state

### Cross-Transaction Ordering

For lifecycle transitions spanning multiple transactions (e.g., admin transfer: nominate → accept):
- Each transaction publishes one or more lifecycle events
- Events are ordered by transaction sequence and then by publication order
- The sequence number within each contract acts as a tiebreaker
- Indexers reconstruct the full lifecycle by collecting events and sorting by sequence

### Idempotency and Durability

If a contract call is retried (due to network failure, timeout, etc.):
- The sequence counter is incremented only on successful transaction commit
- Retried calls with identical inputs DO NOT emit duplicate events
- If a retry succeeds with different inputs, a new event is published with a new sequence number
- Indexers can detect retries by unchanged sequence numbers in a fixed time window

## Payload Determinism

### Immutable Payload Fields

Every lifecycle event payload includes:
- `sequence: u64` — immutable counter value at event time
- `version: u32` — immutable schema version
- `timestamp: u64` — immutable ledger timestamp (optional, for audit trails)
- Event-specific fields (caller, subject, state change details)

None of these fields are computed or derived; they are captured at event time and never modified.

### No Serialization Variations

All payloads are serialized using Soroban's canonical XDR encoding:
- Deterministic field ordering (struct definition order)
- No dynamic dispatch or polymorphism
- Type safety enforced at compile time
- Indexers expect exact XDR byte sequences

## Testing Event Ordering

### Test Categories

1. **Topic Testing** — verify topic strings are stable and unique
2. **Payload Testing** — verify payload structures have correct fields and types
3. **Sequence Testing** — verify events are emitted in expected order
4. **Replay Testing** — verify retries do not create duplicate events
5. **Compatibility Testing** — verify new versions do not break old consumers
6. **Lifecycle Testing** — verify full state transitions emit correct event sequences

### Example: Admin Rotation Lifecycle Test

```rust
#[test]
fn full_rotation_emits_ordered_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &new_admin);
    warp_past_timelock(&env);
    admin::accept_admin(&env, &new_admin);
    
    // Collect all events
    let events = env.events().all();
    let admin_events: Vec<_> = events
        .iter()
        .filter(|e| is_admin_lifecycle_event(&e))
        .collect();
    
    // Verify event sequence and ordering
    assert_eq!(admin_events.len(), 3);
    assert_eq!(event_sequence(&admin_events[0]), 1);
    assert_eq!(event_sequence(&admin_events[1]), 2);
    assert_eq!(event_sequence(&admin_events[2]), 3);
    
    assert_eq!(event_topic(&admin_events[0]), "admin_init");
    assert_eq!(event_topic(&admin_events[1]), "admin_nominated");
    assert_eq!(event_topic(&admin_events[2]), "admin_changed");
}
```

## Implementation Checklist

- [ ] Add `event_sequence` counter to contract instance storage
- [ ] Add `sequence: u32` field to all lifecycle event payloads
- [ ] Add `version: u32` field to all lifecycle event payloads
- [ ] Update event emission to capture sequence at event time
- [ ] Add snapshot tests for topic strings
- [ ] Add payload structure tests (version, sequence fields present)
- [ ] Add ordering tests for major lifecycle transitions
- [ ] Add retry/idempotency tests
- [ ] Add compatibility tests for version handling
- [ ] Document event topics and payloads in `docs/EVENTS_INDEX.md`
- [ ] Update README with event ordering guarantees

## References

- Issue #1053: Guarantee event ordering for lifecycle transitions
- [EVENT_TOPICS.md](EVENT_TOPICS.md) — listing of all event topics by contract
- [EVENTS_INDEX.md](EVENTS_INDEX.md) — detailed event schema documentation
- Soroban Events: https://soroban.stellar.org/docs

