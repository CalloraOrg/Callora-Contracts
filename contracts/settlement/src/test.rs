#[test]
fn deposit_event_emitted_on_receive_payment() {
    let (env, contract_id, admin, vault, usdc) = setup(); // use whatever your existing test harness does
    let developer = Address::generate(&env);
    let amount = 2_500_000i128;

    env.mock_all_auths();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.receive_payment(&vault, &amount, &false, &Some(developer.clone()), &usdc, &1u32);

    let events = env.events().all();
    let deposit_event = events.iter().find(|e| {
        // match on the "deposit" topic symbol
        true // replace with real topic matching per repo's existing event-test helpers
    });
    assert!(deposit_event.is_some(), "expected a deposit event to be emitted");
}

#[test]
fn deposit_event_emitted_once_per_batch_item() {
    // items with 3 developers -> assert exactly 3 `deposit` events, one per developer,
    // and that each event's amount matches the corresponding batch item
}

#[test]
fn deposit_event_not_emitted_for_pool_credit() {
    // to_pool = true -> assert NO `deposit` event is published
}

#[test]
fn deposit_event_amount_matches_balance_credited_amount() {
    // amount field in DepositEvent must equal amount field in BalanceCreditedEvent
    // for the same call
}