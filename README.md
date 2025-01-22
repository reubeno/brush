# brush

[![Crates.io](https://img.shields.io/crates/v/brush-shell?style=flat-square)](https://crates.io/crates/brush-shell)
[![Crates.io](https://img.shields.io/crates/d/brush-shell?style=flat-square)](https://crates.io/crates/brush-shell)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![CI workflow badge](https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/ci.yaml)
[![Devcontainer workflow badge](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml)

## About

`brush` (**B**o(u)rn(e) **RU**sty **SH**ell) is a [POSIX-](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html) and [bash-](https://www.gnu.org/software/bash/)compatible shell,
implemented in Rust. It's built and tested on Linux and macOS, with experimental support on Windows. (Its Linux build is fully supported running on Windows via WSL.)

![screenshot](https://github.com/user-attachments/assets/0e64d1b9-7e4e-43be-8593-6c1b9607ac52)

`brush` is functional for interactive use as a daily driver! It can execute most `sh` and `bash` scripts we've
encountered. Known limitations are tracked with filed issues. Out of an abundance of caution, 
we wouldn't recommend using it yet in _production_ scenarios in case it doesn't behave identically
to your existing stable shell. (If you do find any behavioral differences, though, please report them with an
issue!)

Contributions and feedback of all kinds are welcome! For more guidance, please consult our
[contribution guidelines](CONTRIBUTING.md). For more technical details, please consult the
[documentation](docs/README.md) in this repo.

This project was originally borne out of curiosity and a desire to learn. We're doing our best to keep that
attitude :).

### License

Available for use and distribution under the [MIT license](LICENSE).

### Try it out!

We don't publish binary releases of `brush`, but if you have a working `rust` toolchain installed you can simply run:

```bash
cargo install --locked brush-shell
```

This will install the most recently released version of `brush` from `crates.io`. Alternatively, for the latest and
greatest bits, you can clone this repo and execute `cargo run`.

If you don't have `rust` installed, we recommend installing it via [`rustup`](https://rustup.rs/).

(If you *are* interested in having a binary release, then please let us know in the 'Discussions' area of this
project or by filing a feature request in 'Issues'.)

When you run `brush`, it should look exactly as `bash` would on your system since it processes `.bashrc` and
other usual configuration. If you'd like to customize the look of `brush` to distinguish it from the other shells
installed on your system, then you can also author a `~/.brushrc` file.

### Known limitations

There are some known gaps in compatibility. Most notably:

* **Honoring the full semantics of all `set` and `shopt` options.**
  The `set` builtin is implemented, as is `set -x` and many frequently used options, but a number of options aren't fully implemented. `set -e`, for example, will execute but its semantics aren't applied across execution.

* **Anything tagged with a `TODO` comment or where `error::unimp()` is used to return a "not implemented" error**.
  These aren't all tracked with GitHub issues right now, but there's a number of these scattered throughout the code base. Some are indicative of missing functionality that may be straightforward to implement; others may be more complicated.

If you feel so inclined, we'd love contributions toward any of the above, with broadening test coverage, deeper compatibility evaluation, or really any other opportunities you can find to help make this project better.

## Testing strategy

This project is primarily tested by comparing its behavior with other existing shells, leveraging the latter as test oracles. The integration tests implemented in this repo include [515+ test cases](brush-shell/tests/cases) run on both this shell and an oracle, comparing standard output and exit codes.

For more details, please consult the [reference documentation on integration testing](docs/reference/integration-testing.md).

## Credits

There's a long list of OSS crates whose shoulders this project rests on. Notably, the following crates are directly relied on for major portions of shell functionality:

* [`reedline`](https://github.com/nushell/reedline) - for readline-like input and interactive usage
* [`clap`](https://github.com/clap-rs/clap) - command-line parsing, used both by the top-level brush CLI as well as built-in commands
* [`fancy-regex`](https://github.com/fancy-regex/fancy-regex) - relied on for everything regex
* [`tokio`](https://github.com/tokio-rs/tokio) - async, well, everything
* [`nix` rust crate](https://github.com/nix-rust/nix) - higher-level APIs for Unix/POSIX system APIs

Huge kudos and thanks also to `pprof` and `criterion` projects for enabling awesome flamegraphs in smooth integration with `cargo bench`'s standard benchmarking facilities.

## Links: other shell implementations

There are a number of other POSIX-ish shells implemented in a non-C/C++ implementation language. Some inspirational examples include:

* [Nushell](https://www.nushell.sh/) - modern Rust-implemented shell (which also provides the `reedline` crate we use!)
* [Rusty Bash](https://github.com/shellgei/rusty_bash)
* [mvdan/sh](https://github.com/mvdan/sh)

We're sure there are plenty more; we're happy to include links to them as well.
