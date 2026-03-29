# Monotonic Interest Index

## Overview

The Monotonic Interest Index is a core component that enforces non-decreasing cumulative interest rate tracking. This prevents accrual anomalies caused by precision loss or edge cases that could break fundamental assumptions about interest compounding.

**Key Property**: Index values **never decrease** or remain flat between updates when elapsed time > 0 and rate > 0.

## Index Units and Conventions

### Fixed-Point Representation

- **Storage Format**: `i128` (signed 128-bit integer)
- **Implicit Scaling**: 10^18 (18 decimal places)
- **Unit Reference**: `1.0` is represented as `1,000,000,000,000,000,000`
- **Entry Point**: `INDEX_SCALE` constant = `1_000_000_000_000_000_000`

### Interpretation

```
Stored Value          Represents
─────────────────────────────────────
0                     0.0 (no accrual)
1,000,000,000,000,000,000  1.0 (100% cumulative growth)
500,000,000,000,000,000    0.5 (50% cumulative growth)
2,000,000,000,000,000,000  2.0 (200% cumulative growth)
```

### Example: Annual Interest Rate

To represent a 5% annual interest rate:
- Stored as: `50_000_000_000_000_000` (5% of `INDEX_SCALE`)
- Represents: 0.05 or 5%

## Calculation Formula

### Basic Formula

The new index after time `t` with rate per second `r` is:

```
new_index = previous_index × (1 + r × t / 10,000)
          = previous_index + (previous_index × r × t / 10,000)
```

Where:
- `t` = elapsed seconds (u64)
- `r` = annual rate in basis points (u32)
- Division by 10,000 converts basis points to decimal

### Implementation Notes

1. **Ceiling Rounding**: All divisions use ceiling rounding to prevent precision-loss-induced backward movement
2. **Minimum Increment**: When elapsed > 0 and rate > 0, index increases by at least 1
3. **Edge Case Clamping**:
   - If `elapsed_seconds == 0`: no change
   - If `rate_per_second_bps == 0`: no change
   - If `previous_index == 0`: remains 0

## Monotonicity Guarantees

### Property 1: Non-Decreasing
```
new_index >= previous_index  (always)
```

### Property 2: Strict Increase (when conditions met)
```
If elapsed_seconds > 0 AND rate > 0 AND previous_index > 0:
    new_index > previous_index
```

### Property 3: Deterministic
```
Same inputs → Same output (always)
```

## Precision Loss Protection

### The Problem
Traditional floor division can cause index to regress:
```
previous_index = 999,999,999,999,999,999
increment (floor) = 0 (due to truncation)
new_index = 999,999,999,999,999,999  ← ❌ No growth!
```

### The Solution
Ceiling division ensures monotonic increase:
```
previous_index = 999,999,999,999,999,999
increment (ceiling) = 1 (rounds up)
new_index = 1,000,000,000,000,000,000  ← ✓ Grows!
```

## Edge Cases

### All Edge Cases Return Unchanged Index

| Scenario | Input | Output |
|----------|-------|--------|
| Zero time elapsed | (base, 0, rate) | base |
| Zero rate | (base, elapsed, 0) | base |
| Both zero | (base, 0, 0) | base |
| Starting from zero | (0, elapsed, rate) | 0 |
| Starting negative | (-base, elapsed, rate) | >= -base |

## Usage Example

```rust
use crate::monotonic_interest_index::{calculate_monotonic_index, INDEX_SCALE};

fn update_accrual() {
    let mut index = INDEX_SCALE;  // Start at 1.0
    
    // Update after 1 day (86,400 seconds) at 10% APY
    let seconds_per_day = 86_400u64;
    let rate_bps_annual = 1_000u32;  // 10% per year
    
    index = calculate_monotonic_index(index, seconds_per_day, rate_bps_annual);
    // Result: index > INDEX_SCALE (guaranteed monotonic increase)
}
```

## Integration Patterns

### Pattern 1: Per-Update Tracking
```rust
// Store both old and new index for event emission
let (old_index, new_index) = update_monotonic_index(env, elapsed, rate);
emit_index_updated_event(old_index, new_index);
```

### Pattern 2: Compound Interest Over Time
```rust
// Index naturally compounds as you apply sequential updates
let mut index = INDEX_SCALE;
for _ in 0..365 {
    index = calculate_monotonic_index(index, 86_400, rate_bps);
}
// Result: Naturally compounded 365-day interest
```

### Pattern 3: Interest Accrual Calculation
```rust
// Use index to calculate accrued amount
let accrued = principal * new_index / INDEX_SCALE;
```

## Testing Strategy

### Unit Tests
- ✓ Basic non-decreasing property
- ✓ Edge cases (zero elapsed, zero rate, etc.)
- ✓ Precision loss regression cases
- ✓ Ceiling rounding verification

### Property Tests
- ✓ Universal monotonicity across all input ranges
- ✓ Monotonicity in elapsed time (for fixed base and rate)
- ✓ Monotonicity in rate (for fixed base and time)
- ✓ Monotonicity in base (for fixed time and rate)

### Integration Tests
- ✓ Compound interest simulation (daily updates for 1 year)
- ✓ Hourly updates (1 week)
- ✓ Varying rates scenario
- ✓ Deterministic behavior verification

### Regression Tests
- ✓ Previously failing truncation cases
- ✓ Accumulated precision loss over 100+ steps
- ✓ Rapid updates with alternating parameters

## Performance Characteristics

| Operation | Time Complexity | Space Complexity |
|-----------|-----------------|------------------|
| Calculate index | O(1) | O(1) |
| Ceiling division | O(1) | O(1) |
| Single update | O(1) | O(1) |
| Verify monotonicity | O(n) | O(1) |

All operations are constant-time, making this suitable for on-chain use.

## Security Considerations

### 1. Overflow Protection
- Uses checked arithmetic with panic messages
- Validates all multiplication and addition operations
- Safe for extreme values (up to i128::MAX)

### 2. Precision Guarantees
- Ceiling rounding prevents precision-loss attacks
- Minimum increment of 1 when time > 0 and rate > 0
- No way to construct decreasing sequence with positive inputs

### 3. Use in Access Control
- **DO NOT** use as sole access control mechanism
- Use as **supporting data** for fairness calculations
- Combine with explicit time-based checks where needed

### 4. Rate Input Validation
- Rate should be validated before calling calculate_monotonic_index
- Ensure rate is in reasonable bounds (typically 0-10,000 bps = 0-100%)
- Contract should enforce rate limits at entry points

## Debugging Tips

### Monotonicity Violations
If you encounter a monotonicity failure:
1. Check input values (elapsed_seconds, rate_bps)
2. Verify ceiling division implementation
3. Ensure INDEX_SCALE constant is correct (10^18)
4. Run property tests on your input range

### Precision Issues
If results seem incorrect:
1. Verify index storage format (i128, not u64)
2. Check scaling is consistent (always use INDEX_SCALE)
3. Ensure rate is in basis points (0-10,000 for typical rates)
4. Use INDEX_SCALE for any calculations with index values

### Performance Concerns
- Single index update: << 1 microsecond
- 1000 sequential updates: << 1 millisecond
- No storage allocation, constant memory

## Future Enhancements

Potential improvements to consider:
1. Configurable precision levels (not just 10^18)
2. Custom rounding modes for specific protocols
3. Bulk update operations
4. Integration with Soroban storage for persistence
5. Multicurrency index tracking

## References

- [Interest Rate Mathematics](https://en.wikipedia.org/wiki/Compound_interest)
- [Fixed-Point Arithmetic](https://en.wikipedia.org/wiki/Fixed-point_arithmetic)
- [Precision in Smart Contracts](https://docs.openzeppelin.com/contracts/4.x/erc20-extensions#erc20votescompat)
