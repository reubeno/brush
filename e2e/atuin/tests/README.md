# Shell integration tests

End-to-end tests for `atuin init <shell>`, driven through a real terminal: each test
starts an interactive shell inside tmux with the integration loaded, sends keystrokes,
and checks the screen and the history database.

```bash
pytest tests/               # needs pytest, tmux, atuin on PATH
pytest tests/ -k history
```

Environment:

- `ATUIN_TEST_BASH` — bash binary to test (default: `bash` from `PATH`).
- `E2E_TEST_TIMEOUT` — seconds to wait for a screen condition (default: 10).

Each test gets a fresh `HOME`, so atuin's config and database are isolated and no
sync is configured. The tmux driver is `lib/tmux_shell.py`, shared with the other
suites in this harness; per-shell test files (`test_atuin.py`) hold the scenarios.
