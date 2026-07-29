import re

with open('contracts/revenue_pool/src/lib.rs', 'r') as f:
    current = f.read()

with open('/tmp/7e15499_revenue_pool.rs', 'r') as f:
    old = f.read()

# Extract from pub fn propose_emergency_drain to the end of the impl block
start_idx = current.find('    pub fn propose_emergency_drain')
if start_idx == -1:
    print("Could not find propose_emergency_drain")
    exit(1)

# Find the end of the file in current
end_idx = current.rfind('}')
if end_idx == -1:
    print("Could not find end of impl block")
    exit(1)

emergency_funcs = current[start_idx:end_idx]

# Replace use statement to include emergency and events modules
old = old.replace('use soroban_sdk::{', 'pub mod emergency;\npub mod events;\n\nuse soroban_sdk::{')

# Add constants
old = old.replace('const ADMIN_KEY: &str = "admin";', 'const LIFETIME_THRESHOLD: u32 = 17_280 * 7;\nconst BUMP_AMOUNT: u32 = 17_280 * 30;\nconst ADMIN_KEY: &str = "admin";')

# Add emergency_funcs inside impl RevenuePool
impl_end_idx = old.rfind('}')
if impl_end_idx == -1:
    print("Could not find impl RevenuePool block")
    exit(1)

new_old = old[:impl_end_idx] + "\n" + emergency_funcs + "\n" + old[impl_end_idx:]

with open('contracts/revenue_pool/src/lib.rs', 'w') as f:
    f.write(new_old)

print("Merged successfully")
