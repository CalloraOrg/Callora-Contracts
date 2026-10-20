//! Shared helpers for settlement unit tests.
//!
//! Soroban has no way to *shrink* an entry's TTL, so tests that need to age the
//! settlement contract below its bump threshold advance the ledger by ~90 days
//! instead. Any other contract they still need to call — the USDC token, for
//! example — would archive in the process, so its entries are extended past the
//! jump up front. Only the settlement contract is left to age out.

#![cfg(test)]

use crate::{PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec};

/// Keep `token`'s instance entry and `holder`'s balance entry alive across a
/// ledger jump that ages the settlement contract.
pub fn keep_token_state_alive(env: &Env, token: &Address, holder: &Address) {
    // The Stellar asset contract stores balances under `[Balance, address]`.
    let balance_key: Vec<Val> = Vec::from_array(
        env,
        [
            Symbol::new(env, "Balance").into_val(env),
            holder.clone().into_val(env),
        ],
    );
    env.as_contract(token, || {
        env.storage()
            .instance()
            .extend_ttl(PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().persistent().extend_ttl(
            &balance_key,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    });
}
