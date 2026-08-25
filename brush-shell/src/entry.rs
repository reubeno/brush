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
use std::sync::LazyLock;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[allow(unused_imports, reason = "only used in some configs")]
use std::io::IsTerminal;

static TRACE_EVENT_CONFIG: LazyLock<Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(None)));

type BrushShellExtensions = brush_core::extensions::ShellExtensionsImpl<error_formatter::Formatter>;
type BrushShell = brush_core::Shell<BrushShellExtensions>;

/// Maximum width used when rendering help and usage messages; mirrors the
/// default width used by `bpaf`.
const PARSE_FAILURE_MAX_WIDTH: usize = 100;

/// Displays output associated with a failed parse of the shell's command-line
/// arguments.
///
/// Rendering is delegated to `bpaf`, which colorizes messages using its bright
/// palette when the terminal supports it (respecting environment variables
/// such as `NO_COLOR`). If colorized output was disabled via `--disable-color`,
/// then falls back to monochrome rendering instead.
///
/// # Arguments
///
/// * `args` - The raw command-line arguments, including the program name.
/// * `failure` - The parse failure to report.
fn display_parse_failure(args: &[String], failure: bpaf::ParseFailure) {
    if !args.iter().any(|arg| arg == "--disable-color") {
        failure.print_message(PARSE_FAILURE_MAX_WIDTH);
    } else if matches!(
        &failure,
        bpaf::ParseFailure::Stdout(..) | bpaf::ParseFailure::Completion(..)
    ) {
        print!("{}", failure.unwrap_stdout());
    } else {
        eprintln!("{}", failure.unwrap_stderr());
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
    // Parse args.
    //
    let args: Vec<_> = std::env::args().collect();

    let parsed_args = match CommandLineArgs::try_parse_from(args.iter().cloned()) {
        Ok(parsed_args) => parsed_args,
        Err(failure) => {
            // Help and version requests go to stdout with a successful exit
            // code; everything else goes to stderr with an invalid usage code.
            let is_success_output = matches!(
                failure,
                bpaf::ParseFailure::Stdout(..) | bpaf::ParseFailure::Completion(..)
            );

            display_parse_failure(&args, failure);

            if is_success_output {
                std::process::exit(0);
            } else {
                std::process::exit(2);
            }
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

/// Determines whether `run_in_shell` will run the shell interactively. Must be sync'd with it.
const fn will_run_interactively(args: &CommandLineArgs) -> bool {
    if args.command.is_some() {
        false
    } else if args.read_commands_from_stdin {
        true
    } else {
        args.script_args.is_empty()
    }
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
    // First load profile and rc files as appropriate.
    initialize_shell(shell_ref, &args).await?;

    // If a command was specified via -c, then run that command and then exit.
    if let Some(command) = args.command {
        shell_ref.lock().await.run_dash_c_command(command).await?;

    // If -s was provided, then read commands from stdin. If there was a script (and optionally
    // args) passed on the command line via positional arguments, then we copy over the
    // parameters but do *not* execute it.
    } else if args.read_commands_from_stdin {
        let interactive_options = ui_options.into();
        brush_interactive::InteractiveShell::new(shell_ref, input_backend, &interactive_options)?
            .run_interactively()
            .await?;

    // If a script path was provided, then run the script.
    } else if !args.script_args.is_empty() {
        // The path to a script was provided on the command line; run the script.
        shell_ref
            .lock()
            .await
            .run_script(
                Path::new(&args.script_args[0]),
                args.script_args.iter().skip(1),
            )
            .await?;

    // If we got down here, then we don't have any commands to run. We'll be reading
    // them in from stdin one way or the other.
    } else {
        let interactive_options = ui_options.into();
        brush_interactive::InteractiveShell::new(shell_ref, input_backend, &interactive_options)?
            .run_interactively()
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
    let login =
        args.login.is_some() || cli_args.first().is_some_and(|argv0| argv0.starts_with('-'));

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
    let read_commands_from_stdin = (args.read_commands_from_stdin && args.command.is_none())
        || (args.script_args.is_empty() && args.command.is_none());

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
        .verbose(args.verbose.is_some())
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
        if std::io::stdin().is_terminal() && will_run_interactively(args) {
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
