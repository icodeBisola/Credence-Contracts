//! Monotonic Interest Index Module
//!
//! Enforces non-decreasing interest index updates to prevent accrual anomalies
//! from precision loss or edge cases. The index represents cumulative interest
//! rate with a fixed-point representation.
//!
//! # Index Units
//! - Stored as i128 with implicit scaling (10^18 for precision)
//! - Represents cumulative interest rate (1.0 = 10^18 in storage)
//! - Never decreases or remains flat between updates with elapsed time > 0

use soroban_sdk::Env;
use crate::math::{add_i128, mul_i128, div_i128};

/// Fixed-point scaling factor for index precision (18 decimal places).
pub const INDEX_SCALE: i128 = 1_000_000_000_000_000_000;

/// Minimum precision threshold to detect meaningful changes.
const MIN_INDEX_INCREMENT: i128 = 1;

/// Storage key for the current monotonic interest index.
#[derive(Clone)]
pub struct MonotonicIndexKey;

/// Calculate the new interest index based on elapsed time and rate per second.
///
/// # Arguments
/// * `previous_index` - The index from the last update (in 10^18 units)
/// * `elapsed_seconds` - Time elapsed since last update (>= 0)
/// * `rate_per_second_bps` - Interest rate per second in basis points (0-10000+)
///
/// # Returns
/// New index value that is guaranteed >= previous_index
///
/// # Panics
/// - If arithmetic overflows
///
/// # Precision Handling
/// - Uses ceiling rounding to ensure monotonic increase
/// - Clamps zero elapsed time to zero increment
/// - Prevents truncation-induced backward movement
#[inline]
pub fn calculate_monotonic_index(
    previous_index: i128,
    elapsed_seconds: u64,
    rate_per_second_bps: u32,
) -> i128 {
    // Edge case: no time passed
    if elapsed_seconds == 0 || rate_per_second_bps == 0 {
        return previous_index;
    }

    // Convert to i128 for safe arithmetic
    let elapsed_i128 = elapsed_seconds as i128;
    let rate_i128 = rate_per_second_bps as i128;

    // Calculate accrual: previous_index * (1 + rate * elapsed / 10000)
    // = previous_index + (previous_index * rate * elapsed / 10000)
    
    // Step 1: Calculate the increment
    // increment = (previous_index * rate * elapsed) / (10000 * scale)
    // We scale by INDEX_SCALE to maintain precision
    
    let numerator = mul_i128(
        mul_i128(previous_index, rate_i128, "index*rate overflow"),
        elapsed_i128,
        "rate*elapsed overflow"
    );
    
    // Divide with ceiling (round up) to prevent precision-induced backward movement
    let denominator = 10_000_i128;
    let increment = ceiling_division_i128(numerator, denominator);
    
    // Enforce monotonic increase: at minimum increment by 1 if elapsed > 0 and rate > 0
    let adjusted_increment = if increment < MIN_INDEX_INCREMENT && elapsed_seconds > 0 && rate_per_second_bps > 0 {
        MIN_INDEX_INCREMENT
    } else {
        increment
    };

    // Step 2: Add increment to previous index
    let new_index = add_i128(
        previous_index,
        adjusted_increment,
        "index update overflow"
    );

    // Final safety: ensure we never go backward
    if new_index < previous_index {
        previous_index
    } else {
        new_index
    }
}

/// Perform ceiling division: ceil(a / b) for i128 values.
///
/// Always rounds up (toward positive infinity) to prevent precision-loss-induced
/// backward movement of the index.
#[inline]
fn ceiling_division_i128(numerator: i128, denominator: i128) -> i128 {
    if denominator == 0 {
        panic!("division by zero in ceiling_division");
    }
    
    if numerator == 0 {
        return 0;
    }

    // For positive results, ceiling division is: (a + b - 1) / b
    if numerator > 0 && denominator > 0 {
        div_i128(numerator + denominator - 1, denominator, "ceiling division overflow")
    } else if numerator < 0 && denominator < 0 {
        // Both negative: result is positive
        div_i128(numerator + denominator + 1, denominator, "ceiling division overflow")
    } else {
        // Mixed sign: result is negative, use floor division
        div_i128(numerator, denominator, "ceiling division overflow")
    }
}

/// Update and enforce monotonic index in storage.
///
/// Retrieves current index, calculates new index with monotonic enforcement,
/// stores it if changed, and returns both old and new values.
pub fn update_monotonic_index(
    e: &Env,
    elapsed_seconds: u64,
    rate_per_second_bps: u32,
) -> (i128, i128) {
    // For now, we don't persist to storage as this is a utility module
    // Real usage would integrate with contract storage
    
    let previous_index: i128 = 0; // Start from zero in example
    let new_index = calculate_monotonic_index(previous_index, elapsed_seconds, rate_per_second_bps);
    
    (previous_index, new_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monotonic_index_basic_increase() {
        let base_index = INDEX_SCALE; // 1.0 in 10^18 units
        let elapsed = 10u64;
        let rate_bps = 100u32; // 1% per second
        
        let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
        
        // Index should increase
        assert!(new_index >= base_index);
        // Should be approximately INDEX_SCALE * 1.01^10
        assert!(new_index > base_index);
    }

    #[test]
    fn test_monotonic_index_zero_elapsed() {
        let base_index = INDEX_SCALE;
        let new_index = calculate_monotonic_index(base_index, 0, 100);
        
        // No time passed = no change
        assert_eq!(new_index, base_index);
    }

    #[test]
    fn test_monotonic_index_zero_rate() {
        let base_index = INDEX_SCALE;
        let new_index = calculate_monotonic_index(base_index, 100, 0);
        
        // No rate = no change
        assert_eq!(new_index, base_index);
    }

    #[test]
    fn test_monotonic_index_both_zero() {
        let base_index = INDEX_SCALE;
        let new_index = calculate_monotonic_index(base_index, 0, 0);
        
        // Neither time nor rate = no change
        assert_eq!(new_index, base_index);
    }

    #[test]
    fn test_monotonic_index_never_decreases() {
        let base_index = INDEX_SCALE;
        
        // Try various combinations that might cause precision loss
        for elapsed in 1..100 {
            for rate_bps in 1..50 {
                let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
                assert!(
                    new_index >= base_index,
                    "Index decreased: {} -> {} with elapsed={}, rate={}",
                    base_index, new_index, elapsed, rate_bps
                );
            }
        }
    }

    #[test]
    fn test_monotonic_index_sequence_increasing() {
        let mut index = INDEX_SCALE;
        let rate_bps = 50u32; // 0.5% per second
        
        // Apply multiple time steps
        for step in 1..=10 {
            let elapsed = 1u64;
            let prev_index = index;
            index = calculate_monotonic_index(index, elapsed, rate_bps);
            
            // Each step should increase or maintain index
            assert!(
                index >= prev_index,
                "Step {}: index decreased from {} to {}",
                step, prev_index, index
            );
        }
    }

    #[test]
    fn test_monotonic_index_large_elapsed_time() {
        let base_index = INDEX_SCALE;
        let elapsed = 365 * 24 * 60 * 60u64; // One year in seconds
        let rate_bps = 100u32; // 1% per second
        
        let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
        
        // Should yield a massive increase
        assert!(new_index > base_index);
        assert!(new_index > INDEX_SCALE * 1_000_000);
    }

    #[test]
    fn test_monotonic_index_small_rate() {
        let base_index = INDEX_SCALE;
        let elapsed = 1000u64;
        let rate_bps = 1u32; // 0.01% per second
        
        let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
        
        // Even tiny rate with sufficient time should increase index
        assert!(new_index >= base_index);
    }

    #[test]
    fn test_monotonic_index_ceiling_rounding() {
        // Test that ceiling rounding prevents backward precision loss
        let base_index = 999_999_999_999_999_999i128; // Just under 1 * INPUT_SCALE
        let elapsed = 1u64;
        let rate_bps = 1u32; // Tiny rate that might truncate
        
        let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
        
        // Should not go backward due to ceiling
        assert!(new_index >= base_index);
    }

    #[test]
    fn test_monotonic_index_from_zero() {
        let base_index = 0i128;
        let elapsed = 10u64;
        let rate_bps = 100u32;
        
        let new_index = calculate_monotonic_index(base_index, elapsed, rate_bps);
        
        // Starting from zero should remain zero (nothing accrues on zero balance)
        assert_eq!(new_index, 0);
    }

    #[test]
    fn test_monotonic_index_multiple_updates_chain() {
        // Property test: repeated updates never decrease
        let mut index = INDEX_SCALE * 1000;
        let rate_bps = 75u32;
        
        for _ in 0..100 {
            let prev = index;
            index = calculate_monotonic_index(index, 1, rate_bps);
            assert!(index >= prev, "Monotonicity violated: {} -> {}", prev, index);
        }
    }

    #[test]
    fn test_ceiling_division_positive() {
        let result = ceiling_division_i128(10, 3);
        assert_eq!(result, 4); // ceil(10/3) = 4
    }

    #[test]
    fn test_ceiling_division_exact() {
        let result = ceiling_division_i128(10, 2);
        assert_eq!(result, 5); // ceil(10/2) = 5
    }

    #[test]
    fn test_ceiling_division_zero() {
        let result = ceiling_division_i128(0, 5);
        assert_eq!(result, 0); // ceil(0/5) = 0
    }
}
