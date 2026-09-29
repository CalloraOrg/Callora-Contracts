import re

with open('contracts/vault/tests/event_order.rs', 'r') as f:
    content = f.read()

# Replace client.deduct(...) calls
# We'll just replace the multiline deduct calls.
def repl_deduct(m):
    return f"client.deduct({m.group(1)}, {m.group(2)}, &0);"

# regex to match: client.deduct( \n &caller, \n &100, \n &Some(..), \n &MAX, \n &developer \n );
pattern = r'client\.deduct\(\s*([^,]+),\s*([^,]+),\s*[^,]+,\s*[^,]+,\s*[^,]+\s*\);'
content = re.sub(pattern, repl_deduct, content)

with open('contracts/vault/tests/event_order.rs', 'w') as f:
    f.write(content)
