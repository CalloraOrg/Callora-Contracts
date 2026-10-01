import re

with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

# Fix init
content = content.replace(
    'pub fn init(env: Env, vault: Address) {',
    'pub fn init(env: Env, admin: Address, vault: Address) {\n        env.storage().instance().set(&StorageKey::Admin, &admin);'
)

# Add migrate functions to the end of contractimpl
migrate_funcs = """

    pub fn migrate_v1_to_v2(env: Env, caller: Address) {
        migrate::migrate_v1_to_v2(&env, &caller);
    }

    pub fn migrate_v1_to_v2_page(
        env: Env,
        caller: Address,
        offset: u32,
        limit: u32,
    ) -> (u32, bool) {
        migrate::migrate_v1_to_v2_page(&env, &caller, offset, limit)
    }

    pub fn migration_storage_version(env: Env) -> u32 {
        migrate::migration_storage_version(&env)
    }
}
"""

content = re.sub(r'}\s*$', migrate_funcs, content)

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)
