# Vi/Readline Adapter Redesign Implementation Plan

> **For agentic workers:** Execute inline with test-driven checkpoints. The user approved the architecture in the preceding deep review. Do not commit or push without a separate request.

**Goal:** Deliver basic vi editing and honest named-keymap support without a second modal parser or unsafe synthetic-input replay.

**Architecture:** Brush owns shell options, Readline names, prefix-map selection, and editor-command policy. Reedline 0.52.1 owns vi grammar, mode-event execution, cursor/selection repair, and editor handoff. Unsupported replay and grammar remapping are explicit limitations rather than silent fallbacks.

**Tech Stack:** Rust 2024, existing Cargo workspace, Reedline 0.52.1, existing expectrl PTY tests.

## Global Constraints

- Preserve unrelated changes and the existing index; do not change dependency versions.
- No new vi implementation for the VT backend and no vendored Reedline engine.
- General modal/control-key macro replay and Bash-compatible rebound motions require separate upstream work. Retain only bounded, literal insertion macros that need no replay; reject unsupported definitions atomically.
- Preserve runtime editor state across host-command suspension; shell options remain configuration, not a mirror written on submission.
- Keep temporary editor files in an owned private directory and refresh editor selection from current shell state.

## Tasks

- [x] Add process regressions for disabled-editing compound commands, here documents, and continuations. Observe failure, then delegate to the existing minimal backend and enable its feature for Reedline builds.
- [x] Add a VT isolation regression. Introduce a default `supports_keymap(KeyMap) -> bool` capability, gate named operations, and reject unsupported maps in `bind` before mutation.
- [x] Add mode-event, unsupported-action, macro-rejection, and prefix-map regressions. Remove custom vi dispatch and eager mode mutation; register Reedline's built-in editors directly and preserve the active prompt mode when rebuilding tables.
- [x] Add PTY regressions for vi operators, visual selection, and editor-family switches across `HostCommand`. Test emacs-meta alias consistency at the adapter boundary; reject emacs-ctlx. Preserve live state until explicit shell configuration changes, including same-option reselection.
- [x] Add external-editor argument/lifetime regressions. Refresh configuration before reads, use brush's parser for quoted literal arguments without expansion, and retain private temporary-directory ownership. Test malformed configuration and cancellation without losing the editor.
- [ ] Run targeted tests, compatibility cases, feature-isolated checks, formatting and strict Clippy; then broader CI where practical. Update compatibility/architecture documentation, run `graphify update .`, and report any unavailable tooling or remaining upstream limitations.

## Acceptance Checks

```sh
cargo test --package brush-interactive --lib --features reedline
cargo test --package brush-interactive --lib --features vt
cargo test --package brush-shell --test reedline_interactive_tests
cargo test --package brush-shell --test brush-compat-tests -- bind
cargo xtask ci quick
```

Tests assert final buffer contents, command effects, exit status, unsupported-operation errors, and temporary-file cleanup. Event-only assertions are limited to the adapter's mode-event contract; they do not substitute for PTY behavior.

## Implementation Notes and Deviations

- Reedline's `EventStatus` return type is not publicly exported in 0.52.1 or current `main`, so an external wrapper cannot implement its full event hook. Registering the built-in emacs/vi machines removes the wrapper rather than forwarding an incomplete contract.
- Emacs-meta is a view of the standard map's Alt chords, avoiding inconsistent shadow tables. Grammar-motion remapping, vi Escape/visual-entry interception, emacs-ctlx, general ordered action sequences, and nonliteral macro replay are rejected rather than emulated.
- The bind parser sometimes emits plain literal text as byte chunks. Only bounded UTF-8 literal chunks are decoded for one insertion; terminal input is never replayed.
- Reedline parses terminal bursts ahead of execution. PTY tests wait for transition repaint boundaries rather than assuming ordered per-key execution across a mode change. This remaining upstream limitation is documented.
- Four oracle cases remain explicit known failures: two general macros, vi default grammar listings, and vi grammar-key unbind/rebind. The original assertions remain available; supported literal macros and table bindings are tested separately.
- `cargo xtask ci full --no-hooks -k` passed formatting, crate structure, all-feature/all-target build and lint, and schema checks, but could not invoke unit/integration tests because `cargo-nextest` is not installed. Standard Cargo tests are the fallback. `prek` is unavailable, so hooks are not run.
- `graphify update .` could not run: `graphify` is not installed. No graph files were hand-edited.

## Draft Checkpoint: Validation and Open Findings

This is a work-in-progress checkpoint, not a merge-ready implementation. The
following checks were run during implementation before this checkpoint; they are
not a claim that the complete CI workflow is green:

- Workspace library tests: 635 passed, none failed, one ignored. The separately
  enabled Reedline library suite passed 55 tests; the VT library suite passed 147.
- Reedline process/PTY suite: 13 passed, including the final workspace run.
- Targeted binding compatibility: 12 succeeded with four documented known
  failures and no unexpected failures. Line-editing option compatibility: six
  succeeded.
- All-feature/all-target build, strict Clippy, formatting, schema checks, and
  isolated Reedline/VT/no-backend builds passed.
- The final no-fail-fast workspace run reported 818 Rust tests passed, one
  failed, and four ignored. Compatibility cases reported 2380 succeeded, two
  failed, 744 known failures, and 38 skipped. Brush-specific cases reported 159
  succeeded, four known failures, and one skipped.
- The Rust failure was the interactive pipeline/pager test with inherited
  `LESS=-XRF`; its isolated rerun passed with `LESS` unset. The compatibility
  failures were Bash-oracle EOF timeouts: the fatal-expansion/subshell case
  reproduced independently, while the `compopt` case passed in isolation.
- A locked release build succeeded. The installed copy matched the artifact,
  and version, command-execution, and emacs/vi option smoke checks passed. These
  smoke checks intentionally skipped startup files and did not establish
  compatibility with an existing Bash startup setup.

The final read-only review identified three brush-local regressions still open:

1. The binding configuration initially selects `vi-insert` before the first
   read, so unqualified startup bindings can land in the wrong editor family.
2. Macro conversion's byte fallback loses Meta modifiers for mixed literal/Meta
   bodies; an unsupported replacement can therefore succeed instead of failing
   atomically. Adapter-only stroke tests do not cover this builtin boundary.
3. Missing reverse mappings hide supported default edit commands such as
   transpose, backward-word, and case conversion from queries and `bind -u`.

Real startup also exposed missing `set keymap ...` output from `bind -v`, which
prevents Atuin from discovering the active map. Stock Atuin/fzf integrations use
multi-key prefixes and ordered macros outside this adapter's supported scope;
fixing map reporting alone will not make those integrations fully compatible.
Unrelated startup builtin fixes remain outside this contribution.

The final acceptance task remains open for these findings and the unavailable
tooling noted above. This checkpoint does not restore the discarded replay
wrapper or change the Reedline dependency version.
