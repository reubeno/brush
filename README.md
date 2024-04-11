# brush

![CI workflow badge](https://github.com/reubeno/brush/actions/workflows/ci.yaml/badge.svg)
![Devcontainer workflow badge](https://github.com/reubeno/brush/actions/workflows/devcontainer.yaml/badge.svg)

## About

`brush` (Born RUsty SHell) is a shell implementation with
aspirations of compatibility with the [POSIX Shell specification](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html) as well as with [bash](https://www.gnu.org/software/bash/).
It's generally functional for interactive use and can
execute many simple to medium complexity scripts, but it's
still very much an incomplete work in progress. 

This project was primarily borne out of curiosity.

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

## References

The POSIX Shell specification was the first resource we
consulted.

Other non-C/C++ implementations of an `sh`/`bash` shell
provided inspiration through their existence and progress:

* https://github.com/shellgei/rusty_bash
* https://github.com/mvdan/sh
