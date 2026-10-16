#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol};

pub mod errors;
pub use errors::ContractError;

/// Ledgers per day at a 5-second close cadence.
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL extension trigger for instance storage keys (~30 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// TTL extension target for instance storage keys (~60 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// Maximum fee rate in basis points (100% = 10_000 bps).
const MAX_BASIS_POINTS: u32 = 10_000;

/// StrKey of the zero contract address (all 32 bytes are zero).
/// Rejected as a fee-withdrawal destination to avoid burning accumulated fees.
const ZERO_CONTRACT_ADDRESS: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

/// Storage key for the fee configuration.
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    FeeBps,
    Accumulated,
}

/// Fee contract configuration stored on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    /// Fee rate in basis points (1 bp = 0.01%).
    pub fee_bps: u32,
    /// Maximum fee rate in basis points (slippage guard).
    pub max_fee_bps: u32,
}

#[contract]
pub struct FeeContract;

#[contractimpl]
impl FeeContract {
    /// Extend instance storage TTL to prevent state archival.
    #[inline]
    pub fn bump_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Initialise the fee contract with an admin and fee token.
    ///
    /// # Arguments
    /// * `admin` - Address that will hold admin privileges.
    /// * `token` - Token contract address used for fee deposits and withdrawals.
    ///
    /// # Errors
    /// - `AlreadyInitialized` if the contract is already initialised.
    pub fn init(env: Env, admin: Address, token: Address) -> Result<(), ContractError> {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::FeeBps, &0u32);
        env.storage().instance().set(&DataKey::Accumulated, &0i128);

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Set the fee rate (in basis points).
    ///
    /// # Arguments
    /// * `admin` - The admin address (must match stored admin).
    /// * `fee_bps` - Fee rate in basis points (0–10_000).
    ///
    /// # Errors
    /// - `NotInitialized` if the contract is not initialised.
    /// - `Unauthorized` if the caller is not the stored admin.
    /// - `FeeTooHigh` if `fee_bps` exceeds `MAX_BASIS_POINTS`.
    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), ContractError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ContractError::Unauthorized);
        }
        if fee_bps > MAX_BASIS_POINTS {
            return Err(ContractError::FeeTooHigh);
        }

        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Deposit fees into the accumulated balance by transferring `amount` of the
    /// configured fee token from `caller` to this contract.
    ///
    /// # Arguments
    /// * `caller` - The address depositing fees.
    /// * `amount` - The amount to deposit (must be positive).
    ///
    /// # Errors
    /// - `NotInitialized` if the contract is not initialised.
    /// - `InvalidAmount` if `amount` is not positive.
    /// - `Overflow` if the accumulated balance would overflow.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> Result<(), ContractError> {
        caller.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(ContractError::NotInitialized)?;
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        let accumulated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Accumulated)
            .unwrap_or(0);
        let new_accumulated = accumulated
            .checked_add(amount)
            .ok_or(ContractError::Overflow)?;

        let contract_addr = env.current_contract_address();
        token::Client::new(&env, &token_addr).transfer(&caller, &contract_addr, &amount);

        env.storage()
            .instance()
            .set(&DataKey::Accumulated, &new_accumulated);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Validate that a recipient is a legitimate destination for withdrawn fees.
    ///
    /// Rejects the contract's own address (which would self-credit accumulated
    /// fees) and the zero address (which would burn them), ensuring the check
    /// runs before any value moves. See [`Self::withdraw`].
    ///
    /// # Errors
    /// - `InvalidRecipient` if `recipient` is the contract itself or the zero address.
    fn validate_recipient(env: &Env, recipient: &Address) -> Result<(), ContractError> {
        if recipient == &env.current_contract_address() {
            return Err(ContractError::InvalidRecipient);
        }
        let zero = Address::from_str(env, ZERO_CONTRACT_ADDRESS);
        if recipient == &zero {
            return Err(ContractError::InvalidRecipient);
        }
        Ok(())
    }

    /// Withdraw accumulated fees to a recipient by transferring `amount` of the
    /// configured fee token from this contract to `recipient`.
    ///
    /// # Arguments
    /// * `admin` - The admin address (must match stored admin).
    /// * `recipient` - Address to receive the withdrawn fees.
    /// * `amount` - The amount to withdraw.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract is not initialised.
    /// - `Unauthorized` if the caller is not the stored admin.
    /// - `InvalidAmount` if `amount` is not positive.
    /// - `InvalidRecipient` if `recipient` is the contract itself or the zero address.
    /// - `InsufficientBalance` if accumulated fees are less than `amount`.
    /// - `Overflow` if the remaining balance would underflow.
    pub fn withdraw(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ContractError::Unauthorized);
        }
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        Self::validate_recipient(&env, &recipient)?;

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(ContractError::NotInitialized)?;

        let accumulated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Accumulated)
            .unwrap_or(0);
        if accumulated < amount {
            return Err(ContractError::InsufficientBalance);
        }

        let remaining = accumulated
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;

        let contract_addr = env.current_contract_address();
        let token_client = token::Client::new(&env, &token_addr);
        if token_client.balance(&contract_addr) < amount {
            return Err(ContractError::InsufficientBalance);
        }

        env.storage()
            .instance()
            .set(&DataKey::Accumulated, &remaining);

        token_client.transfer(&contract_addr, &recipient, &amount);

        env.events()
            .publish((Symbol::new(&env, "fee_withdrawn"),), (recipient, amount));

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Return the current fee configuration.
    pub fn get_fee_config(env: Env) -> Result<FeeConfig, ContractError> {
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FeeBps)
            .ok_or(ContractError::NotInitialized)?;
        Self::bump_instance_ttl(&env);
        Ok(FeeConfig {
            fee_bps,
            max_fee_bps: MAX_BASIS_POINTS,
        })
    }

    /// Return the total accumulated fees.
    pub fn get_accumulated(env: Env) -> Result<i128, ContractError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::NotInitialized);
        }
        let accumulated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Accumulated)
            .unwrap_or(0);
        Self::bump_instance_ttl(&env);
        Ok(accumulated)
    }

    /// Return the stored admin address.
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        Self::bump_instance_ttl(&env);
        Ok(admin)
    }

    /// Return the configured fee token address.
    pub fn get_token(env: Env) -> Result<Address, ContractError> {
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(ContractError::NotInitialized)?;
        Self::bump_instance_ttl(&env);
        Ok(token)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token, Address, Env};

    fn create_token<'a>(
        env: &'a Env,
        admin: &Address,
    ) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
        let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
        let address = contract_address.address();
        let client = token::Client::new(env, &address);
        let admin_client = token::StellarAssetClient::new(env, &address);
        (address, client, admin_client)
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert_eq!(client.get_token(), token_addr);
        assert_eq!(
            client.try_init(&admin, &token_addr).unwrap_err().unwrap(),
            ContractError::AlreadyInitialized
        );
    }

    #[test]
    fn test_set_fee() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        client.set_fee(&admin, &250);

        let config = client.get_fee_config();
        assert_eq!(config.fee_bps, 250);
    }

    #[test]
    fn test_set_fee_too_high() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert_eq!(
            client.try_set_fee(&admin, &10_001).unwrap_err().unwrap(),
            ContractError::FeeTooHigh
        );
    }

    #[test]
    fn test_deposit_and_withdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let recipient = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &1_500);
        client.init(&admin, &token_addr);
        assert_eq!(client.get_token(), token_addr);

        client.deposit(&caller, &1_000);
        assert_eq!(client.get_accumulated(), 1_000);
        assert_eq!(token_client.balance(&contract_id), 1_000);
        assert_eq!(token_client.balance(&caller), 500);
        assert!(client.get_accumulated() <= token_client.balance(&contract_id));

        client.withdraw(&admin, &recipient, &400);
        assert_eq!(client.get_accumulated(), 600);
        assert_eq!(token_client.balance(&contract_id), 600);
        assert_eq!(token_client.balance(&recipient), 400);
        assert!(client.get_accumulated() <= token_client.balance(&contract_id));
    }

    #[test]
    fn test_deposit_without_token_balance_fails_and_keeps_counter_in_sync() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let (token_addr, token_client, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert!(client.try_deposit(&caller, &500).is_err());
        assert_eq!(client.get_accumulated(), 0);
        assert_eq!(token_client.balance(&contract_id), 0);
    }

    #[test]
    fn test_deposit_invalid_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert_eq!(
            client.try_deposit(&caller, &0).unwrap_err().unwrap(),
            ContractError::InvalidAmount
        );
        assert_eq!(
            client.try_deposit(&caller, &-1).unwrap_err().unwrap(),
            ContractError::InvalidAmount
        );
    }

    #[test]
    fn test_withdraw_insufficient() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let recipient = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &100);
        client.init(&admin, &token_addr);
        client.deposit(&caller, &100);
        assert_eq!(
            client
                .try_withdraw(&admin, &recipient, &200)
                .unwrap_err()
                .unwrap(),
            ContractError::InsufficientBalance
        );
        assert_eq!(client.get_accumulated(), 100);
        assert_eq!(token_client.balance(&contract_id), 100);
    }

    #[test]
    fn test_unauthorized_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let fake_admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert_eq!(
            client.try_set_fee(&fake_admin, &100).unwrap_err().unwrap(),
            ContractError::Unauthorized
        );
        assert_eq!(
            client
                .try_withdraw(&fake_admin, &recipient, &10)
                .unwrap_err()
                .unwrap(),
            ContractError::Unauthorized
        );
    }

    #[test]
    fn test_not_initialized_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        assert_eq!(
            client.try_set_fee(&admin, &100).unwrap_err().unwrap(),
            ContractError::NotInitialized
        );
        assert_eq!(
            client.try_get_fee_config().unwrap_err().unwrap(),
            ContractError::NotInitialized
        );
        assert_eq!(
            client.try_get_token().unwrap_err().unwrap(),
            ContractError::NotInitialized
        );
    }

    #[test]
    fn test_get_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let (token_addr, _, _) = create_token(&env, &admin);

        client.init(&admin, &token_addr);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_overflow_protected_deposit() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &i128::MAX);
        client.init(&admin, &token_addr);
        client.deposit(&caller, &i128::MAX);
        assert_eq!(token_client.balance(&contract_id), i128::MAX);
        assert_eq!(
            client.try_deposit(&caller, &1).unwrap_err().unwrap(),
            ContractError::Overflow
        );
    }

    #[test]
    fn test_withdraw_to_contract_self_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &1_000);
        client.init(&admin, &token_addr);
        client.deposit(&caller, &1_000);

        assert_eq!(
            client
                .try_withdraw(&admin, &contract_id, &400)
                .unwrap_err()
                .unwrap(),
            ContractError::InvalidRecipient
        );
        assert_eq!(client.get_accumulated(), 1_000);
        assert_eq!(token_client.balance(&contract_id), 1_000);
    }

    #[test]
    fn test_withdraw_to_zero_address_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &1_000);
        client.init(&admin, &token_addr);
        client.deposit(&caller, &1_000);

        let zero = Address::from_str(&env, ZERO_CONTRACT_ADDRESS);
        assert_eq!(
            client
                .try_withdraw(&admin, &zero, &400)
                .unwrap_err()
                .unwrap(),
            ContractError::InvalidRecipient
        );
        assert_eq!(client.get_accumulated(), 1_000);
        assert_eq!(token_client.balance(&contract_id), 1_000);
    }

    #[test]
    fn test_withdraw_to_valid_recipient_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, FeeContract);
        let client = FeeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let recipient = Address::generate(&env);
        let (token_addr, token_client, token_admin) = create_token(&env, &admin);

        token_admin.mint(&caller, &1_000);
        client.init(&admin, &token_addr);
        client.deposit(&caller, &1_000);

        client.withdraw(&admin, &recipient, &400);
        assert_eq!(client.get_accumulated(), 600);
        assert_eq!(token_client.balance(&contract_id), 600);
        assert_eq!(token_client.balance(&recipient), 400);
    }
}

#[cfg(test)]
mod test_ttl_bump;
