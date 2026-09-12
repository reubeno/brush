# Minimum Supported Rust Version (MSRV) Policy

## Overview

The `brush` project maintains a conservative MSRV policy to balance two key concerns:

1. **Binary distribution**: Users building `brush` from source should not need a bleeding-edge compiler
2. **Library usage**: Downstream projects depending on `brush` crates should not face aggressive MSRV increases

These concerns pull against each other, and they do not apply evenly across the workspace. The crates
that other projects embed are held to a stricter standard than the ones that exist to produce the
shell itself. The workspace is therefore split into two tiers.

## Tiers

### Library tier

`brush-core`, `brush-parser`, `brush-builtins`, `brush-coreutils-builtins`, and
`brush-experimental-builtins` are the reusable pieces. Other projects embed them to get a shell
runtime, a bash-compatible parser, or a set of builtins without taking on brush's interactive front
end. Their dependency sets are deliberately narrow for the same reason.

These crates inherit `rust-version` from `[workspace.package]` in the root `Cargo.toml`. That value
is the project's MSRV, and it is the one the rules below are mostly about.

`xtask` and `brush-test-harness` inherit the same value. Neither is published, but `cargo xtask` is
how CI runs the MSRV check, so `xtask` has to compile with the MSRV toolchain. That is why `xtask`
must not depend on `brush-shell`: the tool that drives the build cannot sit downstream of the thing
it builds.

### Application tier

`brush`, `brush-shell`, `brush-interactive`, and `brush-fuzz` exist to produce, host, and exercise
the shell. `brush-shell` and `brush-interactive` are published as libraries, but their reason to
exist is the interactive front end, and they carry its dependencies: line editing, terminal
handling, completion. Those dependencies raise their own MSRVs on a schedule the project does not
control, and refusing to follow would mean freezing the interactive experience on whatever line
editor was current at the time.

These crates declare `rust-version` in their own manifests, above the workspace value. Those
manifests are the only record of it; no other file in the tree repeats the number.

## Policy

### Updating the library tier

We **do not** update the workspace MSRV proactively. Updates only occur when:

- A meaningful set of language features or capabilities becomes available that provides clear value
  to the project
- The return-on-investment justifies the potential impact on users and downstream dependencies

When we do update, we move to a Rust version that is **at least 4-6 months old** at the time of the
update. This ensures:

- Sufficient time for the Rust version to stabilize
- Wide availability in package managers and development environments
- Reduced friction for users building from source

### Updating the application tier

An application-tier crate raises its `rust-version` only when a dependency it needs actually
requires it, and only as far as that dependency requires. We do not round up to current stable, and
we do not move a crate ahead of the workspace speculatively. The commit that raises the value should
be the commit that takes the dependency, so the reason is visible in one place rather than inferred
later.

The 4-6 month age requirement does not apply to this tier; the forcing dependency's own MSRV decides
the number. In exchange, a raise is confined to the crates that need it. If only `brush-interactive`
requires a newer toolchain, `brush-shell` and `brush` follow because they depend on it, and the
library tier does not move at all.

### Communication

MSRV changes are always:

- Explicitly documented in release notes
- Considered a notable change requiring user awareness
- Announced with clear justification for the update

A library-tier change is the more significant of the two, because it reaches every downstream
consumer. An application-tier change affects people building the shell from source, and should say
which dependency forced it.

## Enforcement

CI runs the source-code checks against both current stable and the workspace MSRV. On the MSRV leg,
the build check runs as:

```bash
cargo xtask check build --workspace-msrv
```

Cargo refuses to build any package whose `rust-version` exceeds the active toolchain, so the
application-tier crates cannot be part of that leg. The flag asks `cargo metadata` which workspace
members declare a `rust-version` above the lowest one in the workspace and excludes exactly those.
No list of crate names exists anywhere in the tooling, so moving a crate between tiers needs no
change to it. Without the flag, every crate is checked, which is what the stable leg wants.

## Rationale

This split recognizes that `brush` serves two different audiences. A project embedding `brush-core`
wants a stable, predictable compiler requirement and gets one. A person building the shell wants a
current line editor and working terminal integration, and that has a cost the embedding audience
should not have to pay.
