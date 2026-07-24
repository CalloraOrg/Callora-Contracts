import re

with open('contracts/vault/src/test.rs', 'r') as f:
    content = f.read()

# Fix Some(xxx.clone();
content = re.sub(r'Some\(([a-zA-Z0-9_]+)\.clone\(\);', r'Some(\1.clone()));', content)

# Fix Err(Ok(VaultError::xxx);
content = re.sub(r'Err\(Ok\(VaultError::([a-zA-Z0-9_]+)\);', r'Err(Ok(VaultError::\1)));', content)

with open('contracts/vault/src/test.rs', 'w') as f:
    f.write(content)
