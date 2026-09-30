---
name: migrate-brush-shell-0.4-to-0.5
description: Migrate a crate that depends on brush-shell 0.4 to 0.5 (command-line types move from clap to usage-rs).
depends-on: brush-shell=0.5
---

# Migrating brush-shell 0.4 → 0.5

Machine-oriented migration spec. Apply the rules below to a codebase that
depends on `brush-shell 0.4` as a library so that it compiles and behaves
correctly against `brush-shell 0.5`. The human walkthrough is
`docs/migrations/brush-shell/0.4-to-0.5.md` in the brush repository.

Scope: `brush_shell::args` and `brush_shell::events`, whose command-line types
now derive usage-rs traits instead of clap's. `brush_shell::entry::run` and
`brush_shell::config` are unchanged.

## Execution model

1. Apply every MECHANICAL rule, in order. Match whole identifiers only.
2. Review every BEHAVIOR item against your tests.
3. Run `cargo build --all-targets`.
4. Do not introduce compatibility shims (a local clap mirror of
   `CommandLineArgs`). Migrate the call sites.

## Required Cargo.toml changes

```toml
# Before
brush-shell = "0.4"

# After
brush-shell = "0.5"
# Only if you call usage-rs APIs on these types directly:
usage = { package = "usage-rs", version = "6.11.1" }
```

## Breaking changes

### 1. MECHANICAL — `CommandLineArgs` is a `usage::Cli`, not a `clap::Parser`

`CommandLineArgs` no longer implements `clap::Parser`, `clap::CommandFactory`,
`clap::FromArgMatches`, or `clap::Args`.

Detection: `grep -rn -E "CommandLineArgs::(parse|try_parse|parse_from|try_parse_from|command|command_for_update)\b|CommandFactory|FromArgMatches"`;
compiler errors naming those traits.

Action:

| 0.4 (clap) | 0.5 (usage-rs) |
|---|---|
| `CommandLineArgs::try_parse_from(argv)` (argv includes the program name) | `CommandLineArgs::parse_from_argv(&os_str_refs)` |
| `CommandLineArgs::parse()` | `CommandLineArgs::parse()` (unchanged name; prints help or errors and exits) |
| `CommandLineArgs::command()` for help or docs | `CommandLineArgs::spec()` or `CommandLineArgs::to_kdl()` |
| `clap_complete::generate(shell, &mut CommandLineArgs::command(), ...)` | `CommandLineArgs::completion_script(usage::complete::Shell::Bash)` |

`parse_from_argv` takes `&[&OsStr]` and returns `usage::Error` on failure.
`usage::Error` implements neither `Display` nor `std::error::Error`, so `?`
into `anyhow::Error` or `Box<dyn Error>` does not compile; map it with
`usage::render_failure(CommandLineArgs::spec(), &argv, &err)`, which returns
the rendered message as a `String`.
`parse_from_argv` does not apply brush's `--` and `-c` handling (bash takes `-c`'s command
string from the first operand); only the `brush` binary does.

### 2. MECHANICAL — `help` and `version` are `bool`

Detection: `grep -rn -E "\.(help|version)\b" | grep -E "CommandLineArgs|args\."`;
compiler error `expected bool, found Option<_>`.

Action: `args.help == Some(true)` → `args.help`; `args.version.is_some()` →
`args.version`.

### 3. MECHANICAL — `TraceEvent` and `InputBackendType` derive `usage::ValueEnum`

`brush_shell::events::TraceEvent` and `brush_shell::args::InputBackendType` no
longer implement `clap::ValueEnum`. Both implement `std::str::FromStr`, with
error types `events::UnknownEventName` and `args::UnknownBackend`.

Detection: `grep -rn -E "\b(TraceEvent|InputBackendType)\b.*(ValueEnum|value_variants|to_possible_value|from_str\(.*, *(true|false)\))"`.

Action: `TraceEvent::from_str(s, true)` (clap's `ValueEnum::from_str`) →
`s.parse::<TraceEvent>()`. For the list of names, use `Display` over the
variants you need.

## Additive APIs (no action required)

- `CommandLineArgs::completion_request` answers the
  `brush __complete_word__ ...` protocol used by generated completion scripts.
- `CommandLineArgs::to_kdl()` returns the command line's usage spec.

## Behavior changes (no code change; check tests)

- `brush --help` is rendered by usage-rs: headings read `Flags:`, and
  descriptions end with a period. Unknown-option errors suggest similar
  options.
- `brush -s -s` (a repeated boolean option) is accepted, as in bash.
- Completion scripts from `cargo xtask gen completion` call back into `brush`
  on every Tab rather than embedding the option list; `brush` must be on
  `PATH` where they run.

## VERIFY

```sh
cargo build --all-targets
grep -rn -E "CommandLineArgs::(try_parse|try_parse_from|command)\(|CommandFactory|\b(TraceEvent|InputBackendType)::(value_variants|from_str)\b" src/
```

The grep must print nothing. Then run your test suite.
