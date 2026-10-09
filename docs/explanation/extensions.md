# Shell extensions

This document describes how hosts and libraries extend `brush-core`'s shell: the
architecture, every API and macro surface, the rules extensions follow, and how the
mechanism evolves. It specifies the design; the rustdoc in `brush-core/src/extensions.rs`
documents the same surface item by item.

## Goals

* **A shell never assumes it owns its process.** Embedders may run several top-level
  shells in one process, and subshells run in-process alongside their parent. Anything
  that acts on process-wide state (the working directory, `exec`, the terminal) is the
  host's decision.
* **Hosts and libraries can interpose on the shell's operations**: watch them, refuse
  them, rewrite them, or carry them out differently.
* **Extensions compose.** A library ships an extension (or a bundle of them) that a host
  adds to a list, like middleware. A host lists each extension once.
* **Clean defaults, zero cost.** A host that doesn't care writes nothing, and pays nothing:
  unused extension points compile down to the shell's built-in behavior.
* **Evolvable.** Adding an extension point, or growing one, doesn't break extensions.
* **Checked by the compiler.** An extension can't be half-registered, and a stack can't
  silently skip a method.

## Concepts

| Term | Meaning |
| --- | --- |
| Host | The program embedding the shell (e.g. `brush-shell`). |
| Host marker | A zero-sized type implementing `ShellExtensions`; `Shell<SE>` is generic over it. It selects the host's state and its stack. |
| Host state | A value of the host's choosing, stored in each shell, through which extensions keep their own state. |
| Area | A group of related extension points, declared as a trait (e.g. `WorkingDir`). |
| Extension method | One extension point: a method of an area trait. |
| Extension | A type that implements every area: the ones it takes part in with methods of its own, the rest as pass-throughs. |
| Stack | The host's extensions, outermost first, ending in the shell's built-in behavior. |
| `Next` | Within an extension method, the rest of the stack. |
| Built-in | `Builtin`: the shell's own implementation of every extension method, at the end of every stack. |
| Request, event | The struct an extension method takes besides its context: a request for an operation, an event for a notification. |
| Bundle | A library's own list of extensions, spliced into a host's stack as one entry. |
| Process ownership | A token, at most one per process, without which a shell never acts on process-wide state. |

## Architecture

A shell's operation that an area covers (say, changing directory) calls the host's stack,
`SE::Extensions`. The stack is a chain of layers, each an extension wrapping the rest:

```text
core call site
   │  SE::Extensions::resolve_working_dir(shell, request)
   ▼
Layer<Audit, Layer<Fence, Builtin>>
   │  Audit::resolve_working_dir::<Layer<Fence, Builtin>>(shell, request)
   ▼
Audit ──(may rewrite / refuse / act after)──► Next = Layer<Fence, Builtin>
                                                │  Fence::resolve_working_dir::<Builtin>(…)
                                                ▼
                                              Fence ──► Next = Builtin
                                                          │  the shell's own behavior
                                                          ▼
                                                        result flows back out
```

Everything is resolved at compile time: each layer calls the next through a type parameter,
so a stack compiles to direct calls, and an extension that doesn't write a method costs
nothing for it.

An extension implements each area separately. The layer for an extension `E` implements an
area's chain only if `E` implements that area, and the host's stack must implement every
area's chain, so an extension that doesn't account for every area can't be listed. Areas it
doesn't take part in it passes through, with empty impls whose defaults call `Next`.

The host's state lives in the shell. Each extension reaches its own part of it by type,
through `HasState<T>`.

## API surface

All items are in `brush_core::extensions` unless noted. Macros are exported at the crate
root (`brush_core::stack!`, etc.).

### `ShellExtensions`

```rust
pub trait ShellExtensions: Clone + Copy + Debug + Default + Send + Sync + 'static {
    type State: Clone + Send + Sync + 'static;
    type Extensions: Stack<Self>;
}

pub struct DefaultShellExtensions; // State = (), Extensions = Builtin
```

A host declares a marker type, never instantiated:

```rust
#[derive(Clone, Copy, Debug, Default)]
struct MyHost;

impl ShellExtensions for MyHost {
    type State = MyState;
    type Extensions = brush_core::stack![Audit, Fence, ..zsh::Bundle];
}
```

The marker's supertraits exist so that types generic over it can derive `Clone`, `Debug`,
and so on.

A shell is built with `Shell::builder_with_extensions::<MyHost>(state)`; `Shell::builder()`
uses `DefaultShellExtensions`.

### Host state

```rust
pub trait HasState<T> {
    fn state(&self) -> &T;
    fn state_mut(&mut self) -> &mut T;
}
impl<T> HasState<T> for T { /* identity */ }

impl<SE: ShellExtensions> Shell<SE> {
    pub fn host<T>(&self) -> &T where SE::State: HasState<T>;
    pub fn host_mut<T>(&mut self) -> &mut T where SE::State: HasState<T>;
}
```

An extension states the part of host state it needs as a bound, e.g.
`impl<SE: ShellExtensions<State: HasState<AuditLog>>> Files<SE> for Audit`, and reaches it
with `shell.host::<AuditLog>()`. The identity impl lets a host reach its whole state, and lets
a single-extension host use that extension's state as its own.

`host_state!` declares a host's state: its own fields, plus one field per part, with the
`HasState` impls generated.

```rust
brush_core::host_state! {
    #[derive(Clone, Default)]
    pub struct State {
        pub use_color: bool,        // the host's own fields
    }
    parts {
        audit: AuditLog,            // reachable as shell.host::<AuditLog>()
        fence: FenceConfig,
    }
}
```

Rules:

* The shell copies host state into every subshell and duplicate (hence `State: Clone`).
  State that should be shared (e.g. a log) goes behind an `Arc`.
* Give each extension's state its own type: parts are found by type, so two extensions
  bounding on the same type would share it.
* Host state isn't serialized. `Shell::load` takes fresh state.

### Stacks

```rust
pub struct Builtin;                          // the end of every stack
pub struct Layer<E, Rest>(PhantomData<…>);   // extension E in front of Rest
pub trait Stack<SE>: ErrorsChain<SE> + WorkingDirChain<SE> + … {}   // blanket-implemented
```

`stack!` builds a stack, outermost first:

| Syntax | Expands to |
| --- | --- |
| `stack![]` | `Builtin` |
| `stack![A, B]` | `Layer<A, Layer<B, Builtin>>` |
| `stack![A, B; Rest]` | `Layer<A, Layer<B, Rest>>` |
| `stack![A, ..Bundle, C]` | `Layer<A, Bundle<Layer<C, Builtin>>>` |
| `stack![..Bundle; Rest]` | `Bundle<Rest>` |

A library ships a bundle as a type alias generic over the rest of the stack:

```rust
pub type Bundle<Rest> = brush_core::stack![Preexec, Chpwd; Rest];
```

Order matters, and it's one order for all areas: the first extension listed is outermost, so
it sees every operation first and every result last.

### Areas

Each area is a pair of traits:

* the **area trait**, which extensions implement. All its methods have defaults that call
  `Next`. Each method is generic over `Next`, bounded by the area's chain trait.
* the **chain trait** (`<Area>Chain`), which says what `Next` can do. It's sealed: only
  `Builtin`, `Layer`, and `testing::Fake` implement it. Its methods mirror the area's,
  without the `Next` parameter.

Current areas:

```rust
pub trait Errors<SE: ShellExtensions> {
    fn format_error<Next: ErrorsChain<SE>>(shell: &Shell<SE>, error: &Error) -> String;
}

pub trait WorkingDir<SE: ShellExtensions> {
    fn resolve_working_dir<Next: WorkingDirChain<SE>>(
        shell: &Shell<SE>,
        request: DirRequest,
    ) -> impl Future<Output = Result<PathBuf, Error>> + Send;

    fn working_dir_changed<Next: WorkingDirChain<SE>>(
        shell: &mut Shell<SE>,
        event: &DirEvent,
    ) -> impl Future<Output = ()> + Send;
}

pub trait Lifecycle<SE: ShellExtensions> {
    fn forked<Next: LifecycleChain<SE>>(child: &mut Shell<SE>, event: &ForkEvent<'_, SE>);
}
```

| Method | Kind | Built-in behavior |
| --- | --- | --- |
| `Errors::format_error` | operation, sync | `error: <error>` and a newline |
| `WorkingDir::resolve_working_dir` | operation, async | Resolves the target (lexically, or physically for `cd -P`), checks it's a searchable directory; returns it absolute and normalized |
| `WorkingDir::working_dir_changed` | notification, async | Nothing |
| `Lifecycle::forked` | notification, sync | Nothing |

When each is invoked:

* `resolve_working_dir`: when a shell changes directory (`Shell::set_working_dir`,
  `set_working_dir_physical`, and hence `cd`, `pushd`, `popd`), for every shell including
  subshells. Not for a new shell's starting directory: as in bash, a shell may start in a
  directory it couldn't change to.
* `working_dir_changed`: after the shell's working directory has changed, even if updating
  `PWD`/`OLDPWD` then failed; when a shell is built or loaded (`DirCause::Started`); and
  when it's given process ownership (`DirCause::GainedOwnership`). Assignments to `PWD`
  don't change the working directory and aren't covered.
* `forked`: when `Shell::subshell` or `Shell::duplicate` makes a copy, before the copy runs
  anything. The shell's own short-lived scratch copies (which run no commands) aren't
  announced.

### Requests and events

```rust
#[non_exhaustive] pub struct DirRequest { pub target: PathBuf, pub physical: bool }
impl DirRequest { pub const fn new(target: PathBuf) -> Self }      // physical = false

#[non_exhaustive] pub struct DirEvent { pub cause: DirCause, pub old: Option<PathBuf>, pub new: PathBuf }
impl DirEvent { pub const fn new(cause: DirCause, old: Option<PathBuf>, new: PathBuf) -> Self }

#[non_exhaustive] pub enum DirCause { Changed, Started, GainedOwnership }

#[non_exhaustive] pub struct ForkEvent<'a, SE> { pub parent: &'a Shell<SE>, pub kind: Fork }
impl<'a, SE> ForkEvent<'a, SE> { pub const fn new(parent: &'a Shell<SE>, kind: Fork) -> Self }

#[non_exhaustive] pub enum Fork { Subshell, Duplicate }
```

Conventions, which every new request and event follows:

* `#[non_exhaustive]`, with public fields, so fields can be added without breaking anyone.
* A public `new` taking what it can't do without, with defaults for the rest (which can be
  set afterwards). Extensions make requests of their own (to pass a different one to
  `Next`); tests make events.
* On hot paths, requests borrow (e.g. a word to expand as `&'a str`), so that taking part
  costs nothing when no extension does.

### `pass_through!`

```rust
brush_core::pass_through!(Fence: Errors, Lifecycle);
// expands to:
// impl<SE: ShellExtensions> Errors<SE> for Fence {}
// impl<SE: ShellExtensions> Lifecycle<SE> for Fence {}
```

An extension lists the areas it doesn't take part in. Every area must be accounted for
exactly once:

* Leaving one out is an error where the extension is listed in a stack ("the trait
  `Lifecycle<MyHost>` is not implemented for `Fence`").
* Implementing one and passing it through is a conflicting-impl error.

For an extension that's itself generic, write the empty impls by hand.

### Process ownership

```rust
pub struct ProcessOwnership;                  // in brush_core; not Clone
impl ProcessOwnership { pub fn claim() -> Option<Self> }   // at most one per process
// dropping it gives it up

impl<SE: ShellExtensions> Shell<SE> {
    pub const fn owns_process(&self) -> bool;
    pub const fn take_process_ownership(&mut self) -> Option<ProcessOwnership>;
    pub async fn give_process_ownership(&mut self, ownership: ProcessOwnership);
    pub fn sync_process_working_dir(&self) -> Result<(), std::io::Error>;
}

// ShellBuilder: .process_ownership(token) / .maybe_process_ownership(Option<token>)
// Shell::load(deserializer, state, Option<ProcessOwnership>, restore)
```

* A shell holds ownership only if the host gives it the token, at build or load time or
  later. Copies (`subshell`, `duplicate`) never hold it.
* Without ownership, core doesn't act on the process:
  * `exec` runs the command and then exits the shell, rather than replacing the process;
  * `suspend` refuses;
  * the shell never takes the terminal's foreground.
* Known exceptions still act on the process regardless: `umask`, `ulimit`, signal handlers,
  job control's terminal hand-off to jobs, and `brush-interactive`'s terminal setup.
* Core never changes the process's working directory itself. `sync_process_working_dir`
  does, for the owner, when asked. `MirrorProcessWorkingDir` is a ready-made extension
  that asks after every change. `give_process_ownership` notifies
  `working_dir_changed` (`GainedOwnership`), so that such an extension can catch the
  process up.

### Testing

```rust
pub mod testing {
    pub struct Fake;                        // an end of stack for tests
    #[derive(Clone, Default)] pub struct Calls;   // what reached Fake
    impl Calls { pub fn entries(&self) -> Vec<String> }
    pub async fn shell<SE: ShellExtensions>(state: SE::State) -> Result<Shell<SE>, Error>;
}
```

To test an extension on its own, end its stack with `Fake` instead of `Builtin`, and give
the host state a `Calls` part:

```rust
#[derive(Clone, Copy, Debug, Default)]
struct TestHost;

impl ShellExtensions for TestHost {
    type State = testing::Calls;
    type Extensions = brush_core::stack![Fence; testing::Fake];
}

let mut shell = testing::shell::<TestHost>(testing::Calls::default()).await?;
assert!(shell.set_working_dir("/forbidden").await.is_err());
shell.set_working_dir("/elsewhere").await?;
assert_eq!(
    shell.host::<testing::Calls>().entries(),
    ["working_dir_changed Started", "resolve_working_dir /elsewhere", "working_dir_changed Changed"],
);
```

`Fake` records each call that reaches it (method name and a summary) and carries it out
without touching the system: errors are formatted as by the shell, and a directory is resolved
lexically, without checking that it exists. `testing::shell` builds a shell with no
inherited environment, well-known variables, builtins, or startup files.

Testing at three levels:

1. **One extension**, against `Fake`, as above: drive the shell, or call an area's chain
   trait on the stack directly, and check results and what reached `Fake`.
2. **Several extensions together**, against `Builtin`, running shell scripts through
   `Shell::run_string`.
3. **The framework's own guarantees**, in core:
   * compile-fail doctests for leaving out an area and for accounting for one twice;
   * a test that a stack of pass-through extensions lets every method reach `Fake`.

## Rules

These are the contracts extension methods follow. They are stated once here, and apply to
every area.

**Every method wraps an operation.** An extension method receives the rest of the stack as
`Next`. An extension that doesn't write the method passes the call on unchanged. One that
does may:

* change the request before calling `Next`;
* refuse, by returning an error without calling `Next`;
* act on the result after `Next` returns;
* not call `Next` at all, replacing everything further in (which then doesn't see the
  operation).

**A notification is an operation whose built-in does nothing.** An extension does its part
and calls `Next`, so that extensions further in are notified too. It may choose to do its
part before or after them. An extension that doesn't call `Next` withholds the notification
from everything further in. That's the same rule as for every method, and a deliberate trade
for having one mechanism.

**One shape.** Methods take no `self`. Parameters are the context, then one request or event.
The context is the shell, plus whatever else the operation is carried out with (e.g.
execution parameters). The shell is read-only unless running shell code is part of the
operation; notifications delivered once an operation is over get it mutably.

**The built-in implementation is innermost**, and it's the only place the operation touches
the OS. So an extension that doesn't call `Next` replaces all of it.

**Failures look native.** An extension that fails returns the error the built-in operation
would (e.g. an `io::Error` of kind `PermissionDenied`), so what a host simulates is
indistinguishable from the real thing.

**Async and cancellation.** Methods on async code paths return `impl Future + Send`; the
built-ins are ready immediately. An extension with nothing to await returns
`std::future::ready(..)`. Like any async operation, a pending extension method can be
cancelled (its future dropped), so notifications are best-effort, and an extension that acts
on the world should do so with no await afterwards.

**Re-entrancy.** An extension method with the shell mutably may run shell code, and so
re-enter the stack. Such a call is recursive, so the extension boxes it
(`Box::pin(shell.invoke_function(..)).await`). Anything that changes as a result is notified
too, possibly before extensions further in hear about the original event. So an extension
that tracks shell state reads it from the shell, not the event. An extension that must not
see its own nested operations keeps a guard in its host state.

**Lifecycle.** Every shell is built or loaded by the host, or forked from another (and
`forked` is notified). There's no other way to get a `Shell`: it isn't `Clone` or
`Default`, and only `Shell::load` deserializes one.

**Coverage.** An extension point is only as good as the paths that reach it. Before an area
covers some state or operation, every path to it goes through one funnel (as
`Shell::set_working_dir` became the only way to change directory), and raw `&mut` accessors
that bypass it are removed.

## Writing an extension

A library extension that refuses changes into a directory and logs changes:

```rust
use brush_core::extensions::{DirEvent, DirRequest, HasState, ShellExtensions, WorkingDir, WorkingDirChain};
use brush_core::{Error, Shell};

#[derive(Clone, Default)]
pub struct FenceConfig { pub forbidden: std::path::PathBuf }

pub struct Fence;

impl<SE: ShellExtensions<State: HasState<FenceConfig>>> WorkingDir<SE> for Fence {
    async fn resolve_working_dir<Next: WorkingDirChain<SE>>(
        shell: &Shell<SE>,
        request: DirRequest,
    ) -> Result<std::path::PathBuf, Error> {
        let dir = Next::resolve_working_dir(shell, request).await?;
        if dir.starts_with(&shell.host::<FenceConfig>().forbidden) {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into());
        }
        Ok(dir)
    }

    async fn working_dir_changed<Next: WorkingDirChain<SE>>(shell: &mut Shell<SE>, event: &DirEvent) {
        tracing::info!("now in {}", shell.working_dir().display());
        Next::working_dir_changed(shell, event).await;
    }
}

brush_core::pass_through!(Fence: Errors, Lifecycle);
```

A host using it:

```rust
brush_core::host_state! {
    #[derive(Clone, Default)]
    struct State {}
    parts { fence: FenceConfig }
}

#[derive(Clone, Copy, Debug, Default)]
struct Host;

impl ShellExtensions for Host {
    type State = State;
    type Extensions = brush_core::stack![brush_core::extensions::MirrorProcessWorkingDir, Fence];
}
```

## Evolving the mechanism

The areas are declared in one table in `extensions.rs`:

```rust
extension_areas! {
    /// Area docs.
    area WorkingDir / WorkingDirChain {
        /// Method docs.
        fn resolve_working_dir(shell: &Shell<SE>, request: DirRequest)
            -> impl Future<Output = Result<PathBuf, error::Error>> + Send
            = builtin::resolve_working_dir;
        …
    }
    …
}
```

From each area it generates the area trait (with defaults), the sealed chain trait, the
`Builtin` and `Layer` impls of the chain trait, and the `Stack` trait requiring every chain.
Each method gives its full return type (`-> T`, or `-> impl Future<Output = T> + Send`) and
the path of its built-in implementation, in the private `builtin` module.

| Change | Breaking? | Steps |
| --- | --- | --- |
| Add a method to an area | No: the default calls `Next` | Table entry; built-in implementation; `testing::Fake` impl; extend the every-method test |
| Add an area | Yes: every extension must account for it | Table entry; built-ins; `Fake` impl; extensions in-tree add it to `pass_through!` |
| Add a field to a request or event | No: `#[non_exhaustive]` | Give it a default in `new` |
| Change a method's signature | Yes | Avoid; add a request field instead |

Adding an area is a compile error, never a silent change: an extension that doesn't account
for the new area can't be listed. Keep areas coarse, and add them rarely.

Checklist for a new extension point:

1. Find, or make, the single funnel every path to the operation goes through.
2. Choose its area; choose operation (built-in does the work) or notification (built-in does
   nothing).
3. Choose its context by the rule above, and design its request or event (`#[non_exhaustive]`,
   `new`, borrowed on hot paths, core-owned types rather than `std` types extensions can't
   inspect or build).
4. Write the table entry, the built-in, and the `Fake` impl.
5. Test the forwarding (every-method test) and the behavior (through the shell).

## Implementation notes

Details that took effort to get right:

* **The built-in behavior is the end of the stack, not a separate parameter.**
  `stack![A, B]` is `Layer<A, Layer<B, Builtin>>`, so a stack is itself a complete chain.
  Each area needs only one chain trait and two forwarding impls (`Builtin`, `Layer`), and
  call sites need no turbofish. Swapping the end is what makes `testing::Fake` possible.
* **Full return types in table entries.** Writing `-> impl Future<Output = T> + Send` or
  `-> T` in each entry means the generator never distinguishes sync from async. Together
  with notifications forwarding like operations, the generator is one macro arm.
* **`SE` in table entries.** Parameter types written in the table (`&Shell<SE>`) refer to the
  generated traits' type parameter. `macro_rules!` hygiene doesn't apply to type parameters,
  so the table can name `SE` freely.
* **Sealing.** The chain traits have a crate-private supertrait in a private module. The
  workspace denies `unnameable_types`, which that pattern trips, so the supertrait carries an
  `#[expect]` with the reason. (A `pub(crate)` type in a public bound trips `private_bounds`
  instead.)
* **Passing `&mut Shell` to `Next`.** An extension method passes its `&mut Shell` to `Next`
  and can keep using it after `Next` returns: the parameter types are concrete, so Rust
  reborrows implicitly rather than moving.
* **Recursion.** An extension that runs shell code from a method recurses through the
  stack's futures. `Box::pin` the recursive call, or the future's type is infinite (E0733).
  Core's own recursive paths (e.g. command substitution during expansion) are already boxed.
* **No `self`.** Extensions are types, not values: the stack is resolved statically and
  costs nothing. Configuration lives in host state, found by type. Two instances of one
  extension with different settings need a generic tag type, and runtime choice comes from
  an extension that keeps trait objects in host state.
* **Only `Shell::load` deserializes.** `Shell`'s `Deserialize` impl is bounded on a
  crate-private host marker, so outside code can't deserialize a shell that skips `load`.
* **Process ownership is a global flag.** `ProcessOwnership::claim` sets a static
  `AtomicBool`, and dropping the token clears it. Copying a shell always leaves the copy
  without one.
* **Clippy and requests by value.** Requests are passed by value so extensions can take them
  apart. A built-in that only borrows its request needs
  `#[expect(clippy::needless_pass_by_value)]`.

## Validation

Two throwaway spikes exercised this design beyond the areas in core. They added areas for
running commands, external processes, expansion, files, and variables, and stacked six
independently written "library" extensions in one host:

* an audit log;
* a sandbox;
* expansion macros;
* emulation of an external program;
* a zsh-style `preexec`/`chpwd` bundle;
* a policy chosen at runtime.

Findings that shape future areas:

* **Commands have three seams**, which an area's methods should take separately:
  * where the command runs (`ShellForCommand`: the shell, or a subshell the command owns);
  * how (`ExecutionParameters`: open files, process group);
  * what (a request: name, arguments, lookup options).

  Running a command can run shell code, so it gets the shell mutably. Launching an external
  program doesn't, so it gets the shell read-only.
* **Requests and results must be core-owned types.** Exposing `std::process::Command` or a
  tokio child kept extensions from replacing how programs are run. `std::fs::OpenOptions`
  can't be inspected, so a sandbox couldn't tell reads from writes. With core types, an
  extension emulated a program outright.
* **`pass_through!` gets verbose with many areas.** An extension that takes part in one of
  eight areas lists seven. Adding five areas touched every extension, each flagged by a
  clear compile error. A derive macro could fill in pass-throughs automatically if this
  becomes a burden.
* **Coverage is the hard part, not the mechanism.** Each candidate area has paths that
  bypass its funnel today. For example, a variable hook would see `x=1` but not `read`,
  `declare`, or `${x:=y}`. Each area needs its funnel before it's trustworthy.

## Open questions

* **Extensions as values.** Per-instance configuration and a `dyn` adapter would come from
  extensions taking `&self`, at the cost of storing the stack in the shell and juggling
  borrows at call sites. Nothing so far has needed it.
* **A derive for participation**, to replace `pass_through!` and make adding areas
  non-breaking for extensions that use it.
* **Raw mutable accessors.** `ShellState`'s `*_mut` methods (notably `env_mut`, at dozens of
  call sites) bypass any future environment area.
* **Remaining process-wide actions** (`umask`, `ulimit`, signal handlers) that don't yet
  honor process ownership.
* **Scripted fakes.** `testing::Fake` answers each request one fixed way. Tests may want to
  script its answers.
