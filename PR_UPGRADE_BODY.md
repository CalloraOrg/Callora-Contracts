Adds admin-gated `upgrade(env, caller, new_wasm_hash)` and `version()` view; persists version and emits `upgraded` event. Includes tests and `UPGRADE.md` documentation.

### Testing
Please run the following command and verify the output:
`cargo test -p callora-revenue-pool`

### Checklist
- [x] Upgrade requires admin authorization
- [x] `version()` returns the stored WASM hash
- [x] `upgraded` event emitted with correct topics
- [x] All existing tests pass