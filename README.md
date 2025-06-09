<div align="center">
  <img src="https://github.com/user-attachments/assets/19351a8e-7b03-4338-81be-dd5b6d7e5abc"/>
</div>

<br/>

<!-- Primary badges -->
<p align="center">
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/v/brush-shell?style=flat-square"/></a>
  <a href="https://crates.io/crates/brush-shell"><img src="https://img.shields.io/crates/d/brush-shell?style=flat-square"/></a>
  <img src="https://img.shields.io/crates/msrv/brush-shell"/>
  <img src="https://tokei.rs/b1/github/reubeno/brush?category=code"/>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square"/></a>
  <a href="https://github.com/reubeno/brush/actions/workflows/ci.yaml"><img src="https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg"/></a>

  <br/>

  <!-- Social badges -->
  <a href="https://discord.gg/kPRgC9j3Tj">
    <img src="https://dcbadge.limes.pink/api/server/https://discord.gg/kPRgC9j3Tj?compact=true&style=flat" alt="Discord invite"/>
  </a>

  <!-- Packaging badges -->
  <a href="https://repology.org/project/brush/versions">
    <img src="https://repology.org/badge/version-for-repo/aur/brush.svg" alt="AUR package"/>
  </a>
  <a href="https://repology.org/project/brush/versions">
    <img src="https://repology.org/badge/version-for-repo/nix_unstable/brush.svg" alt="AUR package"/>
  </a>

</p>

<a href="https://repology.org/project/brush/versions">
</a>

</p> 

<hr/>

`brush` (**B**o(u)rn(e) **RU**sty **SH**ell) is a [POSIX-](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html) and [bash-](https://www.gnu.org/software/bash/)compatible shell,
implemented in Rust. It's built and tested on Linux, macOS, and WSL. Native Windows support is experimental.

<p align="center">
  <img src="https://github.com/user-attachments/assets/0e64d1b9-7e4e-43be-8593-6c1b9607ac52" width="80%"/>
</p>

`brush` is functional for interactive use as a daily driver! It executes most `sh` and `bash` scripts we've
encountered. Known limitations are tracked with filed issues. Out of an abundance of caution,
we wouldn't recommend using it yet in _production_ scenarios in case it doesn't behave identically
to your existing stable shell. (If you do find any behavioral differences, though, please report them with an
issue!)

Contributions and feedback of all kinds are welcome! For more guidance, please consult our
[contribution guidelines](CONTRIBUTING.md). For more technical details, please consult the
[documentation](docs/README.md) in this repo.

This project was originally borne out of curiosity and a desire to learn. We're doing our best to keep that
attitude :).

<br/>

## 📝 License

Available for use and distribution under the [MIT license](LICENSE).

## ⌨️ Installation

When you run `brush`, it should look exactly as `bash` does on your system: it processes your `.bashrc` and
other standard configuration. If you'd like to distinguish the look of `brush` from the other shells
on your system, you may author a `~/.brushrc` file.

### 🔨 Building from sources

We don't (yet) publish binary releases of `brush` (but will Real Soon Now). To build from sources, first install a working `rust` toolchain; we recommend installing it via [`rustup`](https://rustup.rs/). Then run:

```bash
cargo install --locked brush-shell
```

### 🐧 Nix

If you are a Nix user, you can also use the registered version.

```bash
nix run 'github:NixOS/nixpkgs/nixpkgs-unstable#brush' -- --version
```

### 🐧 Arch Linux

Arch Linux users can install `brush` [from the AUR](https://aur.archlinux.org/packages/brush) with their [AUR helper](https://wiki.archlinux.org/title/AUR_helpers) of choice, e.g.

```bash
paru -S brush
```

## 👥 Community

`brush` has a community Discord server, available [here](https://discord.gg/kPRgC9j3Tj).

## 🔍 Known limitations

There are some known gaps in compatibility. Most notably:

* **Some `set` and `shopt` options.**
  The `set` builtin is implemented, as is `set -x` and many frequently used `set`/`shopt` options, but a number aren't fully implemented. For example, `set -e` will execute but its semantics aren't applied across execution.

* **The `history` builtin and support for programmatically manipulating command history.**
  This is something we're actively working on, with promises for supporting shell extensions like [atuin](https://atuin.sh/).

If you feel so inclined, we'd love contributions toward any of the above, with broadening test coverage, deeper compatibility evaluation, or really any other opportunities you can find to help us make this project better.

## 🧪 Testing strategy

This project is primarily tested by comparing its behavior with other existing shells, leveraging the latter as test oracles. The integration tests implemented in this repo include [620+ test cases](brush-shell/tests/cases) run on both this shell and an oracle, comparing standard output and exit codes.

For more details, please consult the [reference documentation on integration testing](docs/reference/integration-testing.md).

## 🙏 Credits

There's a long list of OSS crates whose shoulders this project rests on. Notably, the following crates are directly relied on for major portions of shell functionality:

* [`reedline`](https://github.com/nushell/reedline) - for readline-like input and interactive usage
* [`clap`](https://github.com/clap-rs/clap) - command-line parsing, used both by the top-level brush CLI as well as built-in commands
* [`fancy-regex`](https://github.com/fancy-regex/fancy-regex) - relied on for everything regex
* [`tokio`](https://github.com/tokio-rs/tokio) - async, well, everything
* [`nix` rust crate](https://github.com/nix-rust/nix) - higher-level APIs for Unix/POSIX system APIs

For testing, performance benchmarking, and other important engineering support, we use and love:

* [`pprof-rs`](https://github.com/tikv/pprof-rs) - for sampling-based CPU profiling
* [`criterion.rs`](https://github.com/bheisler/criterion.rs) - for statistics-based benchmarking
* [`bash-completion`](https://github.com/scop/bash-completion) - for its completion test suite and general completion support!

## 🔗 Links: other shell implementations

There are a number of other POSIX-ish shells implemented in a non-C/C++ implementation language. Some inspirational examples include:

* [`nushell`](https://www.nushell.sh/) - modern Rust-implemented shell (which also provides the `reedline` crate we use!)
* [`rusty_bash`](https://github.com/shellgei/rusty_bash)
* [`mvdan/sh`](https://github.com/mvdan/sh)
* [`Oils`](https://github.com/oils-for-unix/oils)
* [`fish`](https://fishshell.com) ([as of 4.0](https://fishshell.com/blog/rustport/))

We're sure there are plenty more; we're happy to include links to them as well.
