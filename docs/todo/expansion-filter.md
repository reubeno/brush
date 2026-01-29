# Expansion Filter

## Status
Not implemented (infrastructure removed to reduce complexity)

## Overview
Add filtering hooks for word expansion operations, allowing observation and modification of shell expansion behavior.

## Use Cases
- Auditing variable expansions for security monitoring
- Sandboxing by blocking or modifying certain expansions
- Debugging expansion behavior
- Custom expansion logic for embedded shells

## Proposed API

```rust
pub trait ExpansionFilter: Clone + Default + Send + Sync + 'static {
    /// Called before a word is expanded.
    fn pre_expand_word<'a, SE: ShellExtensions>(
        &self,
        params: WordExpandParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<WordExpandParams<'a, SE>, WordExpandOutput>> + Send;

    /// Called after a word is expanded.
    fn post_expand_word(
        &self,
        result: WordExpandOutput,
    ) -> impl Future<Output = PostFilterResult<WordExpandOutput>> + Send;
}
```

## Integration Points
- `brush-core/src/expansion.rs` - main expansion logic
- Parameter expansion (`${var}`, `${var:-default}`, etc.)
- Command substitution (`$(cmd)`, `` `cmd` ``)
- Arithmetic expansion (`$((expr))`)
- Brace expansion (`{a,b,c}`)
- Tilde expansion (`~`, `~user`)
- Pathname expansion (globbing)

## Considerations
- Performance impact on every expansion
- Granularity: single hook vs per-expansion-type hooks
- Recursion handling for nested expansions
- Order of operations in expansion pipeline

## Related
- [env-variable-filter.md](env-variable-filter.md) - variable access filtering
