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
    array_var_name: String,
}

impl builtins::Command for MapFileCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, error::Error> {
        if self.delimiter != "\n" {
            // This will require reading a single char at a time and stoping as soon as
            // the delimiter is hit.
            return error::unimp("mapfile with non-newline delimiter not yet implemented");
        }

        if self.max_count != 0 {
            return error::unimp("mapfile -n is not yet implemented");
        }

        if self.origin.is_some() {
            // This will require merging into a potentially already-existing array.
            return error::unimp("mapfile -O is not yet implemented");
        }

        if self.skip_count != 0 {
            return error::unimp("mapfile -s is not yet implemented");
        }

        if self.callback.is_some() {
            return error::unimp("mapfile -C is not yet implemented");
        }

        let input_file = context
            .params
            .fd(self.fd)
            .ok_or_else(|| error::Error::BadFileDescriptor(self.fd))?;

        // Read!
        let results = self.read_entries(input_file)?;

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
    fn read_entries(
        &self,
        mut input_file: openfiles::OpenFile,
    ) -> Result<variables::ArrayLiteral, error::Error> {
        let mut entries = vec![];

        let orig_term_attr = setup_terminal_settings(&input_file)?;

        let mut current_entry = String::new();
        let mut buffer: [u8; 1] = [0; 1]; // 1-byte buffer

        loop {
            // TODO: Figure out how to restore terminal settings on error?
            let n = input_file.read(&mut buffer)?;
            if n == 0 {
                // EOF reached.
                break;
            }

            let ch = buffer[0] as char;

            // Check for Ctrl+C.
            if ch == '\x03' {
                break;
            // Ctrl+D is EOF *if* there's no entry in progress.
            } else if ch == '\x04' && current_entry.is_empty() {
                break;
            }

            // Check for a delimiting newline char.
            // TODO: Support other delimiters.
            if ch == '\n' {
                if !self.remove_delimiter {
                    current_entry.push(ch);
                }

                entries.push((None, std::mem::take(&mut current_entry)));
            } else {
                current_entry.push(ch);
            }
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
