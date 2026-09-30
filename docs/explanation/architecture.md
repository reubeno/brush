# Architecture

This describes how brush is split into crates, and the principles that decide which crate
a piece of code belongs in. When a change raises the question "where should this go?",
the answer should follow from here. If it doesn't, this document needs updating.

## Crates

| Crate | Published | Role |
|---|---|---|
| `brush-parser` | yes | Tokenizes and parses shell syntax. Knows nothing about a running shell. |
| `brush-core` | yes | The shell itself: the `Shell` type, expansion, execution, jobs, variables, traps, and programmable completion. Has no line editor or UI of its own. |
| `brush-builtins` | yes | The standard builtins, registered on a `Shell` through `brush-core`'s public API. |
| `brush-experimental-builtins` | yes | Builtins not ready to be standard. |
| `brush-coreutils-builtins` | yes | Optional builtins wrapping [uutils/coreutils](https://github.com/uutils/coreutils); doesn't depend on the other crates. |
| `brush-interactive` | yes | The interactive front end: reads input, edits the line (with reedline or a basic line reader), and presents prompts, completions, suggestions, and highlighting. |
| `brush-shell` | yes | The `brush` binary: its command line, and the wiring of the crates above into a shell. |
| `brush` | yes | A thin wrapper binary around `brush-shell`. |
| `brush-test-harness` | no | Runs the YAML test cases that compare brush with bash. |
| `xtask`, `fuzz` | no | Development tooling. |

Their dependencies run one way, from front end to parser:

```text
brush → brush-shell → brush-interactive ──────────→ brush-core → brush-parser
                    ↘ brush-builtins ─────────────↗
                    ↘ brush-experimental-builtins ↗
                    ↘ brush-coreutils-builtins
```

Anything a published crate exports is public API, including to people who embed brush
without its front end. Keep that in mind when deciding what to export.

## Structure within `brush-core`

- Shells are built with `Shell::builder()`, a type-safe builder.
- `Shell` is generic over `ShellExtensions`, which lets embedders swap in parts of its
  behavior (today, how errors are formatted) at compile time.
- Platform-specific code lives under the `sys` module, so the rest of the crate stays
  platform-neutral.

## Layering principles

These decide where a behavior belongs when more than one crate could plausibly own it.

1. **Meaning vs. presentation.** If getting it wrong changes what the shell does -- what
   command a line runs, or what a script sees -- it belongs in `brush-core`. If it only
   changes what the user sees, or how many keystrokes something takes, it belongs to the
   front end. For example, core composes a prompt from `PS1`, and the front end draws it.
   A front end presents what core produces and chooses among it, but doesn't recreate
   shell semantics on its own.

   Shell semantics that a front end needs are exposed as core APIs rather than
   reimplemented. Helpers that only make sense inside one of core's own operations stay
   private.

2. **Preferences are inputs, not state.** A front-end setting that affects what core
   produces is passed into core by the caller. Core doesn't read it from its own state,
   even where, for now, that's where it's stored.

3. **Hints are data.** Script intent that affects only presentation is reported to the
   front end as data, which it may ignore, rather than being acted on by core.

4. **Report, don't guess.** Core reports what it already knows as structured data, so no
   higher layer has to reconstruct it by re-parsing text or guessing. Result types are
   `#[non_exhaustive]` so they can grow, but don't add a field until a consumer needs it.
   Plain values that are complete by definition (e.g. a range of text and its
   replacement) needn't be.

5. **Layers own their state.** A feature layered above the interpreter should reach
   `Shell` only through public APIs, and keep its own state. Until `Shell` can hold state
   owned by other crates, such features live in `brush-core`; don't add new coupling
   between them and the rest of core.

Programmable completion is the most thorough application of these principles so far:
`Shell::complete` returns each candidate as a ready-to-apply edit of the line, and
`brush-interactive` only chooses among them and lists them. The `brush_core::completion`
module docs describe how.

## Known gaps

- **Features that could be layers still live in `brush-core`.** `Shell` has no place for
  state owned by crates above core. So programmable completion, which a shell could
  otherwise do without, can't be split out, and settings that belong to the front end but
  are set by builtins (e.g. `bind`) have to be stored in core.
