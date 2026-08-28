use crate::{CalloraSettlement, SettlementError};
use soroban_sdk::{contracttype, Address, Env, Vec};

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

pub fn batch_settle(env: &Env, settlements: Vec<SettleInput>) -> Vec<SettleOutcome> {
    if settlements.len() > 64 {
        panic!("batch exceeds max batch size");
    }

    let mut seen_developers = soroban_sdk::Map::<Address, ()>::new(env);
    for input in settlements.iter() {
        if input.amount <= 0 {
            panic!("amount must be positive");
        }
        if seen_developers.contains_key(input.developer.clone()) {
            panic!("duplicate developer in batch");
        }
        seen_developers.set(input.developer.clone(), ());
    }

    let mut outcomes = Vec::new(env);

    for input in settlements.iter() {
        let res = CalloraSettlement::withdraw_developer_balance(
            env.clone(),
            input.developer.clone(),
            input.amount,
            input.to.clone(),
        );
        match res {
            Ok(_) => outcomes.push_back(SettleOutcome::Success),
            Err(e) => panic!("batch settle failed: {:?}", e),
        }
    }

    outcomes
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    #[test]
    #[should_panic(expected = "batch exceeds max batch size")]
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

        batch_settle(&env, settlements);
    }

    #[test]
    #[should_panic(expected = "duplicate developer in batch")]
    fn test_batch_settle_rejects_duplicates() {
        let env = Env::default();
        let mut settlements = Vec::new(&env);
        let dev = Address::generate(&env);

        settlements.push_back(SettleInput {
            developer: dev.clone(),
            amount: 100,
            to: None,
        });
        settlements.push_back(SettleInput {
            developer: dev.clone(),
            amount: 200,
            to: None,
        });

        batch_settle(&env, settlements);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_batch_settle_rejects_zero_amount() {
        let env = Env::default();
        let mut settlements = Vec::new(&env);
        let dev = Address::generate(&env);

        settlements.push_back(SettleInput {
            developer: dev.clone(),
            amount: 0,
            to: None,
        });

        batch_settle(&env, settlements);
    }

    #[test]
    fn test_batch_settle_empty() {
        let env = Env::default();
        let settlements = Vec::new(&env);
        let outcomes = batch_settle(&env, settlements);
        assert_eq!(outcomes.len(), 0);
    }
}
