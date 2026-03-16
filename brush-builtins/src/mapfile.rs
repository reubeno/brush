use std::io::{Read, Write};

use clap::Parser;

use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, error, variables};

/// Read lines from standard input into an indexed array variable.
#[derive(Parser)]
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    #[arg(short = 'd')]
    delimiter: Option<String>,

    /// Maximum number of entries to read (0 means no limit).
    #[arg(short = 'n', default_value_t = 0)]
    max_count: i64,

    /// Index into array at which to start assignment.
    #[arg(short = 'O', allow_hyphen_values = true)]
    origin: Option<i64>,

    /// Number of initial entries to skip.
    #[arg(short = 's', default_value_t = 0, value_parser = clap::value_parser!(i64).range(0..))]
    skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    #[arg(short = 't')]
    remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    #[arg(short = 'u', default_value_t = 0)]
    fd: brush_core::ShellFd,

    /// Name of function to call for each group of lines.
    #[arg(short = 'C')]
    callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    #[arg(short = 'c', default_value_t = 5000, value_parser = clap::value_parser!(i64).range(1..))]
    callback_group_size: i64,

    /// Name of array to read into.
    #[arg(default_value = "MAPFILE")]
    array_var_name: String,
}

impl builtins::Command for MapFileCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.callback_group_size != 5000 || self.callback.is_some() {
            return error::unimp("mapfile -C/-c is not yet implemented");
        }

        if let Some(origin) = self.origin {
            if origin < 0 {
                writeln!(
                    context.stderr(),
                    "{}: {origin}: invalid array origin",
                    context.command_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        if let Some(resolved) = context.shell.env().lookup(&self.array_var_name).get() {
            if matches!(
                resolved.base_var().value(),
                variables::ShellValue::AssociativeArray(_)
                    | variables::ShellValue::Unset(
                        variables::ShellValueUnsetType::AssociativeArray
                    )
            ) {
                writeln!(
                    context.stderr(),
                    "{}: {}: not an indexed array",
                    context.command_name,
                    self.array_var_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        let input_file = context
            .try_fd(self.fd)
            .ok_or_else(|| ErrorKind::BadFileDescriptor(self.fd))?;

        // Read!
        let results = self.read_entries(input_file)?;

        let write_result: Result<(), brush_core::Error> = if let Some(origin) = self.origin {
            // -O: preserve existing array, assign at offset.
            (|| {
                for (elem_idx, (_key, value)) in results.0.into_iter().enumerate() {
                    // If the user is getting to wraparounds in *bash*, they got bigger problems.
                    #[allow(clippy::cast_possible_wrap)]
                    let elem_idx = elem_idx as i64;
                    context.shell.env_mut().set_var_element(
                        &self.array_var_name,
                        (elem_idx + origin).to_string(),
                        value,
                    )?;
                }
                Ok(())
            })()
        } else {
            // No -O: replace the entire variable (clears existing).
            context.shell.env_mut().set_var(
                &self.array_var_name,
                variables::ShellValueLiteral::Array(results),
            )
        };

        match write_result {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(err) if matches!(err.kind(), ErrorKind::CircularNameReference(_)) => {
                // Bash emits a warning but exits 0 from mapfile on cycle.
                context.shell.warn_circular_nameref(&err)?;
                Ok(ExecutionResult::success())
            }
            Err(err) if matches!(err.kind(), ErrorKind::SubscriptedNameRefTarget { .. }) => {
                // Bash exits 1 with a "<cmd>: target: not a valid identifier"
                // diagnostic.
                writeln!(context.stderr(), "{}: {err}", context.command_name)?;
                Ok(ExecutionResult::general_error())
            }
            Err(err) => Err(err),
        }
    }
}

impl MapFileCommand {
    fn read_entries(
        &self,
        mut input_file: brush_core::openfiles::OpenFile,
    ) -> Result<variables::ArrayLiteral, brush_core::Error> {
        let _term_mode = setup_terminal_settings(&input_file)?;

        let mut entries = vec![];
        let mut read_count = 0;
        let max_count = self.max_count.try_into()?;
        let delimiter = match &self.delimiter {
            Some(d) if d.is_empty() => b'\0',
            Some(d) => d.as_bytes().first().copied().unwrap_or(b'\n'),
            None => b'\n',
        };

        let mut buf = [0u8; 1];

        while max_count == 0 || entries.len() < max_count {
            let mut line = vec![];
            let mut saw_delimiter = false;

            loop {
                match input_file.read(&mut buf) {
                    Ok(0) => break,                                         // End of input
                    Ok(1) if buf[0] == b'\x03' => break,                    // Ctrl+C
                    Ok(1) if buf[0] == b'\x04' && line.is_empty() => break, // Ctrl+D
                    Ok(1) => {
                        let byte = buf[0];
                        line.push(byte);
                        if byte == delimiter {
                            saw_delimiter = true;
                            break;
                        }
                    }
                    Ok(_) => unreachable!("input can only be 0, 1, or error"),
                    Err(e) => return Err(e.into()),
                }
            }

            if line.is_empty() && !saw_delimiter {
                break;
            }

            if read_count < self.skip_count {
                read_count += 1;
                continue;
            }

            if self.remove_delimiter && line.ends_with(&[delimiter]) {
                line.pop();
            }

            let line_str = String::from_utf8_lossy(&line).to_string();

            entries.push((None, line_str));
        }

        Ok(variables::ArrayLiteral(entries))
    }
}

fn setup_terminal_settings(
    file: &brush_core::openfiles::OpenFile,
) -> Result<Option<brush_core::terminal::AutoModeGuard>, brush_core::Error> {
    let mode = brush_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
    if let Some(mode) = &mode {
        let config = brush_core::terminal::Settings::builder()
            .line_input(false)
            .interrupt_signals(false)
            .build();

        mode.apply_settings(&config)?;
    }

    Ok(mode)
}
