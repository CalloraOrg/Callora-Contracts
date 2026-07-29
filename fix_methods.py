import os
import re

for root, _, files in os.walk('contracts/vault'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()

            # Fix deduct arguments
            # Old deduct: client.deduct(&caller, &150, &Some(Symbol::new(&env, "r1")), &u16::MAX, &developer)
            # We want to change it to: client.deduct(&caller, &150, &0)
            
            # Since there are multiline deduct calls:
            # client.deduct(
            #     &caller,
            #     &150,
            #     &Some(Symbol::new(&env, "deduct_1")),
            #     &u16::MAX,
            #     &developer,
            # )
            # We will use regex to find client.deduct with 5 arguments and replace it with 3 arguments.
            # Actually, `client.deduct` might be `f.client.deduct`.
            
            # Fix Symbol.to_str() -> Symbol.to_string()
            content = content.replace('.to_str()', '.to_string()')
            
            with open(path, 'w') as f:
                f.write(content)
