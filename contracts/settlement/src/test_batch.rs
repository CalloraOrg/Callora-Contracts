#[cfg(test)]
mod test {
    use crate::{CalloraSettlement, CalloraSettlementClient, batch::{SettleInput, SettleOutcome}};
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::token as token_mod;
    use soroban_sdk::{Address, Env, IntoVal, Symbol, Vec, String};

    fn setup() -> (Env, Address, Address, Address, Address, token_mod::StellarAssetClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
        let token_client = token_mod::StellarAssetClient::new(&env, &token_addr);

        let contract = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &contract);
        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &token_addr);
        
        (env, contract, admin, vault, token_addr, token_client)
    }

    #[test]
    #[should_panic]
    fn test_mid_batch_failure_reverts() {
        let (env, contract, _admin, vault, token_addr, token_client) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let dev1_to = Address::generate(&env);
        let dev2_to = Address::generate(&env);

        let _offering_id = String::from_str(&env, "offering1");

        // Admin forces some balance for dev1, but NOT for dev2.
        client.force_credit_developer(&_admin, &dev1, &100, &token_addr, &Symbol::new(&env, "reason"));
        // Mint to vault so it has tokens
        token_client.mint(&contract, &1000);
        
        let mut settlements = Vec::new(&env);
        settlements.push_back(SettleInput {
            developer: dev1.clone(),
            amount: 50,
            to: Some(dev1_to.clone()),
        });
        settlements.push_back(SettleInput {
            developer: dev2.clone(),
            amount: 50, // This will fail because dev2 has 0 balance!
            to: Some(dev2_to.clone()),
        });

        // This should panic and revert
        client.batch_settle(&settlements);
    }

    #[test]
    fn test_batch_success_conserves_value() {
        let (env, contract, _admin, _vault, token_addr, token_client) = setup();
        let token_user_client = token_mod::Client::new(&env, &token_addr);
        let client = CalloraSettlementClient::new(&env, &contract);

        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let dev1_to = Address::generate(&env);
        let dev2_to = Address::generate(&env);

        client.force_credit_developer(&_admin, &dev1, &100, &token_addr, &Symbol::new(&env, "reason1"));
        client.force_credit_developer(&_admin, &dev2, &100, &token_addr, &Symbol::new(&env, "reason2"));

        // Mint enough for the withdrawals
        token_client.mint(&contract, &200);

        let mut settlements = Vec::new(&env);
        settlements.push_back(SettleInput {
            developer: dev1.clone(),
            amount: 50,
            to: Some(dev1_to.clone()),
        });
        settlements.push_back(SettleInput {
            developer: dev2.clone(),
            amount: 70,
            to: Some(dev2_to.clone()),
        });

        client.batch_settle(&settlements);

        // Check balances are conserved (dev1 balance 100 - 50 = 50, dev2 balance 100 - 70 = 30)
        assert_eq!(client.get_developer_balance(&dev1, &token_addr), 50);
        assert_eq!(client.get_developer_balance(&dev2, &token_addr), 30);
        assert_eq!(token_user_client.balance(&dev1_to), 50);
        assert_eq!(token_user_client.balance(&dev2_to), 70);
        assert_eq!(token_user_client.balance(&contract), 80); // 200 - 50 - 70 = 80
    }
}
