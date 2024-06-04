# How to run tests

To run all workspace tests:

```bash
cargo test --workspace
```

To run just integration tests:

```bash
cargo test --test integration_tests
```

To run a specific integration test case

```bash
cargo test --test integration_tests -- '<name of test case>'
```
