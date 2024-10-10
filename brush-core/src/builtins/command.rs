use clap::Parser;
use std::{fmt::Display, io::Write, path::Path};

use crate::{builtins, commands, error, shell, sys::fs::PathExt, ExecutionResult};

/// The value for PATH when invoking `command -p`. This is only used when
/// the Posix.2 `confstr()` returns nothing
/// The value of this variable is taken from the BASH source code.
const STANDARD_UTILS_PATH: &[&str] = &["/bin", "/usr/bin", "/sbin", "/usr/sbin", "/etc:/usr/etc"];

/// Directly invokes an external command, without going through typical search order.
#[derive(Parser)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[arg(short = 'p')]
    use_default_path: bool,

    /// Display a short description of the command.
    #[arg(short = 'v')]
    print_description: bool,

    /// Display a more verbose description of the command.
    #[arg(short = 'V')]
    print_verbose_description: bool,

    /// Command and arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_and_args: Vec<String>,
}

impl CommandCommand {
    fn command(&self) -> Option<&String> {
        self.command_and_args.first()
    }
}

impl builtins::Command for CommandCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        // Silently exit if no command was provided.
        if let Some(command_name) = self.command() {
            if self.print_description || self.print_verbose_description {
                if let Some(found_cmd) = Self::try_find_command(
                    context.shell,
                    command_name.as_str(),
                    self.use_default_path,
                ) {
                    if self.print_description {
                        writeln!(context.stdout(), "{found_cmd}")?;
                    } else {
                        match found_cmd {
                            FoundCommand::Builtin(_name) => {
                                writeln!(context.stdout(), "{command_name} is a shell builtin")?;
                            }
                            FoundCommand::External(path) => {
                                writeln!(context.stdout(), "{command_name} is {path}")?;
                            }
                        }
                    }
                    Ok(builtins::ExitCode::Success)
                } else {
                    if self.print_verbose_description {
                        writeln!(context.stderr(), "command: {command_name}: not found")?;
                    }
                    Ok(builtins::ExitCode::Custom(1))
                }
            } else {
                self.execute_command(context, command_name).await
            }
        } else {
            Ok(builtins::ExitCode::Success)
        }
    }
}

enum FoundCommand {
    Builtin(String),
    External(String),
}

impl Display for FoundCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FoundCommand::Builtin(name) => write!(f, "{name}"),
            FoundCommand::External(path) => write!(f, "{path}"),
        }
    }
}

impl CommandCommand {
    #[allow(clippy::unwrap_in_result)]
    fn try_find_command(
        shell: &mut shell::Shell,
        command_name: &str,
        use_default_path: bool,
    ) -> Option<FoundCommand> {
        // Look in path.
        if command_name.contains(std::path::MAIN_SEPARATOR) {
            let candidate_path = shell.get_absolute_path(Path::new(command_name));
            if candidate_path.executable() {
                Some(FoundCommand::External(
                    candidate_path
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                ))
            } else {
                None
            }
        } else {
            if let Some(builtin_cmd) = shell.builtins.get(command_name) {
                if !builtin_cmd.disabled {
                    return Some(FoundCommand::Builtin(command_name.to_owned()));
                }
            }

            if use_default_path {
                let path = confstr_path();
                // Without an allocation if possible.
                let path = path.as_ref().map(|p| String::from_utf8_lossy(p));
                let path = path.as_ref().map_or(
                    itertools::Either::Right(STANDARD_UTILS_PATH.iter().copied()),
                    |p| itertools::Either::Left(p.split(':')),
                );

                return shell
                    .find_executables_in(path, command_name)
                    .first()
                    .map(|path| FoundCommand::External(path.to_string_lossy().to_string()));
            }

            shell
                .find_first_executable_in_path_using_cache(command_name)
                .map(|path| FoundCommand::External(path.to_string_lossy().to_string()))
        }
    }

    async fn execute_command(
        &self,
        mut context: commands::ExecutionContext<'_>,
        command_name: &str,
    ) -> Result<builtins::ExitCode, error::Error> {
        command_name.clone_into(&mut context.command_name);
        let command_and_args = self.command_and_args.iter().map(|arg| arg.into()).collect();

        // We do not have an existing process group to place this into.
        let mut pgid = None;

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        match commands::execute(
            context,
            &mut pgid,
            command_and_args,
            false, /* use functions? */
        )
        .await?
        {
            commands::CommandSpawnResult::SpawnedProcess(mut child) => {
                // TODO: jobs: review this logic
                let wait_result = child.wait().await?;
                let exec_result = ExecutionResult::from(wait_result);
                Ok(builtins::ExitCode::Custom(exec_result.exit_code))
            }
            commands::CommandSpawnResult::ImmediateExit(code) => {
                Ok(builtins::ExitCode::Custom(code))
            }
            commands::CommandSpawnResult::ExitShell(_)
            | commands::CommandSpawnResult::ReturnFromFunctionOrScript(_)
            | commands::CommandSpawnResult::BreakLoop(_)
            | commands::CommandSpawnResult::ContinueLoop(_) => {
                unreachable!("external command cannot return this spawn result")
            }
        }
    }
}

/// A wrapper for [`nix::libc::confstr`]. Returns a value for the default PATH variable which
/// indicates where all the POSIX.2 standard utilities can be found.
fn confstr_path() -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        let required_size =
            unsafe { nix::libc::confstr(nix::libc::_CS_PATH, std::ptr::null_mut(), 0) };
        if required_size == 0 {
            return None;
        }
        // NOTE: Writing `c_char` (i8 or u8 depending on the platform) into `Vec<u8>` is fine,
        // as i8 and u8 have compatible representations,
        // and Rust does not support platforms where `c_char` is not 8-bit wide.
        let mut buffer = Vec::<u8>::with_capacity(required_size);
        let final_size = unsafe {
            nix::libc::confstr(
                nix::libc::_CS_PATH,
                buffer.as_mut_ptr().cast(),
                required_size,
            )
        };
        if final_size == 0 {
            return None;
        }
        // ERANGE
        if final_size > required_size {
            return None;
        }
        unsafe { buffer.set_len(final_size - 1) }; // The last byte is a null terminator.
        return Some(buffer);
    }
    #[allow(unreachable_code)]
    None
}
