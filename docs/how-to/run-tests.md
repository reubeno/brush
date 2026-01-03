# How to run tests

## Using xtask (Recommended)

The project provides `cargo xtask` commands for running tests:

```bash
# Run all tests (unit + compatibility)
cargo xtask test all

# Run unit tests only
cargo xtask test unit

# Run compatibility tests only
cargo xtask test compat

# Run tests with code coverage
cargo xtask test coverage --output codecov.xml
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
