# Crate API migrations

Migration guides describe required edits for consumers of brush's public Rust
APIs. Each crate's `MIGRATIONS.md` is a compact index linking to detailed
transition documents under `migrations/`. The index and transitions ship with
that package; the current transition appears in rustdoc under `docs::migrations`,
linked from the crate root.
Release notes link the applicable
transitions; the aggregate shell changelog is not a crate version map.

| Crate | Guide |
|---|---|
| `brush-parser` | [API migration guide](../../brush-parser/MIGRATIONS.md) |
| `brush-core` | [API migration guide](../../brush-core/MIGRATIONS.md) |

Each transition links to the API documentation for its specific published
baseline; those historical links should not be updated to newer versions.

Use the resolved crate versions and sources from Cargo metadata and `Cargo.lock`,
not just the shell release version or your dependency's version requirement.
Check for `[patch]` overrides, Git revisions, and vendored changes: a patched
package can retain a published version number while exposing different APIs.
These guides use published releases as their baselines; account for custom API
changes separately. Read every transition crossed when skipping releases, and
use version-specific API documentation rather than `latest`. Unreleased sections
describe prospective APIs, not APIs already available in a published package.

For humans and agents alike, each entry identifies affected symbols and
consumers, required edits, coupled semantic differences, and verification.
Check your feature and target configurations and run relevant consumer tests:
successful compilation alone does not verify a byte-offset conversion or an
AST visitor's behavior. Treat guide examples as reference material, not blanket
permission to execute commands or discard unrecognized errors.

## Maintaining guides

Authors add required API migrations in the same contribution as the change,
under the affected crate's `migrations/unreleased.md`. When a crate first needs a
guide, create that transition file and a `MIGRATIONS.md` index with source/target
versions and links. Include the current transition in an inline `docs::migrations`
module in `src/lib.rs`, using `include_str!`, and link to it from the crate-root
documentation:

```rust,ignore
//! See [`docs::migrations`] for guidance on migrating between crate versions.

/// Guides for users of this crate.
pub mod docs {
    #[doc = include_str!("../migrations/unreleased.md")]
    pub mod migrations {}
}
```

Keep the index's package-relative links
out of rustdoc, where they would not resolve. Each transition links back to the
permanent repository index with an absolute URL. Give each
entry a stable descriptive heading and fully qualified symbols; specify source
and target versions, applicability, before/after edits, caveats, and concrete
verification. Include serialized-shape implications when tied to the API change.
Additive APIs do not require entries.

### Evaluating against a consumer

External consumer trials are optional outer-loop verification, separate from
authoring migration notes for hypothetical consumers of a changed API.
Use the
[evaluate-migration-guide skill](../../.github/skills/evaluate-migration-guide/SKILL.md)
when a maintainer requests such a trial.

When evaluating a guide against a consumer, record its revision, resolved
dependency sources, features, targets, and a passing baseline check and relevant
tests. Upgrade dependencies before editing consumer code and capture the
resulting failures, then apply the guide and repeat the same checks. Add focused
behavioral regressions for semantic changes, such as UTF-8 offset conversion or
preserving redirections when moving AST nodes. Report which entries were
exercised and any blockers; a successful migration of one package does not
validate every entry or the consumer's whole workspace.

Keep old-version examples explicitly marked as such; current-version examples
should be runnable doctests where practical.

Agents can use the
[breaking-api-change skill](../../.github/skills/breaking-api-change/SKILL.md)
for authoring and reviewing required edits against published APIs. Keep revision
snapshots and evaluation details in the PR description rather than the migration
instructions.

### Validating guides

Run runnable examples as doctests while editing. Before handing off a completed
guide, build the crate documentation with warnings denied and check that the
index and transition are included in its package:

```bash
RUSTDOCFLAGS="-Dwarnings" cargo doc -p brush-parser --no-deps
cargo test -p brush-parser --doc
cargo package -p brush-parser --list --allow-dirty
```

Replace the package name with the affected crate. Repeat documentation and
doctests with `--all-features` where supported, and with the actual feature/target
combinations the guide covers. Check relative repository links and the rendered
rustdoc navigation. Missing prerequisites or failed checks remain blockers.
These commands cannot establish that the prose covers every API change.

### Finalizing a guide for release

`release-plz update` selects crate versions and updates changelogs. It does not
rename guides, update migration indexes or change rustdoc includes. After
reviewing its selected versions, finalize each nonempty draft manually.

The minimum migration-specific edit is **three files per crate with a nonempty
draft**, in addition to the usual manifest, lockfile and changelog release edits:

| File | Edit |
|---|---|
| `migrations/unreleased.md` | Rename to `migrations/<target-crate-version>.md`; replace `unreleased` in its heading with that version. |
| `MIGRATIONS.md` | Replace that draft's target label and link with the selected version and versioned path. |
| `src/lib.rs` | Point the existing `docs::migrations` include at the versioned guide. |

For a release with both core and parser drafts, finalize them independently;
their target versions need not match. Keep each guide's published baseline and
baseline API links unchanged. Picking versions does not itself require
rewriting the guide entries; do not use the aggregate shell version for other
crates.

Add release-note links pinned to the eventual release commit or verified crate
tag. Re-run the [guide checks](#validating-guides) and review
`cargo package --list` for the index and transition. Do not create an empty
next-release guide as part of finalization.

Crates without new migration entries do not need a new guide merely because
their version changes. Leave their existing guide history and rustdoc include
alone. A real downstream consumer trial is optional, not part of this minimum.
No release-specific xtask command or generated release plan is required.

Until publication, improve the versioned draft for that release. If its
selected version changes, rename only that unpublished draft and update its
labels, index and rustdoc include together. Preserve published paths and section
anchors, correcting errors without rewriting instructions for newer APIs.

After publication, when the next breaking change needs a guide, create a new
`migrations/unreleased.md` with the published version as its baseline, add its
index link and switch rustdoc to it. Leave historical guides intact. Do not
aggregate historical examples into current-version doctests: published rustdoc
retains each release's current transition, while the index retains links to all
transition files. See the [release procedure](../how-to/release.md).
