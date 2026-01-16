# How to run benchmarks

## Using xtask (Recommended)

The project provides `cargo xtask` commands for running benchmarks:

```bash
# Run benchmarks
cargo xtask analyze bench

# Run benchmarks and save output to a file
cargo xtask analyze bench --output benchmarks.txt
```

## Manual Approach (Alternate)

To run performance benchmarks:

```bash
cargo bench --workspace --benches
```

## Collecting flamegraphs

To collect flamegraphs from performance benchmarks (running for 10 seconds):

```bash
cargo bench --workspace --benches -- --profile-time 10
```

The flamegraphs will be created as `.svg` files and placed under `target/criterion/<benchmark_name>/profile`.
