# Redirection Filter

## Status
Not implemented

## Overview
Add filtering hooks for file operations during I/O redirection, allowing observation, modification, or blocking of file access.

## Use Cases
- Security: restrict file access to specific paths
- Sandboxing: virtualize filesystem access
- Auditing: log all file operations
- Testing: mock file operations

## Proposed API

```rust
pub trait RedirectionFilter: Clone + Default + Send + Sync + 'static {
    /// Called before a file is opened for redirection.
    fn pre_open_file<'a, SE: ShellExtensions>(
        &self,
        params: OpenFileParams<'a, SE>,
    ) -> impl Future<Output = PreFilterResult<OpenFileParams<'a, SE>, OpenFileOutput>> + Send;

    /// Called after a file is opened.
    fn post_open_file(
        &self,
        result: OpenFileOutput,
    ) -> impl Future<Output = PostFilterResult<OpenFileOutput>> + Send;
}

pub struct OpenFileParams<'a, SE: ShellExtensions> {
    pub shell: &'a Shell<SE>,
    pub path: Cow<'a, Path>,
    pub mode: OpenMode,  // Read, Write, Append, ReadWrite
    pub fd: ShellFd,     // Target file descriptor
}
```

## Integration Points
- `brush-core/src/openfiles.rs` - file descriptor management
- Input redirection (`< file`)
- Output redirection (`> file`, `>> file`)
- Here-documents (`<< EOF`)
- Here-strings (`<<< string`)
- File descriptor duplication (`2>&1`)
- Process substitution (`<(cmd)`, `>(cmd)`)

## Considerations
- Performance impact on every redirection
- Handle special files (/dev/null, /dev/stdin, etc.)
- Pipe creation may need separate hook
- Network redirections (/dev/tcp, /dev/udp) if supported

## Related
- [io-filter.md](io-filter.md) - per-I/O operation filtering
