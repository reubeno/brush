# brush

[![CI workflow badge](https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/ci.yaml)
[![Devcontainer workflow badge](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml/badge.svg)](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml)

## About

`brush` (**B**orn **RU**sty **SH**ell) is a shell implementation with aspirations of compatibility with the [POSIX Shell specification](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html) as well as with [bash](https://www.gnu.org/software/bash/). It's generally functional for interactive use and can execute many scripts, but it's still very much a work in progress. 

This project was primarily borne out of curiosity and a desire to learn. If it proves to be sufficiently interesting and/or useful, then that's a bonus :).

### License

Licensed under [MIT license](LICENSE).

### Known limitations

There are many gaps in compatibility; most notably:

* Commands run asynchronously as jobs, job management
* Honoring `set` options (e.g., `set -e`)
* Input/output redirection for subshells and function definitions

Shell built-ins are a mixed bag. Some are completely implemented
(e.g. echo), some only support their most commonly used options,
and others aren't implemented at all.

## Testing strategy

This project is primarily tested by comparing its
behavior with other existing shells, leveraging the latter
as test oracles. The integration tests implemented in this
repo include a few hundred test cases run on both this
shell and an oracle, comparing standard output and exit
codes.

## Links: other shell implementations

This is certainly not the first attempt to implement a feature-rich POSIX-ish shell in a non-C/C++ implementation language. Some examples include:

* https://github.com/shellgei/rusty_bash
* https://github.com/mvdan/sh
