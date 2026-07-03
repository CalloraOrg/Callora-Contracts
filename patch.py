with open('/tmp/7e15499_revenue_pool.rs', 'r') as f:
    lines = f.readlines()

with open('/tmp/emergency_funcs.rs', 'r') as f:
    emergency = f.read()

# Replace use statement to add emergency module (events is already there at the bottom, but we'll move it)
for i, line in enumerate(lines):
    if line.startswith('use soroban_sdk::{'):
        lines[i] = 'pub mod emergency;\npub mod events;\n\n' + line
        break

# Remove `mod events;` from the bottom
lines = [line for line in lines if not line.startswith('mod events;')]

# Insert emergency at line 919 (which is index 918)
# But wait, we added lines at the top, so let's find the closing brace by searching for `pub fn chunk_iter`
chunk_iter_idx = -1
for i, line in enumerate(lines):
    if line.startswith('pub fn chunk_iter'):
        chunk_iter_idx = i
        break

impl_end_idx = -1
for i in range(chunk_iter_idx - 1, -1, -1):
    if lines[i].strip() == '}':
        impl_end_idx = i
        break

lines.insert(impl_end_idx, emergency + '\n')

with open('contracts/revenue_pool/src/lib.rs', 'w') as f:
    f.write(''.join(lines))
