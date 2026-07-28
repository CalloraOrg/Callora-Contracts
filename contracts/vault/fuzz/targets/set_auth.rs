#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Address, Env};

use callora_vault::CalloraVault;

#[derive(arbitrary::Arbitrary, Debug)]
struct Input {
    auth_enabled: bool,
    set_to_vault_address: bool,
    use_current_nonce: bool,
    nonce: u64,
}

fn create_usdc<'a>(env: &'a Env, admin: &'a Address) -> Address {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    ca.address()
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);

    let vault_addr = env.register(CalloraVault, ());
    let client = callora_vault::CalloraVaultClient::new(&env, &vault_addr);

    let usdc = create_usdc(&env, &owner);

    env.mock_all_auths();
    let _ = client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    if !input.auth_enabled {
        env.set_auths(&[]);
    }

    let target_caller = if input.set_to_vault_address {
        Some(vault_addr.clone())
    } else {
        Some(new_caller)
    };

    let expected_nonce = if input.use_current_nonce {
        client.get_authorized_caller_nonce()
    } else {
        input.nonce
    };

    let before_meta = client.get_meta();
    let before_nonce = client.get_authorized_caller_nonce();

    let result = client.try_set_authorized_caller(&target_caller, &expected_nonce);

    let after_meta = client.get_meta();
    let after_nonce = client.get_authorized_caller_nonce();

    if !input.auth_enabled {
        assert!(result.is_err());
        assert_eq!(after_meta.authorized_caller, before_meta.authorized_caller);
        assert_eq!(after_nonce, before_nonce);
        return;
    }

    if input.set_to_vault_address {
        assert_eq!(
            result,
            Err(Ok(callora_vault::VaultError::AuthorizedCallerCannotBeVault))
        );
        assert_eq!(after_meta.authorized_caller, before_meta.authorized_caller);
        assert_eq!(after_nonce, before_nonce);
        return;
    }

    if input.use_current_nonce {
        assert!(result.is_ok());
        assert_eq!(after_nonce, before_nonce.wrapping_add(1));
    } else {
        assert!(result.is_err());
        assert_eq!(after_nonce, before_nonce);
    }
});
