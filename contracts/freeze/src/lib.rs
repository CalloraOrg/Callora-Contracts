#![no_std]

#[cfg(test)]
extern crate std;

pub mod errors;

use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct CalloraFreeze;

#[contractimpl]
impl CalloraFreeze {
    pub fn init(_env: Env) -> Result<(), errors::ContractError> {
        Err(errors::ContractError::NotInitialized)
    }
}
