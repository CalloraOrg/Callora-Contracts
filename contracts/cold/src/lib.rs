#![no_std]

#[cfg(test)]
extern crate std;

pub mod errors;
pub mod events;

use errors::ColdError;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

const BPS_DENOMINATOR: i128 = 10_000;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Admin,
    PendingAdmin,
    ColdConfig,
    ColdBalances,
    PendingColdSweep,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ColdConfig {
    pub hot_bps: u32,
    pub rebalance_threshold_bps: u32,
    pub cold_signers: Vec<Address>,
    pub cold_threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ColdBalances {
    pub hot: i128,
    pub cold: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingColdSweep {
    pub amount: i128,
    pub destination: Address,
    pub approvals: Vec<Address>,
    pub proposed_at: u64,
}

#[contract]
pub struct ColdStorage;

#[contractimpl]
impl ColdStorage {
    pub fn init(env: Env, admin: Address) -> Result<(), ColdError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(ColdError::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.events()
            .publish((events::event_init(&env), admin), ());
        Ok(())
    }

    fn admin(env: &Env) -> Result<Address, ColdError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(ColdError::NotInitialized)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), ColdError> {
        caller.require_auth();
        let current = Self::admin(env)?;
        if caller != &current {
            return Err(ColdError::Unauthorized);
        }
        Ok(())
    }

    pub fn get_admin(env: Env) -> Result<Address, ColdError> {
        Self::admin(&env)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    pub fn set_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), ColdError> {
        Self::require_admin(&env, &caller)?;
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events()
            .publish((events::event_admin_nominated(&env), caller, new_admin), ());
        Ok(())
    }

    pub fn accept_admin(env: Env, caller: Address) -> Result<(), ColdError> {
        caller.require_auth();
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(ColdError::Unauthorized)?;
        if caller != pending {
            return Err(ColdError::Unauthorized);
        }
        env.storage().instance().set(&StorageKey::Admin, &caller);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_accepted(&env), caller), ());
        Ok(())
    }

    pub fn cancel_admin(env: Env, caller: Address) -> Result<(), ColdError> {
        Self::require_admin(&env, &caller)?;
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_cancelled(&env), caller), ());
        Ok(())
    }

    pub fn set_config(env: Env, caller: Address, config: ColdConfig) -> Result<(), ColdError> {
        Self::require_admin(&env, &caller)?;
        Self::validate_config(&config)?;
        env.storage()
            .instance()
            .set(&StorageKey::ColdConfig, &config);
        env.events()
            .publish((events::event_config_set(&env), caller), ());
        Ok(())
    }

    pub fn get_config(env: Env) -> Option<ColdConfig> {
        env.storage().instance().get(&StorageKey::ColdConfig)
    }

    pub fn set_balances(
        env: Env,
        caller: Address,
        balances: ColdBalances,
    ) -> Result<(), ColdError> {
        Self::require_admin(&env, &caller)?;
        if balances.hot < 0 || balances.cold < 0 {
            return Err(ColdError::InvalidHotBps);
        }
        balances
            .hot
            .checked_add(balances.cold)
            .ok_or(ColdError::Overflow)?;
        env.storage()
            .instance()
            .set(&StorageKey::ColdBalances, &balances);
        env.events()
            .publish((events::event_balances_set(&env), caller), ());
        Ok(())
    }

    pub fn get_balances(env: Env) -> Option<ColdBalances> {
        env.storage().instance().get(&StorageKey::ColdBalances)
    }

    fn validate_config(config: &ColdConfig) -> Result<(), ColdError> {
        if config.hot_bps == 0 || config.hot_bps > BPS_DENOMINATOR as u32 {
            return Err(ColdError::InvalidHotBps);
        }
        if config.rebalance_threshold_bps == 0
            || config.rebalance_threshold_bps > BPS_DENOMINATOR as u32
        {
            return Err(ColdError::InvalidRebalanceThreshold);
        }
        if config.cold_signers.is_empty() {
            return Err(ColdError::ColdSignersEmpty);
        }
        if config.cold_threshold == 0 || config.cold_threshold > config.cold_signers.len() {
            return Err(ColdError::InvalidColdThreshold);
        }
        for i in 0..config.cold_signers.len() {
            for j in (i + 1)..config.cold_signers.len() {
                if config.cold_signers.get(i) == config.cold_signers.get(j) {
                    return Err(ColdError::DuplicateColdSigner);
                }
            }
        }
        Ok(())
    }

    fn get_config_or_err(env: &Env) -> Result<ColdConfig, ColdError> {
        env.storage()
            .instance()
            .get(&StorageKey::ColdConfig)
            .ok_or(ColdError::NotInitialized)
    }

    pub fn propose_cold_sweep(
        env: Env,
        caller: Address,
        amount: i128,
        destination: Address,
    ) -> Result<(), ColdError> {
        caller.require_auth();
        let config = Self::get_config_or_err(&env)?;
        if !config.is_cold_signer(&caller) {
            return Err(ColdError::NotColdSigner);
        }
        if amount <= 0 {
            return Err(ColdError::AmountNotPositive);
        }
        if env.storage().instance().has(&StorageKey::PendingColdSweep) {
            return Err(ColdError::SweepExists);
        }
        let mut approvals: Vec<Address> = Vec::new(&env);
        approvals.push_back(caller.clone());
        let sweep = PendingColdSweep {
            amount,
            destination: destination.clone(),
            approvals,
            proposed_at: env.ledger().timestamp(),
        };
        env.storage()
            .instance()
            .set(&StorageKey::PendingColdSweep, &sweep);
        env.events()
            .publish((events::event_cold_sweep_proposed(&env), caller, destination), amount);
        Ok(())
    }

    pub fn approve_cold_sweep(env: Env, caller: Address) -> Result<(), ColdError> {
        caller.require_auth();
        let config = Self::get_config_or_err(&env)?;
        if !config.is_cold_signer(&caller) {
            return Err(ColdError::NotColdSigner);
        }
        let mut sweep: PendingColdSweep = env
            .storage()
            .instance()
            .get(&StorageKey::PendingColdSweep)
            .ok_or(ColdError::NoSweepPending)?;
        if sweep.approvals.iter().any(|a| a == caller) {
            return Err(ColdError::AlreadyApproved);
        }
        sweep.approvals.push_back(caller.clone());
        let threshold = config.cold_threshold as usize;
        let approved = sweep.approvals.len() >= threshold;
        if approved {
            env.storage()
                .instance()
                .remove(&StorageKey::PendingColdSweep);
            env.events()
                .publish(
                    (events::event_cold_sweep_executed(&env), caller, sweep.destination),
                    sweep.amount,
                );
        } else {
            env.storage()
                .instance()
                .set(&StorageKey::PendingColdSweep, &sweep);
            env.events()
                .publish((events::event_cold_sweep_approved(&env), caller), ());
        }
        Ok(())
    }

    pub fn get_cold_sweep(env: Env) -> Option<PendingColdSweep> {
        env.storage().instance().get(&StorageKey::PendingColdSweep)
    }

    pub fn hot_share_bps(env: Env) -> i128 {
        let balances = env
            .storage()
            .instance()
            .get::<_, ColdBalances>(&StorageKey::ColdBalances)
            .unwrap_or(ColdBalances { hot: 0, cold: 0 });
        let total = balances.hot + balances.cold;
        if total == 0 {
            return 0;
        }
        balances
            .hot
            .checked_mul(BPS_DENOMINATOR)
            .unwrap_or(0)
            .checked_div(total)
            .unwrap_or(0)
    }

    pub fn target_hot_amount(env: Env) -> i128 {
        let config = env
            .storage()
            .instance()
            .get::<_, ColdConfig>(&StorageKey::ColdConfig);
        let balances = env
            .storage()
            .instance()
            .get::<_, ColdBalances>(&StorageKey::ColdBalances)
            .unwrap_or(ColdBalances { hot: 0, cold: 0 });
        let total = balances.hot + balances.cold;
        if total == 0 {
            return 0;
        }
        let bps = config
            .map(|c| c.hot_bps as i128)
            .unwrap_or(BPS_DENOMINATOR);
        total
            .checked_mul(bps)
            .unwrap_or(0)
            .checked_div(BPS_DENOMINATOR)
            .unwrap_or(0)
    }
}

impl ColdConfig {
    pub fn is_cold_signer(&self, addr: &Address) -> bool {
        self.cold_signers.iter().any(|s| s == *addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn addr(env: &Env, seed: u8) -> Address {
        let _ = seed;
        Address::generate(env)
    }

    fn setup(env: &Env) -> (ColdStorageClient, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let contract_id = env.register(ColdStorage, ());
        let client = ColdStorageClient::new(env, &contract_id);
        client.init(&admin).unwrap();
        (client, admin)
    }

    fn make_config(env: &Env, hot_bps: u32) -> ColdConfig {
        let mut signers: Vec<Address> = Vec::new(env);
        signers.push_back(addr(env, 1));
        signers.push_back(addr(env, 2));
        signers.push_back(addr(env, 3));
        ColdConfig {
            hot_bps,
            rebalance_threshold_bps: 500,
            cold_signers: signers,
            cold_threshold: 2,
        }
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(ColdStorage, ());
        let client = ColdStorageClient::new(&env, &contract_id);
        env.mock_all_auths();
        assert!(client.init(&admin).is_ok());
        assert_eq!(client.get_admin().unwrap(), admin);
    }

    #[test]
    fn test_init_rejects_double() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(ColdStorage, ());
        let client = ColdStorageClient::new(&env, &contract_id);
        env.mock_all_auths();
        assert!(client.init(&admin).is_ok());
        assert_eq!(
            client.try_init(&admin).unwrap_err(),
            ColdError::AlreadyInitialized
        );
    }

    #[test]
    fn test_set_config() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = make_config(&env, 2000);
        assert!(client.set_config(&admin, &config).is_ok());
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    fn test_set_config_invalid_hot_bps() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = make_config(&env, 0);
        assert_eq!(
            client.try_set_config(&admin, &config).unwrap_err(),
            ColdError::InvalidHotBps
        );
    }

    #[test]
    fn test_set_balances() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let balances = ColdBalances { hot: 1000, cold: 4000 };
        assert!(client.set_balances(&admin, &balances).is_ok());
        assert_eq!(client.get_balances(), Some(balances));
    }

    #[test]
    fn test_propose_and_approve_cold_sweep() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = make_config(&env, 2000);
        client.set_config(&admin, &config).unwrap();

        let signer1 = config.cold_signers.get(0);
        let signer2 = config.cold_signers.get(1);
        let dest = addr(&env, 99);

        // Proposer needs the signer to be the caller — since mock_all_auths
        // is on, any address passes require_auth, but the signer check still
        // requires a signer address.
        client.propose_cold_sweep(&signer1, &500, &dest).unwrap();
        let sweep = client.get_cold_sweep().unwrap();
        assert_eq!(sweep.amount, 500);
        assert_eq!(sweep.approvals.len(), 1);

        // Second signer approves — crosses threshold (2/3) and executes.
        client.approve_cold_sweep(&signer2).unwrap();
        // Sweep should be cleared (executed).
        assert!(client.get_cold_sweep().is_none());
    }

    #[test]
    fn test_hot_share_bps() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        // No balances set → 0.
        assert_eq!(client.hot_share_bps(), 0);
        let balances = ColdBalances { hot: 2000, cold: 8000 };
        client.set_balances(&admin, &balances).unwrap();
        assert_eq!(client.hot_share_bps(), 2000);
    }

    #[test]
    fn test_target_hot_amount() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = make_config(&env, 2000);
        client.set_config(&admin, &config).unwrap();
        let balances = ColdBalances { hot: 5000, cold: 5000 };
        client.set_balances(&admin, &balances).unwrap();
        assert_eq!(client.target_hot_amount(), 2000);
    }
}
