# Minimum Supported Rust Version (MSRV) Policy

## Overview

The `brush` project maintains a conservative MSRV policy to balance two key concerns:

1. **Binary distribution**: Users building `brush` from source should not need a bleeding-edge compiler
2. **Library usage**: Downstream projects depending on `brush` crates should not face aggressive MSRV increases

## Policy

### When We Update MSRV

We **do not** update MSRV proactively. Updates only occur when:

- A meaningful set of language features or capabilities becomes available that provides clear value to the project
- The return-on-investment justifies the potential impact on users and downstream dependencies

### MSRV Age Requirements

When we do update MSRV, we move to a Rust version that is **at least 4-6 months old** from the time of the update. This ensures:

- Sufficient time for the Rust version to stabilize
- Wide availability in package managers and development environments
- Reduced friction for users building from source

### Communication

MSRV changes are always:

- Explicitly documented in release notes
- Considered a notable change requiring user awareness
- Announced with clear justification for the update

## Rationale

This conservative approach recognizes that `brush` serves dual purposes: as a standalone binary tool and as a library for integration into other projects. Both use cases benefit from stability and predictability in compiler requirements.
