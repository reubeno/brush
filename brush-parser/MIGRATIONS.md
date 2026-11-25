# API migrations

Upgrading `brush-parser`? Start with the version in your `Cargo.lock` and read
each guide between it and your target version. These guides cover breaking Rust
API changes, not new APIs. Forks and patched dependencies may need additional
changes.

| From | To | Guide |
|---|---|---|
| 0.4.0 | 0.5.0 | [Migration details](migrations/0.5.0.md) |

Transition files ship in the crate package. The current transition is also
rendered in crate-level rustdoc, with runnable examples where practical.
