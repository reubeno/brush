# Shared State for brush-core Builtins

## Goal

Add type-safe shared state to brush-core's builtin system, allowing multiple
builtins (e.g. `inherit`, `has_version`) to share state (e.g. an eclass AST
cache) across a shell and its clones.

## Current State

- `Command::State` — per-builtin, string-keyed, seeded by `state_init` fn pointer
- `Registration<SE>` — carries `state_init`, no shared state concept
- `register_builtin` / `register_builtin_with_state` — two registration paths
- Shared state not needed by existing brush builtins; needed by portage-repo

## Command Trait (updated)

```rust
pub trait Command: clap::Parser {
    type Error: BuiltinError + 'static;
    type State: Clone + Default + Send + Sync + 'static = ();
    type SharedState: Clone + Send + Sync + 'static = ();
    // ... existing methods ...

    // New convenience method
    fn shared<'a, SE: ShellExtensions>(
        &self,
        ctx: &'a ExecutionContext<'_, SE>,
    ) -> Result<&'a Self::SharedState, Error> {
        ctx.shared::<Self::SharedState>()
    }
}
```

- `SharedState` does NOT require `Default` — value always provided by caller
- `SharedState = ()` (the default) means "no shared state"

## Factory Function

```rust
pub fn builtin<B, SE>() -> Registration<SE, B::SharedState>
where
    B: Command + Send + Sync,
    SE: ShellExtensions,
```

The return type's phantom parameter encodes `B::SharedState`:
- `B::SharedState = ()` → `Registration<SE, ()>` → accepted by `register_builtin`
- `B::SharedState = Arc<RepoCache>` → `Registration<SE, Arc<RepoCache>>` → **rejected** by `register_builtin`

Same for `simple_builtin`, `decl_builtin`, `raw_arg_builtin` — all gain the phantom.

## Registration Type

```rust
pub struct Registration<SE: ShellExtensions, S = ()> {
    pub execute_func: CommandExecuteFunc<SE>,
    pub content_func: CommandContentFunc,
    pub disabled: bool,
    pub special_builtin: bool,
    pub declaration_builtin: bool,
    pub state_init: fn() -> Box<dyn AnyState>,
    _shared: PhantomData<S>,
}
```

The phantom `S` is for compile-time routing only. When stored in
`Shell.builtins`, the phantom is erased (see Storage section below).

### Clone impl

```rust
impl<SE: ShellExtensions, S> Clone for Registration<SE, S> {
    fn clone(&self) -> Self {
        Self {
            execute_func: self.execute_func,
            content_func: self.content_func,
            disabled: self.disabled,
            special_builtin: self.special_builtin,
            declaration_builtin: self.declaration_builtin,
            state_init: self.state_init,
            _shared: PhantomData,
        }
    }
}
```

### `.special()` preserved

```rust
impl<SE: ShellExtensions, S> Registration<SE, S> {
    pub const fn special(self) -> Self { /* same fields, PhantomData */ }
}
```

## Storage on Shell

```rust
// shell.rs
pub struct Shell<SE: ShellExtensions = DefaultShellExtensions> {
    // ... existing fields ...
    builtins: HashMap<String, ErasedRegistration<SE>>,
    builtin_states: HashMap<String, Box<dyn AnyState>>,
    shared_states: HashMap<TypeId, Box<dyn AnyState>>,  // NEW
}
```

### ErasedRegistration

Since `builtins` stores registrations after the phantom is erased:

```rust
// Type-erased registration for storage
pub struct ErasedRegistration<SE: ShellExtensions> {
    pub execute_func: CommandExecuteFunc<SE>,
    pub content_func: CommandContentFunc,
    pub disabled: bool,
    pub special_builtin: bool,
    pub declaration_builtin: bool,
}
```

Or simpler: just keep `Registration<SE, ()>` as the stored type (phantom = ()
means nothing). The phantom is only meaningful at the factory/registration
boundary.

**Decision needed**: `ErasedRegistration` vs `Registration<SE, ()>` for storage.

### Shell::clone()

```rust
// shared_states cloned via Box<dyn AnyState>::clone_box()
// Arc<T> clones cheaply (bumps refcount)
shared_states: self.shared_states
    .iter()
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect(),
```

## Shell Registration Methods

```rust
impl<SE: ShellExtensions> Shell<SE> {
    /// Register a builtin with no shared state.
    /// Only accepts Registration<SE, ()>.
    pub fn register_builtin<S>(&mut self, name: S, reg: Registration<SE, ()>)
    where S: Into<String>
    {
        let key = name.into();
        self.builtins.insert(key.clone(), reg.into_erased());
        self.builtin_states
            .entry(key)
            .or_insert_with(reg.state_init);
    }

    /// Register a builtin only if no builtin with that name is already registered.
    pub fn register_builtin_if_unset<S>(&mut self, name: S, reg: Registration<SE, ()>)
    where S: Into<String>
    {
        let key = name.into();
        if self.builtins.contains_key(&key) { return; }
        self.register_builtin(key, reg);
    }

    /// Bulk-register builtins that share state.
    pub fn register_shared<T>(&mut self, builder: SharedBuilder<T, SE>)
    where T: Clone + Send + Sync + 'static
    {
        self.shared_states.insert(TypeId::of::<T>(), Box::new(builder.value));
        for (name, reg, local_state) in builder.builtins {
            let key = name;
            self.builtins.insert(key.clone(), reg.into_erased());
            match local_state {
                Some(state) => { self.builtin_states.insert(key, state); }
                None => { self.builtin_states.entry(key).or_insert_with(reg.state_init); }
            }
        }
    }

    /// Get a handle to register more builtins against an existing shared state.
    pub fn shared_handle<T>(&mut self) -> SharedHandle<'_, T, SE>
    where T: Clone + Send + Sync + 'static
    {
        SharedHandle { shell: self, _phantom: PhantomData }
    }
}
```

### Removed methods

- `register_builtin_with_state` — unnecessary. Local state is seeded by
  `state_init` (default). Custom local state for shared builtins goes through
  `SharedBuilder::builtin_with_state`.

## SharedBuilder (consuming, like ShellBuilder)

```rust
pub struct SharedBuilder<T, SE: ShellExtensions = DefaultShellExtensions> {
    value: T,
    builtins: Vec<(String, Registration<SE, T>, Option<Box<dyn AnyState>>)>,
}

impl<T: Clone + Send + Sync + 'static, SE: ShellExtensions> SharedBuilder<T, SE> {
    pub fn new(value: T) -> Self {
        Self { value, builtins: Vec::new() }
    }

    /// Add a builtin that shares state type T.
    /// Compile error if Registration's shared type != T.
    pub fn builtin(mut self, name: impl Into<String>, reg: Registration<SE, T>) -> Self {
        self.builtins.push((name.into(), reg, None));
        self
    }

    /// Add a builtin with a custom local state override.
    pub fn builtin_with_state<S>(
        mut self,
        name: impl Into<String>,
        reg: Registration<SE, T>,
        state: S,
    ) -> Self
    where S: Clone + Send + Sync + 'static
    {
        self.builtins.push((name.into(), reg, Some(Box::new(state))));
        self
    }
}
```

Consumed by `shell.register_shared(builder)`. No `let mut` needed.

## SharedHandle (&mut shell, registers immediately)

```rust
pub struct SharedHandle<'a, T, SE: ShellExtensions> {
    shell: &'a mut Shell<SE>,
    _phantom: PhantomData<T>,
}

impl<T: Clone + Send + Sync + 'static, SE: ShellExtensions> SharedHandle<'_, T, SE> {
    /// Register a builtin against an existing shared state.
    pub fn builtin(&mut self, name: impl Into<String>, reg: Registration<SE, T>) {
        let key = name.into();
        self.shell.builtins.insert(key.clone(), reg.into_erased());
        self.shell.builtin_states
            .entry(key)
            .or_insert_with(reg.state_init);
    }

    /// Register a builtin with custom local state.
    pub fn builtin_with_state<S>(
        &mut self,
        name: impl Into<String>,
        reg: Registration<SE, T>,
        state: S,
    ) where S: Clone + Send + Sync + 'static
    {
        let key = name.into();
        self.shell.builtins.insert(key.clone(), reg.into_erased());
        self.shell.builtin_states.insert(key, Box::new(state));
    }
}
```

No terminal method — registers immediately on each call.

## ExecutionContext Accessors

```rust
impl<SE: ShellExtensions> ExecutionContext<'_, SE> {
    pub fn shared<T: Clone + Send + Sync + 'static>(&self) -> Result<&T, Error> {
        self.shell.shared_states
            .get(&TypeId::of::<T>())
            .and_then(|s| (**s).as_any().downcast_ref::<T>())
            .ok_or_else(|| ErrorKind::SharedStateNotRegistered(type_name::<T>()).into())
    }

    pub fn shared_mut<T: Clone + Send + Sync + 'static>(&mut self) -> Result<&mut T, Error> {
        let name = type_name::<T>().to_string();
        self.shell.shared_states
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| (**s).as_any_mut().downcast_mut::<T>())
            .ok_or_else(|| ErrorKind::SharedStateNotRegistered(name).into())
    }
}
```

## Shell Raw Accessors (for direct use, not through builtins)

```rust
fn set_shared<T: Clone + Send + Sync + 'static>(&mut self, state: T) {
    self.shared_states.insert(TypeId::of::<T>(), Box::new(state));
}

fn shared<T: 'static>(&self) -> Option<&T> {
    self.shared_states
        .get(&TypeId::of::<T>())
        .and_then(|s| (**s).as_any().downcast_ref::<T>())
}

fn shared_mut<T: 'static>(&mut self) -> Option<&mut T> {
    self.shared_states
        .get_mut(&TypeId::of::<T>())
        .and_then(|s| (**s).as_any_mut().downcast_mut::<T>())
}
```

## Compile-time Guarantees

| Code | Result |
|---|---|
| `register_builtin("die", builtin::<DieCommand, _>())` | ✓ `Registration<SE, ()>` accepted |
| `register_builtin("inherit", builtin::<InheritCommand, _>())` | ✗ `Registration<SE, Arc<RepoCache>>` rejected — type mismatch |
| `SharedBuilder.builtin("inherit", builtin::<InheritCommand, _>())` | ✓ types match |
| `SharedBuilder.builtin("die", builtin::<DieCommand, _>())` | ✗ `Registration<SE, ()>` ≠ `T` — type mismatch |
| `shared_handle.builtin("inherit", builtin::<InheritCommand, _>())` | ✓ types match |
| `shared_handle.builtin("die", builtin::<DieCommand, _>())` | ✗ type mismatch |

## Usage in portage-repo

### InheritCommand

```rust
impl Command for InheritCommand {
    type State = InheritState;              // per-invocation: inherited list
    type SharedState = Arc<RepoCache>;      // cross-builtin: eclass AST cache

    async fn execute<SE>(&self, mut ctx: ExecutionContext<'_, SE>) -> ... {
        let state = self.state_mut::<SE>(&mut ctx)?;
        let cache = self.shared::<SE>(&ctx)?;
        // ...
    }
}
```

### Shell setup

```rust
let cache = SharedBuilder::new(Arc::new(RepoCache::default()))
    .builtin("inherit", builtin::<InheritCommand, _>());
//  .builtin("has_version", builtin::<HasVersionCommand, _>());
shell.register_shared(cache);

shell.register_builtin("die", builtin::<DieCommand, _>());
shell.register_builtin("use", builtin::<UseCommand, _>());
// ...
```

### Subshell behavior

`Arc<RepoCache>` in `shared_states` — `Shell::clone()` calls
`AnyState::clone_box()` which clones the `Arc` (cheap refcount bump).
All subshells share the same underlying cache. Interior mutability
via papaya's lock-free maps.

## Error Type

```rust
// error.rs
pub enum ErrorKind {
    // ... existing variants ...
    SharedStateNotRegistered(String),
}
```

## Documentation Requirements

Heavy doc comments on:
- `SharedBuilder` — purpose, subshell cloning behavior (`T::clone()`),
  `Arc<T>` for sharing, interior mutability requirements, newtype pattern
  for uniqueness
- `register_shared` — seeds shared state + registers builtins atomically
- `shared`/`shared_mut` — runtime error if type not registered
- `set_shared` — for direct use, can override shared state
- Module-level note: `builtin_states` (per-builtin, string-keyed) vs
  `shared_states` (cross-builtin, type-keyed via `SharedBuilder`)

### Footguns to document

1. **Bare `T` vs `Arc<T>`**: bare `T` deep-copies on `Shell::clone()` (each
   subshell gets independent state). `Arc<T>` shares. Use `Arc<T>` when state
   should be visible across subshells.
2. **Interior mutability**: `Arc<T>` only gives shared references. To mutate
   through it, `T` needs interior mutability (e.g., `Mutex`, `papaya::HashMap`,
   atomics).
3. **Uniqueness by type**: `TypeId::of::<T>()` is the key. Two unrelated uses
   of the same generic type (e.g., `HashMap<String, String>`) would collide.
   Use newtype wrappers for isolation.
4. **Ordering**: `register_shared` must be called before builtins execute.
   Accessing unregistered shared state returns a runtime error.
5. **Re-entrancy**: same rules as `builtin_state_mut` — drop the `&mut`
   reference before calling back into the shell.

## Tests

- `SharedBuilder::new` + `register_shared` seeds shared state
- `shared::<T>()` retrieves correct type
- Two builtins registered through same builder see same shared state
- `Shell::clone()` shares `Arc<T>` (Arc strong count increases)
- Type isolation: two `SharedBuilder<T>` with different `T` don't collide
- Runtime error: accessing unregistered type returns `Err`
- `set_shared` overwrites previously seeded value
- `shared_handle` registers against existing shared state
- `register_builtin` rejects `Registration<SE, NonUnitType>` (compile-time test)

## Files to Modify in brush-core

| File | Change |
|---|---|
| `builtins.rs` | `Registration<SE, S>` phantom, factory functions return typed phantoms, add `SharedBuilder`, add `SharedHandle`, `ErasedRegistration` or equivalent |
| `shell.rs` | `shared_states` field, update `new()` and `clone()`, update `builtins` field type |
| `shell/builtin_registry.rs` | `register_builtin` accepts `Registration<SE, ()>`, add `register_shared`, `shared_handle`, `set_shared`/`shared`/`shared_mut` |
| `commands.rs` | `shared`/`shared_mut` on `ExecutionContext` |
| `shell/builder.rs` | Update `builtins` field type from `HashMap<String, Registration<SE>>` to `HashMap<String, Registration<SE, ()>>` |
| `error.rs` | `SharedStateNotRegistered` variant |
| `lib.rs` | Export `SharedBuilder`, `SharedHandle`, `ErasedRegistration` |

## Files to Modify in portage-repo

| File | Change |
|---|---|
| `src/shell.rs` | Use `SharedBuilder` for inherit, `register_builtin` for others |
| `src/inherit.rs` | Remove cache from `InheritState`, use `context.shared()` |
| New `src/cache.rs` | `RepoCache` struct |

## Backward Compatibility Concerns

- `default_builtins()` returns `HashMap<String, Registration<SE, ()>>` instead
  of `HashMap<String, Registration<SE>>`. All callers must be updated.
- `ShellBuilder::builtin(name, reg)` — `builtins` field type changes.
- `ShellBuilder::builtins(iter)` — same.
- `.clone()` on `Registration` — phantom must be preserved.
- All `impl Command` blocks that set `type SharedState = ()` explicitly —
  not needed, `()` is the default. But if they already exist (from the
  stateful-builtins PR), they're fine.

## What Was Removed

- `register_builtin_with_state` — unnecessary. Local state seeded by `state_init`
  (default). Custom local state for shared builtins goes through
  `SharedBuilder::builtin_with_state`.
- `with_shared()` on Registration — loophole that bypasses SharedBuilder.
- `with_state()` on Registration — not needed. Custom local state goes through
  `SharedBuilder::builtin_with_state` or `SharedHandle::builtin_with_state`.
- `NeedsLocal` typestate — local state is always optional, handled by
  `state_init` default or explicit override in SharedBuilder/SharedHandle.
- `local_state_override` field on Registration — carried internally by
  SharedBuilder instead.

## Key Decisions

1. **Phantom on `Registration`** encodes shared state type for compile-time
   routing. Stored erased in `Shell.builtins`.
2. **`SharedBuilder` is consuming** (like `ShellBuilder`) — no `let mut`.
3. **`SharedHandle` borrows `&mut Shell`** — registers immediately, no terminal
   method.
4. **No `with_shared()`** — forces all shared-state registration through
   `SharedBuilder` or `SharedHandle`.
5. **No `register_builtin_with_state`** — local state always seeded by default
   or through builder/handle methods.
6. **`AnyState` trait unchanged** — blanket impl handles all types including
   `Arc<T>`.
7. **`TypeId` as key** — not `type_name`. Zero-cost, collision-free.
   But doesn't support trait objects (acceptable).
