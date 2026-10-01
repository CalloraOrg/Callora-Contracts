# Revenue pool pause matrix

| Entry point | Regular pause | Emergency pause |
| --- | --- | --- |
| `distribute`, `batch_distribute` | blocked | blocked |
| `pause`, `unpause`, guardian administration | available as authorized | blocked until recovery |
| `propose_emergency_drain` | available to admin | available to admin |
| `execute_emergency_drain` | available to admin after the 24-hour timelock | available to admin after the 24-hour timelock |
| `cancel_emergency_drain` | available to admin | available to admin |
| `recover_from_emergency` | not applicable | available to admin |

Emergency pause is intended to stop distributions while preserving the
timelocked rescue path. Admin authorization and the drain timelock remain
unchanged.
