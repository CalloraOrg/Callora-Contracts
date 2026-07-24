with open('contracts/revenue_pool/src/lib.rs', 'r') as f:
    content = f.read()

# Fix 1: add use soroban_sdk::testutils::storage::Instance as _
content = content.replace(
    'env.storage().instance().get_ttl()',
    'soroban_sdk::testutils::storage::Instance::get_ttl(&env.storage().instance())'
)

# Fix 2: add `version` method
version_method = """
    pub fn version(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }
"""

impl_end_idx = content.rfind('}')
if impl_end_idx != -1:
    content = content[:impl_end_idx] + version_method + content[impl_end_idx:]

with open('contracts/revenue_pool/src/lib.rs', 'w') as f:
    f.write(content)
