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
