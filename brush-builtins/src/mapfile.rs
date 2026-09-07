//! The `mapfile` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(MapFileCommand);

use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, env, error, variables};
use std::io::{Read, Write};

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

pub(super) fn setup_terminal_settings(
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &MapFileCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.callback_group_size != 5000 || command.callback.is_some() {
        return error::unimp("mapfile -C/-c is not yet implemented");
    }

    if let Some(origin) = command.origin {
        if origin < 0 {
            writeln!(
                context.stderr(),
                "{}: {origin}: invalid array origin",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }
    }

    if let Some((_, var)) = context.shell.env().get(&command.array_var_name) {
        if var.value().is_associative_array() {
            writeln!(
                context.stderr(),
                "{}: {}: not an indexed array",
                context.command_name,
                command.array_var_name
            )?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }
    }

    let input_file = context
        .try_fd(command.fd)
        .ok_or_else(|| ErrorKind::BadFileDescriptor(command.fd))?;

    // Read!
    let results = command.read_entries(input_file)?;

    if let Some(origin) = command.origin {
        // -O: preserve existing array, assign at offset.
        for (elem_idx, (_key, value)) in results.0.into_iter().enumerate() {
            // If the user is getting to wraparounds in *bash*, they got bigger problems.
            #[allow(clippy::cast_possible_wrap)]
            let elem_idx = elem_idx as i64;
            context.shell.env_mut().update_or_add_array_element(
                &command.array_var_name,
                (elem_idx + origin).to_string(),
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }
    } else {
        // No -O: replace the entire variable (clears existing).
        context.shell.env_mut().update_or_add(
            &command.array_var_name,
            variables::ShellValueLiteral::Array(results),
            |_| Ok(()),
            env::EnvironmentLookup::Anywhere,
            env::EnvironmentScope::Global,
        )?;
    }

    Ok(ExecutionResult::success())
}
