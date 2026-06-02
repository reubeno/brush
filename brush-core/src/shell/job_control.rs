//! Job management for shell instances.

use std::io::Write;

use crate::{error, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns whether this shell reports and clears completed jobs through its
    /// own interactive prompt cycle (see [`Self::check_for_completed_jobs`]).
    ///
    /// Only the top-level interactive session does: `-c` command strings, script
    /// files (even under `-i`), and subshells run no prompt cycle, so callers
    /// such as the `wait` builtin must clear completed jobs themselves there. The
    /// `!is_subshell` check is required because a subshell inherits (clones) the
    /// interactive-session call frame but does not run its own prompt loop.
    pub fn reports_completed_jobs_at_prompt(&self) -> bool {
        self.call_stack.in_interactive_session() && !self.is_subshell()
    }

    /// Checks for completed jobs in the shell, reporting any changes found.
    pub fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }
}
