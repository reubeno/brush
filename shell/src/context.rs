use crate::Shell;

#[allow(clippy::module_name_repetitions)]
pub struct CommandExecutionContext<'a> {
    pub shell: &'a mut Shell,
    pub command_name: String,
    pub open_files: crate::openfiles::OpenFiles,
}

impl CommandExecutionContext<'_> {
    pub fn stdout(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    pub fn stderr(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }
}
