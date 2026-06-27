extern crate std;

use crate::RevenuePoolError;
use std::collections::BTreeSet;

#[test]
fn revenue_pool_error_codes_are_stable_and_unique() {
    let mappings = [
        (1_u32, RevenuePoolError::BatchEmpty),
        (2, RevenuePoolError::BatchTooLarge),
    ];

    let mut seen = BTreeSet::new();
    for (expected_code, variant) in mappings {
        assert_eq!(variant as u32, expected_code);
        assert!(
            seen.insert(expected_code),
            "duplicate revenue-pool error code {expected_code}"
        );
    }

    assert_eq!(seen.len(), 2);
}

#[test]
fn error_code_docs_list_every_revenue_pool_code() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");
    let expected_lines = [
        "| 1 | `BatchEmpty` | Revenue Pool | `batch_distribute` received an empty `payments` vector |",
        "| 2 | `BatchTooLarge` | Revenue Pool | `batch_distribute` exceeded `MAX_BATCH_SIZE` |",
    ];

    for line in expected_lines {
        assert!(docs.contains(line), "missing revenue-pool docs line: {line}");
    }
}
