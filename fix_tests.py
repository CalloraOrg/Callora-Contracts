import os
import re

def process_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Replace 7-arg init with 8-arg init
    # client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    # -> client.init(&owner, &usdc, &500, &owner, &1, &None, &10000000000, &settlement);
    # But wait, settlement needs to be defined. We can just define `let settlement = Address::generate(&env);` at the top of the test function,
    # or just use `&usdc` as a dummy settlement address if we don't care, or replace it with `&Address::generate(&env)`.
    
    # Let's replace client.init(...) with a multiline replacement.
    
    # We will just replace `client.set_settlement` with nothing.
    content = re.sub(r'client\.set_settlement\([^)]+\);', '', content)
    content = re.sub(r'vault_client\.set_settlement\([^)]+\);', '', content)
    
    # Replace init arguments:
    # client.init(&owner, &usdc, &Some(X), &None, &None, &None, &None) -> client.init(&owner, &usdc, &X, &owner, &1, &None, &1000000000, &Address::generate(&env))
    # Actually, a regex for the init call:
    pattern = r'client\.init\(\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^)]+)\s*\)'
    def init_repl(m):
        args = [m.group(i) for i in range(1, 8)]
        owner = args[0]
        usdc = args[1]
        
        def extract_opt(val, default):
            if 'Some' in val:
                match = re.search(r'Some\((.*?)\)', val)
                if match:
                    return f"&({match.group(1).replace('&', '')})"
            return default
            
        initial_balance = extract_opt(args[2], "&0")
        auth = extract_opt(args[3], owner)
        min_dep = extract_opt(args[4], "&1")
        rev_pool = args[5] # Keep as Option
        max_ded = extract_opt(args[6], "&10000000000")
        
        return f"client.init({owner}, {usdc}, {initial_balance}, {auth}, {min_dep}, {rev_pool}, {max_ded}, &soroban_sdk::Address::generate(&env))"
        
    content = re.sub(pattern, init_repl, content)

    with open(filepath, 'w') as f:
        f.write(content)

for root, _, files in os.walk('contracts/vault/src'):
    for file in files:
        if file.startswith('test') and file.endswith('.rs'):
            process_file(os.path.join(root, file))
