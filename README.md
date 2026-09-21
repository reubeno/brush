<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/extras/brand/brush-wordmark-white.svg">
    <img src="docs/extras/brand/brush-wordmark-black.svg" alt="brush" width="380">
  </picture>
</div>

<p align="center"><em>Bash-compatible. Embeddable. Extensible.</em></p>

<p align="center">
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/v/brush-shell?style=flat-square" alt="crates.io version"/></a>
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/d/brush-shell?style=flat-square" alt="crates.io downloads"/></a>
  <a href="https://github.com/reubeno/brush/actions/workflows/ci.yaml"><img src="https://img.shields.io/github/actions/workflow/status/reubeno/brush/ci.yaml?branch=main&style=flat-square&label=CI" alt="CI status"/></a>
  <a href="brush-shell/tests/cases"><img src="https://img.shields.io/badge/compat_tests-2%2C500%2B-0d9488?style=flat-square" alt="2,500+ compatibility tests"/></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-0d9488?style=flat-square" alt="MIT license"/></a>
  <a href="https://discord.gg/kPRgC9j3Tj"><img src="https://img.shields.io/badge/discord-join-5865F2?style=flat-square&logo=discord&logoColor=white" alt="Discord"/></a>
</p>

<p align="center">
  <a href="https://brush.sh">Website</a> ·
  <a href="https://brush.sh/getting-started/install/">Install</a> ·
  <a href="https://brush.sh/reference/compatibility/">Compatibility</a> ·
  <a href="https://brush.sh/releases/">Release notes</a> ·
  <a href="https://discord.gg/kPRgC9j3Tj">Discord</a>
</p>

<hr/>

[brush](https://brush.sh) is a Bash-compatible shell written in Rust.

***Run it as your everyday shell.*** It loads your existing `.bashrc`, aliases, functions, and completions, runs the scripts you already have, and adds some modern shell amenities: history-based suggestions and live syntax highlighting.

***Build with it.*** [`brush-core`](https://docs.rs/brush-core) is a tested implementation of Bash semantics that your own Rust software can embed and extend. [`brush-parser`](https://docs.rs/brush-parser), which turns shell source into a syntax tree, is used on its own by other projects, including [Zed](https://github.com/zed-industries/zed), [Vite+](https://github.com/voidzero-dev/vite-plus), and [oh-my-pi](https://github.com/can1357/oh-my-pi), to parse shell commands.

<p align="center">
  <img src="https://github.com/user-attachments/assets/0e64d1b9-7e4e-43be-8593-6c1b9607ac52" alt="brush in action: bash-completion, job control, and shell functions" width="80%"/>
</p>

## Why brush

Bash is the shell most of us already know. It's in our fingers, our scripts, and our team's runbooks. brush keeps that behavior as it is, and adds suggestions, highlighting, and the other modern conveniences for those who want them. Keeping that promise means being faithful, so every change is tested against Bash itself.

The brush shell and its libraries are the same code. Embedding `brush-core` gives your software the behavior the shell has, and an improvement to one is an improvement to the other. brush is written in Rust with extension points designed in: custom builtins already plug in alongside the standard ones, and we intend to open more of the shell's internals the same way, so that tools can observe and extend it natively.

## Get started

Install the latest release on Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://brush.sh/install.sh | sh
```

Or with Homebrew or cargo:

```sh
brew install brush                    # Homebrew, on macOS or Linux
cargo install --locked brush-shell    # build from crates.io
cargo binstall brush-shell            # prebuilt, via cargo-binstall
```

Packagers have also brought brush to [Homebrew](https://formulae.brew.sh/formula/brush), [Arch Linux](https://archlinux.org/packages/extra/x86_64/brush/), [Fedora via Terra](https://terrapkg.com/), [MSYS2](https://packages.msys2.org/base/mingw-w64-brush), [Nix](https://search.nixos.org/packages?channel=unstable&query=brush), and [more](https://repology.org/project/brush/versions). See [all install options](https://brush.sh/getting-started/install/) for details.

Then run `brush`. It reads the same startup files Bash does, so it picks up your Bash setup as it is. To give brush a look of its own, add a `~/.brushrc`.

## Use the shell

- **Your configuration comes with you.** `.bashrc`, `.bash_profile`, aliases, functions, `PS1`, `PROMPT_COMMAND`, and prompt tools like [starship](https://starship.rs) all work as they do in Bash.
- **Programmable completion.** Works with the [bash-completion](https://github.com/scop/bash-completion) package you already have installed, so `git`, `docker`, `systemctl`, and the rest complete as usual.
- **Job control.** Background jobs, suspend and resume, `fg`, `bg`, and `jobs`.
- **Auto-suggestions.** History-based hints as you type, on by default. Powered by [reedline](https://github.com/nushell/reedline).
- **Syntax highlighting.** Live, as you type, one setting away: `brush --enable-highlighting` or `syntax-highlighting = true` under `[ui]` in brush's [TOML config file](https://brush.sh/reference/config-files/).
- **Scripts, too.** The builtins, expansions, arrays, redirections, and options your scripts already use, with `set -e`, `pipefail`, `extglob`, `globstar`, and friends.
- **Experimental extras.** zsh-style `precmd` and `preexec` hooks, and terminal shell integration for VS Code, iTerm2, and other supporting terminals. Both are off by default; see [experimental features](https://brush.sh/reference/experimental/).

> Not everything is there yet. Most notably, `select`, `wait -n`, `disown`, some traps, and a set of edge cases are still missing. The [compatibility reference](https://brush.sh/reference/compatibility/) lists what works, what's partial, and what isn't implemented. If you spot something that doesn't look right, please let us know by filing an issue.

## Build with the engine

The same implementation that runs the shell is available as a set of crates. Create a shell, run Bash-compatible code in it, and inspect the result:

```rust
let mut shell = brush_core::Shell::builder().build().await?;

let result = shell
    .run_string(
        r#"greet() { echo "Hello, $1!"; }; greet world"#,
        &brush_core::SourceInfo::default(),
        &shell.default_exec_params(),
    )
    .await?;

assert!(result.is_success());
```

For more, see the examples: [register a builtin written in Rust](brush-core/examples/custom-builtin.rs) alongside the standard ones, [call a shell function from Rust](brush-core/examples/call-func.rs) with control over its I/O, or [parse a script and serialize its syntax tree](brush-parser/examples/serde.rs). The [examples guide](docs/reference/examples.md) describes each one and when it's useful.

| Crate | API docs | What it provides |
|---|---|---|
| [`brush-core`](brush-core) | [docs.rs](https://docs.rs/brush-core) | The shell runtime: expansion, execution, jobs, completion, and the `Shell` API. Start here to embed. |
| [`brush-parser`](brush-parser) | [docs.rs](https://docs.rs/brush-parser) | Tokenizer and parser for Bash and POSIX shell syntax, with an optional `serde` AST. Usable independently from the other crates. |
| [`brush-builtins`](brush-builtins) | [docs.rs](https://docs.rs/brush-builtins) | The standard builtins, usable as a set. Depends on `brush-core`. |
| [`brush-interactive`](brush-interactive) | [docs.rs](https://docs.rs/brush-interactive) | Line editing, highlighting, suggestions, and completion UI, built on [reedline](https://github.com/nushell/reedline). Depends on `brush-core`. |
| [`brush-shell`](brush-shell) | [docs.rs](https://docs.rs/brush-shell) | The `brush` binary and its command line. Depends on everything else. |

Optional crates add bundled [coreutils builtins](brush-coreutils-builtins) and [experimental builtins](brush-experimental-builtins).

## How we test it

- **Compatibility suite.** More than 2,500 [test cases](brush-shell/tests/cases) run the same script under brush and Bash and compare stdout, stderr, exit status, and filesystem side effects. Every pull request runs them on Linux (x86_64 and aarch64) and macOS, and inside Arch Linux, Debian, Fedora, NixOS, openSUSE, and Azure Linux containers.
- **Real tools, real tests.** [End-to-end suites](e2e) exercise brush with the tools people pair with a shell: [fzf](https://github.com/junegunn/fzf), [atuin](https://github.com/atuinsh/atuin), [starship](https://github.com/starship/starship), [zoxide](https://github.com/ajeetdsouza/zoxide), [mise](https://github.com/jdx/mise), and others. Where a project has its own shell-integration tests, those run against brush; where it doesn't, we've added some.
- **Everything else.** CodeQL, dependency auditing, and benchmarks on every pull request, plus fuzz targets for the parser and the highlighter.
- **Verifiable releases.** Binaries are built by the release workflow with signed build provenance. The install script checks each download's SHA-256 checksum and, when the GitHub CLI is available, its attestation.

## Platforms

| Tier | Platforms | What that means |
|---|---|---|
| **Supported** | _Linux_: x86_64, aarch64 (glibc, musl) <br/> macOS: aarch64 | Prebuilt binaries for every release, the full test suite on Linux x86_64 and aarch64 (glibc) and macOS aarch64, build checks for Linux musl and macOS x86_64, and daily-driver quality. |
| **Preview**   | _Windows_: x86_64, aarch64 | Windows receives build and static checks in CI along with a small brush-specific test suite; prebuilt binaries are on the way. Parts of the shell are still missing or limited but it's functional for basic usage, particularly when paired with Microsoft's build of [coreutils for Windows](https://github.com/microsoft/coreutils). |
| **Experimental** | _WASI 0.2_ | WASI builds run under wasmtime in CI. There are *many* significant gaps; more of a starting point for further experimentation.  |
| **Builds only** | _Android_, _FreeBSD_, _NetBSD_, _OpenBSD_ | Cross-compiled in CI so they keep compiling. No official tests, binaries, or validation. |

## Community and contributing

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/extras/brand/mascot/turtle-dark.png">
  <img src="docs/extras/brand/mascot/turtle-light.png" alt="" width="110" align="right" hspace="24" vspace="8">
</picture>

brush started as a curiosity-driven project, and that curiosity is still what drives it. Most contributors arrived the same way: tried brush, hit something unexpected, and helped fix it. However much time you have, there's a way in.

- **Try it and tell us what you find.** A script or command that behaves differently in Bash is the most useful report we get. [File a compatibility bug](https://github.com/reubeno/brush/issues/new?template=compatibility-bug.yml), or a [feature request](https://github.com/reubeno/brush/issues/new?template=feature-request.yml) if brush could do more for you.
- **Say hello on [Discord](https://discord.gg/kPRgC9j3Tj)**, whether you have a question, an idea, or a shell setup you'd like to see work. Or just come to chat and hang out.
- **Pick up an issue.** Issues labeled ["good first issue"](https://github.com/reubeno/brush/labels/good%20first%20issue) are intended for newcomers, and ["help wanted"](https://github.com/reubeno/brush/labels/help%20wanted) tag additional items where an extra pair of hands would help. Draft pull requests are welcome; if you ask us, we'll take an early look before you polish.
- **Read the [contribution guidelines](CONTRIBUTING.md)** for the workflow, and the [technical docs](docs/README.md) for how brush is built and tested. Everyone here is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).

A star, a mention in your own project's README, or a post about brush all help too.

Curious how brush relates to other shells? See [related projects](docs/reference/related-projects.md).

### Contributors

brush is shaped by everyone who has given it their time: bug reports with a reproducer, reviews that caught what we missed, packages for distributions we'd never touched, and a good deal of patient chat. Thank you, all of you. It's appreciated more than a line in a README can say.

<a href="https://github.com/reubeno/brush/graphs/contributors"><img src="https://contrib.rocks/image?repo=reubeno/brush" alt="brush contributors"/></a>

There's room here for you, too.

## Credits

brush stands on the shoulders of excellent open source projects, including (but not limited to):

- [reedline](https://github.com/nushell/reedline) for line editing and interactive features
- [clap](https://github.com/clap-rs/clap) for command-line parsing
- [fancy-regex](https://github.com/fancy-regex/fancy-regex) for regular expressions
- [tokio](https://github.com/tokio-rs/tokio) as the async runtime
- [nix](https://github.com/nix-rust/nix) for accessing Unix and POSIX APIs
- [criterion.rs](https://github.com/bheisler/criterion.rs) for benchmarking
- [bash-completion](https://github.com/scop/bash-completion) for its completion test suite

---

Released as open source under the [MIT license](LICENSE). Built in the open by the [brush community](https://github.com/reubeno/brush/graphs/contributors).
