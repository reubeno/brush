# Filter Clone Optimization

## Status
Deferred optimization

## Overview
Investigate whether filter invocation can use references instead of cloning the filter, to reduce overhead for stateful filters.

## Current Implementation
```rust
// In with_filter! macro
let __filter = $shell.$filter_accessor().clone();
match __filter.$pre_method($params).await {
    // ...
}
```

The filter is cloned before use. For `NoOpCmdExecFilter` (a ZST), this is zero-cost. For filters with `Arc<Mutex<...>>` state, it's a cheap atomic increment. But it's still unnecessary if we can use a reference.

## Challenge
The filter methods take `&self`, so a reference should work. However:
1. Async methods may have lifetime constraints that make borrowing difficult
2. The shell reference in params already borrows from the shell
3. Filter might need to outlive the await point

## Proposed Investigation
1. Try changing macro to use `&$shell.$filter_accessor()` instead of clone
2. Check if lifetime errors arise
3. Benchmark both approaches with stateful filters
4. Consider `&'static` filter references if filters are always `'static`

## Impact
- Minor performance improvement for stateful filters
- No impact for ZST no-op filters (already zero-cost)
- Low priority since Arc clone is already cheap

## Related
- Filter architecture documentation
- Zero-overhead design goals
