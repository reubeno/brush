# I/O Filter

## Status
Not implemented

## Overview
Add filtering hooks for individual I/O operations, allowing observation or transformation of data flowing through shell file descriptors.

## Use Cases
- Logging: capture all shell I/O for debugging or auditing
- Transformation: modify output (e.g., add timestamps, redact sensitive data)
- Testing: inject test input or capture output programmatically
- Sandboxing: rate-limit or block I/O to specific descriptors

## Proposed API

```rust
pub trait IoFilter: Clone + Default + Send + Sync + 'static {
    /// Called before data is written to a file descriptor.
    fn pre_write<'a, SE: ShellExtensions>(
        &self,
        params: WriteParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<WriteParams<'a, SE>, WriteOutput>> + Send;

    /// Called after data is written.
    fn post_write(
        &self,
        result: WriteOutput,
    ) -> impl Future<Output = PostFilterResult<WriteOutput>> + Send;

    /// Called before data is read from a file descriptor.
    fn pre_read<'a, SE: ShellExtensions>(
        &self,
        params: ReadParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<ReadParams<'a, SE>, ReadOutput>> + Send;

    /// Called after data is read.
    fn post_read(
        &self,
        result: ReadOutput,
    ) -> impl Future<Output = PostFilterResult<ReadOutput>> + Send;
}

pub struct WriteParams<'a, SE: ShellExtensions> {
    pub shell: &'a Shell<SE>,
    pub fd: ShellFd,
    pub data: Cow<'a, [u8]>,
}
```

## Integration Points
- `brush-core/src/openfiles.rs` - OpenFile wrapper
- All write operations (echo, printf, command output)
- All read operations (read builtin, command input)
- Builtin command I/O

## Considerations
- **Performance critical**: I/O happens constantly
- May need to be opt-in or have fast-path for no-op
- Buffer management and ownership
- Async I/O handling
- Binary vs text data
- Partial reads/writes

## Complexity Warning
This is the most performance-sensitive filter type. Implementation should carefully consider:
- Zero-copy where possible
- Avoiding allocations in the hot path
- Potential for buffering/batching

## Related
- [redirection-filter.md](redirection-filter.md) - file open filtering
