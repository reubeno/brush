# Shell integration tests

End-to-end tests for `atuin init <shell>`, driven through a real terminal: each test
starts an interactive shell inside tmux with the integration loaded, sends keystrokes,
and checks the screen and the history database.

```bash
bats tests/                 # needs bats, tmux, atuin on PATH
bats tests/bash.bats -f history
```

Environment:

- `ATUIN_TEST_BASH` — bash binary to test (default: `bash` from `PATH`).
- `ATUIN_TEST_TIMEOUT` — seconds to wait for a screen condition (default: 10).
- `ATUIN_TEST_SKIP` — newline-separated test names to skip.

Each test gets a fresh `HOME`, so atuin's config and database are isolated and no
sync is configured. `tests/helpers/shell.bash` holds the tmux driver; per-shell test
files (`bash.bats`) hold the scenarios.
