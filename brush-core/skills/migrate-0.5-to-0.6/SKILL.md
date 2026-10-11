---
name: migrate-brush-core-0.5-to-0.6
description: Migrate a crate that embeds brush-core 0.5 (custom builtins, registrations, help content, completion enums) to brush-core 0.6's engine-neutral builtin contracts.
depends-on: brush-core=0.6
---

# Migrating brush-core 0.5 → 0.6

Machine-oriented migration spec. Apply the rules below to a codebase that
depends on `brush-core 0.5` so that it compiles and behaves correctly against
`brush-core 0.6`. The human walkthrough of the high-traffic changes is
`docs/migrations/brush-core/0.5-to-0.6.md` in the brush repository.

Scope: `brush_core::builtins` and `brush_core::completion`. Nothing else in
brush-core changed incompatibly.

## Execution model

1. Apply every MECHANICAL rule, in order. Match whole identifiers only.
2. Apply each CONDITIONAL rule where its Detection matches.
3. Review every SILENT item. These compile unchanged and behave differently;
   the compiler will not find them.
4. Run `cargo build --all-targets`. Any remaining error names a symbol that
   appears in exactly one entry below.
5. Do not introduce compatibility shims (re-exports of removed names, wrapper
   traits). Migrate the call sites.

## Required Cargo.toml changes

```toml
# Before
brush-core = "0.5"

# After
brush-core = "0.6"
# Only if any of your builtins derive clap::Parser (entry 1):
brush-builtin-utils = "0.1"
clap = { version = "4", features = ["derive"] }
```

`brush-core` no longer depends on `clap`. A crate that used `clap` only through
brush-core's dependency must now depend on it directly (entry 10).

## Breaking changes

### 1. MECHANICAL — `Command` no longer requires `clap::Parser`

`builtins::Command`'s supertrait is now `FromArgs + HelpContent`. Its `new`,
`takes_plus_options`, and `get_content` methods are gone; `execute` is its only
method.

Detection: `grep -rn -E "impl.*\bCommand for\b"`; every implementing type that
also has `#[derive(Parser)]` or `#[derive(clap::Parser)]`.

Action: after the struct, add one `clap_builtin!` invocation, choosing the form
by what the old impl overrode:

| Old impl contained | New invocation |
|---|---|
| nothing beyond `execute` | `brush_builtin_utils::clap_builtin!(Type);` |
| `fn new` using `try_parse_known` to keep `--` | `brush_builtin_utils::clap_builtin!(Type, trailing_args = field);` where `field` is the `Vec<String>` the override extended |
| `fn takes_plus_options() -> bool { true }` | nothing extra; `+x` options are inferred from `long = "+x"` declarations on the derive |
| `fn get_content` | do not use the macro (it also implements `HelpContent`). Implement `HelpContent` by hand (entry 4) and implement `FromArgs` with a one-line body: `brush_builtin_utils::clap_adapter::parse::<Self>(name, args)` |

Then delete `new`, `takes_plus_options`, and `get_content` from the `impl Command`.

```rust
// Before
#[derive(Parser)]
struct GreetCommand { /* ... */ }

impl builtins::Command for GreetCommand {
    type Error = brush_core::Error;
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where I: IntoIterator<Item = String>
    {
        let (mut this, rest) = brush_core::builtins::try_parse_known::<Self>(args)?;
        if let Some(rest) = rest { this.args.extend(rest); }
        Ok(this)
    }
    async fn execute<SE: ShellExtensions>(&self, context: ExecutionContext<'_, SE>) -> Result<ExecutionResult, Self::Error> { /* ... */ }
}

// After
#[derive(Parser)]
struct GreetCommand { /* ... */ }

brush_builtin_utils::clap_builtin!(GreetCommand, trailing_args = args);

impl builtins::Command for GreetCommand {
    type Error = brush_core::Error;
    async fn execute<SE: ShellExtensions>(&self, context: ExecutionContext<'_, SE>) -> Result<ExecutionResult, Self::Error> { /* ... */ }
}
```

Verify: `cargo build`. A misuse of the macro fails with a message naming its
accepted forms.

### 2. MECHANICAL — `SimpleCommand` and `simple_builtin` removed

Detection: `grep -rn -E "builtins::SimpleCommand|\bSimpleCommand for\b|\bsimple_builtin\b"`.
(`brush_core::commands::SimpleCommand`, the command struct, is unrelated and
unchanged.)

Action: a `SimpleCommand` that ignored its arguments becomes a
`verbatim_builtin!`; one that read them implements `FromArgs` by hand. In both
cases `execute` becomes the async `Command::execute` taking `&self`, and the
registration becomes `builtin::<Type, _>()`.

```rust
// Before
impl builtins::SimpleCommand for TrueCommand {
    fn get_content(name: &str, content_type: ContentType, _o: &ContentOptions) -> Result<String, Error> {
        match content_type {
            ContentType::DetailedHelp => Ok("Returns a successful exit status.".into()),
            ContentType::ShortUsage => Ok("true".into()),
            ContentType::ShortDescription => Ok("true - success".into()),
            ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }
    fn execute<SE: ShellExtensions, I: Iterator<Item = S>, S: AsRef<str>>(_context: ExecutionContext<'_, SE>, _args: I) -> Result<ExecutionResult, Error> {
        Ok(ExecutionResult::success())
    }
}
m.insert("true".into(), simple_builtin::<TrueCommand, SE>());

// After
brush_builtin_utils::verbatim_builtin!(
    TrueCommand,
    synopsis = "true",
    description = "success",
    help = "Returns a successful exit status.\n",
);

impl builtins::Command for TrueCommand {
    type Error = brush_core::Error;
    async fn execute<SE: ShellExtensions>(&self, _context: ExecutionContext<'_, SE>) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
m.insert("true".into(), builtin::<TrueCommand, _>());
```

Note the short forms lose their framing: `"true"` not `"true: true\n"`, and
`"success"` not `"true - success\n"` (entry 4).

### 3. MECHANICAL — `DeclarationCommand`, `decl_builtin`, `raw_arg_builtin` removed

Detection: `grep -rn -E "\b(DeclarationCommand|set_declarations|decl_builtin|raw_arg_builtin)\b"`.

Action, by case:

| Old | New |
|---|---|
| clap type + `impl DeclarationCommand` + `decl_builtin::<T, SE>()` | delete the impl; `clap_builtin!(T, declarations = field);` (keep the `#[clap(skip)]` field); register with `builtin::<T, _>()` |
| `Default` clap type + `impl DeclarationCommand` + `raw_arg_builtin::<T, SE>()` | delete the impl and the derive; `verbatim_builtin!(T, args = field, synopsis = "...", description = "...");` register with `builtin::<T, _>()` |
| hand-written type | in `impl FromArgs`, add `const TAKES_DECLARATIONS: bool = true;` and take the operands out of `args` yourself |

Behavior: the shell no longer splits options from operands for you. The
adapter's `declarations` mode applies bash's rule (options are the leading
words starting with `-` or `+`; the first operand or a `--` ends them). The old
core splitter treated every dash-prefixed word as an option regardless of
position.

### 4. MECHANICAL — `ContentType`, `CommandContentFunc`, and `get_content` replaced by `HelpContent`

Detection: `grep -rn -E "\b(ContentType|CommandContentFunc|content_func|get_content)\b"`.

Action for providers: implement `builtins::HelpContent`. The short forms are
unframed; `help -s` and `help -d` add `name: ` and `name - ` and the newline.
`ManPage` has no replacement (`help -m` reports unimplemented on its own).

```rust
// Before (inside impl Command or impl SimpleCommand)
fn get_content(name: &str, content_type: ContentType, options: &ContentOptions) -> Result<String, Error> {
    match content_type {
        ContentType::ShortUsage => Ok(format!("{name}: {name} [-n count]\n")),
        ContentType::ShortDescription => Ok(format!("{name} - Greet the user\n")),
        ContentType::DetailedHelp => Ok(render_help(options.colorized)),
        ContentType::ManPage => error::unimp("man page not yet implemented"),
    }
}

// After (a separate impl)
impl builtins::HelpContent for GreetCommand {
    fn synopsis(name: &str) -> String { format!("{name} [-n count]") }
    fn description(_name: &str) -> String { "Greet the user".into() }
    fn detailed_help(_name: &str, options: &ContentOptions) -> Result<String, Error> {
        Ok(render_help(options.colorized))
    }
}
```

`detailed_help` has a default (synopsis, newline, indented description), so a
minimal implementation is the two one-liners.

Action for consumers of a `Registration`:

| Old | New |
|---|---|
| `(reg.content_func)(name, ContentType::ShortUsage, &o)` | `format!("{name}: {}\n", reg.synopsis(name))` |
| `(reg.content_func)(name, ContentType::ShortDescription, &o)` | `format!("{name} - {}\n", reg.description(name))` |
| `(reg.content_func)(name, ContentType::DetailedHelp, &o)` | `reg.detailed_help(name, &o)` |
| `(reg.content_func)(name, ContentType::ManPage, &o)` | none; report unimplemented |

### 5. MECHANICAL — `Registration` fields are private

Detection: `grep -rn -E "Registration \{|\.(execute_func|content_func|disabled|special_builtin|declaration_builtin)\b"`.

Action:

| Old | New |
|---|---|
| `Registration { execute_func: f, content_func: c, disabled: false, special_builtin: false, declaration_builtin: false }` | `Registration::new::<H>(f)` where `H: HelpContent` provides what `c` did (a unit struct is enough); `.special()` if it was special |
| `reg.execute_func` (read) | `reg.execute_func()` |
| `reg.disabled` (read) | `reg.is_disabled()` |
| `reg.disabled = b` | `reg.set_disabled(b)` |
| `reg.special_builtin` | `reg.is_special()` |
| `reg.declaration_builtin` | `reg.takes_declarations()` |
| `reg.content_func` | entry 4 |

A registration for a bare function never takes declarations. If yours did,
convert it to a `Command` type with `TAKES_DECLARATIONS` (entry 3).

### 6. SILENT — execute functions no longer receive the invoked name as `args[0]`

This compiles unchanged and drops the builtin's first real argument.

Detection: every function passed to `Registration::new` or previously stored in
`Registration::execute_func`; inside it, `args[0]`, `args.first()`,
`args.iter().skip(1)`, `args.into_iter().skip(1)`, `args.remove(0)`,
`args.split_first()`, or any comment saying the name is `args[0]`.

Action: delete the skip. The name is `context.command_name`.

```rust
// Before
fn shim_execute<SE: ShellExtensions>(context: ExecutionContext<'_, SE>, args: Vec<CommandArg>) -> BoxFuture<'_, Result<ExecutionResult, Error>> {
    let name = args[0].to_string();
    let rest: Vec<_> = args.into_iter().skip(1).collect();
    /* ... */
}

// After
fn shim_execute<SE: ShellExtensions>(context: ExecutionContext<'_, SE>, args: Vec<CommandArg>) -> BoxFuture<'_, Result<ExecutionResult, Error>> {
    let name = context.command_name.clone();
    let rest = args;
    /* ... */
}
```

Verify: run the builtin with exactly one argument and confirm the argument is
seen. `Command` types are unaffected: `FromArgs::from_args` receives the name
as its `name` parameter.

### 7. MECHANICAL — `parse_known` and `try_parse_known` removed

Detection: `grep -rn -E "\b(parse_known|try_parse_known)\b"`.

Action: inside a builtin, use `clap_builtin!(T, trailing_args = field)` (entry
1) or call `brush_builtin_utils::clap_adapter::parse_with_trailing::<T>(name, args)`,
which returns `(T, Vec<String>)` where the vector is `--` and everything after
it, or empty. Outside a builtin (a CLI), vendor the helper:

```rust
fn try_parse_known<T: clap::Parser>(
    args: impl IntoIterator<Item = String>,
) -> Result<(T, Option<impl Iterator<Item = String>>), clap::Error> {
    let mut args = args.into_iter();
    let mut hyphen = None;
    let before = args.by_ref().take_while(|a| {
        let is_hyphen = a == "--";
        if is_hyphen { hyphen = Some(a.clone()); }
        !is_hyphen
    });
    let parsed = T::try_parse_from(before)?;
    Ok((parsed, hyphen.map(|h| std::iter::once(h).chain(args))))
}
```

### 8. MECHANICAL — `ContentOptions` is `#[non_exhaustive]`

Detection: `grep -rn "ContentOptions {"`.

Action:

```rust
// Before
let options = ContentOptions { colorized: true };
// After
let mut options = ContentOptions::default();
options.colorized = true;
```

### 9. MECHANICAL — `CompleteAction` and `CompleteOption` no longer implement `clap::ValueEnum`

Detection: `grep -rn -E "\b(CompleteAction|CompleteOption)\b"` combined with
`ValueEnum`, `to_possible_value`, `value_variants`, or a `#[arg]` field of
either type.

Action: both now implement `Display`, `FromStr` (error `strum::ParseError`),
`strum::VariantNames`, and `Copy`, with the names `complete -A` / `-o` use
(`arrayvar`, `nospace`, ...). For a clap field:

```rust
// Before
#[arg(short = 'A')]
actions: Vec<CompleteAction>,

// After
#[arg(short = 'A', value_parser = clap::builder::PossibleValuesParser::new(CompleteAction::VARIANTS)
    .try_map(|s| s.parse::<CompleteAction>()))]
actions: Vec<CompleteAction>,
```

`use strum::VariantNames;` and `use clap::builder::TypedValueParser;` are
required for `VARIANTS` and `try_map`.

`GenerationOptions` is now `BTreeSet<CompleteOption>` rather than a struct of
bools. Detection: `grep -rn "options\.\(no_space\|file_names\|dir_names\|plus_dirs\|no_quote\|no_sort\|bash_default\|default\)\b"`.

```rust
// Before
if spec.options.no_space { ... }
spec.options.file_names = true;

// After
if spec.options.contains(&CompleteOption::NoSpace) { ... }
spec.options.insert(CompleteOption::FileNames);
```

### 10. CONDITIONAL — `clap` must be a direct dependency

Detection: your crate's Cargo.toml lacks `clap` but your code contains `clap::`
or `#[derive(Parser)]`.

Action: add `clap = { version = "4", features = ["derive"] }`.

### 11. CONDITIONAL — example relocated

Detection: documentation or scripts referring to
`cargo run --package brush-core --example custom-builtin` expecting a clap
example.

Action: the brush-core example is now hand-written and engine-free. The clap
example is `cargo run --package brush-builtin-utils --example clap-builtin`.

## Additive APIs (no action required)

- `ShellBuilder::command::<T>(name)`: shorthand for
  `.builtin(name, builtins::builtin::<T, _>())`.
- `builtins::builtin::<T, SE>()` is `const`; `SE` is inferable, write `_`.
- `brush_builtin_utils::verbatim_builtin!`.

## Behavior changes (no code change; check tests)

- `declare`, `typeset`, `local`, `readonly`, `export`: options end at the first
  operand, as in bash. `declare a=1 -x` now treats `-x` as a name.
- `export` rejects operands that are not valid identifiers with status 1.
- `help -s` and `help -d` print one framed line per builtin.
- `help builtin` prints fixed bash-style text; `help :`, `help true`,
  `help false` end with a newline.

## VERIFY

```sh
cargo build --all-targets
grep -rn -E "builtins::SimpleCommand|\bSimpleCommand for\b|\b(DeclarationCommand|set_declarations|simple_builtin|decl_builtin|raw_arg_builtin|ContentType|CommandContentFunc|takes_plus_options|get_content)\b|builtins::(try_)?parse_known" src/
```

The grep must print nothing (a vendored copy of `try_parse_known` from entry 7
is expected and is not matched). Then run your test suite. For each
bare-function builtin (entry 6), run it with one argument and confirm the
argument is not dropped.
