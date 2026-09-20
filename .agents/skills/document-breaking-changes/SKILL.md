---
name: document-breaking-changes
description: How to document a breaking API or behaviour change to a published brush crate: the migration guide pair (crate-shipped agent guide plus human walkthrough), the commit footer, rustdoc notes, and tests. Use whenever a change removes, renames, or re-types public API in a library-tier crate, changes a convention that compiles unchanged, or changes observable shell behaviour that embedders or scripts could depend on.
---

# Documenting breaking changes

A breaking change to a library-tier crate (`brush-core`, `brush-parser`,
`brush-builtin-utils`, `brush-builtins`, `brush-coreutils-builtins`,
`brush-experimental-builtins`) is not done until it is documented in all four
places below. Do this in the same pull request as the change.

## 1. Classify every change

Go through the diff and list each incompatible change as one of:

- **MECHANICAL**: a rename, removal, or re-typing that the compiler catches and
  that has a single replacement. Most changes.
- **CONDITIONAL**: has a replacement that depends on how the caller used it.
  Give the cases as a table.
- **SILENT**: compiles unchanged but behaves differently. The most dangerous
  kind: a changed argument convention, a changed default, a field that means
  something else. Call these out separately and give a verification step a
  reader can run.
- **BEHAVIOUR**: no API change, but scripts or embedders can observe a
  difference (an error now reported, output framed differently).

If you are unsure whether something is breaking, it is: any change to an item
exported from a published crate counts (`AGENTS.md` §3).

## 2. Write or extend the agent guide

Location: `<crate>/skills/migrate-<from>-to-<to>/SKILL.md`. `<from>` is the
version in the crate's `Cargo.toml` today; `<to>` is the next version release
tooling will assign (minor bump for 0.x, major otherwise). If the directory
already exists for this cycle, append entries; do not start a second guide.

Frontmatter:

```yaml
---
name: migrate-<crate>-<from>-to-<to>
description: Migrate a crate that depends on <crate> <from> to <to> (<one-line scope>).
depends-on: <crate>=<to>
---
```

Body, in this order:

1. One paragraph: what it is, its scope, and a link to the human guide.
2. **Execution model**: apply MECHANICAL rules in order, then CONDITIONAL where
   detected, review SILENT, build, no compatibility shims.
3. **Required Cargo.toml changes** as a before/after block.
4. **Breaking changes**, numbered, each headed
   `### N. KIND — short description` with three parts:
   - *Detection*: a `grep -rn -E` pattern with word boundaries that finds the
     affected code, plus anything the compiler will say.
   - *Action*: the exact replacement, as a before/after code block or an
     old → new table. Order rewrites longest-match-first when one identifier
     is a substring of another.
   - *Verify* (SILENT and CONDITIONAL entries): a command or check that
     proves the fix took.
5. **Additive APIs** (no action) and **Behaviour changes** (no code change;
   check tests).
6. **VERIFY**: a build command and one grep over all removed identifiers that
   must print nothing.

Write it terse and mechanical. It is read by an agent applying it to a
codebase it has never seen, not by a person learning the design.

## 3. Write or extend the human guide

Location: `docs/migrations/<crate>/<from>-to-<to>.md`. Add it to the table in
`docs/migrations/README.md` if new.

Contents: what changed and why (two or three paragraphs), a quick summary, then
steps for the two or three cases most users hit, with before/after snippets,
then behaviour changes, then how to verify. Link to the agent guide for the
exhaustive list. SILENT changes get their own step with the word "silent" in
it.

## 4. Commit message, rustdoc, tests

- Commit: conventional-commit `!` marker plus a `BREAKING CHANGE:` footer that
  lists every removed or changed item by name and names the guide. The
  changelog is generated from commits, so this footer is the only text that
  reaches it.
- Rustdoc: on the item that replaces a removed one, and on any item whose
  convention changed silently, add a short note ("Since <to>, …") so someone
  who lands there from a compile error sees the migration without searching.
- Tests: BEHAVIOUR changes get a compat YAML case under
  `brush-shell/tests/cases/`; removed macro forms get a compile-fail case with
  a clear message where a macro is involved.

## 5. Check before finishing

- [ ] Every entry in the agent guide has Detection and Action; SILENT entries
      have Verify.
- [ ] Running the guide's VERIFY grep over this workspace prints nothing.
- [ ] The in-tree consumers (`brush-builtins`, `brush-shell`, examples) were
      migrated the way the guide says, and `cargo xtask ci quick` passes.
- [ ] The human guide's steps were tried against one of those consumers.
- [ ] `docs/migrations/README.md` lists the guide.
- [ ] The commit footer names each item.
