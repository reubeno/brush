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
| [`custom-builtin`](../../brush-core/examples/custom-builtin.rs) | `brush-core` | A builtin implemented in Rust and registered alongside the standard ones: the `Command` trait, `clap` argument parsing, `thiserror` errors mapped to exit codes, and access to shell state and I/O through the execution context. | You want to extend the shell with commands specific to your application. |
| [`serde`](../../brush-parser/examples/serde.rs) | `brush-parser` (`--features serde`) | Parse a script and serialize the syntax tree to JSON, then deserialize it back. | Tooling that inspects, stores, or transmits shell syntax trees. |
| [`miette`](../../brush-parser/examples/miette.rs) | `brush-parser` (`--features diagnostics`) | Parse a file given on the command line and render any parse error as a [miette](https://github.com/zkat/miette) diagnostic with source spans. | Editors, linters, and other tools that need to show where a parse failed. |
| [`gen`](../../brush-shell/examples/gen.rs) | `brush-shell` (`--features schema`) | Generate the artifacts derived from the command line: man pages, markdown help, shell completion scripts, and the config file's JSON schema. Driven by `cargo xtask generate`. | You're a maintainer regenerating distributed artifacts. |
