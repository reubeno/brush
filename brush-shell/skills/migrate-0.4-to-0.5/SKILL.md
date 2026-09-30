---
name: migrate-brush-shell-0.4-to-0.5
description: Migrate a crate that depends on brush-shell 0.4 to 0.5 (command-line types move from clap to winnow-args).
depends-on: brush-shell=0.5
---

# Migrating brush-shell 0.4 → 0.5

Machine-oriented migration spec. Apply the rules below to a codebase that
depends on `brush-shell 0.4` as a library so that it compiles and behaves
correctly against `brush-shell 0.5`. The human walkthrough is
`docs/migrations/brush-shell/0.4-to-0.5.md` in the brush repository.

Scope: `brush_shell::args` and `brush_shell::events`, whose command-line types
now derive winnow-args traits instead of clap's. `brush_shell::entry::run` and
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
# Only if you call winnow-args APIs on these types directly:
winnow-args = "0.1"
```

## Breaking changes

### 1. MECHANICAL — `CommandLineArgs` is a `winnow_args::Args`, not a `clap::Parser`

`CommandLineArgs` no longer implements `clap::Parser`, `clap::CommandFactory`,
`clap::FromArgMatches`, or `clap::Args`.

Detection: `grep -rn -E "CommandLineArgs::(parse|try_parse|parse_from|try_parse_from|command|command_for_update)\b|CommandFactory|FromArgMatches"`;
compiler errors naming those traits.

Action (bring `winnow_args::Args` into scope):

| 0.4 (clap) | 0.5 (winnow-args) |
|---|---|
| `CommandLineArgs::try_parse_from(argv)` (argv includes the program name) | `CommandLineArgs::try_parse_from(&argv[1..])` (no program name) |
| `CommandLineArgs::parse()` | `CommandLineArgs::parse()` (unchanged; prints help or errors and exits) |
| `CommandLineArgs::command()` for help or docs | `CommandLineArgs::HELP` (a `winnow_args::help::Command`) |
| `clap_complete::generate(shell, &mut CommandLineArgs::command(), ...)` | `CommandLineArgs::completion_script(winnow_args::complete::Shell::Bash)` |

`try_parse_from` returns `winnow_args::Error`, which implements
`std::error::Error`; `render_help` and `render` give its text. It does not
apply brush's `--` and `-c` handling (bash takes `-c`'s command string from
the first operand); only the `brush` binary does.

### 2. MECHANICAL — `help` is gone, `version` is `bool`

Detection: `grep -rn -E "\.(help|version)\b" | grep -E "CommandLineArgs|args\."`;
compiler errors `no field help` or `expected bool, found Option<_>`.

Action: `args.version.is_some()` → `args.version`. Remove reads of
`args.help`: `--help` is answered while parsing, as an error of kind
`HelpRequested`.

### 3. MECHANICAL — `TraceEvent` and `InputBackendType` derive `winnow_args::ValueEnum`

`brush_shell::events::TraceEvent` and `brush_shell::args::InputBackendType` no
longer implement `clap::ValueEnum`. Both implement `std::str::FromStr`, with
error types `events::UnknownEventName` and `args::UnknownBackend`.

Detection: `grep -rn -E "\b(TraceEvent|InputBackendType)\b.*(ValueEnum|value_variants|to_possible_value|from_str\(.*, *(true|false)\))"`.

Action: `TraceEvent::from_str(s, true)` (clap's `ValueEnum::from_str`) →
`s.parse::<TraceEvent>()`. For the list of names, use
`<TraceEvent as winnow_args::FromArg>::CHOICES`.

## Additive APIs (no action required)

- `CommandLineArgs::completion_request` answers the
  `brush __complete_word__ --shell SHELL --line LINE` callback used by
  generated completion scripts.

## Behavior changes (no code change; check tests)

- `brush --help` is rendered by winnow-args; `-h` is not a help flag, as in
  bash.
- `brush -s -s` (a repeated boolean option) is accepted, as in bash.
- `+o`/`+O` are options only before the first operand.
- Completion scripts from `cargo xtask gen completion` call back into `brush`
  on every Tab rather than embedding the option list; `brush` must be on
  `PATH` where they run.

## VERIFY

```sh
cargo build --all-targets
grep -rn -E "CommandLineArgs::command\(|CommandFactory|\b(TraceEvent|InputBackendType)::(value_variants|from_str)\b" src/
```

The grep must print nothing. Then run your test suite.
