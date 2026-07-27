use soroban_sdk::{Env, Symbol};

/// Event topic for contract initialization.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Event topic emitted when an offering is registered.
pub fn event_offering_registered(env: &Env) -> Symbol {
    Symbol::new(env, "offering_registered")
}
