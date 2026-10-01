import re

with open('contracts/revenue_pool/src/lib.rs', 'r') as f:
    current = f.read()

# The emergency drain functions are all at the end of the file, starting with propose_emergency_drain
# up to the end of the `impl` block which is just before the end of the file.
start_idx = current.find('    /// Propose an emergency drain of USDC from the revenue pool')
if start_idx == -1:
    print("Could not find propose_emergency_drain documentation")
    exit(1)

# Find the end of the impl block
end_idx = current.rfind('}')
if end_idx == -1:
    print("Could not find end of impl block")
    exit(1)

emergency_funcs = current[start_idx:end_idx]

with open('/tmp/emergency_funcs.rs', 'w') as f:
    f.write(emergency_funcs)

print(f"Extracted {len(emergency_funcs)} bytes")
