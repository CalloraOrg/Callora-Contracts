//! GrantFox FWC26 — Per-Entrypoint Access-Control Matrix (Stellar Wave)
//!
//! Asserts every auth-guarded entrypoint across Vault, Settlement, and
//! Revenue Pool enforces `require_auth`:
//!
//! * **authorized** — `mock_all_auths()` active; expected role succeeds.
//! * **unauthorized** — `set_auths(&[])` strips all sigs; outsider / wrong
//!   role gets `Err`, proving `require_auth` fired before any state change.
//!
//! # Entrypoint coverage (tasks 4 & 5)
//!
//! ## Settlement (task 4)
//! `receive_payment`, `batch_receive_payment`,
//! `set_admin`, `accept_admin`, `cancel_admin_transfer`,
//! `propose_vault`, `accept_vault`,
//! `set_usdc_token`, `set_developer_claim_window`,
//! `clear_developer_claim_window`, `get_all_developer_balances`,
//! `force_credit_developer`, `set_daily_withdraw_cap`,
//! `set_developer_min_balance`, `withdraw_developer_balance`,
//! `propose_balance_migration`, `broadcast`, `upgrade`
//!
//! ## Revenue Pool (task 5)
//! `distribute`, `batch_distribute`,
//! `set_admin`, `accept_admin`, `claim_admin`, `cancel_admin_transfer`,
//! `set_pause_guardian`, `clear_pause_guardian`,
//! `pause` (admin path + guardian path), `unpause`,
//! `receive_payment`, `deposit_yield`, `set_max_distribute`,
//! `broadcast`, `upgrade`,
//! `propose_emergency_drain`, `cancel_emergency_drain`,
//! `execute_emergency_drain` (after 24 h timelock)

#![no_std]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};
use soroban_sdk::token as soroban_token;

use callora_vault::CalloraVaultClient;
use callora_settlement::CalloraSettlementClient;
use callora_revenue_pool::RevenuePoolClient;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, soroban_token::Client<'a>, soroban_token::StellarAssetClient<'a>) {
    let sa = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = sa.address();
    (
        addr.clone(),
        soroban_token::Client::new(env, &addr),
        soroban_token::StellarAssetClient::new(env, &addr),
    )
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let addr = env.register(callora_vault::CalloraVault, ());
    (addr.clone(), CalloraVaultClient::new(env, &addr))
}

fn create_settlement(env: &Env) -> (Address, CalloraSettlementClient<'_>) {
    let addr = env.register(callora_settlement::CalloraSettlement, ());
    (addr.clone(), CalloraSettlementClient::new(env, &addr))
}

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let addr = env.register(callora_revenue_pool::RevenuePool, ());
    (addr.clone(), RevenuePoolClient::new(env, &addr))
}

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    env:               Env,
    vault_addr:        Address,
    vault:             CalloraVaultClient<'a>,
    settlement_addr:   Address,
    settlement:        CalloraSettlementClient<'a>,
    pool_addr:         Address,
    pool:              RevenuePoolClient<'a>,
    usdc_addr:         Address,
    usdc:              soroban_token::Client<'a>,
    usdc_admin:        soroban_token::StellarAssetClient<'a>,
    owner:             Address,
    admin:             Address,
    authorized_caller: Address,
    developer:         Address,
    pending_admin:     Address,
    outsider:          Address,
}

fn setup() -> Ctx<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let owner             = Address::generate(&env);
    let admin             = Address::generate(&env);
    let authorized_caller = Address::generate(&env);
    let developer         = Address::generate(&env);
    let pending_admin     = Address::generate(&env);
    let outsider          = Address::generate(&env);

    let (vault_addr,      vault)      = create_vault(&env);
    let (settlement_addr, settlement) = create_settlement(&env);
    let (pool_addr,       pool)       = create_pool(&env);
    let (usdc_addr,       usdc,       usdc_admin) = create_usdc(&env, &admin);

    usdc_admin.mint(&vault_addr,      &1_000_000_000);
    usdc_admin.mint(&pool_addr,       &1_000_000_000);
    usdc_admin.mint(&settlement_addr, &1_000_000_000);

    vault.init(
        &owner,
        &usdc_addr,
        &1_000_000,
        &authorized_caller,
        &1,
        &Some(pool_addr.clone()),
        &100_000_000,
        &settlement_addr,
    );

    settlement.init(&admin, &vault_addr);
    pool.init(&admin, &usdc_addr);

    Ctx {
        env,
        vault_addr,
        vault,
        settlement_addr,
        settlement,
        pool_addr,
        pool,
        usdc_addr,
        usdc,
        usdc_admin,
        owner,
        admin,
        authorized_caller,
        developer,
        pending_admin,
        outsider,
    }
}

// ===========================================================================
// SETTLEMENT — per-entrypoint access-control matrix (task 4)
// ===========================================================================

mod settlement {
    use super::*;

    // -----------------------------------------------------------------------
    // receive_payment — vault or admin
    // -----------------------------------------------------------------------

    #[test]
    fn receive_payment_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_receive_payment(
            &ctx.admin, &100, &true, &None, &ctx.usdc_addr, &1,
        ).is_ok());
    }

    #[test]
    fn receive_payment_via_vault_deduct_succeeds() {
        // vault.deduct triggers on-ledger transfer; settlement.receive_payment
        // is indirectly authorized because the vault is the registered vault.
        let ctx = setup();
        assert!(ctx.vault.try_deduct(&ctx.authorized_caller, &100, &42).is_ok());
    }

    #[test]
    fn receive_payment_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_receive_payment(
            &ctx.outsider, &100, &true, &None, &ctx.usdc_addr, &2,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // batch_receive_payment — vault or admin
    // -----------------------------------------------------------------------

    #[test]
    fn batch_receive_payment_admin_succeeds() {
        let ctx = setup();
        let items = Vec::from_array(
            &ctx.env,
            [(ctx.developer.clone(), 50i128)],
        );
        assert!(ctx.settlement.try_batch_receive_payment(
            &ctx.admin, &items, &ctx.usdc_addr, &1,
        ).is_ok());
    }

    #[test]
    fn batch_receive_payment_outsider_rejected_without_auth() {
        let ctx = setup();
        let items = Vec::from_array(
            &ctx.env,
            [(ctx.developer.clone(), 50i128)],
        );
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_batch_receive_payment(
            &ctx.outsider, &items, &ctx.usdc_addr, &2,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // set_admin — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_admin_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_set_admin(&ctx.admin, &ctx.pending_admin).is_ok());
    }

    #[test]
    fn set_admin_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_set_admin(&ctx.outsider, &ctx.pending_admin).is_err());
    }

    // -----------------------------------------------------------------------
    // accept_admin — pending admin only (no explicit caller arg)
    // -----------------------------------------------------------------------

    #[test]
    fn accept_admin_pending_admin_succeeds() {
        let ctx = setup();
        ctx.settlement.set_admin(&ctx.admin, &ctx.pending_admin);
        assert!(ctx.settlement.try_accept_admin().is_ok());
        assert_eq!(ctx.settlement.get_admin(), ctx.pending_admin);
    }

    // -----------------------------------------------------------------------
    // cancel_admin_transfer — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn cancel_admin_transfer_admin_succeeds() {
        let ctx = setup();
        ctx.settlement.set_admin(&ctx.admin, &ctx.pending_admin);
        assert!(ctx.settlement.try_cancel_admin_transfer(&ctx.admin).is_ok());
    }

    #[test]
    fn cancel_admin_transfer_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.settlement.set_admin(&ctx.admin, &ctx.pending_admin);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_cancel_admin_transfer(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // propose_vault — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn propose_vault_admin_succeeds() {
        let ctx = setup();
        let new_vault = Address::generate(&ctx.env);
        assert!(ctx.settlement.try_propose_vault(&ctx.admin, &new_vault).is_ok());
    }

    #[test]
    fn propose_vault_outsider_rejected_without_auth() {
        let ctx = setup();
        let new_vault = Address::generate(&ctx.env);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_propose_vault(&ctx.outsider, &new_vault).is_err());
    }

    // -----------------------------------------------------------------------
    // accept_vault — pending vault or admin
    // -----------------------------------------------------------------------

    #[test]
    fn accept_vault_pending_vault_succeeds() {
        let ctx = setup();
        let new_vault = Address::generate(&ctx.env);
        ctx.settlement.propose_vault(&ctx.admin, &new_vault);
        assert!(ctx.settlement.try_accept_vault(&new_vault).is_ok());
    }

    #[test]
    fn accept_vault_admin_succeeds() {
        let ctx = setup();
        let new_vault = Address::generate(&ctx.env);
        ctx.settlement.propose_vault(&ctx.admin, &new_vault);
        assert!(ctx.settlement.try_accept_vault(&ctx.admin).is_ok());
    }

    #[test]
    fn accept_vault_outsider_rejected_without_auth() {
        let ctx = setup();
        let new_vault = Address::generate(&ctx.env);
        ctx.settlement.propose_vault(&ctx.admin, &new_vault);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_accept_vault(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // set_usdc_token — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_usdc_token_admin_succeeds() {
        let ctx = setup();
        let new_usdc = Address::generate(&ctx.env);
        assert!(ctx.settlement.try_set_usdc_token(&ctx.admin, &new_usdc).is_ok());
    }

    #[test]
    fn set_usdc_token_outsider_rejected_without_auth() {
        let ctx = setup();
        let new_usdc = Address::generate(&ctx.env);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_set_usdc_token(&ctx.outsider, &new_usdc).is_err());
    }

    // -----------------------------------------------------------------------
    // set_developer_claim_window — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_developer_claim_window_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_set_developer_claim_window(
            &ctx.admin, &ctx.developer, &100, &200,
        ).is_ok());
    }

    #[test]
    fn set_developer_claim_window_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_set_developer_claim_window(
            &ctx.outsider, &ctx.developer, &100, &200,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // clear_developer_claim_window — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn clear_developer_claim_window_admin_succeeds() {
        let ctx = setup();
        ctx.settlement.set_developer_claim_window(&ctx.admin, &ctx.developer, &100, &200);
        assert!(ctx.settlement.try_clear_developer_claim_window(
            &ctx.admin, &ctx.developer,
        ).is_ok());
    }

    #[test]
    fn clear_developer_claim_window_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_clear_developer_claim_window(
            &ctx.outsider, &ctx.developer,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // get_all_developer_balances — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn get_all_developer_balances_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_get_all_developer_balances(
            &ctx.admin, &ctx.usdc_addr,
        ).is_ok());
    }

    #[test]
    fn get_all_developer_balances_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_get_all_developer_balances(
            &ctx.outsider, &ctx.usdc_addr,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // force_credit_developer — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn force_credit_developer_admin_succeeds() {
        let ctx = setup();
        let reason = Symbol::new(&ctx.env, "test");
        assert!(ctx.settlement.try_force_credit_developer(
            &ctx.admin, &ctx.developer, &200, &ctx.usdc_addr, &reason,
        ).is_ok());
    }

    #[test]
    fn force_credit_developer_outsider_rejected_without_auth() {
        let ctx = setup();
        let reason = Symbol::new(&ctx.env, "test");
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_force_credit_developer(
            &ctx.outsider, &ctx.developer, &200, &ctx.usdc_addr, &reason,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // set_daily_withdraw_cap — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_daily_withdraw_cap_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_set_daily_withdraw_cap(
            &ctx.admin, &ctx.developer, &10_000,
        ).is_ok());
    }

    #[test]
    fn set_daily_withdraw_cap_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_set_daily_withdraw_cap(
            &ctx.outsider, &ctx.developer, &10_000,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // set_developer_min_balance — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_developer_min_balance_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.settlement.try_set_developer_min_balance(
            &ctx.admin, &ctx.developer, &100,
        ).is_ok());
    }

    #[test]
    fn set_developer_min_balance_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_set_developer_min_balance(
            &ctx.outsider, &ctx.developer, &100,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // withdraw_developer_balance — developer (own balance) only
    // -----------------------------------------------------------------------

    #[test]
    fn withdraw_developer_balance_developer_succeeds() {
        let ctx = setup();
        let reason = Symbol::new(&ctx.env, "pay");
        ctx.settlement.force_credit_developer(
            &ctx.admin, &ctx.developer, &500, &ctx.usdc_addr, &reason,
        );
        ctx.settlement.set_usdc_token(&ctx.admin, &ctx.usdc_addr);
        assert!(ctx.settlement.try_withdraw_developer_balance(
            &ctx.developer, &100, &None,
        ).is_ok());
    }

    #[test]
    fn withdraw_developer_balance_outsider_rejected_without_auth() {
        let ctx = setup();
        let reason = Symbol::new(&ctx.env, "pay");
        ctx.settlement.force_credit_developer(
            &ctx.admin, &ctx.developer, &500, &ctx.usdc_addr, &reason,
        );
        ctx.settlement.set_usdc_token(&ctx.admin, &ctx.usdc_addr);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_withdraw_developer_balance(
            &ctx.outsider, &100, &None,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // propose_balance_migration — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn propose_balance_migration_admin_succeeds() {
        let ctx = setup();
        let new_dev = Address::generate(&ctx.env);
        assert!(ctx.settlement.try_propose_balance_migration(
            &ctx.admin, &ctx.developer, &new_dev,
        ).is_ok());
    }

    #[test]
    fn propose_balance_migration_outsider_rejected_without_auth() {
        let ctx = setup();
        let new_dev = Address::generate(&ctx.env);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_propose_balance_migration(
            &ctx.outsider, &ctx.developer, &new_dev,
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // broadcast — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn broadcast_admin_succeeds() {
        let ctx = setup();
        let severity = callora_settlement::Severity::Info;
        let msg = soroban_sdk::String::from_str(&ctx.env, "hello");
        assert!(ctx.settlement.try_broadcast(&ctx.admin, &severity, &msg).is_ok());
    }

    #[test]
    fn broadcast_outsider_rejected_without_auth() {
        let ctx = setup();
        let severity = callora_settlement::Severity::Warn;
        let msg = soroban_sdk::String::from_str(&ctx.env, "bad");
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_broadcast(&ctx.outsider, &severity, &msg).is_err());
    }

    // -----------------------------------------------------------------------
    // upgrade — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn upgrade_admin_succeeds() {
        let ctx = setup();
        let hash = BytesN::from_array(&ctx.env, &[0u8; 32]);
        assert!(ctx.settlement.try_upgrade(&ctx.admin, &hash).is_ok());
    }

    #[test]
    fn upgrade_outsider_rejected_without_auth() {
        let ctx = setup();
        let hash = BytesN::from_array(&ctx.env, &[0u8; 32]);
        ctx.env.set_auths(&[]);
        assert!(ctx.settlement.try_upgrade(&ctx.outsider, &hash).is_err());
    }
}

// ===========================================================================
// REVENUE POOL — per-entrypoint access-control matrix (task 5)
// ===========================================================================

mod revenue_pool {
    use super::*;

    // -----------------------------------------------------------------------
    // distribute — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.pool.try_distribute(&ctx.admin, &ctx.developer, &100).is_ok());
    }

    #[test]
    fn distribute_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_distribute(&ctx.outsider, &ctx.developer, &100).is_err());
    }

    // -----------------------------------------------------------------------
    // batch_distribute — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn batch_distribute_admin_succeeds() {
        let ctx = setup();
        let payments = Vec::from_array(&ctx.env, [(ctx.developer.clone(), 100i128)]);
        assert!(ctx.pool.try_batch_distribute(&ctx.admin, &payments).is_ok());
    }

    #[test]
    fn batch_distribute_outsider_rejected_without_auth() {
        let ctx = setup();
        let payments = Vec::from_array(&ctx.env, [(ctx.developer.clone(), 100i128)]);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_batch_distribute(&ctx.outsider, &payments).is_err());
    }

    // -----------------------------------------------------------------------
    // set_admin — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_admin_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.pool.try_set_admin(&ctx.admin, &ctx.pending_admin).is_ok());
    }

    #[test]
    fn set_admin_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_set_admin(&ctx.outsider, &ctx.pending_admin).is_err());
    }

    // -----------------------------------------------------------------------
    // accept_admin — pending admin only
    // -----------------------------------------------------------------------

    #[test]
    fn accept_admin_pending_admin_succeeds() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        assert!(ctx.pool.try_accept_admin(&ctx.pending_admin).is_ok());
        assert_eq!(ctx.pool.get_admin(), ctx.pending_admin);
    }

    #[test]
    fn accept_admin_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_accept_admin(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // claim_admin — pending admin only (alias)
    // -----------------------------------------------------------------------

    #[test]
    fn claim_admin_pending_admin_succeeds() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        assert!(ctx.pool.try_claim_admin(&ctx.pending_admin).is_ok());
        assert_eq!(ctx.pool.get_admin(), ctx.pending_admin);
    }

    #[test]
    fn claim_admin_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_claim_admin(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // cancel_admin_transfer — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn cancel_admin_transfer_admin_succeeds() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        assert!(ctx.pool.try_cancel_admin_transfer(&ctx.admin).is_ok());
    }

    #[test]
    fn cancel_admin_transfer_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.pool.set_admin(&ctx.admin, &ctx.pending_admin);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_cancel_admin_transfer(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // set_pause_guardian — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_pause_guardian_admin_succeeds() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        assert!(ctx.pool.try_set_pause_guardian(&ctx.admin, &guardian).is_ok());
    }

    #[test]
    fn set_pause_guardian_outsider_rejected_without_auth() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_set_pause_guardian(&ctx.outsider, &guardian).is_err());
    }

    // -----------------------------------------------------------------------
    // clear_pause_guardian — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn clear_pause_guardian_admin_succeeds() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        ctx.pool.set_pause_guardian(&ctx.admin, &guardian);
        assert!(ctx.pool.try_clear_pause_guardian(&ctx.admin).is_ok());
    }

    #[test]
    fn clear_pause_guardian_outsider_rejected_without_auth() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        ctx.pool.set_pause_guardian(&ctx.admin, &guardian);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_clear_pause_guardian(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // pause — admin or pause guardian
    // -----------------------------------------------------------------------

    #[test]
    fn pause_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.pool.try_pause(&ctx.admin).is_ok());
    }

    #[test]
    fn pause_guardian_succeeds() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        ctx.pool.set_pause_guardian(&ctx.admin, &guardian);
        assert!(ctx.pool.try_pause(&guardian).is_ok());
    }

    #[test]
    fn pause_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_pause(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // unpause — admin only (guardian cannot unpause)
    // -----------------------------------------------------------------------

    #[test]
    fn unpause_admin_succeeds() {
        let ctx = setup();
        ctx.pool.pause(&ctx.admin);
        assert!(ctx.pool.try_unpause(&ctx.admin).is_ok());
    }

    #[test]
    fn unpause_guardian_rejected() {
        let ctx = setup();
        let guardian = Address::generate(&ctx.env);
        ctx.pool.set_pause_guardian(&ctx.admin, &guardian);
        ctx.pool.pause(&guardian);
        // Guardian must not be able to unpause — business-logic rejection.
        assert!(ctx.pool.try_unpause(&guardian).is_err());
    }

    #[test]
    fn unpause_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.pool.pause(&ctx.admin);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_unpause(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // receive_payment — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn receive_payment_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.pool.try_receive_payment(&ctx.admin, &100, &true).is_ok());
    }

    #[test]
    fn receive_payment_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_receive_payment(&ctx.outsider, &100, &true).is_err());
    }

    // -----------------------------------------------------------------------
    // deposit_yield — admin (treasury) only
    // -----------------------------------------------------------------------

    #[test]
    fn deposit_yield_admin_succeeds() {
        let ctx = setup();
        ctx.usdc_admin.mint(&ctx.admin, &1000);
        ctx.usdc.approve(&ctx.admin, &ctx.pool_addr, &1000, &999_999);
        let source = Symbol::new(&ctx.env, "yield");
        assert!(ctx.pool.try_deposit_yield(&ctx.admin, &200, &source).is_ok());
    }

    #[test]
    fn deposit_yield_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.usdc_admin.mint(&ctx.outsider, &1000);
        ctx.usdc.approve(&ctx.outsider, &ctx.pool_addr, &1000, &999_999);
        let source = Symbol::new(&ctx.env, "yield");
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_deposit_yield(&ctx.outsider, &200, &source).is_err());
    }

    // -----------------------------------------------------------------------
    // set_max_distribute — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn set_max_distribute_admin_succeeds() {
        let ctx = setup();
        assert!(ctx.pool.try_set_max_distribute(&ctx.admin, &50_000).is_ok());
    }

    #[test]
    fn set_max_distribute_outsider_rejected_without_auth() {
        let ctx = setup();
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_set_max_distribute(&ctx.outsider, &50_000).is_err());
    }

    // -----------------------------------------------------------------------
    // broadcast — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn broadcast_admin_succeeds() {
        let ctx = setup();
        let severity = callora_revenue_pool::Severity::Info;
        let msg = soroban_sdk::String::from_str(&ctx.env, "all good");
        assert!(ctx.pool.try_broadcast(&ctx.admin, &severity, &msg).is_ok());
    }

    #[test]
    fn broadcast_outsider_rejected_without_auth() {
        let ctx = setup();
        let severity = callora_revenue_pool::Severity::Crit;
        let msg = soroban_sdk::String::from_str(&ctx.env, "bad");
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_broadcast(&ctx.outsider, &severity, &msg).is_err());
    }

    // -----------------------------------------------------------------------
    // upgrade — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn upgrade_admin_succeeds() {
        let ctx = setup();
        let hash = BytesN::from_array(&ctx.env, &[0u8; 32]);
        assert!(ctx.pool.try_upgrade(&ctx.admin, &hash).is_ok());
    }

    #[test]
    fn upgrade_outsider_rejected_without_auth() {
        let ctx = setup();
        let hash = BytesN::from_array(&ctx.env, &[0u8; 32]);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_upgrade(&ctx.outsider, &hash).is_err());
    }

    // -----------------------------------------------------------------------
    // propose_emergency_drain — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn propose_emergency_drain_admin_succeeds() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        assert!(ctx.pool.try_propose_emergency_drain(&ctx.admin, &treasury, &500).is_ok());
    }

    #[test]
    fn propose_emergency_drain_outsider_rejected_without_auth() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_propose_emergency_drain(&ctx.outsider, &treasury, &500).is_err());
    }

    // -----------------------------------------------------------------------
    // cancel_emergency_drain — admin only
    // -----------------------------------------------------------------------

    #[test]
    fn cancel_emergency_drain_admin_succeeds() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        ctx.pool.propose_emergency_drain(&ctx.admin, &treasury, &500);
        assert!(ctx.pool.try_cancel_emergency_drain(&ctx.admin).is_ok());
    }

    #[test]
    fn cancel_emergency_drain_outsider_rejected_without_auth() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        ctx.pool.propose_emergency_drain(&ctx.admin, &treasury, &500);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_cancel_emergency_drain(&ctx.outsider).is_err());
    }

    // -----------------------------------------------------------------------
    // execute_emergency_drain — admin only, after 24 h timelock
    // -----------------------------------------------------------------------

    #[test]
    fn execute_emergency_drain_admin_after_timelock_succeeds() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        ctx.pool.propose_emergency_drain(&ctx.admin, &treasury, &500);
        // Advance clock past the 24-hour emergency drain timelock.
        ctx.env.ledger().with_mut(|l| l.timestamp += 86_401);
        assert!(ctx.pool.try_execute_emergency_drain(&ctx.admin).is_ok());
    }

    #[test]
    fn execute_emergency_drain_outsider_rejected_without_auth() {
        let ctx = setup();
        let treasury = Address::generate(&ctx.env);
        ctx.pool.propose_emergency_drain(&ctx.admin, &treasury, &500);
        ctx.env.ledger().with_mut(|l| l.timestamp += 86_401);
        ctx.env.set_auths(&[]);
        assert!(ctx.pool.try_execute_emergency_drain(&ctx.outsider).is_err());
    }
}
