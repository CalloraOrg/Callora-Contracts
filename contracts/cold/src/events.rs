use soroban_sdk::{Env, Symbol};

pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

pub fn event_config_set(env: &Env) -> Symbol {
    Symbol::new(env, "config_set")
}

pub fn event_balances_set(env: &Env) -> Symbol {
    Symbol::new(env, "balances_set")
}

pub fn event_cold_sweep_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_proposed")
}

pub fn event_cold_sweep_approved(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_approved")
}

pub fn event_cold_sweep_executed(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_executed")
}

pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}
