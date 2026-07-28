import os
import re

for root, _, files in os.walk('c:/Users/Stephan/Documents/Callora-Contracts/contracts/vault/src'):
    for file in files:
        if file.startswith('test') and file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()
            
            # Replace deduct calls: &u16::MAX) -> &u16::MAX, &Address::generate(&env))
            # Also replace other max_fee_bps values if any (e.g. &50), let's just do a regex
            # deduct(&owner, &5, &None, &50) -> deduct(&owner, &5, &None, &50, &Address::generate(&env))
            
            content = re.sub(r'(client\.deduct\([^;]+?)(,\s*&[^,]+)\)', r'\1\2, &Address::generate(&env))', content)
            content = re.sub(r'(vault_client\.deduct\([^;]+?)(,\s*&[^,]+)\)', r'\1\2, &Address::generate(&env))', content)
            
            # Replace DeductItem { amount, request_id }
            content = re.sub(r'(request_id:\s*[^,}]+)\s*}', r'\1, developer: Address::generate(&env) }', content)
            
            with open(path, 'w', encoding='utf-8') as f:
                f.write(content)
