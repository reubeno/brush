//! Facilities for implementing and managing builtins

use clap::builder::styling;
pub use futures::future::BoxFuture;
use std::any::{Any, TypeId, type_name};
use std::io::Write;
use std::marker::PhantomData;

use crate::{BuiltinError, CommandArg, commands, error, extensions, results};

/// A type-erased, cloneable container for per-builtin state stored on the shell.
///
/// Any `T: Clone + Send + Sync + 'static` automatically implements this trait
/// thanks to a blanket impl.
///
/// # Important: calling methods on `Box<dyn AnyState>`
///
/// Because `Box<dyn AnyState>` itself satisfies `Clone + Send + Sync + 'static`,
/// the blanket impl also applies to it. When calling `as_any`, `as_any_mut`, or
/// `clone_box` on a `Box<dyn AnyState>`, you **must** explicitly dereference
/// first (e.g. `(&**state).as_any()`) so that dispatch goes through the vtable to
/// the concrete inner type, rather than the blanket impl on `Box<dyn AnyState>`
/// itself. The accessors on [`Shell`](crate::Shell) and
/// [`ExecutionContext`](crate::commands::ExecutionContext) already handle this
/// correctly.
pub trait AnyState: Send + Sync + 'static {
    /// Deep-clone the state into a new heap allocation.
    fn clone_box(&self) -> Box<dyn AnyState>;

    /// Downcast to `&dyn Any` for typed access.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to `&mut dyn Any` for typed mutable access.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Clone + Send + Sync + 'static> AnyState for T {
    fn clone_box(&self) -> Box<dyn AnyState> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Clone for Box<dyn AnyState> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}

/// Marker type indicating that a [`Registration`] has not yet been given a
/// custom local-state override via [`Registration::with_state`].
///
/// The phantom parameter `St` carries the builtin's `State` type so that
/// `with_state(state)` can enforce the correct argument type at compile time.
pub struct NeedsLocalState<St>(PhantomData<St>);

/// Marker type indicating that a [`Registration`] is in its storage/terminal
/// form.
///
/// Either local state was provided via [`Registration::with_state`], or the
/// registration was produced by [`simple_builtin`] (which has no local state
/// concept). In this state, [`with_state`](Registration::with_state) is not
/// available, preventing double-provision.
pub struct HasLocalState;

/// Type of a function implementing a built-in command.
///
/// # Arguments
///
/// * The context in which the command is being executed.
/// * The arguments to the command.
#[allow(type_alias_bounds)]
pub type CommandExecuteFunc<SE: extensions::ShellExtensions> =
    fn(
        commands::ExecutionContext<'_, SE>,
        Vec<commands::CommandArg>,
    ) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>>;

/// Type of a function to retrieve help content for a built-in command.
///
/// # Arguments
///
/// * `name` - The name of the command.
/// * `content_type` - The type of content to retrieve.
/// * `options` - Additional options for content retrieval.
pub type CommandContentFunc =
    fn(&str, ContentType, &ContentOptions) -> Result<String, error::Error>;

/// Trait implemented by built-in shell commands.
pub trait Command: clap::Parser {
    /// The error type returned by the command.
    type Error: BuiltinError + 'static;

    /// The type of persistent state carried by this builtin across invocations.
    ///
    /// Stateful builtins override this with a custom type that implements
    /// `Clone + Default + Send + Sync + 'static`. The shell allocates a default
    /// instance at registration time and stores it keyed by the builtin's
    /// registered name. Builtins access state through
    /// [`ExecutionContext::builtin_state_mut`] and external code uses
    /// [`Shell::builtin_state_of`] / [`Shell::builtin_state_mut_of`].
    type State: Clone + Default + Send + Sync + 'static;

    /// The type of shared state that this builtin accesses in coordination with
    /// other builtins registered through the same [`SharedBuilder`].
    ///
    /// Unlike [`State`](Command::State), shared state is **not** per-builtin —
    /// it is keyed by type (`TypeId`) and shared across all builtins registered
    /// through the same builder. The default `()` means "no shared state".
    ///
    /// Use `Arc<T>` when state should survive `Shell::clone()` (subshells
    /// share the same underlying data). Use a bare `T` when each subshell
    /// should get an independent copy (via `T::clone`).
    ///
    /// To mutate shared state, `T` must provide interior mutability (e.g.
    /// `Mutex`, `papaya::HashMap`, atomics).
    type SharedState: Clone + Send + Sync + 'static;

    /// Returns a shared reference to this builtin's persistent state.
    ///
    /// This is a convenience wrapper around
    /// [`ExecutionContext::builtin_state`] that infers the command type
    /// from `&self`, so no turbofish is needed.
    fn state<'a, SE: extensions::ShellExtensions>(
        &self,
        context: &'a commands::ExecutionContext<'_, SE>,
    ) -> Result<&'a Self::State, error::Error> {
        context.builtin_state::<Self>()
    }

    /// Returns an exclusive reference to this builtin's persistent state.
    ///
    /// This is a convenience wrapper around
    /// [`ExecutionContext::builtin_state_mut`] that infers the command type
    /// from `&self`, so no turbofish is needed.
    ///
    /// The caller must drop the returned reference before calling any other
    /// `&mut Shell` method (including `source_script`), so that re-entrant
    /// builtin invocations can access state independently.
    fn state_mut<'a, SE: extensions::ShellExtensions>(
        &self,
        context: &'a mut commands::ExecutionContext<'_, SE>,
    ) -> Result<&'a mut Self::State, error::Error> {
        context.builtin_state_mut::<Self>()
    }

    /// Returns a shared reference to this builtin's shared state.
    ///
    /// This is a convenience wrapper around
    /// [`ExecutionContext::shared`] that infers the shared-state type
    /// from `Self::SharedState`, so no turbofish is needed.
    fn shared<'a, SE: extensions::ShellExtensions>(
        &self,
        context: &'a commands::ExecutionContext<'_, SE>,
    ) -> Result<&'a Self::SharedState, error::Error> {
        context.shared::<Self::SharedState>()
    }

    /// Instantiates the built-in command with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments to the command.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        if !Self::takes_plus_options() {
            Self::try_parse_from(args)
        } else {
            let args = args.into_iter();

            let (lower, _) = args.size_hint();

            // N.B. clap doesn't support named options like '+x'. To work around this, we
            // establish a pattern of renaming them.
            let mut updated_args = Vec::with_capacity(lower);
            for arg in args {
                if let Some(plus_options) = arg.strip_prefix("+") {
                    updated_args.extend(plus_options.chars().map(|c| format!("--+{c}")));
                } else {
                    updated_args.push(arg);
                }
            }

            Self::try_parse_from(updated_args)
        }
    }

    /// Returns whether or not the command takes options with a leading '+' or '-' character.
    fn takes_plus_options() -> bool {
        false
    }

    /// Executes the built-in command in the provided context.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the command is being executed.
    // NOTE: we use desugared async here because we need a Send marker
    fn execute<SE: extensions::ShellExtensions>(
        &self,
        context: commands::ExecutionContext<'_, SE>,
    ) -> impl std::future::Future<Output = Result<results::ExecutionResult, Self::Error>>
    + std::marker::Send;

    /// Returns the textual help content associated with the command.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    /// * `content_type` - The type of content to retrieve.
    /// * `options` - Additional options for content retrieval.
    fn get_content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error> {
        let mut clap_command = Self::command()
            .styles(brush_help_styles())
            .next_line_help(false);
        clap_command.set_bin_name(name);

        let s = match content_type {
            ContentType::DetailedHelp => {
                let rendered = clap_command.render_help();
                if options.colorized {
                    rendered.ansi().to_string()
                } else {
                    rendered.to_string()
                }
            }
            ContentType::ShortUsage => get_builtin_short_usage(name, &clap_command),
            ContentType::ShortDescription => get_builtin_short_description(name, &clap_command),
            ContentType::ManPage => get_builtin_man_page(name, &clap_command)?,
        };

        Ok(s)
    }
}

/// Trait implemented by built-in shell commands that take specially handled declarations
/// as arguments.
pub trait DeclarationCommand: Command {
    /// Stores the declarations within the command instance.
    ///
    /// # Arguments
    ///
    /// * `declarations` - The declarations to store.
    fn set_declarations(&mut self, declarations: Vec<commands::CommandArg>);
}

/// Type of help content, typically associated with a built-in command.
pub enum ContentType {
    /// Detailed help content for the command.
    DetailedHelp,
    /// Short usage information for the command.
    ShortUsage,
    /// Short description for the command.
    ShortDescription,
    /// man-style help page.
    ManPage,
}

/// Options for retrieving built-in command content.
#[derive(Default)]
pub struct ContentOptions {
    /// Whether or not the content should be colorized.
    pub colorized: bool,
}

/// Encapsulates a registration for a built-in command.
///
/// # Type parameters
///
/// * `SE` — the [`ShellExtensions`](extensions::ShellExtensions) type.
/// * `S`  — the **shared-state** phantom. `S = ()` (default) means this
///   registration can be passed directly to
///   [`Shell::register_builtin`](crate::Shell::register_builtin). Any other
///   `S` (e.g. `Arc<RepoCache>`) means it must go through
///   [`SharedBuilder`] or [`SharedHandle`].
/// * `L`  — the **local-state** phantom, governing
///   [`with_state`](Registration::with_state) availability:
///   - [`NeedsLocalState<St>`] — `with_state` is available, takes `St`.
///   - [`HasLocalState`] — `with_state` is not available (terminal/storage form).
///
/// # Stored form
///
/// `Registration<SE>` (using both defaults) is the **stored form** used by
/// `Shell.builtins` and `ShellBuilder`. It is produced by calling
/// [`into_storage`](Registration::into_storage) on a freshly-created
/// registration.
pub struct Registration<SE: extensions::ShellExtensions, S = (), L = HasLocalState> {
    /// Function to execute the builtin.
    pub execute_func: CommandExecuteFunc<SE>,

    /// Function to retrieve the builtin's content/help text.
    pub content_func: CommandContentFunc,

    /// Has this registration been disabled?
    pub disabled: bool,

    /// Is the builtin classified as "special" by specification?
    pub special_builtin: bool,

    /// Is this builtin one that takes specially handled declarations?
    pub declaration_builtin: bool,

    /// Factory function that creates the default state for this builtin.
    /// Called by [`Shell::register_builtin`](crate::Shell::register_builtin)
    /// to seed the per-builtin state map.
    pub state_init: fn() -> Box<dyn AnyState>,

    /// Explicit local-state override set by [`with_state`](Registration::with_state).
    /// `None` means "use `state_init`".
    pub local_override: Option<Box<dyn AnyState>>,

    /// Shared-state phantom (`()` for stored form).
    pub _shared: PhantomData<S>,
    /// Local-state phantom (`HasLocalState` for stored form).
    pub _local: PhantomData<L>,
}

impl<SE: extensions::ShellExtensions, S, L> Clone for Registration<SE, S, L> {
    fn clone(&self) -> Self {
        Self {
            execute_func: self.execute_func,
            content_func: self.content_func,
            disabled: self.disabled,
            special_builtin: self.special_builtin,
            declaration_builtin: self.declaration_builtin,
            state_init: self.state_init,
            local_override: self.local_override.clone(),
            _shared: PhantomData,
            _local: PhantomData,
        }
    }
}

impl<SE: extensions::ShellExtensions, S, L> Registration<SE, S, L> {
    /// Updates the given registration to mark it for a special builtin.
    #[must_use]
    pub fn special(self) -> Self {
        Self {
            special_builtin: true,
            ..self
        }
    }

    /// Convert to the stored form (`Registration<SE, (), HasLocalState>`).
    ///
    /// This erases the shared-state phantom and transitions the local-state
    /// phantom to [`HasLocalState`], while preserving any
    /// [`with_state`](Registration::with_state) override in
    /// `local_override`.
    ///
    /// Called by [`stored_builtin`], [`stored_decl_builtin`],
    /// [`stored_raw_arg_builtin`], and
    /// [`ShellBuilder::builtin`](crate::ShellBuilder::builtin).
    pub(crate) fn into_storage(self) -> Registration<SE> {
        Registration {
            execute_func: self.execute_func,
            content_func: self.content_func,
            disabled: self.disabled,
            special_builtin: self.special_builtin,
            declaration_builtin: self.declaration_builtin,
            state_init: self.state_init,
            local_override: self.local_override,
            _shared: PhantomData,
            _local: PhantomData,
        }
    }

    /// Destruct into (stored registration, local-state override, `state_init` fn).
    ///
    /// For internal use by registration methods in `Shell` and
    /// [`SharedBuilder`]/[`SharedHandle`].
    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Registration<SE>,
        Option<Box<dyn AnyState>>,
        fn() -> Box<dyn AnyState>,
    ) {
        let state_init = self.state_init;
        let local_override = self.local_override;
        let stored = Registration {
            execute_func: self.execute_func,
            content_func: self.content_func,
            disabled: self.disabled,
            special_builtin: self.special_builtin,
            declaration_builtin: self.declaration_builtin,
            state_init,
            local_override: None,
            _shared: PhantomData,
            _local: PhantomData,
        };
        (stored, local_override, state_init)
    }
}

impl<SE: extensions::ShellExtensions, S, St: Clone + Send + Sync + 'static>
    Registration<SE, S, NeedsLocalState<St>>
{
    /// Provide a custom initial value for this builtin's local state,
    /// replacing the default produced by `B::State::default()`.
    ///
    /// The argument type is exactly `St` (= `B::State`), enforced at compile
    /// time by the [`NeedsLocalState<St>`] phantom.
    ///
    /// This method is only available once. After calling it the registration
    /// transitions to [`HasLocalState`] and `with_state` is no longer
    /// available.
    #[must_use]
    pub fn with_state(self, state: St) -> Registration<SE, S, HasLocalState> {
        Registration {
            execute_func: self.execute_func,
            content_func: self.content_func,
            disabled: self.disabled,
            special_builtin: self.special_builtin,
            declaration_builtin: self.declaration_builtin,
            state_init: self.state_init,
            local_override: Some(Box::new(state)),
            _shared: self._shared,
            _local: PhantomData,
        }
    }
}

fn get_builtin_man_page(_name: &str, _command: &clap::Command) -> Result<String, error::Error> {
    error::unimp("man page rendering is not yet implemented")
}

fn get_builtin_short_description(name: &str, command: &clap::Command) -> String {
    let about = command
        .get_about()
        .map_or_else(String::new, |s| s.to_string());

    std::format!("{name} - {about}\n")
}

fn get_builtin_short_usage(name: &str, command: &clap::Command) -> String {
    let mut usage = String::new();

    let mut needs_space = false;

    let mut optional_short_opts = vec![];
    let mut required_short_opts = vec![];
    for opt in command.get_opts() {
        if opt.is_hide_set() {
            continue;
        }

        if let Some(c) = opt.get_short() {
            if !opt.is_required_set() {
                optional_short_opts.push(c);
            } else {
                required_short_opts.push(c);
            }
        }
    }

    if !optional_short_opts.is_empty() {
        if needs_space {
            usage.push(' ');
        }

        usage.push('[');
        usage.push('-');
        for c in optional_short_opts {
            usage.push(c);
        }

        usage.push(']');
        needs_space = true;
    }

    if !required_short_opts.is_empty() {
        if needs_space {
            usage.push(' ');
        }

        usage.push('-');
        for c in required_short_opts {
            usage.push(c);
        }

        needs_space = true;
    }

    for pos in command.get_positionals() {
        if pos.is_hide_set() {
            continue;
        }

        if !pos.is_required_set() {
            if needs_space {
                usage.push(' ');
            }

            usage.push('[');
            needs_space = false;
        }

        if let Some(names) = pos.get_value_names() {
            for name in names {
                if needs_space {
                    usage.push(' ');
                }

                usage.push_str(name);
                needs_space = true;
            }
        }

        if !pos.is_required_set() {
            usage.push(']');
            needs_space = true;
        }
    }

    std::format!("{name}: {name} {usage}\n")
}

fn brush_help_styles() -> clap::builder::Styles {
    styling::Styles::styled()
        .header(
            styling::AnsiColor::Yellow.on_default()
                | styling::Effects::BOLD
                | styling::Effects::UNDERLINE,
        )
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Magenta.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

/// This function and the [`try_parse_known`] exists to deal with
/// the Clap's limitation of treating `--` like a regular value
/// `https://github.com/clap-rs/clap/issues/5055`
///
/// # Arguments
///
/// * `args` - An Iterator from [`std::env::args`]
///
/// # Returns
///
/// * a parsed struct T from [`clap::Parser::parse_from`]
/// * the remain iterator `args` with `--` and the rest arguments if they present otherwise None
///
/// # Examples
/// ```
///    use clap::{builder::styling, Parser};
///    #[derive(Parser)]
///    struct CommandLineArgs {
///       #[clap(allow_hyphen_values = true, num_args=1..)]
///       script_args: Vec<String>,
///    }
///
///    let (mut parsed_args, raw_args) =
///        brush_core::builtins::parse_known::<CommandLineArgs, _>(std::env::args());
///    if raw_args.is_some() {
///        parsed_args.script_args = raw_args.unwrap().collect();
///    }
/// ```
pub fn parse_known<T: clap::Parser, S>(
    args: impl IntoIterator<Item = S>,
) -> (T, Option<impl Iterator<Item = S>>)
where
    S: Into<std::ffi::OsString> + Clone + PartialEq<&'static str>,
{
    let mut args = args.into_iter();
    // the best way to save `--` is to get it out with a side effect while `clap` iterates over the
    // args this way we can be 100% sure that we have '--' and the remaining args
    // and we will iterate only once
    let mut hyphen = None;
    let args_before_hyphen = args.by_ref().take_while(|a| {
        let is_hyphen = *a == "--";
        if is_hyphen {
            hyphen = Some(a.clone());
        }
        !is_hyphen
    });
    let parsed_args = T::parse_from(args_before_hyphen);
    let raw_args = hyphen.map(|hyphen| std::iter::once(hyphen).chain(args));
    (parsed_args, raw_args)
}

/// Similar to [`parse_known`] but with [`clap::Parser::try_parse_from`]
/// This function is used to parse arguments in builtins such as
/// `crate::echo::EchoCommand`
pub fn try_parse_known<T: clap::Parser>(
    args: impl IntoIterator<Item = String>,
) -> Result<(T, Option<impl Iterator<Item = String>>), clap::Error> {
    let mut args = args.into_iter();
    let mut hyphen = None;
    let args_before_hyphen = args.by_ref().take_while(|a| {
        let is_hyphen = a == "--";
        if is_hyphen {
            hyphen = Some(a.clone());
        }
        !is_hyphen
    });
    let parsed_args = T::try_parse_from(args_before_hyphen)?;

    let raw_args = hyphen.map(|hyphen| std::iter::once(hyphen).chain(args));
    Ok((parsed_args, raw_args))
}

/// A simple command that can be registered as a built-in.
pub trait SimpleCommand {
    /// Returns the content of the built-in command.
    fn get_content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error>;

    /// Executes the built-in command.
    fn execute<SE: extensions::ShellExtensions, I: Iterator<Item = S>, S: AsRef<str>>(
        context: commands::ExecutionContext<'_, SE>,
        args: I,
    ) -> Result<results::ExecutionResult, error::Error>;
}

/// Returns a built-in command registration, given an implementation of the
/// `SimpleCommand` trait.
///
/// The returned [`Registration`] is in its stored form (`HasLocalState`)
/// because `SimpleCommand` has no per-builtin state concept.
pub fn simple_builtin<B: SimpleCommand + Send + Sync, SE: extensions::ShellExtensions>()
-> Registration<SE> {
    Registration {
        execute_func: exec_simple_builtin::<B, SE>,
        content_func: B::get_content,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
        state_init: default_state_fn::<()>,
        local_override: None,
        _shared: PhantomData,
        _local: PhantomData,
    }
}

/// Returns a built-in command registration, given an implementation of the
/// `Command` trait.
///
/// The phantom types encode:
/// * `S = B::SharedState` — gates which registration method can be used.
/// * `L = NeedsLocalState<B::State>` — enables [`with_state`](Registration::with_state).
pub fn builtin<B: Command + Send + Sync, SE: extensions::ShellExtensions>()
-> Registration<SE, B::SharedState, NeedsLocalState<B::State>> {
    Registration {
        execute_func: exec_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
        state_init: default_state_fn::<B::State>,
        local_override: None,
        _shared: PhantomData,
        _local: PhantomData,
    }
}

/// Like [`builtin`], but returns the stored form directly (`HasLocalState`).
///
/// Use this in `default_builtins`-style factory functions where
/// Returns a built-in command registration, given an implementation of the
/// `DeclarationCommand` trait. Used for select commands that can take parsed
/// declarations as arguments.
pub fn decl_builtin<B: DeclarationCommand + Send + Sync, SE: extensions::ShellExtensions>()
-> Registration<SE, B::SharedState, NeedsLocalState<B::State>> {
    Registration {
        execute_func: exec_declaration_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: true,
        state_init: default_state_fn::<B::State>,
        local_override: None,
        _shared: PhantomData,
        _local: PhantomData,
    }
}

#[allow(clippy::too_long_first_doc_paragraph)]
/// Returns a built-in command registration, given an implementation of the
/// `DeclarationCommand` trait that can be default-constructed. The command
/// implementation is expected to implement clap's `Parser` trait solely
/// for help/usage information. Arguments are passed directly to the command
/// via `set_declarations`. This is primarily only expected to be used with
/// select builtin commands that wrap other builtins (e.g., "builtin").
pub fn raw_arg_builtin<
    B: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>() -> Registration<SE, B::SharedState, NeedsLocalState<B::State>> {
    Registration {
        execute_func: exec_raw_arg_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: true,
        state_init: default_state_fn::<B::State>,
        local_override: None,
        _shared: PhantomData,
        _local: PhantomData,
    }
}

fn default_state_fn<S: Clone + Default + Send + Sync + 'static>() -> Box<dyn AnyState> {
    Box::new(S::default())
}

fn get_builtin_content<T: Command + Send + Sync>(
    name: &str,
    content_type: ContentType,
    options: &ContentOptions,
) -> Result<String, error::Error> {
    T::get_content(name, content_type, options)
}

fn exec_simple_builtin<T: SimpleCommand + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_simple_builtin_impl::<T, SE>(context, args).await })
}

#[expect(clippy::unused_async)]
async fn exec_simple_builtin_impl<
    T: SimpleCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let plain_args = args.into_iter().map(|arg| match arg {
        CommandArg::String(s) => s,
        CommandArg::Assignment(a) => a.to_string(),
    });

    T::execute(context, plain_args)
}

fn exec_builtin<T: Command + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_builtin_impl<T: Command + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let plain_args = args.into_iter().map(|arg| match arg {
        CommandArg::String(s) => s,
        CommandArg::Assignment(a) => a.to_string(),
    });

    let result = T::new(plain_args);
    let command = match result {
        Ok(command) => command,
        Err(e) => {
            let _ = writeln!(context.stderr(), "{e}");
            return Ok(results::ExecutionExitCode::InvalidUsage.into());
        }
    };

    call_builtin(command, context).await
}

fn exec_declaration_builtin<
    T: DeclarationCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_declaration_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_declaration_builtin_impl<
    T: DeclarationCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let mut options = vec![];
    let mut declarations = vec![];

    for (i, arg) in args.into_iter().enumerate() {
        match arg {
            CommandArg::String(s)
                if i == 0 || (s.len() > 1 && (s.starts_with('-') || s.starts_with('+'))) =>
            {
                options.push(s);
            }
            _ => declarations.push(arg),
        }
    }

    let result = T::new(options);
    let mut command = match result {
        Ok(command) => command,
        Err(e) => {
            let _ = writeln!(context.stderr(), "{e}");
            return Ok(results::ExecutionExitCode::InvalidUsage.into());
        }
    };

    command.set_declarations(declarations);

    call_builtin(command, context).await
}

fn exec_raw_arg_builtin<
    T: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_raw_arg_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_raw_arg_builtin_impl<
    T: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let mut command = T::default();
    command.set_declarations(args);

    call_builtin(command, context).await
}

async fn call_builtin(
    command: impl Command,
    context: commands::ExecutionContext<'_, impl extensions::ShellExtensions>,
) -> Result<results::ExecutionResult, error::Error> {
    let builtin_name = context.command_name.clone();
    let result = command
        .execute(context)
        .await
        .map_err(|e| error::ErrorKind::BuiltinError(Box::new(e), builtin_name))?;

    Ok(result)
}

/// Consuming builder that registers multiple builtins sharing a single
/// typed state value.
///
/// # Subshell cloning behaviour
///
/// When [`Shell::clone()`](crate::Shell::clone) is called, shared state is
/// cloned via [`AnyState::clone_box`]. With `Arc<T>` the clone is a cheap
/// refcount bump (all subshells see the same data). With a bare `T` each
/// subshell gets an independent deep copy.
///
/// # Interior mutability
///
/// The accessor [`ExecutionContext::shared`] returns `&T`. To mutate through
/// an `Arc<T>`, `T` itself must provide interior mutability (e.g. `Mutex`,
/// `papaya::HashMap`, atomics).
///
/// # Type uniqueness
///
/// Shared state is keyed by [`TypeId`]. Two unrelated uses of the same
/// generic type (e.g. `HashMap<String, String>`) would collide. Use newtype
/// wrappers for isolation.
///
/// # Example
///
/// ```ignore
/// let cache = SharedBuilder::new(Arc::new(RepoCache::default()))
///     .builtin("inherit", builtin::<InheritCommand, _>());
/// shell.register_shared(cache);
/// ```
pub struct SharedBuilder<T, SE: extensions::ShellExtensions = extensions::DefaultShellExtensions> {
    /// The shared state value to be seeded into `Shell::shared_states`.
    pub(crate) value: T,
    /// Builtins to register alongside the shared state.
    pub(crate) builtins: Vec<(String, Registration<SE, T>)>,
}

impl<T: Clone + Send + Sync + 'static, SE: extensions::ShellExtensions> SharedBuilder<T, SE> {
    /// Create a new builder that will share `value` across all added builtins.
    pub const fn new(value: T) -> Self {
        Self {
            value,
            builtins: Vec::new(),
        }
    }

    /// Add a builtin that shares state type `T`.
    ///
    /// Compile error if the registration's shared-state phantom is not `T`.
    #[must_use]
    pub fn builtin(mut self, name: impl Into<String>, reg: Registration<SE, T>) -> Self {
        self.builtins.push((name.into(), reg));
        self
    }
}

/// Borrowing handle that registers builtins against an **existing** shared
/// state entry on a [`Shell`](crate::Shell).
///
/// Obtained via [`Shell::shared_handle`](crate::Shell::shared_handle).
/// Each call to [`builtin`](SharedHandle::builtin) registers immediately.
pub struct SharedHandle<'a, T, SE: extensions::ShellExtensions> {
    pub(crate) shell: &'a mut crate::Shell<SE>,
    pub(crate) _phantom: PhantomData<T>,
}

impl<T: Clone + Send + Sync + 'static, SE: extensions::ShellExtensions> SharedHandle<'_, T, SE> {
    /// Register a builtin against the existing shared state of type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the shared state has not been seeded (i.e. if
    /// [`register_shared`](crate::Shell::register_shared) or
    /// [`set_shared`](crate::Shell::set_shared) has not been called for `T`).
    pub fn builtin(&mut self, name: impl Into<String>, reg: Registration<SE, T>) {
        assert!(
            self.shell.shared_states().contains_key(&TypeId::of::<T>()),
            "SharedHandle::builtin called but shared state for {} has not been seeded",
            type_name::<T>(),
        );
        let key = name.into();
        let (stored, local_override, state_init) = reg.into_parts();
        self.shell.builtins.insert(key.clone(), stored);
        match local_override {
            Some(state) => {
                self.shell.builtin_states.insert(key, state);
            }
            None => {
                self.shell
                    .builtin_states
                    .entry(key)
                    .or_insert_with(state_init);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Counter {
        value: usize,
    }

    #[test]
    fn any_state_clone_roundtrips() {
        let original: Box<dyn AnyState> = Box::new(Counter { value: 42 });
        let cloned = original.clone();
        let downcasted = (*cloned).as_any().downcast_ref::<Counter>().unwrap();
        assert_eq!(downcasted.value, 42);
    }

    #[test]
    fn any_state_mut_roundtrip() {
        let mut state: Box<dyn AnyState> = Box::new(Counter { value: 0 });
        (*state)
            .as_any_mut()
            .downcast_mut::<Counter>()
            .unwrap()
            .value += 1;
        assert_eq!(
            (*state).as_any().downcast_ref::<Counter>().unwrap().value,
            1
        );
    }

    #[test]
    fn any_state_wrong_type_returns_none() {
        let state: Box<dyn AnyState> = Box::new(Counter { value: 1 });
        assert!((*state).as_any().downcast_ref::<String>().is_none());
    }

    #[test]
    fn any_state_as_any_mut_downcast() {
        let mut state: Box<dyn AnyState> = Box::new(Counter { value: 5 });
        let c = (*state).as_any_mut().downcast_mut::<Counter>();
        assert!(c.is_some(), "downcast_mut to Counter should succeed");
        assert_eq!(c.unwrap().value, 5);
    }

    #[test]
    fn any_state_complex_type() {
        let mut state: Box<dyn AnyState> = Box::new(Counter { value: 5 });
        (*state)
            .as_any_mut()
            .downcast_mut::<Counter>()
            .unwrap()
            .value += 1;
        assert_eq!(
            (*state).as_any().downcast_ref::<Counter>().unwrap().value,
            6
        );
    }
}
