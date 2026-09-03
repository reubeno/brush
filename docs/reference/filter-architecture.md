# Filter Architecture

This document describes the filter infrastructure in brush, which provides hooks for intercepting and modifying shell operations.

## Design Philosophy

### Zero-Overhead Abstraction

Filters use Rust's trait system and monomorphization to achieve zero runtime cost when using the default no-op filters:

- Filter types are generic parameters on `Shell<SE>` where `SE: ShellExtensions`
- No-op filters are zero-sized types (ZSTs) with inline default implementations
- The compiler eliminates filter code paths entirely when no-op filters are used
- Custom filters incur only the cost of their actual implementation

### Minimal Hook-Site Boilerplate

The `with_filter!` macro reduces boilerplate at hook sites:

```rust
// Observation-only hook (params not used after filter)
with_filter!(
    shell,
    cmd_exec_filter,
    pre_simple_cmd,
    post_simple_cmd,
    SimpleCmdParams::new(&shell, cmd_name, &args),
    execute_impl().await
)

// Hook with params capture (for filters that modify)
with_filter!(
    shell,
    source_filter,
    pre_source_script,
    post_source_script,
    SourceScriptParams::new(&shell, path, &args),
    |p| run_script(p.path(), p.args()).await
)
```

## Current Filters

### CmdExecFilter

Hooks for command execution at two levels:

| Hook | Scope | Modification Support |
|------|-------|---------------------|
| `pre_simple_cmd` | All commands (builtins, functions, externals) | Observation/short-circuit only |
| `post_simple_cmd` | After any command | Result transformation |
| `pre_external_cmd` | External process spawning only | Full modification via `ExternalCommand` |
| `post_external_cmd` | After external spawn | Result transformation |

**Execution Flow:**
```
Command Invoked
    │
    ▼
pre_simple_cmd ──[Return]──► Short-circuit with result
    │
    │ [Continue]
    ▼
Resolve command type (builtin/function/external)
    │
    ├─[Builtin]──► Execute builtin
    ├─[Function]─► Execute function
    └─[External]─┬► pre_external_cmd ──[Return]──► Short-circuit
                 │      │ [Continue]
                 │      ▼
                 │  Spawn process
                 │      │
                 │      ▼
                 └► post_external_cmd
    │
    ▼
post_simple_cmd
    │
    ▼
Return result
```

### SourceFilter

Hooks for script sourcing (`.` and `source` builtins):

| Hook | Purpose |
|------|---------|
| `pre_source_script` | Observe/modify path and args, or block sourcing |
| `post_source_script` | Transform execution result |

## Filter Result Types

### PreFilterResult

```rust
pub enum PreFilterResult<I, O> {
    /// Continue with operation, optionally with modified input
    Continue(I),
    /// Short-circuit and return this result immediately
    Return(O),
}
```

### PostFilterResult

```rust
pub enum PostFilterResult<O> {
    /// Return this (possibly modified) result
    Return(O),
}
```

## Implementing Custom Filters

### Basic Example

```rust
use brush_core::filter::{CmdExecFilter, PreFilterResult, SimpleCmdParams, SimpleCmdOutput};
use brush_core::extensions::ShellExtensions;

#[derive(Clone, Default)]
struct LoggingFilter;

impl CmdExecFilter for LoggingFilter {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        println!("Executing: {}", params.command_name());
        PreFilterResult::Continue(params)
    }
}
```

### Stateful Filter with Shared State

```rust
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct AuditingFilter {
    log: Arc<Mutex<Vec<String>>>,
}

impl CmdExecFilter for AuditingFilter {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        if let Ok(mut log) = self.log.lock() {
            log.push(params.command_name().to_string());
        }
        PreFilterResult::Continue(params)
    }
}
```

### Wiring Filters into Shell

1. **Define ShellExtensions implementation:**

```rust
use brush_core::extensions::{ShellExtensions, DefaultErrorFormatter};
use brush_core::filter::{NoOpSourceFilter};

#[derive(Clone, Default)]
struct MyExtensions {
    cmd_filter: MyCustomFilter,
}

impl ShellExtensions for MyExtensions {
    type ErrorFormatter = DefaultErrorFormatter;
    type CmdExecFilter = MyCustomFilter;
    type SourceFilter = NoOpSourceFilter;
}
```

2. **Build shell with extensions:**

```rust
let shell = Shell::builder_with_extensions::<MyExtensions>()
    .cmd_exec_filter(my_preconfigured_filter)
    .build()
    .await?;
```

## Thread Safety

Filters must implement `Clone + Send + Sync + 'static`:

- **Clone**: Filters may be cloned when shell is forked
- **Send + Sync**: Filters are accessed from async contexts
- **'static**: Filters live as long as the shell

For shared mutable state, use `Arc<Mutex<T>>` or `Arc<RwLock<T>>`. The `Arc` ensures state is shared across shell clones (subshells).

## Panic Safety

**Filter implementations must not panic.** Panics in filter methods will propagate through the shell and may terminate the process. There is no `catch_unwind` wrapper around filter invocations for performance reasons.

When implementing filters with fallible operations:

```rust
// DO: Handle errors gracefully
if let Ok(mut log) = self.log.lock() {
    log.push(entry);
}

// DON'T: Panic on lock failure
self.log.lock().unwrap().push(entry);  // May panic on poisoned mutex!
```

## Future Filters

See `docs/todo/` for planned filter types:

- **ExpansionFilter**: Word expansion hooks
- **EnvFilter**: Variable mutation hooks
- **RedirectionFilter**: File open hooks
- **IoFilter**: Per-I/O operation hooks

## Future Considerations

The following considerations apply to planned filter types and may influence the current design:

### Performance-Critical Filter Paths

**EnvFilter** (variable reads) and **IoFilter** (read/write operations) will be invoked extremely frequently:

- Every `$variable` expansion triggers EnvFilter
- Every `echo`, `printf`, or pipe read/write triggers IoFilter
- The current design clones filters before invocation; this is zero-cost for ZSTs but has `Arc` overhead for stateful filters
- Future optimization may introduce `&self` reference-based invocation for read-only filter operations (see `docs/todo/filter-clone-optimization.md`)

### Zero-Copy Data Handling

**IoFilter** will need to handle data buffers efficiently:

- The current `Cow<'a, [u8]>` pattern (used in params) supports both borrowed and owned data
- Filters that only observe data should avoid cloning
- Filters that transform data can return owned variants
- Consider providing `Bytes` or similar zero-copy types for large data

### Synchronous vs Asynchronous Hooks

Current filters are async-only. Future high-frequency filters may benefit from sync paths:

- EnvFilter read hooks could be synchronous for common cases
- IoFilter sync hooks could reduce overhead for small I/O operations
- May require trait-level distinction or separate hook methods

### Hook Granularity

Future filters face granularity tradeoffs:

- **Coarse**: Single `pre_expand` hook for all expansion types (simpler, less control)
- **Fine**: Separate hooks per expansion type (complex, precise control)
- Current `CmdExecFilter` uses fine granularity (`pre_simple_cmd` + `pre_external_cmd`)
- Recommendation: Start fine-grained, compose via `FilterStack` for coarser needs

