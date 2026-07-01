use callora_settlement::CalloraSettlement;
use callora_vault::{CalloraVault, CalloraVaultClient, DeductItem};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, Symbol, Vec};

fn setup(env: &Env) -> (CalloraVaultClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let developer = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);

    let usdc_addr = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let usdc_admin = token::StellarAssetClient::new(env, &usdc_addr);

    usdc_admin.mint(&vault_addr, &10_000);
    client.init(
        &owner,
        &usdc_addr,
        &Some(10_000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement_addr = env.register(CalloraSettlement, ());
    let settlement_client = callora_settlement::CalloraSettlementClient::new(env, &settlement_addr);
    settlement_client.init(&owner, &vault_addr);
    client.set_settlement(&owner, &settlement_addr);

    (client, owner, developer, vault_addr)
}

fn collect_deduct_events(env: &Env) -> Vec<(Address, Symbol)> {
    let events = env.events().all();
    let mut result: Vec<(Address, Symbol)> = Vec::new(env);
    for ev in events.iter() {
        if ev.1.is_empty() {
            continue;
        }
        let s: Symbol = ev.1.get(0).unwrap().into_val(env);
        if s != Symbol::new(env, "deduct") {
            continue;
        }
        let caller: Address = ev.1.get(1).unwrap().into_val(env);
        let rid: Symbol = ev.1.get(2).unwrap().into_val(env);
        result.push_back((caller, rid));
    }
    result
}

#[test]
fn batch_deduct_events_match_item_order() {
    let env = Env::default();
    let (client, owner, developer, _) = setup(&env);

    let items = Vec::from_array(
        &env,
        [
            DeductItem {
                amount: 100,
                request_id: Some(Symbol::new(&env, "item_a")),
                developer: developer.clone(),
            },
            DeductItem {
                amount: 200,
                request_id: Some(Symbol::new(&env, "item_b")),
                developer: developer.clone(),
            },
            DeductItem {
                amount: 300,
                request_id: Some(Symbol::new(&env, "item_c")),
                developer: developer.clone(),
            },
        ],
    );

    client.batch_deduct(&owner, &items);

    let deduct_events = collect_deduct_events(&env);
    assert_eq!(deduct_events.len(), 3);

    let (_c1, rid1) = deduct_events.get(0).unwrap();
    assert_eq!(rid1, Symbol::new(&env, "item_a"));

    let (_c2, rid2) = deduct_events.get(1).unwrap();
    assert_eq!(rid2, Symbol::new(&env, "item_b"));

    let (_c3, rid3) = deduct_events.get(2).unwrap();
    assert_eq!(rid3, Symbol::new(&env, "item_c"));
}

#[test]
fn batch_deduct_reverse_order_is_preserved() {
    let env = Env::default();
    let (client, owner, developer, _) = setup(&env);

    let items = Vec::from_array(
        &env,
        [
            DeductItem {
                amount: 300,
                request_id: Some(Symbol::new(&env, "z_last")),
                developer: developer.clone(),
            },
            DeductItem {
                amount: 200,
                request_id: Some(Symbol::new(&env, "m_mid")),
                developer: developer.clone(),
            },
            DeductItem {
                amount: 100,
                request_id: Some(Symbol::new(&env, "a_first")),
                developer: developer.clone(),
            },
        ],
    );

    client.batch_deduct(&owner, &items);

    let deduct_events = collect_deduct_events(&env);
    assert_eq!(deduct_events.len(), 3);

    let (_, rid0) = deduct_events.get(0).unwrap();
    assert_eq!(rid0, Symbol::new(&env, "z_last"));
    let (_, rid1) = deduct_events.get(1).unwrap();
    assert_eq!(rid1, Symbol::new(&env, "m_mid"));
    let (_, rid2) = deduct_events.get(2).unwrap();
    assert_eq!(rid2, Symbol::new(&env, "a_first"));
}

#[test]
fn sequential_deduct_events_match_call_order() {
    let env = Env::default();
    let (client, owner, developer, _) = setup(&env);

    client.deduct(
        &owner,
        &100,
        &Some(Symbol::new(&env, "first_call")),
        &u16::MAX,
        &developer,
    );
    client.deduct(
        &owner,
        &200,
        &Some(Symbol::new(&env, "second_call")),
        &u16::MAX,
        &developer,
    );
    client.deduct(
        &owner,
        &300,
        &Some(Symbol::new(&env, "third_call")),
        &u16::MAX,
        &developer,
    );

    let deduct_events = collect_deduct_events(&env);
    assert_eq!(deduct_events.len(), 3);

    let (_, rid0) = deduct_events.get(0).unwrap();
    assert_eq!(rid0, Symbol::new(&env, "first_call"));
    let (_, rid1) = deduct_events.get(1).unwrap();
    assert_eq!(rid1, Symbol::new(&env, "second_call"));
    let (_, rid2) = deduct_events.get(2).unwrap();
    assert_eq!(rid2, Symbol::new(&env, "third_call"));
}

#[test]
fn mixed_deposit_deduct_withdraw_event_order() {
    let env = Env::default();
    let (client, owner, developer, vault_addr) = setup(&env);

    client.deposit(&owner, &500);

    client.deduct(
        &owner,
        &200,
        &Some(Symbol::new(&env, "deduct_1")),
        &u16::MAX,
        &developer,
    );

    client.withdraw(&100);

    let events = env.events().all();
    let mut event_types: Vec<Symbol> = Vec::new(&env);
    for ev in events.iter() {
        if ev.1.is_empty() {
            continue;
        }
        let s: Symbol = ev.1.get(0).unwrap().into_val(&env);
        event_types.push_back(s);
    }

    let mut idx = 0;
    let expected_order = ["init", "set_settlement", "deposit", "deduct", "withdraw"];
    for et in event_types.iter() {
        let s: Symbol = et;
        let mut buf = [0u8; 32];
        let len = s.to_str().copy_into_slice(&mut buf);
        let name = core::str::from_utf8(&buf[..len as usize]).unwrap();
        if idx < expected_order.len() && name == expected_order[idx] {
            idx += 1;
        }
    }
    assert!(
        idx >= expected_order.len(),
        "expected all event types in order, got stuck at {}",
        expected_order.get(idx).unwrap_or(&"end")
    );
}

#[test]
fn batch_deduct_deterministic_across_repeated_calls() {
    let env = Env::default();
    let (client, owner, developer, _) = setup(&env);

    let items = Vec::from_array(
        &env,
        [
            DeductItem {
                amount: 100,
                request_id: Some(Symbol::new(&env, "id1")),
                developer: developer.clone(),
            },
            DeductItem {
                amount: 200,
                request_id: Some(Symbol::new(&env, "id2")),
                developer: developer.clone(),
            },
        ],
    );

    for _ in 0..5 {
        let snapshot = env.events().all();
        client.batch_deduct(&owner, &items);
        let events = env.events().all();
        let new_count = events.len() - snapshot.len();
        let deduct_count = events
            .iter()
            .filter(|e| {
                if e.1.is_empty() {
                    return false;
                }
                let s: Symbol = e.1.get(0).unwrap().into_val(&env);
                s == Symbol::new(&env, "deduct")
            })
            .count();
        let snapshot_deduct_count = snapshot
            .iter()
            .filter(|e| {
                if e.1.is_empty() {
                    return false;
                }
                let s: Symbol = e.1.get(0).unwrap().into_val(&env);
                s == Symbol::new(&env, "deduct")
            })
            .count();
        assert_eq!(
            deduct_count - snapshot_deduct_count,
            2,
            "each batch_deduct call should emit exactly 2 deduct events (iteration {})",
            _
        );
    }
}

#[test]
fn deposit_event_emitted_before_deduct_event() {
    let env = Env::default();
    let (client, owner, developer, _) = setup(&env);

    client.deposit(&owner, &500);

    client.deduct(
        &owner,
        &100,
        &Some(Symbol::new(&env, "r1")),
        &u16::MAX,
        &developer,
    );

    let events = env.events().all();
    let mut deposit_positions: Vec<u32> = Vec::new(&env);
    let mut deduct_positions: Vec<u32> = Vec::new(&env);
    for (i, ev) in events.iter().enumerate() {
        if ev.1.is_empty() {
            continue;
        }
        let s: Symbol = ev.1.get(0).unwrap().into_val(&env);
        let name = s.to_str();
        let mut buf = [0u8; 32];
        let len = name.copy_into_slice(&mut buf);
        let name_str = core::str::from_utf8(&buf[..len as usize]).unwrap();
        if name_str == "deposit" {
            deposit_positions.push_back(i as u32);
        } else if name_str == "deduct" {
            deduct_positions.push_back(i as u32);
        }
    }

    assert!(
        deposit_positions.len() >= 1,
        "expected at least one deposit event"
    );
    assert!(
        deduct_positions.len() >= 1,
        "expected at least one deduct event"
    );

    let last_deposit = deposit_positions.get(deposit_positions.len() - 1).unwrap();
    let first_deduct = deduct_positions.get(0).unwrap();
    assert!(
        last_deposit < first_deduct,
        "deposit event must appear before deduct event in the event log"
    );
}
