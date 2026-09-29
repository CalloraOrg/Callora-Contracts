use callora_settlement::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

#[test]
fn minimum_balance_aliases_are_exposed_and_persisted() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let dev = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    client.set_minimum_balance(&admin, &dev, &150i128);
    assert_eq!(client.get_minimum_balance(&dev), 150i128);

    client.set_developer_min_balance(&admin, &dev, &250i128);
    assert_eq!(client.get_minimum_balance(&dev), 250i128);
}
