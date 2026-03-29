#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};
    use crate::redeem::{self, RedeemConfig};
    use crate::{DataKey, IdentityBond};

    /// Setup helper: Create a test environment and initialize bond data
    fn setup_test(e: &Env) -> (Address, IdentityBond) {
        let user = Address::generate(e);
        let bond = IdentityBond {
            identity: user.clone(),
            bonded_amount: 1_000_000,
            bond_start: 1000,
            bond_duration: 86400, // 1 day
            slashed_amount: 0,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period_duration: 0,
        };
        e.storage().instance().set(&DataKey::Bond, &bond);
        (user, bond)
    }

    /// Setup helper: Initialize liquidity
    fn setup_liquidity(e: &Env, amount: i128) {
        e.storage()
            .persistent()
            .set(&DataKey::TotalLiquidity, &amount);
    }

    /// Setup helper: Initialize redeem configuration
    fn setup_redeem_config(e: &Env, config: RedeemConfig) {
        e.storage()
            .persistent()
            .set(&DataKey::RedeemConfig, &config);
    }

    #[test]
    fn test_redeem_success_with_sufficient_liquidity() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 500_000);

        e.mock_all_auths();

        // Should succeed with sufficient liquidity
        let result = redeem::execute_redeem(&e, user.clone(), 100_000);
        assert_eq!(result.bonded_amount, 900_000);

        // Verify liquidity was decremented
        let updated_liquidity: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        assert_eq!(updated_liquidity, 400_000);
    }

    #[test]
    #[should_panic(expected = "insufficient liquidity for redemption")]
    fn test_redeem_fails_with_insufficient_liquidity() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        // Setup insufficient liquidity
        setup_liquidity(&e, 50_000);

        e.mock_all_auths();

        // Should panic: requesting 100,000 but only 50,000 available
        redeem::execute_redeem(&e, user.clone(), 100_000);
    }

    #[test]
    #[should_panic(expected = "insufficient liquidity for redemption")]
    fn test_redeem_respects_minimum_reserve() {
        let e = Env::default();
        let (user, _) = setup_test(&e);

        let config = RedeemConfig {
            min_liquidity_reserve: 100_000,
            max_redeem_amount: i128::MAX,
        };
        setup_redeem_config(&e, config);
        setup_liquidity(&e, 200_000);

        e.mock_all_auths();

        // Should panic: trying to redeem 150_000 but reserve is 100_000,
        // so only 100_000 is available (200_000 - 100_000)
        redeem::execute_redeem(&e, user.clone(), 150_000);
    }

    #[test]
    #[should_panic(expected = "insufficient balance for redemption")]
    fn test_redeem_fails_with_insufficient_balance() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 2_000_000);

        e.mock_all_auths();

        // Bond only has 1_000_000, trying to redeem 1_500_000 should fail
        redeem::execute_redeem(&e, user.clone(), 1_500_000);
    }

    #[test]
    #[should_panic(expected = "insufficient balance for redemption")]
    fn test_redeem_respects_slashed_amount() {
        let e = Env::default();
        let user = Address::generate(&e);

        // Setup bond with slashing
        let bond = IdentityBond {
            identity: user.clone(),
            bonded_amount: 1_000_000,
            bond_start: 1000,
            bond_duration: 86400,
            slashed_amount: 300_000, // 300k is slashed
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period_duration: 0,
        };
        e.storage().instance().set(&DataKey::Bond, &bond);
        setup_liquidity(&e, 2_000_000);

        e.mock_all_auths();

        // Redeemable amount is 1_000_000 - 300_000 = 700_000
        // Trying to redeem 800_000 should fail
        redeem::execute_redeem(&e, user.clone(), 800_000);
    }

    #[test]
    #[should_panic(expected = "redeem amount must be positive")]
    fn test_redeem_rejects_zero_amount() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        redeem::execute_redeem(&e, user.clone(), 0);
    }

    #[test]
    #[should_panic(expected = "redeem amount must be positive")]
    fn test_redeem_rejects_negative_amount() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        redeem::execute_redeem(&e, user.clone(), -1000);
    }

    #[test]
    #[should_panic(expected = "redeem amount exceeds maximum")]
    fn test_redeem_respects_max_amount_limit() {
        let e = Env::default();
        let (user, _) = setup_test(&e);

        let config = RedeemConfig {
            min_liquidity_reserve: 0,
            max_redeem_amount: 500_000, // Max is 500k
        };
        setup_redeem_config(&e, config);
        setup_liquidity(&e, 2_000_000);

        e.mock_all_auths();

        // Trying to redeem 600_000 should fail
        redeem::execute_redeem(&e, user.clone(), 600_000);
    }

    #[test]
    fn test_redeem_preserves_invariant_slashed_never_exceeds_bonded() {
        let e = Env::default();
        let user = Address::generate(&e);

        let bond = IdentityBond {
            identity: user.clone(),
            bonded_amount: 1_000_000,
            bond_start: 1000,
            bond_duration: 86400,
            slashed_amount: 900_000, // High slashing, only 100k available
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period_duration: 0,
        };
        e.storage().instance().set(&DataKey::Bond, &bond);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        // Redeem 50k (leaves 950k bonded, 900k slashed - still valid)
        let result = redeem::execute_redeem(&e, user.clone(), 50_000);
        assert_eq!(result.bonded_amount, 950_000);
        assert_eq!(result.slashed_amount, 900_000);
        assert!(result.slashed_amount <= result.bonded_amount);
    }

    #[test]
    #[should_panic(expected = "no bond")]
    fn test_redeem_fails_with_no_bond() {
        let e = Env::default();
        let user = Address::generate(&e);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        // No bond set up - should fail
        redeem::execute_redeem(&e, user.clone(), 100_000);
    }

    #[test]
    fn test_simulate_redeem_does_not_modify_state() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        // Simulate and check result
        let simulated_amount = redeem::simulate_redeem(&e, &user, 100_000);
        assert_eq!(simulated_amount, 100_000);

        // Verify state was not modified
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&DataKey::Bond)
            .unwrap();
        assert_eq!(bond.bonded_amount, 1_000_000);

        let liquidity: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        assert_eq!(liquidity, 1_000_000);
    }

    #[test]
    #[should_panic(expected = "insufficient liquidity for redemption")]
    fn test_simulate_redeem_validates_liquidity() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 50_000); // Insufficient

        e.mock_all_auths();

        // Simulation should also validate liquidity
        redeem::simulate_redeem(&e, &user, 100_000);
    }

    #[test]
    fn test_get_available_liquidity_respects_reserve() {
        let e = Env::default();

        let config = RedeemConfig {
            min_liquidity_reserve: 100_000,
            max_redeem_amount: i128::MAX,
        };
        setup_redeem_config(&e, config);
        setup_liquidity(&e, 500_000);

        let available = redeem::get_available_liquidity(&e);
        // Should be 500k - 100k reserve = 400k
        assert_eq!(available, 400_000);
    }

    #[test]
    fn test_get_available_liquidity_never_negative() {
        let e = Env::default();

        let config = RedeemConfig {
            min_liquidity_reserve: 1_000_000,
            max_redeem_amount: i128::MAX,
        };
        setup_redeem_config(&e, config);
        setup_liquidity(&e, 500_000); // Less than reserve

        let available = redeem::get_available_liquidity(&e);
        // Should return 0, not negative
        assert_eq!(available, 0);
    }

    #[test]
    fn test_redeem_multiple_times_atomicity() {
        let e = Env::default();
        let (user, _) = setup_test(&e);
        setup_liquidity(&e, 1_000_000);

        e.mock_all_auths();

        // First redemption: 100k
        let result1 = redeem::execute_redeem(&e, user.clone(), 100_000);
        assert_eq!(result1.bonded_amount, 900_000);

        // Second redemption: 200k
        let result2 = redeem::execute_redeem(&e, user.clone(), 200_000);
        assert_eq!(result2.bonded_amount, 700_000);

        // Verify liquidity tracking
        let liquidity: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        assert_eq!(liquidity, 700_000); // 1_000_000 - 100_000 - 200_000
    }

    #[test]
    fn test_redeem_config_persistence() {
        let e = Env::default();
        let admin = Address::generate(&e);

        let config = RedeemConfig {
            min_liquidity_reserve: 250_000,
            max_redeem_amount: 750_000,
        };

        e.mock_all_auths();
        redeem::set_config(&e, admin.clone(), config);

        let retrieved_config = redeem::get_or_init_config(&e);
        assert_eq!(retrieved_config.min_liquidity_reserve, 250_000);
        assert_eq!(retrieved_config.max_redeem_amount, 750_000);
    }
}
