# `bash` Compatibility Reference

This document details `brush`'s compatibility with `bash`, including supported features, known limitations, and how to report issues.

## Overview

`brush` aims for high compatibility with `bash`. We validate this through **1400+ compatibility test cases** that compare behavior against `bash` as an oracle.

**Compatibility snapshot:** Production-ready for most use cases. Your `.bashrc`, aliases, functions, and completions should "just work."

## Fully Supported Features ✅

### Shell Syntax & Control Flow

- `if`/`then`/`elif`/`else`/`fi` conditionals
- `for`, `while`, `until` loops
- Arithmetic `for` loops: `for ((i=0; i<10; i++))`
- `case`/`esac` pattern matching
- `&&`, `||` conditional execution
- Subshells `()` and command grouping `{}`
- Pipelines and pipeline negation `!`

### Expansions

- Brace expansion: `{a,b,c}`, `{1..10}`, `{a..z}`
- Parameter expansion: `${var:-default}`, `${var:+set}`, `${var#pattern}`, `${var%pattern}`, `${var//find/replace}`, etc.
- Command substitution: `$(cmd)`, `` `cmd` ``
- Arithmetic expansion: `$((expr))`
- Process substitution: `<(cmd)`, `>(cmd)`
- Tilde expansion: `~`, `~user`
- Globbing: `*`, `?`, `[...]`
- Extended globbing: `?(pat)`, `*(pat)`, `+(pat)`, `@(pat)`, `!(pat)`
- `globstar`: `**` recursive matching

### Builtins (50+)

- **I/O:** `echo`, `printf`, `read`, `mapfile`/`readarray`
- **Variables:** `declare`, `local`, `export`, `unset`, `readonly`, `typeset`
- **Control:** `break`, `continue`, `return`, `exit`
- **Navigation:** `cd`, `pushd`, `popd`, `dirs`, `pwd`
- **Jobs:** `jobs`, `fg`, `bg`, `wait`, `kill`
- **Completion:** `complete`, `compgen`, `compopt`
- **History:** `history`, `fc`
- **Testing:** `test`, `[`, `[[`
- **Sourcing:** `.`, `source`, `eval`
- **Misc:** `alias`, `unalias`, `hash`, `type`, `command`, `builtin`, `enable`, `help`, `times`, `ulimit`, `umask`, `trap`, `shopt`, `set`, `shift`, `getopts`

### Arrays

- Indexed arrays: `arr=(a b c)`, `${arr[0]}`, `${arr[@]}`
- Associative arrays: `declare -A map`
- Array slicing: `${arr[@]:start:length}`
- Array operations: `${#arr[@]}`, `${!arr[@]}`, `${!arr[*]}`

### Job Control

- Background execution: `cmd &`
- Suspend/resume: Ctrl+Z, `fg`, `bg`
- Job listing: `jobs`
- Process groups and pipelines

### Dynamic Variables

- `RANDOM`, `SRANDOM`
- `LINENO`, `FUNCNAME`, `BASH_SOURCE`
- `EPOCHSECONDS`, `EPOCHREALTIME`
- `SECONDS`
- `PWD`, `OLDPWD`
- `BASH_VERSINFO`, `BASH_VERSION`

### Programmable Completion

- Compatible with [`bash-completion`](https://github.com/scop/bash-completion)
- Git, Docker, systemctl, etc. completions work out of the box
- `complete`, `compgen`, `compopt` builtins

### Redirection

- Standard: `>`, `>>`, `<`, `2>&1`
- Here documents: `<<EOF`, `<<-EOF` (tab-stripped), `<<<` (here strings)
- File descriptor manipulation: `>&n`, `<&n`, `n>&m`
- Process substitution redirects: `>(cmd)`, `<(cmd)`
- Clobber control: `>|`, `set -o noclobber`

## Partially Supported Features 🔷

### Traps

| Status | Feature |
|--------|---------|
| ✅ | `EXIT` trap |
| ✅ | `DEBUG` trap (basic) |
| 🔷 | Signal traps (`SIGINT`, `SIGTERM`, etc.) — in progress |
| 🚧 | `ERR` trap — not yet implemented |

### Key Bindings (`bind`)

| Status | Feature |
|--------|---------|
| ✅ | Basic `bind` support |
| ✅ | `bind -x` for custom key-bound commands |
| 🔷 | Advanced bind features — in progress |

### Shell Options

| Status | Feature |
|--------|---------|
| ✅ | Common options: `errexit`, `pipefail`, `extglob`, `globstar`, `noclobber`, `nounset` |
| 🔷 | Less common options — in progress |

## Not Yet Supported Features 🚧

These features are on our roadmap but not yet implemented:

### `select` Statement

The `select` builtin for creating menu-driven scripts is not yet implemented.

```bash
# Not yet supported
select opt in "Option A" "Option B" "Quit"; do
    case $opt in
        "Option A") echo "A";;
        "Option B") echo "B";;
        "Quit") break;;
    esac
done
```

### `coproc` (Coprocesses)

The `coproc` keyword for creating asynchronous co-processes is not yet implemented.

```bash
# Not yet supported
coproc { some_command; }
echo "input" >&${COPROC[1]}
read output <&${COPROC[0]}
```

### `wait -n`

The `wait -n` option to wait for the next background job to complete is not implemented.

```bash
# Not yet supported
job1 &
job2 &
wait -n  # Wait for whichever finishes first
```

### `BASH_COMMAND` Variable

The special variable `BASH_COMMAND` that contains the currently executing command is currently only available in trap contexts.

### `disown` and `logout`

These job control builtins are not yet implemented.

## Known Edge Cases

These areas have known differences from `bash` in edge cases. Most users won't encounter these, but they're documented for completeness.

### IFS (Input Field Separator)

There are ~10 known edge cases where IFS word splitting behavior differs from `bash`, particularly around:

- Non-whitespace IFS characters and empty field creation
- Mixed whitespace and non-whitespace IFS
- Leading/trailing delimiter handling

### `printf` Format Specifiers

Some advanced `printf` format specifiers behave differently (~8 known cases), particularly features not supported by the underlying `uucore` library.

### Arithmetic Expressions

- Division by zero handling may differ in `errexit` mode
- `$(( exit N ))` syntax edge case

### Aliases

Some complex alias expansion scenarios differ from `bash` (see GitHub issues #57, #286).

## Test Suite Statistics

- **Total test cases:** 1400+
- **Known failures:** ~81 (~5.8%)
- **Most failures are edge cases** in IFS handling, ERR traps, and printf

The test suite runs on every PR and compares behavior against `bash` as an oracle.

## Version Compatibility

`brush` targets compatibility with **`bash` 5.3+**. Behavior may differ from older `bash` versions (3.x, 4.x) in some areas.

## Reporting Compatibility Issues

Found a script that works in `bash` but not in `brush`?

1. **Check existing issues:** [GitHub Issues](https://github.com/reubeno/brush/issues)
2. **Create a minimal reproducer:** Reduce to the smallest failing script
3. **File an issue** with:
   - The script or command that fails
   - Expected behavior (what `bash` does)
   - Actual behavior (what `brush` does)
   - Your platform (Linux/macOS/etc.)

## Tracking Progress

- **GitHub Issues:** Track specific compatibility work
- **Test Suite:** 1400+ tests run on every PR
- **This Document:** Updated as features are implemented

## Related Resources

- [`bash` Reference Manual](https://www.gnu.org/software/bash/manual/)
- [POSIX Shell Specification](https://pubs.opengroup.org/onlinepubs/9699919799/)
- [`brush` Test Cases](https://github.com/reubeno/brush/tree/main/brush-shell/tests/cases)
