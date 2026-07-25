import re

with open('contracts/vault/tests/event_order.rs', 'r') as f:
    content = f.read()

# Replace client.deduct blocks
content = re.sub(
    r'client\.deduct\(\s*&owner,\s*&([0-9]+),\s*&Some\(Symbol::new\(&env,\s*".*?"\)\),\s*&u16::MAX,\s*&developer,\s*\);',
    r'client.deduct(&owner, &\1, &0);',
    content,
    flags=re.MULTILINE
)

with open('contracts/vault/tests/event_order.rs', 'w') as f:
    f.write(content)
