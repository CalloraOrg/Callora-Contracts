import re

with open('/tmp/emergency_funcs.rs', 'r') as f:
    emergency = f.read()

with open('/tmp/7e15499_revenue_pool.rs', 'r') as f:
    old = f.read()

# Remove 'mod events;'
old = old.replace('mod events;\n', '')
# Add 'pub mod emergency;\npub mod events;\n' at the top
old = old.replace('#![no_std]\n', '#![no_std]\npub mod emergency;\npub mod events;\n')

# Find the end of `impl RevenuePool` block which is line 919 in 7e15499
lines = old.split('\n')

# Find where impl RevenuePool ends (line 919 is the '}' right before 'mod events;')
impl_end = -1
for i, line in enumerate(lines):
    if line.startswith('pub fn chunk_iter'):
        # Go backwards to find the closing brace of impl RevenuePool
        for j in range(i-1, -1, -1):
            if lines[j] == '}':
                impl_end = j
                break
        break

if impl_end == -1:
    print("Could not find end of impl RevenuePool")
    exit(1)

# Insert emergency
lines.insert(impl_end, emergency)

with open('contracts/revenue_pool/src/lib.rs', 'w') as f:
    f.write('\n'.join(lines))

print("Fixed successfully")
