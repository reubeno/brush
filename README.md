<div align="center">
  <img src="https://github.com/user-attachments/assets/19351a8e-7b03-4338-81be-dd5b6d7e5abc"/>
</div>

<br/>

<!-- Primary badges -->
<p align="center">
  <!-- crates.io version badge -->
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/v/brush-shell?style=flat-square"/></a>
  <!-- msrv badge -->
  <img src="https://img.shields.io/crates/msrv/brush-shell"/>
  <!-- compat tests badge -->
  <img src="https://img.shields.io/badge/compat_tests-1400%2B-brightgreen?style=flat-square" alt="1400+ compatibility tests"/>
  <!-- license badge -->
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square"/>
  <!-- CI status badge -->
  <a href="https://github.com/reubeno/brush/actions/workflows/ci.yaml"><img src="https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg"/></a>
  <br/>
  <!-- crates.io download badge -->
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/d/brush-shell?style=flat-square"/></a>
  <!-- Packaging badges -->
  <a href="https://repology.org/project/brush/versions">
    <img src="https://repology.org/badge/tiny-repos/brush.svg" alt="Packaging status"/>
  </a>
  <!-- Dependencies badges -->
  <a href="https://deps.rs/repo/github/reubeno/brush"><img src="https://deps.rs/repo/github/reubeno/brush/status.svg" alt="Dependency status"/></a>
  <!-- Social badges -->
  <a href="https://discord.gg/kPRgC9j3Tj">
    <img src="https://dcbadge.limes.pink/api/server/https://discord.gg/kPRgC9j3Tj?compact=true&style=flat" alt="Discord invite"/>
  </a>
</p>

<hr/>

`brush` (**B**o(u)rn(e) **RU**sty **SH**ell) is a modern [bash-](https://www.gnu.org/software/bash/) and [POSIX-](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html)compatible shell
written in Rust. It can be used as an interactive shell, to run scripts, or embedded as a library in other Rust applications.
Built and tested on Linux, macOS, and WSL, with experimental Windows and WebAssembly (WASM) support.

<p align="center">
  <img src="https://github.com/user-attachments/assets/0e64d1b9-7e4e-43be-8593-6c1b9607ac52" width="80%"/>
</p>

`brush` is ready for use as a daily driver! It runs most `sh` and `bash` scripts we've encountered.
We validate compatibility through [1400+ test cases](brush-shell/tests/cases) that compare behavior against `bash`.
If you find any behavioral differences, please [report them](https://github.com/reubeno/brush/issues)—battle-testing is welcome!

This project was originally borne out of curiosity and a desire to learn. We're doing our best to keep that
attitude :).

## ✨ Features

### 🐚 `bash` Compatibility

| | Feature | Description |
|--|---------|-------------|
| ✅ | **50+ builtins** | `echo`, `declare`, `read`, `complete`, `fc`, `history`, `ulimit`, `mapfile`, `bind`, `trap`, ... |
| ✅ | **Full expansions** | Brace, parameter, command substitution, arithmetic, process substitution, tilde, `extglob`, `globstar`, ... |
| ✅ | **Control flow** | `if`/`then`/`else`, `for`, arithmetic `for`, `while`, `until`, `case`, `&&`, `\|\|`, subshells, pipelines, ... |
| ✅ | **Redirection** | Here documents, here strings, fd duplication (`>&`, `<&`), process substitution redirects, `\|&`, ... |
| ✅ | **Dynamic variables** | Rust-backed magic variables: `RANDOM`, `SRANDOM`, `LINENO`, `EPOCHSECONDS`, `EPOCHREALTIME`, `SECONDS`, ... |
| ✅ | **Programmable completion** | Compatible with [bash-completion](https://github.com/scop/bash-completion) — git, docker, etc. work out of the box |
| ✅ | **Job control** | Background jobs, suspend/resume (`Ctrl+Z`), `fg`/`bg`/`jobs`, process groups |
| ✅ | **Arrays** | Indexed and associative arrays with slicing, subscripts, `${!prefix@}`, etc. |
| 🔷 | **Traps** | `trap` support for `EXIT`, `DEBUG`; signal traps are in progress |
| 🔷 | **Key bindings** | Partial `bind` support including `bind -x` for custom key-bound commands |
| 🔷 | **Shell options** | Common options (`errexit`, `pipefail`, `extglob`, ...) work; less common options are in progress |

### ⌨️ User Experience

| | Feature | Description |
|--|---------|-------------|
| ✅ | **Syntax highlighting** | Real-time highlighting as you type (powered by [reedline](https://github.com/nushell/reedline)) |
| ✅ | **Autosuggestions** | History-based command suggestions (powered by [reedline](https://github.com/nushell/reedline)) |
| ✅ | **Rich prompts** | `PS0`/`PS1`/`PS2`/`PS4` plus right-side prompts, `PROMPT_COMMAND` support; works with [starship](https://starship.rs) |
| ✅ | **TOML configuration** | Optional `~/.config/brush/config.toml` for persistent settings |
| 🧪 | **ZSH-style hooks** | `precmd`/`preexec` hooks for prompt customization |
| 🧪 | **Terminal integration** | Terminal integration sequences (`OSC 633`) for VS Code and compatible terminals |
| 🧪 | **History extensions** | Basic support for [`fzf`](https://github.com/junegunn/fzf), [`atuin`](https://github.com/atuinsh/atuin), and similar tools |

### 🔧 For Developers

| | Feature | Description |
|--|---------|-------------|
| ✅ | **Embeddable API** | `Shell::builder()` for integration into Rust applications |
| ✅ | **Cross-platform** | Full support for Linux and macOS; Windows and WASM are experimental |
| 🔷 | **Rich error diagnostics** | Optional [`miette`](https://github.com/zkat/miette) integration for pretty parse errors |
| 🚧 | **Custom shell extensions** | Zero-overhead hooks on key internal Shell events via extension traits |

<br/>

## 📝 License

Available for use and distribution under the [MIT license](LICENSE).

## ⌨️ Installation

_When you run `brush`, it should look exactly as `bash` does on your system: it processes your `.bashrc` and
other standard configuration. If you'd like to distinguish the look of `brush` from the other shells
on your system, you may author a `~/.brushrc` file._

<details open>
<summary>🚀 <b>Installing prebuilt binaries from GitHub</b></summary>

We publish prebuilt binaries of `brush` for Linux (x86_64, aarch64) and macOS (aarch64) to GitHub for official [releases](https://github.com/reubeno/brush/releases). You can manually download and extract the `brush` binary from one of the archives published there, or otherwise use the GitHub CLI to download it, e.g.:

```bash
gh release download --repo reubeno/brush --pattern "brush-x86_64-unknown-linux-gnu.*"
```

After downloading the archive for your platform, you may verify its authenticity using the [GitHub CLI](https://cli.github.com/), e.g.:

```bash
gh attestation verify brush-x86_64-unknown-linux-gnu.tar.gz --repo reubeno/brush
```

</details>

<details open>
<summary>🚀 <b>Installing prebuilt binaries via `cargo binstall`</b></summary>

You may use [cargo binstall](https://github.com/cargo-bins/cargo-binstall) to install pre-built `brush` binaries. Once you've installed `cargo-binstall` you can run:

```bash
cargo binstall brush-shell
```

</details>

<details>
<summary> 🔨 <b>Installing from sources</b></summary>

To build from sources, first install a working (and recent) `rust` toolchain; we recommend installing it via [`rustup`](https://rustup.rs/). Then run:

```bash
cargo install --locked brush-shell
```

</details>

<details>
<summary>🐧 <b>Installing using Nix</b></summary>

If you are a Nix user, you can use the registered version:

```bash
nix run 'github:NixOS/nixpkgs/nixpkgs-unstable#brush' -- --version
```

</details>

<details>
<summary>🐧 <b>Installing on Arch Linux</b></summary>

Arch Linux users can install `brush` from the official [extra repository](https://archlinux.org/packages/extra/x86_64/brush/):

```bash
pacman -S brush
```

</details>

<details>
<summary>🍺 <b>Installing using Homebrew</b></summary>

Homebrew users can install using [the `brush` formula](https://formulae.brew.sh/formula/brush):

```bash
brew install brush
```

</details>

## 👥 Community

`brush` has a community Discord server, available [here](https://discord.gg/kPRgC9j3Tj).

## 🛠️ For Developers

Contributions and feedback are welcome! Resources for contributors:

* [Building from source](docs/how-to/build.md) — includes test commands and development workflow
* [Contribution guidelines](CONTRIBUTING.md) — how to submit changes
* [Technical documentation](docs/README.md) — architecture and reference docs
* [Agent development guide](AGENTS.md) — for AI-assisted development

The project uses 1400+ compatibility tests comparing behavior against bash. Run them with:

```bash
cargo test --test brush-compat-tests
```

## 🔗 Related Projects

Other POSIX-ish shells implemented in non-C/C++ languages that inspired or relate to this project:

* [`nushell`](https://www.nushell.sh/) — modern Rust shell (provides the `reedline` crate we use)
* [`fish`](https://fishshell.com) — user-friendly shell ([Rust port in 4.0](https://fishshell.com/blog/rustport/))
* [`Oils`](https://github.com/oils-for-unix/oils) — bash-compatible with new Oil language
* [`mvdan/sh`](https://github.com/mvdan/sh) — Go implementation
* [`rusty_bash`](https://github.com/shellgei/rusty_bash) — another Rust-implemented bash-like shell

## 🙏 Credits

<details>
<summary>Key dependencies and acknowledgments</summary>

This project relies on many excellent OSS crates. Notable dependencies for core shell functionality:

* [`reedline`](https://github.com/nushell/reedline) - for readline-like input and interactive usage
* [`clap`](https://github.com/clap-rs/clap) - command-line parsing, used both by the top-level brush CLI as well as built-in commands
* [`fancy-regex`](https://github.com/fancy-regex/fancy-regex) - relied on for everything regex
* [`tokio`](https://github.com/tokio-rs/tokio) - async, well, everything
* [`nix` rust crate](https://github.com/nix-rust/nix) - higher-level APIs for Unix/POSIX system APIs

For testing, performance benchmarking, and other important engineering support, we use and love:

* [`criterion.rs`](https://github.com/bheisler/criterion.rs) - for statistics-based benchmarking
* [`bash-completion`](https://github.com/scop/bash-completion) - for its completion test suite and general completion support!

</details>
