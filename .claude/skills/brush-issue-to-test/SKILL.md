---
name: brush-issue-to-test
description: Work a brush bug report end-to-end -- reproduce it, shrink it into YAML compat regression tests, prove those tests fail, fix the bug, and prove they pass. Use when handed a brush GitHub issue or a "X behaves differently from bash" report and you want the investigation to leave behind permanent regression tests, not just a fix.
---

# Brush: from issue to regression test to fix

A disciplined loop for turning a brush compatibility bug into a durable fix backed by
tests. The non-negotiable principle: **every bit of sleuthing must end up captured as a
test that fails before the fix and passes after.** A fix without a failing-then-passing
test is unfinished work.

## The loop

1. **Read the issue.** Identify every distinct symptom. Reports often bundle several bugs;
   list them separately, because each needs its own reproduction and its own test.
2. **Reproduce the simplest form.** Don't start from the user's full scenario. Strip it to
   the smallest shell snippet that still shows the symptom, then confirm bash does *not*
   show it. If the real program is available, run it under the locally built brush to see
   the raw failure first, then shrink toward a minimal case.
3. **Reduce to YAML compat cases.** Translate each minimal repro into a case under
   `brush-shell/tests/cases/compat/`. The harness runs each case under both brush and bash
   and diffs stdout/stderr/exit-code, so a good case is one where pre-fix brush diverges
   from bash.
4. **Prove the tests fail.** Run them against the *current, unfixed* brush. If a case
   passes pre-fix, it isn't capturing the bug -- make it sharper or more deterministic.
5. **Fix it.** Root-cause, then fix. Note any *other* code paths the same root cause
   touches, and reproduce/test those too.
6. **Prove the tests pass**, then run the whole compat suite to check for regressions.

## Building and running

```bash
cargo build -p brush-shell                      # target/debug/brush

# Run compat tests; trailing args are substring filters over case names.
cargo test -p brush-shell --test brush-compat-tests -- "Some case name" "another"
# --verbose shows each case's pass/fail line; --skip <pat> excludes; no filter = whole suite.
```

The harness invokes both shells with default args `--norc --noprofile` and appends each
case's `args`. Reproduce manually with the same flags:
`target/debug/brush --noprofile --norc -c '...'` versus `bash --noprofile --norc -c '...'`.

Brush diagnostics: `--debug <category>` (arithmetic, commands, complete, expand, functions,
input, jobs, parse, pattern, tokenize, unimplemented).

## YAML case schema (the fields that matter)

Cases live in `*.yaml` files (loaded recursively) as `{ name, cases: [...] }`. Per case:

- `args: ["-c", "..."]` -- argv appended after the default flags. `-c` scripts get job
  control *off* by default; add `set -m` inside the script to turn it on.
- `stdin: |` -- feed a script on stdin instead (non-interactive, job control off).
- `known_failure: true` -- documents a case brush currently gets wrong. Flip to false (or
  delete the line) in the same commit that fixes it.
- `ignore_stderr` / `ignore_stdout` / `ignore_exit_status` -- relax a comparison axis.
  Prefer *not* setting these for a regression test: comparing stderr is often what catches
  the bug. Only relax an axis that legitimately differs for reasons unrelated to the bug.
- `pty: true` -- needs a pseudo-terminal (genuinely interactive behavior).
- `min_oracle_version` / `max_oracle_version`, `incompatible_os`, `incompatible_configs` --
  scope a case to environments where bash's behavior is stable/comparable. Prefer this over
  weakening an assertion when you hit a version-specific bash quirk.

Make outputs **deterministic**: sort unordered output (`| sort`), count it (`| wc -l | tr -d
' '`), or gate concurrency with `wait`. Background jobs interleave nondeterministically
otherwise.

## Proving pre-fix failure without throwing away your fix

You usually write the tests and the fix together, then need to show the tests fail on the
*old* code. Stash only the source files, leaving the test files in place:

```bash
git stash push -m wip <source-file> ...      # leaves new (untracked) test YAMLs untouched
cargo build -p brush-shell
cargo test -p brush-shell --test brush-compat-tests -- "<your case names>"   # expect FAIL
git stash pop && cargo build -p brush-shell
cargo test -p brush-shell --test brush-compat-tests -- "<your case names>"   # expect PASS
```

`git stash push <paths>` stashes only those tracked paths; brand-new test files are
untracked and stay put, so the same tests run against both old and new code.

## One architectural fact that explains many bugs

Brush runs `( ... )`, command substitutions, background jobs (`&`), and process
substitutions as **in-process tasks that clone the shell, not `fork(2)` children.** When a
construct behaves differently from bash, ask whether the divergence comes from this:
shared process-global state (file descriptors, the environment, signal state) where bash
would have an isolated child, and per-clone job tables that must not surface their jobs to
the parent. Relevant files: `brush-core/src/interp.rs` (subshell / cmd-subst / process-subst
spawning, redirects), `brush-core/src/commands.rs` (function & command-substitution
invocation), `brush-core/src/jobs.rs` + `brush-core/src/shell/job_control.rs` (jobs),
`brush-core/src/openfiles.rs` (descriptors).

## Don't forget

- A bundled report = multiple bugs. Fix and test each independently.
- When the symptom is resource-based (descriptors, memory, processes), measure rather than
  guess: brush is single-process, so e.g. `/proc/<pid>/fd` shows the whole picture, and a
  workload + `ulimit` can turn a resource bug into a deterministic pass/fail test.
- After fixing, run the **full** compat suite -- a shared-state change can ripple into
  unrelated constructs.
