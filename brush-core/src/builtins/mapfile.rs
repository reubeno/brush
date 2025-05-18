use std::io::Read;

use clap::Parser;

use crate::{builtins, commands, env, error, openfiles, sys, variables};

/// Inspect and modify key bindings and other input configuration.
#[derive(Parser)]
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    #[arg(short = 'd', default_value = "\n")]
    delimiter: String,

    /// Maximum number of entries to read (0 means no limit).
    #[arg(short = 'n', default_value_t = 0)]
    max_count: i64,

    /// Index into array at which to start assignment.
    #[arg(short = 'O')]
    origin: Option<i64>,

    /// Number of initial entries to skip.
    #[arg(short = 's', default_value_t = 0, value_parser = validate_less_than_zero)]
    skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    #[arg(short = 't')]
    remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    #[arg(short = 'u', default_value_t = 0)]
    fd: u32,

    /// Name of function to call for each group of lines.
    #[arg(short = 'C')]
    callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    #[arg(short = 'c', default_value_t = 5000, value_parser = validate_non_zero)]
    callback_group_size: i64,

    /// Name of array to read into.
    #[arg(default_value = "MAPFILE")]
    array_var_name: String,
}

fn validate_less_than_zero(val: &str) -> Result<i64, String> {
    match val.parse::<i64>() {
        Ok(v) if v < 0 => Ok(v),
        Ok(_) => Err("invalid line count".into()),
        Err(e) => Err(format!("invalid number: {e}")),
    }
}

fn validate_non_zero(val: &str) -> Result<i64, String> {
    match val.parse::<i64>() {
        Ok(v) if v > 0 => Ok(v),
        Ok(_) => Err("invalid callback quantum".into()),
        Err(e) => Err(format!("invalid number: {e}")),
    }
}

impl builtins::Command for MapFileCommand {
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, error::Error> {
        if self.origin.is_some() {
            // This will require merging into a potentially already-existing array.
            return error::unimp("mapfile -O is not yet implemented");
        }

        let input_file = context
            .params
            .fd(self.fd)
            .ok_or_else(|| error::Error::BadFileDescriptor(self.fd))?;

        // Read!
        let results = self.read_entries(input_file, &mut context).await?;

        // Assign!
        context.shell.env.update_or_add(
            &self.array_var_name,
            variables::ShellValueLiteral::Array(results),
            |_| Ok(()),
            env::EnvironmentLookup::Anywhere,
            env::EnvironmentScope::Global,
        )?;

        Ok(builtins::ExitCode::Success)
    }
}

impl MapFileCommand {
    async fn read_entries(
        &self,
        mut input_file: openfiles::OpenFile,
        context: &mut commands::ExecutionContext<'_>,
    ) -> Result<variables::ArrayLiteral, error::Error> {
        let orig_term_attr = setup_terminal_settings(&input_file)?;

        let mut entries = vec![];
        let mut idx = self.skip_count;
        let mut read_count = 0;
        let max_count = self.max_count.try_into()?;
        let delimiter = self.delimiter.chars().next().unwrap_or('\n') as u8;

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

            if let Some(callback) = &self.callback {
                if (idx - self.origin.unwrap_or(0)) % self.callback_group_size == 0 {
                    // Ignore shell error.
                    let _ = context
                        .shell
                        .invoke_function(callback, &[idx.to_string().as_str(), &line_str])
                        .await;
                }
            }

            entries.push((None, line_str));
            idx += 1;
        }

        if let Some(orig_term_attr) = &orig_term_attr {
            input_file.set_term_attr(orig_term_attr)?;
        }

        Ok(variables::ArrayLiteral(entries))
    }
}

fn setup_terminal_settings(
    file: &openfiles::OpenFile,
) -> Result<Option<sys::terminal::TerminalSettings>, crate::Error> {
    let orig_term_attr = file.get_term_attr()?;
    if let Some(orig_term_attr) = &orig_term_attr {
        let mut updated_term_attr = orig_term_attr.to_owned();

        updated_term_attr.set_canonical(false);
        updated_term_attr.set_int_signal(false);

        file.set_term_attr(&updated_term_attr)?;
    }
    Ok(orig_term_attr)
}
