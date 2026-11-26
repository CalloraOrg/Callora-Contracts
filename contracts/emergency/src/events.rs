//! Event helpers for the emergency pause coordinator.

use soroban_sdk::{Address, Env, Symbol};

use crate::PauseReport;

pub fn event_initialized(env: &Env) -> Symbol {
    Symbol::new(env, "emergency_initialized")
}

pub fn emit_initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((event_initialized(env), admin.clone()), admin.clone());
}

pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

pub fn emit_admin_nominated(env: &Env, current_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (event_admin_nominated(env), current_admin.clone()),
        new_admin.clone(),
    );
}

pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

pub fn emit_admin_accepted(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (event_admin_accepted(env), old_admin.clone()),
        new_admin.clone(),
    );
}

pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

pub fn emit_admin_cancelled(env: &Env, admin: &Address) {
    env.events()
        .publish((event_admin_cancelled(env), admin.clone()), ());
}

pub fn event_guardian_set(env: &Env) -> Symbol {
    Symbol::new(env, "guardian_set")
}

pub fn emit_guardian_set(env: &Env, admin: &Address, guardian: &Address) {
    env.events().publish(
        (event_guardian_set(env), admin.clone()),
        guardian.clone(),
    );
}

pub fn event_guardian_cleared(env: &Env) -> Symbol {
    Symbol::new(env, "guardian_cleared")
}

pub fn emit_guardian_cleared(env: &Env, admin: &Address) {
    env.events()
        .publish((event_guardian_cleared(env), admin.clone()), ());
}

pub fn event_contract_registered(env: &Env) -> Symbol {
    Symbol::new(env, "contract_registered")
}

pub fn emit_contract_registered(env: &Env, admin: &Address, target: &Address) {
    env.events()
        .publish((event_contract_registered(env), admin.clone()), target.clone());
}

pub fn event_contract_deregistered(env: &Env) -> Symbol {
    Symbol::new(env, "contract_deregistered")
}

pub fn emit_contract_deregistered(env: &Env, admin: &Address, target: &Address) {
    env.events().publish(
        (event_contract_deregistered(env), admin.clone()),
        target.clone(),
    );
}

pub fn event_pause_all_executed(env: &Env) -> Symbol {
    Symbol::new(env, "pause_all_executed")
}

pub fn emit_pause_all_executed(env: &Env, caller: &Address, report: &PauseReport) {
    env.events()
        .publish((event_pause_all_executed(env), caller.clone()), report.clone());
}

pub fn event_unpause_all_executed(env: &Env) -> Symbol {
    Symbol::new(env, "unpause_all_executed")
}

pub fn emit_unpause_all_executed(env: &Env, caller: &Address, report: &PauseReport) {
    env.events().publish(
        (event_unpause_all_executed(env), caller.clone()),
        report.clone(),
    );
}
