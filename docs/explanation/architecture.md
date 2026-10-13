# Architecture

This describes how brush is split into crates, and the principles that decide which crate
a piece of code belongs in. When a change raises the question "where should this go?",
the answer should follow from here. If it doesn't, this document needs updating.

## Crates

| Crate | Published | Role |
|---|---|---|
| `brush-parser` | yes | Tokenizes and parses shell syntax. Knows nothing about a running shell. |
| `brush-core` | yes | The shell itself: the `Shell` type, expansion, execution, jobs, variables, traps, and programmable completion. Has no line editor or UI of its own. |
| `brush-builtin-clap` | yes | The [clap](https://docs.rs/clap) adapter (`impl_parse_args!`) that brush's own builtins are parsed with. |
| `brush-builtins` | yes | The standard builtins, registered on a `Shell` through `brush-core`'s public API. |
| `brush-experimental-builtins` | yes | Builtins not ready to be standard. |
| `brush-coreutils-builtins` | yes | Optional builtins wrapping [uutils/coreutils](https://github.com/uutils/coreutils); doesn't depend on the other crates. |
| `brush-interactive` | yes | The interactive front end: reads input, edits the line (with reedline or a basic line reader), and presents prompts, completions, suggestions, and highlighting. |
| `brush-shell` | yes | The `brush` binary: its command line, and the wiring of the crates above into a shell. |
| `brush` | yes | A thin wrapper binary around `brush-shell`. |
| `brush-test-harness` | no | Runs the YAML test cases that compare brush with bash. |
| `xtask`, `fuzz` | no | Development tooling. |

Their dependencies run one way, from front end to parser. Each crate's direct dependencies
on the others (not counting dev-dependencies) are:

```text
brush                        → brush-shell
brush-shell                  → brush-interactive, brush-builtins, brush-builtin-clap,
                               brush-core, brush-experimental-builtins (optional),
                               brush-coreutils-builtins (optional)
brush-interactive            → brush-core, brush-parser
brush-builtins               → brush-builtin-clap, brush-core, brush-parser
brush-experimental-builtins  → brush-builtin-clap, brush-core
brush-builtin-clap           → brush-core
brush-core                   → brush-parser
brush-parser                 (none)
brush-coreutils-builtins     (none)
```

Anything a published crate exports is public API, including to people who embed brush
without its front end. Keep that in mind when deciding what to export.

## Structure within `brush-core`

- Shells are built with `Shell::builder()`, a type-safe builder.
- `Shell` is generic over `ShellExtensions`, which lets embedders swap in parts of its
  behavior (today, how errors are formatted and how processes are launched) at compile
  time.
- Platform-specific code lives under the `sys` module, so the rest of the crate stays
  platform-neutral.
- Builtins meet the shell at one contract, `brush_core::builtins`, which names no
  argument-parsing engine. A builtin describes itself (its help, and whether it's a
  declaration utility), parses an invocation's arguments, and runs. The shell owns
  everything around that: expanding the arguments, rendering help and usage errors in
  every form, and printing and attributing every failure a builtin returns.
  Builtins run other builtins through the shell (`ExecutionContext::run_builtin`).
- A builtin is a value (`builtins::Builtin`), so it can hold configuration or state;
  most builtins have none of their own and implement `builtins::StatelessBuiltin`.
  A builtin's value is shared by every copy of the shell, subshells included, so
  state that should follow subshell rules (as `getopts`'s does) belongs in the shell.
- A builtin's help is plain-text data (`builtins::Help`) at a fixed width, which the
  shell renders in every form. A builtin can also render it for a terminal
  (`ParseArgs::terminal_help`), which the adapters do with their parsing engine's own
  renderer; the shell asks for that when the output is a terminal or color is forced on.
- The engine adapters' API is their `impl_parse_args!` macro (and `to_usage_error`); what
  the macro expands to lives in a hidden `adapter` module, free to change.
- The builtin traits are generic over the shell's `ShellExtensions`. brush's own
  builtins are implemented for every shell, so they run in any; an embedder's builtin
  can instead be implemented for its own shell's type, to rely on it.

## Layering principles

These decide where a behavior belongs when more than one crate could plausibly own it.

1. **Meaning vs. presentation.** If getting it wrong changes what the shell does -- what
   command a line runs, or what a script sees -- it generally belongs in `brush-core`.
   If it only changes what the user sees, or how many keystrokes something takes, it
   belongs to the front end. For example, core composes a prompt from `PS1`, and the
   front end draws it. A front end presents what core produces and chooses among it,
   but doesn't recreate shell semantics on its own.

   Shell semantics that a front end needs are exposed as core APIs rather than
   reimplemented. Helpers that only make sense inside one of core's own operations stay
   private.

2. **Layers own their state.** A feature layered above the interpreter should reach
   `Shell` only through public APIs, and keep its own state. Until `Shell` can hold state
   owned by other crates, such features live in `brush-core`; don't add new coupling
   between them and the rest of core.
