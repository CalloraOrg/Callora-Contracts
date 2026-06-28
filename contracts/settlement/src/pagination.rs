use crate::{DeveloperBalance, StorageKey, MAX_DEVELOPER_BALANCES_PAGE_SIZE};
use soroban_sdk::{Address, Env, Vec};

pub fn get_page(
    env: &Env,
    index: &Vec<Address>,
    cursor: Option<Address>,
    limit: u32,
    token: Address,
) -> (Vec<DeveloperBalance>, Option<Address>) {
    let effective_limit = if limit == 0 {
        return (Vec::new(env), None);
    } else {
        limit.min(MAX_DEVELOPER_BALANCES_PAGE_SIZE)
    };

    let mut result = Vec::new(env);
    let mut past_cursor = cursor.is_none();
    let mut last_address: Option<Address> = None;

    for address in index.iter() {
        if !past_cursor {
            if let Some(ref c) = cursor {
                if &address == c {
                    past_cursor = true;
                }
            }
            continue;
        }

        let balance: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(address.clone(), token.clone()))
            .unwrap_or(0);

        result.push_back(DeveloperBalance {
            address: address.clone(),
            token: token.clone(),
            balance,
        });
        last_address = Some(address.clone());

        if result.len() >= effective_limit {
            break;
        }
    }

    let next_cursor = if result.len() >= effective_limit {
        last_address
    } else {
        None
    };

    (result, next_cursor)
}
