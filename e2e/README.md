# End-to-end tests against real applications

Each subdirectory here runs a real application's *own* shell-integration test
suite inside a container, with the shell under test swapped in for `bash`.
This catches interactive-shell regressions (bindings, `bind -x`, `READLINE_*`,
completion, history) that unit tests can't.

```bash
cargo build --release
e2e/run.sh fzf                     # against target/release/brush (or debug)
e2e/run.sh --shell bash fzf        # baseline: the same suite against real bash
e2e/run.sh --shell /path/to/sh fzf # any other binary
e2e/run.sh fzf -n /ctrl_r/         # extra args go to the app's test runner
```

Results land in `target/e2e/<app>/`: `log.txt` (full runner output) and
`junit/*.xml` (JUnit, for CI upload). The exit status is the suite's.

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
   written in bash (bats, say), which must not run under the shell under test.
2. **Writes results** to `/results`: `log.txt` plus JUnit XML under
   `junit/`. Write whatever else helps debugging there too.
3. **Exits non-zero** when any test fails. Known failures go in the adapter's
   `skip-list.txt` (one test per line, with a comment saying why); the entrypoint
   excludes them from the run.

The container runs as the host user with `HOME=/tmp`, so `chmod` anything
the tests write into.

## Rules

1. **Never copy upstream files into this tree.** Clone the application at
   build time, pinned to a commit (not a branch).
2. **The Dockerfile is our own environment recipe.** Deriving its package
   list from upstream's Dockerfile or CI config is fine; say so in a comment,
   with the source and license.
3. **The entrypoint adapts upstream's runner to the results contract.** If the
   runner can't emit JUnit itself, add a reporter there (fzf: `minitest-ci`).
4. **Not every app has a suite to borrow.** When upstream only tests its own
   code (atuin, say), the adapter brings its own tests: a small tmux-driven
   script that installs the app's hooks into the shell under test and checks
   the observable behavior. The contract is the same; only the author differs.

## Adapters

| app | notes |
|-----|-------|
| fzf | upstream `test/test_shell_integration.rb`, `TestBash` only; minitest + tmux, JUnit via `minitest-ci` |
| atuin | our own suite in `atuin/tests/` (bats + tmux), written to be upstreamable |

Planned: ble.sh.
