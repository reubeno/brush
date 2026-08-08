# `declare -a`/plain array assignment on a dynamic well-known variable (e.g. `PIPESTATUS`) permanently freezes it

## Summary

Once a script `declare -a`s or plain-array-assigns a dynamic well-known variable such as `PIPESTATUS`, brush **never updates it again** on later pipelines in that shell — it stays frozen at whatever was last assigned. Real bash keeps the variable live and refreshes it on every pipeline.

## Minimal repro

```bash
declare -a PIPESTATUS=([0]="1")
true | true | true
echo "${PIPESTATUS[@]}"
```

- **bash**: `0 0 0`
- **brush** (current `origin/main`): `1`

The same freeze happens with a plain array assignment (no `declare`):

```bash
PIPESTATUS=([0]="9")
true | false | true
echo "${PIPESTATUS[@]}"
```

- **bash**: `0 1 0`
- **brush**: `9`

## Root cause

`PIPESTATUS` is implemented as a dynamic well-known variable backed by a `ShellValue::Dynamic { getter, setter }` (`brush-core/src/wellknownvars.rs`). Reads go through `getter` (which calls `shell.last_pipeline_statuses()` live), and every write is supposed to be a no-op via `setter: |_| ()`. But two write paths in `brush-core/src/variables.rs` unconditionally overwrite `self.value` with a static array, **discarding the `Dynamic` binding**:

1. `ShellVariable::convert_to_indexed_array` — the `_` arm hits for a `Dynamic` value and replaces `self.value` with `ShellValue::IndexedArray(...)`. Triggered by `declare -a` via `brush-builtins/src/declare.rs`.
2. `ShellVariable::assign` (non-append Array-literal arm) — the match explicitly includes `ShellValue::Dynamic { .. }` in the list of variants that get overwritten with `ShellValue::indexed_array_from_literals(...)`. Triggered by a plain `VAR=([0]="...")` assignment.

From that point on the variable is just an ordinary frozen array; nothing in the pipeline-execution path ever calls the getter again, because the getter closure is gone.

The same class of issue affects every dynamic well-known variable (`RANDOM`, `SECONDS`, `FUNCNAME`, `BASH_LINENO`, `BASH_SOURCE`, `BASH_ALIASES`, etc.) — all of them register `setter: |_| ()`, so the setter field exists precisely for this case but isn't consulted on these two paths.

## Suggested fix

In both paths, route `ShellValue::Dynamic` through the setter (or simply no-op it) instead of overwriting `self.value`. Concretely, in `brush-core/src/variables.rs`:

- `convert_to_indexed_array`: add an explicit `ShellValue::Dynamic { .. } => Ok(())` arm before the `_` arm.
- `assign`: drop `ShellValue::Dynamic { .. }` from the variants that get converted to a static indexed array; the existing `(ShellValue::Dynamic { .. }, _) => Ok(())` arm below already does the right thing.

### Note on `declare -A` / `convert_to_associative_array`

`convert_to_associative_array` has the same overwrite pattern, but real bash is asymmetric here: `declare -A PIPESTATUS=([x]="9")` **does** permanently freeze it in bash itself (PIPESTATUS becomes a plain associative array that pipelines don't refresh). Pre-fix brush already matches bash's `-A` behavior, so it's intentionally left unchanged. (One could argue keeping dynamic vars live even after `declare -A` is more internally consistent for brush, but it would diverge from bash compat.)

## Why this matters

Any script that explicitly `declare -a`s `PIPESTATUS` (or any other dynamic well-known variable) will silently get a stale/wrong value for the rest of that shell's life. This surfaced in the wild when a worker-env dump/restore mechanism restored a `declare -a PIPESTATUS=([0]="1")` line and a downstream `pipestatus || die` check fired incorrectly, misreporting a successful pipeline as failed.

## Environment

- brush: current `origin/main` (`4ff5cc83`)
- bash: 5.x (verified against 5.2/5.3)
