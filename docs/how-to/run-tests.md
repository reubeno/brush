# How to run tests

## Using xtask (Recommended)

The project provides `cargo xtask` commands for running tests:

```bash
# Run unit tests (fast tests excluding integration binaries)
cargo xtask test unit

# Run integration tests (all workspace tests including compat tests)
cargo xtask test integration

# Run tests with code coverage
cargo xtask test integration --coverage --coverage-output codecov.xml
```

## CI Workflows

For comprehensive validation, use the CI workflows:

```bash
# Quick inner-loop checks (~7s warm): fmt, build, lint, unit tests
cargo xtask ci quick

# Full pre-commit checks (~45s warm): quick + deps, schemas, integration tests
cargo xtask ci pre-commit
```

## Manual Approach (Alternate)

To run all workspace tests:

```bash
cargo test --workspace
```

To run just bash compatibility tests:

```bash
cargo test --test brush-compat-tests
```

To run a specific compatibility test case

```bash
cargo test --test brush-compat-tests -- '<name of test case>'
```
