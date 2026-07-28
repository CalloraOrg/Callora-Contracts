import re

with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

bad_init = '''    pub fn init(env: Env, vault: Address) {
        if env.storage().instance().has(&DataKey::Vault) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Vault, &vault);
        env.storage().instance().set(&DataKey::TotalSettled, &0i128);
    }'''

good_init = '''    pub fn init(env: Env, admin: Address, vault: Address) {
        if env.storage().instance().has(&DataKey::Vault) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Vault, &vault);
        env.storage().instance().set(&DataKey::TotalSettled, &0i128);
    }'''

content = content.replace(bad_init, good_init)

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)
