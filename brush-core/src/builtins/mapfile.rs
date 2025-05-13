use std::io::{BufRead, BufReader};

use clap::Parser;

use crate::{builtins, commands, env, error, openfiles, sys, variables};

/// Inspect and modify key bindings and other input configuration.
#[derive(Parser)]
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    #[arg(short = 'd', default_value = "\n")]
    delimiter: String,

    /// Maximum number of entries to read (0 means no limit).
    #[arg(short = 'n', default_value = "0")]
    max_count: i64,

    /// Index into array at which to start assignment.
    #[arg(short = 'O')]
    origin: Option<i64>,

    /// Number of initial entries to skip.
    #[arg(short = 's', default_value = "0")]
    skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    #[arg(short = 't')]
    remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    #[arg(short = 'u', default_value = "0")]
    fd: u32,

    /// Name of function to call for each group of lines.
    #[arg(short = 'C')]
    callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    #[arg(short = 'c', default_value = "5000")]
    callback_group_size: i64,

    /// Name of array to read into.
    #[arg(default_value = "MAPFILE")]
    array_var_name: String,
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

        if self.skip_count != 0 {
            return error::unimp("mapfile -s is not yet implemented");
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
        input_file: openfiles::OpenFile,
        context: &mut commands::ExecutionContext<'_>,
    ) -> Result<variables::ArrayLiteral, error::Error> {
        let orig_term_attr = setup_terminal_settings(&input_file)?;

        let mut entries = vec![];
        let mut buf_reader = BufReader::new(input_file.clone());
        let mut line = vec![];
        let mut idx = self.skip_count;
        let mut read_count = 0;
        let delimiter = self.delimiter.chars().next().unwrap_or('\n');

        while self.max_count == 0 || entries.len() < usize::try_from(self.max_count)? {
            line.clear();
            let bytes = buf_reader.read_until(delimiter as u8, &mut line)?;
            if bytes == 0 {
                break;
            }

            if read_count < self.skip_count {
                read_count += 1;
                continue;
            }

            if self.remove_delimiter && line.ends_with(&[delimiter as u8]) {
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
