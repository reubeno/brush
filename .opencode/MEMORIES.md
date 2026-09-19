# brush Repository Guide

## Remotes

- **`origin`** = `reubeno/brush.git` (upstream) — PRs go here
- **`mine`** = `lu-zero/brush.git` (fork) — **push branches here**

## Workflow

1. Create feature branch
2. `git push -u mine <branch>`
3. `gh pr create --repo reubeno/brush --head lu-zero:<branch> --base main ...`

## Key Branches

- `main` — tracks upstream `origin/main`
- `for-portage-repo` — portage-repo's dependency branch (rebased onto feature branches as needed)
- `stateful-builtins` — stateful builtin infrastructure (PR #1151)

## portage-repo Integration

`../portage-repo` depends on this repo via path deps. After changes here:
1. Rebase `for-portage-repo` onto the new feature branch
2. Run `cargo check` / `cargo test` in portage-repo to verify
3. Run `cargo run --example regen_cache -- gentoo 'dev-lang/python*' --jobs 4` for full integration test
