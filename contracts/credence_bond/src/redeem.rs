//! # Redemption Module
//!
//! Implements the redemption flow with liquidity validation.
//! Ensures that user balances are not reduced if redemption cannot settle
//! by validating available cash BEFORE burning share tokens.
//!
//! ## Key Invariants
//!
//! 1. **Pre-Burn Validation**: Check available liquidity before any state mutations
//! 2. **Atomic Settlement**: Either fully settle or fully revert - no partial states
//! 3. **Coherent Accounting**: Update internal state before external transfers
//! 4. **Event Integrity**: Emit events in correct order and only on success

use soroban_sdk::{Address, Env, Symbol};
use crate::{DataKey, IdentityBond};

/// Configuration for redemptions
#[derive(Clone, Copy, Debug)]
pub struct RedeemConfig {
    /// Minimum liquidity reserve (prevents draining contract)
    pub min_liquidity_reserve: i128,
    /// Maximum redeem amount per transaction (safety limit)
    pub max_redeem_amount: i128,
}

/// Configuration for redemptions
#[derive(Clone, Copy, Debug)]
pub struct RedeemConfig {
    /// Minimum liquidity reserve (prevents draining contract)
    pub min_liquidity_reserve: i128,
    /// Maximum redeem amount per transaction (safety limit)
    pub max_redeem_amount: i128,
}

impl Default for RedeemConfig {
    fn default() -> Self {
        RedeemConfig {
            min_liquidity_reserve: 0,
            max_redeem_amount: i128::MAX,
        }
    }
}

/// Get or initialize the redeem configuration
pub fn get_or_init_config(e: &Env) -> RedeemConfig {
    e.storage()
        .persistent()
        .get(&DataKey::RedeemConfig)
        .unwrap_or_default()
}

/// Set the redeem configuration (admin only)
pub fn set_config(e: &Env, admin: Address, config: RedeemConfig) {
    admin.require_auth();
    e.storage()
        .persistent()
        .set(&DataKey::RedeemConfig, &config);
    emit_config_update(e, &admin, config);
}

/// Get total available liquidity in the contract
///
/// This represents the cash available for redemptions.
/// Formula: total_balance - min_reserve
pub fn get_available_liquidity(e: &Env) -> i128 {
    let config = get_or_init_config(e);
    let total_balance: i128 = e
        .storage()
        .persistent()
        .get(&DataKey::TotalLiquidity)
        .unwrap_or(0);

    // Ensure we never go below minimum reserve
    let available = total_balance.saturating_sub(config.min_liquidity_reserve);
    available.max(0)
}

/// Check if sufficient liquidity is available for a redemption
fn validate_liquidity_available(e: &Env, amount: i128) -> bool {
    if amount <= 0 {
        return false; // Invalid amount
    }

    let available = get_available_liquidity(e);
    amount <= available
}

/// Core redemption logic: validates, burns shares, and transfers underlying assets
///
/// # Arguments
/// * `e` - Environment
/// * `user` - User redeeming their position
/// * `amount` - Amount of shares to burn (must equal amount of underlying)
///
/// # Returns
/// Updated bond with reduced position
///
/// # Panics
/// * If insufficient liquidity is available
/// * If user is unauthorized
/// * If insufficient redeemable balance
/// * If amount would violate safety limits
///
/// # Invariant
/// User balance only changes after liquidity is confirmed available.
/// If any step fails, the entire transaction reverts atomically.
pub fn execute_redeem(e: &Env, user: Address, amount: i128) -> IdentityBond {
    user.require_auth();

    // Step 1: Load current bond state
    let key = DataKey::Bond;
    let mut bond: IdentityBond = e
        .storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| panic!("no bond"));

    // Step 2: Validate redeem amount (BEFORE any state changes)
    if amount <= 0 {
        panic!("redeem amount must be positive");
    }

    let config = get_or_init_config(e);
    if amount > config.max_redeem_amount {
        panic!("redeem amount exceeds maximum");
    }

    // Step 3: Calculate redeemable balance (bonded - slashed)
    let redeemable = bond
        .bonded_amount
        .checked_sub(bond.slashed_amount)
        .expect("slashed exceeds bonded");

    if amount > redeemable {
        panic!("insufficient balance for redemption");
    }

    // Step 4: CRITICAL: Validate liquidity is available BEFORE mutating state
    if !validate_liquidity_available(e, amount) {
        panic!("insufficient liquidity for redemption");
    }

    // Step 5: State mutation - reduce bond position
    let previous_amount = bond.bonded_amount;
    bond.bonded_amount = bond
        .bonded_amount
        .checked_sub(amount)
        .expect("redemption underflow");

    // Verify invariant: slashed should never exceed bonded after modification
    if bond.slashed_amount > bond.bonded_amount {
        panic!("slashed amount exceeds bonded amount after redemption");
    }

    // Step 6: Update persistent storage (before external calls per CEI pattern)
    e.storage().instance().set(&key, &bond);

    // Step 7: Update liquidity tracking
    let current_liquidity: i128 = e
        .storage()
        .persistent()
        .get(&DataKey::TotalLiquidity)
        .unwrap_or(0);

    let new_liquidity = current_liquidity
        .checked_sub(amount)
        .expect("liquidity underflow");

    e.storage()
        .persistent()
        .set(&DataKey::TotalLiquidity, &new_liquidity);

    // Step 8: Emit redeem event (after state updates, before external transfer)
    emit_redeem_initiated(e, &user, amount, previous_amount, bond.bonded_amount);

    // Step 9: External transfer (per Checks-Effects-Interactions pattern)
    // The actual token transfer would be handled by the calling contract
    // or delegated to token_integration module

    // Step 10: Emit redemption completed event
    emit_redeem_completed(e, &user, amount, bond.bonded_amount);

    bond
}

/// Validate all invariants for a successful redemption
pub fn validate_invariants(e: &Env, user: &Address, amount: i128) {
    // User must have authorized this call
    user.require_auth();

    // Bond must exist
    let key = DataKey::Bond;
    let bond: IdentityBond = e
        .storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| panic!("no bond"));

    // User must own the bond
    if bond.identity != *user {
        panic!("unauthorized: user does not own bond");
    }

    // Amount must be positive
    if amount <= 0 {
        panic!("redeem amount must be positive");
    }

    // Redeemable balance must be sufficient
    let redeemable = bond
        .bonded_amount
        .checked_sub(bond.slashed_amount)
        .expect("slashed exceeds bonded");

    if amount > redeemable {
        panic!("insufficient balance for redemption");
    }

    // Liquidity must be available
    if !validate_liquidity_available(e, amount) {
        panic!("insufficient liquidity for redemption");
    }
}

/// Simulate a redemption without modifying state
/// Used for gas estimation and UI preview
pub fn simulate_redeem(e: &Env, user: &Address, amount: i128) -> i128 {
    // Perform all validation checks
    validate_invariants(e, user, amount);

    // Return the net amount that would be received
    amount
}

/// Event: redemption configuration updated
fn emit_config_update(e: &Env, admin: &Address, config: RedeemConfig) {
    let topics = (Symbol::new(e, "redeem_config_updated"), admin.clone());
    let data = (config.min_liquidity_reserve, config.max_redeem_amount);
    e.events().publish(topics, data);
}

/// Event: redemption initiated (sale of shares)
fn emit_redeem_initiated(
    e: &Env,
    user: &Address,
    amount: i128,
    previous_bonded: i128,
    new_bonded: i128,
) {
    let topics = (Symbol::new(e, "redeem_initiated"), user.clone());
    let data = (amount, previous_bonded, new_bonded);
    e.events().publish(topics, data);
}

/// Event: redemption completed (shares burned, liquidity transferred)
fn emit_redeem_completed(e: &Env, user: &Address, amount: i128, remaining_bonded: i128) {
    let topics = (Symbol::new(e, "redeem_completed"), user.clone());
    let data = (amount, remaining_bonded);
    e.events().publish(topics, data);
}
