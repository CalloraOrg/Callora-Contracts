import re

with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

mods = """pub mod archive;
pub mod batch;
pub mod admin;
pub mod errors;
pub mod limits;
pub mod pagination;
pub mod timelock;
pub mod types;
pub mod events;
pub mod migrate;
"""

content = content.replace("pub mod archive;", mods)

# Fix the function name that is too long
content = content.replace(
    "pub fn batch_withdraw_developer_balance_cursor",
    "pub fn batch_withdraw_cursor"
)

# And if I rename the function, I also need to rename it in tests.
with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)

# Fix tests
import glob
for test_file in glob.glob('contracts/settlement/src/test*.rs'):
    with open(test_file, 'r') as f:
        test_content = f.read()
    if 'batch_withdraw_developer_balance_cursor' in test_content:
        test_content = test_content.replace(
            'batch_withdraw_developer_balance_cursor',
            'batch_withdraw_cursor'
        )
        with open(test_file, 'w') as f:
            f.write(test_content)
