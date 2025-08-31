use std::io::Read;

use clap::Parser;

use brush_core::{Error, builtins, env, error, sys, variables};

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
    #[arg(short = 'O', default_value_t = 0)]
    origin: i64,

    /// Number of initial entries to skip.
    #[arg(short = 's', default_value_t = 0, value_parser = clap::value_parser!(i64).range(0..))]
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
    #[arg(short = 'c', default_value_t = 5000, value_parser = clap::value_parser!(i64).range(1..))]
    callback_group_size: i64,

    /// Name of array to read into.
    #[arg(default_value = "MAPFILE")]
    array_var_name: String,
}

impl builtins::Command for MapFileCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        if self.origin != 0 {
            // This will require merging into a potentially already-existing array.
            return error::unimp("mapfile -O is not yet implemented");
        }

        if self.callback_group_size != 5000 || self.callback.is_some() {
            return error::unimp("mapfile -C/-c is not yet implemented");
        }

        let input_file = context
            .params
            .fd(self.fd)
            .ok_or_else(|| Error::BadFileDescriptor(self.fd))?;

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
        mut input_file: brush_core::openfiles::OpenFile,
    ) -> Result<variables::ArrayLiteral, brush_core::Error> {
        let orig_term_attr = setup_terminal_settings(&input_file)?;

        let mut entries = vec![];
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

            entries.push((None, line_str));
        }

        if let Some(orig_term_attr) = &orig_term_attr {
            brush_core::sys::terminal::set_term_attr_now(input_file, orig_term_attr)?;
        }

        Ok(variables::ArrayLiteral(entries))
    }
}

fn setup_terminal_settings(
    file: &brush_core::openfiles::OpenFile,
) -> Result<Option<sys::terminal::TerminalSettings>, brush_core::Error> {
    let orig_term_attr = brush_core::sys::terminal::get_term_attr(file).ok();
    if let Some(orig_term_attr) = &orig_term_attr {
        let mut updated_term_attr = orig_term_attr.to_owned();

        updated_term_attr.set_canonical(false);
        updated_term_attr.set_int_signal(false);

        brush_core::sys::terminal::set_term_attr_now(file, &updated_term_attr)?;
    }

    Ok(orig_term_attr)
}
