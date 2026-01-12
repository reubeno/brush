# Packaging brush for Distribution

This document provides guidance to package maintainers on how to
package brush for distribution.

*Some of this document's structure and content are based on
[Ghostty's PACKAGING.md](https://github.com/ghostty-org/ghostty/blob/main/PACKAGING.md) 
(MIT license), used with much appreciation.*

> [!IMPORTANT]
>
> This document is only accurate for the brush source alongside it.
> **Do not use this document for older or newer versions of brush!** If
> you are reading this document in a different version of brush, please
> find the `PACKAGING.md` file alongside that version.

## Source Availability

brush source is available from the following locations:

- **GitHub releases**: <https://github.com/reubeno/brush/releases>
  - Each release includes SHA-256 and SHA-512 checksums for verification
- **crates.io**: <https://crates.io/crates/brush-shell>

For reproducibility, we recommend using tagged releases from GitHub or
published versions from crates.io.

## Rust Version

brush requires a specific minimum Rust version to build. The authoritative
source for this requirement is the `rust-version` field in the workspace
[Cargo.toml](Cargo.toml).

At the time of writing, brush requires **Rust 1.87.0** or later and uses the
**Rust 2024 edition**.

## Building brush

The following is a standard example of how to build brush for system packages:

```sh
cargo build --release --locked --package brush-shell
```

The resulting binary will be at `target/release/brush`.

### Build Flags

The workspace is configured with the following release profile optimizations
(defined in [Cargo.toml](Cargo.toml)):

| Setting | Value | Description |
|---------|-------|-------------|
| `strip` | `true` | Debug symbols are stripped from the binary |
| `lto` | `"fat"` | Full link-time optimization for smaller/faster binaries |
| `codegen-units` | `1` | Single codegen unit for better optimization |
| `panic` | `"abort"` | Abort on panic (smaller binary) |

These settings are automatically applied when building with `--release`.

## Build Options

brush supports several Cargo features that can be enabled or disabled:

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | Yes | Enables `basic`, `reedline`, and `minimal` features |
| `basic` | Yes | Basic interactive features |
| `reedline` | Yes | Reedline-based line editing support |
| `minimal` | Yes | Primitive line editing support (used by integration tests) |
| `experimental` | No | Experimental features (not recommended for packaging) |
| `experimental-builtins` | No | Experimental builtin commands (not recommended for packaging) |

For most distributions, building with default features is recommended:

```sh
cargo build --release --locked --package brush
```

For minimal builds (e.g., embedded systems):

```sh
cargo build --release --locked --package brush --no-default-features --features minimal
```

## Verification

After building, you can verify the binary is functional using the smoke test:

```sh
cargo xtask test smoke --release
```

This runs basic sanity checks:
1. `brush --version` - Verifies the binary starts and reports its version
2. `brush -c 'echo ok'` - Verifies basic command execution works

For more comprehensive testing:

```sh
cargo xtask test integration --release
```

This runs the full integration test suite, including compatibility tests
against bash behavior.

## Generated Artifacts

brush provides several generated artifacts that packages may want to include:

### Shell Completion Scripts

Generate completion scripts for various shells:

```sh
# Bash completions (output to stdout)
cargo xtask gen completion bash > brush.bash

# Zsh completions
cargo xtask gen completion zsh > _brush

# Fish completions
cargo xtask gen completion fish > brush.fish
```

Typical installation paths:
- **Bash**: `/usr/share/bash-completion/completions/brush`
- **Zsh**: `/usr/share/zsh/site-functions/_brush`
- **Fish**: `/usr/share/fish/vendor_completions.d/brush.fish`

### Man Pages

Generate man pages:

```sh
cargo xtask gen docs man --output-dir /path/to/man/man1/
```

This generates `brush.1` in the specified directory.

### Third-Party License Notices

Generate an HTML file containing license information for all dependencies:

```sh
cargo xtask gen licenses --out THIRD_PARTY_LICENSES.html
```

This requires `cargo-about` to be installed:

```sh
cargo install cargo-about
```

## Supported Platforms

### Primary Platforms

These platforms are fully supported and tested in CI:

| Target | Notes |
|--------|-------|
| `x86_64-unknown-linux-gnu` | Linux x86_64 with glibc |
| `x86_64-unknown-linux-musl` | Linux x86_64 with musl (static) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 with glibc |
| `aarch64-unknown-linux-musl` | Linux ARM64 with musl (static) |
| `x86_64-apple-darwin` | macOS Intel |
| `aarch64-apple-darwin` | macOS Apple Silicon |

### Experimental Platforms

These platforms build but are not fully tested:

| Target | Notes |
|--------|-------|
| `x86_64-pc-windows-gnu` | Windows (experimental) |
| `wasm32-unknown-unknown` | WebAssembly (requires `--no-default-features --features minimal`) |
| `wasm32-wasip2` | WASI Preview 2 (requires `--no-default-features --features minimal`) |

### OS Compatibility

The official release binaries are built on Ubuntu 22.04 to ensure broad
glibc compatibility. When building for distribution, consider your target
users' glibc versions.

CI runs compatibility tests on:
- Arch Linux
- Debian (testing)
- Fedora
- NixOS
- openSUSE Tumbleweed

## Existing Packages

brush is already packaged for several distributions:

- **Arch Linux**: `pacman -S brush` (official extra repository)
- **Homebrew**: `brew install brush`
- **Nix**: `nix run 'github:NixOS/nixpkgs/nixpkgs-unstable#brush'`
- **cargo-binstall**: `cargo binstall brush-shell`

If you're packaging brush for a new distribution, feel free to open a PR
to add it to this list and the main README.

## Contact

If you have questions about packaging brush, please:

1. Open an issue on GitHub: <https://github.com/reubeno/brush/issues>
2. Check existing packaging-related issues and discussions

---

