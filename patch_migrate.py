import re

with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

bad_migrate = '''    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_single_dev_v2(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }'''

good_migrate = '''    pub fn migrate_v1_to_v2(env: Env, caller: Address) {
        migrate::migrate_v1_to_v2(&env, &caller)
    }

    pub fn migrate_v1_to_v2_page(env: Env, caller: Address, start_index: u32, limit: u32) -> (u32, bool) {
        migrate::migrate_v1_to_v2_page(&env, &caller, start_index, limit)
    }

    pub fn migration_storage_version(env: Env) -> u32 {
        migrate::storage_version(&env)
    }'''

content = content.replace(bad_migrate, good_migrate)

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)
