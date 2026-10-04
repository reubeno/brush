//! Process management utilities

use std::path::Path;

pub(crate) use crate::sys::tokio_process::*;

/// Creates a command to run the executable at `program` as a child of the shell, starting in
/// `working_dir` (the shell's working directory) rather than the host process's. If
/// `working_dir` is empty (the shell doesn't know where it is), the child starts in the host
/// process's.
///
/// `program` is a path the shell has already found -- absolute, or relative to `working_dir`
/// -- never a name to search for.
///
/// The child changes to `working_dir` before it runs `program`, so a relative path resolves
/// against it. It's left relative (with a `./` added if it has no `/`, which keeps it from
/// being searched for), so a script's `$0` is what bash would give it.
///
/// # Arguments
///
/// * `program` - The path of the executable to run.
/// * `working_dir` - The directory the child starts in.
pub(crate) fn create_command(
    program: impl AsRef<Path>,
    working_dir: &crate::ResolvedPath,
) -> std::process::Command {
    let program = program.as_ref();
    let mut command = if program.parent() == Some(Path::new("")) {
        std::process::Command::new(Path::new(".").join(program))
    } else {
        std::process::Command::new(program)
    };
    if !working_dir.is_empty() {
        command.current_dir(working_dir);
    }
    command
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;

    #[test]
    fn command_starts_in_the_working_directory() -> std::io::Result<()> {
        let scratch = tempfile::tempdir()?;
        let working_dir = crate::ResolvedPath::try_from(scratch.path().to_owned())?;

        let command = create_command("bin/prog", &working_dir);

        assert_eq!(command.get_current_dir(), Some(scratch.path()));
        assert_eq!(command.get_program(), "bin/prog");
        Ok(())
    }

    /// A bare file name is a file in the working directory, not a command to search for.
    #[test]
    fn bare_file_name_is_run_from_the_working_directory() -> std::io::Result<()> {
        let scratch = tempfile::tempdir()?;
        let working_dir = crate::ResolvedPath::try_from(scratch.path().to_owned())?;

        let command = create_command("prog", &working_dir);

        assert_eq!(command.get_program(), "./prog");
        Ok(())
    }

    #[test]
    fn command_without_a_working_directory_starts_where_the_process_is() {
        let command = create_command("/bin/prog", &crate::ResolvedPath::default());

        assert_eq!(command.get_current_dir(), None);
    }
}
