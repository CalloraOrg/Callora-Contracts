extern crate std;

use crate::{CalloraVault, CalloraVaultClient, DeductItem};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Vec};

// ---------------------------------------------------------------------------
// Malicious Token Mock
// ---------------------------------------------------------------------------

#[contract]
pub struct MaliciousToken;

#[contractimpl]
impl MaliciousToken {
    pub fn transfer(env: Env, from: Address, _to: Address, _amount: i128) {
        from.require_auth();

        let vault_addr: Option<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "vault_addr"));
        let attack_active: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "attack_active"))
            .unwrap_or(false);

        if attack_active {
            if let Some(vault) = vault_addr {
                // Prevent infinite recursion in the mock
                env.storage()
                    .instance()
                    .set(&Symbol::new(&env, "attack_active"), &false);

                let caller: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "attack_caller"))
                    .unwrap();
                let client = CalloraVaultClient::new(&env, &vault);

                // Attempt re-entry into deduct
                let _ = client.try_deduct(&caller, &1, &Some(Symbol::new(&env, "reentry_token")), &u16::MAX);
            }
        }
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000_000
    }

    pub fn set_token_attack_config(env: Env, vault: Address, caller: Address, active: bool) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "vault_addr"), &vault);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "attack_caller"), &caller);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "attack_active"), &active);
    }
}

// ---------------------------------------------------------------------------
// Malicious Settlement Mock
// ---------------------------------------------------------------------------

#[contract]
pub struct MaliciousSettlement;

#[contractimpl]
impl MaliciousSettlement {
    pub fn receive_payment(
        env: Env,
        _caller: Address,
        _amount: i128,
        _to_pool: bool,
        _developer: Option<Address>,
        _token: Address,
    ) {
        let vault_addr: Option<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "vault_addr"));
        let attack_active: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "attack_active"))
            .unwrap_or(false);

        if attack_active {
            if let Some(vault) = vault_addr {
                env.storage()
                    .instance()
                    .set(&Symbol::new(&env, "attack_active"), &false);
                let caller: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "attack_caller"))
                    .unwrap();
                let client = CalloraVaultClient::new(&env, &vault);

                // Attempt re-entry into deduct
                let _ = client.try_deduct(&caller, &1, &Some(Symbol::new(&env, "reentry_settle")), &u16::MAX);
            }
        }
    }

    pub fn set_settle_attack_config(env: Env, vault: Address, caller: Address, active: bool) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "vault_addr"), &vault);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "attack_caller"), &caller);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "attack_active"), &active);
    }
}

// ---------------------------------------------------------------------------
// Reentrancy Tests
// ---------------------------------------------------------------------------

fn setup_reentrancy_test(env: &Env) -> (Address, CalloraVaultClient, Address, Address, Address) {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let vault_client = CalloraVaultClient::new(env, &vault_addr);

    let token_addr = env.register(MaliciousToken, ());
    let settlement_addr = env.register(MaliciousSettlement, ());

    env.mock_all_auths();

    // Init vault with the malicious token
    vault_client.init(&owner, &token_addr, &Some(1000), &None, &None, &None, &None);
    vault_client.set_settlement(&owner, &settlement_addr);

    (vault_addr, vault_client, token_addr, settlement_addr, owner)
}

#[test]
fn test_reentrancy_via_token_transfer_is_blocked_by_auth() {
    let env = Env::default();
    let (vault_addr, vault_client, token_addr, _settlement_addr, owner) =
        setup_reentrancy_test(&env);

    let token_mock = MaliciousTokenClient::new(&env, &token_addr);
    token_mock.set_token_attack_config(&vault_addr, &owner, &true);

    let initial_balance = vault_client.balance();
    assert_eq!(initial_balance, 1000);

    // Trigger deduct -> calls token.transfer -> calls vault.deduct (re-entry)
    let result = vault_client.try_deduct(&owner, &100, &Some(Symbol::new(&env, "first_call")), &u16::MAX);

    assert!(result.is_ok(), "First deduct should succeed");
    assert_eq!(
        vault_client.balance(),
        900,
        "Balance should only be deducted once"
    );

    // Check if the re-entry event was published (it shouldn't be if it failed)
    let events = env.events().all();
    let mut reentry_count = 0;
    for e in events.iter() {
        if e.0 != vault_addr {
            continue;
        }
        let topics = &e.1;
        if topics.len() < 3 {
            continue;
        }
        let rid: Symbol = topics.get(2).unwrap().into_val(&env);
        if rid == Symbol::new(&env, "reentry_token") {
            reentry_count += 1;
        }
    }

    assert_eq!(reentry_count, 0, "Re-entry should not have succeeded");
}

#[test]
fn test_reentrancy_via_settlement_callback_is_blocked() {
    let env = Env::default();
    let (vault_addr, vault_client, _token_addr, settlement_addr, owner) =
        setup_reentrancy_test(&env);

    let settlement_mock = MaliciousSettlementClient::new(&env, &settlement_addr);
    settlement_mock.set_settle_attack_config(&vault_addr, &owner, &true);

    let initial_balance = vault_client.balance();
    assert_eq!(initial_balance, 1000);

    // Trigger deduct -> calls settlement.receive_payment -> calls vault.deduct (re-entry)
    let result = vault_client.try_deduct(&owner, &100, &Some(Symbol::new(&env, "first_call")), &u16::MAX);

    assert!(result.is_ok(), "First deduct should succeed");
    assert_eq!(
        vault_client.balance(),
        900,
        "Balance should only be deducted once"
    );

    let events = env.events().all();
    let mut reentry_count = 0;
    for e in events.iter() {
        if e.0 != vault_addr {
            continue;
        }
        let topics = &e.1;
        if topics.len() < 3 {
            continue;
        }
        let rid: Symbol = topics.get(2).unwrap().into_val(&env);
        if rid == Symbol::new(&env, "reentry_settle") {
            reentry_count += 1;
        }
    }

    assert_eq!(
        reentry_count, 0,
        "Re-entry via settlement should not have succeeded"
    );
}

#[test]
fn test_batch_deduct_reentrancy_via_token() {
    let env = Env::default();
    let (vault_addr, vault_client, token_addr, _settlement_addr, owner) =
        setup_reentrancy_test(&env);

    let token_mock = MaliciousTokenClient::new(&env, &token_addr);
    token_mock.set_token_attack_config(&vault_addr, &owner, &true);

    let items = Vec::from_array(
        &env,
        [
            DeductItem {
                amount: 50,
                request_id: Some(Symbol::new(&env, "item1")),
            },
            DeductItem {
                amount: 50,
                request_id: Some(Symbol::new(&env, "item2")),
            },
        ],
    );

    let result = vault_client.try_batch_deduct(&owner, &items);

    assert!(result.is_ok(), "Batch deduct should succeed");
    assert_eq!(
        vault_client.balance(),
        900,
        "Balance should only be deducted by batch amount"
    );

    let events = env.events().all();
    let mut reentry_count = 0;
    for e in events.iter() {
        if e.0 != vault_addr {
            continue;
        }
        let topics = &e.1;
        if topics.len() < 3 {
            continue;
        }
        let rid: Symbol = topics.get(2).unwrap().into_val(&env);
        if rid == Symbol::new(&env, "reentry_token") {
            reentry_count += 1;
        }
    }

    assert_eq!(
        reentry_count, 0,
        "Re-entry during batch should not have succeeded"
    );
}

#[test]
fn test_reentrancy_by_authorized_attacker() {
    let env = Env::default();
    let (vault_addr, vault_client, token_addr, _settlement_addr, _owner) =
        setup_reentrancy_test(&env);

    let attacker = Address::generate(&env);
    vault_client.set_authorized_caller(&Some(attacker.clone()), &0u64);
    
    let token_mock = MaliciousTokenClient::new(&env, &token_addr);
    token_mock.set_token_attack_config(&vault_addr, &attacker, &true);

    let initial_balance = vault_client.balance();
    assert_eq!(initial_balance, 1000);

    // Attacker calls deduct -> token.transfer -> attacker calls vault.deduct (re-entry)
    let result = vault_client.try_deduct(&attacker, &100, &Some(Symbol::new(&env, "first_call")), &u16::MAX);

    assert!(result.is_ok(), "First deduct should succeed");
    assert_eq!(
        vault_client.balance(),
        900,
        "Balance should only be deducted once"
    );

    let events = env.events().all();
    let mut reentry_count = 0;
    for e in events.iter() {
        if e.0 != vault_addr {
            continue;
        }
        let topics = &e.1;
        if topics.len() < 3 {
            continue;
        }
        let rid: Symbol = topics.get(2).unwrap().into_val(&env);
        if rid == Symbol::new(&env, "reentry_token") {
            reentry_count += 1;
        }
    }

    assert_eq!(
        reentry_count, 0,
        "Re-entry by authorized attacker should still fail or be blocked"
    );
}

#[test]
fn test_withdraw_reentrancy_via_token() {
    let env = Env::default();
    let (vault_addr, vault_client, token_addr, _settlement_addr, owner) =
        setup_reentrancy_test(&env);

    let token_mock = MaliciousTokenClient::new(&env, &token_addr);
    // Withdraw calls token.transfer. We attempt to call deduct() during withdraw's transfer.
    token_mock.set_token_attack_config(&vault_addr, &owner, &true);

    let result = vault_client.try_withdraw(&100);

    assert!(result.is_ok(), "Withdraw should succeed");
    assert_eq!(
        vault_client.balance(),
        900,
        "Balance should only be deducted by withdraw amount"
    );

    let events = env.events().all();
    let mut reentry_count = 0;
    for e in events.iter() {
        if e.0 != vault_addr {
            continue;
        }
        let topics = &e.1;
        if topics.len() < 3 {
            continue;
        }
        let rid: Symbol = topics.get(2).unwrap().into_val(&env);
        if rid == Symbol::new(&env, "reentry_token") {
            reentry_count += 1;
        }
    }

    assert_eq!(
        reentry_count, 0,
        "Re-entry during withdraw should not have succeeded"
    );
}
