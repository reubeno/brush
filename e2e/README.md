# End-to-end tests against real applications

Each subdirectory here runs a real application's *own* shell-integration test
suite inside a container, with the shell under test swapped in for `bash`.
This catches interactive-shell regressions (bindings, `bind -x`, `READLINE_*`,
completion, history) that unit tests can't.

```bash
cargo build
cargo xtask test e2e fzf                     # one adapter against brush
cargo xtask test e2e                         # default adapters (excludes blesh)
cargo xtask test e2e blesh                   # opt in to the slow ble.sh suite
cargo xtask test e2e --baseline fzf          # baseline against Bash
cargo xtask test e2e --shell /path/to/sh fzf # any other binary
cargo xtask test e2e fzf -- -n /ctrl_r/      # adapter-specific arguments
cargo xtask test --release e2e fzf           # use target/release/brush
cargo xtask test e2e --verbose fzf           # stream the runner's own output
cargo xtask test e2e --timeout 600 fzf      # bound each adapter (default 1800s)
```

A run reports one line per adapter and a summary, read back from the JUnit
each runner writes:

```
    Starting 7 e2e adapter(s) against brush
     Results target/e2e/run-ABC123
        PASS [    1.2s] starship  3 tests
        FAIL [   13.3s] atuin     5 tests, 1 failed
        PASS [  542.2s] blesh     9 tests, 9 xfailed
------------
     Summary [ 11m35.0s] 7 adapter(s): 6 passed, 1 failed

  atuin     failed: ctrl-r search runs the selected command
            -> target/e2e/run-ABC123/atuin/log.txt
```

Results land in `target/e2e/run-<id>/<app>/`: `log.txt` (full runner output),
`junit/*.xml` (JUnit, for CI upload), and any adapter-specific diagnostic logs
(such as PTY transcripts). Each invocation creates and prints a fresh run directory,
retaining previous runs and leaving existing files untouched, including when using
`--results-dir`. Missing, malformed, or empty JUnit reports fail the adapter, as does an adapter that runs longer
than `--timeout` (default 1800s): its container is killed, so a hung shell cannot hang the run.
The runner's own output goes to `log.txt` rather than the terminal; `--verbose`
streams it instead.
Multi-adapter runs continue after failures and report them together at the end.
On a terminal, statuses are green for clean passes, yellow for passes with expected
failures, and red for failures. Redirected output, `TERM=dumb`, and a nonempty
`NO_COLOR` disable color.

These suites are diagnostic tools, not gates: nothing in CI runs them, and they
need Docker and a built shell binary. `blesh` is excluded from the default selection
because compatibility failures and timeouts make it take several minutes against
brush. Run it explicitly with `cargo xtask test e2e blesh`, or select one file with
`cargo xtask test e2e blesh -- util`. Its per-file timeout remains 180 seconds.

## Adapter contract

An adapter is a directory `e2e/<app>/` containing a `Dockerfile` (build
context is `e2e/`, so it can `COPY shim /e2e/bin`) whose entrypoint:

1. **Receives the shell** via `$SHELL_UNDER_TEST`, an absolute path to a
   binary bind-mounted into the container (`/shell/<name>`). It's built on
   the host, so the image's glibc must be at least the host's. Most apps
   hard-code `bash`, so the shared `shim/bash` exec's `$SHELL_UNDER_TEST`;
   put `/e2e/bin` first on `PATH`. When the variable is unset the shim
   runs the real `bash`, which gives the baseline run. If the suite has its
   own knob for which shell to test (atuin's `ATUIN_TEST_BASH`), set that
   instead and skip the shim; a shim on `PATH` also captures test tooling
   written in bash (tmux, say), which must not run under the shell under test.
   A suite that takes a shell *path* has nowhere to put a command-line flag; turn
   the behavior on through brush's config file (`$HOME/.config/brush/config.toml`,
   and `HOME` is `/tmp` in the container) instead, which bash ignores and so leaves
   the baseline run alone.
2. **Writes results** to `/results`: `log.txt` plus JUnit XML under
   `junit/`. Write whatever else helps debugging there too.
3. **Exits non-zero** when any test fails. Entrypoints pipe the runner through
   `tee`, so exit `${PIPESTATUS[0]}` rather than `$?`, which is `tee`'s status.
   Known failures are not the adapter's problem: the runner reads them from
   `xfail-list.txt` (below) and decides the result from the JUnit, so an adapter that
   exits non-zero purely because of expected failures still passes.

The container runs as the host user with `HOME=/tmp`, so `chmod` anything
the tests write into.

## Tracking known failures

Two lists per adapter, both optional, both `#`-commented, one test name per line.

`xfail-list.txt` is the default home for anything that fails today. The test still **runs**;
the runner matches it against the JUnit and counts it as `xfailed` instead of a failure.
Crucially it also fails the run when such a test **passes** ("passed unexpectedly") or when an
entry **matches no test that ran** ("stale xfail entry" -- renamed, removed, or moved to
`skip-list.txt`), so a gap that closes is reported rather than absorbed, and the list cannot rot.
Stale detection needs the whole suite, so it is skipped when adapter arguments select a subset. Because the matching happens over JUnit, one mechanism
covers every runner. Name a test by the `name` its JUnit carries, or as `classname::name`.
These expectations apply only to brush runs; `--baseline` and `--shell` runs use
no brush expectations and fail on any reported test failure.

`skip-list.txt` is for tests that must **not** run: they hang, or they cannot exercise the shell
at all (nvm has one with a `/bin/zsh` shebang). This is the exception -- prefer `xfail-list.txt`,
because a skipped test reports nothing when it starts working -- and listing one in both files
fails the run as a stale xfail entry. The runner reports skipped tests by name, so a suite that
starts skipping something itself does not pass unnoticed.

Applying the list is mostly handled for you: `lib/run_pytest.sh` exports the file as
`$E2E_SKIP_LIST` and loads `lib/e2e_skip.py`, a pytest plugin that skips any test named in it.
Adapters that must decide before collection (blesh) or at image build time (nvm, which takes the
execute bit off) parse `$E2E_SKIP_LIST` themselves; fzf turns it into a minitest `--exclude`
regex, since its runner is upstream's.

Every entry in either list needs a comment saying why, with an issue link where one exists.

## Shared adapter code

Every adapter's own tests are pytest, so `lib/` (copied to `/e2e/lib`, on `PYTHONPATH`) carries
what they would otherwise each reinvent:

| | |
|-----|-------|
| `run_pytest.sh` | the pytest half of the contract: JUnit, log, skip list, exit status. Source it and call `e2e_run_pytest <report name>` |
| `rcfile.py` | `write_rc()`: a known prompt and no `HISTFILE`, then the app's own integration lines |
| `pty_shell.py` | drives a shell over a raw PTY (pexpect) and matches its output stream. The default |
| `tmux_shell.py` | drives a shell inside tmux (via `libtmux`) and matches the *rendered screen*. For full-screen TUIs, whose redraws a byte stream cannot answer questions about |
| `e2e_skip.py` | the skip-list pytest plugin, plus `parse()` for adapters that must skip before collection |

## Rules

1. **Never copy upstream files into this tree.** Clone the application at
   build time, pinned to a commit (not a branch).
2. **The Dockerfile is our own environment recipe.** Deriving its package
   list from upstream's Dockerfile or CI config is fine; say so in a comment,
   with the source and license.
3. **The entrypoint adapts upstream's runner to the results contract.** If the
   runner can't emit JUnit itself, add a reporter there (fzf: `minitest-ci`).
4. **Not every app has a suite to borrow.** When upstream only tests its own
   code (atuin, say), the adapter brings its own tests: a small terminal-driven
   suite that installs the app's hooks into the shell under test and checks the
   observable behavior. The contract is the same; only the author differs.

## Adapters

| app | notes |
|-----|-------|
| fzf | upstream `test/test_shell_integration.rb`, `TestBash` only; minitest + tmux, JUnit via `minitest-ci` |
| atuin | our own suite in `atuin/tests/` (pytest + tmux), written to be upstreamable. atuin's bash integration is bash-preexec, so the adapter turns on brush's `zsh-hooks` through a config file to exercise the native hooks rather than the `DEBUG`-trap emulation |
| blesh | upstream `ble.sh --test`, one pytest case per test section with a process-group timeout (`BLESH_TEST_TIMEOUT`, default 180s); sections are read from ble.sh's own build output, so a section added upstream runs rather than being missed; full output and leftover per-section artifacts are retained; args select sections: `cargo xtask test e2e blesh -- util`. Every section is currently expected to fail: ble.sh gets far enough to emit no section summary at all |
| mise | selected upstream bash activation tests (pytest), run directly against the shell under test |
| nvm | upstream `test/fast/Listing versions` suite (urchin) plus interactive regression coverage for [#1173](https://github.com/reubeno/brush/issues/1173) |
| starship | our own suite in `starship/tests/` (pytest + direct PTY) |
| zoxide | our own suite in `zoxide/tests/` (pytest + direct PTY) |
