with open('/tmp/7e15499_settlement_lib.rs', 'r') as f:
    content = f.read()

# Replace mod types
content = content.replace('mod types;\npub use types::*;\n', 'mod types;\npub use types::*;\npub mod batch;\n')

# Add batch_settle inside impl CalloraSettlement
batch_settle_code = """
    pub fn batch_settle(
        env: Env,
        settlements: soroban_sdk::Vec<batch::SettleInput>,
    ) -> soroban_sdk::Vec<batch::SettleOutcome> {
        batch::batch_settle(&env, settlements)
    }
"""

impl_end_idx = content.rfind('}')
if impl_end_idx != -1:
    # Need to find the end of `impl CalloraSettlement`.
    # Let's search for the last `}` before `mod events;`
    events_idx = content.find('mod events;')
    if events_idx != -1:
        impl_end_idx = content.rfind('}', 0, events_idx)
        content = content[:impl_end_idx] + batch_settle_code + content[impl_end_idx:]

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)
