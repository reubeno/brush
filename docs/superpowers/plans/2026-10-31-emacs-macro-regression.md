# Emacs macro compatibility restoration implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans to implement this plan task-by-task inline, with verification checkpoints.

**Goal:** Restore the Emacs behavior lost by the vi contribution without weakening existing integration tests.

**Architecture:** Retain raw intermediate actions in the private Reedline binding configuration and eagerly compile Emacs macro bodies with longest-prefix matching, as upstream did. Keep Reedline's built-in editors and existing callback boundaries; do not introduce a replay engine.

**Tech Stack:** Rust 2024, Reedline 0.52.1, existing Rust/PTy tests, containerized pytest/Atuin 18.21.0.

## Global constraints

- Follow `docs/superpowers/specs/2026-10-31-emacs-macro-regression-design.md`.
- No new dependencies or public API changes.
- No Atuin-specific production code, integration/version changes, assertion weakening, or expected-failure additions.
- Bound macro bodies and compiled output; rejected replacements must not mutate existing definitions.
- Preserve map isolation and built-in vi grammar/visual state.
- Preserve published history and separate prerequisite/callback layers. Do not merge PRs.

## Task 1: Restore private Emacs compatibility translation

**Files:**
- Modify/test: `brush-interactive/src/reedline/edit_mode.rs`.
- Test: `brush-shell/tests/reedline_interactive_tests.rs`.
- Test: `brush-shell/tests/cases/compat/builtins/bind.yaml`.
- Modify/test: `brush-builtins/src/bind.rs` (reusable macro arrow formatting).
- Update: `docs/reference/compatibility.md` and `docs/reference/reedline-upstreaming.md`.

**Interfaces:**
- Consume existing `KeySequence`, `KeyAction`, `KeyMap`, and `KeyBindings` methods unchanged.
- Produce only private configuration helpers and regression tests; `UpdatableBindings::editors` continues returning built-in Reedline machines.

- [x] Add a unit test that defines an Emacs Ctrl-R macro before its intermediate target, installs/replaces/removes the raw target, and observes `None`, the first host command, the replacement host command, then `None` through `EditMode::parse_event`.
- [x] Add longest-prefix, chained-action, bounded-output, replacement-atomicity, and named-map isolation coverage. Use generic bytes such as `b"\x18\x1fprobe\x07"`, not application-specific names.
- [x] Add a real-PTY test using the unchanged builtin syntax:

  ```rust
  session.send_line(r#"bind '"\C-r":"\C-x\C-_probe\a"'; bind -x '"\C-x\C-_probe\a": printf "PREFIX_""FIRED\\n"'"#)?;
  expect_next_prompt(&mut session, 0)?;
  session.send("\x12")?;
  session.expect("PREFIX_FIRED")?;
  ```

- [x] Run the new tests and require failures at the rejected intermediate binding/macro, not setup errors:

  ```bash
  cargo test --locked -p brush-interactive --features reedline --lib emacs_prefix
  cargo test --locked -p brush-shell --test reedline_interactive_tests emacs_prefix
  ```

- [x] Retain raw intermediate actions in `KeyMapState`:

  ```rust
  raw_mappings: HashMap<Vec<u8>, KeyAction>,
  ```

- [x] Restore Emacs raw-action storage/removal and bounded longest-prefix compilation when definitions change. Compile to existing Reedline events; preserve the current literal-insertion path and vi rejection rules. Unresolved macros must shadow unrelated defaults until their targets are available. Keep composite host-event semantics explicitly eager, not resumable.
- [x] Add a compatibility case that installs/removes a generic raw intermediate shell binding and checks its macro listing/status against Bash.
- [x] Restore reusable arrow spellings in macro listings. The first restored E2E run passed search/cancellation but still failed its unchanged `"\e[A"` listing assertion because it printed `"Up"`. Add a generic arrow serialization unit test and Bash compatibility case; observe both fail before the formatting fix.
- [x] Address independent review: restore nonempty eager raw byte aliases, refresh dependencies atomically, reject cycles, and bound depth/total work/output. Add alias late-install/replacement/removal and no-output expansion tests. Fix the generic arrow decoder so saved listings round-trip for all four arrows without relying on terminfo aliases; observe unit/compat failures before the fix.
- [x] Run the focused tests, unchanged Atuin suite, all explicit Reedline library/PTy tests, and affected compatibility cases:

  ```bash
  cargo build --locked -p brush-shell
  cargo xtask test --brush-path target/debug/brush e2e atuin
  cargo test --locked -p brush-interactive --features reedline --lib
  cargo test --locked -p brush-shell --test reedline_interactive_tests
  cargo test --locked --test brush-compat-tests -- 'Builtins: bind'
  ```

- [x] Require original Atuin expectations: four passes, zero unexpected failures, one original expected failure. Update documentation to distinguish restored eager Emacs compatibility from unsupported ordered/modal replay.
- [ ] Run broader checks, review the diff, then commit with an `Assisted-by: OpenCode:gpt-6.1-sol-fast` trailer:

  ```bash
  cargo fmt --check
  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
  cargo test --locked --workspace
  git diff --check
  ```

## Task 2: Propagate and follow CI

**Files:** No further production changes unless validation identifies a specific regression.

**Interfaces:** Consume the verified prerequisite commit; preserve the callback branch's separate layer.

- [ ] Push the prerequisite follow-up normally to `origin/reassess/vi-readline-upstream`, after verifying the remote head has not advanced independently.
- [ ] Check out the clean callback branch in an isolated worktree and merge the prerequisite normally, without rebasing or force-pushing.
- [ ] Rerun its full Reedline library/PTy and unchanged Atuin suites; check formatting and strict Clippy.
- [ ] Push the callback update normally. Correct the two PR descriptions and post the actual before/after evidence, removing the suggestion that an Emacs regression is an acceptable limitation.
- [ ] Follow new fork checks and upstream approval status. Report any infrastructure/permission blockers separately; do not claim success before observing completed verification.

## Local verification record (2026-10-31)

- New prefix/alias and arrow round-trip regressions were observed failing before
  their implementation. Existing Atuin tests were not modified.
- Reedline-enabled interactive library: 66 passed.
- Builtins library: 28 passed.
- Reedline PTY suite: 14 passed, including prefix callback/buffer preservation,
  literal insertion, vi operators/visual selection, and mode-switch coverage.
- `Builtins: bind`: 16 succeeded, zero unexpected failures, two original known
  failures. The previously exempted `bind macro 1`/`bind macro 2` now pass.
- Unchanged pinned Atuin 18.21.0: four passes and the sole original expected
  failure (`target/e2e/run-9R4NnE`). Baseline Bash passed all five.
- Locked all-target/all-feature workspace strict Clippy passed. Formatting,
  crate structure, all-feature/all-target build, schema drift, and whitespace
  checks passed.
- `cargo xtask ci full --no-hooks -k` could not run its nextest-based unit and
  integration runners because `cargo-nextest` is absent. Hooks were explicitly
  skipped; no host tools installed.
- Completed `cargo test --locked --workspace --no-fail-fast`: Rust tests/doctests
  reported 832 passed, one failed, four ignored; compatibility reported 2386
  succeeded, one failed, 742 known failures, 38 skipped; brush-specific integration
  reported 159 succeeded, four known failures, one skipped. The two failed targets
  were the previously observed Bash-oracle EOF timeout in the fatal
  expansion/subshell case and `run_pipeline_interactively` timing out while
  waiting for less help. This was not a green full-workspace run.
- The unchanged pager test passed on a focused rerun with `LESS`, `LESSOPEN`,
  `LESSCLOSE`, `PAGER`, and `MANPAGER` removed from that command's environment.
  No startup files or test assertions were changed. The broader oracle timeout
  remains unresolved and is reported separately from the Emacs regression gates.
- Independent re-review found both initial medium findings resolved (eager raw
  aliases and four-arrow listing round trips), with no remaining substantive
  blocker. It independently reran the 66 interactive and 28 builtin library tests,
  arrow restoration, and rejected-replacement preservation checks.
