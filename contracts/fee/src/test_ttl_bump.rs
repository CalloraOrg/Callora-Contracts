#[cfg(test)]
use crate::{FeeContract, FeeContractClient, INSTANCE_BUMP_AMOUNT};
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn read_ttl(env: &Env, contract_id: &Address) -> u32 {
    env.as_contract(contract_id, || env.storage().instance().get_ttl())
}

struct TtlSetup {
    env: Env,
    admin: Address,
    token: Address,
    contract_id: Address,
}

fn setup_ttl() -> (TtlSetup, FeeContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(Address::generate(&env));
    let contract_id = env.register_contract(None, FeeContract);
    let client = FeeContractClient::new(&env, &contract_id);
    (
        TtlSetup {
            env,
            admin,
            token,
            contract_id,
        },
        client,
    )
}

#[test]
fn test_init_bumps_instance_ttl() {
    let (s, client) = setup_ttl();
    client.init(&s.admin, &s.token, &0u32);
    let ttl = read_ttl(&s.env, &s.contract_id);
    assert_eq!(ttl, INSTANCE_BUMP_AMOUNT);
}
