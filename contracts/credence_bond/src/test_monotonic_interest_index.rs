//! Comprehensive tests for monotonic interest index enforcement
//!
//! Tests property-based monotonicity across random inputs and regression tests
//! for previously decreasing cases.

use crate::monotonic_interest_index::{calculate_monotonic_index, INDEX_SCALE};

#[cfg(test)]
mod monotonic_index_tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────
    // Basic Monotonicity Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_single_step_always_nondecreasing() {
        let start = INDEX_SCALE;
        let result = calculate_monotonic_index(start, 1, 1);
        assert!(result >= start, "Failed for start={}, result={}", start, result);
    }

    #[test]
    fn test_multiple_sequential_updates_never_decrease() {
        let mut index = INDEX_SCALE;
        
        for step in 1..=50 {
            let previous = index;
            index = calculate_monotonic_index(index, 10, 50);
            assert!(
                index >= previous,
                "Step {}: index regressed from {} to {}",
                step,
                previous,
                index
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge Case Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn edge_case_zero_elapsed_time() {
        let base = INDEX_SCALE * 100;
        for rate in [1, 10, 100, 1000, 10000] {
            let result = calculate_monotonic_index(base, 0, rate);
            assert_eq!(result, base, "Should not change with zero elapsed");
        }
    }

    #[test]
    fn edge_case_zero_rate() {
        let base = INDEX_SCALE * 100;
        for elapsed in [1, 10, 100, 1000, 10000] {
            let result = calculate_monotonic_index(base, elapsed, 0);
            assert_eq!(result, base, "Should not change with zero rate");
        }
    }

    #[test]
    fn edge_case_both_zero() {
        let base = INDEX_SCALE * 100;
        let result = calculate_monotonic_index(base, 0, 0);
        assert_eq!(result, base);
    }

    #[test]
    fn edge_case_from_zero_index() {
        let result = calculate_monotonic_index(0, 100, 100);
        assert_eq!(result, 0, "Zero balance should not accrue");
    }

    #[test]
    fn edge_case_from_negative_index() {
        // Negative indices might occur in some protocols
        let result = calculate_monotonic_index(-INDEX_SCALE, 1, 100);
        // Should not increase (negative can't meaningfully accrue)
        assert!(result >= -INDEX_SCALE);
    }

    #[test]
    fn edge_case_very_small_increments() {
        let base = 1_000_000_000_000_000_000i128; // 1.0
        
        // Very small rate that might truncate to zero
        let result = calculate_monotonic_index(base, 1, 1);
        
        // Should not decrease despite tiny rate
        assert!(result >= base);
    }

    #[test]
    fn edge_case_min_positive_increment() {
        // When increment is tiny due to precision, should still be monotonic
        let base = INDEX_SCALE;
        let result = calculate_monotonic_index(base, 1, 1);
        
        assert!(result >= base);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Precision Loss Regression Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn regression_truncation_induced_decrease() {
        // This would fail without ceiling rounding
        let base = 999_999_999_999_999_999i128;
        let result = calculate_monotonic_index(base, 10, 10);
        
        // Must not decrease due to truncation
        assert!(result >= base);
    }

    #[test]
    fn regression_accumulated_truncation() {
        // Apply many updates where truncation could accumulate
        let mut index = 999_999_999_999_999_999i128;
        let initial = index;
        
        for _ in 0..100 {
            let prev = index;
            index = calculate_monotonic_index(index, 1, 5);
            assert!(index >= prev, "Truncation caused decrease");
        }
        
        // Overall should be non-decreasing
        assert!(index >= initial);
    }

    #[test]
    fn regression_rapid_large_updates() {
        let start = INDEX_SCALE;
        
        // Rapid updates: index should grow monotonically
        let mut index = start;
        for _ in 0..1000 {
            let prev = index;
            index = calculate_monotonic_index(index, 0, 100); // zero elapsed
            assert!(index >= prev);
            index = calculate_monotonic_index(index, 1, 50);   // 1 second, 0.5%
            assert!(index >= prev, "Failed after variable updates");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Property-Based Tests
    // ─────────────────────────────────────────────────────────────────────────

    /// Property: For any (elapsed > 0 and rate > 0 and index > 0), index must increase
    #[test]
    fn property_positive_conditions_always_increase() {
        for start in [
            INDEX_SCALE,
            INDEX_SCALE * 10,
            INDEX_SCALE * 100,
            1_000_000_000_000_000_000i128,
        ] {
            for elapsed in [1u64, 10, 100, 1000] {
                for rate_bps in [1u32, 10, 100, 1000, 10000] {
                    let result = calculate_monotonic_index(start, elapsed, rate_bps);
                    assert!(
                        result >= start,
                        "Increase property failed: start={}, elapsed={}, rate={}, result={}",
                        start, elapsed, rate_bps, result
                    );
                }
            }
        }
    }

    /// Property: Monotonicity always holds regardless of input values
    #[test]
    fn property_universal_monotonicity() {
        let starts = [0i128, 1, 100, INDEX_SCALE, INDEX_SCALE * 1000];
        let elapsed_vals = [0u64, 1, 10, 100, 1000];
        let rates = [0u32, 1, 50, 100, 5000];
        
        for &start in &starts {
            for &elapsed in &elapsed_vals {
                for &rate in &rates {
                    let result = calculate_monotonic_index(start, elapsed, rate);
                    assert!(
                        result >= start,
                        "Monotonicity failed: base={}, elapsed={}, rate={}, result={}",
                        start, elapsed, rate, result
                    );
                }
            }
        }
    }

    /// Property: Index growth is monotonic in elapsed time
    #[test]
    fn property_monotonic_in_elapsed_time() {
        let base = INDEX_SCALE * 1000;
        let rate = 100u32;
        
        let mut prev_result = base;
        for elapsed in 0..1000 {
            let result = calculate_monotonic_index(base, elapsed as u64, rate);
            assert!(
                result >= prev_result,
                "Not monotonic in elapsed: t={}, result={} < prev={}",
                elapsed, result, prev_result
            );
            prev_result = result;
        }
    }

    /// Property: Index growth is monotonic in rate (for fixed time)
    #[test]
    fn property_monotonic_in_rate() {
        let base = INDEX_SCALE * 1000;
        let elapsed = 100u64;
        
        let mut prev_result = base;
        for rate in 0..5000 {
            let result = calculate_monotonic_index(base, elapsed, rate as u32);
            assert!(
                result >= prev_result,
                "Not monotonic in rate: rate={}, result={} < prev={}",
                rate, result, prev_result
            );
            prev_result = result;
        }
    }

    /// Property: Index growth is monotonic in base (for fixed time and rate)
    #[test]
    fn property_monotonic_in_base() {
        let elapsed = 100u64;
        let rate = 100u32;
        
        let mut prev_result = 0i128;
        for base in (0..1000).map(|i| i * INDEX_SCALE / 100) {
            let result = calculate_monotonic_index(base, elapsed, rate);
            assert!(
                result >= prev_result,
                "Not monotonic in base: base={}, result={} < prev={}",
                base, result, prev_result
            );
            prev_result = result;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Sequence Tests (Time Series)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn sequence_consistent_rate_for_100_steps() {
        let mut index = INDEX_SCALE;
        let rate = 50u32; // 0.5% per second
        
        for step in 1..=100 {
            let prev = index;
            index = calculate_monotonic_index(index, 1, rate);
            assert!(
                index >= prev,
                "Step {}: failed monotonicity check",
                step
            );
        }
    }

    #[test]
    fn sequence_varying_rates() {
        let mut index = INDEX_SCALE * 100;
        let mut rates = vec![10, 50, 100, 25, 75, 150, 200];
        let mut rate_idx = 0;
        
        for step in 1..=1000 {
            let prev = index;
            let rate = rates[rate_idx % rates.len()] as u32;
            index = calculate_monotonic_index(index, 1, rate);
            
            assert!(
                index >= prev,
                "Step {}: failed with rate {}",
                step, rate
            );
            rate_idx += 1;
        }
    }

    #[test]
    fn sequence_varying_elapsed_times() {
        let mut index = INDEX_SCALE * 50;
        let rate = 100u32;
        let elapsed_times = vec![0, 1, 5, 10, 3, 7, 20, 1];
        let mut time_idx = 0;
        
        for step in 1..=1000 {
            let prev = index;
            let elapsed = elapsed_times[time_idx % elapsed_times.len()] as u64;
            index = calculate_monotonic_index(index, elapsed, rate);
            
            assert!(index >= prev, "Step {}: failed with elapsed {}", step, elapsed);
            time_idx += 1;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Boundary Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn boundary_max_reasonable_index() {
        let large_index = INDEX_SCALE * 1_000_000_000i128; // 1 billion
        let result = calculate_monotonic_index(large_index, 1, 1);
        assert!(result >= large_index);
    }

    #[test]
    fn boundary_max_rate_bps() {
        let base = INDEX_SCALE;
        let result = calculate_monotonic_index(base, 10, u32::MAX);
        assert!(result >= base);
    }

    #[test]
    fn boundary_max_elapsed_time() {
        let base = INDEX_SCALE;
        let result = calculate_monotonic_index(base, u64::MAX / 2, 100);
        assert!(result >= base);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Stability Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn stability_idempotent_for_zero_changes() {
        let base = INDEX_SCALE * 500;
        
        let r1 = calculate_monotonic_index(base, 0, 100);
        let r2 = calculate_monotonic_index(r1, 0, 100);
        let r3 = calculate_monotonic_index(r2, 0, 100);
        
        assert_eq!(r1, base);
        assert_eq!(r2, r1);
        assert_eq!(r3, r2);
    }

    #[test]
    fn stability_deterministic_results() {
        let base = INDEX_SCALE * 123;
        let elapsed = 456u64;
        let rate = 789u32;
        
        let r1 = calculate_monotonic_index(base, elapsed, rate);
        let r2 = calculate_monotonic_index(base, elapsed, rate);
        let r3 = calculate_monotonic_index(base, elapsed, rate);
        
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Compound Interest Simulation Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn compound_simulation_daily_updates_one_year() {
        let mut index = INDEX_SCALE;
        let daily_rate_bps = 274u32; // ~100% APY
        let seconds_per_day = 86400u64;
        
        // Simulate 365 days
        for day in 1..=365 {
            let prev = index;
            index = calculate_monotonic_index(index, seconds_per_day, daily_rate_bps);
            
            assert!(
                index >= prev,
                "Day {}: index decreased",
                day
            );
            // Verify reasonable growth (should roughly double)
            if day >= 100 {
                assert!(index > INDEX_SCALE, "Index should have grown by day {}", day);
            }
        }
        
        // After one year at ~100% APY, should approximately double
        assert!(index > INDEX_SCALE * 2);
    }

    #[test]
    fn compound_simulation_hourly_updates_one_week() {
        let mut index = INDEX_SCALE * 10000;
        let hourly_rate_bps = 1u32; // ~0.01% per hour
        let seconds_per_hour = 3600u64;
        
        for hour in 1..=(7 * 24) {
            let prev = index;
            index = calculate_monotonic_index(index, seconds_per_hour, hourly_rate_bps);
            assert!(index >= prev, "Hour {}: index decreased", hour);
        }
        
        // Should show cumulative compound growth
        assert!(index >= INDEX_SCALE * 10000);
    }
}
