import re

# Fix lib.rs
with open(r'C:\Users\Administrator\Desktop\Callora-Contracts\contracts\settlement\src\lib.rs', 'rb') as f:
    content = f.read().decode('utf-8')

# Normalize line endings to LF for easier processing
content = content.replace('\r\n', '\n')

# Remove contracterror and contracttype from imports
content = content.replace('contracterror, contracttype,', '')

# Remove SettlementError inline block
pattern = r'^/// Typed errors for the settlement contract\.\n.*?^}\n\n'
content = re.sub(pattern, '', content, flags=re.MULTILINE | re.DOTALL)

# Remove StorageKey inline block
pattern = r'^/// Persistent storage keys for settlement contract\n\[contracttype\]\n.*?^}\n\n'
content = re.sub(pattern, '', content, flags=re.MULTILINE | re.DOTALL)

# Remove all inline struct blocks
struct_patterns = [
    r'^/// Developer balance record in settlement contract\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Global pool balance tracking\.\n.*?^}\n\n',
    r"^/// Tracks a developer's cumulative withdrawal amount for a given epoch day\.\n.*?^}\n\n",
    r'^/// Timestamp range during which a developer may claim accrued balance\.\n.*?^}\n\n',
    r'^/// Read-only preview of a developer claim/withdrawal\.\n.*?^}\n\n',
    r'^/// Payment received event\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Balance credited event\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Emitted when a new vault address is proposed via `propose_vault`\.\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Emitted when the proposed vault is accepted via `accept_vault`\.\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Emitted when a developer withdraws their balance\.\n\[contracttype\]\n.*?^}\n\n',
    r"^/// Emitted when the admin sets or changes a developer's daily withdrawal cap\.\n\[contracttype\]\n.*?^}\n\n",
    r'^/// Emitted when the admin sets or clears a developer claim window\.\n\[contracttype\]\n.*?^}\n\n',
    r'^/// Emitted when an admin force-credits a developer balance \(escape hatch\)\.\n\[contracttype\]\n.*?^}\n\n',
]

for pat in struct_patterns:
    content = re.sub(pat, '', content, flags=re.MULTILINE | re.DOTALL)

# Add price_registry entrypoints before internal helpers
content = content.replace(
    '    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Internal helpers ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━',
    '''    pub fn set_price(env: Env, caller: Address, offering_id: soroban_sdk::String, price: soroban_sdk::String) {
        price_registry::set_price(&env, caller, offering_id, price);
    }

    pub fn remove_price(env: Env, caller: Address, offering_id: soroban_sdk::String) {
        price_registry::remove_price(&env, caller, offering_id);
    }

    pub fn get_price(env: Env, offering_id: soroban_sdk::String) -> Option<soroban_sdk::String> {
        price_registry::get_price(&env, offering_id)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Internal helpers ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━'''
)

with open(r'C:\Users\Administrator\Desktop\Callora-Contracts\contracts\settlement\src\lib.rs', 'wb') as f:
    f.write(content.encode('utf-8'))

print("lib.rs done")

# Fix errors.rs duplicate
with open(r'C:\Users\Administrator\Desktop\Callora-Contracts\contracts\settlement\src\errors.rs', 'rb') as f:
    content = f.read().decode('utf-8')

content = content.replace('\r\n', '\n')
content = content.replace(
    '    /// Admin attempted a price write before the minimum interval elapsed.\n    WriteRateLimitExceeded = 33,\n    /// Admin attempted a price write before the minimum interval elapsed.\n    WriteRateLimitExceeded = 33,',
    '    /// Admin attempted a price write before the minimum interval elapsed.\n    WriteRateLimitExceeded = 33,'
)

with open(r'C:\Users\Administrator\Desktop\Callora-Contracts\contracts\settlement\src\errors.rs', 'wb') as f:
    f.write(content.encode('utf-8'))

print("errors.rs done")
