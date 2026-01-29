# Environment/Variable Filter

## Status
Not implemented

## Overview
Add filtering hooks for environment and shell variable mutations, allowing observation, modification, or blocking of variable operations.

## Use Cases
- Security: prevent modification of critical variables (PATH, LD_PRELOAD, etc.)
- Auditing: log all variable changes for compliance
- Sandboxing: restrict variable scope in embedded shells
- Debugging: trace variable mutations

## Proposed API

```rust
pub trait EnvFilter: Clone + Default + Send + Sync + 'static {
    /// Called before a variable is set or modified.
    fn pre_set_var<'a, SE: ShellExtensions>(
        &self,
        params: SetVarParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<SetVarParams<'a, SE>, SetVarOutput>> + Send;

    /// Called after a variable is set.
    fn post_set_var(
        &self,
        result: SetVarOutput,
    ) -> impl Future<Output = PostFilterResult<SetVarOutput>> + Send;

    /// Called before a variable is unset.
    fn pre_unset_var<'a, SE: ShellExtensions>(
        &self,
        params: UnsetVarParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<UnsetVarParams<'a, SE>, UnsetVarOutput>> + Send;

    /// Called when a variable is read/accessed.
    fn on_read_var<'a, SE: ShellExtensions>(
        &self,
        params: ReadVarParams<'a, SE>,
    ) -> impl Future<Output = ReadVarResult<'a>> + Send;
}
```

## Integration Points
- `brush-core/src/env.rs` - shell environment
- `brush-core/src/variables.rs` - variable types and operations
- Assignment commands (`VAR=value`)
- `export`, `declare`, `local`, `readonly` builtins
- `unset` builtin
- Variable expansion reads

## Considerations
- Performance: variables are accessed constantly
- Read hooks may need special handling (very hot path)
- Distinguish between shell variables and environment variables
- Handle array and associative array operations
- Special variables ($?, $!, $$, etc.) may need different treatment

## Related
- [expansion-filter.md](expansion-filter.md) - expansion filtering
