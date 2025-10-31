# How to upgrade MSRV

This document outlines the process for upgrading the Minimum Supported Rust Version (MSRV) for the `brush` project.

## Overview

Before upgrading MSRV, review the [MSRV Policy](../reference/msrv-policy.md) to ensure the update aligns with project guidelines.

## Process

### 1. Find all MSRV references

Search for the current MSRV version throughout the codebase:

```bash
grep -r "<current-version>" .
```

Typically, MSRV is specified in:
- `Cargo.toml` (workspace `rust-version` field)
- `.github/workflows/ci.yaml` (CI test matrix)
- `.github/copilot-instructions.md` (GitHub Copilot instructions)

### 2. Update MSRV references

Update all occurrences to the new version:

- **`Cargo.toml`**: Update the `rust-version` field under `[workspace.package]`
- **`.github/workflows/ci.yaml`**: Update the version in the test matrix

### 3. Verify the build

Test that the project builds successfully with the updated MSRV:

```bash
cargo check --workspace
```

### 4. Run tests

Verify that tests pass with the new MSRV:

```bash
cargo test --workspace
```

### 5. Run static checks

After upgrading MSRV, it's important to rerun all static checks, as newer Rust versions may introduce new lints and clippy warnings that weren't present in the previous MSRV. These warnings need to be resolved to maintain code quality.

Run clippy with all warnings treated as errors:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Also run other static checks such as formatting:

```bash
cargo fmt --all -- --check
```

**Note**: Upgrading MSRV often enables new clippy lints (particularly in nursery categories) that may flag code patterns that were previously acceptable. Review and fix these warnings, as they often suggest improvements like adding `const` to functions or other optimizations that are newly available in the updated Rust version.

### 6. Update documentation

When merging the MSRV update:
- Call out the update in an appropriate Conventional Commit commit description
- Include justification for the change in the release notes
