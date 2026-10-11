# Migration guides

When a published brush crate makes a breaking change, two guides are written,
one per audience, and both are kept in this repository:

| Audience | Location | Style |
|---|---|---|
| AI agents | `<crate>/skills/migrate-<from>-to-<to>/SKILL.md` | Exhaustive, terse, mechanical: every breaking change with a detection grep, a before/after action, and a verification step. Source of truth. |
| People | `docs/migrations/<crate>/<from>-to-<to>.md` | A walkthrough of the changes most users hit, with the why. Links to the agent guide for the complete list. |

The agent guide lives inside the crate so that it ships with it. A project that
depends on the crate can install it with [Symposium](https://symposium.dev/)
(`cargo agents sync`), which picks up any `skills/` directory in a dependency
and activates each skill only when its `depends-on` frontmatter matches, so
`depends-on: brush-core=0.6` activates the 0.5 → 0.6 guide for projects on 0.6
and stays dormant everywhere else. Guides are never deleted; the predicate
retires them.

`<from>` and `<to>` are the compatible-version prefixes: `0.5-to-0.6` for a
0.x crate, `1-to-2` once a crate reaches 1.0.

## When a guide is written

At the time the breaking change lands, in the same pull request, not at release
time. `<to>` is the next version release tooling will assign: a minor bump for
a 0.x crate, a major bump otherwise. Later breaking changes in the same release
cycle append to the existing guide. Every guide states its scope and, if a
release has no breaking changes, no guide is needed.

The checklist and the entry format are in the `document-breaking-changes`
skill (`.agents/skills/document-breaking-changes/SKILL.md`), which is also what
AI agents working on this repository follow.

## Guides

| Crate | Versions | People | Agents |
|---|---|---|---|
| brush-core | 0.5 → 0.6 | [walkthrough](brush-core/0.5-to-0.6.md) | [SKILL.md](../../brush-core/skills/migrate-0.5-to-0.6/SKILL.md) |
