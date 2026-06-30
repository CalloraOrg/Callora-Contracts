/// # Gas Budget Regression Tests — `callora-vault`
///
/// Each test exercises one public entrypoint, then reads the host's CPU and
/// memory counters via `soroban_sdk::testutils::budget` and prints a single
/// JSON line to stdout:
///
/// ```json
/// {"contract":"callora-vault","entrypoint":"deposit","cpu":341234,"mem":33210}
/// ```
///
/// `scripts/gas-regression.sh` harvests those lines, compares them against
/// `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
/// than 5 %.
#[cfg(test)]
mod gas_budget {
    extern crate std;
    use std::println;

    use soroban_sdk::{testutils::Address as _, token, Address, Env};

    use crate::{CalloraVault, CalloraVaultClient, DeductItem};
    use callora_settlement::{CalloraSettlement, CalloraSettlementClient};

    fn create_usdc<'a>(
        env: &'a Env,
        admin: &Address,
    ) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
        let addr = env.register_stellar_asset_contract_v2(admin.clone());
        let address = addr.address();
        (
            address.clone(),
            token::Client::new(env, &address),
            token::StellarAssetClient::new(env, &address),
        )
    }

    fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
        let address = env.register(CalloraVault, ());
        (address.clone(), CalloraVaultClient::new(env, &address))
    }

    fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
        let addr = env.register(CalloraSettlement, ());
        CalloraSettlementClient::new(env, &addr).init(admin, vault_address);
        addr
    }

    fn fund_vault(admin: &token::StellarAssetClient, vault_address: &Address, amount: i128) {
        admin.mint(vault_address, &amount);
    }

    fn emit(entrypoint: &str, cpu: u64, mem: u64) {
        println!(
            "{{\"contract\":\"callora-vault\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
        );
    }

    macro_rules! measure {
        ($env:expr, $ep:literal, $body:expr) => {{
            $body;
            let res = $env.cost_estimate().resources();
            let cpu = res.instructions as u64;
            let mem = res.read_bytes as u64 + res.write_bytes as u64;
            emit($ep, cpu, mem);
        }};
    }

    #[test]
    fn gas_budget_init() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        measure!(env, "init", {
            client.init(
                &owner,
                &usdc_address,
                &Some(1_000),
                &None,
                &None,
                &None,
                &None,
            );
        });
    }

    #[test]
    fn gas_budget_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        usdc_admin.mint(&owner, &500);
        usdc_client.approve(&owner, &vault_address, &500, &10_000);
        measure!(env, "deposit", {
            client.deposit(&owner, &500);
        });
    }

    #[test]
    fn gas_budget_deduct() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let settlement = create_settlement(&env, &owner, &vault_address);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 5_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(5_000),
            &None,
            &None,
            &None,
            &None,
        );
        client.set_settlement(&owner, &settlement);
        measure!(env, "deduct", {
            client.deduct(&owner, &100, &None);
        });
    }

    #[test]
    fn gas_budget_batch_deduct() {
        use soroban_sdk::Vec;
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let settlement = create_settlement(&env, &owner, &vault_address);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 10_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(10_000),
            &None,
            &None,
            &None,
            &None,
        );
        client.set_settlement(&owner, &settlement);
        let mut items: Vec<DeductItem> = Vec::new(&env);
        items.push_back(DeductItem {
            amount: 100,
            request_id: None,
        });
        items.push_back(DeductItem {
            amount: 200,
            request_id: None,
        });
        items.push_back(DeductItem {
            amount: 300,
            request_id: None,
        });
        measure!(env, "batch_deduct", {
            client.batch_deduct(&owner, &items);
        });
    }

    #[test]
    fn gas_budget_set_allowed_depositor() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let depositor = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "set_allowed_depositor", {
            client.set_allowed_depositor(&owner, &Some(depositor));
        });
    }

    #[test]
    fn gas_budget_clear_allowed_depositors() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let depositor = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        client.set_allowed_depositor(&owner, &Some(depositor));
        measure!(env, "clear_allowed_depositors", {
            client.clear_allowed_depositors(&owner);
        });
    }

    #[test]
    fn gas_budget_set_authorized_caller() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let new_caller = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        let nonce = client.get_authorized_caller_nonce();
        measure!(env, "set_authorized_caller", {
            client.set_authorized_caller(&Some(new_caller), &nonce);
        });
    }

    #[test]
    fn gas_budget_set_max_deduct() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "set_max_deduct", {
            client.set_max_deduct(&5_000);
        });
    }

    #[test]
    fn gas_budget_pause() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "pause", {
            client.pause(&owner);
        });
    }

    #[test]
    fn gas_budget_unpause() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        client.pause(&owner);
        measure!(env, "unpause", {
            client.unpause(&owner);
        });
    }

    #[test]
    fn gas_budget_get_meta() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "get_meta", {
            let _ = client.get_meta();
        });
    }

    #[test]
    fn gas_budget_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "balance", {
            let _ = client.balance();
        });
    }

    #[test]
    fn gas_budget_is_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "is_paused", {
            let _ = client.is_paused();
        });
    }

    #[test]
    fn gas_budget_get_max_deduct() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "get_max_deduct", {
            let _ = client.get_max_deduct();
        });
    }

    #[test]
    fn gas_budget_get_contract_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "get_contract_addresses", {
            let _ = client.get_contract_addresses();
        });
    }

    #[test]
    fn gas_budget_is_authorized_depositor() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let depositor = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        client.init(
            &owner,
            &usdc_address,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
        measure!(env, "is_authorized_depositor", {
            let _ = client.is_authorized_depositor(&depositor);
        });
    }

    /// Sanity: verify budget API returns non-zero values after a real call.
    #[test]
    fn gas_budget_sanity_nonzero() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &owner);
        fund_vault(&usdc_admin, &vault_address, 1_000);
        env.cost_estimate().resources(); // warm up
        let res = env.cost_estimate().resources();
        assert!(res.instructions > 0, "CPU must be >0");
        assert!(res.read_bytes + res.write_bytes > 0, "mem must be >0");
    }
}
