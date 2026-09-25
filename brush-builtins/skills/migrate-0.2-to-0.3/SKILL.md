---
name: migrate-brush-builtins-0.2-to-0.3
description: Migrate a crate that depends on brush-builtins 0.2 to 0.3 (usage-rs argument parsing, the exported +/- flag macro, MSRV 1.91).
depends-on: brush-builtins=0.3
---

# Migrating brush-builtins 0.2 → 0.3

Machine-oriented migration spec. Apply the rules below to a codebase that
depends on `brush-builtins 0.2` so that it compiles and behaves correctly
against `brush-builtins 0.3`. The human walkthrough is
`docs/migrations/brush-builtins/0.2-to-0.3.md` in the brush repository.

Scope: the exported `minus_or_plus_flag_arg!` macro, the crate's MSRV, and the
observable behavior of builtins whose arguments are now parsed by usage-rs
instead of clap. `BuiltinSet`, `default_builtins`, and `ShellBuilderExt` are
unchanged.

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
# Only if you invoke usage_minus_or_plus_flag_arg! (entry 1):
usage = { package = "usage-rs", version = "6.11.1" }
```

Your crate's `rust-version` must be at least `1.91` (entry 2).

## Breaking changes

### 1. MECHANICAL — `minus_or_plus_flag_arg!` replaced by `usage_minus_or_plus_flag_arg!`

The macro now declares a `usage::Args` struct instead of a `clap::Parser` one,
takes the `+` spelling explicitly, and no longer implements
`From<Struct> for Option<bool>`.

Detection: `grep -rn -E "\bminus_or_plus_flag_arg!"`; compiler error
`cannot find macro minus_or_plus_flag_arg`.

Action: rename, add the `+` long spelling as the third argument, and flatten
with usage instead of clap:

```rust
// Before
brush_builtins::minus_or_plus_flag_arg!(ExportFlag, 'x', "Mark for export.");

#[derive(clap::Parser)]
struct MyCommand {
    #[clap(flatten)]
    export: ExportFlag,
}

// After
brush_builtins::usage_minus_or_plus_flag_arg!(ExportFlag, 'x', "+x", "Mark for export.");

#[derive(usage::Cli)]
struct MyCommand {
    #[usage(flatten)]
    export: ExportFlag,
}
```

Replace `Option::<bool>::from(flag)` and `flag.into()` (as `Option<bool>`)
with `flag.to_bool()`.

The generated struct is `pub(crate)` and derives `usage::Args`, so the
enclosing command must also parse with usage-rs, through
`brush_builtin_usage::usage_builtin!`.

### 2. MECHANICAL — MSRV is 1.91

usage-rs requires rustc 1.91.

Detection: `grep -rn -E "^rust-version" Cargo.toml`, or the Cargo error
`package brush-builtins ... requires rustc 1.91`.

Action: set `rust-version = "1.91"` or later.

## Additive APIs (no action required)

- `brush-builtin-usage`: `usage_builtin!` implements `FromArgs` and
  `HelpContent` for a `#[derive(usage::Cli)]` type, in the same three forms as
  `clap_builtin!` (plain, `trailing_args = field`, `declarations = field`).
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
- Help text (`help NAME`, `NAME --help`) and usage errors are rendered by
  usage-rs: headings read `Flags:`, descriptions end with a period, and
  unknown-option messages suggest similar options.

## VERIFY

```sh
cargo build --all-targets
grep -rn -E "\bminus_or_plus_flag_arg!" src/
```

The grep must print nothing. Then run your test suite, paying attention to
tests that compare builtin help or error text.
