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
| `authorize_external_cmd` | Final external spawn or `exec` request | Immutable allow/error decision after every rewrite |
| `post_external_cmd` | After external spawn | Result transformation |

Security policies should authorize in `authorize_external_cmd`. Every member of
a `FilterStack` sees the same final immutable command after all pre-filters have
run. The runtime executes that command directly: its program, arguments,
explicit environment, and working directory are not reconstructed afterward.
`original_command()` retains the spelling before path resolution;
`command.program()` is the executable and `command.argv0()` is the requested
argument zero. Argument-zero overrides use native platform support (Unix).

The `exec` builtin uses the same pre-filter and final authorization phases.
A successful `exec` replaces the process image, so no returning post-hook runs.
Embedders that must keep a worker protocol alive should omit that builtin.
Short-circuits still clean up temporary command environment scopes.

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
                 │  authorize_external_cmd ──[Error]──► Refuse
                 │      │ [Ok]
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

### FileOpenFilter

`pre_open_file` synchronously authorizes shell-originated file opens. Its
`FileOpenParams` provide the shell, original path spelling, resolved path, and
typed `FileOpenAccess::{Read, Write, ReadWrite}`. The call site supplies access;
it is not inferred from `OpenOptions` debug output. Redirections, sourced
scripts, and history reads pass through the hook before native opens, special
device handling, or `/dev/fd` alias resolution.

Returning an error refuses the open. A policy that must stop the current run
can mark that error with `into_terminating()`. The default `NoOpFileOpenFilter`
preserves ordinary behavior, and `FilterStack` checks policies in order until
one refuses. This hook does not mediate filesystem operations inside external
processes or direct filesystem operations outside `Shell::open_file` (for
example, writing the history store); the embedder must enforce that boundary
separately.

Terminating errors stop the current evaluation through loops, functions,
subshells, and sourced/evaluated scripts. Process substitutions run as separate
tasks: their denial prevents the child operation, but their result is not joined
into the parent status. Embedders must not treat a successful parent status as
proof that every asynchronous substitution succeeded.

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
use brush_core::filter::{NoOpSourceFilter, NoOpFileOpenFilter};

#[derive(Clone, Default)]
struct MyExtensions {
    cmd_filter: MyCustomFilter,
}

impl ShellExtensions for MyExtensions {
    type ErrorFormatter = DefaultErrorFormatter;
    type CmdExecFilter = MyCustomFilter;
    type SourceFilter = NoOpSourceFilter;
    type FileOpenFilter = NoOpFileOpenFilter;
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

## Policy Serialization

With the `serde` feature, shell snapshots preserve installed command, source,
and file-open policy values. A shell implements serialization only when those
policy types implement the corresponding serde traits. Unsupported policies
cannot be silently replaced with defaults. Deserializing a snapshot missing
any policy field fails; older snapshots without policy state must be rebuilt
with an explicit policy. Default no-op filters and serializable filter stacks
can round-trip normally.

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

Command and source filters are asynchronous; file-open policy is synchronous.
Future high-frequency filters may also benefit from sync paths:

- EnvFilter read hooks could be synchronous for common cases
- IoFilter sync hooks could reduce overhead for small I/O operations
- May require trait-level distinction or separate hook methods

### Hook Granularity

Future filters face granularity tradeoffs:

- **Coarse**: Single `pre_expand` hook for all expansion types (simpler, less control)
- **Fine**: Separate hooks per expansion type (complex, precise control)
- Current `CmdExecFilter` uses fine granularity (`pre_simple_cmd` + `pre_external_cmd`)
- Recommendation: Start fine-grained, compose via `FilterStack` for coarser needs

## Explicit descriptor delegation

On Unix, `ExternalCommand` may carry `DelegatedFd` values: an already-owned
source descriptor and an explicit child target above stderr. Filters can inspect
`delegated_fds()` during final authorization. The runtime merges these with the
shell's existing descriptor mappings and rejects duplicate targets or collisions
with redirections before spawning. Descriptor delegation does not change the
command's arguments, environment, working directory, or standard streams.
`into_std_command()` is fallible so descriptor-duplication errors are reported.
Admission normalizes the owned source to close-on-exec; unrelated children do
not inherit a retained source descriptor. Filter code correlating capabilities
by their source descriptor must inspect the admitted `DelegatedFd`, since its
descriptor number may differ from the caller's original source.

The static `external_cmd_spawned` hook receives the exact authorized command and
the spawned process ID before Brush exposes the spawn result. This allows an
embedder to register a specific process against its delegated capability,
without relying on pipeline ordering or a shared last-command slot. The child
may already be running: a broker must withhold replies until registration is
acknowledged. If the hook refuses registration, Brush terminates and reaps the
child and propagates a terminating error. Failed spawns and successful `exec`
replacement do not call this hook. Cancellation while registration is pending
terminates the child even when ordinary shell children would survive a dropped
waiter. Successful registration restores the configured lifecycle policy;
worker embedders should enable `kill_external_commands_on_drop` for subsequent
execution as well.

A private broker can use this mechanism without granting a new filesystem path
or network destination. Possession of a transport endpoint is not permission to
sign arbitrary data: the parent must still validate invocation identity,
canonical payloads, sequence, and actual transaction results. Windows requires
an equivalent owned-handle mapping through an explicit handle list; this Unix
API does not claim to provide that Windows transport.
