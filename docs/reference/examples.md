# Examples

Each crate that has something to demonstrate keeps runnable examples in its `examples/` directory. Run one with:

```bash
cargo run --package <crate> --example <name>
```

A few need a Cargo feature, noted below.

| Example | Crate | What it shows | Useful when |
|---|---|---|---|
| [`readme`](../../brush-core/examples/readme.rs) | `brush-core` | The shortest embedding: build a `Shell`, run a string of shell code, check the result. This is the snippet in the repository README; a test keeps the two identical. | You want the minimum needed to run shell code inside your own program. |
| [`call-func`](../../brush-core/examples/call-func.rs) | `brush-core` | Define a shell function from Rust, list the functions the shell knows, invoke one with arguments, and redirect its stdout using `ExecutionParameters`. | Your program calls into user-supplied shell code and needs control over its I/O. |
| [`custom-builtin`](../../brush-core/examples/custom-builtin.rs) | `brush-core` | A builtin implemented in Rust and registered alongside the standard ones: the `StatelessBuiltin` trait for a builtin parsed by hand, the `Builtin` trait for one with state of its own, failures reported in an error type of the builtin's own with an exit code per variant, and access to shell state and I/O through the execution context. | You want to extend the shell with commands specific to your application. |
| [`clap-builtin`](../../brush-builtin-clap/examples/clap-builtin.rs) | `brush-builtin-clap` | The same contract with parsing and help from a `clap::Parser` derive, via `impl_parse_args!`, and a failure of the builtin's own in a `builtins::Failure`. | You're writing a real builtin and want options, validation, and help without parsing by hand. |
| [`serde`](../../brush-parser/examples/serde.rs) | `brush-parser` (`--features serde`) | Parse a script and serialize the syntax tree to JSON, then deserialize it back. | Tooling that inspects, stores, or transmits shell syntax trees. |
| [`miette`](../../brush-parser/examples/miette.rs) | `brush-parser` (`--features diagnostics`) | Parse a file given on the command line and render any parse error as a [miette](https://github.com/zkat/miette) diagnostic with source spans. | Editors, linters, and other tools that need to show where a parse failed. |
| [`gen`](../../brush-shell/examples/gen.rs) | `brush-shell` (`--features schema`) | Generate the artifacts derived from the command line: man pages, markdown help, shell completion scripts, and the config file's JSON schema. Driven by `cargo xtask gen`. | You're a maintainer regenerating distributed artifacts. |
