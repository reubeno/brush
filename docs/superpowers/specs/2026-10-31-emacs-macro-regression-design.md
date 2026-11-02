# Preserve Emacs macro compatibility alongside vi editing

## Requirement and scope

The vi contribution must preserve existing Emacs functionality. Restore the
upstream Emacs prefix/macro translation removed by the vi refactor; keep
Reedline's built-in Emacs and vi machines and the callback-restoration layer.
The user approved this narrow approach instead of a new ordered replay engine.

Do not change the Atuin integration, its version, its five existing assertions,
or its expected-failure list. No new expected failures, integration-specific
Rust special cases, public API changes, or dependencies are permitted.

## Reproduction and cause

On prerequisite head `f3ee39f0`, the unchanged containerized Atuin suite reports
one pass, three unexpected failures, and one existing expected failure. Bash
passes all five tests in the same pinned Atuin 18.21.0 container. The failing
tests run the default Emacs editor; they do not enable vi.

The old binding configuration retained raw intermediate key sequences and
eagerly translated macro bodies into Reedline actions. The refactor rejects
those intermediate bindings and control-bearing macro bodies for every map.
Consequently Ctrl-R uses Reedline's native history search and Up navigates
history instead of opening Atuin.

## Design

Keep compatibility translation in the private binding-configuration layer of
`brush-interactive`, not in a replacement edit-mode wrapper. Emacs configuration
retains raw intermediate actions and macro definitions. When either changes,
recompute affected compiled bindings with the upstream longest-prefix behavior.
An unresolved intermediate target must not execute an unrelated default binding;
installing or changing that target refreshes the compiled trigger.

Preserve ordinary literal insertion macros and their existing validation. Bound
macro input sizes and avoid recursive expansion of arbitrary definitions.
Validate before mutation so rejected replacements retain their prior binding.
Keep map ownership explicit: Emacs compatibility definitions must not override
vi insert/command tables, visual mode, or Reedline's vi grammar.

This restores the previous eager translation contract, not faithful ordered
Readline replay. In particular, it does not promise that macro suffixes resume
after a callback and observe bindings changed by that callback. The existing
Atuin command-acceptance expected failure remains visible.

## Verification and propagation

1. Add focused unit and real-PTY coverage for Emacs intermediate-prefix macro
   dispatch, late target installation, rebinding, removal, and map isolation;
   observe the relevant tests fail before changing production code.
2. Run the unchanged pinned Atuin suite. Require four passes, zero unexpected
   failures, and exactly its original expected failure.
3. Preserve literal-macro, mode-switch, vi-operator, visual-mode, and direct
   callback tests. Run affected crate tests, compatibility tests, workspace
   checks, formatting, and strict Clippy; distinguish unrelated baseline issues.
4. Commit and push the prerequisite fix without rewriting published history.
   Merge that prerequisite normally into the callback branch, retaining the
   separate callback contribution, then rerun its callback and Atuin coverage.
5. Correct PR descriptions/comments that previously framed lost Emacs behavior
   as an acceptable scope limitation. Follow fresh CI and report approval or
   infrastructure blockers separately from test failures. Do not merge PRs.
