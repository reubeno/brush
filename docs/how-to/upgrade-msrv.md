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

### 5. Update documentation

When merging the MSRV update:
- Call out the update in an appropriate Conventional Commit commit description
- Include justification for the change in the release notes
