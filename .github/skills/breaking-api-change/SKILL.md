---
name: breaking-api-change
description: Document and validate brush public Rust API changes that require downstream edits. Use when removing, renaming, or reshaping public APIs.
---

# Breaking API changes

Read [AGENTS.md](../../../AGENTS.md) and the
[independent-implementation policy](../../../CONTRIBUTING.md#independent-implementation-and-provenance)
before investigating. Follow the canonical
[migration authoring guidance](../../../docs/reference/migrations.md#maintaining-guides);
it defines guide placement, version labels, and examples.

1. Establish the affected crate's published baseline and verify the source
   revision associated with that published package.
2. Infer required edits for hypothetical consumers from the API diff: calls, constructors, exhaustive patterns,
   trait implementations, ownership, and directly coupled serialized shapes.
   Do not inspect actual downstream repositories in this inner loop or add
   entries for purely additive APIs.
3. Group the migration by consumer task. Name the affected symbols, show the
   before/after edits, and explain semantic differences that compilation cannot
   catch.
4. Update the crate's current migration draft in the same contribution. Read the
   index first: if a maintainer already renamed the draft for a release, edit that
   versioned file until publication rather than starting the next transition.
   After publication, create or populate the next unreleased draft, link it
   from the index, and switch rustdoc to it. Leave historical guides intact.
5. Add tests for any changed behavior, following the repository's testing
   guidance. Run checks relevant to the change; use doctests when iterating on
   runnable migration examples rather than requiring a separate documentation
   build for every API edit. Before handing off, build the affected crate's
   documentation with warnings denied and confirm with `cargo package --list`
   that the index and transition ship in its package. Follow the authoring
   policy's validation commands.

Leave version selection to `release-plz update` when a maintainer prepares a
release, following the [release procedure](../../../docs/how-to/release.md).
Finalize the guides manually after reviewing the selected crate versions.
Do not update release versions merely to document one API change. Put inspected
revision details and evaluation evidence in the PR description, not in the
enduring migration instructions.
