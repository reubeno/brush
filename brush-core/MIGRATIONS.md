# API migrations

Upgrading `brush-core`? Start with the version in your `Cargo.lock` and read each
guide between it and your target version. These guides cover breaking Rust API
changes, not new APIs or shell behavior fixes. Forks and patched dependencies may
need additional changes.

| From | To | Guide |
|---|---|---|
| 0.5.0 | 0.6.0 | [Migration details](migrations/0.6.0.md) |

Transition files ship in the crate package. The current transition is also
rendered in crate-level rustdoc, with runnable examples where practical.
