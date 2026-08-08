# Dynamic well-known variables never honor assignment (RANDOM=N, SECONDS=N, ...)

## Status

Not started. This is a planning doc, written up per request instead of rushing into
a large cross-cutting change. Nothing below has been implemented yet.

## Problem

Assigning a scalar to a dynamic well-known variable is always a silent no-op in
brush today:

```bash
RANDOM=5
echo $RANDOM   # bash: reseeds the sequence (still "random", but deterministic from
               # here on for a given seed). brush: ignored, unaffected.

SECONDS=100
sleep 1
echo $SECONDS  # bash: 101 (offset applied). brush: whatever the real elapsed time is,
               # unaffected by the assignment.
```

This is *separate* from the `declare -a`/`declare -A` shape-conversion bug fixed
earlier in this branch (`DynamicValueKind`, `convert_to_*_array_for_reassignment`,
etc.) — that work fixed what happens when a dynamic variable's *shape* is
force-converted. This doc is about what happens when a dynamic variable is
*assigned a value through its normal setter path*, which today does nothing at all
for every dynamic variable, on every path.

## Root cause

- `ShellValue::Dynamic` stores a `setter: DynamicValueSetter` where
  `type DynamicValueSetter = fn(&dyn ShellState) -> ();` (`brush-core/src/variables.rs`).
  It takes **no value parameter** — even if something called it, it couldn't know what
  was being assigned.
- Grepping the whole tree: **the `setter` field is never invoked anywhere.** It's
  dead code. `ShellVariable::assign()`'s two `Dynamic` match arms (append and
  non-append) are unconditional `Ok(())` no-ops (see the `// TODO(dynamic)` comments
  already in place there).
- `RANDOM`/`SRANDOM`'s getters (`wellknownvars.rs`) draw straight from
  `rand::rng()`, the global thread-local RNG — there is no per-shell RNG state to
  reseed in the first place.
- `SECONDS`'s getter computes `now - shell.last_stopwatch_time() + shell.last_stopwatch_offset()`.
  `Shell` already stores both fields and exposes read-only accessors
  (`last_stopwatch_time()`, `last_stopwatch_offset()`) but **no setter** for either.

## Why this is bigger than it looks

To make `RANDOM=N`/`SECONDS=N` actually do something, the setter needs to run with
access to shell-level state (RNG seed, stopwatch fields), and it needs to run at
every place a scalar can be assigned to a variable. I traced where scalar
assignment actually happens and it's not just the 10 direct callers of
`ShellVariable::assign()`:

- Direct `ShellVariable::assign()` callers (10): `env.rs` (x3, inside
  `update_or_add`/`update_or_add_array_element`), `variables.rs` (x3, internal
  unset-defaulting, never Dynamic in practice), `declare.rs` (x2), `export.rs` (x1),
  `interp.rs` (x1, the general array/associative in-place update path).
- But the *common* path for `NAME=value`, arithmetic assignment, `read`, `getopts`,
  `mapfile`, completion, and array-default expansion (`${x:=default}`) all go
  through **`ShellEnvironment::update_or_add()` / `update_or_add_array_element()`**
  (`brush-core/src/env.rs`), which is called from ~16 more sites across:
  `interp.rs`, `arithmetic.rs`, `completion.rs`, `expansion.rs` (x2),
  `commands.rs`, `extendedtests.rs`, `shell/fs.rs` (x2), and the builtins
  `read.rs` (x3), `getopts.rs` (x4), `mapfile.rs` (x2), `export.rs` (x1).
- **`ShellEnvironment` has no reference to `Shell` at all.** It's a strictly
  lower-level construct (just the variable scope stack). `update_or_add` cannot
  reach RNG state or the stopwatch fields today, structurally.
- At least one call site aliases badly if we naively add a `&dyn ShellState`
  parameter: `shell/fs.rs` calls `self.env.update_or_add(...)` where `self` **is**
  the `Shell`. A `&dyn ShellState` parameter sourced from that same `self` would
  be borrowing all of `self` immutably while `self.env` is already borrowed
  mutably as the method receiver — the trait object erases field boundaries, so
  the borrow checker can't see that RNG/stopwatch fields are disjoint from `env`.
  A concrete, narrow reference (see Option B below) sidesteps this; a `&dyn
  ShellState` trait object does not.

So doing this consistently (not just for the one or two most common paths) means
touching on the order of **25+ call sites across a dozen files**, several of which
have borrow-checker traps, not just plumbing.

## Options considered

### Option A — Thread `&dyn ShellState` (or `&mut dyn ShellState`) through `assign()` and `update_or_add()`

Most "obvious" fix. Add a parameter to `ShellVariable::assign()` and
`ShellEnvironment::update_or_add[_array_element]()`, update every call site to pass
the shell handle through.

- Pro: reuses the existing `ShellState` trait, no new state-sharing primitive.
- Con: hits the `shell/fs.rs`-style aliasing problem described above at every
  `impl Shell` method that calls `self.env.update_or_add(...)` directly (need to
  check exactly how many; `shell/fs.rs` is a confirmed instance). Would likely
  need `self.env.update_or_add(&self.random_state, ...)`-style disjoint field
  splitting anyway, undermining the "just pass `&dyn ShellState`" simplicity.
  Largest diff of the three options; touches the most files.

### Option B — Concrete narrow state, not a trait object

Give `Shell` two small fields for exactly what setters need:
`random_state: Cell<u64>` (or similar; see "RANDOM reseeding" below) and change
`last_stopwatch_time`/`last_stopwatch_offset` to `Cell<SystemTime>`/`Cell<u32>` (both
`Copy`, `Cell` is a natural fit). Pass `&Cell<u64>` / `&Cell<SystemTime>` /
`&Cell<u32>` — concrete types, not a trait object — into `update_or_add`/`assign`
only where needed.

- Pro: because these are literal disjoint fields (not an opaque "whole shell" view),
  Rust's borrow checker allows `self.env.update_or_add(&self.random_state, ...)`
  even while `self.env` is mutably borrowed as the receiver — no aliasing problem
  at the `shell/fs.rs`-style call sites.
- Con: still requires adding a parameter (now 2-3 concrete params instead of 1
  trait-object param) through the same ~25+ call sites. Less elegant call
  signature. `Cell` requires the setter logic to be synchronous, single-threaded,
  which is already true for everything else in `Shell`.

### Option C — Move the mutable state out of `Shell`'s borrow entirely (recommended)

Change `DynamicValueGetter`/`DynamicValueSetter` from bare `fn` pointers to
capturing closures (`Rc<dyn Fn(...)>`, since `Shell` is not required to be
`Send`/`Sync` today — confirm before committing to `Rc` vs `Arc`) that close over a
`Rc<Cell<u64>>` (RNG) / `Rc<Cell<(SystemTime, u32)>>` (stopwatch) created once when
the variable is registered in `wellknownvars.rs::init_well_known_vars`. Then:

- `assign()` calls `setter(new_value)` directly — **no shell reference needed at
  all**, because the closure already owns a handle to the state it mutates. This
  eliminates the entire plumbing problem: `ShellEnvironment::update_or_add` and
  every one of its ~16 callers need **zero changes**.
- `ShellVariable::assign()`'s two `Dynamic` arms change from unconditional
  `Ok(())` to `(setter)(value); Ok(())` (roughly) — a small, contained change.
- Getters (`get_random_value`, `get_srandom_value`, the `SECONDS` getter in
  `wellknownvars.rs`) switch from reading `rand::rng()` / `shell.last_stopwatch_*()`
  to reading the same captured `Rc<Cell<_>>`.
- Needs care in `impl Clone for Shell` (`shell.rs:148`): today
  `last_stopwatch_time`/`last_stopwatch_offset` are plain `Copy` fields, so
  `Shell::clone()` naturally gives a subshell an independent copy that diverges
  after fork — which matches real bash (a forked subshell's `RANDOM` sequence
  diverges from the parent's after the fork point). If the state becomes a shared
  `Rc<Cell<_>>` captured inside the `Dynamic` closures at variable-registration
  time, a naive `Shell::clone()` would **share** the `Rc` (same pointer), which is
  wrong — parent and subshell would then mutate the *same* cell instead of
  diverging. Cloning must re-run `init_well_known_vars`-style re-registration (or
  explicitly unwrap-and-rewrap each `Rc<Cell<_>>` with a fresh `Rc::new(old.get())`)
  so each clone gets an independent cell seeded from the parent's current value.
  This needs to be gotten right and tested (fork/subshell + `RANDOM`/`SECONDS`
  compat cases) or subshells will corrupt each other's RNG/stopwatch state.
- `ShellValue` derives `Debug`; a `Rc<dyn Fn>` field isn't `Debug`. Same shape of
  problem it already has with `serde` on `Dynamic` (see the existing
  `#[cfg_attr(feature = "serde", serde(skip, default = ...))]` pattern) — will
  need either a manual `Debug` impl for `ShellValue`, or a small non-`Debug`-derived
  wrapper type around the closure with a hand-written `Debug` that just prints
  `"<dynamic>"` or similar.
- Con: bare `fn` pointers today are trivially `Copy`/`Clone`/(pseudo-)`Debug`-safe
  and cheap; moving to `Rc<dyn Fn>` adds an allocation per dynamic variable (only
  at shell-init/clone time, not per-read) and a bit of ceremony (custom `Debug`,
  careful `Clone`).
- Pro: by far the smallest, most contained diff of the three options. Confines the
  change to `variables.rs` (type defs + the two `assign()` arms) and
  `wellknownvars.rs` (closure construction for `RANDOM`/`SRANDOM`/`SECONDS`, plus
  updating `Shell::clone()`). Zero changes to `env.rs`, and zero changes to any of
  the ~25 call sites enumerated above.

## Recommendation

Option C. It turns a 25+-call-site, multi-file refactor with borrow-checker risk
into a change confined to two files, at the cost of a bit of closure/`Rc`/`Debug`
ceremony and needing to get `Shell::clone()` right for subshell isolation. Worth
double-checking whether `Shell`/its extensions are required to be `Send`/`Sync`
anywhere (async executor bounds?) before committing to `Rc` over `Arc`+`Mutex` —
if `Send` is required, the closures and cells need to be `Arc<Mutex<_>>` or
`Arc<AtomicU64>`/atomics instead, which changes some details above but not the
overall shape of the plan.

## Scope for RANDOM/SRANDOM specifically

Bash's actual `RANDOM` PRNG algorithm can't be replicated bit-for-bit without
reimplementing bash's specific internal generator, and no one should try — any
compat test for this can only assert **self-consistency** (same seed via `RANDOM=N`
twice in the same shell produces the same subsequent value; different seeds
produce different sequences), never exact cross-shell value equality. The existing
`RANDOM`/`SRANDOM` compat tests already follow this pattern (checking
non-determinism/range, not exact values) — new tests for reseeding should follow
the same style, e.g.:

```yaml
- name: "RANDOM reseeds deterministically"
  stdin: |
    RANDOM=42
    first=$RANDOM
    RANDOM=42
    second=$RANDOM
    [[ $first == $second ]] && echo "RANDOM reseeds deterministically"
```

## Suggested follow-up steps (once this plan is agreed)

1. Confirm `Send`/`Sync` requirements on `Shell<SE>` (check `extensions::ShellExtensions`
   bounds and anywhere `Shell` crosses an `async` boundary) to settle `Rc` vs `Arc`.
2. Change `DynamicValueGetter`/`DynamicValueSetter` typedefs and add the captured-cell
   construction helpers in `wellknownvars.rs`.
3. Wire `RANDOM`/`SRANDOM` to the new per-shell RNG cell; re-derive their exact algorithm
   choice (doesn't need to match bash, just needs to be a real, seedable PRNG).
4. Wire `SECONDS` to a captured stopwatch cell; drop `Shell`'s now-redundant
   `last_stopwatch_time`/`last_stopwatch_offset` fields (or keep them as the
   canonical initial values seeded into the cell at shell construction — decide
   during implementation).
5. Fix `impl Clone for Shell` so cloned/forked shells get independent cells seeded
   from the parent's current value, not shared pointers.
6. Change `ShellVariable::assign()`'s two `Dynamic` arms to actually invoke the setter.
7. Add compat tests (self-consistency style, per above) for `RANDOM=N` and `SECONDS=N`.
8. Manually verify subshell isolation: `( RANDOM=1; echo $RANDOM ); echo $RANDOM` should
   show the subshell's reseeded seq without affecting the parent's.
