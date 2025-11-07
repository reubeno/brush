# How to record "tapes"

Under the `docs/demos` directory of this repo, we have some `.tape` files checked in.
These are interactive scripts for recording screencast-style demos of `brush` using
the [`VHS` tool](https://github.com/charmbracelet/vhs).

## Install `vhs`

You first need to install `vhs`. For consistency, we've found it easiest to install
`golang` and then follow the instructions on the `vhs` github page to install via
`go install`. (Also note that there are some native prerequisites required.)

## Run `vhs`

To run `vhs` against the `.tape` file you may need to use `VHS_NO_SANDBOX=1`. For more
details see [this issue on GitHub](https://github.com/charmbracelet/vhs/issues/504).