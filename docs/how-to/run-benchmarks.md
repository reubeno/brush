# How to run benchmarks

To run performance benchmarks:

```bash
cargo bench --workspace
```

## Collecting flamegraphs

To collect flamegraphs from performance benchmarks (running for 10 seconds):

```bash
cargo bench --workspace -- --profile-time 10
```

The flamegraphs will be created as `.svg` files and placed under `target/criterion/<benchmark_name>/profile`.
