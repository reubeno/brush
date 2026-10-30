# Architecture

This describes how brush is split into crates, and the principles that decide which crate
a piece of code belongs in. When a change raises the question "where should this go?",
the answer should follow from here. If it doesn't, this document needs updating.

## Crates

| Crate | Published | Role |
|---|---|---|
| `brush-parser` | yes | Tokenizes and parses shell syntax. Knows nothing about a running shell. |
| `brush-core` | yes | The shell itself: the `Shell` type, expansion, execution, jobs, variables, traps, and programmable completion. Has no line editor or UI of its own. |
| `brush-clap-adapter` | yes | Implements `brush-core`'s engine-neutral builtin contract for builtins whose arguments are parsed with clap. |
| `brush-winnow-args-adapter` | yes | The same, for builtins whose arguments are parsed with [winnow-args](https://github.com/lu-zero/winnow-args). |
| `brush-builtins` | yes | The standard builtins, registered on a `Shell` through `brush-core`'s public API. |
| `brush-experimental-builtins` | yes | Builtins not ready to be standard. |
| `brush-coreutils-builtins` | yes | Optional builtins wrapping [uutils/coreutils](https://github.com/uutils/coreutils); doesn't depend on the other crates. |
| `brush-interactive` | yes | The interactive front end: reads input, edits the line (with reedline or a basic line reader), and presents prompts, completions, suggestions, and highlighting. |
| `brush-hosting` | yes | What a program that owns its process does to host shells, through `brush-core`'s public API, a feature for each: the async runtime shells run on (`runtime`), and the terminal and process group an interactive shell's job control needs (`terminal`: `ProcessTerminal`, which `acquire` takes them for). Not shell semantics, nor presentation. |
| `brush-shell` | yes | The `brush` binary: its command line, and the wiring of the crates above into a shell. |
| `brush` | yes | A thin wrapper binary around `brush-shell`. |
| `brush-test-harness` | no | Runs the YAML test cases that compare brush with bash. |
| `brush-bundled-tests` | no | Tests the bundled-command shim end to end, against a brush with fake bundled commands. |
| `xtask`, `fuzz` | no | Development tooling. |

Their dependencies run one way, from front end to parser. Each crate's direct dependencies
on the others (not counting dev-dependencies) are:

```text
brush                        → brush-shell
brush-shell                  → brush-interactive, brush-builtins, brush-clap-adapter,
                               brush-core, brush-hosting,
                               brush-experimental-builtins (optional),
                               brush-coreutils-builtins (optional)
brush-interactive            → brush-core, brush-parser
brush-hosting                → brush-core
brush-builtins               → brush-clap-adapter, brush-core, brush-parser
brush-experimental-builtins  → brush-clap-adapter, brush-core
brush-clap-adapter           → brush-core
brush-winnow-args-adapter    → brush-core
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
  argument-parsing engine. A builtin is a type that is its own parsed arguments: it
  parses an invocation's words (`ParseArgs`), supplies its help, and runs
  (`Builtin`), and is registered by type (`Registration::of`). Builtins that parse
  with clap implement `ParseArgs` through `brush-clap-adapter`, and those that parse
  with winnow-args through `brush-winnow-args-adapter`.
- A shell keeps its builtins' registrations, by name, in a `builtins::Registry`
  (`Shell::builtins`, `Shell::builtins_mut`), which is the one way to look them up,
  register them, or enable and disable them. How it stores them is private, so
  that can change (to share them between a shell and its subshells, say) without
  changing the API.
- State a builtin keeps between invocations (as `getopts`'s place in its arguments)
  lives in the shell, kept by its type (`ExecutionContext::state_mut`), and is cloned
  with the shell, so it follows subshell rules as a variable does. A program embedding
  the shell can set it beforehand (`Shell::set_builtin_state`).

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

3. **The host owns the process.** A shell runs in a process it doesn't own: whoever hosts
   it may run several shells, or have uses of its own for the process. So what the shell
   must keep asking of the process is a contract the host implements, named in the shell's
   `ShellExtensions`, as starting programs is (`ProcessLauncher`). The test for a contract:
   would two legitimate hosts of the same code -- a terminal emulator embedding a shell, a
   test harness running fifty shells in one process, a sandbox -- want different answers? If
   so, it's a contract; if not, it's shell behavior, with platform code under `sys` called
   directly. Handing the terminal's foreground to each job under job control is the latter:
   a job in a process group of its own on a terminal must have the foreground, and the
   shell must take it back to read its next command, whoever hosts it. What the host gives
   once, such as the standard streams or the environment, is a value on the shell's
   builder, not a contract.

   What takes over something the whole process shares -- the process group, the terminal,
   the runtime -- lives in `brush-hosting`, a layer above core using only its public API,
   and the `brush` binary opts in by calling it; a library such as `brush-interactive`
   never takes the process over. (Core's signal listeners are the exception, for now.)
