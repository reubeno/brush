# How to upgrade MSRV

This document outlines the process for upgrading the Minimum Supported Rust Version (MSRV) for the
`brush` project.

## Overview

Before upgrading, review the [MSRV Policy](../reference/msrv-policy.md). The workspace is split into
two tiers, and which one you are raising decides both the process and the bar you have to clear:

- **Library tier** (`brush-core`, `brush-parser`, `brush-builtins`, and the other embeddable
  crates): governed by the workspace-wide `rust-version`. Raising it reaches every downstream
  consumer, and the policy's age requirement applies.
- **Application tier** (`brush`, `brush-shell`, `brush-interactive`, `brush-fuzz`): each declares its
  own `rust-version`. Raise it only when a dependency forces it, in the same change that takes the
  dependency.

## Raising an application-tier crate's MSRV

This is the common case: a dependency of the interactive front end has moved.

### 1. Raise the crate and everything that depends on it

Set `rust-version` in the crate's own `Cargo.toml` to the version the dependency requires, and no
higher. Cargo rejects a package whose dependency declares a higher `rust-version` than it does, so
dependents have to move with it. In practice `brush-interactive` pulls `brush-shell`, which pulls
`brush`.

The manifests are the only place the number appears. Nothing else needs editing: CI derives the set
of higher-MSRV crates from `cargo metadata`.

### 2. Verify both CI legs locally

Run the MSRV leg against the MSRV toolchain, not whatever is active. Only the pinned toolchain
proves the library tier still compiles where it claims to:

```bash
# The workspace `rust-version`, read from the root manifest.
msrv=$(grep -m1 '^rust-version' Cargo.toml | cut -d'"' -f2)
rustup toolchain install "$msrv"

# What the stable leg runs: every crate, current toolchain.
cargo xtask check build

# What the MSRV leg runs; the higher-MSRV crates drop out automatically.
cargo "+$msrv" xtask check build --workspace-msrv
```

The second command prints the crates it skipped. If that list does not match what you just changed,
something is declared in the wrong place.

To see the other half of the split, ask the MSRV toolchain to build a crate you just raised. Cargo
should refuse by name:

```bash
cargo "+$msrv" check -p brush-shell
# error: rustc <workspace MSRV> is not supported by the following packages:
#   brush-interactive@<version> requires rustc <the value you just set>
```

### 3. Confirm the library tier did not move

```bash
cargo metadata --no-deps --format-version 1 |
  jq -r '.packages[] | "\(.name) \(.rust_version)"'
```

Every library-tier crate should still report the workspace value. If one moved, a dependency crossed
the tier boundary, and that is a policy decision rather than a mechanical update.

## Raising the workspace MSRV

### 1. Find all MSRV references

```bash
grep -r "<current-version>" .
```

The workspace MSRV is specified in:

- `Cargo.toml` (the `rust-version` field under `[workspace.package]`)
- `.github/workflows/ci.yaml` (the CI test matrix)
- `.github/copilot-instructions.md` (GitHub Copilot instructions)

Note that the `rust-version` values in the application-tier manifests are deliberately independent.
Leave them alone unless the workspace value has caught up with or passed them, in which case the
crate no longer needs its own declaration and should go back to `rust-version.workspace = true`.

### 2. Update MSRV references

Update all occurrences to the new version.

### 3. Verify the build

```bash
cargo +<new-version> xtask check build --workspace-msrv
```

Confirm the value the library tier will publish, which is what downstream consumers actually see.
`rust-version.workspace = true` is resolved when the crate is packaged, so read it back out of the
package rather than trusting the source manifest:

```bash
cargo package --no-verify -p brush-core
tar -xzOf target/package/brush-core-*.crate '*/Cargo.toml' | grep rust-version
```

### 4. Run tests

```bash
cargo +<new-version> test --workspace
```

### 5. Run static checks

After upgrading MSRV, it's important to rerun all static checks, as newer Rust versions may introduce
new lints and clippy warnings that weren't present in the previous MSRV. These warnings need to be
resolved to maintain code quality.

Run clippy with all warnings treated as errors:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Also run other static checks such as formatting:

```bash
cargo fmt --all -- --check
```

**Note**: Upgrading MSRV often enables new clippy lints (particularly in nursery categories) that may
flag code patterns that were previously acceptable. Review and fix these warnings, as they often
suggest improvements like adding `const` to functions or other optimizations that are newly available
in the updated Rust version.

### 6. Update documentation

When merging the MSRV update:

- Call out the update in an appropriate Conventional Commit commit description
- Include justification for the change in the release notes
