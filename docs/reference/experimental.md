# Experimental Features

`brush` ships several features that are intentionally marked as
**experimental**. They are usable today, but their interface or behavior
may evolve based on feedback before being stabilized. This page is the
canonical index of what's currently experimental and how to opt in.

Experimental features fall into two categories:

1. **Build-time experiments** — additional functionality gated behind
   Cargo feature flags. To get them, you build `brush-shell` with the
   relevant feature(s) enabled.
2. **Run-time experiments** — features that ship in standard builds but
   are off by default and enabled through configuration.

> Names, defaults, and semantics of experimental features may change
> between releases.

---

## Build-time experiments (Cargo features)

These are flags on the `brush-shell` crate. You can enable them
individually, or pull in the whole set with the umbrella `experimental`
feature.

```bash
# Enable everything experimental
cargo install --locked brush-shell --features experimental

# Or pick and choose
cargo install --locked brush-shell --features experimental-bundled-coreutils
```

### `experimental-bundled-coreutils`

Bundles a configurable subset of [`uutils/coreutils`](https://github.com/uutils/coreutils)
implementations directly into `brush-shell` as builtins. Useful when:

- shipping `brush` into containers, embedded systems, or other
  environments where a standalone coreutils package is inconvenient or
  unavailable,
- distributing a single self-contained `brush` binary that doesn't
  rely on host utilities being present.

When enabled (via the umbrella `experimental` feature, or directly), the
full set of supported utilities is bundled. When building the
`brush-coreutils-builtins` crate directly, individual utilities can be
selected via `coreutils.<name>` features (e.g., `coreutils.cat`,
`coreutils.ls`); the `coreutils.all` feature enables all of them.

Bundled utilities run in-process and take precedence over external
executables of the same name on `PATH` when invoked unqualified. As with
any builtin, you can bypass the in-process implementation with
`command <name>` or by giving an explicit path.

### `experimental-builtins`

Pulls in the [`brush-experimental-builtins`](../../brush-experimental-builtins)
crate, which provides additional builtins that are too new or too
narrow-purpose to ship in the default builtin set. Currently this
includes:

- **`save`** — serializes the current shell state to JSON on stdout.
  Primarily intended for debugging and tooling. ⚠️ The serialized state
  may include sensitive information (variable values, command history,
  environment).

### `experimental-load`

Enables `serde`-based serialization support in `brush-core`, and adds a
`--load <FILE>` command-line flag that restores shell state from a JSON
file previously produced by the experimental [`save`](#experimental-builtins)
builtin (or by other tooling that emits the same format). State loaded
this way overrides non-UI command-line options. Useful for tooling
built on top of `brush-core` that wants to snapshot or transfer shell
state.

### `experimental-parser`

Switches on the in-development [`winnow`](https://crates.io/crates/winnow)-based
parser scaffolding in `brush-parser`, and adds an `--experimental-parser`
command-line flag that selects it at runtime. This parser is not yet the
production parser; the existing PEG parser remains the default. Enable
this only if you're contributing to or experimenting with the parser
work.

---

## Run-time experiments (configuration)

These features ship in standard builds but are off by default. Each one
can be enabled either through the optional [TOML configuration
file](configuration.md) (under the `[experimental]` section, persistent
across sessions) or through a command-line flag at startup (one-shot).

When both are specified, command-line flags take precedence over the
configuration file.

```toml
[experimental]
zsh-hooks = true
terminal-shell-integration = true
```

| Feature | TOML setting (`[experimental]`) | Command-line flag |
|---------|---------------------------------|-------------------|
| zsh-style hooks | `zsh-hooks = true` | `--enable-zsh-hooks` |
| Terminal shell integration | `terminal-shell-integration = true` | `--enable-terminal-integration` |

### `zsh-hooks`

Enables zsh-style `preexec` and `precmd` hook functions. When set:

- A function named `preexec` (if defined) is invoked before each
  interactively-entered command runs, with the command line as `$1`.
- A function named `precmd` (if defined) is invoked just before each
  prompt is displayed.

This is convenient for prompt frameworks, command timing, and
integrations that expect zsh-style hook conventions. Equivalent
behavior in stock bash typically requires `DEBUG`/`PROMPT_COMMAND`
plumbing.

Enable persistently with `zsh-hooks = true` under `[experimental]` in
`config.toml`, or per-invocation with `brush --enable-zsh-hooks`.

### `terminal-shell-integration`

Emits standard terminal shell-integration escape sequences (semantic
prompt and command boundary marking) that modern terminal emulators —
including VS Code, iTerm2, WezTerm, and others — use to enable features
like command navigation, exit-status display, and selective output
copying. This is off by default to avoid emitting escape sequences in
terminals that don't recognize them.

Enable persistently with `terminal-shell-integration = true` under
`[experimental]` in `config.toml`, or per-invocation with
`brush --enable-terminal-integration`.

---

## Reporting feedback

Experimental features are the place where your feedback is most valuable
— they exist precisely because we want to iterate on them before
stabilizing. If you try one and find a rough edge, missing capability,
or behavior that surprises you, please [file an
issue](https://github.com/reubeno/brush/issues).
