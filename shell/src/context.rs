use crate::Shell;

/// Represents the context for executing a command.
#[allow(clippy::module_name_repetitions)]
pub struct CommandExecutionContext<'a> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell,
    /// The name of the command being executed.    
    pub command_name: String,
    /// The open files tracked by the current context.
    pub open_files: crate::openfiles::OpenFiles,
}

impl CommandExecutionContext<'_> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&0).unwrap().try_dup().unwrap()
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }
}
