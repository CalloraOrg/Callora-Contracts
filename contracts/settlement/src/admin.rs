//! Admin-only developer balance recovery operations.

use soroban_sdk::{Address, Env, Vec};

use crate::{
    events, timelock, AdminMigrationEvent, CalloraSettlement, SettlementError, StorageKey,
};

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin = CalloraSettlement::get_admin(env.clone());
    if caller != &admin {
        env.panic_with_error(SettlementError::Unauthorized);
    }
}

pub(crate) fn propose_balance_migration(env: &Env, caller: &Address, from: &Address, to: &Address) {
    require_admin(env, caller);
    if from == to {
        env.panic_with_error(SettlementError::MigrationSameAddress);
    }
    if to == &env.current_contract_address() {
        env.panic_with_error(SettlementError::InvalidMigrationTarget);
    }

    let amount: i128 = env
        .storage()
        .persistent()
        .get(&StorageKey::DeveloperBalance(from.clone()))
        .unwrap_or(0);
    if amount <= 0 {
        env.panic_with_error(SettlementError::NoDeveloperBalance);
    }

    let proposed_at = env.ledger().timestamp();
    let execute_after = proposed_at
        .checked_add(timelock::DEVELOPER_MIGRATION_TIMELOCK_SECONDS)
        .unwrap_or_else(|| env.panic_with_error(SettlementError::TimelockOverflow));
    let migration = timelock::PendingDeveloperMigration {
        from: from.clone(),
        to: to.clone(),
        amount,
        proposed_at,
        execute_after,
    };
    timelock::set_pending_migration(env, &migration);

    env.events().publish(
        (events::event_admin_migration_proposed(env), from.clone()),
        migration,
    );
}

pub(crate) fn execute_balance_migration(env: &Env, caller: &Address, from: &Address) {
    require_admin(env, caller);
    let migration = timelock::get_pending_migration(env, from)
        .unwrap_or_else(|| env.panic_with_error(SettlementError::MigrationNotFound));
    let executed_at = env.ledger().timestamp();
    if executed_at < migration.execute_after {
        env.panic_with_error(SettlementError::TimelockNotExpired);
    }

    let source_balance: i128 = env
        .storage()
        .persistent()
        .get(&StorageKey::DeveloperBalance(from.clone()))
        .unwrap_or(0);
    let new_source_balance = source_balance
        .checked_sub(migration.amount)
        .filter(|balance| *balance >= 0)
        .unwrap_or_else(|| env.panic_with_error(SettlementError::MigrationBalanceChanged));
    let destination_balance: i128 = env
        .storage()
        .persistent()
        .get(&StorageKey::DeveloperBalance(migration.to.clone()))
        .unwrap_or(0);
    let new_destination_balance = destination_balance
        .checked_add(migration.amount)
        .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));

    let source_key = StorageKey::DeveloperBalance(from.clone());
    let destination_key = StorageKey::DeveloperBalance(migration.to.clone());
    env.storage()
        .persistent()
        .set(&source_key, &new_source_balance);
    env.storage()
        .persistent()
        .set(&destination_key, &new_destination_balance);
    env.storage()
        .persistent()
        .extend_ttl(&source_key, 50_000, 50_000);
    env.storage()
        .persistent()
        .extend_ttl(&destination_key, 50_000, 50_000);

    let mut index: Vec<Address> = env
        .storage()
        .instance()
        .get(&StorageKey::DeveloperIndex)
        .unwrap_or_else(|| Vec::new(env));
    CalloraSettlement::sorted_insert(env, &mut index, migration.to.clone());
    env.storage()
        .instance()
        .set(&StorageKey::DeveloperIndex, &index);
    timelock::remove_pending_migration(env, from);

    env.events().publish(
        (
            events::event_admin_migration(env),
            from.clone(),
            migration.to.clone(),
        ),
        AdminMigrationEvent {
            from: from.clone(),
            to: migration.to,
            amount: migration.amount,
            executed_at,
        },
    );
}
