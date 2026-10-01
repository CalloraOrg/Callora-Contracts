# Callora Checkpoint Smart Contract Fuzz Testing

This directory contains the `cargo-fuzz` target for testing the `CalloraCheckpoint` smart contract in `contracts/checkpoint`.

## Overview

The `checkpoint` fuzzer continuously generates randomized inputs and sequences of operation tokens to exercise the contract's public surface, state transitions, boundary conditions, serialization/deserialization, range queries, TTL bumping, and two-step admin rotation.

## Invariants Verified

The fuzz target enforces the following invariants during execution:

1. **Monotonic & Sequential IDs**: `NextCheckpointId` and `CheckpointCount` increment sequentially starting from 1 with each created record and never decrease.
2. **Immutability of Records**: Once created, a `CheckpointRecord` for a given ID is never altered by subsequent single, batch, range, or TTL operations.
3. **Non-Negative Balance Safety**: Any attempt to record a negative balance in `create_checkpoint` or `batch_create_checkpoints` is rejected with `CheckpointError::AmountNegative`.
4. **Batch Bounds & Atomic Rollback**: `batch_create_checkpoints` enforces `1 <= items.len() <= MAX_BATCH_SIZE (50)`. If any balance item in a batch is invalid or negative, the transaction fails atomically without mutating storage or ID counters.
5. **Paginated Range Bounds**: `get_checkpoints_range` rejects a page limit of `0`, returns an empty list for `start_id > count`, and caps returned records to `min(limit, MAX_PAGE_SIZE (100))`.
6. **Authentication & Authorization**: All state-changing entrypoints require proper authorization (`require_auth` / `require_admin`).
7. **View Method Non-Mutation**: View methods never alter contract storage or counter states.
8. **Panic Safety**: All input sequences are handled gracefully without unexpected runtime panics or process crashes.

## Prerequisites

Install `cargo-fuzz` (requires Rust Nightly):

```bash
cargo install cargo-fuzz
```

## Running the Fuzz Target

To execute the fuzz target:

```bash
# Navigate to the fuzz package directory
cd contracts/checkpoint/fuzz

# Run the checkpoint target
cargo fuzz run checkpoint

# Alternatively, run via target name 'main'
cargo fuzz run main
```

To run for a specific number of iterations:

```bash
cargo fuzz run checkpoint -- -runs=10000
```

To run with sanitizers:

```bash
cargo fuzz run checkpoint -- -sanitizer=address
```

## Structure

- `Cargo.toml`: Package configuration defining `callora-checkpoint-fuzz`.
- `targets/main.rs`: The state-machine fuzz harness.
- `corpus/`: Corpus directory storing seed inputs.
