#![cfg(test)]

use crate::{CalloraSettlement, CalloraVault};
use soroban_sdk::{contractimpl, contracttype, i128, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TestDeductItem {
    pub amount: i128,
    pub request_id: Option<Symbol>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentFlowTest {
    pub vault_address: Address,
    pub settlement_address: Address,
    pub developer: Address,
    pub initial_vault_balance: i128,
    pub payment_amount: i128,
    pub final_settlement_balance: i128,
}

#[contractimpl]
impl CalloraSettlement {
    #[cfg(test)]
    pub fn __test_init(env: Env, admin: Address, vault_address: Address) {
        Self::init(env, admin, vault_address);
    }

    #[cfg(test)]
    pub fn __test_get_admin(env: Env) -> Address {
        Self::get_admin(env)
    }

    #[cfg(test)]
    pub fn __test_get_vault(env: Env) -> Address {
        Self::get_vault(env)
    }

    #[cfg(test)]
    pub fn __test_get_global_pool(env: Env) -> crate::GlobalPool {
        Self::get_global_pool(env)
    }

    #[cfg(test)]
    pub fn __test_get_developer_balance(env: Env, developer: Address) -> i128 {
        Self::get_developer_balance(env, developer)
    }

    #[cfg(test)]
    pub fn __test_get_all_developer_balances(env: Env) -> Vec<crate::DeveloperBalance> {
        Self::get_all_developer_balances(env)
    }
}

#[cfg(test)]
mod settlement_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token};

    #[test]
    fn test_settlement_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);

        // Test successful initialization
        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        assert_eq!(CalloraSettlement::__test_get_admin(env.clone()), admin);
        assert_eq!(CalloraSettlement::__test_get_vault(env.clone()), vault);

        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, 0);
        assert!(global_pool.last_updated > 0);
    }

    #[test]
    fn test_receive_payment_to_pool() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);

        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        // Test receiving payment to global pool
        let payment_amount = 1000i128;
        CalloraSettlement::receive_payment(
            env.clone(),
            vault.clone(), // vault is authorized caller
            payment_amount,
            true, // to_pool = true
            None, // no developer needed for pool
        );

        // Verify global pool balance updated
        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, payment_amount);
        assert!(global_pool.last_updated > 0);
    }

    #[test]
    fn test_receive_payment_to_developer() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);

        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        // Test receiving payment to specific developer
        let payment_amount = 500i128;
        CalloraSettlement::receive_payment(
            env.clone(),
            vault.clone(), // vault is authorized caller
            payment_amount,
            false,                   // to_pool = false
            Some(developer.clone()), // specify developer
        );

        // Verify developer balance updated
        let balance =
            CalloraSettlement::__test_get_developer_balance(env.clone(), developer.clone());
        assert_eq!(balance, payment_amount);

        // Verify global pool unchanged
        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, 0);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller must be vault or admin")]
    fn test_receive_payment_unauthorized() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        // Test unauthorized caller
        CalloraSettlement::receive_payment(
            env.clone(),
            unauthorized, // not vault or admin
            100i128,
            true,
            None,
        );
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_receive_payment_zero_amount() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);

        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        // Test zero amount
        CalloraSettlement::receive_payment(env.clone(), vault.clone(), 0i128, true, None);
    }

    #[test]
    #[should_panic(expected = "developer address required when to_pool=false")]
    fn test_receive_payment_pool_false_no_developer() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);

        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault.clone());

        // Test pool=false without developer
        CalloraSettlement::receive_payment(
            env.clone(),
            vault.clone(),
            100i128,
            false, // to_pool = false
            None,  // no developer specified
        );
    }
}

#[cfg(test)]
mod vault_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token};

    #[test]
    fn test_vault_settlement_address() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let usdc = Address::generate(&env);
        let settlement = Address::generate(&env);

        // Initialize vault
        let contract_id = env.register_contract(crate::CalloraVault, ());
        let client = crate::CalloraVaultClient::new(&env, &contract_id);
        client.init(&admin, &usdc, &Some(1000i128), &Some(100i128));

        // Set settlement address
        client.set_settlement(&admin, &settlement);

        // Verify settlement address is set
        assert_eq!(client.get_settlement(), settlement);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_vault_settlement_address_unauthorized() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let usdc = Address::generate(&env);
        let settlement = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        // Initialize vault
        let contract_id = env.register_contract(crate::CalloraVault, ());
        let client = crate::CalloraVaultClient::new(&env, &contract_id);
        client.init(&admin, &usdc, &Some(1000i128), &Some(100i128));

        // Try to set settlement address as unauthorized user
        client.set_settlement(&unauthorized, &settlement);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token};

    #[test]
    fn test_end_to_end_payment_flow() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault_owner = Address::generate(&env);
        let developer = Address::generate(&env);
        let usdc = Address::generate(&env);

        // Initialize vault contract
        let vault_contract_id = env.register_contract(crate::CalloraVault, ());
        let vault_client = crate::CalloraVaultClient::new(&env, &vault_contract_id);
        vault_client.init(&vault_owner, &usdc, &Some(10000i128), &Some(100i128));

        // Initialize settlement contract
        let settlement_contract_id = env.register_contract(crate::CalloraSettlement, ());
        let settlement_client = CalloraSettlement::new(&env, &settlement_contract_id);
        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault_contract_id);

        // Set settlement address in vault
        vault_client.set_settlement(&admin, &settlement_contract_id);

        // Mock USDC token for testing
        env.register_token(&usdc, &admin);
        token::Client::new(&env, &usdc).mint(&vault_contract_id, &20000i128);

        // Perform deduct (should trigger transfer to settlement)
        let deduct_amount = 1000i128;
        vault_client.deduct(&vault_contract_id, deduct_amount, None);

        // Verify vault balance decreased
        let vault_balance = vault_client.balance();
        assert_eq!(vault_balance, 9000i128); // 10000 - 1000

        // Verify settlement received payment (to pool)
        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, deduct_amount);

        // Test multiple payments
        vault_client.deduct(&vault_contract_id, &500i128, None);
        assert_eq!(global_pool.total_balance, 1500i128); // 1000 + 500
    }

    #[test]
    fn test_end_to_end_developer_payment_flow() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault_owner = Address::generate(&env);
        let developer = Address::generate(&env);
        let usdc = Address::generate(&env);

        // Initialize contracts
        let vault_contract_id = env.register_contract(crate::CalloraVault, ());
        let vault_client = crate::CalloraVaultClient::new(&env, &vault_contract_id);
        vault_client.init(&vault_owner, &usdc, &Some(10000i128), &Some(100i128));

        let settlement_contract_id = env.register_contract(crate::CalloraSettlement, ());
        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault_contract_id);
        vault_client.set_settlement(&admin, &settlement_contract_id);

        // Mock USDC token
        env.register_token(&usdc, &admin);
        token::Client::new(&env, &usdc).mint(&vault_contract_id, &20000i128);

        // Receive payment to specific developer (not pool)
        let payment_amount = 750i128;
        settlement_client.receive_payment(
            vault_contract_id, // vault is authorized caller
            payment_amount,
            false,           // to_pool = false
            Some(developer), // credit to specific developer
        );

        // Verify developer balance
        let dev_balance = CalloraSettlement::__test_get_developer_balance(env.clone(), developer);
        assert_eq!(dev_balance, payment_amount);

        // Verify global pool unchanged
        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, 0);
    }

    #[test]
    fn test_batch_deduct_with_settlement_transfer() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let vault_owner = Address::generate(&env);
        let usdc = Address::generate(&env);

        // Initialize vault
        let vault_contract_id = env.register_contract(crate::CalloraVault, ());
        let vault_client = crate::CalloraVaultClient::new(&env, &vault_contract_id);
        vault_client.init(&vault_owner, &usdc, &Some(10000i128), &Some(100i128));

        let settlement_contract_id = env.register_contract(crate::CalloraSettlement, ());
        CalloraSettlement::__test_init(env.clone(), admin.clone(), vault_contract_id);
        vault_client.set_settlement(&admin, &settlement_contract_id);

        // Mock USDC token
        env.register_token(&usdc, &admin);
        token::Client::new(&env, &usdc).mint(&vault_contract_id, &20000i128);

        // Create batch deduct items
        let mut items = Vec::new(&env);
        items.push_back(TestDeductItem {
            amount: 1000i128,
            request_id: Some(Symbol::new(&env, "req1")),
        });
        items.push_back(TestDeductItem {
            amount: 500i128,
            request_id: Some(Symbol::new(&env, "req2")),
        });
        items.push_back(TestDeductItem {
            amount: 250i128,
            request_id: None,
        });

        // Perform batch deduct (total: 1750)
        vault_client.batch_deduct(&vault_contract_id, items);

        // Verify vault balance
        let vault_balance = vault_client.balance();
        assert_eq!(vault_balance, 8250i128); // 10000 - 1750

        // Verify settlement received total payment
        let global_pool = CalloraSettlement::__test_get_global_pool(env.clone());
        assert_eq!(global_pool.total_balance, 1750i128);
    }
}
