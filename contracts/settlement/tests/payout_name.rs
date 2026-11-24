//! Cross-contract tests for registry-backed developer payout names (#1520).
//!
//! A developer can configure a payout name that is resolved through the
//! `callora-recipient` registry at withdrawal time. These tests deploy both
//! contracts in one `Env` so the resolution path, registry rotation, and
//! failure modes are exercised end to end.

#![cfg(test)]

extern crate std;

use callora_recipient::{CalloraRecipient, CalloraRecipientClient};
use callora_settlement::{CalloraSettlement, CalloraSettlementClient, SettlementError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, Env, String, Symbol, Vec};

struct Fixture {
    env: Env,
    settlement_id: Address,
    registry_id: Address,
    admin: Address,
    registry_admin: Address,
    developer: Address,
    usdc: Address,
    name: String,
    payout: Address,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let payout = Address::generate(&env);
        let registry_admin = Address::generate(&env);

        let settlement_id = env.register(CalloraSettlement, ());
        let settlement = CalloraSettlementClient::new(&env, &settlement_id);
        settlement.init(&admin, &vault);

        let usdc = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        settlement.set_usdc_token(&admin, &usdc);

        let registry_id = env.register(CalloraRecipient, ());
        let registry = CalloraRecipientClient::new(&env, &registry_id);
        registry.init(&registry_admin);

        let name = String::from_str(&env, "alice-payout");
        registry.register_recipient(&registry_admin, &name, &payout);

        settlement.set_recipient_registry(&admin, &registry_id);

        Self {
            env,
            settlement_id,
            registry_id,
            admin,
            registry_admin,
            developer,
            usdc,
            name,
            payout,
        }
    }

    fn settlement(&self) -> CalloraSettlementClient<'_> {
        CalloraSettlementClient::new(&self.env, &self.settlement_id)
    }

    fn registry(&self) -> CalloraRecipientClient<'_> {
        CalloraRecipientClient::new(&self.env, &self.registry_id)
    }

    fn fund(&self, amount: i128) {
        let settlement = self.settlement();
        settlement.force_credit_developer(
            &self.admin,
            &self.developer,
            &amount,
            &self.usdc,
            &Symbol::new(&self.env, "credit"),
        );
        token::StellarAssetClient::new(&self.env, &self.usdc)
            .mint(&self.settlement_id, &amount);
    }

    fn balance_of(&self, address: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(address)
    }
}

#[test]
fn payout_name_pays_current_registry_address() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);

    let sim = f
        .settlement()
        .try_simulate_claim(&f.developer, &200i128, &None)
        .unwrap()
        .unwrap();
    assert_eq!(sim.recipient, f.payout);
    assert_eq!(sim.remaining_balance, 300);

    f.settlement()
        .withdraw_developer_balance(&f.developer, &200i128, &None);

    assert_eq!(f.balance_of(&f.payout), 200);
    assert_eq!(f.balance_of(&f.developer), 0);
    assert_eq!(
        f.settlement()
            .get_developer_balance(&f.developer, &f.usdc),
        300
    );
}

#[test]
fn explicit_recipient_overrides_payout_name() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);

    let explicit = Address::generate(&f.env);
    f.settlement()
        .withdraw_developer_balance(&f.developer, &200i128, &Some(explicit.clone()));

    assert_eq!(f.balance_of(&explicit), 200);
    assert_eq!(f.balance_of(&f.payout), 0);
}

#[test]
fn payout_name_follows_registry_rotation() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);

    f.settlement()
        .withdraw_developer_balance(&f.developer, &100i128, &None);
    assert_eq!(f.balance_of(&f.payout), 100);

    let rotated = Address::generate(&f.env);
    f.registry()
        .update_recipient(&f.registry_admin, &f.name, &rotated);

    f.settlement()
        .withdraw_developer_balance(&f.developer, &150i128, &None);
    assert_eq!(f.balance_of(&rotated), 150);
    assert_eq!(f.balance_of(&f.payout), 100);
}

#[test]
fn missing_registry_name_fails_without_mutating_state() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);
    f.registry().remove_recipient(&f.registry_admin, &f.name);

    assert_eq!(
        f.settlement()
            .try_withdraw_developer_balance(&f.developer, &100i128, &None),
        Err(Ok(SettlementError::PayoutNameNotFound))
    );
    assert_eq!(
        f.settlement()
            .get_developer_balance(&f.developer, &f.usdc),
        500
    );
    assert_eq!(f.settlement().get_withdrawal_today(&f.developer), 0);
}

#[test]
fn payout_name_without_registry_fails_without_mutating_state() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);

    // A fresh settlement instance has no registry configured.
    let other_settlement = f.env.register(CalloraSettlement, ());
    let other = CalloraSettlementClient::new(&f.env, &other_settlement);
    let vault = Address::generate(&f.env);
    other.init(&f.admin, &vault);
    other.set_usdc_token(&f.admin, &f.usdc);
    other.force_credit_developer(
        &f.admin,
        &f.developer,
        &500i128,
        &f.usdc,
        &Symbol::new(&f.env, "credit"),
    );
    other.set_payout_name(&f.developer, &f.name);

    assert_eq!(
        other.try_withdraw_developer_balance(&f.developer, &100i128, &None),
        Err(Ok(SettlementError::RecipientRegistryNotSet))
    );
    assert_eq!(other.get_developer_balance(&f.developer, &f.usdc), 500);
    assert_eq!(other.get_withdrawal_today(&f.developer), 0);
}

#[test]
fn payout_address_cannot_be_settlement_contract() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);
    f.registry()
        .update_recipient(&f.registry_admin, &f.name, &f.settlement_id);

    assert_eq!(
        f.settlement()
            .try_withdraw_developer_balance(&f.developer, &100i128, &None),
        Err(Ok(SettlementError::InvalidRecipient))
    );
    assert_eq!(
        f.settlement()
            .get_developer_balance(&f.developer, &f.usdc),
        500
    );
}

#[test]
fn clearing_payout_name_restores_direct_payment() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);
    f.settlement().clear_payout_name(&f.developer);

    assert_eq!(f.settlement().get_payout_name(&f.developer), None);
    f.settlement()
        .withdraw_developer_balance(&f.developer, &100i128, &None);
    assert_eq!(f.balance_of(&f.developer), 100);
    assert_eq!(f.balance_of(&f.payout), 0);
}

#[test]
fn batch_withdraw_uses_payout_name() {
    let f = Fixture::new();
    f.fund(500);
    f.settlement().set_payout_name(&f.developer, &f.name);

    let (next, done) = f
        .settlement()
        .batch_withdraw_balance_cursor(
            &Vec::from_array(&f.env, [f.developer.clone()]),
            &Vec::from_array(&f.env, [250i128]),
            &0u32,
            &1u32,
        );
    assert_eq!((next, done), (1, true));
    assert_eq!(f.balance_of(&f.payout), 250);
}

#[test]
fn empty_or_oversized_payout_name_is_rejected() {
    let f = Fixture::new();
    let empty = String::from_str(&f.env, "");
    assert_eq!(
        f.settlement().try_set_payout_name(&f.developer, &empty),
        Err(Ok(SettlementError::InvalidRecipient))
    );

    let too_long = String::from_str(&f.env, &"x".repeat(65));
    assert_eq!(
        f.settlement().try_set_payout_name(&f.developer, &too_long),
        Err(Ok(SettlementError::InvalidRecipient))
    );
    assert_eq!(f.settlement().get_payout_name(&f.developer), None);
}

#[test]
fn payout_name_set_and_cleared_events_are_emitted() {
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::{IntoVal, Symbol};

    let f = Fixture::new();
    f.settlement().set_payout_name(&f.developer, &f.name);

    let events = f.env.events().all();
    let event = events.last().unwrap();
    let topic: Symbol = event.1.get(0).unwrap().into_val(&f.env);
    assert_eq!(topic, Symbol::new(&f.env, "payout_name_set"));
    let payload: String = event.2.into_val(&f.env);
    assert_eq!(payload, f.name);

    f.settlement().clear_payout_name(&f.developer);
    let events = f.env.events().all();
    let event = events.last().unwrap();
    let topic: Symbol = event.1.get(0).unwrap().into_val(&f.env);
    assert_eq!(topic, Symbol::new(&f.env, "payout_name_cleared"));
}
