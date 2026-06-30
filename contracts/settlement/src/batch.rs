use soroban_sdk::{contracttype, Address, Env, Vec};
use crate::{CalloraSettlement, SettlementError, StorageKey};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SettleInput {
    pub developer: Address,
    pub amount: i128,
    pub to: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SettleOutcome {
    Success,
    AmountNotPositive,
    ClaimWindowClosed,
    InsufficientBalance,
    DailyWithdrawCapExceeded,
    DeveloperBalanceUnderflow,
    OtherError,
}

impl From<SettlementError> for SettleOutcome {
    fn from(err: SettlementError) -> Self {
        match err {
            SettlementError::AmountNotPositive => SettleOutcome::AmountNotPositive,
            SettlementError::ClaimWindowClosed => SettleOutcome::ClaimWindowClosed,
            SettlementError::InsufficientDeveloperBalance => SettleOutcome::InsufficientBalance,
            SettlementError::DailyWithdrawCapExceeded => SettleOutcome::DailyWithdrawCapExceeded,
            SettlementError::DeveloperBalanceUnderflow => SettleOutcome::DeveloperBalanceUnderflow,
            _ => SettleOutcome::OtherError,
        }
    }
}

pub fn batch_settle(
    env: &Env,
    settlements: Vec<SettleInput>,
) -> Vec<SettleOutcome> {
    let mut outcomes = Vec::new(env);
    
    if settlements.len() > 64 {
        for _ in 0..settlements.len() {
            outcomes.push_back(SettleOutcome::OtherError);
        }
        return outcomes;
    }

    for input in settlements.iter() {
        // We use CalloraSettlement::withdraw_developer_balance internally
        // but we must catch the error to allow partial success.
        let res = CalloraSettlement::withdraw_developer_balance(
            env.clone(),
            input.developer.clone(),
            input.amount,
            input.to.clone(),
        );
        match res {
            Ok(_) => outcomes.push_back(SettleOutcome::Success),
            Err(e) => outcomes.push_back(e.into()),
        }
    }
    
    outcomes
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};
    
    #[test]
    fn test_batch_settle_cap_enforced() {
        let env = Env::default();
        let mut settlements = Vec::new(&env);
        
        // Push 65 items (exceeding cap of 64)
        for _ in 0..65 {
            settlements.push_back(SettleInput {
                developer: Address::generate(&env),
                amount: 100,
                to: None,
            });
        }
        
        let outcomes = batch_settle(&env, settlements);
        
        assert_eq!(outcomes.len(), 65);
        for i in 0..65 {
            assert_eq!(outcomes.get(i).unwrap(), SettleOutcome::OtherError);
        }
    }
}
