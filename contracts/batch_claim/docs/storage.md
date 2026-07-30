# Batch Claim Storage Documentation

This document cannot currently document the storage layout for `batch_claim` because no `contracts/batch_claim` contract or source implementation exists in this repository.

## Repository status

The repository does not currently contain:

- A `contracts/batch_claim` directory
- A batch-claim contract implementation
- Storage key definitions for `Admin`, `Token`, `Batch`, or `Claimed(address)`
- Tests or public entrypoints for `batch_claim`

Consequently, storage tiers and rationales cannot be verified without inventing an API and storage layout. In particular, this document must not assert that those keys exist or that they use Instance or Persistent storage until the contract implementation is added or its source location is provided.

## Required follow-up

Once the `batch_claim` implementation is present, document every actual storage key with:

| Logical key | Tier | Stored value | Rationale |
|---|---|---|---|
| Implementation-defined | Instance, Persistent, or Temporary | Implementation-defined | Explain the lifecycle, access pattern, TTL requirements, and security rationale. |

The completed documentation should also state whether claim records require persistent TTL management, whether temporary storage is used, and how failed claims preserve authorization, accounting, and replay-prevention invariants.

## API impact

No `batch_claim` API or storage layout is currently present in this repository, so no API-visible change can be documented at this time.
