import os
import re

for root, _, files in os.walk('contracts/vault/src'):
    for file in files:
        if file.startswith('test') and file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            # Find and replace `)));` with `);`
            content = content.replace(')));', ');')
            with open(path, 'w') as f:
                f.write(content)
