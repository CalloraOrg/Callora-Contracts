use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Versioned topic prefix shared by all fee contract events.
const VERSION: Symbol = symbol_short!("v1");

/// Publishes `fee_set` with `(old_bps, new_bps)` as data.
pub fn fee_set(env: &Env, old_bps: u32, new_bps: u32) {
    env.events().publish(
        (VERSION, symbol_short!("fee_set")),
        (old_bps, new_bps),
    );
}

/// Publishes `admin_nominated` with `(current_admin, nominee)` as data.
pub fn admin_nominated(env: &Env, current_admin: Address, nominee: Address) {
    env.events().publish(
        (VERSION, symbol_short!("adm_nom")),
        (current_admin, nominee),
    );
}

/// Publishes `admin_accepted` with `(old_admin, new_admin)` as data.
pub fn admin_accepted(env: &Env, old_admin: Address, new_admin: Address) {
    env.events().publish(
        (VERSION, symbol_short!("adm_acc")),
        (old_admin, new_admin),
    );
}

/// Publishes `admin_cancelled` with `(admin, cancelled_nominee)` as data.
pub fn admin_cancelled(env: &Env, admin: Address, cancelled_nominee: Address) {
    env.events().publish(
        (VERSION, symbol_short!("adm_can")),
        (admin, cancelled_nominee),
    );
}
