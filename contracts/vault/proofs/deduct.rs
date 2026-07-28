//! Kani proof harnesses for `CalloraVault::deduct` balance conservation.
//!
//! The vault's successful `deduct` path moves `amount` out of the tracked vault
//! balance and credits that same `amount` to settlement. These harnesses model
//! that state transition with the same arithmetic preconditions enforced by the
//! contract and prove that the combined accounting total is unchanged.
//!
//! Run with:
//! ```bash
//! cargo kani --package callora-vault --harness kani_deduct_conserves_total_supply
//! ```

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeductState {
    vault_balance: i128,
    settlement_credit: i128,
}

impl DeductState {
    fn total_supply(self) -> i128 {
        self.vault_balance
            .checked_add(self.settlement_credit)
            .expect("modeled supply total must fit in i128")
    }
}

fn model_successful_deduct(state: DeductState, amount: i128, max_deduct: i128) -> DeductState {
    assert!(
        state.vault_balance >= 0,
        "vault balance must be non-negative"
    );
    assert!(
        state.settlement_credit >= 0,
        "settlement credit must be non-negative"
    );
    assert!(amount > 0, "deduct amount must be positive");
    assert!(max_deduct > 0, "max_deduct must be positive");
    assert!(
        amount <= max_deduct,
        "deduct amount must respect max_deduct"
    );
    assert!(state.vault_balance >= amount, "deduct must be funded");

    DeductState {
        vault_balance: state
            .vault_balance
            .checked_sub(amount)
            .expect("funded deduct cannot underflow"),
        settlement_credit: state
            .settlement_credit
            .checked_add(amount)
            .expect("modeled settlement credit cannot overflow"),
    }
}

#[cfg(kani)]
#[kani::proof]
fn kani_deduct_conserves_total_supply() {
    let vault_balance: i128 = kani::any();
    let settlement_credit: i128 = kani::any();
    let amount: i128 = kani::any();
    let max_deduct: i128 = kani::any();

    kani::assume(vault_balance >= 0);
    kani::assume(settlement_credit >= 0);
    kani::assume(amount > 0);
    kani::assume(max_deduct > 0);
    kani::assume(amount <= max_deduct);
    kani::assume(vault_balance >= amount);
    // Both the pre-state total and post-state settlement credit are modeled as
    // i128 values, matching the contract's checked arithmetic domain.
    kani::assume(vault_balance <= i128::MAX - settlement_credit);
    kani::assume(settlement_credit <= i128::MAX - amount);

    let before = DeductState {
        vault_balance,
        settlement_credit,
    };
    let before_total = before.total_supply();

    let after = model_successful_deduct(before, amount, max_deduct);

    assert_eq!(
        after.total_supply(),
        before_total,
        "successful deduct must conserve total supply"
    );
    assert_eq!(
        after.vault_balance,
        vault_balance - amount,
        "vault balance must decrease by exactly amount"
    );
    assert_eq!(
        after.settlement_credit,
        settlement_credit + amount,
        "settlement credit must increase by exactly amount"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modeled_deduct_moves_amount_without_changing_total() {
        let before = DeductState {
            vault_balance: 1_000,
            settlement_credit: 250,
        };

        let after = model_successful_deduct(before, 400, 500);

        assert_eq!(after.vault_balance, 600);
        assert_eq!(after.settlement_credit, 650);
        assert_eq!(after.total_supply(), before.total_supply());
    }

    #[test]
    #[should_panic(expected = "deduct must be funded")]
    fn modeled_deduct_rejects_unfunded_amount_before_mutation() {
        let before = DeductState {
            vault_balance: 99,
            settlement_credit: 0,
        };

        let _ = model_successful_deduct(before, 100, 100);
    }

    #[test]
    #[should_panic(expected = "deduct amount must respect max_deduct")]
    fn modeled_deduct_rejects_amount_above_max_deduct() {
        let before = DeductState {
            vault_balance: 100,
            settlement_credit: 0,
        };

        let _ = model_successful_deduct(before, 100, 99);
    }
}
