use soroban_sdk::{contracttype, Address, Bytes, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Tracks the FIFO queue bounds for a developer: (tail, head).
    Cursor(Address),
    /// Active event data payload: (developer, event_index).
    ActiveEvent(Address, u64),
    /// Archived event data payload: (developer, event_index).
    ArchivedEvent(Address, u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub tail: u64, // Oldest unarchived index
    pub head: u64, // Next index to insert
}

/// Minimum TTL threshold before extending.
pub const MIN_TTL_LEDGERS: u32 = 17_280; // ~1 day
/// TTL for archived elements.
pub const ARCHIVE_TTL_LEDGERS: u32 = 3_110_400; // ~6 months

/// Archives a batch of events for a developer using a FIFO cursor.
///
/// # Parameters
/// - `env`: Execution environment context.
/// - `developer`: Address of the developer whose events are being archived.
/// - `batch_size`: Maximum number of events to process in this invocation.
///
/// # Returns
/// - `u32`: The exact number of events successfully archived.
pub fn archive_events(env: &Env, developer: Address, batch_size: u32) -> u32 {
    developer.require_auth();

    let cursor_key = DataKey::Cursor(developer.clone());
    
    // Retrieve cursor or initialize a default instance. No unwraps permitted.
    let mut cursor: Cursor = match env.storage().persistent().get(&cursor_key) {
        Some(c) => c,
        None => Cursor { tail: 0, head: 0 },
    };

    let mut archived_count: u32 = 0;

    while archived_count < batch_size {
        if cursor.tail >= cursor.head {
            break;
        }

        let active_key = DataKey::ActiveEvent(developer.clone(), cursor.tail);
        let archive_key = DataKey::ArchivedEvent(developer.clone(), cursor.tail);

        // Perform atomic read-write-delete for the event payload
        if let Some(event_data) = env.storage().persistent().get::<_, Bytes>(&active_key) {
            env.storage().temporary().set(&archive_key, &event_data);
            env.storage().temporary().extend_ttl(
                &archive_key,
                MIN_TTL_LEDGERS,
                ARCHIVE_TTL_LEDGERS,
            );
            env.storage().persistent().remove(&active_key);
        }

        // Overflow-safe cursor progression
        cursor.tail = match cursor.tail.checked_add(1) {
            Some(val) => val,
            None => break,
        };
        
        archived_count = match archived_count.checked_add(1) {
            Some(val) => val,
            None => break,
        };
    }

    if archived_count > 0 {
        env.storage().persistent().set(&cursor_key, &cursor);
        env.storage().persistent().extend_ttl(
            &cursor_key,
            MIN_TTL_LEDGERS,
            ARCHIVE_TTL_LEDGERS,
        );
    }

    archived_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    #[test]
    fn test_fifo_archival_batching_and_ttl() {
        let env = Env::default();
        env.mock_all_auths();
        let developer = Address::generate(&env);

        let cursor_key = DataKey::Cursor(developer.clone());
        let cursor = Cursor { tail: 0, head: 5 };
        env.storage().persistent().set(&cursor_key, &cursor);

        // Seed 5 active events
        for i in 0..5 {
            let active_key = DataKey::ActiveEvent(developer.clone(), i);
            env.storage()
                .persistent()
                .set(&active_key, &Bytes::from_slice(&env, &[i as u8]));
        }

        // Execute batch constraint test
        let archived_first_pass = archive_events(&env, developer.clone(), 3);
        assert_eq!(archived_first_pass, 3);

        // Verify cursor state
        let updated_cursor: Cursor = env.storage().persistent().get(&cursor_key).unwrap();
        assert_eq!(updated_cursor.tail, 3);
        assert_eq!(updated_cursor.head, 5);

        // Verify isolation and data movement
        for i in 0..3 {
            let archive_key = DataKey::ArchivedEvent(developer.clone(), i);
            let active_key = DataKey::ActiveEvent(developer.clone(), i);
            
            assert!(env.storage().temporary().has(&archive_key));
            assert!(!env.storage().persistent().has(&active_key));
        }

        // Exhaust remaining events
        let archived_second_pass = archive_events(&env, developer.clone(), 10);
        assert_eq!(archived_second_pass, 2);

        let final_cursor: Cursor = env.storage().persistent().get(&cursor_key).unwrap();
        assert_eq!(final_cursor.tail, 5);
        assert_eq!(final_cursor.head, 5);
    }

    #[test]
    #[should_panic]
    fn test_require_auth_enforcement() {
        let env = Env::default();
        let developer = Address::generate(&env);
        // Will panic as auth is not mocked
        archive_events(&env, developer, 1);
    }
}