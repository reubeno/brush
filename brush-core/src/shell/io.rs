//! I/O support for shell instances.

use crate::{error, extensions, ioutils, openfiles};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the standard output file.
    pub fn stdout(&self) -> openfiles::OpenFile {
        self.open_files.try_stdout().cloned().unwrap_or_else(|| {
            openfiles::OpenFile::from(openfiles::blocking::File::from(
                ioutils::FailingReaderWriter::new("standard output not available"),
            ))
        })
    }

    /// Returns the standard error file.
    pub fn stderr(&self) -> openfiles::OpenFile {
        self.open_files.try_stderr().cloned().unwrap_or_else(|| {
            openfiles::OpenFile::from(openfiles::blocking::File::from(
                ioutils::FailingReaderWriter::new("standard error not available"),
            ))
        })
    }

    /// Outputs `set -x` style trace output for a command. Intentionally does not return
    /// a result or error to avoid risk that a caller treats an error as fatal.
    pub(crate) async fn trace_command<S: AsRef<str>>(
        &mut self,
        params: &crate::interp::ExecutionParameters,
        command: S,
    ) {
        let mut prefix = self
            .as_mut()
            .expand_prompt_var("PS4", "")
            .await
            .unwrap_or_default();

        let additional_depth = self.call_stack.script_source_depth() + self.depth;
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        let trace_file = if let Some((_, xtracefd_var)) = self.env.get("BASH_XTRACEFD")
            && let Ok(fd) = xtracefd_var
                .value()
                .to_cow_str(self)
                .parse::<super::ShellFd>()
            && let Some(file) = self.open_files.try_fd(fd)
        {
            Some(file.clone())
        } else {
            params.try_stderr(self)
        };

        if let Some(mut trace_file) = trace_file {
            let output = format!("{prefix}{}\n", command.as_ref());
            #[cfg(unix)]
            {
                if let Ok(owned_fd) = trace_file.try_clone_to_owned() {
                    tokio::task::block_in_place(|| {
                        use std::io::Write;
                        let mut file = std::fs::File::from(owned_fd);
                        let _ = file.write_all(output.as_bytes());
                    });
                    return;
                }
            }
            let _ = trace_file.write_all(output.as_bytes()).await;
            let _ = trace_file.flush().await;
        }
    }

    /// Displays the given error to the user.
    pub async fn display_error(
        &self,
        err: &error::Error,
        stderr: &mut openfiles::OpenFile,
    ) -> Result<(), error::Error> {
        use crate::extensions::ErrorFormatter as _;

        let str = self.error_formatter.format_error(err, self);
        stderr.write_all(str.as_bytes()).await?;

        Ok(())
    }
}
