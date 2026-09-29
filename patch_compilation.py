import re

# 1. Fix lib.rs imports and duplicates
with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

# Remove duplicate `pub mod batch;` if any
content = content.replace("pub mod batch;\n", "", 1) # remove first instance

# Add Symbol, String, Vec to imports
content = content.replace(
    'use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, token};',
    'use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, token, Symbol, String, Vec};'
)

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)

# 2. Fix pagination.rs
with open('contracts/settlement/src/pagination.rs', 'r') as f:
    pag = f.read()

pag = pag.replace('token: token.clone(),\n', '')
with open('contracts/settlement/src/pagination.rs', 'w') as f:
    f.write(pag)

# 3. Add TotalReceived to StorageKey in types.rs
with open('contracts/settlement/src/types.rs', 'r') as f:
    types = f.read()

if 'TotalReceived' not in types:
    types = types.replace(
        'DeveloperClaimWindow(Address),',
        'DeveloperClaimWindow(Address),\n    TotalReceived,\n    TotalSettled,'
    )
    with open('contracts/settlement/src/types.rs', 'w') as f:
        f.write(types)
