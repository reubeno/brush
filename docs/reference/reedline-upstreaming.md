# `brush` vs `reedline` for Readline and Vi Compatibility Work

## Implemented redesign: 2026-10-29

This section describes the redesigned adapter. The review and prototype analysis
below are historical evidence, not claims about its current behavior. For usage,
see the [compatibility reference](compatibility.md#line-editing).

### Ownership and supported scope

- Brush retains shell options, Readline names, `bind` parsing, named-map aliases,
  shell-command execution, and editor-command policy.
- The custom edit-mode wrapper, eager vi key interception, recursive macro replay,
  and synthetic Bash default tables have been removed. Built-in emacs and vi
  machines are registered directly with Reedline's existing APIs.
- Both pinned Reedline 0.52.1 and current `main` omit `EventStatus` from public
  exports. An external wrapper cannot name the return type of the mode-event
  hook. Direct registration avoids that API obstacle rather than bypassing the
  hook or duplicating a modal engine.
- Shell configuration and live editor state are separate. Explicit selections
  carry a revision; host-command suspension does not reset the live family, and
  submitting input does not write the family back to shell options. Updating
  binding tables preserves the active prompt mode, but reconstructs the library
  machines rather than preserving their private pending/repeat state.
- Named maps are capability-gated. Reedline supports emacs standard/meta and vi
  insert/command; meta is a view of the same Alt bindings, not a shadowing table.
  Unsupported maps cannot mutate VT's emacs bindings.
- Unsupported actions, multi-key definitions, intercepted vi keys, and grammar
  motion remapping are rejected. Listings reflect representable table entries,
  not invented defaults for library grammar. The boolean unbinding interface
  cannot report errors for internal grammar keys; those keys remain library-owned.
- Macros are restricted to bounded literal insertion compiled into one edit.
  Terminal/control/modal input replay and recursive definitions are deferred.
- Disabled editing delegates to the existing minimal reader. External-editor
  commands use quoted literal arguments, current shell cwd/exported environment,
  and an owned private temporary directory. Invalid configuration is an input
  error; it is not silently reinterpreted as a different command.

### Draft checkpoint: open brush-local findings

The redesign is not yet ready to merge. Final review found three implementation
regressions, distinct from the deliberately unsupported features below:

- Unqualified startup bindings can select the wrong family before the first read.
- The builtin macro parser's byte fallback can lose Meta modifiers, bypassing
  unsupported-macro rejection and replacing a previous definition.
- Missing reverse mappings omit some supported default commands from queries and
  function-based removal.

In addition, `bind -v` does not yet report the active keymap for consumers such as
Atuin. Stock Atuin/fzf scripts still require prefix maps and ordered macros that
are outside this adapter's supported scope. The validation record and remaining
checks are captured in the [implementation plan](../superpowers/plans/2026-10-29-vi-readline-redesign.md#draft-checkpoint-validation-and-open-findings).

### Remaining upstream work

Keep Reedline at the current lockfile version for this change. Current `main`
includes a breaking history API change, so upgrading is a separate migration.

The strongest follow-up remains ordered engine input: Reedline currently parses
a terminal batch before applying its emitted events. Live tests must wait for
transition repaint boundaries; feeding a family switch followed by a key in one
burst can dispatch that key through the old editor. Bursts across vi transitions
can also use the final mode's cursor policy for earlier edits. General macro
replay cannot be built faithfully on top of this parse-ahead pipeline.

An upstream proposal should interleave parsing with execution, define suspension
and resumption at host-command/search/editor barriers, and bound total work as
well as recursion. Faithful rebound vi motions need a separate public grammar
extension point. Neither feature is emulated locally in this redesign.

## Pre-redesign reassessment: 2026-10-29

This review superseded the implementation recommendations in the prototype note below.
It is for maintainers deciding how to split the unfinished vi/readline work into reviewable
contributions; it is not a claim that the working implementation is ready to merge.

### Baseline and goals

The work originally sat uncommitted on brush `851d56a`. The reassessment uses upstream
brush `3871270a` (292 commits later), whose lock file resolves Reedline 0.52.1 rather
than the old 0.47 dependency. Rebasing only onto the fork's May-era `main` would not
have updated the relevant baseline.

The useful goals remain:

- Implement mutually exclusive emacs/vi shell options, including disabling editing.
- Make `bind -m vi-insert` and `bind -m vi-command` actually configure and query their
  named keymaps, including during shell startup.
- Support readline macros without losing distinctions such as vi `a` versus `A`.
- Let shell-specific bindings run commands and open an external editor correctly.

[brush issue #984](https://github.com/reubeno/brush/issues/984) still tracks vi mode.
The unrelated startup follow-ups are not acceptance criteria for this contribution.

### What upstream already provides

Do not propose these capabilities as new Reedline work:

- `Signal::HostCommand` distinguishes bound shell commands from submitted input.
  Brush already uses it; the old `# bind-command` string marker is obsolete.
- `ReedlineEvent::SwitchMode(PromptEditMode)` replaces `ViChangeMode(String)`.
  `with_additional_edit_mode`, `clear_edit_modes`, and the `EditMode` event hook support
  switching between editor machines as well as their internal states.
- `Vi::new` takes insert, normal, and visual keybinding tables. Visual mode must not
  accidentally inherit command-mode dispatch that erases its selection semantics.
- Vi already has a modal parser. Its handling of counts, pending motions, and mode
  transitions should not be duplicated in the host adapter.

See [Reedline PR #1214](https://github.com/nushell/reedline/pull/1214) and the
[0.52.1 edit-mode contract](https://github.com/nushell/reedline/blob/v0.52.1/src/edit_mode/base.rs).
The wrapper must preserve that contract rather than changing mode before the engine
can apply its cursor/selection repair.

Brush also now has a `vt` backend with its own binding and macro machinery, an existing
minimal reader, and real-terminal interactive tests. A Reedline-only implementation
must state its scope, not silently reinterpret a vi keymap as the active emacs map on
other backends. Disabling editing should reuse the existing no-editor reader rather
than introducing another reader with different multiline behavior.

### Recommended split

| Contribution | Owner | Disposition |
| --- | --- | --- |
| Shell options, `SHELLOPTS`, mode selection policy | brush | Keep; test independently of the editor. |
| `bind` syntax, readline names, keymap aliases, listing and unbinding | brush | Keep, with explicit backend capability handling. |
| Basic vi editing and cross-editor switching | brush integration over Reedline | Rework around existing library APIs; do not upstream a second modal parser. |
| Bash-specific defaults and shell command execution | brush | Keep policy local; verify actual key behavior, not only `bind -q` output. |
| Ordered synthetic-input replay | Reedline candidate | Investigate a bounded engine queue that interleaves parsing and execution. |
| External editor selection and temporary-file ownership | brush integration | Keep host policy; revisit lifecycle, shell environment, and error handling separately. |

The remaining strong library proposal is **ordered synthetic input through the active
editor**, not wholesale upstreaming of the current adapter. The local workaround parses
all macro strokes before executing their collected events. It also discards the remaining
strokes when a search or host-command barrier is encountered. A library proposal should
specify when input is suspended, resumed, or discarded, and bound both recursion and total
expanded work. Test it entirely within Reedline before depending on it in brush.

### Reproduced failures

Two checks demonstrate why the mechanical port is not ready to merge:

1. **Rebound motions bypass a pending vi operator.** In clean interactive brush and
   Bash sessions, run `set -o vi` and `bind -m vi-command '"w":forward-char'`. Type
   `abc def`, then press `Escape`, `0`, `d`, `w`. Bash leaves `bc def`; brush leaves
   `abc def` unchanged and moves the cursor right. The adapter intercepts the rebound
   key before Reedline's pending-delete parser can consume it. Direct action tests do
   not cover this interaction.
2. **Named keymaps leak on the `vt` backend.** Run
   `bind -m vi-command '"x":clear-screen'`, then
   `bind -m emacs-standard -q clear-screen` with `vt` selected. The latter reports both
   `"\C-l"` and `"x"`. The new keymap-specific trait defaults ignore the requested map,
   so an unsupported vi binding mutates the active emacs map rather than being rejected
   or stored separately.

### Source-review risks still requiring behavioral tests

These findings come from inspection, not additional reproduced terminal failures:

- Macro replay bounds recursion depth at 32 but does not bound total expanded work.
  A branching definition such as `x` expanding to `xx` can still grow exponentially.
  The existing `vt` replay instead limits total macro expansions.
- The new no-editing reader returns a single line without accumulating a complete
  shell program. The existing minimal backend already handles compound commands,
  here documents, and continuations; use that behavior rather than a second reader.
- Editor-mode changes are copied back to shell options only after successful input,
  not after `HostCommand` or interrupts. Verify that switching modes across those
  boundaries cannot be overwritten by the next prompt's option-to-editor sync.
- Buffer-editor setup relinquishes temporary-file cleanup ownership and selects its
  command before startup files run. It also splits editor commands on whitespace,
  which does not preserve quoted arguments. Revisit lifetime and host policy before
  advertising readline-compatible external editing.

Also exercise startup bindings without `-m`, visual-mode selection, and mode-specific
events routed back from the engine. A non-terminal `bind -q clear-screen` comparison
did not establish a startup regression: both brush and Bash reported it unbound.
The existing macro unit tests inspect returned events but do not establish end-to-end
buffer, history-search, cursor, or selection correctness.

### Validation of the mechanical port

The following targeted checks passed on the rebased work:

- 43 interactive-library unit tests with Reedline enabled.
- Six targeted core option tests and two builtin binding unit tests.
- 14 binding compatibility cases and six line-editing option compatibility cases.
- Six existing Reedline pseudo-terminal tests.
- Compilation of the interactive library with only the `vt` backend enabled.
- Rust formatting and Git whitespace checks.

Strict Clippy stops at `set_line_editing_mode` in the core options API because it can
be a `const fn` (`clippy::missing_const_for_fn`). This lint is still outstanding;
additional diagnostics may appear after it is addressed. The full workspace suite
and full CI workflow were not run. Passing targeted tests does not negate the two
reproduced failures above.

### Suggested delivery order

The mechanical rebase retains the unfinished implementation for review; it does not
endorse its custom dispatch layer. Prefer small brush contributions for options and
basic vi integration, followed by narrowly scoped compatibility work and a separate
Reedline replay proposal. Do not make basic vi support wait for a general macro engine.

## Historical pre-rebase analysis

The following describes the earlier prototype and its assumptions, not the current
library API or verified compatibility status.

This note is for maintainers deciding where future interactive compatibility work should live.

After reading it, you should be able to do three things:

1. Classify a new issue as `brush`-local, `reedline`-upstream, or split.
2. Decide whether an upstream issue or PR is worth opening.
3. Avoid upstreaming `bash` policy when the real gap is a generic editor primitive.

## Current Boundary

Today the interactive stack is split into three layers:

- `brush-builtins` parses `bind` syntax and owns the `bash`/readline compatibility surface.
- `brush-core` owns the shell-facing keybinding model: keymaps, input functions, macros, and shell-command bindings.
- `brush-interactive` adapts that model onto `reedline` through `ReedlineInputBackend` and `UpdatableBindings`.
- `reedline` owns the actual line editor: modal parsing, event execution, history search, menus, and painting.

That split implies a simple rule:

- If the work depends on `bash` syntax, readline names, startup policy, or shell-command execution, it belongs in `brush`.
- If the work is a generic editor capability that would help any host using `reedline`, it is a candidate for upstreaming.
- If `brush` can parse and decide correctly but lacks a clean editor primitive to execute the result, the work is split.

## Why This Came Up

Recent vi/readline macro fixes exposed a mismatch between readline semantics and `reedline`'s public model.

In readline, a macro can behave like synthetic keypresses that re-enter the normal parser. That matters in vi mode because the parser distinguishes commands like `a` and `A`, tracks mode changes, and treats some keys as operators rather than direct edit actions.

In `reedline`, keybindings resolve to `ReedlineEvent` values, and `Multiple` executes a list of events, not a list of synthetic keypresses. That is good enough for many editor actions, but it is not the same thing as replaying keys through the active edit mode.

`brush` now works around that by replaying stroke macros through its normal key parser before handing the resulting events back to `reedline`. That is the right local fix for compatibility, but it is also a signal that part of the problem is lower-level than `brush` itself.

## Decision Framework

Ask these questions in order:

1. Does this behavior exist because `bash` exposes `bind`, `.inputrc`, readline function names, or shell-bound commands?
2. Would another `reedline` consumer want this even if it had never heard of `bash`?
3. Can the change be described in terms of keys, modes, parser state, events, or editor execution, instead of `bind` syntax and readline jargon?
4. If upstream existed, would it let `brush` delete compatibility glue instead of just moving code around?

If the answer to (1) is yes, the change usually stays in `brush`.

If the answers to (2), (3), and (4) are yes, it is a strong upstream candidate.

If both are true, `brush` should keep the compatibility-facing layer while `reedline` gets a new primitive underneath it.

## Work That Should Stay in `brush`

### `bind` parsing and startup behavior

Parsing `bind` syntax, deciding which keymap is active, handling `.inputrc`, and implementing `bind -f` are all part of the shell's compatibility contract. `reedline` should not need to understand readline configuration files or `bash` startup policy.

### Readline vocabulary

`brush` owns the mapping from readline names such as `vi-movement-mode`, `unix-line-discard`, and `reverse-search-history` into shell-facing actions. Those names are part of the readline interface, not the editor engine.

### Shell-command bindings

`bind -x` is inherently host-specific. Executing a shell command from a keybinding is not an editor concern; it is application behavior.

### Compatibility presentation

Output such as `bind -p`, `bind -s`, and escape-sequence formatting belongs in `brush`. Those commands are about matching user-visible readline behavior, not about how the editor stores bindings internally.

### Bash-specific defaults

The exact default bindings needed to feel like `bash` are policy. `reedline` documents its vi support as basic and keeps its own defaults intentionally general. Trying to upstream every `bash`-specific binding choice would couple `reedline` to one host's compatibility target.

## Work That Is a Good `reedline` Candidate

### Engine-level macro replay

This is the strongest upstream candidate.

The underlying gap is not "support `bash` macros." The gap is that hosts cannot ask `reedline` to replay synthetic input through the active edit mode as if the user had typed it. That is a generic modal-editor capability.

An upstream solution should be framed in editor terms:

- enqueue synthetic input events or keystrokes
- feed them back through the active `EditMode`
- preserve mode transitions and parser behavior
- stop replay when the editor exits, suspends, or hands off to another subsystem
- centralize recursion limits and safety checks

If `reedline` had that primitive, `brush` could keep parsing readline macros while deleting much of its custom replay logic.

### Better support for advanced vi semantics

`reedline` already notes that support for more advanced vi commands is future work. When a bug or gap is genuinely about generic vi behavior rather than readline naming or shell integration, upstreaming is a good fit.

The key test is whether the change improves `reedline` as a reusable editor, not whether it makes one `bash` compatibility test pass.

### Cleaner host hooks around mode changes

`brush` currently has to mirror some mode transitions so that its shell-owned keymap model stays aligned with `reedline`'s internal vi state. If `reedline` exposed a cleaner way for hosts to observe or drive mode changes during synthetic replay, that could reduce custom glue.

This is a weaker candidate than macro replay, because the current API is usable, but it is still a reasonable follow-up if replay work exposes awkward host bookkeeping.

## Mixed Cases

Some issues naturally split across both projects.

### Readline macros

The shell-facing pieces stay in `brush`:

- parse the macro body
- decide which keymap it applies to
- keep `bind` output compatible
- preserve readline naming and policy

The editor-facing primitive belongs in `reedline`:

- replay keys through the active parser
- manage replay depth and exit conditions
- keep vi and emacs mode transitions internally consistent

### Raw byte macros

`brush` currently has a split implementation:

- if a macro can be represented as abstract strokes, it replays keys through the normal parser
- if it only exists as raw bytes, it still uses eager resolution to bound actions

That is an acceptable local compromise, but it is also evidence that the current public seam is not ideal. A stronger replay primitive in `reedline` would make it easier for hosts to converge on one model.

## Tradeoffs of Keeping the Work in `brush`

### Benefits

- Immediate compatibility wins without waiting for upstream review or release timing.
- Full control over `bash`-specific policy.
- Easy to test against `bash` as the oracle.
- No need to persuade `reedline` to accept API shaped by shell compatibility requirements.

### Costs

- More bridge code between shell actions and editor events.
- Duplication of vi defaults and mode-transition knowledge that `reedline` also has.
- Greater risk that a future `reedline` upgrade changes assumptions that `brush` is mirroring.
- Local compromises remain local, such as the current split between stroke replay and eager byte-macro resolution.
- Other `reedline` consumers do not benefit from the work.

## Tradeoffs of Pushing the Work into `reedline`

### Benefits

- The primitive becomes reusable for any host, not just `brush`.
- `brush` can delete custom compatibility glue instead of growing it.
- Vi behavior becomes more correct at the editor layer, where it naturally belongs.
- Future hosts with modal editing needs can build on the same mechanism.

### Costs

- Delivery slows down because it now depends on upstream review, API discussion, and release cadence.
- A poorly scoped PR can accidentally push `bash`-specific policy into a general-purpose editor.
- `reedline` may reasonably reject changes that solve only one host's compatibility target.
- Even accepted upstream work still leaves `brush` owning parsing, naming, config loading, and user-visible compatibility behavior.

## Recommendation

For the current state of the codebase:

1. Keep the compatibility-facing implementation in `brush`.
2. Upstream only generic editor primitives, not readline policy.
3. Treat engine-level macro replay as the first and best upstream target.
4. Treat cleaner mode-sync hooks as a secondary target if replay alone does not remove enough glue.
5. Do not upstream `bind` parsing, `.inputrc` loading, `bind -x`, `bind -p`, or `bash`-specific default binding tables.

In short: upstream the mechanism, keep the compatibility policy local.

## A Good First Upstream Proposal

If we open an issue or PR against `reedline`, the strongest proposal is:

"Add a host-facing way to enqueue synthetic input so macros can be replayed through the active edit mode instead of being compiled directly into editor events."

That proposal should explicitly avoid `bash` jargon and focus on generic requirements:

- works for vi and emacs modes
- preserves parser-level distinctions such as operator vs motion vs insertion
- handles nested replay safely
- stops replay on exit or subsystem hand-off
- can be tested entirely inside `reedline`

That is much more likely to land than a PR framed as "make `reedline` behave like readline."

## Questions to Ask Before Opening an Upstream Issue

Use this checklist:

1. Can I explain the problem without mentioning `bind`, `.inputrc`, or `bashrc`?
2. Would a non-shell host still want this capability?
3. Does the proposal add an editor primitive instead of a host policy?
4. Will `brush` get simpler if the change lands?
5. Can `reedline` test and maintain the behavior without importing readline semantics wholesale?

If the answer is no to most of those questions, keep the work in `brush`.

## Bottom Line

`brush` should continue to own readline compatibility.

`reedline` is still the right place for lower-level editing primitives that make compatibility easier to implement cleanly. The recent macro replay work suggests that the best contribution is not a `bash` compatibility patch, but a generic synthetic-input replay mechanism for modal editing.
