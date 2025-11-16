# Agent Development Guide for `brush`

This guide helps AI agents work efficiently on the `brush` codebase by providing essential context about architecture, patterns, and development workflows.

## 1. Architecture Overview & Navigation

### Project Structure
The brush project is organized into several key crates:

- **`brush-core/`**: Core shell functionality, builtins, and runtime
- **`brush-parser/`**: Shell script parsing (AST generation)
- **`brush-builtins/`**: Implementation of shell builtins (e.g., echo, cd)
- **`brush-interactive/`**: Interactive shell interfaces (readline, etc.)
- **`brush-shell/`**: Main CLI application and entry point

### Key Files & Entry Points

**Critical files to understand first:**
- `brush-core/src/shell.rs` - Main `Shell` struct and creation logic
- `brush-core/src/lib.rs` - Public API exports
- `brush-shell/src/main.rs` - CLI application entry point

**Architecture patterns:**
- Shell instances are created via `Shell::builder()`
- The project uses builder patterns for type-safe configuration
- We try to keep platform-specific code in `brush-core` under the `sys` module
- Follows Rust 2024 edition standards

### Module Dependencies
```
brush-shell → brush-interactive → brush-core → brush-parser
            ↘ brush-builtins ↗
```

## 2. Testing Strategy

### Test Execution Priority

**Recommended development workflow:**

#### Inner Loop (Fast Iteration)
1. **Quick validation**: `cargo check --package <changed-package>` - Fast syntax/type checking
2. **Correctness validation**: `cargo test --package <changed-package>` - Target specific crates for faster feedback

#### Outer Loop (Comprehensive Testing)  
3. **Compatibility tests**: `cargo test --test brush-compat-tests` - Bash compatibility validation
4. **Full workspace tests**: `cargo test --workspace` - Complete test suite

#### Pre-Finish Quality Validation
Before considering work complete, run these validation steps:
- **Compatibility tests**: `cargo test --test brush-compat-tests`
- **Linting**: `cargo clippy` 
- **Formatting**: `cargo fmt --check`
- **Security/License audit**: `cargo deny check all`
- **Full test suite**: `cargo test --workspace`

**When tests fail:**
- Focus on failures in the area you changed first
- Compatibility test failures often indicate shell behavior changes
- Check if new functionality needs corresponding test cases
- Format/clippy failures should be fixed before proceeding

**Common pitfalls:**
- **Test scope mistakes**: Running full test suite too early instead of targeting specific areas first
- **Skipping test-driven development**: Add tests that specify desired behavior before implementing

**Test-driven development approach:**
- When possible, write tests first that specify the desired behavior
- Use unit tests for logic changes, compatibility tests for shell behavior changes
- Use these tests as validation that your implementation is working correctly

**Pro tip**: For specific compatibility test cases, use:
```bash
cargo test --test brush-compat-tests -- '<name of test case>'
```

**Fast iteration strategies:**
- Target specific crates: `cargo test --package <changed-package>`
- Target specific test cases: `cargo test <test-name>` or `cargo test --test <test-file>`
- Requires knowledge of which tests best exercise the code being changed

**Testing approach:**
- Follow good software engineering practice: start by validating the specific area being changed, then iteratively move to incrementally broader sets of tests

### Test Organization

**Testing expectations for new public APIs:**
- Unit tests are expected if feasible
- Examples are nice to have and worthwhile for sufficiently critical APIs

**Test patterns and conventions:**
- **Compatibility tests**: For any compatibility-related fixes, it's critical to add new test cases to the compat tests (see docs/how-to/run-tests.md and section 3 for when breaking changes apply)

**Test categories:**
- Unit tests: In `src/` files with `#[cfg(test)]`
- Integration tests: In `tests/` directories
- Examples: In `examples/` directories (must be runnable)
- Shell script tests: YAML-based test cases in `brush-shell/tests/cases/`

### Performance Testing

**Performance regression testing:**
- Not a chief concern for most changes
- For performance-specific work, benchmarks are available (see docs/how-to/run-benchmarks.md)
- Performance sensitivity will be identified in the initial brief if relevant

## 3. Breaking Changes & Compatibility

### API Stability Guidelines

**Breaking change policy:**
- Non-backwards compatible changes to public APIs are considered breaking
- Breaking changes are still in consideration, but need to be highlighted and carefully reviewed
- Any APIs exported from crates are considered public because all of the crates are published to crates.io

**Adding new fields to public structs:**
- New optional fields are fine to add as long as the struct implements the Default trait and as long as the defaulted value is a sensible one

### Dependency Impact
When changing public APIs in `brush-core` (see section 3 for breaking change policy):
1. Check `brush-shell/src/main.rs` for struct initialization sites
2. Check `brush-interactive/` for any usage

## 4. Documentation & Examples Standards

### Documentation Requirements

**Rustdoc documentation standards:**
- At minimum we must have good rustdoc documentation for exported types, functions, traits, etc. as well as on all exported modules and crates
- Documentation for internal components should be a best-effort, nice to have thing

**Examples for new features:**
- Unless explicitly requested, only major feature additions warrant an example.

**Documentation style:**
- Follow general best practices for Rust

### Example Standards
Examples should:
- Be self-contained and runnable with `cargo run --package brush-core --example <name>`
- Include comprehensive error handling
- Demonstrate both basic and advanced usage patterns
- Include output examples in comments when helpful

## 5. Build & Release Process

### Development Tools
The project uses several tools for code quality:

**Standard development workflow:**
- Mostly standard cargo commands for now (e.g., check, test, build, run, clippy)
- You may need to reverse engineer some of the args looking at CI checks in .github/*.yml

**Command frequency guidelines:**
- **Frequent (inner loop)**: `cargo check`, `cargo test --package <pkg>`
- **Regular (before commits)**: `cargo fmt`, `cargo clippy`
- **Occasional (outer loop)**: `cargo test --workspace`, `cargo test --test brush-compat-tests`
- **Rare (pre-finish only)**: `cargo deny check`

**Pre-commit validation:**
- Always run `cargo fmt` and `cargo clippy` before committing

**Outer loop validation:**
- `cargo deny check all` should pass (security/license auditing) - not for frequent use during development

## 6. Performance & Error Handling Patterns

### Error Handling

**Error handling patterns:**
- `thiserror` is used for implementing crate-specific errors
- Use `anyhow` only in tests

**Logging and tracing patterns:**
- Use `tracing` for debug logging with predefined categories
- Categories are defined in `trace_categories.rs` modules (e.g., `COMMANDS`, `COMPLETION`, `EXPANSION`, `FUNCTIONS`, `INPUT`, `JOBS`, `PARSE`, `PATTERN`, `UNIMPLEMENTED`)
- Usage pattern: `tracing::debug!(target: trace_categories::CATEGORY_NAME, "message")`
- Example: `tracing::debug!(target: trace_categories::JOBS, "Polling job {} for completion...", job_id)`

### Performance Considerations

**Clone vs references:**
- Avoid cloning by default, no reason to make extra copies
- Only use cloning when you really must capture a separate copy for async safety or similarly important reasons

---

## Quick Reference Checklist

When making changes to brush:

### Before Starting
- [ ] Understand which crate(s) are affected
- [ ] Check if changes might break dependent crates
- [ ] Identify relevant test files and examples

### During Development  
- [ ] Run `cargo check` frequently during development
- [ ] Test changes with package-specific tests first (see section 2 for testing workflow)
- [ ] Update dependent crate usage if needed (see section 3 for compatibility considerations)
- [ ] Add/update examples for major feature additions only (see section 4)

### Before Committing
- [ ] Run full test suite: `cargo test` (see section 2 for complete testing workflow)
- [ ] Format code: `cargo fmt` (see section 5 for tool details)
- [ ] Check linting: `cargo clippy`
- [ ] Use conventional commit format

### Documentation
- [ ] Add rustdoc to exported APIs (see section 4 for documentation standards)
- [ ] Include working examples for major features only
- [ ] Update this guide if new patterns emerge
