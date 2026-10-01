//! Gas Budget Regression Tests — `callora-migrate`
//!
//! Snapshots CPU/memory cost for every public migration entrypoint exposed
//! by `callora-settlement`'s V1→V2 storage migration, re-exported through the
//! `callora-migrate` crate (see `contracts/migrate/src/lib.rs`).
//!
//! Each test exercises one entrypoint, then reads the host's CPU and memory
//! counters via `env.cost_estimate().resources()` and prints a single JSON
//! line to stdout:
//!
//! ```json
//! {"contract":"callora-migrate","entrypoint":"migrate_v1_to_v2","cpu":341234,"mem":33210}
//! ```
//!
//! `scripts/gas-regression.sh` harvests those lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
//! than 5 %.

#[cfg(test)]
mod gas_budget {
    extern crate std;
    use std::println;

    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    use callora_settlement::{CalloraSettlement, CalloraSettlementClient, StorageKey};

    fn create_settlement(env: &Env) -> (Address, CalloraSettlementClient<'_>) {
        let address = env.register(CalloraSettlement, ());
        (address.clone(), CalloraSettlementClient::new(env, &address))
    }

    fn setup_initialized(env: &Env) -> (Address, CalloraSettlementClient<'_>, Address, Address) {
        let (address, client) = create_settlement(env);
        let admin = Address::generate(env);
        let vault = Address::generate(env);
        let usdc = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        (address, client, admin, usdc)
    }

    /// Seed a single developer's V1 balance directly into persistent storage,
    /// mirroring the pre-migration state a real deployment would have.
    fn seed_v1_developer(env: &Env, contract: &Address, dev: &Address, balance: i128) {
        env.as_contract(contract, || {
            let inst = env.storage().instance();
            let mut index: soroban_sdk::Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| soroban_sdk::Vec::new(env));
            if !index.iter().any(|a| a == *dev) {
                index.push_back(dev.clone());
            }
            inst.set(&StorageKey::DeveloperIndex, &index);
            env.storage()
                .persistent()
                .set(&StorageKey::DeveloperBalanceV1(dev.clone()), &balance);
        });
    }

    fn emit(entrypoint: &str, cpu: u64, mem: u64) {
        println!(
            "{{\"contract\":\"callora-migrate\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
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
    fn gas_budget_migrate_v1_to_v2() {
        let env = Env::default();
        env.mock_all_auths();
        let (address, client, admin, _usdc) = setup_initialized(&env);
        let dev = Address::generate(&env);
        seed_v1_developer(&env, &address, &dev, 1_000);

        measure!(env, "migrate_v1_to_v2", {
            client.migrate_v1_to_v2(&admin);
        });
    }

    #[test]
    fn gas_budget_migrate_v1_to_v2_page() {
        let env = Env::default();
        env.mock_all_auths();
        let (address, client, admin, _usdc) = setup_initialized(&env);
        for _ in 0..10 {
            let dev = Address::generate(&env);
            seed_v1_developer(&env, &address, &dev, 1_000);
        }

        measure!(env, "migrate_v1_to_v2_page", {
            client.migrate_v1_to_v2_page(&admin, &0u32, &10u32);
        });
    }

    #[test]
    fn gas_budget_migrate_developer_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (address, client, admin, _usdc) = setup_initialized(&env);
        let dev = Address::generate(&env);
        seed_v1_developer(&env, &address, &dev, 1_000);

        measure!(env, "migrate_developer_balance", {
            client.migrate_developer_balance(&admin, &dev);
        });
    }

    #[test]
    fn gas_budget_migration_storage_version() {
        let env = Env::default();
        env.mock_all_auths();
        let (_address, client, _admin, _usdc) = setup_initialized(&env);

        measure!(env, "migration_storage_version", {
            let _ = client.migration_storage_version();
        });
    }

    /// Sanity: verify budget API returns non-zero values after a real call.
    #[test]
    fn gas_budget_sanity_nonzero() {
        let env = Env::default();
        env.mock_all_auths();
        let (address, client, admin, _usdc) = setup_initialized(&env);
        let dev = Address::generate(&env);
        seed_v1_developer(&env, &address, &dev, 1_000);
        client.migrate_v1_to_v2(&admin);
        let res = env.cost_estimate().resources();
        assert!(res.instructions > 0, "CPU must be >0");
        assert!(res.read_bytes + res.write_bytes > 0, "mem must be >0");
    }
}
