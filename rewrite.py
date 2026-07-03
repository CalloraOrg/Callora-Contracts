import os
import re

def rewrite(content):
    # 1. Remove set_settlement
    content = re.sub(r'^[ \t]*client\.set_settlement.*$', '', content, flags=re.MULTILINE)
    content = re.sub(r'^[ \t]*vault_client\.set_settlement.*$', '', content, flags=re.MULTILINE)
    
    # 2. Fix init
    def repl_init(m):
        prefix = m.group(1)
        args_str = m.group(2)
        args = [a.strip() for a in args_str.split(',')]
        if len(args) == 7:
            owner = args[0]
            usdc = args[1]
            initial_balance = args[2].replace('&Some(', '').replace(')', '').replace('&None', '&0')
            if initial_balance == 'None': initial_balance = '&0'
            auth = args[3].replace('&Some(', '').replace(')', '').replace('&None', owner)
            min_dep = args[4].replace('&Some(', '').replace(')', '').replace('&None', '&1')
            rev = args[5]
            max_ded = args[6].replace('&Some(', '').replace(')', '').replace('&None', '&10000000000')
            settlement = "&soroban_sdk::Address::generate(&env)"
            
            return f"{prefix}init({owner}, {usdc}, {initial_balance}, {auth}, {min_dep}, {rev}, {max_ded}, {settlement})"
        return m.group(0)

    content = re.sub(r'(client\.)init\(([^)]+)\)', repl_init, content)
    content = re.sub(r'(vault_client\.)init\(([^)]+)\)', repl_init, content)

    # 3. Comment out rate limit stuff
    content = re.sub(r'^[ \t]*client\.set_developer_rate_limit.*$', '', content, flags=re.MULTILINE)
    
    return content

for root, _, files in os.walk('contracts/vault/src'):
    for file in files:
        if file.startswith('test') and file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            content = rewrite(content)
            with open(path, 'w') as f:
                f.write(content)
