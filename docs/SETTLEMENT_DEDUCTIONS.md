# Settlement deduction batches

`settlement.record_deductions` accepts a bounded batch and records all deductions in one authorized cross-contract call. Validation failure leaves every deduction unapplied.
