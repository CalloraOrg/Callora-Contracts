import re

with open('contracts/vault/src/test.rs', 'r') as f:
    content = f.read()

content = content.replace('Some(d.clone();', 'Some(d.clone()));')
content = content.replace('Some(d2.clone();', 'Some(d2.clone()));')
content = content.replace('Some(alice.clone();', 'Some(alice.clone()));')
content = content.replace('Some(bob.clone();', 'Some(bob.clone()));')
content = content.replace('Some(depositor.clone();', 'Some(depositor.clone()));')
content = content.replace('Some(new_owner.clone();', 'Some(new_owner.clone()));')
content = content.replace('Some(new_admin.clone();', 'Some(new_admin.clone()));')
content = content.replace('Some(new_hash.clone();', 'Some(new_hash.clone()));')
content = content.replace('Some(hash2.clone();', 'Some(hash2.clone()));')
content = content.replace('Err(Ok(VaultError::Slippage);', 'Err(Ok(VaultError::Slippage)));')
content = content.replace('Err(Ok(VaultError::AuthorizedCallerCannotBeVault);', 'Err(Ok(VaultError::AuthorizedCallerCannotBeVault)));')
content = content.replace('Err(Ok(VaultError::StaleNonce);', 'Err(Ok(VaultError::StaleNonce)));')
content = content.replace('client.init(&owner, &usdc, &0, &owner, &1, &None, 50, &soroban_sdk::Address::generate(&env)', 'client.init(&owner, &usdc, &0, &owner, &1, &None, 50, &soroban_sdk::Address::generate(&env));')

# The `client.init` in 5126 and 5164 might have a trailing `)` removed by my previous `)));` -> `);` fix.
# Wait, the previous fix changed `client.init(...)));` to `client.init(...);`.
# Let's just fix it properly with regex.
# Let's find any client.init without a closing );
content = re.sub(r'(client\.init\(&owner, &usdc, &0, &owner, &1, &None, 50, &soroban_sdk::Address::generate\(&env\))\n', r'\1);\n', content)

with open('contracts/vault/src/test.rs', 'w') as f:
    f.write(content)
