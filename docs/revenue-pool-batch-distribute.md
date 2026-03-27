# Revenue Pool Batch Distribute

`callora-revenue-pool::batch_distribute` rejects an empty `payments` vector with
`"batch_distribute requires at least one payment"`.

Rationale:
- This matches the vault contract's `batch_deduct` policy for empty batches.
- Admin tooling gets an explicit failure for malformed payout jobs instead of a silent no-op.
- Indexers and operators do not need to infer whether an empty successful transaction was intentional.
