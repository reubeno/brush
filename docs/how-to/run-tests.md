# How to run tests

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
