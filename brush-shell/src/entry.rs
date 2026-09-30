//! Implements the command-line interface for the `brush` shell.

use crate::args::CommandLineArgs;
use crate::args::InputBackendType;
use crate::brushctl::ShellBuilderBrushBuiltinExt as _;
use crate::bundled;
use crate::config;
use crate::error_formatter;
use crate::events;
use crate::productinfo;
use brush_builtins::ShellBuilderExt as _;
#[cfg(feature = "experimental-builtins")]
use brush_experimental_builtins::ShellBuilderExt as _;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::sync::LazyLock;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[allow(unused_imports, reason = "only used in some configs")]
use std::io::IsTerminal;

static TRACE_EVENT_CONFIG: LazyLock<Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(None)));

type BrushShellExtensions = brush_core::extensions::ShellExtensionsImpl<error_formatter::Formatter>;
type BrushShell = brush_core::Shell<BrushShellExtensions>;

/// A command-line failure, already rendered. Help and version requests use
/// exit code 0; every other failure uses 2.
#[derive(Debug)]
pub(crate) struct ParseError {
    pub(crate) message: String,
    pub(crate) exit_code: u8,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses `args`, splitting off `--` and everything after it first.
///
/// Returns the parsed value plus the `--` separator and the words that followed
/// it, or `None` if no separator was present. The first word is the program name
/// and is not an argument.
fn try_parse_known(
    args: impl IntoIterator<Item = String>,
) -> Result<(CommandLineArgs, Option<Vec<String>>), ParseError> {
    let args: Vec<String> = args.into_iter().collect();
    let words = args.split_first().map_or(&[][..], |(_, rest)| rest);

    let mut before = Vec::new();
    let mut hyphen = None;
    let mut after = Vec::new();
    for word in words {
        if hyphen.is_none() && word == "--" {
            hyphen = Some(word.clone());
            continue;
        }
        if hyphen.is_some() {
            after.push(word.clone());
        } else {
            before.push(word.clone());
        }
    }

    let parsed = parse_words(&before)?;
    let raw = hyphen.map(|marker| {
        let mut tail = vec![marker];
        tail.extend(after);
        tail
    });
    Ok((parsed, raw))
}

fn parse_words(words: &[String]) -> Result<CommandLineArgs, ParseError> {
    let owned: Vec<OsString> = words
        .iter()
        .map(|word| OsString::from(word.as_str()))
        .collect();
    let refs: Vec<&OsStr> = owned.iter().map(OsString::as_os_str).collect();
    match CommandLineArgs::parse_from(&refs) {
        Ok(parsed) => Ok(parsed),
        Err(err) => Err(render_parse_error(&refs, &err)),
    }
}

/// Renames the shell's leading `+o` / `+O` options to the `--+o` / `--+O` long
/// options that usage accepts.
///
/// Only options are renamed: the first operand (a script path, or `-c`'s
/// command string) or a `--` ends the scan, so a script's own `+o` arguments
/// reach it intact. `args` includes the program name.
fn rewrite_plus_options(args: &mut [String]) {
    let command = CommandLineArgs::command();
    let mut expecting_value = false;

    for arg in args.iter_mut().skip(1) {
        if expecting_value {
            expecting_value = false;
            continue;
        }
        if arg == "--" || arg == "-" || !arg.starts_with(['-', '+']) {
            break;
        }

        if arg.starts_with("+o") || arg.starts_with("+O") {
            expecting_value = arg.len() == 2;
            arg.insert_str(0, "--");
        } else if let Some(long) = arg.strip_prefix("--") {
            let name = long.split('=').next().unwrap_or(long);
            expecting_value = !long.contains('=')
                && command
                    .flags
                    .iter()
                    .any(|flag| flag.takes_value && flag.longs.contains(&name));
        } else if let Some(shorts) = arg.strip_prefix('-') {
            // In a bundle, the first short that takes a value consumes the rest of
            // the word, or the next word when it's last.
            expecting_value = false;
            for (i, short) in shorts.bytes().enumerate() {
                let takes_value = command
                    .flags
                    .iter()
                    .any(|flag| flag.takes_value && flag.shorts.contains(&short));
                if takes_value {
                    expecting_value = i + 1 == shorts.len();
                    break;
                }
            }
        }
    }
}

fn render_parse_error(argv: &[&OsStr], err: &usage::Error<'_, '_>) -> ParseError {
    let spec = CommandLineArgs::spec()
        .view()
        .name("brush")
        .bin("brush")
        .version(crate::args::VERSION)
        .spec();
    match err {
        usage::Error::Help { cmd, long } => ParseError {
            message: usage::help::render_styled(&spec, cmd, *long, usage::help::Style::PLAIN)
                .unwrap_or_default(),
            exit_code: 0,
        },
        usage::Error::HelpAll { cmd } => ParseError {
            message: usage::help::render_styled(&spec, cmd, true, usage::help::Style::PLAIN)
                .unwrap_or_default(),
            exit_code: 0,
        },
        usage::Error::Version { .. } => ParseError {
            message: format!("{} {}", productinfo::PRODUCT_NAME, crate::args::VERSION),
            exit_code: 0,
        },
        _ => ParseError {
            message: usage::render_failure_plain(&spec, argv, err),
            exit_code: 2,
        },
    }
}

impl CommandLineArgs {
    /// Parses the shell command line, including brush's `--` handling.
    pub(crate) fn parse_shell_args(
        itr: impl IntoIterator<Item = String>,
    ) -> Result<Self, ParseError> {
        let (mut this, script_args) = try_parse_known(itr)?;

        // Collect any args from after `--` (handled by try_parse_known) into
        // script_args, which become positional parameters ($0, $1, ...).
        if let Some(args) = script_args {
            let mut args = args.into_iter().peekable();

            // Bash stops option parsing at the first non-option word: a script path
            // or the `-c` command string. A `--` seen before either is the option
            // terminator and is dropped (`bash -s -- a`, `bash -- script.sh`); one
            // seen after is an ordinary positional (`bash script.sh -- a`).
            if this.script_args.is_empty() {
                args.next_if(|a| a == "--");
            }
            this.script_args.extend(args);
        }

        // With `-c`, bash takes the command string from the first operand; the
        // operands after it keep their usual meaning, so the next one sets `$0`
        // and the rest become positional parameters.
        if this.command_string_mode {
            if this.script_args.is_empty() {
                return Err(ParseError {
                    message: "brush: -c: option requires an argument".to_owned(),
                    exit_code: 2,
                });
            }
            this.command = Some(this.script_args.remove(0));
        }

        Ok(this)
    }
}

/// Main entry point for the `brush` shell.
pub fn run() {
    //
    // Install the bundled-command registry so it's available both for
    // bundled dispatch (handled next) and for builtin shim registration
    // during shell construction. With no bundled-providing features enabled
    // the registry is empty and both code paths become no-ops.
    //
    bundled::install_default_providers();

    //
    // If we were invoked as `brush <DISPATCH_FLAG> <name> [args...]`, run the
    // bundled command and exit before doing any shell setup. This is the
    // hot path for in-binary coreutils invocations.
    //
    if let Some(code) = bundled::maybe_dispatch() {
        std::process::exit(code);
    }

    //
    // Install panic handlers to clean up on panic.
    //
    install_panic_handlers();

    //
    // Answer a request from a generated completion script (`brush __complete_word__ ...`)
    // before parsing: it isn't part of the shell's command line.
    //
    let completion_args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(answer) = CommandLineArgs::completion_request(&completion_args) {
        let _ = write!(std::io::stdout(), "{answer}");
        std::process::exit(0);
    }

    //
    // Parse args.
    //
    let mut args: Vec<_> = std::env::args().collect();

    rewrite_plus_options(&mut args);

    let parsed_args = match CommandLineArgs::parse_shell_args(args.iter().cloned()) {
        Ok(parsed_args) => parsed_args,
        Err(error) => {
            // Help and version requests succeed, so they go to stdout like any
            // other requested output; real failures go to stderr.
            let _ = if error.exit_code == 0 {
                writeln!(std::io::stdout(), "{}", error.message)
            } else {
                writeln!(std::io::stderr(), "{}", error.message)
            };
            std::process::exit(i32::from(error.exit_code));
        }
    };

    //
    // Run.
    //
    #[cfg(any(unix, windows))]
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    #[cfg(not(any(unix, windows)))]
    let mut builder = tokio::runtime::Builder::new_current_thread();

    let Ok(runtime) = builder.enable_all().build() else {
        tracing::error!("error: failed to create Tokio runtime");
        std::process::exit(1);
    };

    let result = runtime.block_on(run_async(&args, parsed_args));

    let exit_code = match result {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    std::process::exit(i32::from(exit_code));
}

/// Installs panic handlers to report our panic and cleanly exit on panic.
fn install_panic_handlers() {
    //
    // Set up panic handler. On release builds, it will capture panic details to a
    // temporary .toml file and report a human-readable message to the screen.
    //
    human_panic::setup_panic!(
        human_panic::Metadata::new(productinfo::PRODUCT_NAME, productinfo::PRODUCT_VERSION)
            .homepage(env!("CARGO_PKG_HOMEPAGE"))
            .support("please post a GitHub issue at https://github.com/reubeno/brush/issues/new")
    );

    //
    // If stdout is connected to a terminal, then register a new panic handler that
    // resets the terminal and then invokes the previously registered handler. In
    // dev/debug builds, the previously registered handler will be the default
    // handler; in release builds, it will be the one registered by `human_panic`.
    //
    if std::io::stdout().is_terminal() {
        let original_panic_handler = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Best-effort attempt to reset the terminal to defaults.
            let _ = try_reset_terminal_to_defaults();

            // Invoke the original handler
            original_panic_handler(panic_info);
        }));
    }
}

#[cfg(feature = "experimental")]
pub(crate) const DEFAULT_ENABLE_HIGHLIGHTING: bool = true;
#[cfg(not(feature = "experimental"))]
pub(crate) const DEFAULT_ENABLE_HIGHLIGHTING: bool = false;

/// Run the brush shell. Returns the exit code.
///
/// # Arguments
///
/// * `cli_args` - The command-line arguments to the shell, in string form.
/// * `args` - The already-parsed command-line arguments.
#[doc(hidden)]
async fn run_async(
    cli_args: &[String],
    args: CommandLineArgs,
) -> Result<u8, brush_interactive::ShellError> {
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.lock().await;
    *event_config = Some(events::TraceEventConfig::init(
        &args.enabled_debug_events,
        &args.disabled_events,
    ));
    drop(event_config);

    // Load configuration file.
    let file_config = config::load_config(args.no_config, args.config_file.as_deref())
        .into_config_or_log()
        .map_err(|e| brush_interactive::ShellError::IoError(std::io::Error::other(e)))?;

    // Instantiate an appropriately configured shell and wrap it in an `Arc`. Note that we do
    // *not* run any code in the shell yet. We'll delay loading profiles and such until after
    // we've set up everything else (in `run_in_shell`).
    let shell: BrushShell = instantiate_shell(&args, cli_args).await?;
    let shell = Arc::new(Mutex::new(shell));

    // Run with the selected input backend. Each branch instantiates the concrete
    // backend type and calls `run_in_shell`, preserving static dispatch.
    let default_backend = get_default_input_backend_type(&args);
    let selected_backend = args.input_backend.unwrap_or(default_backend);

    // Build UI options by merging config file with CLI args.
    #[allow(unused_variables, reason = "not used when no backend features enabled")]
    let ui_options = file_config.to_ui_options(&args);

    let result = match selected_backend {
        #[cfg(all(feature = "reedline", any(unix, windows)))]
        InputBackendType::Reedline => {
            let mut input_backend =
                brush_interactive::ReedlineInputBackend::new(&ui_options, &shell)?;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(any(not(feature = "reedline"), not(any(unix, windows))))]
        InputBackendType::Reedline => Err(brush_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "basic")]
        InputBackendType::Basic => {
            let mut input_backend = brush_interactive::BasicInputBackend;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(not(feature = "basic"))]
        InputBackendType::Basic => Err(brush_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "minimal")]
        InputBackendType::Minimal => {
            let mut input_backend = brush_interactive::MinimalInputBackend;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(not(feature = "minimal"))]
        InputBackendType::Minimal => Err(brush_interactive::ShellError::InputBackendNotSupported),
    };

    // Display any error that percolated up.
    let exit_code = match result {
        Ok(code) => code,
        Err(brush_interactive::ShellError::ShellError(e)) => {
            let shell = shell.lock().await;
            let mut stderr = shell.stderr();
            let _ = shell.display_error(&mut stderr, &e);
            drop(shell);
            1
        }
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    Ok(exit_code)
}

/// Runs the shell according to the provided command-line arguments.
/// Also responsible for loading profiles and rc files as appropriate.
///
/// # Arguments
///
/// * `shell_ref` - A reference to the shell to run.
/// * `args` - The parsed command-line arguments.
/// * `input_backend` - The input backend to use.
/// * `ui_options` - The user interface options to use.
async fn run_in_shell(
    shell_ref: &brush_interactive::ShellRef<impl brush_core::ShellExtensions>,
    args: CommandLineArgs,
    input_backend: &mut impl brush_interactive::InputBackend,
    ui_options: &brush_interactive::UIOptions,
) -> Result<u8, brush_interactive::ShellError> {
    let read_commands_from_stdin = args.will_read_commands_from_stdin();
    let interactive_options: brush_interactive::InteractiveOptions = ui_options.into();

    // Before config files run, so they (and what they source) observe it -- and only for a
    // shell that goes on to read commands interactively below, since only such a shell
    // dispatches hooks.
    if read_commands_from_stdin {
        brush_interactive::init_zsh_style_hooks(
            &mut *shell_ref.lock().await,
            &interactive_options,
        )?;
    }

    // Load profile and rc files as appropriate.
    initialize_shell(shell_ref, &args).await?;

    // If a command was specified via -c, then run that command and then exit.
    if let Some(command) = args.command {
        shell_ref.lock().await.run_dash_c_command(command).await?;

    // Otherwise read commands interactively: -s was given (positional args become parameters;
    // a script named among them is *not* run), or nothing to run was specified.
    } else if read_commands_from_stdin {
        brush_interactive::InteractiveShell::new(shell_ref, input_backend, &interactive_options)?
            .run_interactively()
            .await?;

    // Otherwise a script path was given; run it.
    } else {
        shell_ref
            .lock()
            .await
            .run_script(
                Path::new(&args.script_args[0]),
                args.script_args.iter().skip(1),
            )
            .await?;
    }

    // Make sure to return the last result observed in the shell.
    let result = shell_ref.lock().await.last_exit_status();

    Ok(result)
}

/// Initializes a shell by loading profile and rc files as appropriate.
///
/// # Arguments
///
/// * `shell_ref` - A reference to the shell to initialize.
/// * `args` - The parsed command-line arguments.
async fn initialize_shell(
    shell_ref: &brush_interactive::ShellRef<impl brush_core::ShellExtensions>,
    args: &CommandLineArgs,
) -> Result<(), brush_interactive::ShellError> {
    // Compute desired profile-loading behavior.
    let profile = if args.no_profile {
        brush_core::ProfileLoadBehavior::Skip
    } else {
        brush_core::ProfileLoadBehavior::LoadDefault
    };

    // Compute desired rc-loading behavior.
    let rc = if args.no_rc {
        brush_core::RcLoadBehavior::Skip
    } else if let Some(rc_file) = &args.rc_file {
        brush_core::RcLoadBehavior::LoadCustom(rc_file.clone())
    } else {
        brush_core::RcLoadBehavior::LoadDefault
    };

    shell_ref.lock().await.load_config(&profile, &rc).await?;

    Ok(())
}

/// Instantiates a shell from command-line arguments. Does *not* run any code in the shell.
///
/// # Arguments
///
/// * `args` - The parsed command-line arguments.
/// * `cli_args` - The raw command-line arguments.
async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: &[String],
) -> Result<BrushShell, brush_interactive::ShellError> {
    #[cfg(feature = "experimental-load")]
    let mut shell = if let Some(load_file) = &args.load_file {
        instantiate_shell_from_file(load_file.as_path())?
    } else {
        instantiate_shell_from_args(args, cli_args).await?
    };

    #[cfg(not(feature = "experimental-load"))]
    let mut shell = instantiate_shell_from_args(args, cli_args).await?;

    // Register shims for any bundled commands in the installed registry.
    // Done here (not inside the inner instantiators) so both paths are
    // covered from a single site.
    bundled::register_shims(&mut shell);

    Ok(shell)
}

#[cfg(feature = "experimental-load")]
fn instantiate_shell_from_file(
    file_path: &Path,
) -> Result<BrushShell, brush_interactive::ShellError> {
    let mut shell: BrushShell = serde_json::from_reader(std::fs::File::open(file_path)?)
        .map_err(|e| brush_interactive::ShellError::IoError(std::io::Error::other(e)))?;

    // NOTE: We need to manually register builtins because we can't serialize/deserialize them.
    // TODO(serde): we should consider whether we could/should at least track *which* are enabled.
    let builtin_set = if shell.options().sh_mode {
        brush_builtins::BuiltinSet::ShMode
    } else {
        brush_builtins::BuiltinSet::BashMode
    };

    let builtins = brush_builtins::default_builtins(builtin_set);

    for (builtin_name, builtin) in builtins {
        shell.register_builtin(&builtin_name, builtin);
    }

    // Add experimental builtins (if enabled).
    #[cfg(feature = "experimental-builtins")]
    for (builtin_name, builtin) in brush_experimental_builtins::experimental_builtins() {
        shell.register_builtin(&builtin_name, builtin);
    }

    Ok(shell)
}

/// Instantiates a shell from command-line arguments. Does *not* run any code in the shell.
///
/// # Arguments
///
/// * `args` - The parsed command-line arguments.
/// * `cli_args` - The raw command-line arguments.
async fn instantiate_shell_from_args(
    args: &CommandLineArgs,
    cli_args: &[String],
) -> Result<BrushShell, brush_interactive::ShellError> {
    // Compute login flag.
    let login = args.login || cli_args.first().is_some_and(|argv0| argv0.starts_with('-'));

    // Compute shell name.
    let shell_name = if args.command.is_some() && !args.script_args.is_empty() {
        Some(args.script_args[0].clone())
    } else if !cli_args.is_empty() {
        Some(cli_args[0].clone())
    } else if args.sh_mode {
        // Simulate having been run as "sh".
        Some(String::from("sh"))
    } else {
        None
    };

    // Compute positional shell arguments.
    let shell_args = if args.command.is_some() {
        Some(args.script_args.iter().skip(1).cloned().collect())
    } else if args.read_commands_from_stdin {
        Some(args.script_args.clone())
    } else {
        None
    };

    // Commands are read from stdin if -s was provided, or if no command was specified (either via
    // -c or as a positional argument).
    let read_commands_from_stdin = args.will_read_commands_from_stdin();

    let builtin_set = if args.sh_mode {
        brush_builtins::BuiltinSet::ShMode
    } else {
        brush_builtins::BuiltinSet::BashMode
    };

    // Identify the file descriptors to inherit.
    let fds = args
        .inherited_fds
        .iter()
        .filter_map(|&fd| brush_core::sys::fd::try_get_file_for_open_fd(fd).map(|file| (fd, file)))
        .collect();

    // Select parser implementation to use.
    #[cfg(feature = "experimental-parser")]
    let parser_impl = if args.experimental_parser {
        brush_core::parser::ParserImpl::Winnow
    } else {
        brush_core::parser::ParserImpl::Peg
    };

    #[cfg(not(feature = "experimental-parser"))]
    let parser_impl = brush_core::parser::ParserImpl::Peg;

    // Set up the shell builder with the requested options.
    // NOTE: We skip loading profile and rc files here; that will be handled later after we've
    // fully instantiated everything we want set before running any code.
    let shell = brush_core::Shell::builder_with_extensions::<BrushShellExtensions>()
        .disable_options(args.disabled_options.clone())
        .disable_shopt_options(args.disabled_shopt_options.clone())
        .disallow_overwriting_regular_files_via_output_redirection(
            args.disallow_overwriting_regular_files_via_output_redirection,
        )
        .enable_options(args.enabled_options.clone())
        .enable_shopt_options(args.enabled_shopt_options.clone())
        .do_not_execute_commands(args.do_not_execute_commands)
        .exit_after_one_command(args.exit_after_one_command)
        .login(login)
        .interactive(args.is_interactive())
        .command_string_mode(args.command.is_some())
        .no_editing(args.no_editing)
        .profile(brush_core::ProfileLoadBehavior::Skip)
        .rc(brush_core::RcLoadBehavior::Skip)
        .do_not_inherit_env(args.do_not_inherit_env)
        .fds(fds)
        .maybe_shell_args(shell_args)
        .posix(args.posix || args.sh_mode)
        .print_commands_and_arguments(args.print_commands_and_arguments)
        .read_commands_from_stdin(read_commands_from_stdin)
        .maybe_shell_name(shell_name)
        .shell_product_display_str(productinfo::get_product_display_str())
        .sh_mode(args.sh_mode)
        .treat_unset_variables_as_error(args.treat_unset_variables_as_error)
        .exit_on_nonzero_command_exit(args.exit_on_nonzero_command_exit)
        .disable_pathname_expansion(args.disable_pathname_expansion)
        .verbose(args.verbose)
        .parser(parser_impl)
        .error_formatter(new_error_behavior(args))
        .shell_version(env!("CARGO_PKG_VERSION").to_string());

    // Add builtins.
    let shell = shell.default_builtins(builtin_set).brush_builtins();

    // Add experimental builtins (if enabled).
    #[cfg(feature = "experimental-builtins")]
    let shell = shell.experimental_builtins();

    // Build the shell.
    let mut shell = shell.build().await?;

    // Make adjustments.
    if let Some(xtrace_file_path) = &args.xtrace_file_path {
        enable_xtrace_to_file(&mut shell, xtrace_file_path)?;
    }

    Ok(shell)
}

fn enable_xtrace_to_file(
    shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    file_path: &Path,
) -> Result<(), brush_interactive::ShellError> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(file_path)
        .map_err(|e| {
            brush_interactive::ShellError::FailedToCreateXtraceFile(file_path.to_path_buf(), e)
        })?;

    let file = brush_core::openfiles::OpenFile::from(file);
    let file_fd = shell.open_files_mut().add(file)?;

    shell.options_mut().print_commands_and_arguments = true;
    shell.set_env_global(
        "BASH_XTRACEFD",
        brush_core::ShellVariable::new(file_fd.to_string()),
    )?;

    Ok(())
}

const fn new_error_behavior(args: &CommandLineArgs) -> error_formatter::Formatter {
    error_formatter::Formatter {
        use_color: !args.disable_color,
    }
}

fn get_default_input_backend_type(args: &CommandLineArgs) -> InputBackendType {
    #[cfg(any(unix, windows))]
    {
        // If stdin isn't a terminal, then `reedline` doesn't do the right thing
        // (reference: https://github.com/nushell/reedline/issues/509). Switch to
        // the minimal input backend instead for that scenario.
        if std::io::stdin().is_terminal() && args.will_read_commands_from_stdin() {
            InputBackendType::Reedline
        } else {
            InputBackendType::Minimal
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _args = args;
        InputBackendType::Minimal
    }
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
}

fn try_reset_terminal_to_defaults() -> Result<(), std::io::Error> {
    #[cfg(any(unix, windows))]
    {
        // Reset the console.
        let exec_result = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::terminal::EnableLineWrap,
            crossterm::style::ResetColor,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show,
            crossterm::cursor::MoveToNextLine(1),
        );

        let raw_result = crossterm::terminal::disable_raw_mode();

        exec_result?;
        raw_result?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plus_options_are_renamed_only_before_operands() {
        let mut words = args(&[
            "brush", "+o", "errexit", "-o", "+ox", "+O", "extglob", "script", "+o", "+Oz",
        ]);
        rewrite_plus_options(&mut words);
        assert_eq!(
            words,
            args(&[
                "brush", "--+o", "errexit", "-o", "+ox", "--+O", "extglob", "script", "+o", "+Oz",
            ])
        );

        let mut words = args(&["brush", "-xc", "echo", "arg0", "+o", "--", "+o"]);
        rewrite_plus_options(&mut words);
        assert_eq!(
            words,
            args(&["brush", "-xc", "echo", "arg0", "+o", "--", "+o"])
        );
    }

    #[test]
    fn parse_empty_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush"]))?;
        assert_matches!(parsed_args.script_args.as_slice(), []);
        Ok(())
    }

    #[test]
    fn parse_script_and_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "some-script",
            "-x",
            "1",
            "--option",
        ]))?;
        assert_eq!(
            parsed_args.script_args,
            ["some-script", "-x", "1", "--option"]
        );
        Ok(())
    }

    #[test]
    fn parse_script_and_args_with_double_dash_in_script_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "some-script", "--"]))?;
        assert_eq!(parsed_args.script_args, ["some-script", "--"]);
        Ok(())
    }

    #[test]
    fn parse_unknown_args() {
        let result = CommandLineArgs::parse_shell_args(args(&["brush", "--unknown-option"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_c_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_no_command() {
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--"])).is_err());
    }

    #[test]
    fn parse_c_with_double_dash_command_is_double_dash() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "--", "echo", "hi"]))?;
        assert_eq!(parsed_args.command, Some("--".to_string()));
        assert_eq!(parsed_args.script_args, ["echo", "hi"]);
        Ok(())
    }

    #[test]
    fn parse_ec_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-ec", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_value_before_double_dash_unchanged() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "echo hi", "--", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hi".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_option_before_command() -> Result<()> {
        // bash takes -c's command string from the first operand, so options may
        // sit between the two: `bash -c -l 'echo hello' arg0`.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_value_taking_option_before_command() -> Result<()> {
        // -o consumes its own value, so the first operand is still the command.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "-c",
            "-o",
            "errexit",
            "echo hello",
        ]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.enabled_options, ["errexit"]);
        assert!(parsed_args.script_args.is_empty());
        Ok(())
    }

    #[test]
    fn parse_ec_with_option_before_command() -> Result<()> {
        // Only the `c` leaves the combined group; -e still applies.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-ec", "-l", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert!(parsed_args.login);
        Ok(())
    }

    #[test]
    fn parse_c_with_several_options_before_command() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "-x", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert!(parsed_args.print_commands_and_arguments);
        assert!(parsed_args.script_args.is_empty());
        Ok(())
    }

    #[test]
    fn parse_c_after_value_taking_option() -> Result<()> {
        // "errexit" is -o's value rather than an operand, so option parsing
        // continues and the command is still the first operand.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "-o",
            "errexit",
            "-c",
            "-l",
            "echo hello",
        ]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.enabled_options, ["errexit"]);
        assert!(parsed_args.login);
        Ok(())
    }

    #[test]
    fn parse_c_after_script_stays_a_script_argument() -> Result<()> {
        // Option parsing stops at the first operand, so these reach the script:
        // `bash announce.sh -c -l` passes "-c" and "-l" as $1 and $2.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "announce.sh", "-c", "-l"]))?;
        assert!(parsed_args.command.is_none());
        assert!(!parsed_args.login);
        assert_eq!(parsed_args.script_args, ["announce.sh", "-c", "-l"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_and_option_like_command() -> Result<()> {
        // After `--`, the next token is the command even when it looks like an
        // option: `bash -c -- -l 'echo zero'` runs `-l` with $0 of "echo zero".
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "-l", "echo zero"]))?;
        assert_eq!(parsed_args.command, Some("-l".to_string()));
        assert!(!parsed_args.login);
        assert_eq!(parsed_args.script_args, ["echo zero"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_option_and_no_command() {
        // bash: "-c: option requires an argument", exit 2.
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l"])).is_err());
    }

    #[test]
    fn parse_c_with_option_then_double_dash() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "--", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert!(parsed_args.script_args.is_empty());
        Ok(())
    }

    #[test]
    fn parse_c_with_option_and_doubled_double_dash() -> Result<()> {
        // Only the first `--` terminates options; the second is the command
        // operand, matching `bash -c -l -- -- echo hi`.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush", "-c", "-l", "--", "--", "echo", "hi",
        ]))?;
        assert_eq!(parsed_args.command, Some("--".to_string()));
        assert_eq!(parsed_args.script_args, ["echo", "hi"]);
        Ok(())
    }

    #[test]
    fn parse_c_combined_with_other_flags() -> Result<()> {
        // `-c` groups with other short flags in any order, as in `brush -cl "echo hi"`.
        for argv in [
            &["brush", "-cl", "echo hi", "name"][..],
            &["brush", "-lc", "echo hi", "name"][..],
            &["brush", "-c", "-l", "echo hi", "name"][..],
        ] {
            let parsed_args = CommandLineArgs::parse_shell_args(args(argv))?;
            assert!(parsed_args.login, "for {argv:?}");
            assert_eq!(
                parsed_args.command,
                Some("echo hi".to_string()),
                "for {argv:?}"
            );
            assert_eq!(parsed_args.script_args, ["name"], "for {argv:?}");
        }
        Ok(())
    }

    #[test]
    fn parse_c_without_command_string() {
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c"])).is_err());
    }

    #[test]
    fn parse_c_with_empty_command_string() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-c", ""]))?;
        assert_eq!(parsed_args.command, Some(String::new()));
        Ok(())
    }

    #[test]
    fn parse_o_with_double_dash_is_not_transformed() {
        // Unlike -c, bash's -o consumes -- as its literal value (invalid option
        // name), not as an option terminator.
        let result = CommandLineArgs::parse_shell_args(args(&["brush", "-o", "--"]));
        // try_parse_known splits at --, so -o ends up
        // without a value and parsing correctly fails. The key assertion is
        // that we MUST NOT reinterpret -- as an option terminator for -o and
        // then take any later argument as its value.
        assert!(result.is_err());
    }

    #[test]
    fn parse_oc_is_o_with_an_attached_value() -> Result<()> {
        // To brush's parser, -oc is -o with attached value "c", not -o flag + -c
        // flag, so the -- is an ordinary option terminator here. NOTE: bash instead
        // takes the *next* word as -o's option name; see the known-failure compat
        // case "-oc with -- takes its option name from the next word".
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-oc", "--", "echo"]))?;
        // -o consumed "c" as its value; -- ended the options; no -c command.
        assert!(parsed_args.command.is_none());
        assert_eq!(parsed_args.script_args, ["echo"]);
        Ok(())
    }

    #[test]
    fn parse_bool_flag_before_double_dash_not_transformed() -> Result<()> {
        // -e is a boolean flag, not -c. The -- ends the options, so
        // everything after it is positional (including -c).
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-e", "--", "-c", "echo"]))?;
        assert!(parsed_args.command.is_none());
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["-c", "echo"]);
        Ok(())
    }

    #[test]
    fn parse_s_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-s", "--", "--", "a"]))?;
        assert!(parsed_args.read_commands_from_stdin);
        assert_eq!(parsed_args.script_args, ["--", "a"]);
        Ok(())
    }

    #[test]
    fn parse_double_dash_before_script() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "--", "script.sh", "--", "x"]))?;
        assert_eq!(parsed_args.script_args, ["script.sh", "--", "x"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_and_later_double_dash() -> Result<()> {
        // After removing the first --, -c gets "echo". The second -- is
        // handled by try_parse_known and appears in script_args.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "echo", "--", "more"]))?;
        assert_eq!(parsed_args.command, Some("echo".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "more"]);
        Ok(())
    }

    #[test]
    fn parse_c_flag_group_edge_cases() -> Result<()> {
        // `-C` is a different flag, and `-oc` is `-o` with the value "c", so
        // neither puts the shell in command mode.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-C", "script.sh"]))?;
        assert!(!parsed_args.command_string_mode);
        assert!(parsed_args.command.is_none());

        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-oc", "script.sh"]))?;
        assert!(!parsed_args.command_string_mode);
        assert_eq!(parsed_args.enabled_options, ["c"]);
        Ok(())
    }
}
