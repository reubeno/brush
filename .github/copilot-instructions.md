# GitHub Copilot Coding Agent Instructions for brush

## Project Overview

**brush** (Bourne Rusty Shell) is a POSIX- and bash-compatible shell implemented in Rust. It's a multi-crate workspace (~60K lines of Rust code) targeting Linux, macOS, and WSL, with experimental Windows and WASM support. The project emphasizes compatibility testing against bash as an oracle.

**Key Stats:** Rust 2024 edition, MSRV 1.87.0, 5 main crates, 675+ compatibility test cases, published to crates.io.

## Critical: Read AGENTS.md First

**BEFORE making any changes, read `/AGENTS.md`.** It contains detailed architecture patterns, testing workflows, and development guidelines specific to this project. The information below supplements (not replaces) AGENTS.md.

## Code Review Checklist

When reviewing PRs, verify:

- [ ] **Documentation**: All exported APIs have rustdoc comments (missing docs = CI failure)
- [ ] **Forbidden patterns**: No `panic`, `unwrap_in_result`, `expect_used`, or `todo` (all denied by clippy)
- [ ] **Error handling**: Uses `thiserror` for crate errors; `anyhow` only in tests
- [ ] **Logging**: Uses `tracing::debug!(target: trace_categories::CATEGORY, "msg")` pattern
- [ ] **Testing**: Compatibility fixes include YAML test cases in `brush-shell/tests/cases/`
- [ ] **Testing**: Builtin changes have tests in `brush-shell/tests/cases/builtin/`
- [ ] **Testing**: Unit tests expected for new public APIs (when feasible) (see AGENTS.md section 2)
- [ ] **Platform code**: Platform-specific code is in `brush-core/src/sys/` modules
- [ ] **Breaking changes**: Public API changes are clearly highlighted and documented
- [ ] **Builder pattern**: Configuration uses builder pattern (see `Shell::builder()`)
- [ ] **Code quality**: Passes `cargo fmt --check` and `cargo clippy` without warnings
- [ ] **Commit format**: Follows [Conventional Commits](https://www.conventionalcommits.org/) (feat:, fix:, docs:, test:)
- [ ] **Dependencies**: No unnecessary cloning (use references when possible)
- [ ] **Cross-platform**: Uses appropriate `cfg(unix)`, `cfg(windows)`, `cfg(target_family = "wasm")`

## Workspace Structure

```
brush/
├── brush-shell/        # CLI application & main entry point
├── brush-interactive/  # Interactive shell (readline, completion)
├── brush-core/         # Core shell runtime & builtins
├── brush-builtins/     # Shell builtin implementations
├── brush-parser/       # AST generation & parsing
├── xtask/             # Build automation tasks
└── docs/              # Diátaxis-structured documentation
```

**Dependency flow:** brush-shell → brush-interactive → brush-core → brush-parser
                                  ↘ brush-builtins ↗

## Build & Validation Commands

### Using xtask (Recommended)

The project provides a `cargo xtask` command that centralizes common development tasks. This is the recommended approach for running checks and tests.

#### Quick Development Cycle

```bash
# Run all pre-commit checks (fmt, lint, deps, build, schemas, tests)
cargo xtask ci pre-commit

# Run with --continue-on-error to see all failures at once
cargo xtask ci pre-commit -k

# Add -v for verbose output showing exact commands being run
cargo xtask -v ci pre-commit
```

#### Individual Checks

```bash
# Format check
cargo xtask check fmt

# Lint check (clippy)
cargo xtask check lint

# Dependency check (cargo-deny)
cargo xtask check deps

# Build check
cargo xtask check build

# Schema check (regenerates and diffs)
cargo xtask check schemas
```

#### Running Tests

```bash
# Run all tests (unit + compat)
cargo xtask test all

# Run unit tests only
cargo xtask test unit

# Run compatibility tests only
cargo xtask test compat

# Run tests with coverage
cargo xtask test coverage --output codecov.xml
```

### Manual Approach (Alternate)

For finer-grained control or when xtask isn't available:

#### Quick Development Cycle (Use These Frequently)

```bash
# Fast syntax/type checking (< 5 seconds)
cargo check --workspace

# Package-specific checking (even faster)
cargo check --package brush-core

# Format code (ALWAYS run before committing)
cargo fmt --all

# Lint code (ALWAYS run before committing)
cargo clippy --workspace --all-features --all-targets

# Run package-specific tests (fast iteration)
cargo test --package brush-parser
cargo test --package brush-core
```

**Note:** `cargo fmt --check` may show warnings about unstable rustfmt features (`wrap_comments`, `comment_width`) on stable Rust. These are harmless and expected.

### Comprehensive Testing Workflow

Follow this **exact order** for efficient testing:

1. **Inner loop** (during development):
   ```bash
   cargo check --package <changed-package>
   cargo test --package <changed-package>
   ```

2. **Compatibility tests** (critical for shell behavior):
   ```bash
   cargo test --test brush-compat-tests
   
   # Run specific test case:
   cargo test --test brush-compat-tests -- 'builtin/echo'
   ```

3. **Full workspace tests** (before considering work complete):
   ```bash
   cargo test --workspace
   ```

**Test timing:** Package tests: 3-20 seconds. Compat tests: ~18 seconds build + test time. Full workspace: several minutes.

### Pre-Commit Validation (Before Every Commit)

**Recommended:** Run the xtask pre-commit workflow:

```bash
cargo xtask ci pre-commit
```

**Manual approach:** Run these before every commit:

```bash
cargo fmt --check --all
cargo clippy --workspace --all-features --all-targets
```

### Pre-PR Validation (Before Opening Pull Request)

**Recommended:** Run pre-commit checks which includes full test suite:

```bash
cargo xtask ci pre-commit
```

**Manual approach:** In addition to pre-commit checks, also run:

```bash
cargo test --workspace
```

### Pre-Finish Quality Gates (Run Before Completing Task)

**Recommended:** Run the xtask pre-commit workflow which covers all essential checks:

```bash
cargo xtask ci pre-commit
```

**Manual approach:**

```bash
cargo test --test brush-compat-tests
cargo deny check all       # License/security audit (run LAST, not frequently)
cargo clippy --workspace --all-features --all-targets
cargo fmt --check --all
cargo test --workspace
```

**Timing note:** `cargo deny check all` takes ~1-5 seconds. Only run as final validation step.

### Build Variants

```bash
# Standard debug build
cargo build

# Release build (takes ~2+ minutes, avoid during iteration)
cargo build --release

# Check all targets and features
cargo check --all-features --all-targets
```

## Testing Philosophy

**Test-driven approach:** When fixing bugs or adding features, write test cases in `brush-shell/tests/cases/*.yaml` BEFORE implementation. Use these to validate your changes.

**Integration test structure:** Tests are YAML-based, run shell commands, compare stdout/stderr/exit codes against bash oracle. See `docs/reference/integration-testing.md` and AGENTS.md section 2 for detailed testing strategy.

**Test categories:**
- Unit tests: In-file with `#[cfg(test)]`
- Integration tests: `brush-shell/tests/` directory
- Compatibility tests: YAML cases in `brush-shell/tests/cases/`
- Benchmarks: `brush-shell/benches/` and crate-level `benches/`

## Common Pitfalls & Solutions

### ❌ Don't Do This
- Run full test suite on every change (too slow)
- Skip `cargo fmt` and `cargo clippy` before committing
- Use `cargo deny check` during development iteration
- Clone values unnecessarily (use references)
- Add breaking changes to public APIs without highlighting them
- Forget to add compat test cases for compatibility fixes

### ✅ Do This
- Target specific packages/tests during development
- Run fmt/clippy before every commit
- Follow builder pattern for configuration (see `Shell::builder()`)
- Keep platform-specific code in `brush-core/src/sys/`
- Document all exported APIs with rustdoc
- Use `tracing::debug!(target: trace_categories::CATEGORY, "msg")` for logging
- Add test cases to `brush-shell/tests/cases/` for compatibility changes

## Error Handling & Logging

```rust
// Use thiserror for crate-specific errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError { ... }

// Use anyhow ONLY in tests
#[cfg(test)]
use anyhow::Result;

// Logging with trace categories
use crate::trace_categories;
tracing::debug!(target: trace_categories::COMMANDS, "executing: {}", cmd);
```

**Available trace categories:** COMMANDS, COMPLETION, EXPANSION, FUNCTIONS, INPUT, JOBS, PARSE, PATTERN, UNIMPLEMENTED

## Linting Configuration

The project uses **extremely strict** linting (workspace-level in `Cargo.toml`):
- All Rust warnings denied
- All clippy warnings denied (pedantic, cargo, nursery, perf)
- `expect_used`, `panic`, `todo`, `unwrap_in_result` are **forbidden**
- Missing docs on exported items are errors

**Your code MUST pass `cargo clippy` without warnings.**

## Cross-Platform Considerations

- Primary targets: Linux (x86_64, aarch64), macOS (aarch64)
- Secondary: Windows (x86_64), WASM (wasm32-unknown-unknown, wasm32-wasip2)
- Platform-specific code goes in `brush-core/src/sys/` modules
- Use `cfg(unix)`, `cfg(windows)`, `cfg(target_family = "wasm")` appropriately
- See `.cargo/config.toml` for target-specific configurations

## CI Pipeline (What Will Run on Your PR)

GitHub Actions runs these checks (from `.github/workflows/ci.yaml`):

1. **Build** on multiple platforms (x86_64/aarch64 Linux, macOS, Windows, WASM)
2. **Tests** on Linux x86_64, Linux aarch64, macOS
3. **Static checks** (format, clippy, cargo-deny) on stable + MSRV (1.87.0)
4. **Compatibility tests** with bash as oracle
5. **Code coverage** reports (70% overall threshold, no 5% negative delta)
6. **External test suites** (bash-completion test suite)
7. **OS compatibility** (Arch, Debian, Fedora, NixOS, openSUSE)
8. **Benchmarks** (performance regression detection on PRs)
9. **Public API analysis** (breaking change detection)

**All of these must pass for PR to merge.**

## Making Changes

### Editing Core Shell Behavior
1. Check `brush-core/src/shell.rs` for `Shell` struct
2. Use `Shell::builder()` for construction
3. Update `brush-shell/src/main.rs` if CLI changes needed

### Adding/Modifying Builtins
1. Edit files in `brush-builtins/src/`
2. Register in `brush-builtins/src/factory.rs`
3. Add test cases in `brush-shell/tests/cases/builtin/`

### Parser Changes
1. Modify `brush-parser/src/`
2. Update AST definitions
3. Test with `cargo test --package brush-parser`

### Breaking Changes Policy
- Avoid breaking public APIs (all crate exports are public)
- If unavoidable, highlight clearly and document thoroughly
- New optional fields on public structs are OK if struct implements `Default`
- See AGENTS.md section 3 for complete breaking change policy

## Performance & Benchmarking

```bash
# Run benchmarks (using xtask)
cargo xtask analyze bench

# Run benchmarks with output file
cargo xtask analyze bench --output benchmarks.txt

# Run benchmarks (manual)
cargo bench --workspace --benches

# Collect flamegraphs (10 second profiling)
cargo bench --workspace --benches -- --profile-time 10
# Output: target/criterion/<benchmark_name>/profile/*.svg
```

**Note:** Performance regression testing runs automatically on PRs. Don't worry about it unless working on performance-specific features.

## Documentation Standards

**Rustdoc:** REQUIRED for all exported types, functions, traits, modules. Missing docs = CI failure.
**Examples:** Only needed for major feature additions.
**Style:** Follow Rust documentation best practices.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):
```
feat: add support for X
fix: correct behavior of Y
docs: update Z documentation
test: add test cases for W
```

## AI-Assisted Contributions

If using AI assistance significantly, add to PR description or commit message:
```
Assisted-by: GitHub Copilot
```

## When Something Fails

1. **Test failures:** Focus on affected area first, check if new tests are needed
2. **Format/clippy failures:** Fix immediately before proceeding
3. **Compat test failures:** Indicates shell behavior change, may need test updates
4. **Build failures:** Check dependencies, verify Rust version (1.87.0+)
5. **Timeout issues:** Build from scratch can take 2+ minutes

## Quick Reference

| Command | When | Time |
|---------|------|------|
| `cargo xtask ci pre-commit` | Before commit (comprehensive) | 2-5min |
| `cargo xtask ci pre-commit -k` | See all failures at once | 2-5min |
| `cargo check` | Constantly during dev | ~3-5s |
| `cargo test --package X` | After each change | 3-20s |
| `cargo xtask test compat` | Before commit | ~18s |
| `cargo xtask check fmt` | Before every commit | <1s |
| `cargo xtask check lint` | Before commit | ~5-10s |
| `cargo xtask test all` | Before PR/finish | 2-5min |
| `cargo xtask check deps` | Final validation only | ~1-5s |

## Trust These Instructions

Only search the codebase if information here or in AGENTS.md is incomplete, contradictory, or proven incorrect. These instructions are validated against the actual working repository.
