# zsh-style hooks (`precmd` and `preexec`)

An [experimental feature](experimental.md). Enable it persistently with
`zsh-hooks = true` under `[experimental]` in `config.toml`, or per-invocation
with `brush --enable-zsh-hooks`.

zsh defines these hooks, and prompt frameworks, command timers, and history
tools (atuin, starship, and friends) are built on them. brush provides a
bash-native adaptation: the hooks live in the shell itself, with bash semantics,
rather than in the `DEBUG`/`PROMPT_COMMAND` plumbing stock bash needs to
approximate them.

[bash-preexec](https://github.com/rcaloras/bash-preexec) is the adaptation of
the same hooks the ecosystem already builds on, so it is what settles the
details zsh leaves open for a bash shell, and what brush interoperates with —
see [below](#relationship-to-bash-preexec).

## The hooks

| Registry | When it runs | Arguments |
|----------|--------------|-----------|
| `precmd_functions` | before each prompt is displayed, ahead of `PROMPT_COMMAND` | none |
| `preexec_functions` | after a command line is read, before it runs | the command line, as `$1` |

The two arrays start out as `(precmd)` and `(preexec)`, so defining a function by
either name is enough. Register more by appending — `preexec_functions+=(my_fn)` —
and they run in array order. Each array is read once per dispatch, so a hook that
edits one is heeded from the next dispatch on.

Every entry names a **shell function**. The name is looked up as a single,
already-expanded word: never re-split, glob-expanded, or resolved against `PATH`.
An entry that isn't a shell function is skipped quietly, so a hook may be
registered before it is defined.

Hooks are active only in interactive shells, in the `$-` sense that also decides
whether rc files are loaded. `brush -s < script` does not qualify and claims none
of the bash-preexec state below; `brush -i -s < script` does, so a hook that reads
standard input there consumes the script's own lines.

`preexec` fires only for a line that runs a command: blank lines, comment-only
lines, and syntax errors dispatch nothing, matching the `DEBUG` trap bash-preexec
dispatches from. Only lines the user typed dispatch it — a command run from a key
binding does not.

## What a hook sees

- `$?`, `PIPESTATUS`, and `$_` hold what the last command left behind. All three
  are restored before every hook and again afterwards, so nothing a hook runs is
  visible to the next hook, to `PROMPT_COMMAND`, or to the next command.
- A hook that fails has its error reported; the remaining hooks still run. Under `set -e` a
  non-zero return exits the shell instead, exactly as it does from `PROMPT_COMMAND`.
- A hook that calls `exit` exits the shell. Nothing after it runs: no later hook,
  no `PROMPT_COMMAND`, and not the command line `preexec` was dispatched for.

## Relationship to bash-preexec

The two must not both be live: bash-preexec's `DEBUG` trap would dispatch the
same hooks a second time. So brush claims bash-preexec's interlock as well as
its behavior — the `bash_preexec_imported` and legacy `__bp_imported` inclusion
guards, along with the two arrays, are set before profile and rc files load. A
copy of bash-preexec sourced afterwards — including the one tools such as atuin inline into their
`init` output — finds itself already loaded and returns, leaving the hooks to
brush rather than installing its `DEBUG`-trap emulation.
[ble.sh](https://github.com/akinomyoga/ble.sh) takes the same approach.

All four are set the way `name=value` sets a variable: nothing is exported, so a
child bash still gets the real implementation, but one that already existed keeps
its attributes. A guard or registry inherited from the environment therefore stays
exported.

Because brush seeds the arrays before rc files rather than appending after them,
an rc file that appends gets `(precmd mine)`, where sourcing bash-preexec after
the same append would give `(mine precmd)`.

Clearing the guards is not a supported way to hand the hooks back. brush keeps dispatching,
so a bash-preexec sourced after they are cleared installs itself over the same registries and
every hook runs twice. To use the real bash-preexec, leave this feature off.

The claim covers bash-preexec's documented surface and no more: the two hook
names, the two arrays, and the inclusion guards. None of its `__bp_*` internals
exist, and `precmd` hooks are not reachable through `PROMPT_COMMAND` the way its
dispatcher is.

### `BP_PIPESTATUS` is not provided

bash-preexec offers `BP_PIPESTATUS` because its hooks run from `PROMPT_COMMAND`,
by which point the real `PIPESTATUS` is gone. brush restores `PIPESTATUS` itself,
so a hook reads it directly. There is no copy, and none is planned. A hook that
must work under both can fall back in one expansion:

```bash
statuses=("${BP_PIPESTATUS[@]:-${PIPESTATUS[@]}}")
```

## Differences

| | brush | zsh | bash-preexec |
|---|---|---|---|
| `preexec` `$1` | the line as typed, interior newlines included (the trailing one is trimmed) | the same | what `history 1` reports, which under `cmdhist` joins the command onto one line |
| `preexec` `$2` and `$3` | not passed | single-line and fully-expanded forms of the command | not passed |
| entries that aren't shell functions | skipped | skipped | run, when `type -t` finds a builtin or a program |
| `PIPESTATUS` inside a hook | the real one | n/a | gone; `BP_PIPESTATUS` holds a copy |

brush matches zsh on `$1`: zsh passes the string the user typed, verbatim
(verified against zsh 5.9). A hook that needs the command on one line can read
`history 1` itself — it is already in the history when `preexec` runs.

Only functions run because that is what zsh does and what bash-preexec's own
usage notes tell you to register. Admitting programs would let a misspelled or
not-yet-defined function name fall through to a same-named program on `PATH` and
run it before every prompt. Wrap such a command in a function
(`log_cmd() { logger "$1"; }`) and register that.
