//! Tests of the contract between the shell and its builtins, through a
//! builtin that parses its arguments by hand.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::io::{Read, Seek, Write};

use anyhow::{Context as _, Result};
use brush_core::builtins::{
    Builtin, ContentOptions, ContentType, DeclarationRole, Invocation, ParseArgs, Registration,
    UsageError,
};
use brush_core::openfiles::OpenFiles;
use brush_core::{CommandArg, ExecutionContext, ExecutionResult, ShellExtensions};

/// Describes arguments by kind and text, e.g. `[x=1 2 (assignment), y]`.
fn describe(args: &[CommandArg]) -> String {
    let args: Vec<_> = args
        .iter()
        .map(|arg| match arg {
            CommandArg::String(s) => s.clone(),
            CommandArg::Assignment(a) => format!("{a} (assignment)"),
        })
        .collect();
    format!("[{}]", args.join(", "))
}

/// Writes how it was invoked: the name its parser saw, the context's command
/// name, and its arguments. A declaration utility when `DECLARATION`.
struct Show<const DECLARATION: bool> {
    name: String,
    args: Vec<CommandArg>,
}

impl<const DECLARATION: bool> ParseArgs for Show<DECLARATION> {
    const DECLARATION_ROLE: DeclarationRole = if DECLARATION {
        DeclarationRole::Utility
    } else {
        DeclarationRole::None
    };

    fn content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok(format!(
            "{name}: {content_type:?} colorized={}\n",
            options.colorized
        ))
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self {
            name: invocation.name().to_owned(),
            args: invocation.into_args(),
        })
    }
}

impl<SE: ShellExtensions, const DECLARATION: bool> Builtin<SE> for Show<DECLARATION> {
    type Error = brush_core::Error;

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the trait's method is async; this one has nothing to wait for"
    )]
    async fn execute(
        self,
        context: ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        writeln!(
            context.stdout(),
            "{} ({}) {}",
            self.name,
            context.command_name,
            describe(&self.args)
        )?;
        Ok(ExecutionResult::success())
    }
}

/// The error `Picky` fails with.
#[derive(Debug, thiserror::Error)]
#[error("refused {0}")]
struct PickyError(String);

impl brush_core::BuiltinError for PickyError {}

impl From<&PickyError> for brush_core::ExecutionExitCode {
    fn from(_: &PickyError) -> Self {
        Self::Custom(42)
    }
}

/// Rejects the argument `bad` while parsing, fails on `fail`, returns status 7
/// for `seven`, and otherwise writes `ran`.
struct Picky {
    arg: Option<String>,
}

impl ParseArgs for Picky {
    fn content(
        name: &str,
        _content_type: ContentType,
        _options: &ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok(format!("{name} - picky\n"))
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let name = invocation.name();
        let mut args = invocation.args().iter().map(ToString::to_string);
        let arg = args.next();
        if arg.as_deref() == Some("bad") {
            return Err(UsageError::new(format!("{name}: bad: invalid argument")));
        }
        if args.next().is_some() {
            return Err(UsageError::new(format!("{name}: too many arguments")));
        }
        Ok(Self { arg })
    }
}

impl<SE: ShellExtensions> Builtin<SE> for Picky {
    type Error = PickyError;

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the trait's method is async; this one has nothing to wait for"
    )]
    async fn execute(
        self,
        context: ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        match self.arg.as_deref() {
            Some("fail") => Err(PickyError("to run".to_owned())),
            Some("seven") => Ok(ExecutionResult::new(7)),
            _ => {
                writeln!(context.stdout(), "ran").map_err(|e| PickyError(e.to_string()))?;
                Ok(ExecutionResult::success())
            }
        }
    }
}

async fn new_shell() -> Result<brush_core::Shell> {
    Ok(brush_core::Shell::builder()
        .builtin("show", Registration::of::<Show<false>>())
        .builtin("also", Registration::of::<Show<false>>())
        .builtin("decl", Registration::of::<Show<true>>())
        .builtin("picky", Registration::of::<Picky>())
        .builtin("fussy", Registration::of::<Picky>())
        .build()
        .await?)
}

/// What a command did.
#[derive(Debug, PartialEq, Eq)]
struct Output {
    status: u8,
    stdout: String,
    stderr: String,
}

/// Runs `command`, capturing its standard output and error.
async fn run(shell: &mut brush_core::Shell, command: &str) -> Result<Output> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut params = shell.default_exec_params();
    params.set_fd(OpenFiles::STDOUT_FD, stdout.try_clone()?.into());
    params.set_fd(OpenFiles::STDERR_FD, stderr.try_clone()?.into());

    let result = shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await?;

    Ok(Output {
        status: u8::from(result.exit_code),
        stdout: read_back(&mut stdout)?,
        stderr: read_back(&mut stderr)?,
    })
}

fn read_back(file: &mut std::fs::File) -> Result<String> {
    let mut text = String::new();
    file.rewind()?;
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn success(stdout: &str) -> Output {
    Output {
        status: 0,
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

#[tokio::test]
async fn a_builtin_is_parsed_under_the_name_it_was_invoked_under() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(
        run(&mut shell, "show a; also b").await?,
        success("show (show) [a]\nalso (also) [b]\n")
    );
    Ok(())
}

#[tokio::test]
async fn a_builtin_s_arguments_arrive_as_expanded() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(
        run(&mut shell, r#"v='1 2'; show "$v" $v x=$v '' --"#).await?,
        success("show (show) [1 2, 1, 2, x=1, 2, , --]\n")
    );
    Ok(())
}

#[tokio::test]
async fn a_declaration_utility_receives_its_assignments_unsplit() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(
        run(&mut shell, "v='1 2'; decl -a x=$v y+=$v z").await?,
        success("decl (decl) [-a, x=1 2 (assignment), y+=1 2 (assignment), z]\n")
    );
    Ok(())
}

#[tokio::test]
async fn a_usage_error_is_written_as_it_is_and_the_builtin_does_not_run() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(
        run(&mut shell, "fussy bad").await?,
        Output {
            status: 2,
            stdout: String::new(),
            stderr: "fussy: bad: invalid argument\n".to_owned(),
        }
    );
    assert_eq!(
        run(&mut shell, "picky a b").await?.stderr,
        "picky: too many arguments\n"
    );
    Ok(())
}

#[tokio::test]
async fn a_usage_error_does_not_end_the_script() -> Result<()> {
    let mut shell = new_shell().await?;
    let output = run(&mut shell, "picky bad; echo $?; picky").await?;
    assert_eq!(output.status, 0);
    assert_eq!(output.stdout, "2\nran\n");
    Ok(())
}

#[tokio::test]
async fn a_failure_is_attributed_to_the_invoked_name_with_its_exit_code() -> Result<()> {
    let mut shell = new_shell().await?;
    let output = run(&mut shell, "fussy fail").await?;
    assert_eq!(output.status, 42);
    assert_eq!(output.stdout, "");
    assert_eq!(output.stderr, "error: fussy: refused to run\n");
    Ok(())
}

#[tokio::test]
async fn a_builtin_s_result_is_the_command_s_status() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(
        run(&mut shell, "picky seven; echo $?").await?,
        success("7\n")
    );
    Ok(())
}

#[tokio::test]
async fn a_registration_runs_its_builtin_given_the_name_first() -> Result<()> {
    let mut shell = new_shell().await?;
    let registration = shell.builtins().get("show").context("no show")?.clone();
    let mut stdout = tempfile::tempfile()?;
    let mut params = shell.default_exec_params();
    params.set_fd(OpenFiles::STDOUT_FD, stdout.try_clone()?.into());

    let context = ExecutionContext::new(&mut shell, "context-name".to_owned(), params.clone());
    let result = registration
        .execute(context, vec!["arg-name".into(), "a".into()])
        .await?;
    assert!(result.is_success());

    // With no name among the arguments, the builtin goes by the context's.
    let context = ExecutionContext::new(&mut shell, "context-name".to_owned(), params);
    let result = registration.execute(context, vec![]).await?;
    assert!(result.is_success());

    assert_eq!(
        read_back(&mut stdout)?,
        "arg-name (context-name) [a]\ncontext-name (context-name) []\n"
    );
    Ok(())
}

#[tokio::test]
async fn a_registration_s_content_comes_from_its_builtin() -> Result<()> {
    let shell = new_shell().await?;
    let registration = shell.builtins().get("show").context("no show")?;
    assert_eq!(
        registration.content(
            "alias",
            ContentType::ShortUsage,
            &ContentOptions { colorized: true }
        )?,
        "alias: ShortUsage colorized=true\n"
    );
    assert_eq!(
        registration.content(
            "show",
            ContentType::DetailedHelp,
            &ContentOptions::default()
        )?,
        "show: DetailedHelp colorized=false\n"
    );
    Ok(())
}

#[tokio::test]
async fn a_disabled_builtin_is_not_found() -> Result<()> {
    let mut shell = new_shell().await?;
    shell
        .builtins_mut()
        .get_mut("show")
        .context("show isn't registered")?
        .set_enabled(false);
    let output = run(&mut shell, "show a").await?;
    assert_eq!(output.status, 127);
    assert_eq!(output.stdout, "");
    // Its other names are separate registrations.
    assert_eq!(
        run(&mut shell, "also a").await?,
        success("also (also) [a]\n")
    );
    Ok(())
}

#[tokio::test]
async fn only_an_enabled_declaration_utility_takes_assignments_unsplit() -> Result<()> {
    let mut shell = new_shell().await?;
    // A function shadowing the builtin runs instead of it, but the words were
    // already expanded by the builtin's role.
    let script = "decl() { show \"$@\"; }; v='1 2'; decl x=$v";
    assert_eq!(
        run(&mut shell, script).await?,
        success("show (show) [x=1 2]\n")
    );

    shell
        .builtins_mut()
        .get_mut("decl")
        .context("decl isn't registered")?
        .set_enabled(false);
    assert_eq!(
        run(&mut shell, script).await?,
        success("show (show) [x=1, 2]\n")
    );
    Ok(())
}

/// A registration can be made at compile time.
const SPECIAL_SHOW: Registration<brush_core::extensions::DefaultShellExtensions> =
    Registration::of::<Show<false>>().special();

#[test]
fn a_registration_can_be_made_at_compile_time() {
    assert!(SPECIAL_SHOW.is_special());
    assert_eq!(SPECIAL_SHOW.declaration_role(), DeclarationRole::None);
}

#[test]
fn a_registration_records_what_it_was_given() {
    type SE = brush_core::extensions::DefaultShellExtensions;

    let plain = Registration::<SE>::of::<Show<false>>();
    assert!(plain.is_enabled());
    assert!(!plain.is_special());
    assert_eq!(plain.declaration_role(), DeclarationRole::None);

    // A builtin that doesn't say is not a declaration utility.
    let picky = Registration::<SE>::of::<Picky>();
    assert_eq!(picky.declaration_role(), DeclarationRole::None);

    let declaration = Registration::<SE>::of::<Show<true>>();
    assert_eq!(declaration.declaration_role(), DeclarationRole::Utility);

    let mut special = Registration::<SE>::of::<Show<true>>().special();
    assert!(special.is_special() && special.is_enabled());
    assert_eq!(special.declaration_role(), DeclarationRole::Utility);

    special.set_enabled(false);
    let copy = special.clone();
    assert!(!copy.is_enabled() && copy.is_special());
    special.set_enabled(true);
    assert!(!copy.is_enabled(), "a clone's flags are its own");
}
