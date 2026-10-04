---
name: migrate-brush-builtins-0.2-to-0.3
description: Migrate a crate that depends on brush-builtins 0.2 to 0.3 (winnow-args argument parsing, the removed +/- flag macro).
depends-on: brush-builtins=0.3
---

# Migrating brush-builtins 0.2 → 0.3

Machine-oriented migration spec. Apply the rules below to a codebase that
depends on `brush-builtins 0.2` so that it compiles and behaves correctly
against `brush-builtins 0.3`. The human walkthrough is
`docs/migrations/brush-builtins/0.2-to-0.3.md` in the brush repository.

Scope: the exported `minus_or_plus_flag_arg!` macro and the observable
behavior of builtins whose arguments are now parsed by winnow-args instead of
clap. `BuiltinSet`, `default_builtins`, and `ShellBuilderExt` are unchanged.

## Execution model

1. Apply every MECHANICAL rule, in order. Match whole identifiers only.
2. Review every BEHAVIOR item against your tests.
3. Run `cargo build --all-targets`.
4. Do not introduce compatibility shims (a local `minus_or_plus_flag_arg!`).
   Migrate the call sites.

## Required Cargo.toml changes

```toml
# Before
brush-builtins = "0.2"

# After
brush-builtins = "0.3"
# Only if you declare builtins of your own with winnow-args (entry 1):
brush-builtin-winnow = "0.1"
winnow-args = "0.1"
```

## Breaking changes

### 1. MECHANICAL — `minus_or_plus_flag_arg!` is removed

A `-x`/`+x` pair is one `Option<bool>` field of a winnow-args command.

Detection: `grep -rn -E "\bminus_or_plus_flag_arg!"`; compiler error
`cannot find macro minus_or_plus_flag_arg`.

Action: delete the macro invocation, and in the command that flattened it add
a field with the same short letter as `short` and `plus`, under
`#[arg(plus_options)]` on the struct:

```rust
// Before
brush_builtins::minus_or_plus_flag_arg!(ExportFlag, 'x', "Mark for export.");

#[derive(clap::Parser)]
struct MyCommand {
    #[clap(flatten)]
    export: ExportFlag,
}

// After
#[derive(winnow_args::Args)]
#[arg(plus_options)]
struct MyCommand {
    /// Mark for export.
    #[arg(short = 'x', plus = 'x')]
    export: Option<bool>,
}

brush_builtin_winnow::winnow_builtin!(MyCommand);
```

Replace `Option::<bool>::from(cmd.export)` and `cmd.export.into()` (as
`Option<bool>`) with `cmd.export`.

## Additive APIs (no action required)

- `brush-builtin-winnow`: `winnow_builtin!` implements `FromArgs` and
  `HelpContent` for a `#[derive(winnow_args::Args)]` type, in the same three
  forms as `clap_builtin!` (plain, `trailing_args = field`,
  `declarations = field`).
- `brush_builtin_utils::into_words` and `split_leading_options` are public.

## Behavior changes (no code change; check tests)

- `echo`: options stop at the first operand. `echo hi -n` prints `hi -n`.
  Repeated options are accepted (`echo -nn x`).
- `set`: an unknown option such as `set -z` or `set +z` is an error with
  status 2. It was silently accepted.
- `unset -f -v` prints `cannot simultaneously unset a function and a variable`
  and returns 1, as in bash. It returned 2 with a clap usage message.
- `printf`: usage errors use bash's wording (`printf: -v: option requires an
  argument`). A `--` after `-v var` ends the options.
- Invalid options read as bash's (`name: -z: invalid option`, then
  `name: usage: ...`). Help text (`help NAME`, `NAME --help`) is rendered by
  winnow-args; `-h` is not a help flag, as in bash.

## VERIFY

```sh
cargo build --all-targets
grep -rn -E "\bminus_or_plus_flag_arg!" src/
```

The grep must print nothing. Then run your test suite, paying attention to
tests that compare builtin help or error text.
