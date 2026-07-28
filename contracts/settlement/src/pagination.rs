use crate::{
    DeveloperBalance, StorageKey, INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_THRESHOLD,
    MAX_DEVELOPER_BALANCES_PAGE_SIZE, PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD,
};
use soroban_sdk::{Address, Env, Vec};

/// Get a paginated page of developer balances using cursor-based pagination.
///
/// # Pagination Behavior
/// Returns up to `limit` developer balance records starting **after** the supplied `cursor`
/// address (exclusive), or from the beginning of the index when `cursor` is `None`.
///
/// # Cursor Semantics
/// The returned `next_cursor` is the address of the last record returned on a full page.
/// Subsequent calls should pass this `next_cursor` as the `cursor` argument.
/// When the returned page has fewer elements than the requested limit (or is empty), the end
/// of the list has been reached, and `None` is returned as the next cursor.
///
/// # Ordering Guarantees
/// The index is maintained in deterministic sorted ascending order by address bytes, guaranteeing
/// stable, deterministic pagination across repeated calls. The output is sorted, meaning pages
/// are stable even if interleaved credits happen for developers that sort after the cursor.
///
/// # Page-size Configuration
/// The page size is capped at `MAX_DEVELOPER_BALANCES_PAGE_SIZE` (100) to limit gas usage
/// and prevent transaction size limits from being exceeded.
///
/// # Intended Use
/// This function is designed for batch reconciliation, indexing, and reporting dashboards
/// where developer balances must be safely and incrementally sync'd.
///
/// # State Mutation
/// This function is read-only for contract state logic but extends storage TTL for retrieved
/// entries to prevent archival.
pub fn get_page(
    env: &Env,
    index: &Vec<Address>,
    cursor: Option<Address>,
    limit: u32,
    usdc_token: &Address,
) -> (Vec<DeveloperBalance>, Option<Address>) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

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

        let key = StorageKey::DeveloperBalance(address.clone(), usdc_token.clone());
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        }

        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        result.push_back(DeveloperBalance {
            address: address.clone(),
            token: usdc_token.clone(),
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
