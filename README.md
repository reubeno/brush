# brush

[![CI workflow badge](https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/ci.yaml)
[![Devcontainer workflow badge](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml)

## About

`brush` (**B**orn **RU**sty **SH**ell) is a shell implementation with aspirations of compatibility with the [POSIX Shell specification](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html) and [bash](https://www.gnu.org/software/bash/).

It's generally functional for interactive use and can execute many scripts but still a work in progress. We do not recommend using this in production scenarios; until it's more stable, there's risk that using this implementation in place of your stable shell may result in unexpected behavior.

This project was primarily borne out of curiosity and a desire to learn. If it proves to be interesting or useful, then that's a bonus :).

Contributions and feedback of all kinds are welcome! For more guidance, please consult our [contribution guidelines](CONTRIBUTING.md). For more technical details, please consult the [documentation](docs/README.md) in this repo.

### License

Available for use and distribution under the [MIT license](LICENSE).

### What's working?

In short, quite a lot. Standard and extended control flow, word expansion, most frequently used builtin commands, pipelines, redirection, variables, etc. The plumbing for completion is present, along with support for common cases (e.g. file/dir completion, basic support for programmable completion such as used with git and other tools). 

### Known limitations

There's a lot that *is* working, but also non-trivial gaps in compatibility. Most notably:

* **Commands run asynchronously as jobs, job management.**
  You can run `some-command &` but it's proof-of-concept quality at best. Standard job management via `fg`, `bg`, and `jobs` is not fully implemented. This would be a great area for enthusiastic contributors to dive in :).
* **Honoring `set` options (e.g., `set -e`).**
  The `set` builtin is implemented, as is `set -x` and a few other options, but most of the behaviors aren't there. `set -e`, for example, will execute but its semantics aren't applied across execution.

Shell built-ins are a mixed bag. Some are completely and fully implemented (e.g. echo), while some only support their most commonly used options. Some aren't implemented at all.

There's certainly more gaps; with time we'll find a way to represent the gaps in some understandable way. Ideally, we'd like to evolve the test suites to add tests for all known missing pieces. That will let us focus on just "fixing the tests". 

## Testing strategy

This project is primarily tested by comparing its behavior with other existing shells, leveraging the latter as test oracles. The integration tests implemented in this repo include [300+ test cases](cli/tests/cases) run on both this shell and an oracle, comparing standard output and exit codes.

For more details, please consult the [reference documentation on integration testing](docs/reference/integration-testing.md).

## Links: other shell implementations

This is certainly not the first attempt to implement a feature-rich POSIX-ish shell in a non-C/C++ implementation language. Some examples include:

* https://github.com/shellgei/rusty_bash
* https://github.com/mvdan/sh

We're sure there are plenty more; we're happy to include links to them as well.