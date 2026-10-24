use crate::{
    CalloraSettlement, DeveloperBalance, SettlementError, INSTANCE_BUMP_AMOUNT,
    INSTANCE_BUMP_THRESHOLD, MAX_DEVELOPER_BALANCES_PAGE_SIZE,
};
use soroban_sdk::{Address, Env, Vec};

/// Get a paginated page of developer balances using cursor-based pagination
/// over the **paged persistent index**.
///
/// # Pagination Behavior
/// Returns up to `limit` developer balance records starting at the supplied
/// global index, or from the beginning of the index when `cursor` is `None`.
///
/// # Cursor Semantics
/// The returned `next_cursor` is the global index at which the next page begins.
/// Subsequent calls should pass this `next_cursor` as the `cursor` argument.
/// When the returned page reaches the end of the index, `None` is returned.
///
/// # Ordering Guarantees
/// Addresses are stored in insertion order (append-only).
/// Ordering across pages is therefore also insertion order, which is stable for
/// sequential pagination. Interleaved credits for *new* developers appended after
/// the cursor will appear on later pages; they never shift existing pages.
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
    cursor: Option<u32>,
    limit: u32,
    usdc_token: &Address,
) -> (Vec<DeveloperBalance>, Option<u32>) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

    if limit == 0 {
        return (Vec::new(env), None);
    }
    let effective_limit = limit.min(MAX_DEVELOPER_BALANCES_PAGE_SIZE);
    let start = cursor.unwrap_or(0);
    let index_len = CalloraSettlement::index_len(env);
    if start > index_len {
        env.panic_with_error(SettlementError::InvalidCursor);
    }
    let result = CalloraSettlement::get_index_slice(env, start, effective_limit, usdc_token);
    let end = start.saturating_add(result.len());
    let next_cursor = if end < index_len && result.len() == effective_limit {
        Some(end)
    } else {
        None
    };
    (result, next_cursor)
}
